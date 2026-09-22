//! Durable, revision-bound Undo for local draft discard. Provider state is not
//! restored from a historical snapshot; existing provider recovery owns it.
use crate::{composition, err, fields, mutations, string, Result, Store};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

pub(crate) fn recipient_inputs(value: &Value) -> Result<Value> {
    fields(value, &["to_recipients", "cc_recipients", "bcc_recipients"])?;
    let mut result = json!({});
    for (field, value) in value.as_object().unwrap() {
        let text = value
            .as_str()
            .filter(|text| text.len() <= 16 * 1024 && !text.contains('\0'))
            .ok_or_else(|| {
                err(
                    "invalid_request",
                    "Unfinished recipient input must be text of at most 16 KiB",
                )
            })?;
        if !text.trim().is_empty() {
            result[field] = json!(text);
        }
    }
    Ok(result)
}

impl Store {
    pub(crate) fn discard_composition(
        &self,
        account: &str,
        p: &Value,
        expected: i64,
    ) -> Result<Value> {
        self.check_draft_writable(account, p)?;
        let saved = self.composition(account, p)?;
        mutations::check_revision(expected, saved["revision"].as_i64().unwrap())?;
        let lifecycle = self.require_active_account(account)?;
        let snapshot = if saved["discarded"] != true && expected > 0 {
            let mut raw = self
                .stored_composition(account, p)?
                .ok_or_else(|| err("not_found", "Saved draft is unavailable"))?;
            if let Some(buffer) = p.get("buffer") {
                self.apply_discard_buffer(account, &mut raw, buffer)?;
            }
            Some(raw)
        } else {
            if p.get("buffer").is_some() {
                return Err(err(
                    "invalid_draft",
                    "Open a saved, active draft before discarding its editor buffer",
                ));
            }
            None
        };
        let mut result =
            self.save_composition_prepared(account, p, expected, true, Some(&saved))?;
        if let Some(snapshot) = snapshot {
            let request = string(p, "request_id")?;
            self.db.execute("INSERT INTO draft_discards(account,request_id,draft_id,discarded_revision,lifecycle_revision,snapshot_json) VALUES(?1,?2,?3,?4,?5,?6)",
                params![account,request,result["draft_id"].as_str(),result["revision"].as_i64(),lifecycle,serde_json::to_string(&snapshot)?])?;
            result["discard_undo"] = json!({"version":1,"request_id":request,"draft_id":result["draft_id"],"revision":result["revision"],"lifecycle_revision":lifecycle});
        }
        Ok(result)
    }

    fn apply_discard_buffer(&self, account: &str, raw: &mut Value, buffer: &Value) -> Result<()> {
        fields(
            buffer,
            &[
                "body",
                "body_html",
                "signature_text",
                "subject",
                "to_recipients",
                "cc_recipients",
                "bcc_recipients",
                "attachment_ids",
                "recipient_inputs",
            ],
        )?;
        for (field, limit) in [
            ("body", composition::MAX_BODY),
            ("signature_text", 16 * 1024),
        ] {
            if let Some(value) = buffer.get(field) {
                let text = value
                    .as_str()
                    .filter(|text| {
                        text.len() <= limit && (field != "signature_text" || !text.contains('\0'))
                    })
                    .ok_or_else(|| {
                        err(
                            "invalid_request",
                            "Draft editor text exceeds its saved-content bound",
                        )
                    })?;
                raw[field] = json!(text);
            }
        }
        if let Some(html) = buffer.get("body_html") {
            if !html.is_null()
                && html
                    .as_str()
                    .is_none_or(|text| text.len() > composition::MAX_BODY)
            {
                return Err(err(
                    "invalid_request",
                    "Draft HTML must be null or text of at most 256 KiB",
                ));
            }
            raw["body_html"] = html.clone();
        } else if buffer.get("body").is_some() {
            // A caller that replaces plain text without HTML cannot retain a
            // different, old rich-text body in the restored draft.
            raw["body_html"] = Value::Null;
        }
        let mut routing: Value = serde_json::from_str(raw["routing_json"].as_str().unwrap())?;
        for field in [
            "subject",
            "to_recipients",
            "cc_recipients",
            "bcc_recipients",
        ] {
            if let Some(value) = buffer.get(field) {
                routing[field] = value.clone();
            }
        }
        composition::validate_routing(&routing)?;
        raw["routing_json"] = json!(serde_json::to_string(&routing)?);
        if let Some(selected) = buffer.get("attachment_ids") {
            let original: Value = serde_json::from_str(raw["attachments_json"].as_str().unwrap())?;
            // Already-selected references remain restorable even if their bytes
            // later become unavailable. A new selection must belong here.
            if original != *selected {
                self.check_retained_attachment_selection(account, selected)?;
            }
            raw["attachments_json"] = json!(serde_json::to_string(selected)?);
        }
        if let Some(inputs) = buffer.get("recipient_inputs") {
            raw["recipient_inputs_json"] =
                json!(serde_json::to_string(&recipient_inputs(inputs)?)?);
        }
        Ok(())
    }

    pub(crate) fn undo_discard_composition(
        &self,
        account: &str,
        p: &Value,
        expected: i64,
    ) -> Result<Value> {
        self.check_draft_writable(account, p)?;
        let draft_id = string(p, "draft_id")?;
        let request = string(p, "discard_request_id")?;
        let saved: Option<(String,i64,i64,String)> = self.db.query_row(
            "SELECT draft_id,discarded_revision,lifecycle_revision,snapshot_json FROM draft_discards WHERE account=?1 AND request_id=?2",
            params![account,request], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?;
        let (original_id, revision, lifecycle, encoded) = saved.ok_or_else(|| {
            err(
                "not_found",
                "This account has no retained Undo for that discard",
            )
        })?;
        if original_id != draft_id {
            return Err(err(
                "invalid_request",
                "Undo must name the original discarded draft",
            ));
        }
        mutations::check_revision(expected, revision)?;
        if self.require_active_account(account)? != lifecycle {
            return Err(err(
                "stale_revision",
                "The account changed after this discard",
            ));
        }
        let current = self
            .stored_composition(account, p)?
            .ok_or_else(|| err("not_found", "Discarded draft is unavailable"))?;
        mutations::check_revision(expected, current["revision"].as_i64().unwrap())?;
        if current["discarded"] != true {
            return Err(err("stale_revision", "This draft is no longer discarded"));
        }
        let snapshot: Value = serde_json::from_str(&encoded)?;
        for field in [
            "draft_id",
            "conversation_id",
            "mode",
            "source_message_id",
            "source_hash",
            "legacy_reply",
            "origin",
            "created_at",
        ] {
            if current[field] != snapshot[field] {
                return Err(err("stale_revision", "The original draft identity changed"));
            }
        }
        // Restore only authored fields and their frozen From display identity.
        // Google baselines, Send/deletion receipts and transfer state stay current.
        self.db.execute("UPDATE drafts SET body=?3,body_html=?4,signature_text=?5,attachments_json=?6,routing_json=?7,from_name=?8,identity_revision=?9,recipient_inputs_json=?10,discarded=0,revision=?11,updated_at=?12 WHERE account=?1 AND draft_id=?2",
            params![account,draft_id,snapshot["body"].as_str(),snapshot["body_html"].as_str(),snapshot["signature_text"].as_str(),snapshot["attachments_json"].as_str(),snapshot["routing_json"].as_str(),snapshot["from_name"].as_str(),snapshot["identity_revision"].as_i64(),snapshot["recipient_inputs_json"].as_str().unwrap_or("{}"),expected+1,composition::now()?])?;
        let mut result = self.composition(account, &json!({"draft_id":draft_id}))?;
        result["undone_discard_request_id"] = json!(request);
        Ok(result)
    }

    pub(crate) fn draft_discard_status(&self, account: &str, p: &Value) -> Result<Value> {
        fields(p, &["request_id"])?;
        let request = string(p, "request_id")?;
        let saved: Option<(String,String,i64,i64)> = self.db.query_row(
            "SELECT o.result_json,d.draft_id,d.discarded_revision,d.lifecycle_revision FROM draft_discards d JOIN operations o ON o.account=d.account AND o.request_id=d.request_id WHERE d.account=?1 AND d.request_id=?2",
            params![account,request], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?;
        let (encoded, draft_id, revision, lifecycle) = saved.ok_or_else(|| {
            err(
                "not_found",
                "No retained discard was found for this request",
            )
        })?;
        let current = self.composition(account, &json!({"draft_id":draft_id}))?;
        let receipt: Value = serde_json::from_str(&encoded)?;
        Ok(
            json!({"account":account,"request_id":request,"draft_id":draft_id,"discard":receipt,
            "current_revision":current["revision"],"can_undo":current["discarded"]==true && current["revision"]==revision && current["read_only"]!=true && self.account_lifecycle(account)?["revision"]==lifecycle}),
        )
    }
}

#[cfg(test)]
#[path = "draft_discard_tests.rs"]
mod tests;
