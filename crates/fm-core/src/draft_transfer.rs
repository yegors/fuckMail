//! An account change creates destination ownership before retiring the source.
//! Provider cleanup is journaled separately and waits for destination readback.
use crate::{err, fields, id, local_content::RetainedDraft, mutations, string, Result, Store};
use fm_google::transport::CancellationToken;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
#[cfg(test)]
#[path = "draft_transfer_tests.rs"]
mod tests;

/// Result of [`Store::draft_transfer_begin`].
pub enum TransferStep {
    /// The request ID already completed; carries its stored reply.
    Reply(Value),
    /// Read the draft's attachments off the writer with [`prepare`].
    Prepare(Box<TransferPlan>),
}
/// Frozen source draft, its Google state and the destination account's
/// identity for moving a draft to another account.
///
/// A transfer row moves `waiting_target` -> `cleanup_pending` -> `complete`,
/// or to `blocked` when cleanup cannot safely continue. It starts `complete`
/// when the original is kept or never reached Gmail.
pub struct TransferPlan {
    account: String,
    params: Value,
    retained: RetainedDraft,
    raw: Value,
    source_google: Value,
    source_lifecycle: i64,
    target: String,
    target_stamp: Value,
}
/// A transfer plan with its attachments verified, ready for
/// [`Store::draft_transfer_finish`].
pub struct PreparedTransfer {
    plan: Box<TransferPlan>,
    files: Vec<fm_mail::StagedAttachment>,
    cancel: CancellationToken,
}
/// Next Gmail cleanup step for a moved draft, from
/// [`Store::draft_transfer_cleanup_due`].
pub struct TransferCleanup {
    /// Transfer the cleanup belongs to.
    pub transfer_request_id: String,
    /// Parameters of the Gmail draft deletion that removes the original.
    pub params: Value,
    /// True when an earlier deletion attempt has an unknown outcome: only a
    /// read-only check may follow, never a second delete.
    pub reconcile: bool,
}

/// Verifies and loads the draft's retained attachments off the writer.
pub fn prepare(plan: Box<TransferPlan>, cancel: CancellationToken) -> Result<PreparedTransfer> {
    let files = plan.retained.prepare(&cancel)?;
    // Destination ownership must survive removal of the source account. A
    // received part may have been decoded from MIME rather than a file blob.
    for file in &files {
        cancelled(&cancel)?;
        plan.retained.blobs.stage_blob(&file.bytes)?;
    }
    Ok(PreparedTransfer {
        plan,
        files,
        cancel,
    })
}
fn cancelled(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        Err(err("cancelled", "Draft account change cancelled"))
    } else {
        Ok(())
    }
}
fn positive(p: &Value, field: &str) -> Result<i64> {
    p[field]
        .as_i64()
        .filter(|v| *v > 0)
        .ok_or_else(|| err("invalid_request", "A saved draft revision is required"))
}
fn params_valid(p: &Value) -> Result<()> {
    fields(
        p,
        &[
            "request_id",
            "draft_id",
            "expected_revision",
            "expected_google_revision",
            "target_account",
            "keep_original",
        ],
    )?;
    for field in ["request_id", "draft_id", "target_account"] {
        id(string(p, field)?)?;
    }
    positive(p, "expected_revision")?;
    if p["expected_google_revision"].as_i64().is_none_or(|n| n < 0)
        || p.get("keep_original").is_some_and(|v| !v.is_boolean())
    {
        return Err(err(
            "invalid_request",
            "Supply a Google revision and boolean copy policy",
        ));
    }
    Ok(())
}

impl Store {
    fn transfer_receipt(&self, account: &str, p: &Value) -> Result<Option<Value>> {
        params_valid(p)?;
        let row: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT fingerprint,result_json FROM operations WHERE account=?1 AND request_id=?2",
                params![account, string(p, "request_id")?],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((fingerprint, result)) = row {
            if fingerprint != mutations::fingerprint("draft_transfer", p)? {
                return Err(err(
                    "request_id_conflict",
                    "This account change request already identifies another action",
                ));
            }
            return Ok(Some(serde_json::from_str(&result)?));
        }
        Ok(None)
    }
    fn transfer_target_stamp(&self, target: &str) -> Result<Value> {
        let lifecycle = self.require_active_account(target)?;
        let (name, _, revision) = self.draft_identity(target)?;
        let email: String =
            self.db
                .query_row("SELECT email FROM accounts WHERE id=?1", [target], |r| {
                    r.get(0)
                })?;
        Ok(
            json!({"account":target,"lifecycle":lifecycle,"name":name,"email":email,"revision":revision}),
        )
    }
    /// Snapshot of the source draft's Google, send and deletion state. A move
    /// needs a settled twin at `expected` and no pending or accepted send or
    /// deletion, since those still own the Gmail draft.
    fn transfer_google_stamp(&self, account: &str, draft: &str, expected: i64) -> Result<Value> {
        self.check_google_send_draft_write(account, draft)?;
        let google = self.google_draft_status(account, draft)?;
        if google["revision"] != expected {
            return Err(err(
                "stale_google_revision",
                "Google draft state changed; refresh before choosing its account",
            ));
        }
        if !matches!(
            google["state"].as_str(),
            Some("local" | "synced" | "adopted" | "failed")
        ) {
            return Err(err(
                "google_draft_pending",
                "Resolve the current Google draft operation before changing its account",
            ));
        }
        let send = self.google_send_for_draft(account, draft)?;
        if matches!(
            send["state"].as_str(),
            Some("prepared" | "in_flight" | "acceptance_unknown" | "accepted")
        ) {
            return Err(err(
                "google_send_pending",
                "Inspect the original submission before changing this draft's account",
            ));
        }
        let deletion = self.google_delete_for_draft(account, draft)?;
        if matches!(
            deletion["state"].as_str(),
            Some("prepared" | "in_flight" | "acceptance_unknown" | "deleted")
        ) {
            return Err(err(
                "google_draft_pending",
                "Resolve the original draft deletion before changing its account",
            ));
        }
        Ok(json!({"draft":google,"send":send,"deletion":deletion}))
    }
    /// Plans moving (or, with `keep_original`, copying) a saved draft to
    /// another account.
    ///
    /// `p` holds `request_id`, `draft_id`, `expected_revision`,
    /// `expected_google_revision`, `target_account` and optional
    /// `keep_original`. Fails with `draft_transfer_pending` while the draft is
    /// part of an unfinished move, `limit_exceeded` with 32 moves pending, and
    /// `google_draft_pending` or `google_send_pending` while Google work owns
    /// the draft.
    pub fn draft_transfer_begin(&self, account: &str, p: &Value) -> Result<TransferStep> {
        if let Some(reply) = self.transfer_receipt(account, p)? {
            return Ok(TransferStep::Reply(reply));
        }
        let target = string(p, "target_account")?;
        if target == account {
            return Err(err("invalid_request", "Choose a different From account"));
        }
        let draft = string(p, "draft_id")?;
        self.require_active_account(account)?;
        let unfinished:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM draft_transfers WHERE ((account=?1 AND source_draft_id=?2 AND keep_original=0) OR (target_account=?1 AND target_draft_id=?2 AND state<>'complete'))) ",params![account,draft],|r|r.get(0))?;
        if unfinished {
            return Err(err(
                "draft_transfer_pending",
                "Finish the existing account change before moving this draft again",
            ));
        }
        let pending: i64 = self.db.query_row(
            "SELECT count(*) FROM draft_transfers WHERE state<>'complete'",
            [],
            |r| r.get(0),
        )?;
        if pending >= 32 {
            return Err(err(
                "limit_exceeded",
                "Finish pending draft account changes first",
            ));
        }
        let source_google = self.transfer_google_stamp(
            account,
            draft,
            p["expected_google_revision"].as_i64().unwrap(),
        )?;
        let retained = self.retained_draft(
            account,
            json!({"draft_id":draft,"expected_revision":p["expected_revision"]}),
        )?;
        if retained.draft()["discarded"] == true {
            return Err(err(
                "invalid_draft",
                "Restore the discarded draft before changing its account",
            ));
        }
        let raw = self
            .stored_composition(account, &json!({"draft_id":draft}))?
            .ok_or_else(|| err("not_found", "Draft is outside this account"))?;
        Ok(TransferStep::Prepare(Box::new(TransferPlan {
            account: account.into(),
            params: p.clone(),
            retained,
            raw,
            source_google,
            source_lifecycle: self.require_active_account(account)?,
            target: target.into(),
            target_stamp: self.transfer_target_stamp(target)?,
        })))
    }
    /// Creates the destination draft and, unless the original is kept,
    /// discards the source, all in one transaction with the receipt.
    ///
    /// The destination gets From set to its own address and copies of the
    /// attachments; source conversation and Gmail IDs are not carried over.
    /// When the source has a Gmail draft, the transfer waits for the
    /// destination's upload before that Gmail draft is deleted. Fails with
    /// `stale_revision` or `stale_google_revision` if either side changed
    /// during preparation.
    pub fn draft_transfer_finish(&mut self, proof: PreparedTransfer) -> Result<Value> {
        cancelled(&proof.cancel)?;
        let plan = proof.plan;
        if let Some(reply) = self.transfer_receipt(&plan.account, &plan.params)? {
            return Ok(reply);
        }
        self.check_retained_draft(&plan.retained)?;
        let pending: i64 = self.db.query_row(
            "SELECT count(*) FROM draft_transfers WHERE state<>'complete'",
            [],
            |r| r.get(0),
        )?;
        if pending >= 32 {
            return Err(err(
                "limit_exceeded",
                "Finish pending draft account changes first",
            ));
        }
        if self.transfer_target_stamp(&plan.target)? != plan.target_stamp {
            return Err(err(
                "stale_revision",
                "Destination account changed during preparation",
            ));
        }
        let source = string(&plan.params, "draft_id")?;
        if self.transfer_google_stamp(
            &plan.account,
            source,
            plan.params["expected_google_revision"].as_i64().unwrap(),
        )? != plan.source_google
        {
            return Err(err(
                "stale_google_revision",
                "Source draft state changed during preparation",
            ));
        }
        let request = string(&plan.params, "request_id")?;
        let keep = plan.params["keep_original"].as_bool().unwrap_or(false);
        // Derived from the request, so a retried transfer names the same
        // destination draft.
        let target_id = format!(
            "draft-{:x}",
            Sha256::digest(
                format!("transfer\0{}\0{}\0{request}", plan.account, plan.target).as_bytes()
            )
        );
        let mut routing: Value = serde_json::from_str(plan.raw["routing_json"].as_str().unwrap())?;
        routing["from"] = plan.target_stamp["email"].clone();
        crate::composition::validate_routing(&routing)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| err("invalid_request", "System clock cannot date this transfer"))?
            .as_secs() as i64;
        let tx = self.db.unchecked_transaction()?;
        let mut ids = Vec::new();
        for (index, file) in proof.files.into_iter().enumerate() {
            let id = format!(
                "transfer-file-{:x}",
                Sha256::digest(format!("{}\0{target_id}\0{index}", plan.target).as_bytes())
            );
            tx.execute("INSERT INTO attachments(account,id,message_id,name,mime_type,size,state,blob_hash) VALUES(?1,?2,NULL,?3,?4,?5,'available',?6)",params![plan.target,id,file.filename,file.content_type,file.bytes.len() as i64,file.sha256])?;
            ids.push(id);
        }
        // RFC reply headers and retained source bytes survive. Conversation and
        // provider message IDs never cross the account ownership boundary.
        tx.execute("INSERT INTO drafts(account,draft_id,mode,source_hash,routing_json,body,revision,body_html,attachments_json,discarded,signature_text,from_name,identity_revision,created_at,updated_at,origin) VALUES(?1,?2,?3,?4,?5,?6,1,?7,?8,0,?9,?10,?11,?12,?12,'local')",params![plan.target,target_id,plan.raw["mode"].as_str(),plan.raw["source_hash"].as_str(),routing.to_string(),plan.raw["body"].as_str(),plan.raw["body_html"].as_str(),json!(ids).to_string(),plan.raw["signature_text"].as_str(),plan.target_stamp["name"].as_str(),plan.target_stamp["revision"].as_i64(),now])?;
        if !keep {
            tx.execute("UPDATE drafts SET discarded=1,revision=revision+1,updated_at=?3 WHERE account=?1 AND draft_id=?2",params![plan.account,source,now])?;
        }
        // Only a moved draft with a Gmail copy has cleanup left to do.
        let remote = plan.source_google["draft"]["effective_target"]["provider_draft_id"].as_str();
        let state = if keep || remote.is_none() {
            "complete"
        } else {
            "waiting_target"
        };
        tx.execute("INSERT INTO draft_transfers(account,request_id,source_draft_id,target_account,target_draft_id,source_lifecycle,target_lifecycle,keep_original,state,source_google_json,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",params![plan.account,request,source,plan.target,target_id,plan.source_lifecycle,plan.target_stamp["lifecycle"].as_i64(),keep,state,plan.source_google.to_string(),now])?;
        let draft = self.composition(&plan.target, &json!({"draft_id":target_id}))?;
        let result = json!({"account":plan.account,"request_id":request,"source_draft_id":source,"target_account":plan.target,"target_draft_id":target_id,"state":state,"keep_original":keep,"draft":draft});
        tx.execute("INSERT INTO operations(account,request_id,fingerprint,result_json) VALUES(?1,?2,?3,?4)",params![plan.account,request,mutations::fingerprint("draft_transfer",&plan.params)?,result.to_string()])?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        cancelled(&proof.cancel)?;
        tx.commit()?;
        Ok(result)
    }
    /// Returns a transfer's status JSON: source and destination drafts, state,
    /// copy policy and any blocking error.
    pub fn draft_transfer_status(&self, account: &str, request: &str) -> Result<Value> {
        id(request)?;
        let row:Option<Value>=self.db.query_row("SELECT source_draft_id,target_account,target_draft_id,state,keep_original,error_json FROM draft_transfers WHERE account=?1 AND request_id=?2",params![account,request],|r|Ok(json!({"account":account,"request_id":request,"source_draft_id":r.get::<_,String>(0)?,"target_account":r.get::<_,String>(1)?,"target_draft_id":r.get::<_,String>(2)?,"state":r.get::<_,String>(3)?,"keep_original":r.get::<_,bool>(4)?,"error":r.get::<_,Option<String>>(5)?}))).optional()?;
        let mut row =
            row.ok_or_else(|| err("not_found", "Draft account change is outside this account"))?;
        if let Some(raw) = row["error"].as_str() {
            row["error"] = serde_json::from_str(raw)?;
        }
        Ok(row)
    }
    /// Status of the latest transfer that moved this draft away or created it,
    /// or JSON `null`.
    pub(crate) fn draft_account_transfer(&self, account: &str, draft: &str) -> Result<Value> {
        let row:Option<(String,String)>=self.db.query_row("SELECT account,request_id FROM draft_transfers WHERE (target_account=?1 AND target_draft_id=?2) OR (account=?1 AND source_draft_id=?2 AND keep_original=0) ORDER BY created_at DESC,request_id DESC LIMIT 1",params![account,draft],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        row.map(|(source, request)| self.draft_transfer_status(&source, &request))
            .transpose()
            .map(|row| row.unwrap_or(Value::Null))
    }
    /// Returns a `blocked` transfer to `waiting_target` so cleanup resumes.
    ///
    /// `p` holds `request_id` (this retry's own ID) and `transfer_request_id`.
    /// A fresh Gmail deletion is allowed only if the earlier one was
    /// definitively rejected or abandoned; after an unknown outcome the retry
    /// keeps the same deletion so it can only be reconciled.
    pub fn draft_transfer_retry(&mut self, account: &str, p: &Value) -> Result<Value> {
        fields(p, &["request_id", "transfer_request_id"])?;
        let request = string(p, "request_id")?;
        id(request)?;
        let transfer = string(p, "transfer_request_id")?;
        id(transfer)?;
        let fingerprint = mutations::fingerprint("draft_transfer_retry", p)?;
        let previous: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT fingerprint,result_json FROM operations WHERE account=?1 AND request_id=?2",
                params![account, request],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((stored, result)) = previous {
            if stored != fingerprint {
                return Err(err(
                    "request_id_conflict",
                    "Retry request already identifies another action",
                ));
            }
            return Ok(serde_json::from_str(&result)?);
        }
        let status = self.draft_transfer_status(account, transfer)?;
        if status["state"] != "blocked" {
            return Err(err(
                "invalid_request",
                "Only an interrupted account change needs retrying",
            ));
        }
        // A lost DELETE response only permits reading its existing receipt and
        // remote target. A new deletion identity requires a known rejection.
        self.transfer_destination_saved(account, transfer)?;
        let cleanup: Option<String> = self.db.query_row(
            "SELECT cleanup_request_id FROM draft_transfers WHERE account=?1 AND request_id=?2",
            params![account, transfer],
            |r| r.get(0),
        )?;
        let mut new_attempt = false;
        if let Some(cleanup) = cleanup {
            match self.google_draft_delete_status(account, &cleanup) {
                Ok(deletion) => {
                    if deletion["state"] == "in_flight" {
                        return Err(err("busy", "The original deletion is still running"));
                    }
                    new_attempt =
                        matches!(deletion["state"].as_str(), Some("rejected" | "abandoned"))
                            && deletion["read_evidence"]["draft_exists"] != false;
                }
                Err(error) if error.code == "not_found" => {}
                Err(error) => return Err(error),
            }
        }
        let tx = self.db.unchecked_transaction()?;
        tx.execute("UPDATE draft_transfers SET state='waiting_target',error_json=NULL,cleanup_generation=cleanup_generation+?3,cleanup_request_id=CASE WHEN ?3 THEN NULL ELSE cleanup_request_id END WHERE account=?1 AND request_id=?2",params![account,transfer,new_attempt])?;
        let result = self.draft_transfer_status(account, transfer)?;
        tx.execute("INSERT INTO operations(account,request_id,fingerprint,result_json) VALUES(?1,?2,?3,?4)",params![account,request,fingerprint,result.to_string()])?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(result)
    }
    /// Reports whether this draft was moved (not copied) to another account.
    pub(crate) fn draft_transferred(&self, account: &str, draft: &str) -> Result<bool> {
        Ok(self.db.query_row("SELECT EXISTS(SELECT 1 FROM draft_transfers WHERE account=?1 AND source_draft_id=?2 AND keep_original=0)",params![account,draft],|r|r.get(0))?)
    }

    /// Reports whether the destination draft is safely in Gmail (synced, or
    /// sent and accepted), which must hold before the original Gmail draft is
    /// deleted. Fails if either account changed lifecycle or the destination
    /// was discarded.
    fn transfer_destination_saved(&self, account: &str, request: &str) -> Result<bool> {
        let (target,draft,source_life,target_life):(String,String,i64,i64)=self.db.query_row("SELECT target_account,target_draft_id,source_lifecycle,target_lifecycle FROM draft_transfers WHERE account=?1 AND request_id=?2",params![account,request],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
        self.check_active_account_revision(account, source_life)?;
        self.check_active_account_revision(&target, target_life)?;
        let discarded: bool = self.db.query_row(
            "SELECT discarded FROM drafts WHERE account=?1 AND draft_id=?2",
            params![target, draft],
            |r| r.get(0),
        )?;
        if discarded {
            return Err(err(
                "draft_transfer_target_discarded",
                "The destination draft was discarded; the original Google draft is preserved",
            ));
        }
        let google = self.google_draft_status(&target, &draft)?;
        let send = self.google_send_for_draft(&target, &draft)?;
        Ok((google["state"] == "synced"
            && google["baseline"]["draft_revision"]
                .as_i64()
                .is_some_and(|n| n >= 1))
            || (send["state"] == "accepted"
                && send["draft_revision"].as_i64().is_some_and(|n| n >= 1)))
    }
    fn block_transfer(
        &self,
        account: &str,
        request: &str,
        code: &str,
        message: &str,
    ) -> Result<()> {
        let tx = self.db.unchecked_transaction()?;
        if tx.execute("UPDATE draft_transfers SET state='blocked',error_json=?3 WHERE account=?1 AND request_id=?2 AND state<>'complete'",params![account,request,json!({"code":code,"message":message}).to_string()])?>0 {
            tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'",[])?;
        }
        tx.commit()?;
        Ok(())
    }
    /// Advances pending transfers and returns the next Gmail deletion to run,
    /// if any.
    ///
    /// Transfers whose deletion already succeeded, or whose original is
    /// observed gone, complete here. Transfers whose destination is not yet
    /// saved wait. Failures that need the user's attention block the transfer.
    /// The returned cleanup is marked `cleanup_pending` first.
    pub fn draft_transfer_cleanup_due(&mut self, account: &str) -> Result<Option<TransferCleanup>> {
        self.require_active_account(account)?;
        let mut statement=self.db.prepare("SELECT request_id,source_draft_id,source_google_json,cleanup_request_id,cleanup_generation FROM draft_transfers WHERE account=?1 AND state IN ('waiting_target','cleanup_pending') ORDER BY created_at,request_id LIMIT 32")?;
        let rows = statement
            .query_map([account], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(statement);
        for (request, source, original, cleanup, generation) in rows {
            // An acknowledged deletion finishes locally even if an account was
            // subsequently removed. It never starts another provider request.
            if let Some(ref cleanup) = cleanup {
                match self.google_draft_delete_status(account, cleanup) {
                    Ok(status)
                        if status["state"] == "deleted"
                            || status["read_evidence"]["draft_exists"] == false =>
                    {
                        self.draft_transfer_cleanup_result(account, &request, Some(&status), None)?;
                        continue;
                    }
                    Ok(status) if status["state"] == "in_flight" => continue,
                    Ok(_) => {}
                    Err(error) if error.code == "not_found" => {}
                    Err(error) => return Err(error),
                }
            }
            match self.transfer_destination_saved(account, &request) {
                Ok(false) => continue,
                Ok(true) => {}
                Err(error) => {
                    self.block_transfer(account, &request, &error.code, &error.message)?;
                    continue;
                }
            }
            // The deletion ID is derived from the transfer and its cleanup
            // generation, so re-deriving it finds the existing journal entry.
            let cleanup = cleanup.unwrap_or_else(|| {
                format!(
                    "transfer-delete-{:x}",
                    Sha256::digest(format!("{account}\0{request}\0{generation}").as_bytes())
                )
            });
            let existing = match self.google_draft_delete_status(account, &cleanup) {
                Ok(status) => Some(status),
                Err(error) if error.code == "not_found" => None,
                Err(error) => return Err(error),
            };
            let (params, reconcile) = if let Some(status) = existing {
                if matches!(status["state"].as_str(), Some("rejected" | "abandoned")) {
                    self.block_transfer(account,&request,"google_draft_cleanup_failed","The original Google draft was kept. Check the account change and retry when ready")?;
                    continue;
                }
                (
                    status["request"].clone(),
                    status["state"] == "acceptance_unknown",
                )
            } else {
                let original: Value = serde_json::from_str(&original)?;
                match self.linked_draft_delete_request(
                    account,
                    &source,
                    &original["draft"]["effective_target"],
                    &cleanup,
                ) {
                    Ok(p) => (p, false),
                    Err(error) if error.code == "content_unavailable" => continue,
                    Err(error) => {
                        self.block_transfer(account, &request, &error.code, &error.message)?;
                        continue;
                    }
                }
            };
            let tx = self.db.unchecked_transaction()?;
            tx.execute("UPDATE draft_transfers SET state='cleanup_pending',cleanup_request_id=?3,error_json=NULL WHERE account=?1 AND request_id=?2",params![account,request,cleanup])?;
            tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
            tx.commit()?;
            return Ok(Some(TransferCleanup {
                transfer_request_id: request,
                params,
                reconcile,
            }));
        }
        Ok(None)
    }
    /// Refuses a deletion that belongs to a transfer unless that transfer is
    /// `cleanup_pending` with its destination confirmed saved.
    pub(crate) fn check_transfer_cleanup_authority(
        &self,
        account: &str,
        cleanup: &str,
    ) -> Result<()> {
        let transfer:Option<(String,String)>=self.db.query_row("SELECT request_id,state FROM draft_transfers WHERE account=?1 AND cleanup_request_id=?2",params![account,cleanup],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((request, state)) = transfer {
            if state != "cleanup_pending" || !self.transfer_destination_saved(account, &request)? {
                return Err(err(
                    "stale_google_work",
                    "Destination draft is not confirmed; the original Google draft is preserved",
                ));
            }
        }
        Ok(())
    }
    /// Records the result of a transfer's Gmail cleanup and returns the
    /// transfer status.
    ///
    /// `status` is the deletion's status JSON; `error` is a failure before any
    /// status. A confirmed deletion or an observed-absent draft completes the
    /// transfer; anything else blocks it for the user to check.
    pub fn draft_transfer_cleanup_result(
        &mut self,
        account: &str,
        request: &str,
        status: Option<&Value>,
        error: Option<&crate::CoreError>,
    ) -> Result<Value> {
        let cleanup: Option<String> = self.db.query_row(
            "SELECT cleanup_request_id FROM draft_transfers WHERE account=?1 AND request_id=?2",
            params![account, request],
            |r| r.get(0),
        )?;
        if let Some(status) = status {
            if status["account"] != account || status["request_id"].as_str() != cleanup.as_deref() {
                return Err(err(
                    "scope_mismatch",
                    "Deletion result does not belong to this account change",
                ));
            }
            // Absence is enough to finish the move, while the deletion receipt
            // keeps its original certainty and notification/delivery meaning.
            if status["state"] == "deleted" || status["read_evidence"]["draft_exists"] == false {
                let tx = self.db.unchecked_transaction()?;
                if tx.execute("UPDATE draft_transfers SET state='complete',error_json=NULL WHERE account=?1 AND request_id=?2 AND state<>'complete'",params![account,request])?>0 {
                    tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'",[])?;
                }
                tx.commit()?;
            } else {
                self.block_transfer(account,request,"google_draft_cleanup_unconfirmed","The destination is saved, but removal of the original Google draft is not confirmed. Check the account change before retrying")?;
            }
        } else if let Some(error) = error {
            self.block_transfer(account, request, &error.code, &error.message)?;
        }
        self.draft_transfer_status(account, request)
    }
}
