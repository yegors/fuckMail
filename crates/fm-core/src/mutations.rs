//! Atomic, explicitly requested local changes and live delivery intent.
use crate::{err, fields, id, string, CoreError, Result, Store};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

impl Store {
    pub(crate) fn mutation_receipt(
        &self,
        method: &str,
        account: &str,
        p: &Value,
    ) -> Result<Option<Value>> {
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
            "mail_filing" => &[
                "id",
                "message_ids",
                "expected_revision",
                "request_id",
                "action",
            ],
            "draft_save" => &[
                "conversation_id",
                "draft_id",
                "source_message_id",
                "body",
                "body_html",
                "attachment_ids",
                "signature_text",
                "subject",
                "to_recipients",
                "cc_recipients",
                "bcc_recipients",
                "recipient_inputs",
                "expected_revision",
                "request_id",
            ],
            "draft_discard" => &[
                "conversation_id",
                "draft_id",
                "expected_revision",
                "request_id",
                "buffer",
            ],
            "draft_undo_discard" => &[
                "draft_id",
                "discard_request_id",
                "expected_revision",
                "request_id",
            ],
            "draft_create" => &["mode", "conversation_id", "source_message_id", "request_id"],
            "draft_attachment_import" => &["draft_id", "expected_revision", "request_id", "path"],
            "view_save" => &["view", "expected_revision", "request_id"],
            _ => return Err(err("unsupported_method", "Unknown mutation")),
        };
        fields(p, allowed)?;
        let request_id = string(p, "request_id")?;
        let fingerprint = fingerprint(method, p)?;
        let previous: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT fingerprint,result_json FROM operations WHERE account=?1 AND request_id=?2",
                params![account, request_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((old_fingerprint, result)) = previous {
            let result: Value = serde_json::from_str(&result)?;
            if old_fingerprint != fingerprint {
                // Updated clients bind the displayed member on their first
                // legacy save. Historical receipts did not record that field.
                // Only that added field may be omitted when matching an old
                // receipt; new receipts keep exact source fingerprint checks.
                let old_first_save = method == "draft_save"
                    && p["expected_revision"] == 0
                    && p.get("conversation_id").is_some()
                    && p.get("draft_id").is_none()
                    && p.get("source_message_id").is_some()
                    && result.get("source_message_id").is_none();
                let compatible = if old_first_save {
                    let mut original = p.clone();
                    original
                        .as_object_mut()
                        .unwrap()
                        .remove("source_message_id");
                    old_fingerprint
                        == format!(
                            "{:x}",
                            Sha256::digest(serde_json::to_vec(&json!([method, original]))?)
                        )
                } else {
                    false
                };
                if !compatible {
                    return Err(err(
                        "request_id_conflict",
                        "This request ID already identifies a different command",
                    ));
                }
            }
            return Ok(Some(result));
        }
        Ok(None)
    }

    pub(crate) fn mutate(&self, method: &str, account: &str, p: &Value) -> Result<Value> {
        self.mutate_prepared(method, account, p, None)
    }

    pub(crate) fn mutate_prepared(
        &self,
        method: &str,
        account: &str,
        p: &Value,
        prepared_draft: Option<&Value>,
    ) -> Result<Value> {
        // Capture the eligible cache generation before opening the transaction;
        // uncommitted reads intentionally invalidate reusable projections.
        let flag_projection_stamp = (method == "flag").then(|| self.flag_projection_stamp());
        // The serialized writer rechecks replay before every final mutation.
        let tx = self.db.unchecked_transaction()?;
        if let Some(reply) = self.mutation_receipt(method, account, p)? {
            return Ok(reply);
        }
        self.require_active_account(account)?;
        let request_id = string(p, "request_id")?;
        let fingerprint = fingerprint(method, p)?;
        let expected = if method == "draft_create" {
            0
        } else {
            p["expected_revision"]
                .as_i64()
                .filter(|v| *v >= 0)
                .ok_or_else(|| {
                    err(
                        "invalid_request",
                        "expected_revision must be a nonnegative integer",
                    )
                })?
        };
        let mut result = match method {
            "draft_create" => self.create_composition(account, p)?,
            "flag" | "set_category" | "mail_filing" => {
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
                } else if method == "set_category" {
                    for message in &targets {
                        tx.execute("INSERT OR IGNORE INTO local_model_label_overrides(account,message_id) VALUES(?1,?2)",params![account,message])?;
                        tx.execute("DELETE FROM assist_category_intents WHERE account=?1 AND message_id=?2",params![account,message])?;
                    }

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
                } else {
                    if self.profile_kind()? != crate::live_profile::ProfileKind::Live {
                        return Err(err(
                            "capability_unavailable",
                            "Provider filing requires a live account",
                        ));
                    }
                    let action = string(p, "action")?;
                    let (label, present) = match action {
                        "archive" => ("INBOX", false),
                        "trash" => ("TRASH", true),
                        "restore" => ("TRASH", false),
                        _ => {
                            return Err(err("invalid_request", "Choose archive, trash or restore"))
                        }
                    };
                    for message in &targets {
                        tx.execute("UPDATE messages SET inbox=CASE WHEN ?3='INBOX' THEN ?4 ELSE inbox END,trashed=CASE WHEN ?3='TRASH' THEN ?4 ELSE trashed END,revision=revision+1 WHERE account=?1 AND id=?2",params![account,message,label,present])?;
                        record_manual_intent(&tx, account, message, "label", label, present)?;
                    }
                }
                tx.execute(
                    "UPDATE conversations SET revision=revision+1 WHERE account=?1 AND id=?2",
                    params![account, conversation],
                )?;
                self.conversation(account, conversation)?
            }
            "draft_save" => {
                self.save_composition_prepared(account, p, expected, false, prepared_draft)?
            }
            "draft_discard" => self.discard_composition(account, p, expected)?,
            "draft_undo_discard" => self.undo_discard_composition(account, p, expected)?,
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
        if matches!(method, "flag" | "set_category" | "mail_filing")
            && self.profile_kind()? == crate::live_profile::ProfileKind::Live
        {
            result["delivery"] = json!({"state":"pending","request_id":request_id});
        }
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.execute("INSERT INTO operations(account,request_id,fingerprint,result_json) VALUES(?1,?2,?3,?4)",
            params![account, request_id, fingerprint, serde_json::to_string(&result)?])?;
        crate::google_live_mutations::enqueue(&tx, account, request_id, method, p)?;
        tx.commit()?;
        if let Some(before) = flag_projection_stamp {
            self.refresh_committed_flag_projections(account, string(p, "id")?, before);
        }
        Ok(result)
    }
}

pub(crate) fn fingerprint(method: &str, p: &Value) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&json!([method, p]))?)
    ))
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
            "selectedDraft",
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
            &[
                "sender",
                "exact_sender",
                "kind",
                "status",
                "has_attachment",
                "after",
                "before",
            ],
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
    if let Some(value) = view
        .get("selectedDraft")
        .filter(|v| !v.is_null() && v.as_str() != Some(""))
    {
        let draft = id(value
            .as_str()
            .ok_or_else(|| err("invalid_request", "selectedDraft must be an identifier"))?)?;
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM drafts WHERE account=?1 AND draft_id=?2)",
            params![account, draft],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(err("not_found", "Selected draft is outside this account"));
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
    if view.get("folder").is_some_and(|v| {
        !["inbox", "sent", "all", "trash", "drafts"].contains(&v.as_str().unwrap_or(""))
    }) {
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

    fn snapshot_counts(
        store: &mut Store,
        message_count: i64,
        unread_count: i64,
        starred_count: i64,
        held_conversation_count: i64,
    ) -> Value {
        let accounts = store.handle("accounts", None, json!({})).unwrap();
        let account = accounts["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|account| account["id"] == "windscribe")
            .unwrap();
        for (key, expected) in json!({
            "conversation_count":110,
            "message_count":message_count,
            "unread_count":unread_count,
            "starred_count":starred_count,
            "unread_conversation_count":25,
            "held_conversation_count":held_conversation_count,
            "done_conversation_count":76
        })
        .as_object()
        .unwrap()
        {
            assert_eq!(&account[key], expected, "account {key}");
        }
        let review = store
            .handle(
                "conversations",
                Some("windscribe"),
                json!({"folder":"all","status":"review","grouped":true,"limit":100}),
            )
            .unwrap();
        assert_eq!(review["filtered_conversation_count"], 25);
        assert_eq!(review["conversations"].as_array().unwrap().len(), 25);
        assert_eq!(
            review["account_counts"],
            json!({"conversation_count":110,"unread_conversation_count":25,
                "held_conversation_count":held_conversation_count,"done_conversation_count":76,
                "scope":"all_account_mail_including_archive","held_includes_unread":true})
        );
        review
    }

    #[test]
    fn a_new_reply_does_not_inherit_an_earlier_snapshot_action() {
        for done in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let mut store = Store::open(temp.path()).unwrap();
            let conversation = "windscribe-thread-000";
            let before = store.conversation("windscribe", conversation).unwrap();
            let snapshot = before["message_ids"].clone();
            assert_eq!(
                snapshot,
                json!([
                    "windscribe-message-000-000",
                    "windscribe-message-000-001",
                    "windscribe-message-000-002"
                ])
            );
            let saved_view = store.handle("view_save", Some("windscribe"), json!({
                "request_id":"snapshot-selection","expected_revision":0,
                "view":{"selected":conversation,"selectedMember":"windscribe-message-000-002",
                    "query":"","folder":"all","readerY":42,"listY":17}
            })).unwrap();
            let other_account = store.account_counts("personal").unwrap();
            // Independent fixture oracle: 110 threads, 143 members; 25 unread
            // threads contain 27 unread members. Eleven held threads contain
            // 28 stars; two overlap unread, leaving 76 Done threads.
            snapshot_counts(&mut store, 143, 27, 28, 11);

            // Simulate one already-normalized provider arrival independently
            // of mutations; this unit test does not claim provider transport.
            let tx = store.db.unchecked_transaction().unwrap();
            tx.execute("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash,revision) SELECT account,'arriving-reply',conversation_id,'new-remote-reply',sender_name,sender_email,recipient,subject,'New source reply newsourcearrival','2026-09-11T19:00:00Z',1,0,1,0,source_hash,1 FROM messages WHERE account='windscribe' AND conversation_id=?1 LIMIT 1", [conversation]).unwrap();
            tx.execute("INSERT INTO message_fts(rowid,sender,subject,body) SELECT rowid,sender_name||' '||sender_email,subject,body FROM messages WHERE account='windscribe' AND id='arriving-reply'", []).unwrap();
            tx.execute(
                "UPDATE conversations SET revision=revision+1 WHERE account='windscribe' AND id=?1",
                [conversation],
            )
            .unwrap();
            tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])
                .unwrap();
            tx.commit().unwrap();
            let arrived = store.conversation("windscribe", conversation).unwrap();
            assert_eq!(arrived["messages"].as_array().unwrap().len(), 4);
            let new_member = arrived["messages"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()
                .clone();
            assert_eq!(new_member["id"], "arriving-reply");
            assert_eq!(new_member["unread"], true);
            assert_eq!(new_member["starred"], false);
            assert_eq!(new_member["revision"], 1);
            snapshot_counts(&mut store, 144, 28, 28, 11);

            let mut command = json!({"id":conversation,"message_ids":snapshot,"expected_revision":before["revision"],"request_id":"old-snapshot","read":true});
            if done {
                command["starred"] = json!(false);
            }
            assert_eq!(
                store
                    .handle("flag", Some("windscribe"), command.clone())
                    .unwrap_err()
                    .code,
                "stale_revision"
            );
            assert_eq!(
                store.conversation("windscribe", conversation).unwrap(),
                arrived
            );
            snapshot_counts(&mut store, 144, 28, 28, 11);

            // A deliberate retry updates only the revision, retaining exactly
            // the IDs seen before arrival, including for atomic Done.
            command["expected_revision"] = arrived["revision"].clone();
            let after = store
                .handle("flag", Some("windscribe"), command.clone())
                .unwrap();
            for message in after["messages"].as_array().unwrap() {
                if message["id"] == "arriving-reply" {
                    assert_eq!(message, &new_member);
                } else {
                    assert_eq!(message["unread"], false);
                    assert_eq!(message["starred"], !done);
                }
            }
            let review = snapshot_counts(
                &mut store,
                144,
                25,
                if done { 25 } else { 28 },
                if done { 10 } else { 11 },
            );
            let row = review["conversations"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["id"] == conversation)
                .unwrap();
            assert_eq!(row["group"], "review");
            assert_eq!(row["message_count"], 4);
            assert_eq!(row["unread"], true);
            assert_eq!(row["starred"], !done);
            let matching = store
                .handle(
                    "conversations",
                    Some("windscribe"),
                    json!({
                        "folder":"all","query":"newsourcearrival","field":"body","grouped":true
                    }),
                )
                .unwrap();
            assert_eq!(matching["filtered_conversation_count"], 1);
            assert_eq!(matching["conversations"][0]["id"], conversation);
            assert_eq!(
                matching["conversations"][0]["matching_message_ids"],
                json!(["arriving-reply"])
            );
            assert_eq!(
                store
                    .handle("view_get", Some("windscribe"), json!({}))
                    .unwrap(),
                saved_view
            );
            assert_eq!(store.account_counts("personal").unwrap(), other_account);
            assert_eq!(
                store.handle("flag", Some("windscribe"), command).unwrap(),
                after
            );
        }
    }
}
