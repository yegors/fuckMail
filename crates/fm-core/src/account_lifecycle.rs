//! Reversible account archives and revision-bound native credential cleanup.
//! No credentials or provider IO enter this module.
use crate::{err, fields, id, string, Result, Store};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{collections::BTreeSet, path::PathBuf};

#[cfg(test)]
mod tests;

pub enum AccountRemovalStep {
    Reply(Value),
    Cleanup(AccountCleanupLease),
}

#[derive(Clone)]
pub struct AccountCleanupLease {
    profile: PathBuf,
    account: String,
    request_id: String,
    revision: i64,
    attempt: i64,
    claimed: bool,
}
impl AccountCleanupLease {
    pub fn account(&self) -> &str {
        &self.account
    }
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
}
pub enum AccountCleanupOutcome {
    Removed,
    Failed { code: String },
}

#[derive(Clone)]
pub struct AccountRestoreLease {
    profile: PathBuf,
    account: String,
    revision: i64,
    subject: String,
    email: String,
    scopes: BTreeSet<String>,
    client_id: String,
}
impl AccountRestoreLease {
    pub fn account(&self) -> &str {
        &self.account
    }
}

pub(crate) fn active(db: &Connection, account: &str) -> Result<i64> {
    let (state, revision) = state(db, account)?;
    if state != "active" {
        return Err(err(
            "account_removed",
            "This retained account is a read-only archive",
        ));
    }
    Ok(revision)
}
fn state(db: &Connection, account: &str) -> Result<(String, i64)> {
    id(account)?;
    db.query_row(
        "SELECT lifecycle_state,lifecycle_revision FROM accounts WHERE id=?1",
        [account],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()?
    .ok_or_else(|| err("not_found", "Account does not exist in this profile"))
}

// Keep persisted diagnostics bounded and independent of native exception text.
fn failure_code(code: &str) -> &str {
    match code {
        "cancelled"
        | "vault_locked"
        | "vault_unavailable"
        | "credential_store_failed"
        | "credential_cleanup_failed"
        | "restore_failed"
        | "interrupted" => code,
        _ => "credential_cleanup_failed",
    }
}
fn expected_revision(params: &Value) -> Result<i64> {
    params["expected_revision"]
        .as_i64()
        .filter(|v| *v > 0)
        .ok_or_else(|| {
            err(
                "invalid_request",
                "expected_revision must be a positive integer",
            )
        })
}
fn stale() -> crate::CoreError {
    err(
        "stale_account_revision",
        "Account lifecycle changed; reload its current state",
    )
}
fn busy() -> crate::CoreError {
    err(
        "account_cleanup_busy",
        "Account credential work is already in progress",
    )
}

impl Store {
    pub fn require_active_account(&self, account: &str) -> Result<i64> {
        active(&self.db, account)
    }
    pub(crate) fn check_active_account_revision(&self, account: &str, revision: i64) -> Result<()> {
        if self.require_active_account(account)? != revision {
            return Err(stale());
        }
        Ok(())
    }
    pub fn account_lifecycle(&self, account: &str) -> Result<Value> {
        let (state, revision) = state(&self.db, account)?;
        let cleanup: Option<String> = self.db.query_row(
            "SELECT cleanup_json FROM accounts WHERE id=?1",
            [account],
            |r| r.get(0),
        )?;
        Ok(
            json!({"state":state,"revision":revision,"cleanup":cleanup.map(|v|serde_json::from_str::<Value>(&v)).transpose()?}),
        )
    }
    pub fn account_remove_preview(&self, account: &str) -> Result<Value> {
        let lifecycle = self.account_lifecycle(account)?;
        let messages: i64 = self.db.query_row(
            "SELECT COUNT(*) FROM messages WHERE account=?1",
            [account],
            |r| r.get(0),
        )?;
        let drafts: i64 = self.db.query_row(
            "SELECT COUNT(*) FROM drafts WHERE account=?1 AND discarded=0",
            [account],
            |r| r.get(0),
        )?;
        Ok(
            json!({"account":account,"lifecycle":lifecycle,"retention":"archive","read_only":true,"retained":{"messages":messages,"drafts":drafts},"remote_data_changed":false}),
        )
    }
    pub fn account_remove_status(&self, account: &str, request_id: &str) -> Result<Value> {
        id(request_id)?;
        let raw: String = self
            .db
            .query_row(
                "SELECT result_json FROM operations WHERE account=?1 AND request_id=?2",
                params![account, request_id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| err("not_found", "Removal request is not recorded"))?;
        let operation: Value = serde_json::from_str(&raw)?;
        if operation["kind"] != "account_remove" {
            return Err(err(
                "request_id_conflict",
                "Request ID belongs to another operation",
            ));
        }
        let lifecycle = self.account_lifecycle(account)?;
        Ok(
            json!({"account":account,"request_id":request_id,"operation":operation,"operation_is_current":lifecycle["cleanup"]["request_id"]==request_id,"lifecycle":lifecycle}),
        )
    }
    pub fn account_remove_begin(&mut self, account: &str, p: &Value) -> Result<AccountRemovalStep> {
        fields(p, &["expected_revision", "request_id", "retention"])?;
        let request_id = string(p, "request_id")?;
        if request_id.len() > 128 || p["retention"] != "archive" {
            return Err(err(
                "invalid_request",
                "Removal requires archive retention and a bounded request ID",
            ));
        }
        let expected = expected_revision(p)?;
        let fingerprint = crate::mutations::fingerprint("account_remove", p)?;
        let previous: Option<String> = self
            .db
            .query_row(
                "SELECT fingerprint FROM operations WHERE account=?1 AND request_id=?2",
                params![account, request_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(previous) = previous {
            if previous != fingerprint {
                return Err(err(
                    "request_id_conflict",
                    "Request ID belongs to a different command",
                ));
            }
            let status = self.account_remove_status(account, request_id)?;
            if status["operation_is_current"] == true
                && status["lifecycle"]["state"] == "removed"
                && status["lifecycle"]["cleanup"]["state"] == "pending"
                && status["lifecycle"]["cleanup"]["replay_cleanup"] == true
            {
                return Ok(AccountRemovalStep::Cleanup(
                    self.cleanup_lease(account, request_id, false)?,
                ));
            }
            return Ok(AccountRemovalStep::Reply(status));
        }
        let life = self.account_lifecycle(account)?;
        if life["revision"] != expected {
            return Err(stale());
        }
        if matches!(
            life["cleanup"]["state"].as_str(),
            Some("in_flight" | "restore_in_flight")
        ) {
            return Err(busy());
        }
        let revision = expected.checked_add(1).ok_or_else(stale)?;
        let cleanup = json!({"request_id":request_id,"state":"pending","code":null,"attempt":0,"replay_cleanup":true});
        let tx = self.db.unchecked_transaction()?;
        tx.execute("UPDATE accounts SET lifecycle_state='removed',lifecycle_revision=?2,cleanup_json=?3 WHERE id=?1",params![account,revision,cleanup.to_string()])?;
        tx.execute("UPDATE google_accounts SET connection_state='disconnected',connection_generation=connection_generation+1,revision=revision+1 WHERE account=?1",[account])?;
        let generation: i64 = tx
            .query_row(
                "SELECT connection_generation FROM google_accounts WHERE account=?1",
                [account],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        let operation = json!({"kind":"account_remove","account":account,"request_id":request_id,"retention":"archive","removed_revision":revision,"connection_generation":generation});
        tx.execute("INSERT INTO operations(account,request_id,fingerprint,result_json) VALUES(?1,?2,?3,?4)",params![account,request_id,fingerprint,operation.to_string()])?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        self.mail_changes.reset_calendar_generation();
        Ok(AccountRemovalStep::Cleanup(
            self.cleanup_lease(account, request_id, false)?,
        ))
    }
    fn cleanup_lease(
        &self,
        account: &str,
        request_id: &str,
        claimed: bool,
    ) -> Result<AccountCleanupLease> {
        let life = self.account_lifecycle(account)?;
        Ok(AccountCleanupLease {
            profile: self.profile.clone(),
            account: account.into(),
            request_id: request_id.into(),
            revision: life["revision"].as_i64().ok_or_else(stale)?,
            attempt: life["cleanup"]["attempt"].as_i64().unwrap_or(0),
            claimed,
        })
    }
    fn check_cleanup(&self, lease: &AccountCleanupLease, state: &str) -> Result<Value> {
        if lease.profile != self.profile {
            return Err(err("scope_mismatch", "Cleanup belongs to another profile"));
        }
        let life = self.account_lifecycle(&lease.account)?;
        if life["state"] != "removed"
            || life["revision"] != lease.revision
            || life["cleanup"]["request_id"] != lease.request_id
            || life["cleanup"]["attempt"] != lease.attempt
            || life["cleanup"]["state"] != state
        {
            return Err(stale());
        }
        Ok(life["cleanup"].clone())
    }
    /// The caller must hold the native coordinator account lock before claiming.
    pub fn account_cleanup_claim(
        &mut self,
        lease: &AccountCleanupLease,
    ) -> Result<AccountCleanupLease> {
        if lease.claimed {
            return Err(busy());
        }
        let mut cleanup = self.check_cleanup(lease, "pending")?;
        cleanup["state"] = json!("in_flight");
        cleanup["attempt"] = json!(lease.attempt.checked_add(1).ok_or_else(stale)?);
        self.write_cleanup(&lease.account, &cleanup, true)?;
        self.cleanup_lease(&lease.account, &lease.request_id, true)
    }
    pub fn account_cleanup_finish(
        &mut self,
        lease: &AccountCleanupLease,
        outcome: AccountCleanupOutcome,
    ) -> Result<Value> {
        if !lease.claimed {
            return Err(stale());
        }
        let mut cleanup = self.check_cleanup(lease, "in_flight")?;
        match outcome {
            AccountCleanupOutcome::Removed => {
                cleanup["state"] = json!("complete");
                cleanup["code"] = Value::Null;
            }
            AccountCleanupOutcome::Failed { code } => {
                cleanup["state"] = json!("failed");
                cleanup["code"] = json!(failure_code(&code));
            }
        }
        cleanup["replay_cleanup"] = json!(false);
        self.write_cleanup(&lease.account, &cleanup, true)?;
        self.account_remove_status(&lease.account, &lease.request_id)
    }
    fn write_cleanup(&self, account: &str, cleanup: &Value, advance: bool) -> Result<()> {
        let revision = self.account_lifecycle(account)?["revision"]
            .as_i64()
            .ok_or_else(stale)?;
        let next = revision.checked_add(i64::from(advance)).ok_or_else(stale)?;
        let tx = self.db.unchecked_transaction()?;
        tx.execute(
            "UPDATE accounts SET cleanup_json=?2,lifecycle_revision=?3 WHERE id=?1",
            params![account, cleanup.to_string(), next],
        )?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(())
    }
    pub fn google_restore_check(&self, account: &str, expected_revision: i64) -> Result<String> {
        if expected_revision <= 0 {
            return Err(err(
                "invalid_request",
                "A positive account revision is required",
            ));
        }
        let life = self.account_lifecycle(account)?;
        if life["revision"] != expected_revision {
            return Err(stale());
        }
        if life["state"] != "removed" {
            return Err(err("account_not_removed", "This account is already active"));
        }
        if life["cleanup"]["state"] != "complete" {
            return Err(err(
                "account_cleanup_required",
                "Finish local credential cleanup before restoring",
            ));
        }
        self.db
            .query_row(
                "SELECT google_subject FROM google_accounts WHERE account=?1",
                [account],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| {
                err(
                    "capability_unavailable",
                    "Only a verified Google identity can be restored",
                )
            })
    }
    pub fn google_restore_admit(
        &mut self,
        account: &str,
        expected_revision: i64,
        subject: &str,
        email: &str,
        scopes: &BTreeSet<String>,
        client_id: &str,
    ) -> Result<AccountRestoreLease> {
        if self.google_restore_check(account, expected_revision)? != subject {
            return Err(err(
                "identity_mismatch",
                "Restore selected a different Google identity",
            ));
        }
        self.validate_google_binding(subject, email, scopes, Some(account))?;
        self.check_google_client_id(client_id)?;
        let mut cleanup = self.account_lifecycle(account)?["cleanup"].clone();
        cleanup["state"] = json!("restore_in_flight");
        cleanup["code"] = Value::Null;
        self.write_cleanup(account, &cleanup, true)?;
        Ok(AccountRestoreLease {
            profile: self.profile.clone(),
            account: account.into(),
            revision: expected_revision + 1,
            subject: subject.into(),
            email: email.into(),
            scopes: scopes.clone(),
            client_id: client_id.into(),
        })
    }
    fn check_restore(&self, lease: &AccountRestoreLease) -> Result<Value> {
        if lease.profile != self.profile {
            return Err(err("scope_mismatch", "Restore belongs to another profile"));
        }
        let life = self.account_lifecycle(&lease.account)?;
        if life["state"] != "removed"
            || life["revision"] != lease.revision
            || life["cleanup"]["state"] != "restore_in_flight"
        {
            return Err(stale());
        }
        Ok(life["cleanup"].clone())
    }
    pub fn google_restore_bind(
        &mut self,
        lease: &AccountRestoreLease,
        config_path: &str,
    ) -> Result<String> {
        self.check_restore(lease)?;
        crate::live_profile::validate_client_path(config_path)?;
        self.check_google_client_id(&lease.client_id)?;
        let tx = self.db.unchecked_transaction()?;
        crate::live_profile::write_binding(
            &tx,
            &lease.account,
            &lease.subject,
            &lease.email,
            &lease.scopes,
            Some((&lease.client_id, config_path)),
        )?;
        tx.execute("UPDATE accounts SET lifecycle_state='active',lifecycle_revision=lifecycle_revision+1,cleanup_json=NULL WHERE id=?1",[&lease.account])?;
        tx.commit()?;
        self.mail_changes.reset_calendar_generation();
        Ok(lease.account.clone())
    }
    pub fn google_restore_failed(
        &mut self,
        lease: &AccountRestoreLease,
        code: &str,
    ) -> Result<Value> {
        let mut cleanup = self.check_restore(lease)?;
        cleanup["state"] = json!("pending");
        cleanup["code"] = json!(failure_code(code));
        self.write_cleanup(&lease.account, &cleanup, true)?;
        self.account_lifecycle(&lease.account)
    }
    pub(crate) fn recover_account_cleanup(&self) -> Result<()> {
        let tx = self.db.unchecked_transaction()?;
        tx.execute("UPDATE accounts SET lifecycle_revision=lifecycle_revision+1,cleanup_json=json_set(cleanup_json,'$.state','pending','$.code','interrupted','$.attempt',COALESCE(json_extract(cleanup_json,'$.attempt'),0)+1) WHERE lifecycle_state='removed' AND json_extract(cleanup_json,'$.state') IN ('in_flight','restore_in_flight')",[])?;
        tx.commit()?;
        Ok(())
    }
}
