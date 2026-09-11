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
            "draft_save" => &["conversation_id", "body", "expected_revision", "request_id"],
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
                    }
                }
                tx.execute(
                    "UPDATE conversations SET revision=revision+1 WHERE account=?1 AND id=?2",
                    params![account, conversation],
                )?;
                self.conversation(account, conversation)?
            }
            "draft_save" => {
                let conversation = string(p, "conversation_id")?;
                let body = p["body"]
                    .as_str()
                    .ok_or_else(|| err("invalid_request", "Draft body must be a string"))?;
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
                let current: i64 = tx
                    .query_row(
                        "SELECT revision FROM drafts WHERE account=?1 AND conversation_id=?2",
                        params![account, conversation],
                        |r| r.get(0),
                    )
                    .optional()?
                    .unwrap_or(0);
                check_revision(expected, current)?;
                tx.execute("INSERT INTO drafts(account,conversation_id,body,revision) VALUES(?1,?2,?3,?4) ON CONFLICT(account,conversation_id) DO UPDATE SET body=excluded.body,revision=excluded.revision",
                    params![account, conversation, body, next_revision(current)?])?;
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
fn check_revision(expected: i64, actual: i64) -> Result<()> {
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
