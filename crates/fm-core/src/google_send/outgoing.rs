//! Read-only presentation of frozen submissions until their provider copy arrives.
use super::*;
use mail_parser::MimeHeaders;

pub struct ReadPlan {
    upload: GoogleSendUploadPlan,
    metadata: Value,
}

impl Store {
    pub fn google_sent(&self, account: &str, request: &Value) -> Result<Value> {
        fields(request, &["offset", "limit", "conversation_id"])?;
        id(account)?;
        let number = |key, default, max| -> Result<u64> {
            match request.get(key) {
                None => Ok(default),
                Some(value) => value
                    .as_u64()
                    .filter(|v| *v <= max)
                    .ok_or_else(|| err("invalid_params", "Invalid outgoing page bound")),
            }
        };
        let offset = number("offset", 0, 1_000_000)?;
        let limit = number("limit", 50, 100)?.max(1);
        let conversation = request
            .get("conversation_id")
            .map(|v| {
                v.as_str()
                    .ok_or_else(|| err("invalid_params", "Invalid conversation identity"))
            })
            .transpose()?;
        // The existing execution-kind index bounds this to this account's Google
        // journal. Acquired identities include Trash/tombstones, preventing revival.
        let mut statement = self.db.prepare("SELECT c.id FROM capture_operations c
            LEFT JOIN messages source ON source.account=c.account AND source.id=json_extract(c.json,'$.headers.source_message_id')
            WHERE c.execution_kind='google' AND c.account=?1 AND json_extract(c.json,'$.kind')='send'
            AND json_extract(c.json,'$.state') IN ('prepared','in_flight','acceptance_unknown','accepted')
            AND (?2 IS NULL OR (source.conversation_id=?2 AND json_extract(c.json,'$.headers.mode') IN ('reply','reply_all')))
            AND NOT EXISTS(SELECT 1 FROM messages acquired WHERE acquired.account=c.account
                AND acquired.remote_id=json_extract(c.json,'$.ack.message_id'))
            ORDER BY json_extract(c.json,'$.created_at') DESC,c.id DESC LIMIT ?3 OFFSET ?4")?;
        let requests = statement
            .query_map(params![account, conversation, limit + 1, offset], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut rows = Vec::new();
        let mut used = 0;
        for request in requests.iter().take(limit as usize) {
            let op = self
                .google_send_operation(account, request)?
                .ok_or_else(not_found)?;
            let row = self.outgoing_metadata(&op)?;
            used += serde_json::to_vec(&row)?.len();
            if used > 850 * 1024 && !rows.is_empty() {
                break;
            }
            rows.push(row);
        }
        let next = (requests.len() > rows.len()).then_some(offset + rows.len() as u64);
        Ok(json!({"account":account,"outgoing":rows,"next_offset":next}))
    }

    fn outgoing_metadata(&self, op: &Operation) -> Result<Value> {
        let source: Option<String> = self
            .db
            .query_row(
                "SELECT conversation_id FROM messages WHERE account=?1 AND id=?2",
                params![
                    op.account,
                    op.headers["source_message_id"].as_str().unwrap_or("")
                ],
                |r| r.get(0),
            )
            .optional()?;
        let observed: Option<String> = self
            .db
            .query_row(
                "SELECT conversation_id FROM messages WHERE account=?1 AND remote_id=?2",
                params![
                    op.account,
                    op.ack.as_ref().map(|a| a.message_id.as_str()).unwrap_or("")
                ],
                |r| r.get(0),
            )
            .optional()?;
        Ok(
            json!({"account":op.account,"id":format!("outgoing:{}",op.request_id),"request_id":op.request_id,
            "draft_id":op.draft_id,"draft_revision":op.draft_revision,"state":op.state,"request":op.request,
            "source_version":op.upload_sha256,"created_at":op.created_at,"headers":op.headers,
            "conversation_id":if matches!(op.headers["mode"].as_str(),Some("reply"|"reply_all")) {source} else {None},
            "observed_conversation_id":observed}),
        )
    }

    pub fn google_sent_message_begin(&self, account: &str, request: &Value) -> Result<ReadPlan> {
        fields(request, &["request_id"])?;
        id(account)?;
        let op = self
            .google_send_operation(account, string(request, "request_id")?)?
            .ok_or_else(not_found)?;
        Ok(ReadPlan {
            metadata: self.outgoing_metadata(&op)?,
            upload: GoogleSendUploadPlan {
                blobs: self.google_blob_store(),
                hash: op.upload_sha256,
                size: op.size,
            },
        })
    }
}

pub fn read(plan: ReadPlan, cancel: &CancellationToken) -> Result<Value> {
    let bytes = load_google_send_upload(plan.upload, cancel)?;
    let parsed = mail_parser::MessageParser::default()
        .parse(&bytes)
        .ok_or_else(|| {
            err(
                "content_unavailable",
                "The retained submission cannot be parsed",
            )
        })?;
    let mut result = plan.metadata;
    let text = parsed.body_text(0).unwrap_or_default();
    // Bound the escaped JSON contribution as well as UTF-8, preserving characters.
    let mut preview = String::new();
    let mut size = 0;
    for c in text.chars() {
        size += if c.is_control() {
            6
        } else {
            c.len_utf8().max(2)
        };
        if size > 256 * 1024 {
            break;
        }
        preview.push(c);
    }
    result["text_truncated"] = (preview.len() < text.len()).into();
    result["text"] = preview.into();
    result["attachments"] = parsed.attachments().take(50).map(|part| json!({"name":part.attachment_name().unwrap_or("Attachment").chars().take(320).collect::<String>(),"size":part.len()})).collect::<Vec<_>>().into();
    cancelled(cancel)?;
    Ok(result)
}
