//! Explicit local diagnostic: durable draft -> MIME -> synthetic capture.
//! There is no network transport and this operation never clears or sends a draft.
use crate::{err, fields, string, Result, Store};
use fm_google::send::{
    submit_once, SendJournal, SendOperation, SendState, SubmissionResult, SubmissionTransport,
};
use fm_google::{AccountId, GoogleError};
use fm_mail::StagedAttachment;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Read;

// Includes the optional 8192-byte subject and its JSON escaping overhead.
const MAX_CAPTURE_RECORD_BYTES: usize = 32 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Accepted,
    Unknown,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct CaptureRecord {
    conversation_id: String,
    draft_revision: i64,
    outcome: Outcome,
    size: u64,
    created_at: u64,
    attempts_started: u32,
    operation: SendOperation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    draft_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    subject: Option<String>,
}

struct CaptureRequest<'a> {
    conversation_id: &'a str,
    draft_id: Option<&'a str>,
    request_id: &'a str,
    expected_revision: i64,
    outcome: Outcome,
}

impl<'a> CaptureRequest<'a> {
    fn parse(value: &'a Value) -> Result<Self> {
        fields(
            value,
            &[
                "conversation_id",
                "draft_id",
                "request_id",
                "expected_revision",
                "outcome",
            ],
        )?;
        let (selected, legacy) = crate::composition::selector(value)?;
        let conversation_id = if legacy { selected } else { "" };
        let draft_id = if legacy { None } else { Some(selected) };
        let request_id = string(value, "request_id")?;
        if request_id.len() > 128 {
            return Err(err(
                "invalid_request",
                "Capture request ID exceeds 128 bytes",
            ));
        }
        let expected_revision = value["expected_revision"]
            .as_i64()
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                err(
                    "invalid_request",
                    "Capture requires a positive draft revision",
                )
            })?;
        let outcome = match value.get("outcome").and_then(Value::as_str) {
            None if value.get("outcome").is_none() => Outcome::Accepted,
            Some("accepted") => Outcome::Accepted,
            Some("unknown") => Outcome::Unknown,
            _ => {
                return Err(err(
                    "invalid_request",
                    "Synthetic capture outcome must be accepted or unknown",
                ))
            }
        };
        Ok(Self {
            conversation_id,
            draft_id,
            request_id,
            expected_revision,
            outcome,
        })
    }

    fn selector(&self) -> Value {
        match self.draft_id {
            Some(id) => json!({"draft_id":id}),
            None => json!({"conversation_id":self.conversation_id}),
        }
    }
}

impl Store {
    pub(crate) fn capture_existing(
        &self,
        account: &str,
        params: &Value,
    ) -> Result<Option<CaptureRecord>> {
        let request = CaptureRequest::parse(params)?;
        let record = load_record(&self.db, account, request.request_id)?;
        if let Some(record) = &record {
            let same_draft = match request.draft_id {
                Some(id) => {
                    record
                        .draft_id
                        .as_deref()
                        .unwrap_or(&record.conversation_id)
                        == id
                }
                None => {
                    record.draft_id.is_none() && record.conversation_id == request.conversation_id
                }
            };
            if !same_draft
                || record.draft_revision != request.expected_revision
                || record.outcome != request.outcome
            {
                return Err(err(
                    "request_id_conflict",
                    "Capture request ID already identifies different immutable content",
                ));
            }
        }
        Ok(record)
    }
    pub(crate) fn draft_capture(&mut self, account: &str, params: &Value) -> Result<Value> {
        let request = CaptureRequest::parse(params)?;
        let mut transport = LocalCapture {
            outcome: request.outcome,
            thread_id: request
                .draft_id
                .unwrap_or(request.conversation_id)
                .to_owned(),
        };
        self.capture_with_transport(account, &request, &mut transport)
    }

    fn capture_with_transport<T: SubmissionTransport>(
        &mut self,
        account: &str,
        request: &CaptureRequest<'_>,
        transport: &mut T,
    ) -> Result<Value> {
        let mut record = match load_record(&self.db, account, request.request_id)? {
            Some(record) => {
                let same_draft = match request.draft_id {
                    Some(id) => {
                        record
                            .draft_id
                            .as_deref()
                            .unwrap_or(&record.conversation_id)
                            == id
                    }
                    None => {
                        record.draft_id.is_none()
                            && record.conversation_id == request.conversation_id
                    }
                };
                if !same_draft
                    || record.draft_revision != request.expected_revision
                    || record.outcome != request.outcome
                {
                    return Err(err(
                        "request_id_conflict",
                        "Capture request ID already identifies different immutable content",
                    ));
                }
                record
            }
            None => self.prepare_capture(account, request)?,
        };
        if record.operation.state == SendState::InFlight
            && self.account_lifecycle(account)?["state"] == "active"
        {
            let next = record.operation.recover();
            SqliteCaptureJournal { db: &mut self.db }
                .transition(&record.operation, &next)
                .map_err(capture_error)?;
            record.operation = next;
        }
        if record.operation.state == SendState::Prepared
            && self.account_lifecycle(account)?["state"] == "active"
        {
            let bytes = self.capture_bytes(&record.operation.mime_sha256, record.size)?;
            submit_once(
                &mut record.operation,
                &bytes,
                transport,
                &mut SqliteCaptureJournal { db: &mut self.db },
            )
            .map_err(capture_error)?;
        }
        let record = load_record(&self.db, account, request.request_id)?
            .ok_or_else(|| err("storage_error", "Capture journal entry disappeared"))?;
        let path = self.export_attachment(
            &record.operation.mime_sha256,
            record.size,
            "synthetic-draft-capture.eml",
        )?;
        let state = match record.operation.state {
            SendState::Prepared => "prepared",
            SendState::InFlight => "in_flight",
            SendState::Accepted { .. } => "accepted",
            SendState::Rejected { .. } => "rejected",
            SendState::AcceptanceUnknown => "acceptance_unknown",
        };
        let mut result = json!({"synthetic":true,"network_used":false,"draft_unchanged":true,"account":account,"request_id":request.request_id,
            "conversation_id":record.conversation_id,"draft_revision":record.draft_revision,"state":state,"sha256":record.operation.mime_sha256,
            "size":record.size,"path":path.to_str().ok_or_else(||err("content_unavailable","Capture export path cannot be represented"))?,
            "attempts_started":record.attempts_started,"created_at":record.created_at});
        if let Some(draft_id) = record.draft_id {
            result["draft_id"] = json!(draft_id);
            if record.conversation_id.is_empty() {
                result["conversation_id"] = Value::Null;
            }
        }
        Ok(result)
    }

    fn prepare_capture(
        &mut self,
        account: &str,
        request: &CaptureRequest<'_>,
    ) -> Result<CaptureRecord> {
        self.require_active_account(account)?;
        let draft = self.composition(account, &request.selector())?;
        if draft["discarded"] == true {
            return Err(err("invalid_draft", "Discarded draft cannot be captured"));
        }
        if draft["revision"].as_i64() != Some(request.expected_revision) {
            return Err(err(
                "stale_revision",
                "Draft changed before capture; reload its current revision",
            ));
        }
        if draft["mode"] != "new" {
            let source_hash = draft["source_hash"].as_str().ok_or_else(|| {
                err(
                    "content_unavailable",
                    "Original composition source is unavailable",
                )
            })?;
            self.composition_source_bytes(source_hash)?;
            if draft["source_unavailable"] == true {
                return Err(err(
                    "content_unavailable",
                    "Original composition routing was unavailable when saved",
                ));
            }
        }
        let attachments = self.capture_attachments(account, &draft)?;
        let mut parameters = request.selector();
        parameters["request_id"] = json!(request.request_id);
        parameters["expected_revision"] = json!(request.expected_revision);
        parameters["outcome"] = json!(request.outcome);
        let proof = prepare_capture_content(
            &self.google_blob_store(),
            account,
            &parameters,
            None,
            Some(&draft),
            &attachments,
        )?;
        let record = proof.record;
        let hash = record.operation.mime_sha256.clone();
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let revision: Option<i64> = tx
            .query_row(
                "SELECT revision FROM drafts WHERE account=?1 AND draft_id=?2",
                [account, draft["draft_id"].as_str().unwrap()],
                |row| row.get(0),
            )
            .optional()?;
        if revision != Some(request.expected_revision) {
            return Err(err(
                "stale_revision",
                "Draft changed before capture was persisted",
            ));
        }
        let serialized = serde_json::to_string(&record)?;
        if serialized.len() > MAX_CAPTURE_RECORD_BYTES {
            return Err(err(
                "invalid_draft",
                "Capture journal entry exceeds its bound",
            ));
        }
        tx.execute(
            "INSERT INTO capture_operations(account,id,json,blob_hash) VALUES(?1,?2,?3,?4)",
            params![account, request.request_id, serialized, hash],
        )?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(record)
    }

    fn capture_attachments(&self, account: &str, draft: &Value) -> Result<Vec<StagedAttachment>> {
        let Some(ids) = draft.get("attachment_ids") else {
            return Ok(vec![]);
        };
        let ids = ids
            .as_array()
            .ok_or_else(|| err("invalid_draft", "Draft attachment references are malformed"))?;
        if ids.len() > 50 {
            return Err(err("invalid_draft", "Draft has too many attachments"));
        }
        let mut seen = std::collections::HashSet::new();
        let mut staged = Vec::new();
        let mut total = 0u64;
        for id in ids {
            let id = id
                .as_str()
                .ok_or_else(|| err("invalid_draft", "Draft attachment reference is malformed"))?;
            crate::id(id)?;
            if !seen.insert(id) {
                return Err(err(
                    "invalid_draft",
                    "Draft repeats an attachment reference",
                ));
            }
            let row: Option<(String,String,i64,String,String)> = self.db.query_row(
                "SELECT name,mime_type,size,state,blob_hash FROM attachments WHERE account=?1 AND id=?2",[account,id],
                |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).optional()?;
            let (name, content_type, size, state, hash) =
                row.ok_or_else(|| err("not_found", "Draft attachment is not in this account"))?;
            let size = u64::try_from(size)
                .map_err(|_| err("content_unavailable", "Draft attachment size is invalid"))?;
            total = total
                .checked_add(size)
                .filter(|total| *total <= fm_mail::MAX_ATTACHMENT_BYTES as u64)
                .ok_or_else(|| {
                    err(
                        "invalid_draft",
                        "Draft attachments exceed the MIME byte limit",
                    )
                })?;
            if state != "available" {
                return Err(err(
                    "content_unavailable",
                    "Draft attachment is not fully retained",
                ));
            }
            let bytes = self.capture_bytes(&hash, size)?;
            staged.push(StagedAttachment {
                account_id: account.to_owned(),
                filename: name,
                content_type,
                bytes,
                sha256: hash,
                content_id: None,
            });
        }
        Ok(staged)
    }

    fn capture_bytes(&self, hash: &str, size: u64) -> Result<Vec<u8>> {
        let path = self.verified_blob(hash, size)?;
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(size + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != size || format!("{:x}", Sha256::digest(&bytes)) != hash {
            return Err(err(
                "content_unavailable",
                "Capture content failed verification",
            ));
        }
        Ok(bytes)
    }

    pub(crate) fn recover_captures(&mut self) -> Result<()> {
        let operations = {
            let mut stmt = self.db.prepare("SELECT account,id FROM capture_operations WHERE execution_kind='synthetic' AND json_extract(json,'$.operation.state')='InFlight'")?;
            let found = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            found
        };
        for (account, id) in operations {
            let record = load_record(&self.db, &account, &id)?
                .ok_or_else(|| err("storage_error", "Capture journal entry disappeared"))?;
            SqliteCaptureJournal { db: &mut self.db }
                .transition(&record.operation, &record.operation.recover())
                .map_err(capture_error)?;
        }
        Ok(())
    }
}

/// Content proof kept private to Core; no MIME bytes enter the writer queue.
pub(crate) struct PreparedCapture {
    record: CaptureRecord,
    path: std::path::PathBuf,
}

pub(crate) fn prepare_capture_content(
    blobs: &crate::content::BlobStore,
    account: &str,
    params: &Value,
    existing: Option<CaptureRecord>,
    draft: Option<&Value>,
    attachments: &[StagedAttachment],
) -> Result<PreparedCapture> {
    let record = if let Some(record) = existing {
        record
    } else {
        let request = CaptureRequest::parse(params)?;
        let draft = draft.ok_or_else(|| err("invalid_draft", "Capture draft is missing"))?;
        if draft["discarded"] == true {
            return Err(err("invalid_draft", "Discarded draft cannot be captured"));
        }
        if draft["source_unavailable"] == true && draft["mode"] != "new" {
            return Err(err(
                "content_unavailable",
                "Original composition routing was unavailable when saved",
            ));
        }
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| err("invalid_request", "System clock cannot date a capture"))?
            .as_secs();
        let digest = format!(
            "{:x}",
            Sha256::digest(format!("{account}\0{}", request.request_id).as_bytes())
        );
        let (compose_request, identity) = crate::composition::compose_request(
            account,
            draft,
            format!("<capture-{digest}@fuckmail.invalid>"),
            created_at,
        )?;
        let composed =
            fm_mail::build_mail(&compose_request, &identity, attachments).map_err(|_| {
                err(
                    "invalid_draft",
                    "Draft headers or content cannot form a bounded MIME message",
                )
            })?;
        blobs.stage_blob(&composed.raw)?;
        CaptureRecord {
            conversation_id: draft["conversation_id"].as_str().unwrap_or("").into(),
            draft_revision: request.expected_revision,
            outcome: request.outcome,
            size: composed.raw.len() as u64,
            created_at,
            attempts_started: 0,
            operation: SendOperation::prepared(
                AccountId::new(account).map_err(capture_error)?,
                request.request_id.into(),
                &composed.raw,
            )
            .map_err(capture_error)?,
            draft_id: if draft["legacy_reply"] == true {
                None
            } else {
                draft["draft_id"].as_str().map(str::to_owned)
            },
            subject: if draft["legacy_reply"] == true {
                None
            } else {
                draft["subject"].as_str().map(str::to_owned)
            },
        }
    };
    if serde_json::to_vec(&record)?.len() > MAX_CAPTURE_RECORD_BYTES {
        return Err(err(
            "invalid_draft",
            "Capture journal entry exceeds its bound",
        ));
    }
    // Export verifies the frozen source and the retained copy off the writer.
    let path = blobs.export_attachment(
        &record.operation.mime_sha256,
        record.size,
        "synthetic-draft-capture.eml",
    )?;
    Ok(PreparedCapture { record, path })
}

impl Store {
    /// None redirects a same-request race to its already frozen winner.
    pub(crate) fn finish_prepared_capture(
        &mut self,
        account: &str,
        params: &Value,
        proof: PreparedCapture,
    ) -> Result<Option<Value>> {
        let request = CaptureRequest::parse(params)?;
        let mut record = if let Some(existing) = self.capture_existing(account, params)? {
            if existing.operation.mime_sha256 != proof.record.operation.mime_sha256
                || existing.size != proof.record.size
            {
                return Ok(None);
            }
            existing
        } else {
            self.require_active_account(account)?;
            let record = proof.record;
            let serialized = serde_json::to_string(&record)?;
            let tx = self
                .db
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            let selected = record
                .draft_id
                .as_deref()
                .unwrap_or(&record.conversation_id);
            let revision: Option<i64> = tx
                .query_row(
                    "SELECT revision FROM drafts WHERE account=?1 AND draft_id=?2",
                    [account, selected],
                    |r| r.get(0),
                )
                .optional()?;
            if revision != Some(record.draft_revision) {
                return Err(err(
                    "stale_revision",
                    "Draft changed before capture was persisted",
                ));
            }
            tx.execute(
                "INSERT INTO capture_operations(account,id,json,blob_hash) VALUES(?1,?2,?3,?4)",
                params![
                    account,
                    request.request_id,
                    serialized,
                    record.operation.mime_sha256
                ],
            )?;
            tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
            tx.commit()?;
            record
        };
        if record.operation.state == SendState::InFlight
            && self.account_lifecycle(account)?["state"] == "active"
        {
            let next = record.operation.recover();
            SqliteCaptureJournal { db: &mut self.db }
                .transition(&record.operation, &next)
                .map_err(capture_error)?;
            record.operation = next;
        }
        if record.operation.state == SendState::Prepared
            && self.account_lifecycle(account)?["state"] == "active"
        {
            let in_flight = SendOperation {
                state: SendState::InFlight,
                ..record.operation.clone()
            };
            SqliteCaptureJournal { db: &mut self.db }
                .transition(&record.operation, &in_flight)
                .map_err(capture_error)?;
            // Local capture's accepted ID is exactly the verified MIME hash.
            // The proof replaces rehashing bytes on the SQLite writer.
            let state = match record.outcome {
                Outcome::Accepted => SendState::Accepted {
                    message_id: format!("synthetic-{}", record.operation.mime_sha256),
                    thread_id: request.draft_id.unwrap_or(request.conversation_id).into(),
                },
                Outcome::Unknown => SendState::AcceptanceUnknown,
            };
            let completed = SendOperation {
                state,
                ..in_flight.clone()
            };
            SqliteCaptureJournal { db: &mut self.db }
                .transition(&in_flight, &completed)
                .map_err(capture_error)?;
        }
        let record = load_record(&self.db, account, request.request_id)?
            .ok_or_else(|| err("storage_error", "Capture journal entry disappeared"))?;
        let state = match record.operation.state {
            SendState::Prepared => "prepared",
            SendState::InFlight => "in_flight",
            SendState::Accepted { .. } => "accepted",
            SendState::Rejected { .. } => "rejected",
            SendState::AcceptanceUnknown => "acceptance_unknown",
        };
        let mut result = json!({"synthetic":true,"network_used":false,"draft_unchanged":true,"account":account,"request_id":request.request_id,
            "conversation_id":record.conversation_id,"draft_revision":record.draft_revision,"state":state,"sha256":record.operation.mime_sha256,
            "size":record.size,"path":proof.path.to_str().ok_or_else(||err("content_unavailable","Capture export path cannot be represented"))?,"attempts_started":record.attempts_started,"created_at":record.created_at});
        if let Some(draft_id) = record.draft_id {
            result["draft_id"] = json!(draft_id);
            if record.conversation_id.is_empty() {
                result["conversation_id"] = Value::Null;
            }
        }
        Ok(Some(result))
    }
}

fn load_record(db: &Connection, account: &str, id: &str) -> Result<Option<CaptureRecord>> {
    crate::google_drafts::check_execution_kind(db, account, id, "synthetic")?;
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT json,blob_hash FROM capture_operations WHERE execution_kind='synthetic' AND account=?1 AND id=?2",
            [account, id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((json, hash)) = row else {
        return Ok(None);
    };
    if json.len() > MAX_CAPTURE_RECORD_BYTES {
        return Err(err(
            "storage_error",
            "Capture journal entry exceeds its bound",
        ));
    }
    let record: CaptureRecord = serde_json::from_str(&json)?;
    if record.operation.account.as_str() != account
        || record.operation.operation_id != id
        || record.operation.mime_sha256 != hash
    {
        return Err(err(
            "storage_error",
            "Capture journal identity is inconsistent",
        ));
    }
    Ok(Some(record))
}

struct SqliteCaptureJournal<'a> {
    db: &'a mut Connection,
}
impl SendJournal for SqliteCaptureJournal<'_> {
    fn transition(
        &mut self,
        expected: &SendOperation,
        next: &SendOperation,
    ) -> std::result::Result<(), GoogleError> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| GoogleError::StoreUnavailable)?;
        let mut record = load_record(&tx, expected.account.as_str(), &expected.operation_id)
            .map_err(|_| GoogleError::StoreUnavailable)?
            .ok_or(GoogleError::StaleCheckpoint)?;
        if record.operation != *expected
            || next.account != expected.account
            || next.operation_id != expected.operation_id
            || next.mime_sha256 != expected.mime_sha256
        {
            return Err(GoogleError::StaleCheckpoint);
        }
        if expected.state == SendState::Prepared && next.state == SendState::InFlight {
            record.attempts_started = record
                .attempts_started
                .checked_add(1)
                .ok_or(GoogleError::StoreUnavailable)?;
        }
        record.operation = next.clone();
        tx.execute(
            "UPDATE capture_operations SET json=?1 WHERE execution_kind='synthetic' AND account=?2 AND id=?3",
            params![
                serde_json::to_string(&record).map_err(|_| GoogleError::StoreUnavailable)?,
                expected.account.as_str(),
                expected.operation_id
            ],
        )
        .map_err(|_| GoogleError::StoreUnavailable)?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])
            .map_err(|_| GoogleError::StoreUnavailable)?;
        tx.commit().map_err(|_| GoogleError::StoreUnavailable)?;
        Ok(())
    }
}

fn capture_error(_: GoogleError) -> crate::CoreError {
    err("capture_error", "Synthetic capture could not complete; a started operation will not be automatically retried")
}

struct LocalCapture {
    outcome: Outcome,
    thread_id: String,
}
impl SubmissionTransport for LocalCapture {
    fn send(&mut self, _: &AccountId, mime: &[u8]) -> SubmissionResult {
        // The source already exists as immutable local content. No provider call,
        // address resolution, SMTP envelope or network service exists on this path.
        match self.outcome {
            Outcome::Accepted => SubmissionResult::Accepted {
                message_id: format!("synthetic-{:x}", Sha256::digest(mime)),
                thread_id: self.thread_id.clone(),
            },
            Outcome::Unknown => SubmissionResult::NoDefinitiveResponse,
        }
    }
}

impl Store {
    pub(crate) fn capture_history(&self, account: &str, p: &Value) -> Result<Value> {
        fields(
            p,
            &[
                "conversation_id",
                "draft_id",
                "limit",
                "offset",
                "unresolved_only",
            ],
        )?;
        let unresolved = match p.get("unresolved_only") {
            None => false,
            Some(Value::Bool(value)) => *value,
            _ => return Err(err("invalid_request", "unresolved_only must be boolean")),
        };
        let conversation = match p.get("conversation_id") {
            None => "",
            Some(_) => string(p, "conversation_id")?,
        };
        let draft = match p.get("draft_id") {
            None => "",
            Some(_) => string(p, "draft_id")?,
        };
        if !conversation.is_empty() && !draft.is_empty() {
            return Err(err(
                "invalid_request",
                "Choose one capture-history draft selector",
            ));
        }
        if !draft.is_empty() {
            self.composition(account, &json!({"draft_id":draft}))?;
        }
        if !conversation.is_empty() {
            self.conversation(account, conversation)?;
        }
        let bounded = |key: &str, default: i64, min: i64, max: i64| -> Result<i64> {
            let v = p
                .get(key)
                .map(|v| v.as_i64().unwrap_or(-1))
                .unwrap_or(default);
            if v < min || v > max {
                Err(err(
                    "invalid_request",
                    "Capture pagination is outside bounds",
                ))
            } else {
                Ok(v)
            }
        };
        let limit = bounded("limit", 30, 1, 100)?;
        let offset = bounded("offset", 0, 0, 10_000_000)?;
        let predicate="execution_kind='synthetic' AND account=?1 AND (?2='' OR json_extract(json,'$.conversation_id')=?2) AND (NOT ?5 OR json_extract(json,'$.operation.state') IN ('Prepared','InFlight','AcceptanceUnknown')) AND (?6='' OR COALESCE(json_extract(json,'$.draft_id'),json_extract(json,'$.conversation_id'))=?6)";
        let total: i64 = self.db.query_row(
            &format!("SELECT COUNT(*) FROM capture_operations WHERE {predicate}"),
            params![account, conversation, limit, offset, unresolved, draft],
            |r| r.get(0),
        )?;
        let mut stmt=self.db.prepare(&format!("SELECT id,json FROM capture_operations WHERE {predicate} ORDER BY json_extract(json,'$.created_at') DESC,id LIMIT ?3 OFFSET ?4"))?;
        let rows = stmt
            .query_map(
                params![account, conversation, limit, offset, unresolved, draft],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let captures=rows.into_iter().map(|(operation_id,text)|->Result<Value>{
            let r:CaptureRecord=serde_json::from_str(&text)?;
            let subject=match r.subject {Some(subject)=>subject,None=>self.db.query_row("SELECT subject FROM conversations WHERE account=?1 AND id=?2",params![account,r.conversation_id],|row|row.get::<_,String>(0)).optional()?.unwrap_or_else(||"Conversation unavailable".into())};
            let state=match r.operation.state{SendState::Prepared=>"prepared",SendState::InFlight=>"in_flight",SendState::Accepted{..}=>"accepted",SendState::Rejected{..}=>"failed",SendState::AcceptanceUnknown=>"acceptance_unknown"};
            let draft_id=r.draft_id.unwrap_or_else(||r.conversation_id.clone());
            Ok(json!({"account":account,"operation_id":operation_id,"subject":subject,"draft_id":draft_id,
                "conversation_id":if r.conversation_id.is_empty(){None}else{Some(r.conversation_id)},
                "draft_revision":r.draft_revision,"synthetic_outcome":r.outcome,"state":state,"mime_sha256":r.operation.mime_sha256,
                "bytes":r.size,"created_at":r.created_at,"attempts_started":r.attempts_started,"local_only":true,"provider_delivered":false}))
        }).collect::<Result<Vec<_>>>()?;
        Ok(
            json!({"captures":captures,"total":total,"next_offset":if offset+limit<total{Some(offset+limit)}else{None},"local_only":true}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mail_parser::MimeHeaders;
    const ACCOUNT: &str = "windscribe";
    const THREAD: &str = "windscribe-thread-000";

    fn save(store: &mut Store, body: &str, request_id: &str) -> Value {
        let revision = store.draft(ACCOUNT, THREAD).unwrap()["revision"].clone();
        store.handle("draft_save", Some(ACCOUNT), json!({"conversation_id":THREAD,"body":body,"expected_revision":revision,"request_id":request_id})).unwrap()
    }

    fn capture_params(draft: &Value, request_id: &str) -> Value {
        json!({"conversation_id":THREAD,"expected_revision":draft["revision"],"request_id":request_id})
    }

    #[test]
    fn unresolved_history_precedes_limit_and_replays_older_revision_without_changing_new_draft() {
        let temporary = tempfile::tempdir().unwrap();
        let mut store = Store::open(temporary.path()).unwrap();
        let draft = save(&mut store, "Original human reply", "old-draft");
        let mut request = capture_params(&draft, "old-unknown");
        request["outcome"] = json!("unknown");
        let receipt = store
            .handle("draft_capture", Some(ACCOUNT), request.clone())
            .unwrap();
        let original = load_record(&store.db, ACCOUNT, "old-unknown")
            .unwrap()
            .unwrap();
        // A synthetic journal fixture buries the real unresolved operation under
        // more accepted receipts than a maximum-size ordinary history page.
        for i in 0..105 {
            let mut record = original.clone();
            record.created_at += 1;
            record.operation.operation_id = format!("accepted-{i:03}");
            record.operation.state = SendState::Accepted {
                message_id: "fixture-message".into(),
                thread_id: "fixture-thread".into(),
            };
            store
                .db
                .execute(
                    "INSERT INTO capture_operations(account,id,json,blob_hash) VALUES(?1,?2,?3,?4)",
                    params![
                        ACCOUNT,
                        record.operation.operation_id,
                        serde_json::to_string(&record).unwrap(),
                        record.operation.mime_sha256
                    ],
                )
                .unwrap();
        }
        for (id, state) in [
            ("older-prepared", SendState::Prepared),
            ("older-flight", SendState::InFlight),
            (
                "known-rejection",
                SendState::Rejected {
                    reason: "Definite synthetic rejection".into(),
                },
            ),
        ] {
            let mut record = original.clone();
            record.created_at -= 1;
            record.operation.operation_id = id.into();
            record.operation.state = state;
            store
                .db
                .execute(
                    "INSERT INTO capture_operations(account,id,json,blob_hash) VALUES(?1,?2,?3,?4)",
                    params![
                        ACCOUNT,
                        id,
                        serde_json::to_string(&record).unwrap(),
                        record.operation.mime_sha256
                    ],
                )
                .unwrap();
        }
        let visible = store
            .capture_history(ACCOUNT, &json!({"conversation_id":THREAD,"limit":100}))
            .unwrap();
        assert!(visible["captures"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["state"] == "accepted"));
        let newer = save(&mut store, "Newer authored text stays intact", "new-draft");
        drop(store);
        let mut store = Store::open(temporary.path()).unwrap();
        let pending = store
            .capture_history(
                ACCOUNT,
                &json!({"conversation_id":THREAD,"unresolved_only":true,"limit":1}),
            )
            .unwrap();
        assert_eq!(pending["total"], 3);
        assert_eq!(pending["next_offset"], 1);
        let row = &pending["captures"][0];
        assert_eq!(row["operation_id"], "old-unknown");
        let replay = json!({"conversation_id":row["conversation_id"],"expected_revision":row["draft_revision"],"request_id":row["operation_id"],"outcome":row["synthetic_outcome"]});
        assert_eq!(
            store
                .handle("draft_capture", Some(ACCOUNT), replay)
                .unwrap(),
            receipt
        );
        assert_eq!(store.draft(ACCOUNT, THREAD).unwrap(), newer);
        assert_eq!(
            store
                .capture_history("personal", &json!({"unresolved_only":true}))
                .unwrap()["total"],
            0
        );
        assert_eq!(
            store
                .capture_history(ACCOUNT, &json!({"unresolved_only":"true"}))
                .unwrap_err()
                .code,
            "invalid_request"
        );
    }

    #[test]
    fn actual_qt_formatting_and_retained_attachment_survive_draft_restart_and_capture() {
        let temporary = tempfile::tempdir().unwrap();
        let profile = temporary.path().join("profile");
        let mut store = Store::open(&profile).unwrap();
        let plain = include_str!("../../../fixtures/composer-qt.txt");
        let html = include_str!("../../../fixtures/composer-qt.html");
        let attachment_id = "windscribe-attachment-0";
        let draft = store.handle("draft_save",Some(ACCOUNT),json!({"conversation_id":THREAD,"body":plain,"body_html":html,"attachment_ids":[attachment_id],"expected_revision":0,"request_id":"save-qt-rich"})).unwrap();
        assert_eq!(draft["body"], plain);
        assert_eq!(draft["body_html"], html);
        assert_eq!(draft["attachment_ids"], json!([attachment_id]));
        let (hash, size): (String, u64) = store
            .db
            .query_row(
                "SELECT blob_hash,size FROM attachments WHERE account=?1 AND id=?2",
                [ACCOUNT, attachment_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let attachment_bytes = store.capture_bytes(&hash, size).unwrap();
        drop(store);
        let mut reopened = Store::open(&profile).unwrap();
        assert_eq!(reopened.draft(ACCOUNT, THREAD).unwrap(), draft);
        let result = reopened
            .handle(
                "draft_capture",
                Some(ACCOUNT),
                capture_params(&draft, "capture-qt-rich"),
            )
            .unwrap();
        let raw = std::fs::read(result["path"].as_str().unwrap()).unwrap();
        let parsed = mail_parser::MessageParser::default().parse(&raw).unwrap();
        assert_eq!(
            parsed.body_text(0).unwrap(),
            format!("{plain}\n\n-- \n—\nWindscribe")
        );
        let alternative = parsed.body_html(0).unwrap();
        assert!(alternative.contains("font-weight:700"));
        assert!(alternative.contains("font-style:italic"));
        assert!(alternative.contains("Montréal"));
        assert!(!alternative.contains("<style"));
        let attachments = parsed.attachments().collect::<Vec<_>>();
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].attachment_name(), Some("Q3-infra-v4-0.txt"));
        assert_eq!(attachments[0].contents(), attachment_bytes);
        assert_eq!(
            format!("{:x}", Sha256::digest(attachments[0].contents())),
            hash
        );
        assert_eq!(reopened.draft(ACCOUNT, THREAD).unwrap(), draft);
        assert_eq!(result["network_used"], false);
    }

    #[test]
    fn older_plain_clients_clear_stale_html_and_preserve_selected_attachments() {
        let temporary = tempfile::tempdir().unwrap();
        let mut store = Store::open(&temporary.path().join("profile")).unwrap();
        let plain = include_str!("../../../fixtures/composer-qt.txt");
        let html = include_str!("../../../fixtures/composer-qt.html");
        let rich = store.handle("draft_save",Some(ACCOUNT),json!({"conversation_id":THREAD,"body":plain,"body_html":html,"attachment_ids":["windscribe-attachment-0"],"expected_revision":0,"request_id":"rich-initial"})).unwrap();
        let unchanged = save(&mut store, plain, "old-client-same-body");
        assert_eq!(unchanged["body_html"], html);
        assert_eq!(unchanged["attachment_ids"], rich["attachment_ids"]);
        let changed = save(&mut store, "New plain human wording", "old-client-new-body");
        assert!(changed["body_html"].is_null());
        assert_eq!(changed["attachment_ids"], rich["attachment_ids"]);
        let cleared = store.handle("draft_save",Some(ACCOUNT),json!({"conversation_id":THREAD,"body":changed["body"],"body_html":null,"attachment_ids":[],"expected_revision":changed["revision"],"request_id":"clear-optional-fields"})).unwrap();
        assert!(cleared["body_html"].is_null());
        assert_eq!(cleared["attachment_ids"], json!([]));
    }

    #[test]
    fn invalid_or_cross_account_attachments_cannot_replace_an_authored_draft() {
        let temporary = tempfile::tempdir().unwrap();
        let mut store = Store::open(&temporary.path().join("profile")).unwrap();
        let saved = save(&mut store, "Preserve this authored draft", "save-preserve");
        for (number, ids, code) in [
            (0, json!(["personal-attachment-0"]), "not_found"),
            (1, json!(["windscribe-attachment-1"]), "content_unavailable"),
            (2, json!(["windscribe-attachment-2"]), "content_unavailable"),
            (
                3,
                json!(["windscribe-attachment-0", "windscribe-attachment-0"]),
                "invalid_request",
            ),
            (4, json!(["../store.sqlite3"]), "not_found"),
        ] {
            let error = store.handle("draft_save",Some(ACCOUNT),json!({"conversation_id":THREAD,"body":"Rejected edit","attachment_ids":ids,"expected_revision":saved["revision"],"request_id":format!("invalid-attachment-{number}")})).unwrap_err();
            assert_eq!(error.code, code);
            assert_eq!(store.draft(ACCOUNT, THREAD).unwrap(), saved);
        }
        let too_large = "x".repeat(256 * 1024 + 1);
        assert_eq!(store.handle("draft_save",Some(ACCOUNT),json!({"conversation_id":THREAD,"body":"Rejected edit","body_html":too_large,"expected_revision":saved["revision"],"request_id":"large-html"})).unwrap_err().code,"invalid_request");
        assert_eq!(store.draft(ACCOUNT, THREAD).unwrap(), saved);
    }

    struct ObservedTransport {
        calls: usize,
        interrupt: bool,
        unknown: bool,
    }
    impl SubmissionTransport for ObservedTransport {
        fn send(&mut self, _: &AccountId, _: &[u8]) -> SubmissionResult {
            self.calls += 1;
            assert!(
                !self.interrupt,
                "simulated interruption after durable InFlight"
            );
            if self.unknown {
                SubmissionResult::NoDefinitiveResponse
            } else {
                SubmissionResult::Accepted {
                    message_id: "local-receipt".into(),
                    thread_id: THREAD.into(),
                }
            }
        }
    }

    #[test]
    fn interrupted_in_flight_capture_recovers_unknown_without_repeat_transport_call() {
        let temporary = tempfile::tempdir().unwrap();
        let profile = temporary.path().join("profile");
        let mut store = Store::open(&profile).unwrap();
        let draft = save(
            &mut store,
            "Human draft survives interruption",
            "save-interrupted",
        );
        let params = capture_params(&draft, "interrupted-capture");
        let request = CaptureRequest::parse(&params).unwrap();
        let mut transport = ObservedTransport {
            calls: 0,
            interrupt: true,
            unknown: false,
        };
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || store.capture_with_transport(ACCOUNT, &request, &mut transport)
        ))
        .is_err());
        assert_eq!(transport.calls, 1);
        assert_eq!(
            load_record(&store.db, ACCOUNT, request.request_id)
                .unwrap()
                .unwrap()
                .operation
                .state,
            SendState::InFlight
        );
        drop(store);
        let mut reopened = Store::open(&profile).unwrap();
        assert_eq!(
            load_record(&reopened.db, ACCOUNT, request.request_id)
                .unwrap()
                .unwrap()
                .operation
                .state,
            SendState::AcceptanceUnknown
        );
        let mut retry = ObservedTransport {
            calls: 0,
            interrupt: false,
            unknown: false,
        };
        for _ in 0..2 {
            let result = reopened
                .capture_with_transport(ACCOUNT, &request, &mut retry)
                .unwrap();
            assert_eq!(result["state"], "acceptance_unknown");
            assert_eq!(result["attempts_started"], 1);
        }
        assert_eq!(retry.calls, 0);
        assert_eq!(reopened.draft(ACCOUNT, THREAD).unwrap(), draft);
    }

    #[test]
    fn explicit_unknown_and_failed_receipt_commit_never_resubmit() {
        let temporary = tempfile::tempdir().unwrap();
        let profile = temporary.path().join("profile");
        let mut store = Store::open(&profile).unwrap();
        let draft = save(&mut store, "Keep this exact draft", "save-unknown");
        let mut params = capture_params(&draft, "unknown-capture");
        params["outcome"] = json!("unknown");
        let result = store
            .handle("draft_capture", Some(ACCOUNT), params.clone())
            .unwrap();
        assert_eq!(result["state"], "acceptance_unknown");
        let history = store
            .handle(
                "capture_history",
                Some(ACCOUNT),
                json!({"conversation_id":THREAD}),
            )
            .unwrap();
        let row = &history["captures"][0];
        assert_eq!(row["synthetic_outcome"], "unknown");
        let recovery = json!({"conversation_id":row["conversation_id"],"expected_revision":row["draft_revision"],"request_id":row["operation_id"],"outcome":row["synthetic_outcome"]});
        drop(store);
        let mut store = Store::open(&profile).unwrap();
        assert_eq!(
            store
                .handle("draft_capture", Some(ACCOUNT), recovery)
                .unwrap(),
            result
        );
        assert_eq!(
            store
                .handle("draft_capture", Some(ACCOUNT), params)
                .unwrap(),
            result
        );
        let params = capture_params(&draft, "receipt-commit-failure");
        let request = CaptureRequest::parse(&params).unwrap();
        store.db.execute_batch("CREATE TRIGGER fail_capture_receipt BEFORE UPDATE ON capture_operations WHEN instr(NEW.json,'\"Accepted\"')>0 BEGIN SELECT RAISE(ABORT,'fixture receipt commit interruption'); END;").unwrap();
        let mut transport = ObservedTransport {
            calls: 0,
            interrupt: false,
            unknown: false,
        };
        assert_eq!(
            store
                .capture_with_transport(ACCOUNT, &request, &mut transport)
                .unwrap_err()
                .code,
            "capture_error"
        );
        assert_eq!(transport.calls, 1);
        assert_eq!(
            load_record(&store.db, ACCOUNT, request.request_id)
                .unwrap()
                .unwrap()
                .operation
                .state,
            SendState::InFlight
        );
        store
            .db
            .execute_batch("DROP TRIGGER fail_capture_receipt")
            .unwrap();
        drop(store);
        let mut reopened = Store::open(&profile).unwrap();
        let result = reopened
            .capture_with_transport(ACCOUNT, &request, &mut transport)
            .unwrap();
        assert_eq!(result["state"], "acceptance_unknown");
        assert_eq!(transport.calls, 1);
        assert_eq!(reopened.draft(ACCOUNT, THREAD).unwrap(), draft);
    }

    #[test]
    fn prepared_capture_resumes_immutable_bytes_and_parameter_conflicts_are_rejected() {
        let temporary = tempfile::tempdir().unwrap();
        let profile = temporary.path().join("profile");
        let mut store = Store::open(&profile).unwrap();
        let draft = save(&mut store, "Prepared before interruption", "save-prepared");
        let params = capture_params(&draft, "prepared-capture");
        let request = CaptureRequest::parse(&params).unwrap();
        let original = store.prepare_capture(ACCOUNT, &request).unwrap();
        assert_eq!(original.attempts_started, 0);
        drop(store);
        let mut reopened = Store::open(&profile).unwrap();
        let later = save(
            &mut reopened,
            "Later revision remains separate",
            "save-newer",
        );
        let mut transport = ObservedTransport {
            calls: 0,
            interrupt: false,
            unknown: false,
        };
        let result = reopened
            .capture_with_transport(ACCOUNT, &request, &mut transport)
            .unwrap();
        assert_eq!(result["sha256"], original.operation.mime_sha256);
        assert_eq!(transport.calls, 1);
        let mut conflicting = params.clone();
        conflicting["outcome"] = json!("unknown");
        assert_eq!(
            reopened
                .handle("draft_capture", Some(ACCOUNT), conflicting)
                .unwrap_err()
                .code,
            "request_id_conflict"
        );
        let mut stale = params;
        stale["request_id"] = json!("new-but-stale");
        assert_eq!(
            reopened
                .handle("draft_capture", Some(ACCOUNT), stale)
                .unwrap_err()
                .code,
            "stale_revision"
        );
        let cross = capture_params(&later, "cross-account");
        assert_eq!(
            reopened
                .handle("draft_capture", Some("personal"), cross)
                .unwrap_err()
                .code,
            "not_found"
        );
        assert_eq!(reopened.draft(ACCOUNT, THREAD).unwrap(), later);
        assert_eq!(
            reopened
                .db
                .query_row("SELECT COUNT(*) FROM capture_operations", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn v1_migration_preserves_authored_drafts_views_flags_and_operation_journal() {
        let temporary = tempfile::tempdir().unwrap();
        let profile = temporary.path().join("profile");
        std::fs::create_dir(&profile).unwrap();
        let db = Connection::open(profile.join("store.sqlite3")).unwrap();
        db.execute_batch(include_str!("../../../fixtures/schema-v1.sql"))
            .unwrap();
        let mut old = Store {
            mail_changes: crate::queries::MailChanges::install(&db),
            db,
            profile: profile.clone(),
            _lock: std::fs::File::create(profile.join("store.lock")).unwrap(),
            projection_cache: Default::default(),
            coverage_cache: Default::default(),
            conversation_cache: Default::default(),
        };
        old.seed().unwrap();
        // Populate the retained v1 SQL shape directly: newer mutation/query
        // implementations must never be prerequisites for creating old data.
        let draft = json!({"conversation_id":THREAD,"body":"Migration preserves 日本語 and human words","revision":9});
        old.db
            .execute(
                "INSERT INTO drafts(account,conversation_id,body,revision) VALUES(?1,?2,?3,9)",
                params![ACCOUNT, THREAD, draft["body"].as_str().unwrap()],
            )
            .unwrap();
        old.db.execute("INSERT INTO operations(account,request_id,fingerprint,result_json) VALUES(?1,'old-save','preserved-fingerprint',?2)",params![ACCOUNT,serde_json::to_string(&draft).unwrap()]).unwrap();
        old.db
            .execute(
                "INSERT INTO views(account,revision,json) VALUES(?1,7,?2)",
                params![ACCOUNT, r#"{"query":"keep this view","folder":"sent"}"#],
            )
            .unwrap();
        old.db
            .execute(
                "UPDATE messages SET unread=0,starred=1 WHERE account=?1 AND conversation_id=?2",
                [ACCOUNT, THREAD],
            )
            .unwrap();
        let old_view = old.view(ACCOUNT).unwrap();
        let journal: String = old
            .db
            .query_row(
                "SELECT result_json FROM operations WHERE account=?1 AND request_id='old-save'",
                [ACCOUNT],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            old.db
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .unwrap(),
            1
        );
        drop(old);
        let mut migrated = Store::open(&profile).unwrap();
        assert_eq!(
            migrated
                .db
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .unwrap(),
            crate::SCHEMA_VERSION
        );
        let migrated_draft = migrated.draft(ACCOUNT, THREAD).unwrap();
        assert_eq!(migrated_draft["body"], draft["body"]);
        assert_eq!(migrated_draft["revision"], draft["revision"]);
        assert_eq!(migrated.view(ACCOUNT).unwrap(), old_view);
        assert_eq!(
            migrated
                .db
                .query_row(
                    "SELECT result_json FROM operations WHERE account=?1 AND request_id='old-save'",
                    [ACCOUNT],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            journal
        );
        assert_eq!(migrated.db.query_row("SELECT COUNT(*) FROM messages WHERE account=?1 AND conversation_id=?2 AND unread=0 AND starred=1",[ACCOUNT,THREAD],|row|row.get::<_,i64>(0)).unwrap(),3);
        let captured = migrated
            .handle(
                "draft_capture",
                Some(ACCOUNT),
                capture_params(&draft, "after-migration"),
            )
            .unwrap();
        assert_eq!(captured["state"], "accepted");
        assert_eq!(
            migrated
                .db
                .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
    }

    #[test]
    fn actual_v2_profile_upgrade_preserves_accepted_capture_without_resubmission() {
        let temporary = tempfile::tempdir().unwrap();
        let profile = temporary.path().join("profile");
        std::fs::create_dir(&profile).unwrap();
        let db = Connection::open(profile.join("store.sqlite3")).unwrap();
        db.execute_batch(include_str!("../../../fixtures/schema-v1.sql"))
            .unwrap();
        db.execute_batch(include_str!("migration-002.sql")).unwrap();
        let mut old = Store {
            mail_changes: crate::queries::MailChanges::install(&db),
            db,
            profile: profile.clone(),
            _lock: std::fs::File::create(profile.join("store.lock")).unwrap(),
            projection_cache: Default::default(),
            coverage_cache: Default::default(),
            conversation_cache: Default::default(),
        };
        old.seed().unwrap();
        old.db.execute("INSERT INTO drafts(account,conversation_id,body,revision) VALUES(?1,?2,'Human v2 draft remains intact',4)",[ACCOUNT,THREAD]).unwrap();
        old.db
            .execute(
                "INSERT INTO views(account,revision,json) VALUES(?1,2,'{\"query\":\"v2 query\"}')",
                [ACCOUNT],
            )
            .unwrap();
        let mime = b"From: you@windscribe.example\r\nTo: maya@windscribe.example\r\nSubject: v2 retained capture\r\nMessage-ID: <v2-capture@fixture.example>\r\n\r\nHuman v2 draft remains intact";
        let hash = old.stage_blob(mime).unwrap();
        let mut operation =
            SendOperation::prepared(AccountId::new(ACCOUNT).unwrap(), "v2-capture".into(), mime)
                .unwrap();
        operation.state = SendState::Accepted {
            message_id: "v2-local-receipt".into(),
            thread_id: THREAD.into(),
        };
        let record = CaptureRecord {
            conversation_id: THREAD.into(),
            draft_revision: 4,
            outcome: Outcome::Accepted,
            size: mime.len() as u64,
            created_at: 1_788_800_000,
            attempts_started: 1,
            operation,
            draft_id: None,
            subject: None,
        };
        let original_json = serde_json::to_string(&record).unwrap();
        old.db.execute("INSERT INTO capture_operations(account,id,json,blob_hash) VALUES(?1,'v2-capture',?2,?3)",params![ACCOUNT,original_json,hash]).unwrap();
        assert_eq!(
            old.db
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .unwrap(),
            2
        );
        drop(old);
        let mut upgraded = Store::open(&profile).unwrap();
        assert_eq!(
            upgraded
                .db
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .unwrap(),
            crate::SCHEMA_VERSION
        );
        let draft = upgraded.draft(ACCOUNT, THREAD).unwrap();
        assert_eq!(draft["body"], "Human v2 draft remains intact");
        assert_eq!(draft["revision"], 4);
        assert!(draft["body_html"].is_null());
        assert_eq!(draft["attachment_ids"], json!([]));
        assert_eq!(upgraded.view(ACCOUNT).unwrap()["view"]["query"], "v2 query");
        assert_eq!(
            upgraded
                .db
                .query_row(
                    "SELECT json FROM capture_operations WHERE account=?1 AND id='v2-capture'",
                    [ACCOUNT],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            original_json
        );
        let params = capture_params(&draft, "v2-capture");
        let request = CaptureRequest::parse(&params).unwrap();
        let mut transport = ObservedTransport {
            calls: 0,
            interrupt: false,
            unknown: false,
        };
        let replay = upgraded
            .capture_with_transport(ACCOUNT, &request, &mut transport)
            .unwrap();
        assert_eq!(transport.calls, 0);
        assert_eq!(replay["state"], "accepted");
        assert_eq!(
            std::fs::read(replay["path"].as_str().unwrap()).unwrap(),
            mime
        );
    }

    #[test]
    fn durable_draft_capture_replays_exact_mime_after_reopen_without_changing_draft() {
        let temporary = tempfile::tempdir().unwrap();
        let profile = temporary.path().join("profile");
        let mut store = Store::open(&profile).unwrap();
        let authored = "Yes — approved.\nこんにちは / café 🦀\n\n> Original question\n";
        let draft = save(&mut store, authored, "save-unicode");
        let params = capture_params(&draft, "capture-once");
        let result = store
            .handle("draft_capture", Some(ACCOUNT), params.clone())
            .unwrap();
        assert_eq!(result["state"], "accepted");
        assert_eq!(result["attempts_started"], 1);
        assert_eq!(result["network_used"], false);
        let raw = std::fs::read(result["path"].as_str().unwrap()).unwrap();
        let parsed = mail_parser::MessageParser::default().parse(&raw).unwrap();
        assert_eq!(
            parsed.body_text(0).unwrap(),
            format!("{authored}\n\n-- \n—\nWindscribe")
        );
        assert_eq!(
            parsed.from().unwrap().first().unwrap().address.as_deref(),
            Some("you@windscribe.example")
        );
        assert_eq!(
            parsed.to().unwrap().first().unwrap().address.as_deref(),
            Some("maya@windscribe.example")
        );
        assert_eq!(
            parsed.in_reply_to().as_text(),
            Some("windscribe-message-000-002@fixture.example")
        );
        assert_eq!(
            parsed.references().as_text(),
            Some("windscribe-message-000-002@fixture.example")
        );
        assert_eq!(store.draft(ACCOUNT, THREAD).unwrap(), draft);
        drop(store);
        let mut reopened = Store::open(&profile).unwrap();
        let later = save(
            &mut reopened,
            "A later human edit remains intact",
            "save-later",
        );
        let replay = reopened
            .handle("draft_capture", Some(ACCOUNT), params)
            .unwrap();
        assert_eq!(replay, result);
        assert_eq!(
            std::fs::read(replay["path"].as_str().unwrap()).unwrap(),
            raw
        );
        assert_eq!(reopened.draft(ACCOUNT, THREAD).unwrap(), later);
        assert_eq!(
            reopened
                .db
                .query_row("SELECT COUNT(*) FROM capture_operations", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}
