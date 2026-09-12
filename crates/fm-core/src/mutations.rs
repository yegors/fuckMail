//! Atomic, explicitly requested local changes. Provider delivery is a later layer.
use crate::{err, fields, id, string, CoreError, Result, Store};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

impl Store {
    pub(crate) fn mutate(&self, method: &str, account: &str, p: &Value) -> Result<Value> {
        let allowed: &[&str] = match method {
            "flag" => &[
                "id",
                "message_ids",
                "expected_revision",
                "request_id",
                "read",
                "starred",
            ],
            "set_category" => &[
                "id",
                "message_ids",
                "category_id",
                "present",
                "expected_revision",
                "request_id",
            ],
            "draft_save" => &[
                "conversation_id",
                "body",
                "body_html",
                "attachment_ids",
                "signature_text",
                "expected_revision",
                "request_id",
            ],
            "draft_discard" => &["conversation_id", "expected_revision", "request_id"],
            "view_save" => &["view", "expected_revision", "request_id"],
            _ => return Err(err("unsupported_method", "Unknown mutation")),
        };
        fields(p, allowed)?;
        let request_id = string(p, "request_id")?;
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&json!([method, p]))?)
        );
        // The serialized engine owns this connection. Query the same transaction
        // while constructing the response; record that response before commit.
        let tx = self.db.unchecked_transaction()?;
        let previous: Option<(String, String)> = tx
            .query_row(
                "SELECT fingerprint,result_json FROM operations WHERE account=?1 AND request_id=?2",
                params![account, request_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((old_fingerprint, result)) = previous {
            if old_fingerprint != fingerprint {
                return Err(err(
                    "request_id_conflict",
                    "This request ID already identifies a different command",
                ));
            }
            return Ok(serde_json::from_str(&result)?);
        }
        let expected = p["expected_revision"]
            .as_i64()
            .filter(|v| *v >= 0)
            .ok_or_else(|| {
                err(
                    "invalid_request",
                    "expected_revision must be a nonnegative integer",
                )
            })?;
        let result = match method {
            "flag" | "set_category" => {
                let conversation = string(p, "id")?;
                let current: i64 = tx
                    .query_row(
                        "SELECT revision FROM conversations WHERE account=?1 AND id=?2",
                        params![account, conversation],
                        |r| r.get(0),
                    )
                    .optional()?
                    .ok_or_else(|| {
                        err("not_found", "Conversation does not exist in this account")
                    })?;
                check_revision(expected, current)?;
                let targets = snapshot(p)?;
                for message in &targets {
                    let exists: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM messages WHERE account=?1 AND conversation_id=?2 AND id=?3)",
                        params![account, conversation, message], |r| r.get(0),
                    )?;
                    if !exists {
                        return Err(err(
                            "not_found",
                            "A target message is not in this account and conversation",
                        ));
                    }
                }
                if method == "flag" {
                    let read = optional_bool(p, "read")?;
                    let starred = optional_bool(p, "starred")?;
                    if read.is_none() && starred.is_none() {
                        return Err(err("invalid_request", "Supply read or starred explicitly"));
                    }
                    for message in &targets {
                        tx.execute("UPDATE messages SET unread=COALESCE(?1,unread),starred=COALESCE(?2,starred),revision=revision+1 WHERE account=?3 AND id=?4",
                            params![read.map(|v| !v), starred, account, message])?;
                        for (field, value) in [("read", read), ("starred", starred)] {
                            if let Some(value) = value {
                                record_manual_intent(&tx, account, message, field, "", value)?;
                            }
                        }
                    }
                } else {
                    let category = string(p, "category_id")?;
                    let present = p["present"]
                        .as_bool()
                        .ok_or_else(|| err("invalid_request", "present must be a boolean"))?;
                    let exists: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM categories WHERE account=?1 AND id=?2)",
                        params![account, category],
                        |r| r.get(0),
                    )?;
                    if !exists {
                        return Err(err("not_found", "Category does not exist in this account"));
                    }
                    for message in &targets {
                        if present {
                            tx.execute("INSERT OR IGNORE INTO message_categories(account,message_id,category_id) VALUES(?1,?2,?3)", params![account, message, category])?;
                        } else {
                            tx.execute("DELETE FROM message_categories WHERE account=?1 AND message_id=?2 AND category_id=?3", params![account, message, category])?;
                        }
                        tx.execute(
                            "UPDATE messages SET revision=revision+1 WHERE account=?1 AND id=?2",
                            params![account, message],
                        )?;
                        record_manual_intent(&tx, account, message, "category", category, present)?;
                    }
                }
                tx.execute(
                    "UPDATE conversations SET revision=revision+1 WHERE account=?1 AND id=?2",
                    params![account, conversation],
                )?;
                self.conversation(account, conversation)?
            }
            "draft_save" | "draft_discard" => {
                let conversation = string(p, "conversation_id")?;
                let discard = method == "draft_discard";
                let body = if discard {
                    ""
                } else {
                    p["body"]
                        .as_str()
                        .ok_or_else(|| err("invalid_request", "Draft body must be a string"))?
                };
                if body.len() > 256 * 1024 {
                    return Err(err("invalid_request", "Draft body exceeds 256 KiB"));
                }
                let exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM conversations WHERE account=?1 AND id=?2)",
                    params![account, conversation],
                    |r| r.get(0),
                )?;
                if !exists {
                    return Err(err(
                        "not_found",
                        "Conversation does not exist in this account",
                    ));
                }
                let (current, old_body, old_html, old_attachments): (i64,String,Option<String>,String) = tx
                    .query_row(
                        "SELECT revision,body,body_html,attachments_json FROM drafts WHERE account=?1 AND conversation_id=?2",
                        params![account, conversation],
                        |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
                    )
                    .optional()?
                    .unwrap_or((0,String::new(),None,"[]".into()));
                check_revision(expected, current)?;
                let body_html = if discard {
                    None
                } else {
                    match p.get("body_html") {
                        Some(Value::Null) => None,
                        Some(Value::String(html)) if html.len() <= 256 * 1024 => Some(html.clone()),
                        Some(_) => {
                            return Err(err(
                                "invalid_request",
                                "Draft HTML must be null or a string of at most 256 KiB",
                            ))
                        }
                        None if old_body == body => old_html,
                        None => None, // A plain-only client must not leave stale rich text behind.
                    }
                };
                let attachments_json = if discard {
                    "[]".to_owned()
                } else {
                    match p.get("attachment_ids") {
                        None => old_attachments,
                        Some(value) => {
                            let ids = value.as_array().filter(|ids| ids.len() <= 50).ok_or_else(
                                || {
                                    err(
                                        "invalid_request",
                                        "Draft attachment_ids must contain at most 50 identifiers",
                                    )
                                },
                            )?;
                            let mut seen = HashSet::new();
                            let mut total = 0u64;
                            for value in ids {
                                let attachment = value.as_str().ok_or_else(|| {
                                    err(
                                        "invalid_request",
                                        "Draft attachment identifier must be a string",
                                    )
                                })?;
                                id(attachment)?;
                                if !seen.insert(attachment) {
                                    return Err(err(
                                        "invalid_request",
                                        "Draft repeats an attachment identifier",
                                    ));
                                }
                                let row: Option<(String,i64,String)> = tx.query_row("SELECT state,size,blob_hash FROM attachments WHERE account=?1 AND id=?2",params![account,attachment],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
                                let (state, size, hash) = row.ok_or_else(|| {
                                    err("not_found", "Draft attachment is not in this account")
                                })?;
                                let size = u64::try_from(size).map_err(|_| {
                                    err("content_unavailable", "Draft attachment size is invalid")
                                })?;
                                total = total
                                    .checked_add(size)
                                    .filter(|total| *total <= fm_mail::MAX_ATTACHMENT_BYTES as u64)
                                    .ok_or_else(|| {
                                        err("invalid_request", "Draft attachments exceed 25 MiB")
                                    })?;
                                if state != "available" {
                                    return Err(err(
                                        "content_unavailable",
                                        "Draft attachment is not fully retained",
                                    ));
                                }
                                self.verified_blob(&hash, size)?;
                            }
                            serde_json::to_string(ids)?
                        }
                    }
                };
                let snapshot: (String,String,i64) = tx.query_row("SELECT m.signature_text,a.name,m.revision FROM accounts a JOIN account_metadata m ON m.account=a.id WHERE a.id=?1",[account],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
                tx.execute("INSERT INTO drafts(account,conversation_id,body,revision,body_html,attachments_json,signature_text,from_name,identity_revision) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(account,conversation_id) DO UPDATE SET body=excluded.body,revision=excluded.revision,body_html=excluded.body_html,attachments_json=excluded.attachments_json,signature_text=CASE WHEN drafts.discarded=1 THEN excluded.signature_text ELSE drafts.signature_text END,from_name=CASE WHEN drafts.discarded=1 THEN excluded.from_name ELSE drafts.from_name END,identity_revision=CASE WHEN drafts.discarded=1 THEN excluded.identity_revision ELSE drafts.identity_revision END,discarded=0",
                    params![account, conversation, body, next_revision(current)?,body_html,attachments_json,if discard { "" } else { &snapshot.0 },snapshot.1,snapshot.2])?;
                if let Some(signature) = p.get("signature_text") {
                    let signature = signature
                        .as_str()
                        .filter(|v| v.len() <= 16 * 1024 && !v.contains('\0'))
                        .ok_or_else(|| {
                            err(
                                "invalid_request",
                                "Draft signature must be text of at most 16 KiB",
                            )
                        })?;
                    tx.execute("UPDATE drafts SET signature_text=?1 WHERE account=?2 AND conversation_id=?3",params![signature,account,conversation])?;
                }
                if discard {
                    tx.execute("UPDATE drafts SET signature_text='',discarded=1 WHERE account=?1 AND conversation_id=?2",params![account,conversation])?;
                }
                self.draft(account, conversation)?
            }
            "view_save" => {
                let view = &p["view"];
                if !view.is_object() {
                    return Err(err("invalid_request", "View must be a JSON object"));
                }
                let encoded = serde_json::to_string(view)?;
                if encoded.len() > 16 * 1024 {
                    return Err(err("invalid_request", "View exceeds 16 KiB"));
                }
                validate_view(&self.db, account, view)?;
                let current: i64 = tx
                    .query_row(
                        "SELECT revision FROM views WHERE account=?1",
                        [account],
                        |r| r.get(0),
                    )
                    .optional()?
                    .unwrap_or(0);
                check_revision(expected, current)?;
                tx.execute("INSERT INTO views(account,revision,json) VALUES(?1,?2,?3) ON CONFLICT(account) DO UPDATE SET revision=excluded.revision,json=excluded.json",
                    params![account, next_revision(current)?, encoded])?;
                self.view(account)?
            }
            _ => unreachable!(),
        };
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.execute("INSERT INTO operations(account,request_id,fingerprint,result_json) VALUES(?1,?2,?3,?4)",
            params![account, request_id, fingerprint, serde_json::to_string(&result)?])?;
        tx.commit()?;
        Ok(result)
    }
}

fn next_revision(current: i64) -> Result<i64> {
    current
        .checked_add(1)
        .ok_or_else(|| err("storage_error", "Revision exhausted"))
}
// A newer provider snapshot must not overwrite an unacknowledged human action.
// These local overlays share the mutation transaction and its exact message set.
fn record_manual_intent(
    tx: &rusqlite::Transaction<'_>,
    account: &str,
    message: &str,
    field: &str,
    category: &str,
    value: bool,
) -> Result<()> {
    tx.execute(
        "INSERT INTO google_manual_intents(account,message_id,field,category_id,value,revision)
         SELECT account,id,?3,?4,?5,revision FROM messages WHERE account=?1 AND id=?2
         ON CONFLICT(account,message_id,field,category_id) DO UPDATE SET value=excluded.value,revision=excluded.revision",
        params![account, message, field, category, value],
    )?;
    Ok(())
}
pub(crate) fn check_revision(expected: i64, actual: i64) -> Result<()> {
    if expected != actual {
        Err(CoreError {
            code: "stale_revision".into(),
            message: "State changed; reload before applying this command".into(),
            details: json!({"expected_revision":expected,"current_revision":actual}),
        })
    } else {
        next_revision(actual).map(|_| ())
    }
}
fn optional_bool(p: &Value, key: &str) -> Result<Option<bool>> {
    match p.get(key) {
        None => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        _ => Err(err("invalid_request", "Flag values must be booleans")),
    }
}
fn snapshot(p: &Value) -> Result<Vec<&str>> {
    let values = p["message_ids"]
        .as_array()
        .ok_or_else(|| err("invalid_request", "Supply a message_ids snapshot"))?;
    if values.is_empty() || values.len() > 200 {
        return Err(err(
            "invalid_request",
            "Snapshot must contain 1 to 200 message IDs",
        ));
    }
    let mut seen = HashSet::new();
    let mut targets = Vec::with_capacity(values.len());
    for value in values {
        let message = id(value
            .as_str()
            .ok_or_else(|| err("invalid_request", "Message IDs must be strings"))?)?;
        if !seen.insert(message) {
            return Err(err("invalid_request", "Duplicate snapshot target"));
        }
        targets.push(message);
    }
    Ok(targets)
}
// Presentation state is data, never a provider command. Keep the currently
// consumed fields explicit rather than turning settings into arbitrary storage.
fn validate_view(db: &rusqlite::Connection, account: &str, view: &Value) -> Result<()> {
    fields(
        view,
        &[
            "selected",
            "conversation_id",
            "selected_conversation",
            "selectedConversation",
            "account",
            "query",
            "folder",
            "category",
            "weekStart",
            "readerY",
            "listY",
            "calendarY",
            "splitWidth",
            "expanded",
            "screen",
            "filters",
            "replyOpen",
            "selectedMember",
        ],
    )?;
    for key in [
        "selected",
        "conversation_id",
        "selected_conversation",
        "selectedConversation",
    ] {
        if let Some(value) = view.get(key) {
            if value.is_null() || value.as_str() == Some("") {
                continue;
            }
            let conversation = id(value
                .as_str()
                .ok_or_else(|| err("invalid_request", "Selected conversation must be an ID"))?)?;
            let exists: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM conversations WHERE account=?1 AND id=?2)",
                params![account, conversation],
                |r| r.get(0),
            )?;
            if !exists {
                return Err(err(
                    "not_found",
                    "Selected conversation is outside this account",
                ));
            }
        }
    }
    if let Some(filters) = view.get("filters") {
        fields(
            filters,
            &["sender", "status", "has_attachment", "after", "before"],
        )?;
        crate::queries::validate_filters(filters)?;
    }
    if view.get("replyOpen").is_some_and(|v| !v.is_boolean()) {
        return Err(err("invalid_request", "replyOpen must be boolean"));
    }
    if let Some(value) = view
        .get("selectedMember")
        .filter(|v| !v.is_null() && v.as_str() != Some(""))
    {
        let mid = id(value
            .as_str()
            .ok_or_else(|| err("invalid_request", "selectedMember must be an identifier"))?)?;
        let found: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM messages WHERE account=?1 AND id=?2)",
            params![account, mid],
            |r| r.get(0),
        )?;
        if !found {
            return Err(err("not_found", "Selected member is outside this account"));
        }
    }
    if let Some(value) = view.get("account") {
        if value.as_str() != Some(account) {
            return Err(err(
                "invalid_request",
                "View account must match request scope",
            ));
        }
    }
    for key in ["readerY", "listY", "calendarY", "splitWidth"] {
        if let Some(value) = view.get(key) {
            // QML scroll offsets can briefly become negative during overscroll.
            if value
                .as_f64()
                .is_none_or(|n| !n.is_finite() || !(-1_000_000.0..=1_000_000.0).contains(&n))
            {
                return Err(err(
                    "invalid_request",
                    "View position must be a bounded number",
                ));
            }
        }
    }
    if view.get("expanded").is_some_and(|v| !v.is_boolean()) {
        return Err(err("invalid_request", "expanded must be a boolean"));
    }
    for key in ["query", "category", "weekStart", "folder", "screen"] {
        if let Some(value) = view.get(key) {
            if value.as_str().is_none_or(|s| s.len() > 1024) {
                return Err(err("invalid_request", "View text must be a bounded string"));
            }
        }
    }
    if view
        .get("folder")
        .is_some_and(|v| !["inbox", "sent", "all"].contains(&v.as_str().unwrap_or("")))
    {
        return Err(err("invalid_request", "Unknown view folder"));
    }
    if view
        .get("screen")
        .is_some_and(|v| !["mail", "calendar", "settings"].contains(&v.as_str().unwrap_or("")))
    {
        return Err(err("invalid_request", "Unknown view screen"));
    }
    if let Some(value) = view.get("weekStart") {
        let valid = value.as_str().is_some_and(|s| {
            s.len() == 10 && chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
        });
        if !valid {
            return Err(err("invalid_request", "weekStart must be an ISO date"));
        }
    }
    if let Some(category) = view
        .get("category")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM categories WHERE account=?1 AND id=?2)",
            params![account, category],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(err("not_found", "View category is outside this account"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_reply_does_not_inherit_an_earlier_snapshot_action() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path()).unwrap();
        let conversation: String = store
            .db
            .query_row(
                "SELECT id FROM conversations WHERE account='windscribe' ORDER BY id LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let before = store.conversation("windscribe", &conversation).unwrap();
        let snapshot = before["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["id"].clone())
            .collect::<Vec<_>>();
        // Simulate one already-normalized provider arrival, independent of the
        // mutation implementation. Only the provider path introduces this row.
        store.db.execute("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash,revision) SELECT account,'arriving-reply',conversation_id,'new-remote-reply',sender_name,sender_email,recipient,subject,'New source reply','2026-09-11T19:00:00Z',1,0,1,0,source_hash,1 FROM messages WHERE account='windscribe' AND conversation_id=?1 LIMIT 1", [&conversation]).unwrap();
        store
            .db
            .execute(
                "UPDATE conversations SET revision=revision+1 WHERE account='windscribe' AND id=?1",
                [&conversation],
            )
            .unwrap();
        let mut command = json!({"id":conversation,"message_ids":snapshot,"expected_revision":before["revision"],"request_id":"old-snapshot","read":true});
        assert_eq!(
            store
                .handle("flag", Some("windscribe"), command.clone())
                .unwrap_err()
                .code,
            "stale_revision"
        );
        command["expected_revision"] = json!(before["revision"].as_i64().unwrap() + 1);
        let after = store.handle("flag", Some("windscribe"), command).unwrap();
        for message in after["messages"].as_array().unwrap() {
            assert_eq!(message["unread"], message["id"] == "arriving-reply");
        }
    }
}
