//! Revision-bound local composition preparation. Plans and proofs are opaque:
//! only this module can admit verified content to the single SQLite writer.
use crate::{capture, composition, content, err, id, mutations, string, Result, Store};
use fm_google::transport::CancellationToken;
use fm_mail::StagedAttachment;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub enum LocalContentStep {
    Reply(Value),
    Prepare(LocalContentPlan),
}

/// No public constructor, serialization, credential, open file or SQLite handle.
pub struct LocalContentPlan(Box<PlanContents>);

struct PlanContents {
    blobs: content::BlobStore,
    account: String,
    lifecycle_revision: i64,
    method: String,
    params: Value,
    snapshot: Option<DraftSnapshot>,
    attachments: Vec<AttachmentRef>,
    capture: Option<capture::CaptureRecord>,
}

pub struct PreparedLocalContent {
    plan: Box<PlanContents>,
    draft: Option<Value>,
    imported: Option<AttachmentRef>,
    capture: Option<capture::PreparedCapture>,
    cancel: CancellationToken,
}

struct DraftSnapshot {
    stamp: Value,
    draft: Value,
    legacy_source: Option<LegacySource>,
}

struct LegacySource {
    from: String,
    title: String,
    message: Option<(String, String, String, String)>,
}

#[derive(Clone, PartialEq, Eq)]
struct AttachmentRef {
    id: String,
    name: String,
    mime_type: String,
    size: u64,
    hash: String,
}

/// SQL-only frozen saved-draft selection shared by capture and provider drafts.
/// Its source and attachments are verified only by the blocking preparation.
pub(crate) struct RetainedDraft {
    pub(crate) blobs: content::BlobStore,
    account: String,
    lifecycle_revision: i64,
    params: Value,
    snapshot: DraftSnapshot,
    attachments: Vec<AttachmentRef>,
}

impl RetainedDraft {
    pub(crate) fn draft(&self) -> &Value {
        &self.snapshot.draft
    }
    pub(crate) fn prepare(&self, cancel: &CancellationToken) -> Result<Vec<StagedAttachment>> {
        cancelled(cancel)?;
        let draft = self.draft();
        if draft["discarded"] == true {
            return Err(err("invalid_draft", "Discarded draft cannot be uploaded"));
        }
        if draft["recipient_inputs"]
            .as_object()
            .is_some_and(|inputs| !inputs.is_empty())
        {
            return Err(err(
                "invalid_draft",
                "Finish the recipient addresses before uploading or sending this draft",
            ));
        }
        if draft["mode"] != "new" {
            if draft["source_unavailable"] == true {
                return Err(err(
                    "content_unavailable",
                    "Original composition routing is unavailable",
                ));
            }
            read_source(
                &self.blobs,
                draft["source_hash"].as_str().ok_or_else(invalid_proof)?,
            )?;
        }
        self.attachments
            .iter()
            .map(|file| {
                cancelled(cancel)?;
                Ok(StagedAttachment {
                    account_id: self.account.clone(),
                    filename: file.name.clone(),
                    content_type: file.mime_type.clone(),
                    bytes: read_blob(&self.blobs, &file.hash, file.size)?,
                    sha256: file.hash.clone(),
                    content_id: None,
                })
            })
            .collect()
    }
}

impl Store {
    pub(crate) fn retained_draft(&self, account: &str, params: Value) -> Result<RetainedDraft> {
        self.check_draft_writable(account, &params)?;
        let snapshot = self.local_draft_snapshot(account, &params)?;
        if snapshot.draft["revision"].as_i64().unwrap_or(0) <= 0 {
            return Err(err(
                "invalid_draft",
                "Save the local draft before uploading it",
            ));
        }
        let attachments = self.local_attachment_refs(account, &snapshot.draft["attachment_ids"])?;
        Ok(RetainedDraft {
            blobs: self.google_blob_store(),
            account: account.into(),
            lifecycle_revision: self.require_active_account(account)?,
            params,
            snapshot,
            attachments,
        })
    }

    pub(crate) fn check_retained_draft(&self, retained: &RetainedDraft) -> Result<()> {
        if retained.blobs.profile != self.profile {
            return Err(err(
                "scope_mismatch",
                "Draft proof belongs to another profile",
            ));
        }
        self.check_active_account_revision(&retained.account, retained.lifecycle_revision)?;
        self.check_draft_writable(&retained.account, &retained.params)?;
        let current = self.local_draft_snapshot(&retained.account, &retained.params)?;
        if current.stamp != retained.snapshot.stamp
            || self.local_attachment_refs(&retained.account, &current.draft["attachment_ids"])?
                != retained.attachments
        {
            return Err(err(
                "stale_revision",
                "Draft source, identity or attachments changed during preparation",
            ));
        }
        Ok(())
    }
    pub fn local_content_begin(
        &self,
        method: &str,
        account: &str,
        params: Value,
    ) -> Result<LocalContentStep> {
        if !matches!(
            method,
            "draft_attachment_import" | "draft_save" | "draft_capture"
        ) {
            return Err(err(
                "unsupported_method",
                "This method has no local content preparation",
            ));
        }
        if !params.is_object() {
            return Err(err("invalid_request", "Parameters must be an object"));
        }
        if method == "draft_capture"
            && self.profile_kind()? == crate::live_profile::ProfileKind::Live
        {
            return Err(err(
                "capability_unavailable",
                "This operation is not available for a live account yet",
            ));
        }
        id(account)?;
        if !self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=?1)",
            [account],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(err("not_found", "Account does not exist in this profile"));
        }
        let existing_capture = if method == "draft_capture" {
            self.capture_existing(account, &params)?
        } else {
            // Includes historical first-save compatibility. In particular this
            // must precede even stat/open of the import path or retained blobs.
            if let Some(reply) = self.mutation_receipt(method, account, &params)? {
                return Ok(LocalContentStep::Reply(reply));
            }
            None
        };
        if existing_capture.is_none() {
            self.check_draft_writable(account, &params)?;
        }
        if method == "draft_attachment_import" {
            let path = params["path"]
                .as_str()
                .ok_or_else(|| err("invalid_request", "An absolute local file path is required"))?;
            if path.len() > 128 * 1024
                || path.chars().any(char::is_control)
                || !Path::new(path).is_absolute()
            {
                return Err(err(
                    "invalid_request",
                    "An absolute local file path is required",
                ));
            }
            string(&params, "draft_id")?;
            if params["expected_revision"]
                .as_i64()
                .is_none_or(|revision| revision <= 0)
            {
                return Err(err(
                    "invalid_request",
                    "Import requires a positive draft revision",
                ));
            }
        }
        let snapshot = if existing_capture.is_none() {
            Some(self.local_draft_snapshot(account, &params)?)
        } else {
            None
        };
        let attachments = if let Some(snapshot) = &snapshot {
            let selected = if method == "draft_save" {
                params
                    .get("attachment_ids")
                    .unwrap_or(&snapshot.draft["attachment_ids"])
            } else {
                &snapshot.draft["attachment_ids"]
            };
            self.local_attachment_refs(account, selected)?
        } else {
            Vec::new()
        };
        if method == "draft_attachment_import" {
            if snapshot
                .as_ref()
                .is_some_and(|s| s.draft["discarded"] == true)
            {
                return Err(err(
                    "invalid_draft",
                    "Discarded draft cannot receive an attachment",
                ));
            }
            if attachments.len() >= 50 {
                return Err(err(
                    "invalid_request",
                    "Draft supports at most 50 attachments",
                ));
            }
        }
        Ok(LocalContentStep::Prepare(LocalContentPlan(Box::new(
            PlanContents {
                blobs: self.google_blob_store(),
                account: account.into(),
                lifecycle_revision: self.account_lifecycle(account)?["revision"]
                    .as_i64()
                    .ok_or_else(invalid_proof)?,
                method: method.into(),
                params,
                snapshot,
                attachments,
                capture: existing_capture,
            },
        ))))
    }

    pub fn local_content_finish(
        &mut self,
        prepared: PreparedLocalContent,
    ) -> Result<LocalContentStep> {
        let PreparedLocalContent {
            plan,
            draft,
            imported,
            capture,
            cancel,
        } = prepared;
        cancelled(&cancel)?;
        if plan.blobs.profile != self.profile {
            return Err(err(
                "scope_mismatch",
                "Content proof belongs to another profile",
            ));
        }
        // A winner's receipt takes precedence over the current draft, including
        // a later edit/discard or removal of its original source file.
        if plan.method != "draft_capture" {
            if let Some(reply) = self.mutation_receipt(&plan.method, &plan.account, &plan.params)? {
                return Ok(LocalContentStep::Reply(reply));
            }
        } else if self
            .capture_existing(&plan.account, &plan.params)?
            .is_some()
        {
            return match self.finish_prepared_capture(
                &plan.account,
                &plan.params,
                capture.ok_or_else(invalid_proof)?,
            )? {
                Some(reply) => Ok(LocalContentStep::Reply(reply)),
                None => self.local_content_begin(&plan.method, &plan.account, plan.params),
            };
        }
        self.check_active_account_revision(&plan.account, plan.lifecycle_revision)?;
        self.check_draft_writable(&plan.account, &plan.params)?;
        let snapshot = plan.snapshot.as_ref().ok_or_else(invalid_proof)?;
        let current = self.local_draft_snapshot(&plan.account, &plan.params)?;
        if current.stamp != snapshot.stamp {
            return Err(err("stale_revision","Draft source or account identity changed during content preparation; reload before retrying"));
        }
        let selected = if plan.method == "draft_save" {
            plan.params
                .get("attachment_ids")
                .unwrap_or(&current.draft["attachment_ids"])
        } else {
            &current.draft["attachment_ids"]
        };
        if self.local_attachment_refs(&plan.account, selected)? != plan.attachments {
            return Err(err(
                "stale_revision",
                "Selected attachment content changed during preparation",
            ));
        }
        let draft = draft.as_ref().ok_or_else(invalid_proof)?;
        match plan.method.as_str() {
            "draft_save" => Ok(LocalContentStep::Reply(self.mutate_prepared(
                "draft_save",
                &plan.account,
                &plan.params,
                Some(draft),
            )?)),
            "draft_attachment_import" => {
                let file = imported.ok_or_else(invalid_proof)?;
                let tx = self.db.unchecked_transaction()?;
                let mut ids = draft["attachment_ids"]
                    .as_array()
                    .ok_or_else(invalid_proof)?
                    .clone();
                ids.push(json!(file.id));
                let revision = draft["revision"]
                    .as_i64()
                    .ok_or_else(invalid_proof)?
                    .checked_add(1)
                    .ok_or_else(|| err("limit_exceeded", "Draft revision exhausted"))?;
                let updated_at = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| err("invalid_request", "System clock cannot date a draft"))?
                    .as_secs();
                tx.execute("INSERT INTO attachments(account,id,message_id,name,mime_type,size,state,blob_hash) VALUES(?1,?2,NULL,?3,?4,?5,'available',?6)",params![plan.account,file.id,file.name,file.mime_type,file.size,file.hash])?;
                tx.execute("UPDATE drafts SET attachments_json=?3,revision=?4,updated_at=?5 WHERE account=?1 AND draft_id=?2",params![plan.account,draft["draft_id"].as_str(),serde_json::to_string(&ids)?,revision,updated_at])?;
                let updated =
                    self.composition(&plan.account, &json!({"draft_id":draft["draft_id"]}))?;
                let reply = json!({"draft":updated,"attachment":{"account":plan.account,"id":file.id,"message_id":null,"name":file.name,"mime_type":file.mime_type,"size":file.size,"state":"available","sha256":file.hash}});
                tx.execute("INSERT INTO operations(account,request_id,fingerprint,result_json) VALUES(?1,?2,?3,?4)",params![plan.account,string(&plan.params,"request_id")?,mutations::fingerprint(&plan.method,&plan.params)?,serde_json::to_string(&reply)?])?;
                tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
                tx.commit()?;
                Ok(LocalContentStep::Reply(reply))
            }
            "draft_capture" => match self.finish_prepared_capture(
                &plan.account,
                &plan.params,
                capture.ok_or_else(invalid_proof)?,
            )? {
                Some(reply) => Ok(LocalContentStep::Reply(reply)),
                None => self.local_content_begin(&plan.method, &plan.account, plan.params),
            },
            _ => Err(invalid_proof()),
        }
    }

    fn local_draft_snapshot(&self, account: &str, p: &Value) -> Result<DraftSnapshot> {
        let (selected, legacy) = composition::selector(p)?;
        let raw = self.stored_composition(account, p)?;
        let identity = self.draft_identity(account)?;
        let from: String =
            self.db
                .query_row("SELECT email FROM accounts WHERE id=?1", [account], |r| {
                    r.get(0)
                })?;
        let (draft, source_stamp, legacy_source) = if let Some(row) = raw.clone() {
            (self.project_composition(account, row)?, Value::Null, None)
        } else if legacy {
            let title: String = self
                .db
                .query_row(
                    "SELECT subject FROM conversations WHERE account=?1 AND id=?2",
                    params![account, selected],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or_else(|| err("not_found", "Conversation is outside this account"))?;
            let source = p
                .get("source_message_id")
                .map(|v| {
                    v.as_str()
                        .ok_or_else(|| {
                            err("invalid_request", "source_message_id must be an identifier")
                        })
                        .and_then(id)
                })
                .transpose()?;
            let message:Option<(String,String,String,String)>=self.db.query_row("SELECT id,source_hash,sender_email,recipient FROM messages WHERE account=?1 AND conversation_id=?2 AND (?4 IS NULL OR id=?4) ORDER BY (sender_email<>?3) DESC,julianday(sent_at) DESC,id DESC LIMIT 1",params![account,selected,from,source],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
            if source.is_some() && message.is_none() {
                return Err(err(
                    "not_found",
                    "Reply source is outside this account and conversation",
                ));
            }
            let recipient = message.as_ref().map(|m| {
                if m.2 == from {
                    m.3.clone()
                } else {
                    m.2.clone()
                }
            });
            let routing = json!({"from":from,"subject":format!("Re: {title}"),"to_recipients":recipient.map(|email|vec![json!({"email":email})]).unwrap_or_default(),"cc_recipients":[],"bcc_recipients":[],"reply_to_message_id":null,"references":[],"reply_all_available":false,"source_unavailable":true});
            let row = json!({"draft_id":selected,"conversation_id":selected,"mode":"reply","source_message_id":message.as_ref().map(|m|&m.0),"source_hash":message.as_ref().map(|m|&m.1),"routing_json":serde_json::to_string(&routing)?,"body":"","body_html":null,"attachments_json":"[]","revision":0,"discarded":false,"signature_text":identity.1,"from_name":identity.0,"identity_revision":identity.2,"created_at":0,"updated_at":0,"legacy_reply":true});
            (
                self.project_composition(account, row)?,
                json!([title, message]),
                Some(LegacySource {
                    from: from.clone(),
                    title,
                    message,
                }),
            )
        } else {
            return Err(err("not_found", "Draft is outside this account"));
        };
        let expected = p["expected_revision"]
            .as_i64()
            .filter(|r| *r >= 0)
            .ok_or_else(|| {
                err(
                    "invalid_request",
                    "expected_revision must be a nonnegative integer",
                )
            })?;
        mutations::check_revision(
            expected,
            draft["revision"].as_i64().ok_or_else(invalid_proof)?,
        )?;
        if let Some(source) = p.get("source_message_id") {
            let source = id(source.as_str().ok_or_else(|| {
                err("invalid_request", "source_message_id must be an identifier")
            })?)?;
            if draft["source_message_id"].as_str() != Some(source) {
                return Err(err(
                    "invalid_request",
                    "Saved draft source cannot be retargeted",
                ));
            }
        }
        Ok(DraftSnapshot {
            stamp: json!([raw, identity, from, source_stamp]),
            draft,
            legacy_source,
        })
    }

    pub(crate) fn check_retained_attachment_selection(
        &self,
        account: &str,
        selected: &Value,
    ) -> Result<()> {
        self.local_attachment_refs(account, selected).map(|_| ())
    }

    fn local_attachment_refs(&self, account: &str, selected: &Value) -> Result<Vec<AttachmentRef>> {
        let ids = selected
            .as_array()
            .filter(|ids| ids.len() <= 50)
            .ok_or_else(|| err("invalid_request", "Draft supports at most 50 attachments"))?;
        let mut seen = std::collections::HashSet::new();
        let mut total = 0u64;
        let mut files = Vec::new();
        for value in ids {
            let aid = id(value
                .as_str()
                .ok_or_else(|| err("invalid_request", "Attachment ID must be a string"))?)?;
            if !seen.insert(aid) {
                return Err(err("invalid_request", "Draft repeats an attachment"));
            }
            let row:Option<(String,String,i64,String,String)>=self.db.query_row("SELECT name,mime_type,size,state,blob_hash FROM attachments WHERE account=?1 AND id=?2",params![account,aid],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
            let (name, mime_type, size, state, hash) =
                row.ok_or_else(|| err("not_found", "Draft attachment is outside this account"))?;
            let size = u64::try_from(size)
                .map_err(|_| err("content_unavailable", "Draft attachment size is invalid"))?;
            total = total
                .checked_add(size)
                .filter(|total| *total <= fm_mail::MAX_ATTACHMENT_BYTES as u64)
                .ok_or_else(|| err("invalid_request", "Draft attachments exceed 25 MiB"))?;
            if state != "available" {
                return Err(err(
                    "content_unavailable",
                    "Draft attachment is not fully retained",
                ));
            }
            files.push(AttachmentRef {
                id: aid.into(),
                name,
                mime_type,
                size,
                hash,
            });
        }
        Ok(files)
    }
}

/// Run only on a bounded blocking worker. Cancellation never makes the caller's
/// worker/memory permit disappear while its regular-file I/O is still running.
pub fn prepare_local_content(
    plan: LocalContentPlan,
    cancel: &CancellationToken,
) -> Result<PreparedLocalContent> {
    prepare_after_import_metadata(plan, cancel, || Ok(()))
}

// Private observer permits deterministic file-change tests without a caller-
// constructible proof or any extra work on the serialized writer.
fn prepare_after_import_metadata(
    plan: LocalContentPlan,
    cancel: &CancellationToken,
    after_import_metadata: impl FnOnce() -> Result<()>,
) -> Result<PreparedLocalContent> {
    let plan = plan.0;
    cancelled(cancel)?;
    let mut draft = plan.snapshot.as_ref().map(|s| s.draft.clone());
    if let (Some(snapshot), Some(draft)) = (&plan.snapshot, &mut draft) {
        if let Some(source) = &snapshot.legacy_source {
            if let Some((_, hash, sender, recipient)) = &source.message {
                // Legacy first-save historically preserves a local draft even
                // when its received routing cannot be recovered. Keep that
                // source_unavailable fallback; capture still refuses it.
                if let Ok(bytes) = read_source(&plan.blobs, hash) {
                    if let Ok((mut routing, _)) =
                        composition::routing_from_source(&bytes, &source.from, "reply", "")
                    {
                        // Reply-To (else From) as the source names it; the
                        // stored sender only when it names nobody.
                        if routing["to_recipients"].as_array().is_none_or(|rows| rows.is_empty()) {
                            routing["to_recipients"] =
                                json!([{"email":if sender==&source.from {recipient}else{sender}}]);
                        }
                        routing["subject"] = json!(format!("Re: {}", source.title));
                        for (key, value) in routing.as_object().ok_or_else(invalid_proof)? {
                            draft[key] = value.clone();
                        }
                    }
                }
            }
        }
        if plan.method == "draft_capture" && draft["mode"] != "new" {
            let hash = draft["source_hash"].as_str().ok_or_else(|| {
                err(
                    "content_unavailable",
                    "Original composition source is unavailable",
                )
            })?;
            read_source(&plan.blobs, hash)?;
        }
    }
    cancelled(cancel)?;
    let mut parts = Vec::new();
    for file in &plan.attachments {
        cancelled(cancel)?;
        if plan.method == "draft_capture" {
            let bytes = read_blob(&plan.blobs, &file.hash, file.size)?;
            parts.push(StagedAttachment {
                account_id: plan.account.clone(),
                filename: file.name.clone(),
                content_type: file.mime_type.clone(),
                bytes,
                sha256: file.hash.clone(),
                content_id: None,
            });
        } else {
            plan.blobs.verified_blob(&file.hash, file.size)?;
        }
    }
    let imported = if plan.method == "draft_attachment_import" {
        let path = Path::new(plan.params["path"].as_str().ok_or_else(invalid_proof)?);
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control))
            .ok_or_else(|| {
                err(
                    "invalid_request",
                    "Attachment filename is not a bounded single line",
                )
            })?
            .to_owned();
        let limit = fm_mail::MAX_ATTACHMENT_BYTES as u64
            - plan.attachments.iter().map(|f| f.size).sum::<u64>();
        let mut file = content::open_regular(path)?;
        let before = file.metadata()?;
        if before.len() > limit {
            return Err(err("invalid_request", "Draft attachments exceed 25 MiB"));
        }
        after_import_metadata()?;
        let mut bytes = Vec::new();
        (&mut file).take(limit + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > limit {
            return Err(err("invalid_request", "Draft attachments exceed 25 MiB"));
        }
        let after = file.metadata()?;
        // Detect short copies and observable edits on the opened handle. These
        // checks do not claim an atomic filesystem snapshot on every platform.
        if bytes.len() as u64 != before.len()
            || after.len() != before.len()
            || after.modified()? != before.modified()?
        {
            return Err(err(
                "content_unavailable",
                "Attachment file changed during import; retry after it finishes changing",
            ));
        }
        cancelled(cancel)?;
        let hash = plan.blobs.stage_blob(&bytes)?;
        Some(AttachmentRef {
            id: format!(
                "local-attachment-{:x}",
                Sha256::digest(
                    format!("{}\0{}", plan.account, string(&plan.params, "request_id")?).as_bytes()
                )
            ),
            name,
            mime_type: "application/octet-stream".into(),
            size: bytes.len() as u64,
            hash,
        })
    } else {
        None
    };
    let capture = if plan.method == "draft_capture" {
        Some(capture::prepare_capture_content(
            &plan.blobs,
            &plan.account,
            &plan.params,
            plan.capture.clone(),
            draft.as_ref(),
            &parts,
        )?)
    } else {
        None
    };
    cancelled(cancel)?;
    Ok(PreparedLocalContent {
        plan,
        draft,
        imported,
        capture,
        cancel: cancel.clone(),
    })
}

pub(crate) fn read_source(blobs: &content::BlobStore, hash: &str) -> Result<Vec<u8>> {
    // Verify the hash syntax before ever joining a stored source reference.
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(err(
            "content_unavailable",
            "Original composition source is unavailable",
        ));
    }
    let size = std::fs::symlink_metadata(blobs.profile.join("blobs").join(hash))
        .map_err(|_| {
            err(
                "content_unavailable",
                "Original composition source is unavailable",
            )
        })?
        .len();
    read_blob(blobs, hash, size)
}

pub(crate) fn read_blob(blobs: &content::BlobStore, hash: &str, size: u64) -> Result<Vec<u8>> {
    let path = blobs.verified_blob(hash, size)?;
    let mut bytes = Vec::new();
    content::open_regular(&path)?
        .take(size + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 != size || format!("{:x}", Sha256::digest(&bytes)) != hash {
        return Err(err(
            "content_unavailable",
            "Retained content failed verification",
        ));
    }
    Ok(bytes)
}

fn cancelled(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        Err(err("cancelled", "Local content preparation was cancelled"))
    } else {
        Ok(())
    }
}
fn invalid_proof() -> crate::CoreError {
    err(
        "invalid_request",
        "Local content preparation is inconsistent",
    )
}

#[cfg(test)]
mod tests;
