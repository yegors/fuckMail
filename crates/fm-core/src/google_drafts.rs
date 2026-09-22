//! Locally authored Google draft twins. The writer handles bounded metadata;
//! MIME construction, verification and fetched RAW retention run off the writer.
use crate::{
    composition, content::BlobStore, err, fields, id, local_content, string, Result, Store,
};
use fm_google::transport::CancellationToken;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
mod fidelity;

pub const MAX_GOOGLE_DRAFT_BYTES: u64 = 35_000_000;
const MAX_METADATA: usize = 32 * 1024;
const DUE_SQL: &str = "SELECT draft_id,revision FROM drafts WHERE account=?1 AND discarded=0 AND origin='local' AND COALESCE(json_extract(google_json,'$.submission.blocks_drafts'),0)=0 AND COALESCE(json_extract(google_json,'$.deletion.blocks_target'),0)=0 AND (google_json IS NULL OR (json_extract(google_json,'$.state') IN ('local','synced','adopted','prepared','acknowledged') AND (json_extract(google_json,'$.state') IN ('prepared','acknowledged') OR revision>COALESCE(json_extract(google_json,'$.baseline.draft_revision'),json_extract(google_json,'$.adoption.draft_revision'),0))) OR (json_extract(google_json,'$.state')='failed' AND revision>COALESCE(json_extract(google_json,'$.preparation_failure.draft_revision'),json_extract(google_json,'$.failed_draft_revision')))) AND (revision!=COALESCE(json_extract(google_json,'$.retry.draft_revision'),0) OR COALESCE(json_extract(google_json,'$.retry.until_ms'),0)<=?3) ORDER BY updated_at,draft_id LIMIT ?2";

#[derive(Clone)]
pub struct GoogleDraftScope {
    pub account: String,
    pub connection_generation: i64,
    pub primary_email: String,
}
pub enum GoogleDraftStep {
    Reply(Value),
    Prepare(GoogleDraftPlan),
}
pub enum GoogleDraftIntent {
    Reply(Value),
    Upload { provider_draft_id: Option<String> },
}
pub struct GoogleDraftPlan(Box<GoogleDraftPlanContents>);
struct GoogleDraftPlanContents {
    scope: GoogleDraftScope,
    params: Value,
    retained: local_content::RetainedDraft,
    google_revision: i64,
    target: Option<String>,
    thread: Option<String>,
    expected_remote: Option<Observation>,
}
impl GoogleDraftPlan {
    pub fn provider_draft_id(&self) -> Option<&str> {
        self.0.target.as_deref()
    }
}
pub struct PreparedGoogleDraft {
    plan: Box<GoogleDraftPlanContents>,
    hash: String,
    size: u64,
    cancel: CancellationToken,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct RemoteIds {
    draft_id: String,
    message_id: String,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Observation {
    draft_id: String,
    message_id: String,
    sha256: String,
    size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    thread_id: Option<String>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Baseline {
    draft_id: String,
    message_id: String,
    observed_sha256: String,
    upload_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content_matches: Option<bool>,
    draft_revision: i64,
    size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    thread_id: Option<String>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Conflict {
    reason: String,
    observed: Option<Observation>,
}
#[derive(Clone, Serialize, Deserialize)]
struct PreparationFailure {
    draft_revision: i64,
    code: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Retry {
    draft_revision: i64,
    until_ms: i64,
    code: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Twin {
    version: u32,
    revision: i64,
    state: String,
    operation_id: Option<String>,
    baseline: Option<Baseline>,
    conflict: Option<Conflict>,
    #[serde(default)]
    preparation_failure: Option<PreparationFailure>,
    #[serde(default)]
    retry: Option<Retry>,
    #[serde(default)]
    failed_draft_revision: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    submission: Option<crate::google_send::SendGuard>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    adoption: Option<crate::google_draft_actions::Adoption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    derivation: Option<crate::google_draft_actions::Derivation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deletion: Option<crate::google_draft_actions::DeletionGuard>,
}
impl Default for Twin {
    fn default() -> Self {
        Self {
            version: 1,
            revision: 0,
            state: "local".into(),
            operation_id: None,
            baseline: None,
            conflict: None,
            preparation_failure: None,
            retry: None,
            failed_draft_revision: None,
            submission: None,
            adoption: None,
            derivation: None,
            deletion: None,
        }
    }
}
fn effective_observation(twin: &Twin) -> Option<Observation> {
    twin.baseline
        .as_ref()
        .map(|b| Observation {
            draft_id: b.draft_id.clone(),
            message_id: b.message_id.clone(),
            sha256: b.observed_sha256.clone(),
            size: b.size,
            thread_id: b.thread_id.clone(),
        })
        .or_else(|| {
            twin.adoption.as_ref().map(|a| Observation {
                draft_id: a.target.provider_draft_id.clone(),
                message_id: a.target.message_id.clone(),
                sha256: a.target.sha256.clone(),
                size: a.target.size,
                thread_id: Some(a.target.thread_id.clone()),
            })
        })
}
fn effective_target(twin: &Twin) -> Value {
    effective_observation(twin).map(|o|json!({"provider_draft_id":o.draft_id,"message_id":o.message_id,"sha256":o.sha256,"size":o.size,"thread_id":o.thread_id})).unwrap_or(Value::Null)
}
#[derive(Clone, Serialize, Deserialize)]
struct Operation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    version: u32,
    account: String,
    request_id: String,
    draft_id: String,
    params: Value,
    revision: i64,
    state: String,
    draft_revision: i64,
    connection_generation: i64,
    upload_sha256: String,
    size: u64,
    from: String,
    thread_id: Option<String>,
    target: Option<String>,
    ack: Option<RemoteIds>,
    error: Option<String>,
    observed: Option<Observation>,
    expected_remote: Option<Observation>,
}

/// The lease is an unforgeable identity/revision fence. Cloning it cannot claim
/// another attempt; a repeated late outcome must match the durable winner.
#[derive(Clone)]
pub struct GoogleDraftLease {
    blobs: BlobStore,
    operation: Operation,
}
#[derive(Clone)]
pub struct GoogleDraftUploadPlan {
    blobs: BlobStore,
    hash: String,
    size: u64,
}
impl GoogleDraftLease {
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
        &self.operation.from
    }
    pub fn provider_draft_id(&self) -> Option<&str> {
        self.operation.target.as_deref()
    }
    pub fn thread_id(&self) -> Option<&str> {
        self.operation.thread_id.as_deref()
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
    pub fn take_upload(&self) -> GoogleDraftUploadPlan {
        GoogleDraftUploadPlan {
            blobs: self.blobs.clone(),
            hash: self.operation.upload_sha256.clone(),
            size: self.operation.size,
        }
    }
}
pub enum GoogleDraftOutcome {
    Acknowledged {
        draft_id: String,
        message_id: String,
    },
    Undispatched {
        code: String,
    },
    Unknown {
        code: String,
    },
    Rejected {
        code: String,
    },
}
pub struct GoogleDraftRemote {
    pub draft_id: String,
    pub message_id: String,
    pub raw: Vec<u8>,
    pub thread_id: String,
}
pub struct GoogleDraftObservationPlan {
    blobs: BlobStore,
    scope: GoogleDraftScope,
    draft_id: String,
    google_revision: i64,
    operation: Option<Operation>,
    target: String,
    adopted_message: Option<String>,
}
impl GoogleDraftObservationPlan {
    pub fn provider_draft_id(&self) -> Option<&str> {
        Some(&self.target)
    }
}
pub struct PreparedGoogleDraftObservation {
    plan: GoogleDraftObservationPlan,
    observation: Option<Observation>,
    content_matches: Option<bool>,
    cancel: CancellationToken,
    retained_source: Option<crate::google_draft_inventory::PreparedGoogleDraftImport>,
}
pub struct GoogleDraftDue {
    pub draft_id: String,
    pub revision: i64,
    pub state: String,
    pub request_id: Option<String>,
    pub operation_revision: Option<i64>,
    pub request: Option<Value>,
}

pub(crate) fn check_execution_kind(
    db: &Connection,
    account: &str,
    request: &str,
    expected: &str,
) -> Result<()> {
    let kind: Option<String> = db
        .query_row(
            "SELECT execution_kind FROM capture_operations WHERE account=?1 AND id=?2",
            [account, request],
            |r| r.get(0),
        )
        .optional()?;
    if kind.as_deref().is_some_and(|kind| kind != expected) {
        return Err(err(
            "request_id_conflict",
            "Request ID belongs to another execution kind",
        ));
    }
    Ok(())
}

impl Store {
    pub fn google_draft_intent(
        &self,
        scope: &GoogleDraftScope,
        p: &Value,
    ) -> Result<GoogleDraftIntent> {
        fields(
            p,
            &[
                "draft_id",
                "expected_revision",
                "request_id",
                "expected_google_revision",
                "action",
            ],
        )?;
        let draft_id = string(p, "draft_id")?;
        let request = request_id(p)?;
        let revision = positive(p, "expected_revision")?;
        let resolving =
            match p.get("action") {
                None if p.get("expected_google_revision").is_none() => false,
                Some(Value::String(action)) if action == "replace_local" => {
                    positive(p, "expected_google_revision")?;
                    true
                }
                _ => return Err(err(
                    "invalid_request",
                    "Conflict replacement requires action replace_local and its Google revision",
                )),
            };
        if let Some(op) = self.google_draft_operation(&scope.account, request)? {
            if op.params != *p {
                return Err(err(
                    "request_id_conflict",
                    "Request ID already freezes another draft revision",
                ));
            }
            return if op.state == "prepared"
                && self.account_lifecycle(&scope.account)?["state"] == "active"
            {
                self.draft_scope(scope)?;
                self.check_google_send_draft_write(&scope.account, draft_id)?;
                Ok(GoogleDraftIntent::Upload {
                    provider_draft_id: op.target,
                })
            } else {
                Ok(GoogleDraftIntent::Reply(self.google_operation_reply(&op)?))
            };
        }
        self.draft_scope(scope)?;
        self.check_google_send_draft_write(&scope.account, draft_id)?;
        let (current_revision, twin) = self.google_twin(&scope.account, draft_id)?;
        if current_revision != revision {
            return Err(err("stale_revision", "Local draft revision changed"));
        }
        if resolving {
            if twin.revision != positive(p, "expected_google_revision")? || twin.state != "conflict"
            {
                return Err(err(
                    "stale_google_revision",
                    "Google conflict changed; inspect it before replacing",
                ));
            }
            let previous = twin
                .operation_id
                .as_deref()
                .map(|id| self.google_draft_operation(&scope.account, id))
                .transpose()?
                .flatten();
            if let Some(op) = previous.filter(|op| op.state == "acceptance_unknown") {
                let observed = twin.conflict.as_ref().and_then(|c| c.observed.as_ref());
                if op
                    .target
                    .as_deref()
                    .zip(observed.map(|o| o.draft_id.as_str()))
                    .is_none_or(|(target, observed)| target != observed)
                {
                    return Err(err("reconciliation_unavailable", "Unknown acceptance requires a freshly observed existing target; creation cannot be retried"));
                }
            }
        } else if twin.state == "conflict" {
            return Err(err(
                "google_draft_conflict",
                "Inspect the changed Google draft before replacing it",
            ));
        }
        if matches!(
            twin.state.as_str(),
            "prepared" | "in_flight" | "acceptance_unknown" | "acknowledged"
        ) {
            return Err(err(
                "google_draft_pending",
                "The previous immutable draft operation must finish or be inspected first",
            ));
        }
        let (from,discarded): (String,bool) = self.db.query_row("SELECT json_extract(routing_json,'$.from'),discarded FROM drafts WHERE account=?1 AND draft_id=?2",params![scope.account,draft_id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        same_from(&from, &scope.primary_email)?;
        if discarded {
            return Err(err("invalid_draft", "Discarded draft cannot be uploaded"));
        }
        if !resolving
            && ((twin.state == "synced"
                && twin
                    .baseline
                    .as_ref()
                    .is_some_and(|b| b.draft_revision == revision))
                || (twin.state == "adopted"
                    && twin
                        .adoption
                        .as_ref()
                        .is_some_and(|a| a.draft_revision == revision)))
        {
            return Ok(GoogleDraftIntent::Reply(
                self.google_draft_status(&scope.account, draft_id)?,
            ));
        }
        let target = if resolving {
            twin.conflict
                .as_ref()
                .and_then(|c| c.observed.as_ref())
                .map(|o| o.draft_id.clone())
        } else {
            effective_observation(&twin).map(|b| b.draft_id)
        };
        Ok(GoogleDraftIntent::Upload {
            provider_draft_id: target,
        })
    }

    pub fn google_draft_begin(
        &self,
        scope: &GoogleDraftScope,
        p: &Value,
    ) -> Result<GoogleDraftStep> {
        let target = match self.google_draft_intent(scope, p)? {
            GoogleDraftIntent::Reply(reply) => return Ok(GoogleDraftStep::Reply(reply)),
            GoogleDraftIntent::Upload { provider_draft_id } => provider_draft_id,
        };
        if let Some(op) = self.google_draft_operation(&scope.account, request_id(p)?)? {
            return Ok(GoogleDraftStep::Reply(self.google_operation_reply(&op)?));
        }
        let draft_id = string(p, "draft_id")?;
        let (_, twin) = self.google_twin(&scope.account, draft_id)?;
        let retained = self.retained_draft(
            &scope.account,
            json!({"draft_id":draft_id,"expected_revision":positive(p,"expected_revision")?}),
        )?;
        let expected_remote = if p.get("action").is_some() {
            twin.conflict.as_ref().and_then(|c| c.observed.clone())
        } else {
            effective_observation(&twin)
        };
        let source = retained.draft()["source_message_id"].as_str();
        let thread = if retained.draft()["mode"] == "reply"
            || retained.draft()["mode"] == "reply_all"
        {
            self.db.query_row("SELECT remote_thread_id FROM google_message_state WHERE account=?1 AND message_id=?2 AND deleted=0", params![scope.account,source], |r|r.get::<_,String>(0)).optional()?
        } else {
            twin.adoption.as_ref().map(|a| a.target.thread_id.clone())
        };
        Ok(GoogleDraftStep::Prepare(GoogleDraftPlan(Box::new(
            GoogleDraftPlanContents {
                scope: scope.clone(),
                params: p.clone(),
                retained,
                google_revision: twin.revision,
                target,
                thread,
                expected_remote,
            },
        ))))
    }

    pub fn google_draft_finish(&mut self, proof: PreparedGoogleDraft) -> Result<Value> {
        cancelled(&proof.cancel)?;
        let plan = proof.plan;
        let request = request_id(&plan.params)?;
        if let Some(op) = self.google_draft_operation(&plan.scope.account, request)? {
            if op.params != plan.params {
                return Err(err(
                    "request_id_conflict",
                    "Request ID already freezes another draft revision",
                ));
            }
            return self.google_operation_reply(&op);
        }
        self.draft_scope(&plan.scope)?;
        self.check_retained_draft(&plan.retained)?;
        let draft_id = string(&plan.params, "draft_id")?;
        self.check_google_send_draft_write(&plan.scope.account, draft_id)?;
        let (_, mut twin) = self.google_twin(&plan.scope.account, draft_id)?;
        if twin.revision != plan.google_revision {
            return Err(err(
                "stale_google_revision",
                "Google draft changed during MIME preparation",
            ));
        }
        let op = Operation {
            kind: Some("draft".into()),
            version: 1,
            account: plan.scope.account.clone(),
            request_id: request.into(),
            draft_id: draft_id.into(),
            params: plan.params.clone(),
            revision: 1,
            state: "prepared".into(),
            draft_revision: positive(&plan.params, "expected_revision")?,
            connection_generation: plan.scope.connection_generation,
            upload_sha256: proof.hash,
            size: proof.size,
            from: plan.retained.draft()["from"]
                .as_str()
                .ok_or_else(invalid)?
                .into(),
            thread_id: plan.thread,
            target: plan.target,
            ack: None,
            error: None,
            observed: None,
            expected_remote: plan.expected_remote,
        };
        twin.operation_id = Some(op.request_id.clone());
        twin.state = op.state.clone();
        twin.conflict = None;
        twin.preparation_failure = None;
        twin.retry = None;
        twin.failed_draft_revision = None;
        self.persist_google_draft(&op, Some(&mut twin), true)?;
        self.google_operation_reply(&op)
    }

    pub fn google_draft_claim(
        &mut self,
        scope: &GoogleDraftScope,
        request: &str,
        expected_operation_revision: i64,
    ) -> Result<GoogleDraftLease> {
        self.draft_scope(scope)?;
        let mut op = self
            .google_draft_operation(&scope.account, request)?
            .ok_or_else(|| err("not_found", "Google draft operation is unavailable"))?;
        if op.revision != expected_operation_revision || op.state != "prepared" {
            return Err(err(
                "stale_google_work",
                "Only the current known-unsent operation can be claimed",
            ));
        }
        same_from(&op.from, &scope.primary_email)?;
        self.check_google_send_draft_write(&scope.account, &op.draft_id)?;
        let (_, mut twin) = self.google_twin(&scope.account, &op.draft_id)?;
        let discarded: bool = self.db.query_row(
            "SELECT discarded FROM drafts WHERE account=?1 AND draft_id=?2",
            params![scope.account, op.draft_id],
            |r| r.get(0),
        )?;
        if discarded {
            return Err(err("invalid_draft", "Discarded draft cannot be uploaded"));
        }
        if twin.operation_id.as_deref() != Some(request) || twin.state != "prepared" {
            return Err(err("stale_google_work", "Draft operation was superseded"));
        }
        // A known-unsent operation can survive reconnect; its frozen identity is
        // checked against the newly verified primary account before dispatch.
        op.connection_generation = scope.connection_generation;
        op.revision += 1;
        op.state = "in_flight".into();
        op.error = None;
        twin.state = op.state.clone();
        self.persist_google_draft(&op, Some(&mut twin), false)?;
        Ok(GoogleDraftLease {
            blobs: self.google_blob_store(),
            operation: op,
        })
    }

    pub fn google_draft_outcome(
        &mut self,
        lease: &GoogleDraftLease,
        outcome: GoogleDraftOutcome,
    ) -> Result<Value> {
        if lease.blobs.profile != self.profile {
            return Err(err(
                "scope_mismatch",
                "Draft lease belongs to another profile",
            ));
        }
        let mut op = self
            .google_draft_operation(lease.account(), lease.request_id())?
            .ok_or_else(invalid)?;
        let (state, ack, error) = match outcome {
            GoogleDraftOutcome::Acknowledged {
                draft_id,
                message_id,
            } => {
                provider_id(&draft_id)?;
                provider_id(&message_id)?;
                (
                    "acknowledged",
                    Some(RemoteIds {
                        draft_id,
                        message_id,
                    }),
                    None,
                )
            }
            GoogleDraftOutcome::Undispatched { code } => {
                ("prepared", None, Some(error_code(code)?))
            }
            GoogleDraftOutcome::Unknown { code } => {
                ("acceptance_unknown", None, Some(error_code(code)?))
            }
            GoogleDraftOutcome::Rejected { code } => ("failed", None, Some(error_code(code)?)),
        };
        if op.revision != lease.operation.revision || op.state != "in_flight" {
            if op.revision == lease.operation.revision + 1
                && op.state == state
                && op.ack == ack
                && op.error == error
            {
                return self.google_operation_reply(&op);
            }
            return Err(err(
                "stale_google_work",
                "Draft outcome does not match the active attempt",
            ));
        }
        op.revision += 1;
        op.state = state.into();
        op.ack = ack;
        op.error = error;
        let (_, mut twin) = self.google_twin(&op.account, &op.draft_id)?;
        let current = twin.operation_id.as_deref() == Some(&op.request_id);
        if current {
            twin.state = op.state.clone();
            if op.state == "failed" {
                twin.failed_draft_revision = Some(op.draft_revision);
            }
        }
        // An exact late transport result is durable even after disconnect. Only
        // a separately generation-fenced RAW observation can publish a baseline.
        self.persist_google_draft(&op, if current { Some(&mut twin) } else { None }, false)?;
        self.google_operation_reply(&op)
    }

    pub fn google_draft_status(&self, account: &str, draft_id: &str) -> Result<Value> {
        let (local_revision, twin) = self.google_twin(account, draft_id)?;
        let operation = twin
            .operation_id
            .as_deref()
            .map(|id| self.google_draft_operation(account, id))
            .transpose()?
            .flatten();
        Ok(
            json!({"account":account,"draft_id":draft_id,"state":twin.state,"revision":twin.revision,"local_revision":local_revision,"operation_is_current":true,"preparation_failure":twin.preparation_failure,"retry":twin.retry,
            "operation":operation.as_ref().map(operation_status),"fidelity_warning":twin.baseline.as_ref().is_some_and(|b|b.content_matches == Some(false)),"effective_target":effective_target(&twin),"baseline":twin.baseline,"conflict":twin.conflict,"adoption":twin.adoption,"derivation":twin.derivation,"deletion":twin.deletion}),
        )
    }

    pub fn google_draft_observe_begin(
        &self,
        scope: &GoogleDraftScope,
        draft_id: &str,
    ) -> Result<GoogleDraftObservationPlan> {
        self.draft_scope(scope)?;
        self.check_google_send_draft_observation(&scope.account, draft_id)?;
        let (_, twin) = self.google_twin(&scope.account, draft_id)?;
        let operation = twin
            .operation_id
            .as_deref()
            .map(|id| self.google_draft_operation(&scope.account, id))
            .transpose()?
            .flatten();
        if operation.as_ref().is_some_and(|op| op.state == "in_flight") {
            return Err(err(
                "google_draft_pending",
                "An active draft write cannot be reconciled",
            ));
        }
        let target = operation.as_ref().and_then(|op|op.ack.as_ref().map(|ack|ack.draft_id.clone()).or_else(||op.target.clone()))
            .or_else(||effective_observation(&twin).map(|b|b.draft_id))
            .ok_or_else(||err("reconciliation_unavailable", "No acknowledged Google draft identity is available; unknown creation cannot be retried"))?;
        Ok(GoogleDraftObservationPlan {
            blobs: self.google_blob_store(),
            scope: scope.clone(),
            draft_id: draft_id.into(),
            google_revision: twin.revision,
            operation,
            target,
            adopted_message: twin.adoption.as_ref().map(|a| a.target.message_id.clone()),
        })
    }

    pub fn google_draft_observe_finish(
        &mut self,
        proof: PreparedGoogleDraftObservation,
    ) -> Result<Value> {
        cancelled(&proof.cancel)?;
        let plan = proof.plan;
        if plan.blobs.profile != self.profile {
            return Err(err(
                "scope_mismatch",
                "RAW observation belongs to another profile",
            ));
        }
        self.draft_scope(&plan.scope)?;
        self.check_google_send_draft_observation(&plan.scope.account, &plan.draft_id)?;
        let (_, mut twin) = self.google_twin(&plan.scope.account, &plan.draft_id)?;
        if twin.revision != plan.google_revision {
            return Err(err(
                "stale_google_work",
                "Google draft changed during RAW observation",
            ));
        }
        if let Some(source) = proof.retained_source {
            self.retain_action_source(source)?;
        }
        let mut operation = plan.operation;
        let observation = proof.observation;
        if twin.state == "conflict"
            && twin
                .conflict
                .as_ref()
                .is_some_and(|c| c.observed == observation)
        {
            return self.google_draft_status(&plan.scope.account, &plan.draft_id);
        }
        let mut baseline = twin.baseline.clone();
        if twin.state == "failed"
            && baseline
                .as_ref()
                .zip(observation.as_ref())
                .is_some_and(|(old, row)| {
                    old.draft_id == row.draft_id
                        && old.message_id == row.message_id
                        && old.observed_sha256 == row.sha256
                })
        {
            return self.google_draft_status(&plan.scope.account, &plan.draft_id);
        }
        let conflict = if let Some(op) = operation.as_ref().filter(|op| op.state == "prepared") {
            if same_observation(op.expected_remote.as_ref(), observation.as_ref()) {
                return self.google_draft_status(&plan.scope.account, &plan.draft_id);
            }
            Some(Conflict {
                reason: "prepared_remote_changed".into(),
                observed: observation.clone(),
            })
        } else if twin.state == "conflict" {
            Some(Conflict {
                reason: "remote_changed_again".into(),
                observed: observation.clone(),
            })
        } else if let Some(op) = operation.as_ref().filter(|op| op.state == "acknowledged") {
            if op.connection_generation != plan.scope.connection_generation {
                Some(Conflict {
                    reason: "connection_changed".into(),
                    observed: observation.clone(),
                })
            } else {
                match (&op.ack, &observation) {
                    (Some(ack), Some(row))
                        if row.draft_id == ack.draft_id && row.message_id == ack.message_id =>
                    {
                        baseline = Some(Baseline {
                            draft_id: row.draft_id.clone(),
                            message_id: row.message_id.clone(),
                            observed_sha256: row.sha256.clone(),
                            upload_sha256: op.upload_sha256.clone(),
                            content_matches: proof.content_matches,
                            draft_revision: op.draft_revision,
                            size: row.size,
                            thread_id: row.thread_id.clone(),
                        });
                        None
                    }
                    (_, None) => Some(Conflict {
                        reason: "missing".into(),
                        observed: None,
                    }),
                    _ => Some(Conflict {
                        reason: "ack_identity_changed".into(),
                        observed: observation.clone(),
                    }),
                }
            }
        } else if operation
            .as_ref()
            .is_some_and(|op| op.state == "acceptance_unknown")
        {
            Some(Conflict {
                reason: "acceptance_unknown".into(),
                observed: observation.clone(),
            })
        } else if baseline.is_none() && twin.adoption.is_some() {
            if same_observation(effective_observation(&twin).as_ref(), observation.as_ref()) {
                None
            } else {
                Some(Conflict {
                    reason: if observation.is_none() {
                        "missing"
                    } else {
                        "adopted_remote_changed"
                    }
                    .into(),
                    observed: observation.clone(),
                })
            }
        } else {
            match (&baseline, &observation) {
                (Some(old), Some(row))
                    if old.draft_id == row.draft_id
                        && old.message_id == row.message_id
                        && old.observed_sha256 == row.sha256
                        && old
                            .thread_id
                            .as_ref()
                            .is_none_or(|t| Some(t) == row.thread_id.as_ref()) =>
                {
                    None
                }
                (_, None) => Some(Conflict {
                    reason: "missing".into(),
                    observed: None,
                }),
                (Some(old), Some(row))
                    if old.draft_id != row.draft_id || old.message_id != row.message_id =>
                {
                    Some(Conflict {
                        reason: "identity_changed".into(),
                        observed: observation.clone(),
                    })
                }
                _ => Some(Conflict {
                    reason: "raw_changed".into(),
                    observed: observation.clone(),
                }),
            }
        };
        if conflict.is_none() {
            if let (Some(old), Some(op), Some(matches)) =
                (&mut baseline, &operation, proof.content_matches)
            {
                if old.upload_sha256 == op.upload_sha256 {
                    old.content_matches = Some(matches);
                }
            }
        }
        let state = if conflict.is_some() {
            "conflict"
        } else if baseline.is_none() && twin.adoption.is_some() {
            "adopted"
        } else {
            "synced"
        };
        if twin.state == state && twin.baseline == baseline && twin.conflict == conflict {
            return self.google_draft_status(&plan.scope.account, &plan.draft_id);
        }
        twin.state = state.into();
        twin.baseline = baseline;
        twin.conflict = conflict;
        if let Some(op) = &mut operation {
            if op.observed.is_none() {
                op.observed = observation;
            }
            if matches!(op.state.as_str(), "acknowledged" | "prepared") {
                op.state = state.into();
                op.revision += 1;
            }
            self.persist_google_draft(op, Some(&mut twin), false)?;
        } else if twin.adoption.is_some() {
            self.persist_google_twin(&plan.scope.account, &plan.draft_id, &mut twin)?;
        } else {
            return Err(invalid());
        }
        self.google_draft_status(&plan.scope.account, &plan.draft_id)
    }

    /// Record only deterministic local preparation failure for this exact
    /// authored/twin/connection version. This does not manufacture an upload.
    pub fn google_draft_prepare_failed(
        &mut self,
        scope: &GoogleDraftScope,
        request: &Value,
        expected_google_revision: i64,
        code: &str,
    ) -> Result<Value> {
        if !matches!(
            code,
            "invalid_draft" | "invalid_identity" | "identity_mismatch" | "limit_exceeded"
        ) {
            return Err(err(
                "invalid_request",
                "Transient errors cannot permanently fail draft preparation",
            ));
        }
        self.draft_connection(&scope.account, scope.connection_generation)?;
        let draft_id = string(request, "draft_id")?;
        let expected_revision = positive(request, "expected_revision")?;
        let (revision, mut twin) = self.google_twin(&scope.account, draft_id)?;
        let conflict_replacement = twin.state == "conflict"
            && request["action"] == "replace_local"
            && request["expected_google_revision"].as_i64() == Some(expected_google_revision);
        if revision != expected_revision
            || twin.revision != expected_google_revision
            || (!matches!(twin.state.as_str(), "local" | "synced" | "failed")
                && !conflict_replacement)
        {
            return Err(err(
                "stale_google_work",
                "Draft changed before preparation failure was recorded",
            ));
        }
        if let Some(request_id) = request.get("request_id") {
            let request_id = request_id.as_str().ok_or_else(invalid)?;
            if self
                .google_draft_operation(&scope.account, request_id)?
                .is_some()
            {
                return Err(err(
                    "stale_google_work",
                    "An existing upload cannot become a preparation failure",
                ));
            }
        }
        if !conflict_replacement {
            twin.state = "failed".into();
        }
        twin.preparation_failure = Some(PreparationFailure {
            draft_revision: revision,
            code: code.into(),
        });
        twin.retry = None;
        self.persist_google_twin(&scope.account, draft_id, &mut twin)?;
        self.google_draft_status(&scope.account, draft_id)
    }

    /// Retryable per-draft local I/O must not occupy the oldest due position
    /// forever. Deferral does not alter a frozen operation or claim acceptance.
    pub fn google_draft_defer(
        &mut self,
        scope: &GoogleDraftScope,
        request: &Value,
        expected_google_revision: i64,
        code: &str,
        delay_seconds: u32,
    ) -> Result<Value> {
        if !(1..=300).contains(&delay_seconds)
            || !matches!(
                code,
                "content_unavailable"
                    | "storage_error"
                    | "invalid_draft"
                    | "limit_exceeded"
                    | "invalid_identity"
                    | "identity_mismatch"
            )
        {
            return Err(err(
                "invalid_request",
                "Only bounded local preparation deferrals are supported",
            ));
        }
        self.draft_connection(&scope.account, scope.connection_generation)?;
        let draft_id = string(request, "draft_id")?;
        let expected_revision = positive(request, "expected_revision")?;
        let (revision, mut twin) = self.google_twin(&scope.account, draft_id)?;
        if revision != expected_revision
            || twin.revision != expected_google_revision
            || !matches!(
                twin.state.as_str(),
                "local" | "synced" | "failed" | "prepared" | "acknowledged"
            )
        {
            return Err(err(
                "stale_google_work",
                "Draft changed before retry deferral was recorded",
            ));
        }
        let until_ms = unix_ms()?
            .checked_add(i64::from(delay_seconds) * 1000)
            .ok_or_else(invalid)?;
        twin.retry = Some(Retry {
            draft_revision: revision,
            until_ms,
            code: code.into(),
        });
        self.persist_google_twin(&scope.account, draft_id, &mut twin)?;
        self.google_draft_status(&scope.account, draft_id)
    }

    fn persist_google_twin(
        &mut self,
        account: &str,
        draft_id: &str,
        twin: &mut Twin,
    ) -> Result<()> {
        self.check_draft_writable(account, &json!({"draft_id":draft_id}))?;
        twin.revision = twin.revision.checked_add(1).ok_or_else(invalid)?;
        let raw = encode(twin)?;
        let tx = self.db.transaction()?;
        if tx.execute(
            "UPDATE drafts SET google_json=?3 WHERE account=?1 AND draft_id=?2",
            params![account, draft_id, raw],
        )? != 1
        {
            return Err(invalid());
        }
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(())
    }

    pub fn google_draft_due(
        &self,
        account: &str,
        generation: i64,
        limit: usize,
    ) -> Result<Vec<GoogleDraftDue>> {
        if !(1..=100).contains(&limit) {
            return Err(err(
                "invalid_request",
                "Draft due limit must be between 1 and 100",
            ));
        }
        self.google_draft_due_at(account, generation, limit, unix_ms()?)
    }

    fn google_draft_due_at(
        &self,
        account: &str,
        generation: i64,
        limit: usize,
        now_ms: i64,
    ) -> Result<Vec<GoogleDraftDue>> {
        self.draft_connection(account, generation)?;
        let mut stmt = self.db.prepare(DUE_SQL)?;
        let rows = stmt
            .query_map(params![account, limit as i64, now_ms], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(draft_id, revision)| {
                let status = self.google_draft_status(account, &draft_id)?;
                let resumable = status["state"] == "prepared" || status["state"] == "acknowledged";
                let request = if resumable {
                    self.google_draft_operation(
                        account,
                        status["operation"]["request_id"]
                            .as_str()
                            .ok_or_else(invalid)?,
                    )?
                    .map(|op| op.params)
                } else {
                    None
                };
                Ok(GoogleDraftDue {
                    draft_id,
                    revision: if resumable {
                        status["operation"]["draft_revision"]
                            .as_i64()
                            .ok_or_else(invalid)?
                    } else {
                        revision
                    },
                    state: status["state"].as_str().ok_or_else(invalid)?.into(),
                    request_id: if resumable {
                        status["operation"]["request_id"]
                            .as_str()
                            .map(str::to_owned)
                    } else {
                        None
                    },
                    operation_revision: if resumable {
                        status["operation"]["revision"].as_i64()
                    } else {
                        None
                    },
                    request,
                })
            })
            .collect()
    }

    pub(crate) fn recover_google_drafts(&mut self) -> Result<()> {
        let rows = {
            let mut stmt = self.db.prepare("SELECT account,id FROM capture_operations WHERE execution_kind='google' AND COALESCE(json_extract(json,'$.kind'),'draft')='draft' AND json_extract(json,'$.state')='in_flight'")?;
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        for (account, request) in rows {
            let mut op = self
                .google_draft_operation(&account, &request)?
                .ok_or_else(invalid)?;
            op.state = "acceptance_unknown".into();
            op.error = Some("engine_interrupted".into());
            op.revision += 1;
            let (_, mut twin) = self.google_twin(&account, &op.draft_id)?;
            let current = twin.operation_id.as_deref() == Some(&request);
            if current {
                twin.state = op.state.clone();
            }
            self.persist_google_draft(&op, if current { Some(&mut twin) } else { None }, false)?;
        }
        Ok(())
    }

    pub(crate) fn draft_connection(&self, account: &str, generation: i64) -> Result<()> {
        self.require_active_account(account)?;
        id(account)?;
        if self.profile_kind()? != crate::live_profile::ProfileKind::Live {
            return Err(err(
                "profile_kind_mismatch",
                "Google drafts require a live profile",
            ));
        }
        if !self.google_account_bindings()?.iter().any(|b| {
            b.account == account
                && b.connection_generation == generation
                && b.state == crate::live_profile::ConnectionState::Connected
                && fm_google::auth::ScopeSet::new(&b.scopes).mail_write()
        }) {
            return Err(err(
                "stale_google_work",
                "Google account connection or mail permission changed",
            ));
        }
        Ok(())
    }
    pub(crate) fn draft_scope(&self, scope: &GoogleDraftScope) -> Result<()> {
        self.draft_connection(&scope.account, scope.connection_generation)?;
        let from: String = self.db.query_row(
            "SELECT email FROM accounts WHERE id=?1",
            [&scope.account],
            |r| r.get(0),
        )?;
        same_from(&from, &scope.primary_email)
    }
    fn google_twin(&self, account: &str, draft_id: &str) -> Result<(i64, Twin)> {
        id(account)?;
        id(draft_id)?;
        let row: Option<(i64, Option<String>)> = self
            .db
            .query_row(
                "SELECT revision,google_json FROM drafts WHERE account=?1 AND draft_id=?2",
                params![account, draft_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (revision, raw) =
            row.ok_or_else(|| err("not_found", "Draft is outside this account"))?;
        let twin: Twin = match raw {
            None => Twin::default(),
            Some(raw) => decode(&raw)?,
        };
        if twin.version != 1 || twin.revision < 0 {
            return Err(invalid());
        }
        Ok((revision, twin))
    }
    fn google_draft_operation(&self, account: &str, request: &str) -> Result<Option<Operation>> {
        check_google_operation_kind(&self.db, account, request, "draft")?;
        let row: Option<(String,String)> = self.db.query_row("SELECT json,blob_hash FROM capture_operations WHERE execution_kind='google' AND COALESCE(json_extract(json,'$.kind'),'draft')='draft' AND account=?1 AND id=?2",params![account,request],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        row.map(|(raw, hash)| {
            let op: Operation = decode(&raw)?;
            if op.version != 1
                || op.kind.as_deref().is_some_and(|kind| kind != "draft")
                || op.account != account
                || op.request_id != request
                || op.upload_sha256 != hash
                || op.size > MAX_GOOGLE_DRAFT_BYTES
                || op.revision < 1
            {
                return Err(invalid());
            }
            Ok(op)
        })
        .transpose()
    }
    fn google_operation_reply(&self, op: &Operation) -> Result<Value> {
        let mut status = self.google_draft_status(&op.account, &op.draft_id)?;
        status["operation_is_current"] = json!(status["operation"]["request_id"] == op.request_id);
        status["operation"] = operation_status(op);
        status["state"] = json!(op.state);
        Ok(status)
    }
    fn persist_google_draft(
        &mut self,
        op: &Operation,
        twin: Option<&mut Twin>,
        insert: bool,
    ) -> Result<()> {
        let raw = encode(op)?;
        let twin_raw = twin
            .map(|twin| {
                twin.revision += 1;
                encode(twin)
            })
            .transpose()?;
        let tx = self.db.transaction()?;
        if insert {
            tx.execute("INSERT INTO capture_operations(account,id,json,blob_hash,execution_kind) VALUES(?1,?2,?3,?4,'google')",params![op.account,op.request_id,raw,op.upload_sha256])?;
        } else if tx.execute("UPDATE capture_operations SET json=?3 WHERE execution_kind='google' AND COALESCE(json_extract(json,'$.kind'),'draft')='draft' AND account=?1 AND id=?2",params![op.account,op.request_id,raw])? != 1 { return Err(invalid()); }
        if let Some(twin_raw) = twin_raw {
            if tx.execute(
                "UPDATE drafts SET google_json=?3 WHERE account=?1 AND draft_id=?2",
                params![op.account, op.draft_id, twin_raw],
            )? != 1
            {
                return Err(invalid());
            }
        }
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn google_send_twin_raw(
        &self,
        account: &str,
        draft_id: &str,
        submission: crate::google_send::SendGuard,
    ) -> Result<String> {
        let (_, mut twin) = self.google_twin(account, draft_id)?;
        twin.submission = Some(submission);
        twin.revision += 1;
        encode(&twin)
    }
    pub(crate) fn google_source_twin_raw(
        &self,
        adoption: Option<crate::google_draft_actions::Adoption>,
        derivation: Option<crate::google_draft_actions::Derivation>,
    ) -> Result<String> {
        let mut twin = Twin {
            adoption,
            derivation,
            ..Twin::default()
        };
        if twin.adoption.is_some() {
            twin.state = "adopted".into();
            twin.revision = 1;
        }
        encode(&twin)
    }
    pub(crate) fn compatible_google_adoption(
        &self,
        account: &str,
        draft: &str,
        target: &crate::google_draft_actions::ObservedTarget,
    ) -> Result<bool> {
        let (_, twin) = self.google_twin(account, draft)?;
        let discarded: bool = self.db.query_row(
            "SELECT discarded FROM drafts WHERE account=?1 AND draft_id=?2",
            params![account, draft],
            |r| r.get(0),
        )?;
        Ok(!discarded
            && matches!(twin.state.as_str(), "adopted" | "synced" | "local")
            && effective_observation(&twin).is_some_and(|o| {
                o.draft_id == target.provider_draft_id
                    && o.message_id == target.message_id
                    && o.sha256 == target.sha256
                    && o.size == target.size
            }))
    }
    pub(crate) fn google_delete_twin_raw(
        &self,
        account: &str,
        draft: &str,
        deletion: crate::google_draft_actions::DeletionGuard,
    ) -> Result<String> {
        let (_, mut twin) = self.google_twin(account, draft)?;
        twin.deletion = Some(deletion);
        twin.revision += 1;
        encode(&twin)
    }
    /// A save that changes no authored field must not turn initial adoption into
    /// an upload. This follows the existing save revision without changing its receipt.
    pub(crate) fn advance_unchanged_adoption(
        &self,
        account: &str,
        draft: &str,
        old: i64,
        next: i64,
    ) -> Result<()> {
        let (_, mut twin) = self.google_twin(account, draft)?;
        if twin.state == "adopted"
            && twin.baseline.is_none()
            && twin
                .adoption
                .as_ref()
                .is_some_and(|a| a.draft_revision == old)
        {
            twin.adoption.as_mut().unwrap().draft_revision = next;
            self.db.execute(
                "UPDATE drafts SET google_json=?3 WHERE account=?1 AND draft_id=?2",
                params![account, draft, encode(&twin)?],
            )?;
        }
        Ok(())
    }
}

pub(crate) fn check_google_operation_kind(
    db: &Connection,
    account: &str,
    request: &str,
    expected: &str,
) -> Result<()> {
    check_execution_kind(db, account, request, "google")?;
    let kind: Option<String> = db.query_row("SELECT COALESCE(json_extract(json,'$.kind'),'draft') FROM capture_operations WHERE account=?1 AND id=?2 AND execution_kind='google'", [account,request], |r|r.get(0)).optional()?;
    if kind.as_deref().is_some_and(|kind| kind != expected) {
        return Err(err(
            "request_id_conflict",
            "Request ID belongs to another Google operation kind",
        ));
    }
    Ok(())
}

pub fn prepare_google_draft(
    plan: GoogleDraftPlan,
    cancel: &CancellationToken,
) -> Result<PreparedGoogleDraft> {
    let plan = plan.0;
    cancelled(cancel)?;
    let parts = plan.retained.prepare(cancel)?;
    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| invalid())?
        .as_secs();
    let digest = format!(
        "{:x}",
        Sha256::digest(format!("{}\0{}", plan.scope.account, request_id(&plan.params)?).as_bytes())
    );
    let (request, identity) = composition::compose_request(
        &plan.scope.account,
        plan.retained.draft(),
        format!("<draft-{digest}@fuckmail.invalid>"),
        created,
    )?;
    let built = fm_mail::build_draft_mail(&request, &identity, &parts)
        .map_err(|_| err("invalid_draft", "Draft cannot form a bounded MIME message"))?;
    let size = built.raw.len() as u64;
    if size > MAX_GOOGLE_DRAFT_BYTES {
        return Err(err(
            "limit_exceeded",
            "Google draft MIME exceeds 35000000 bytes",
        ));
    }
    cancelled(cancel)?;
    let hash = plan.retained.blobs.stage_blob(&built.raw)?;
    cancelled(cancel)?;
    Ok(PreparedGoogleDraft {
        plan,
        hash,
        size,
        cancel: cancel.clone(),
    })
}
pub fn load_google_draft_upload(
    plan: GoogleDraftUploadPlan,
    cancel: &CancellationToken,
) -> Result<Vec<u8>> {
    cancelled(cancel)?;
    if plan.size > MAX_GOOGLE_DRAFT_BYTES {
        return Err(invalid());
    }
    let bytes = local_content::read_blob(&plan.blobs, &plan.hash, plan.size)?;
    cancelled(cancel)?;
    Ok(bytes)
}
pub fn prepare_google_draft_observation(
    plan: GoogleDraftObservationPlan,
    remote: Option<GoogleDraftRemote>,
    cancel: &CancellationToken,
) -> Result<PreparedGoogleDraftObservation> {
    cancelled(cancel)?;
    let observation = remote
        .as_ref()
        .map(|remote| {
            provider_id(&remote.draft_id)?;
            provider_id(&remote.message_id)?;
            provider_id(&remote.thread_id)?;
            if remote.raw.is_empty() || remote.raw.len() as u64 > MAX_GOOGLE_DRAFT_BYTES {
                return Err(err(
                    "limit_exceeded",
                    "Observed Google draft RAW is outside bounds",
                ));
            }
            let sha256 = plan.blobs.stage_blob(&remote.raw)?;
            Ok(Observation {
                draft_id: remote.draft_id.clone(),
                message_id: remote.message_id.clone(),
                thread_id: Some(remote.thread_id.clone()),
                sha256,
                size: remote.raw.len() as u64,
            })
        })
        .transpose()?;
    let content_matches = if let (Some(op), Some(remote)) = (&plan.operation, &remote) {
        let path = plan.blobs.verified_blob(&op.upload_sha256, op.size)?;
        let upload = std::fs::read(path)?;
        fidelity::equivalent(&upload, &remote.raw)
    } else {
        None
    };
    let retained_source = if let Some(message) = &plan.adopted_message {
        Some(crate::google_draft_actions::prepare_adopted_source(
            &plan.blobs,
            &plan.scope,
            &plan.target,
            message,
            remote,
            cancel,
        )?)
    } else {
        None
    };
    cancelled(cancel)?;
    Ok(PreparedGoogleDraftObservation {
        plan,
        observation,
        content_matches,
        cancel: cancel.clone(),
        retained_source,
    })
}
fn same_observation(expected: Option<&Observation>, actual: Option<&Observation>) -> bool {
    match (expected, actual) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.draft_id == b.draft_id
                && a.message_id == b.message_id
                && a.sha256 == b.sha256
                && a.size == b.size
                && a.thread_id
                    .as_ref()
                    .is_none_or(|t| Some(t) == b.thread_id.as_ref())
        }
        _ => false,
    }
}

fn operation_status(op: &Operation) -> Value {
    json!({"request_id":op.request_id,"revision":op.revision,"state":op.state,"draft_revision":op.draft_revision,"connection_generation":op.connection_generation,
        "upload_sha256":op.upload_sha256,"size":op.size,"ack":op.ack,"error":op.error,"provider_draft_id":op.target})
}

pub(crate) fn unix_ms() -> Result<i64> {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| invalid())?
        .as_millis();
    i64::try_from(ms).map_err(|_| invalid())
}
fn positive(p: &Value, key: &str) -> Result<i64> {
    p[key]
        .as_i64()
        .filter(|v| *v > 0)
        .ok_or_else(|| err("invalid_request", "A positive revision is required"))
}
fn request_id(p: &Value) -> Result<&str> {
    let value = string(p, "request_id")?;
    if value.len() > 128 {
        return Err(err("invalid_request", "Request ID exceeds 128 bytes"));
    }
    Ok(value)
}
fn provider_id(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        Err(err(
            "invalid_google_response",
            "Google draft identity is invalid",
        ))
    } else {
        Ok(())
    }
}
pub(crate) fn same_from(a: &str, b: &str) -> Result<()> {
    for value in [a, b] {
        fm_mail::validate_address(&fm_mail::Address {
            name: None,
            email: value.into(),
        })
        .map_err(|_| err("invalid_identity", "Google primary address is invalid"))?;
    }
    if a.trim().to_lowercase() != b.trim().to_lowercase() {
        return Err(err(
            "identity_mismatch",
            "Draft From differs from the verified Google primary address",
        ));
    }
    Ok(())
}
fn error_code(value: String) -> Result<String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(err(
            "invalid_request",
            "Only a bounded error code may be retained",
        ));
    }
    Ok(value)
}
fn encode<T: Serialize>(value: &T) -> Result<String> {
    let raw = serde_json::to_string(value)?;
    if raw.len() > MAX_METADATA {
        return Err(err(
            "limit_exceeded",
            "Google draft metadata exceeds its bound",
        ));
    }
    Ok(raw)
}
fn decode<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T> {
    if raw.len() > MAX_METADATA {
        return Err(invalid());
    }
    Ok(serde_json::from_str(raw)?)
}
fn invalid() -> crate::CoreError {
    err(
        "storage_error",
        "Google draft metadata or content proof is inconsistent",
    )
}
fn cancelled(token: &CancellationToken) -> Result<()> {
    if token.is_cancelled() {
        Err(err("cancelled", "Google draft preparation was cancelled"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
