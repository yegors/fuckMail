//! Explicit Google submissions. The journal owns certainty; read-only provider
//! observations are retained separately and cannot turn a match into delivery.
use crate::{
    composition, content::BlobStore, err, fields, google_drafts, id, local_content, string, Result,
    Store,
};
use fm_google::transport::CancellationToken;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub use google_drafts::GoogleDraftScope as GoogleSendScope;
pub const MAX_GOOGLE_SEND_BYTES: u64 = 35_000_000;
const MAX_METADATA: usize = 512 * 1024;
const MAX_EVIDENCE: usize = 32 * 1024;

/// Stored inside the existing draft twin. A later rejection must not forget a
/// previously consumed target. Authored fields and revision are never changed.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SendGuard {
    request_id: String,
    state: String,
    draft_revision: i64,
    last_accepted_draft_revision: Option<i64>,
    blocks_drafts: bool,
}

pub enum GoogleSendIntent {
    Reply(Value),
    Submit { provider_draft_id: Option<String> },
}
pub enum GoogleSendStep {
    Reply(Value),
    Prepare(GoogleSendPlan),
}
pub struct GoogleSendPlan(Box<Plan>);
struct Plan {
    scope: GoogleSendScope,
    request: Value,
    retained: local_content::RetainedDraft,
    target: Option<String>,
    thread_id: Option<String>,
    baseline: Value,
}
impl GoogleSendPlan {
    pub fn provider_draft_id(&self) -> Option<&str> {
        self.0.target.as_deref()
    }
}
pub struct PreparedGoogleSend {
    plan: Box<Plan>,
    hash: String,
    size: u64,
    message_id: String,
    created_at: u64,
    cancel: CancellationToken,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct AcceptedIds {
    message_id: String,
    thread_id: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Operation {
    version: u32,
    kind: String,
    account: String,
    draft_id: String,
    request_id: String,
    request: Value,
    revision: i64,
    state: String,
    draft_revision: i64,
    google_revision: i64,
    connection_generation: i64,
    provider_draft_id: Option<String>,
    thread_id: Option<String>,
    baseline: Value,
    message_id: String,
    upload_sha256: String,
    size: u64,
    headers: Value,
    created_at: u64,
    ack: Option<AcceptedIds>,
    error: Option<String>,
    read_evidence: Option<Value>,
}

#[derive(Clone)]
pub struct GoogleSendLease {
    blobs: BlobStore,
    operation: Operation,
}
#[derive(Clone)]
pub struct GoogleSendUploadPlan {
    blobs: BlobStore,
    hash: String,
    size: u64,
}
impl GoogleSendLease {
    pub fn account(&self) -> &str {
        &self.operation.account
    }
    pub fn request_id(&self) -> &str {
        &self.operation.request_id
    }
    pub fn draft_id(&self) -> &str {
        &self.operation.draft_id
    }
    pub fn from(&self) -> &str {
        self.operation.headers["from"].as_str().unwrap()
    }
    pub fn provider_draft_id(&self) -> Option<&str> {
        self.operation.provider_draft_id.as_deref()
    }
    pub fn thread_id(&self) -> Option<&str> {
        self.operation.thread_id.as_deref()
    }
    pub fn message_id(&self) -> &str {
        &self.operation.message_id
    }
    pub fn upload_sha256(&self) -> &str {
        &self.operation.upload_sha256
    }
    pub fn size(&self) -> u64 {
        self.operation.size
    }
    pub fn revision(&self) -> i64 {
        self.operation.revision
    }
    pub fn take_upload(&self) -> GoogleSendUploadPlan {
        GoogleSendUploadPlan {
            blobs: self.blobs.clone(),
            hash: self.operation.upload_sha256.clone(),
            size: self.operation.size,
        }
    }
}
pub enum GoogleSendOutcome {
    Accepted {
        message_id: String,
        thread_id: String,
    },
    Rejected {
        code: String,
    },
    Undispatched {
        code: String,
    },
    Unknown {
        code: String,
    },
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GoogleSendObservedMessage {
    pub message_id: String,
    pub thread_id: String,
    pub history_id: Option<String>,
    pub labels: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct GoogleSendEvidence {
    pub query: String,
    pub draft_observation: String,
    pub truncated: bool,
    pub matches: Vec<GoogleSendObservedMessage>,
    pub acknowledged_message: Option<GoogleSendObservedMessage>,
    pub missing_ids: Vec<String>,
}
pub struct GoogleSendReconcilePlan {
    profile: std::path::PathBuf,
    scope: GoogleSendScope,
    operation: Operation,
}
impl GoogleSendReconcilePlan {
    pub fn message_id(&self) -> &str {
        &self.operation.message_id
    }
    pub fn accepted_message_id(&self) -> Option<&str> {
        self.operation.ack.as_ref().map(|a| a.message_id.as_str())
    }
    pub fn provider_draft_id(&self) -> Option<&str> {
        self.operation.provider_draft_id.as_deref()
    }
}

impl Store {
    /// Metadata-only admission: no retained body, MIME, attachment or open file
    /// escapes this call while the engine waits for submission quota.
    pub fn google_send_intent(
        &self,
        scope: &GoogleSendScope,
        p: &Value,
    ) -> Result<GoogleSendIntent> {
        validate_request(p)?;
        self.draft_scope(scope)?;
        let request = request_id(p)?;
        if let Some(op) = self.google_send_operation(&scope.account, request)? {
            if op.request != *p {
                return Err(err(
                    "request_id_conflict",
                    "Request ID already freezes another submission",
                ));
            }
            if op.state == "prepared" {
                self.check_send_resume(scope, &op)?;
                return Ok(GoogleSendIntent::Submit {
                    provider_draft_id: op.provider_draft_id,
                });
            }
            return Ok(GoogleSendIntent::Reply(self.send_reply(&op)?));
        }
        let draft_id = string(p, "draft_id")?;
        let twin = self.google_draft_status(&scope.account, draft_id)?;
        let local_revision = twin["local_revision"].as_i64().ok_or_else(invalid)?;
        let google_revision = twin["revision"].as_i64().ok_or_else(invalid)?;
        let expected_local = positive(p, "expected_revision")?;
        let expected_google = nonnegative(p, "expected_google_revision")?;
        let stale = if local_revision != expected_local {
            Some(err("stale_revision", "Local draft revision changed"))
        } else if google_revision != expected_google {
            Some(err(
                "stale_google_revision",
                "Google draft changed; inspect it before sending",
            ))
        } else {
            None
        };
        if let Some(mut failure) = stale {
            // The same writer has proved that this exact request has no journal
            // row and a monotonic revision has passed its frozen expectation.
            // Every held preparation re-enters this check before publication;
            // no claim can exist or be created without that journal row. Mere
            // absence, future revisions and existing operations prove nothing.
            if local_revision > expected_local || google_revision > expected_google {
                failure.details = json!({"send_request_obsolete_without_dispatch":true});
            }
            return Err(failure);
        }
        self.check_draft_writable(&scope.account, &json!({"draft_id":draft_id}))?;
        let guard = self.send_guard(&scope.account, draft_id)?;
        if guard.as_ref().is_some_and(|g| pending(&g.state)) {
            return Err(err(
                "google_send_pending",
                "Inspect the original submission before starting another",
            ));
        }
        let accepted = guard.as_ref().and_then(|g| g.last_accepted_draft_revision);
        if accepted.is_some_and(|r| positive(p, "expected_revision").unwrap() <= r) {
            return Err(err("google_send_consumed", "This saved version was already accepted; only newer authored work can form another email"));
        }
        if accepted.is_none()
            && matches!(
                twin["state"].as_str(),
                Some("prepared" | "in_flight" | "acceptance_unknown" | "acknowledged" | "conflict")
            )
        {
            return Err(err(
                "google_draft_pending",
                "Unresolved Google draft work cannot be bypassed by Send",
            ));
        }
        self.check_send_identity(scope, draft_id)?;
        let target = if accepted.is_some() {
            None
        } else {
            twin["baseline"]["draft_id"].as_str().map(str::to_owned)
        };
        Ok(GoogleSendIntent::Submit {
            provider_draft_id: target,
        })
    }

    pub fn google_send_begin(&self, scope: &GoogleSendScope, p: &Value) -> Result<GoogleSendStep> {
        let target = match self.google_send_intent(scope, p)? {
            GoogleSendIntent::Reply(value) => return Ok(GoogleSendStep::Reply(value)),
            GoogleSendIntent::Submit { provider_draft_id } => provider_draft_id,
        };
        if let Some(op) = self.google_send_operation(&scope.account, request_id(p)?)? {
            return Ok(GoogleSendStep::Reply(self.send_reply(&op)?));
        }
        let draft_id = string(p, "draft_id")?;
        let retained = self.retained_draft(
            &scope.account,
            json!({"draft_id":draft_id,"expected_revision":positive(p,"expected_revision")?}),
        )?;
        let draft = retained.draft();
        let thread_id = if matches!(draft["mode"].as_str(), Some("reply" | "reply_all")) {
            self.db.query_row("SELECT remote_thread_id FROM google_message_state WHERE account=?1 AND message_id=?2 AND deleted=0",params![scope.account,draft["source_message_id"].as_str()],|r|r.get(0)).optional()?
        } else {
            None
        };
        let baseline = if target.is_some() {
            self.google_draft_status(&scope.account, draft_id)?["baseline"].clone()
        } else {
            Value::Null
        };
        Ok(GoogleSendStep::Prepare(GoogleSendPlan(Box::new(Plan {
            scope: scope.clone(),
            request: p.clone(),
            retained,
            target,
            thread_id,
            baseline,
        }))))
    }

    pub fn google_send_finish(&mut self, proof: PreparedGoogleSend) -> Result<Value> {
        cancelled(&proof.cancel)?;
        let plan = proof.plan;
        match self.google_send_intent(&plan.scope, &plan.request)? {
            GoogleSendIntent::Reply(value) => return Ok(value),
            GoogleSendIntent::Submit { provider_draft_id } if provider_draft_id == plan.target => {}
            _ => {
                return Err(err(
                    "stale_google_work",
                    "Submission target changed during preparation",
                ))
            }
        }
        if let Some(op) =
            self.google_send_operation(&plan.scope.account, request_id(&plan.request)?)?
        {
            return self.send_reply(&op);
        }
        self.check_retained_draft(&plan.retained)?;
        let draft = plan.retained.draft();
        let mut headers = json!({});
        for key in [
            "from",
            "from_name",
            "to_recipients",
            "cc_recipients",
            "bcc_recipients",
            "subject",
            "reply_to_message_id",
            "references",
            "source_message_id",
            "source_hash",
            "mode",
        ] {
            headers[key] = draft[key].clone();
        }
        let op = Operation {
            version: 1,
            kind: "send".into(),
            account: plan.scope.account.clone(),
            draft_id: string(&plan.request, "draft_id")?.into(),
            request_id: request_id(&plan.request)?.into(),
            request: plan.request.clone(),
            revision: 1,
            state: "prepared".into(),
            draft_revision: positive(&plan.request, "expected_revision")?,
            google_revision: nonnegative(&plan.request, "expected_google_revision")?,
            connection_generation: plan.scope.connection_generation,
            provider_draft_id: plan.target,
            thread_id: plan.thread_id,
            baseline: plan.baseline,
            message_id: proof.message_id,
            upload_sha256: proof.hash,
            size: proof.size,
            headers,
            created_at: proof.created_at,
            ack: None,
            error: None,
            read_evidence: None,
        };
        self.persist_send(&op, true)?;
        self.send_reply(&op)
    }

    pub fn google_send_claim(
        &mut self,
        scope: &GoogleSendScope,
        request: &str,
        expected_operation_revision: i64,
    ) -> Result<GoogleSendLease> {
        self.draft_scope(scope)?;
        let mut op = self
            .google_send_operation(&scope.account, request)?
            .ok_or_else(not_found)?;
        if op.revision != expected_operation_revision || op.state != "prepared" {
            return Err(err(
                "stale_google_work",
                "Only the current proven-undispatched submission can be claimed",
            ));
        }
        self.check_send_resume(scope, &op)?;
        op.revision += 1;
        op.state = "in_flight".into();
        op.error = None;
        op.connection_generation = scope.connection_generation;
        self.persist_send(&op, false)?;
        Ok(GoogleSendLease {
            blobs: self.google_blob_store(),
            operation: op,
        })
    }

    /// Exact late outcomes remain durable after cancellation or reconnect. The
    /// lease fences one claim; reconnect cannot fabricate a second HTTP attempt.
    pub fn google_send_outcome(
        &mut self,
        lease: &GoogleSendLease,
        outcome: GoogleSendOutcome,
    ) -> Result<Value> {
        if lease.blobs.profile != self.profile {
            return Err(err(
                "scope_mismatch",
                "Submission lease belongs to another profile",
            ));
        }
        let mut op = self
            .google_send_operation(lease.account(), lease.request_id())?
            .ok_or_else(not_found)?;
        let (state, ack, error) = match outcome {
            GoogleSendOutcome::Accepted {
                message_id,
                thread_id,
            } => {
                provider_id(&message_id)?;
                provider_id(&thread_id)?;
                (
                    "accepted",
                    Some(AcceptedIds {
                        message_id,
                        thread_id,
                    }),
                    None,
                )
            }
            GoogleSendOutcome::Rejected { code } => ("rejected", None, Some(error_code(code)?)),
            GoogleSendOutcome::Undispatched { code } => ("prepared", None, Some(error_code(code)?)),
            GoogleSendOutcome::Unknown { code } => {
                ("acceptance_unknown", None, Some(error_code(code)?))
            }
        };
        if op.revision != lease.revision() || op.state != "in_flight" {
            if op.revision == lease.revision() + 1
                && op.state == state
                && op.ack == ack
                && op.error == error
            {
                return self.send_reply(&op);
            }
            return Err(err(
                "stale_google_work",
                "Submission outcome does not match the active claim",
            ));
        }
        op.revision += 1;
        op.state = state.into();
        op.ack = ack;
        op.error = error;
        self.persist_send(&op, false)?;
        self.send_reply(&op)
    }

    pub fn google_send_status(&self, account: &str, request: &str) -> Result<Value> {
        id(account)?;
        id(request)?;
        let op = self
            .google_send_operation(account, request)?
            .ok_or_else(not_found)?;
        self.send_reply(&op)
    }
    pub(crate) fn google_send_for_draft(&self, account: &str, draft_id: &str) -> Result<Value> {
        match self.send_guard(account, draft_id)? {
            Some(guard) => self.google_send_status(account, &guard.request_id),
            None => Ok(Value::Null),
        }
    }
    pub fn google_send_abandon(&mut self, account: &str, p: &Value) -> Result<Value> {
        fields(p, &["request_id", "expected_revision"])?;
        id(account)?;
        let mut op = self
            .google_send_operation(account, request_id(p)?)?
            .ok_or_else(not_found)?;
        if op.state != "prepared" || op.revision != positive(p, "expected_revision")? {
            return Err(err(
                "stale_google_work",
                "Only this revision of a proven-undispatched submission can be abandoned",
            ));
        }
        let guard = self
            .send_guard(account, &op.draft_id)?
            .ok_or_else(invalid)?;
        if guard.request_id != op.request_id {
            return Err(err("stale_google_work", "Submission is no longer current"));
        }
        op.revision += 1;
        op.state = "abandoned".into();
        op.error = None;
        self.persist_send(&op, false)?;
        self.send_reply(&op)
    }

    pub fn google_send_reconcile_begin(
        &self,
        scope: &GoogleSendScope,
        request: &str,
    ) -> Result<GoogleSendReconcilePlan> {
        self.draft_scope(scope)?;
        let operation = self
            .google_send_operation(&scope.account, request)?
            .ok_or_else(not_found)?;
        if operation.state == "in_flight" {
            return Err(err(
                "google_send_pending",
                "The active submission must finish before read reconciliation",
            ));
        }
        Ok(GoogleSendReconcilePlan {
            profile: self.profile.clone(),
            scope: scope.clone(),
            operation,
        })
    }
    pub fn google_send_reconcile_finish(
        &mut self,
        plan: GoogleSendReconcilePlan,
        evidence: GoogleSendEvidence,
    ) -> Result<Value> {
        if plan.profile != self.profile {
            return Err(err(
                "scope_mismatch",
                "Submission observation belongs to another profile",
            ));
        }
        self.draft_scope(&plan.scope)?;
        let mut op = self
            .google_send_operation(&plan.scope.account, &plan.operation.request_id)?
            .ok_or_else(not_found)?;
        if op.revision != plan.operation.revision {
            return Err(err(
                "stale_google_work",
                "Submission changed during read reconciliation",
            ));
        }
        validate_evidence(&evidence, &op)?;
        let mut value = serde_json::to_value(evidence)?;
        value["checked_at"] = json!(now()?.0);
        op.read_evidence = Some(value);
        op.revision += 1;
        // Neither candidates nor their absence changes state, ACK or error.
        self.persist_send(&op, false)?;
        self.send_reply(&op)
    }

    pub(crate) fn recover_google_sends(&mut self) -> Result<()> {
        let rows=self.db.prepare("SELECT account,id FROM capture_operations WHERE execution_kind='google' AND json_extract(json,'$.kind')='send' AND json_extract(json,'$.state')='in_flight'")?.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<std::result::Result<Vec<_>,_>>()?;
        for (account, request) in rows {
            let mut op = self
                .google_send_operation(&account, &request)?
                .ok_or_else(invalid)?;
            op.revision += 1;
            op.state = "acceptance_unknown".into();
            op.error = Some("engine_interrupted".into());
            self.persist_send(&op, false)?;
        }
        Ok(())
    }

    fn google_send_operation(&self, account: &str, request: &str) -> Result<Option<Operation>> {
        google_drafts::check_google_operation_kind(&self.db, account, request, "send")?;
        let row:Option<(String,String)>=self.db.query_row("SELECT json,blob_hash FROM capture_operations WHERE execution_kind='google' AND json_extract(json,'$.kind')='send' AND account=?1 AND id=?2",params![account,request],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        row.map(|(raw, hash)| {
            if raw.len() > MAX_METADATA {
                return Err(invalid());
            }
            let op: Operation = serde_json::from_str(&raw)?;
            if op.version != 1
                || op.kind != "send"
                || op.account != account
                || op.request_id != request
                || op.upload_sha256 != hash
                || op.size == 0
                || op.size > MAX_GOOGLE_SEND_BYTES
                || op.revision < 1
                || op.headers["from"].as_str().is_none()
            {
                return Err(invalid());
            }
            Ok(op)
        })
        .transpose()
    }
    fn send_guard(&self, account: &str, draft_id: &str) -> Result<Option<SendGuard>> {
        let raw:Option<String>=self.db.query_row("SELECT json_extract(google_json,'$.submission') FROM drafts WHERE account=?1 AND draft_id=?2",params![account,draft_id],|r|r.get(0)).optional()?.flatten();
        raw.map(|raw| serde_json::from_str(&raw).map_err(Into::into))
            .transpose()
    }
    pub(crate) fn check_google_send_draft_write(
        &self,
        account: &str,
        draft_id: &str,
    ) -> Result<()> {
        self.check_draft_writable(account, &json!({"draft_id":draft_id}))?;
        if self
            .send_guard(account, draft_id)?
            .is_some_and(|g| g.blocks_drafts)
        {
            return Err(err("google_send_pending","Submission owns this target; it cannot be recreated or overwritten by draft synchronization"));
        }
        Ok(())
    }
    pub(crate) fn check_google_send_draft_observation(
        &self,
        account: &str,
        draft_id: &str,
    ) -> Result<()> {
        self.check_draft_writable(account, &json!({"draft_id":draft_id}))?;
        if self.send_guard(account, draft_id)?.is_some_and(|g| {
            g.last_accepted_draft_revision.is_some()
                || matches!(g.state.as_str(), "in_flight" | "acceptance_unknown")
        }) {
            return Err(err(
                "google_send_pending",
                "Inspect the submission; its remote draft may have been consumed",
            ));
        }
        Ok(())
    }
    fn check_send_identity(&self, scope: &GoogleSendScope, draft_id: &str) -> Result<()> {
        self.check_draft_writable(&scope.account, &json!({"draft_id":draft_id}))?;
        let (from,discarded):(String,bool)=self.db.query_row("SELECT json_extract(routing_json,'$.from'),discarded FROM drafts WHERE account=?1 AND draft_id=?2",params![scope.account,draft_id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        google_drafts::same_from(&from, &scope.primary_email)?;
        if discarded {
            return Err(err("invalid_draft", "Discarded draft cannot be sent"));
        }
        Ok(())
    }
    fn check_send_resume(&self, scope: &GoogleSendScope, op: &Operation) -> Result<()> {
        google_drafts::same_from(
            op.headers["from"].as_str().ok_or_else(invalid)?,
            &scope.primary_email,
        )?;
        self.check_send_identity(scope, &op.draft_id)?;
        let guard = self
            .send_guard(&scope.account, &op.draft_id)?
            .ok_or_else(invalid)?;
        if guard.request_id != op.request_id || guard.state != "prepared" {
            return Err(err(
                "stale_google_work",
                "Submission no longer owns its frozen target",
            ));
        }
        let twin = self.google_draft_status(&scope.account, &op.draft_id)?;
        if op.provider_draft_id.is_some()
            && (twin["baseline"] != op.baseline || twin["state"] == "conflict")
        {
            return Err(err(
                "google_draft_conflict",
                "The frozen remote draft changed before submission",
            ));
        }
        Ok(())
    }
    fn send_reply(&self, op: &Operation) -> Result<Value> {
        let twin = self.google_draft_status(&op.account, &op.draft_id)?;
        let guard = self.send_guard(&op.account, &op.draft_id)?;
        let mut value = json!({"account":op.account,"draft_id":op.draft_id,"request_id":op.request_id,"revision":op.revision,"current_google_revision":twin["revision"],"state":op.state,"draft_revision":op.draft_revision,"google_revision":op.google_revision,"connection_generation":op.connection_generation,"operation_is_current":guard.as_ref().is_some_and(|g|g.request_id==op.request_id),"request":op.request,"provider_draft_id":op.provider_draft_id,"thread_id":op.thread_id,"message_id":op.message_id,"upload_sha256":op.upload_sha256,"size":op.size,"created_at":op.created_at,"ack":op.ack,"error":op.error,"read_evidence":op.read_evidence,"last_accepted_draft_revision":guard.and_then(|g|g.last_accepted_draft_revision)});
        for key in [
            "from",
            "to_recipients",
            "cc_recipients",
            "bcc_recipients",
            "subject",
        ] {
            value[key] = op.headers[key].clone();
        }
        Ok(value)
    }
    fn persist_send(&mut self, op: &Operation, insert: bool) -> Result<()> {
        let raw = serde_json::to_string(op)?;
        if raw.len() > MAX_METADATA {
            return Err(err(
                "limit_exceeded",
                "Submission metadata exceeds its bound",
            ));
        }
        let previous = self.send_guard(&op.account, &op.draft_id)?;
        let current = insert
            || previous
                .as_ref()
                .is_some_and(|g| g.request_id == op.request_id);
        let twin_raw = if current {
            let accepted = previous.and_then(|g| g.last_accepted_draft_revision);
            let accepted = if op.state == "accepted" {
                Some(accepted.unwrap_or(0).max(op.draft_revision))
            } else {
                accepted
            };
            Some(self.google_send_twin_raw(
                &op.account,
                &op.draft_id,
                SendGuard {
                    request_id: op.request_id.clone(),
                    state: op.state.clone(),
                    draft_revision: op.draft_revision,
                    last_accepted_draft_revision: accepted,
                    blocks_drafts: pending(&op.state) || accepted.is_some(),
                },
            )?)
        } else {
            None
        };
        let tx = self.db.transaction()?;
        if insert {tx.execute("INSERT INTO capture_operations(account,id,json,blob_hash,execution_kind) VALUES(?1,?2,?3,?4,'google')",params![op.account,op.request_id,raw,op.upload_sha256])?;}
        else if tx.execute("UPDATE capture_operations SET json=?3 WHERE account=?1 AND id=?2 AND execution_kind='google' AND json_extract(json,'$.kind')='send'",params![op.account,op.request_id,raw])?!=1 {return Err(invalid());}
        if let Some(raw) = twin_raw {
            if tx.execute(
                "UPDATE drafts SET google_json=?3 WHERE account=?1 AND draft_id=?2",
                params![op.account, op.draft_id, raw],
            )? != 1
            {
                return Err(invalid());
            }
        }
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(())
    }
}

pub fn prepare_google_send(
    plan: GoogleSendPlan,
    cancel: &CancellationToken,
) -> Result<PreparedGoogleSend> {
    cancelled(cancel)?;
    let plan = plan.0;
    let parts = plan.retained.prepare(cancel)?;
    let (created_at, nanos) = now()?;
    let digest = format!(
        "{:x}",
        Sha256::digest(
            format!(
                "send\0{}\0{}\0{nanos}",
                plan.scope.account,
                request_id(&plan.request)?
            )
            .as_bytes()
        )
    );
    let message_id = format!("<send-{digest}@fuckmail.invalid>");
    let (request, identity) = composition::compose_request(
        &plan.scope.account,
        plan.retained.draft(),
        message_id.clone(),
        created_at,
    )?;
    google_drafts::same_from(&identity.from.email, &plan.scope.primary_email)?;
    let built = fm_mail::build_mail(&request, &identity, &parts).map_err(|_| {
        err(
            "invalid_draft",
            "Submission requires valid recipients and bounded strict MIME",
        )
    })?;
    let size = built.raw.len() as u64;
    if size > MAX_GOOGLE_SEND_BYTES {
        return Err(err(
            "limit_exceeded",
            "Google submission exceeds 35000000 bytes",
        ));
    }
    cancelled(cancel)?;
    let hash = plan.retained.blobs.stage_blob(&built.raw)?;
    cancelled(cancel)?;
    Ok(PreparedGoogleSend {
        plan,
        hash,
        size,
        message_id,
        created_at,
        cancel: cancel.clone(),
    })
}
pub fn load_google_send_upload(
    plan: GoogleSendUploadPlan,
    cancel: &CancellationToken,
) -> Result<Vec<u8>> {
    cancelled(cancel)?;
    if plan.size == 0 || plan.size > MAX_GOOGLE_SEND_BYTES {
        return Err(invalid());
    }
    let bytes = local_content::read_blob(&plan.blobs, &plan.hash, plan.size)?;
    cancelled(cancel)?;
    Ok(bytes)
}
fn validate_request(p: &Value) -> Result<()> {
    fields(
        p,
        &[
            "draft_id",
            "expected_revision",
            "expected_google_revision",
            "request_id",
        ],
    )?;
    string(p, "draft_id")?;
    request_id(p)?;
    positive(p, "expected_revision")?;
    nonnegative(p, "expected_google_revision")?;
    Ok(())
}
fn validate_evidence(e: &GoogleSendEvidence, op: &Operation) -> Result<()> {
    if e.query != "sent_message_id"
        || e.draft_observation != "not_checked"
        || e.matches.len() > 10
        || e.missing_ids.len() > 11
        || serde_json::to_vec(e)?.len() > MAX_EVIDENCE
    {
        return Err(err(
            "invalid_google_response",
            "Submission evidence exceeds its bounded read scope",
        ));
    }
    let mut ids = std::collections::HashSet::new();
    for row in e.matches.iter().chain(e.acknowledged_message.iter()) {
        provider_id(&row.message_id)?;
        provider_id(&row.thread_id)?;
        if row
            .history_id
            .as_ref()
            .is_some_and(|h| h.is_empty() || h.len() > 64 || !h.bytes().all(|c| c.is_ascii_digit()))
            || row.labels.len() > 100
        {
            return Err(err(
                "invalid_google_response",
                "Submission evidence metadata is invalid",
            ));
        }
        for label in &row.labels {
            provider_id(label)?;
        }
    }
    for row in &e.matches {
        if !ids.insert(&row.message_id) {
            return Err(err(
                "invalid_google_response",
                "Submission evidence repeats a candidate",
            ));
        }
    }
    if let Some(row) = &e.acknowledged_message {
        if op.ack.as_ref().map(|a| &a.message_id) != Some(&row.message_id) {
            return Err(err(
                "invalid_google_response",
                "Acknowledged message evidence targets another identity",
            ));
        }
    }
    let mut missing = std::collections::HashSet::new();
    for missing_id in &e.missing_ids {
        provider_id(missing_id)?;
        if !missing.insert(missing_id)
            || ids.contains(missing_id)
            || e.acknowledged_message
                .as_ref()
                .is_some_and(|a| &a.message_id == missing_id)
        {
            return Err(err(
                "invalid_google_response",
                "Submission evidence contradicts its missing identities",
            ));
        }
    }
    Ok(())
}
fn pending(state: &str) -> bool {
    matches!(state, "prepared" | "in_flight" | "acceptance_unknown")
}
fn positive(p: &Value, key: &str) -> Result<i64> {
    p[key]
        .as_i64()
        .filter(|n| *n > 0)
        .ok_or_else(|| err("invalid_request", "A positive revision is required"))
}
fn nonnegative(p: &Value, key: &str) -> Result<i64> {
    p[key].as_i64().filter(|n| *n >= 0).ok_or_else(|| {
        err(
            "invalid_request",
            "A nonnegative Google revision is required",
        )
    })
}
fn request_id(p: &Value) -> Result<&str> {
    let s = string(p, "request_id")?;
    if s.len() > 128 {
        return Err(err("invalid_request", "Request ID exceeds 128 bytes"));
    }
    Ok(s)
}
fn provider_id(s: &str) -> Result<()> {
    if s.is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
        Err(err(
            "invalid_google_response",
            "Provider identity is invalid",
        ))
    } else {
        Ok(())
    }
}
fn error_code(s: String) -> Result<String> {
    if s.is_empty() || s.len() > 128 || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        Err(err(
            "invalid_request",
            "Only a bounded error code may be retained",
        ))
    } else {
        Ok(s)
    }
}
fn now() -> Result<(u64, u128)> {
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| invalid())?;
    Ok((time.as_secs(), time.as_nanos()))
}
fn cancelled(c: &CancellationToken) -> Result<()> {
    if c.is_cancelled() {
        Err(err("cancelled", "Submission preparation was cancelled"))
    } else {
        Ok(())
    }
}
fn invalid() -> crate::CoreError {
    err(
        "storage_error",
        "Submission metadata or proof is inconsistent",
    )
}
fn not_found() -> crate::CoreError {
    err("not_found", "Submission is outside this account")
}

#[cfg(test)]
mod tests;
