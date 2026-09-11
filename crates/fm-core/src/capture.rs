//! Explicit local diagnostic: durable draft -> MIME -> synthetic capture.
//! There is no network transport and this operation never clears or sends a draft.
use crate::{err, fields, string, Result, Store};
use fm_google::send::{
    submit_once, SendJournal, SendOperation, SendState, SubmissionResult, SubmissionTransport,
};
use fm_google::{AccountId, GoogleError};
use fm_mail::{AccountIdentity, Address, ComposeRequest};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Read;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Accepted,
    Unknown,
}

#[derive(Clone, Serialize, Deserialize)]
struct CaptureRecord {
    conversation_id: String,
    draft_revision: i64,
    outcome: Outcome,
    size: u64,
    created_at: u64,
    attempts_started: u32,
    operation: SendOperation,
}

struct CaptureRequest<'a> {
    conversation_id: &'a str,
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
                "request_id",
                "expected_revision",
                "outcome",
            ],
        )?;
        let conversation_id = string(value, "conversation_id")?;
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
            request_id,
            expected_revision,
            outcome,
        })
    }
}

impl Store {
    pub(crate) fn draft_capture(&mut self, account: &str, params: &Value) -> Result<Value> {
        let request = CaptureRequest::parse(params)?;
        let mut transport = LocalCapture {
            outcome: request.outcome,
            thread_id: request.conversation_id.to_owned(),
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
                if record.conversation_id != request.conversation_id
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
        if record.operation.state == SendState::InFlight {
            let next = record.operation.recover();
            SqliteCaptureJournal { db: &mut self.db }
                .transition(&record.operation, &next)
                .map_err(capture_error)?;
            record.operation = next;
        }
        if record.operation.state == SendState::Prepared {
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
        Ok(
            json!({"synthetic":true,"network_used":false,"draft_unchanged":true,"account":account,"request_id":request.request_id,
            "conversation_id":record.conversation_id,"draft_revision":record.draft_revision,"state":state,"sha256":record.operation.mime_sha256,
            "size":record.size,"path":path.to_str().ok_or_else(||err("content_unavailable","Capture export path cannot be represented"))?,
            "attempts_started":record.attempts_started,"created_at":record.created_at}),
        )
    }

    fn prepare_capture(
        &mut self,
        account: &str,
        request: &CaptureRequest<'_>,
    ) -> Result<CaptureRecord> {
        let draft = self.draft(account, request.conversation_id)?;
        if draft["revision"].as_i64() != Some(request.expected_revision) {
            return Err(err(
                "stale_revision",
                "Draft changed before capture; reload its current revision",
            ));
        }
        let accounts = self.accounts()?;
        let identity = accounts["accounts"]
            .as_array()
            .and_then(|accounts| accounts.iter().find(|row| row["id"] == account))
            .ok_or_else(|| err("not_found", "Capture account does not exist"))?;
        let source_hash: String = self.db.query_row("SELECT source_hash FROM messages WHERE account=?1 AND conversation_id=?2 ORDER BY sent_at DESC,id DESC LIMIT 1", [account, request.conversation_id], |row| row.get(0))?;
        if !self.blob_present(&source_hash) {
            return Err(err(
                "content_unavailable",
                "Original reply source is unavailable",
            ));
        }
        let source_size =
            std::fs::symlink_metadata(self.profile.join("blobs").join(&source_hash))?.len();
        let source = self.capture_bytes(&source_hash, source_size)?;
        let parsed = mail_parser::MessageParser::default()
            .parse(&source)
            .ok_or_else(|| {
                err(
                    "content_unavailable",
                    "Original reply source cannot be parsed",
                )
            })?;
        let reply_to_message_id = parsed.message_id().map(|id| format!("<{id}>"));
        let references = parsed
            .references()
            .as_text_list()
            .map(|ids| ids.iter().map(|id| format!("<{id}>")).collect())
            .unwrap_or_default();
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| err("invalid_request", "System clock cannot date a capture"))?
            .as_secs();
        let identity = AccountIdentity {
            account_id: account.to_owned(),
            from: Address {
                name: Some(identity["name"].as_str().unwrap_or("").to_owned()),
                email: draft["from"]
                    .as_str()
                    .ok_or_else(|| err("invalid_request", "Draft From identity is missing"))?
                    .to_owned(),
            },
            signature_text: identity["signature_text"].as_str().unwrap_or("").to_owned(),
            signature_html: None,
        };
        let operation_digest = format!(
            "{:x}",
            Sha256::digest(format!("{account}\0{}", request.request_id).as_bytes())
        );
        let composed = fm_mail::build_mail(
            &ComposeRequest {
                account_id: account.to_owned(),
                to: vec![Address {
                    name: None,
                    email: draft["to"]
                        .as_str()
                        .ok_or_else(|| err("invalid_request", "Draft recipient is missing"))?
                        .to_owned(),
                }],
                cc: vec![],
                bcc: vec![],
                subject: draft["subject"].as_str().unwrap_or("").to_owned(),
                body_text: draft["body"].as_str().unwrap_or("").to_owned(),
                body_html: None,
                message_id: format!("<capture-{operation_digest}@fuckmail.invalid>"),
                reply_to_message_id,
                references,
                timestamp_secs: created_at,
            },
            &identity,
            &[],
        )
        .map_err(|_| {
            err(
                "invalid_draft",
                "Draft headers or content cannot form a bounded MIME message",
            )
        })?;
        let hash = self.stage_blob(&composed.raw)?;
        let record = CaptureRecord {
            conversation_id: request.conversation_id.to_owned(),
            draft_revision: request.expected_revision,
            outcome: request.outcome,
            size: composed.raw.len() as u64,
            created_at,
            attempts_started: 0,
            operation: SendOperation::prepared(
                AccountId::new(account).map_err(capture_error)?,
                request.request_id.to_owned(),
                &composed.raw,
            )
            .map_err(capture_error)?,
        };
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let revision: Option<i64> = tx
            .query_row(
                "SELECT revision FROM drafts WHERE account=?1 AND conversation_id=?2",
                [account, request.conversation_id],
                |row| row.get(0),
            )
            .optional()?;
        if revision != Some(request.expected_revision) {
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
                serde_json::to_string(&record)?,
                hash
            ],
        )?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(record)
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
            let mut stmt = self.db.prepare("SELECT account,id FROM capture_operations WHERE json_extract(json,'$.operation.state')='InFlight'")?;
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

fn load_record(db: &Connection, account: &str, id: &str) -> Result<Option<CaptureRecord>> {
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT json,blob_hash FROM capture_operations WHERE account=?1 AND id=?2",
            [account, id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((json, hash)) = row else {
        return Ok(None);
    };
    if json.len() > 16 * 1024 {
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
            "UPDATE capture_operations SET json=?1 WHERE account=?2 AND id=?3",
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

#[cfg(test)]
mod tests {
    use super::*;
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
