//! Reversible account archives and revision-bound native credential cleanup.
//! No credentials or provider IO enter this module.
//!
//! Removing an account never deletes local mail or calendar data, and never
//! touches the Google account. The row switches to `removed` (a read-only
//! archive), its Google connection is disconnected, and the engine then
//! deletes the stored credential under a cleanup lease. A removed account
//! whose cleanup completed can be restored, but only by signing in again as
//! the same Google subject.
//!
//! Every step bumps `lifecycle_revision`, and each lease records the revision
//! it was issued at, so a stale or duplicated engine task cannot finish work
//! that a later remove or restore has superseded. The cleanup state
//! (`pending`, `in_flight`, `complete`, `failed`, `restore_in_flight`) lives
//! in `accounts.cleanup_json`.
use crate::{err, fields, id, string, Result, Store};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{collections::BTreeSet, path::PathBuf};

#[cfg(test)]
mod tests;

/// Result of [`Store::account_remove_begin`].
pub enum AccountRemovalStep {
    /// Nothing more to do; return this removal status to the caller.
    Reply(Value),
    /// The account is archived and its stored credential must now be deleted
    /// under this lease.
    Cleanup(AccountCleanupLease),
}

/// Authority to delete one removed account's stored credential. Valid only
/// for the profile, lifecycle revision, request and attempt it was issued at.
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
    /// Returns the ID of the removal request that issued this lease.
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
}
/// What happened when the engine tried to delete the stored credential.
pub enum AccountCleanupOutcome {
    /// The credential is gone from the OS keychain.
    Removed,
    /// Deletion failed. Codes outside a small known set are recorded as
    /// `credential_cleanup_failed`, so native error text is never persisted.
    Failed { code: String },
}

/// Authority to bind a fresh Google grant to a removed account, issued by
/// [`Store::google_restore_admit`] for one verified identity and scope set.
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

/// Returns the lifecycle revision of an active account, or fails with
/// `account_removed` for an archive and `not_found` for an unknown ID.
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
    /// Returns the account's lifecycle revision if it is active.
    ///
    /// Every write path calls this first: a removed account is a read-only
    /// archive and fails with `account_removed`; an unknown ID fails with
    /// `not_found`. Callers that stash the revision can later use it to detect
    /// a remove-and-restore in between.
    pub fn require_active_account(&self, account: &str) -> Result<i64> {
        active(&self.db, account)
    }
    /// Fails with `stale_account_revision` unless the account is still active
    /// at exactly `revision`.
    pub(crate) fn check_active_account_revision(&self, account: &str, revision: i64) -> Result<()> {
        if self.require_active_account(account)? != revision {
            return Err(stale());
        }
        Ok(())
    }
    /// Returns `{state, revision, cleanup}` for an account, where `state` is
    /// `active` or `removed` and `cleanup` is the credential-cleanup record
    /// (null while active).
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
    /// Describes what removal would do so the user can confirm it: the data
    /// is kept as a read-only archive (with message and open-draft counts) and
    /// nothing changes at Google.
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
    /// Returns the recorded removal operation for `request_id` with the
    /// account's current lifecycle. `operation_is_current` is false once a
    /// later removal request has replaced this one.
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
    /// Archives an account and returns the cleanup lease for its credential.
    ///
    /// `p` is `{request_id, expected_revision, retention: "archive"}`. In one
    /// transaction the account becomes `removed`, its Google connection is
    /// disconnected (bumping the connection generation, which fences in-flight
    /// sync work), and the operation is journaled. Replaying the same request
    /// returns its status, or a new lease while that request's first cleanup
    /// is still pending (for example after a crash). Fails
    /// with `stale_account_revision` on a revision mismatch and
    /// `account_cleanup_busy` while credential work is running.
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
            // Only an interrupted first cleanup of this same request is resumed;
            // a replay after completion or restore must not touch credentials.
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
        // Make Calendar clients reload: the status `calendar_generation` changes
        // even though no calendar publication row did.
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
    /// Marks a pending cleanup `in_flight` and returns the claimed lease the
    /// engine uses to report the outcome.
    ///
    /// The caller must hold the native coordinator account lock before claiming.
    /// Fails with `stale_account_revision` if the lease was superseded, which
    /// is how a duplicate queued before a restore is turned away.
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
    /// Records the credential deletion outcome for a claimed lease and returns
    /// the removal status. Cleanup is then `complete` or `failed`, and a
    /// replay of the same removal request no longer re-runs it.
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
    /// Checks that a removed account can be restored and returns the Google
    /// subject (`sub`) the new sign-in must match.
    ///
    /// Fails with `stale_account_revision`, `account_not_removed`,
    /// `account_cleanup_required` until credential cleanup is complete, and
    /// `capability_unavailable` for accounts without a verified Google identity.
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
    /// Admits a verified sign-in for restoring `account`, marks the restore
    /// `restore_in_flight` and returns its lease.
    ///
    /// The grant must be for the archived Google subject (`identity_mismatch`
    /// otherwise) and pass the usual binding and OAuth client checks. The
    /// engine then stores the credential and calls
    /// [`Store::google_restore_bind`], or [`Store::google_restore_failed`].
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
    /// Completes a restore after the new credential is stored: rewrites the
    /// Google binding with the admitted identity, scopes and OAuth client
    /// config path, and makes the account active again. Returns the account ID.
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
    /// Records a failed restore and returns the lifecycle. The account stays
    /// removed with cleanup back at `pending`, because the failed attempt may
    /// already have written a credential that must be cleaned up again before
    /// another restore.
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
    /// Startup recovery: credential work that was running when the process
    /// stopped becomes `pending` with code `interrupted`, under a new revision
    /// so leases from the dead process are rejected.
    pub(crate) fn recover_account_cleanup(&self) -> Result<()> {
        let tx = self.db.unchecked_transaction()?;
        tx.execute("UPDATE accounts SET lifecycle_revision=lifecycle_revision+1,cleanup_json=json_set(cleanup_json,'$.state','pending','$.code','interrupted','$.attempt',COALESCE(json_extract(cleanup_json,'$.attempt'),0)+1) WHERE lifecycle_state='removed' AND json_extract(cleanup_json,'$.state') IN ('in_flight','restore_in_flight')",[])?;
        tx.commit()?;
        Ok(())
    }
}
