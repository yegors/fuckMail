//! Durable bridge from Google's replayable page plans to the one Core store.
//! Only this adapter publishes staged messages and advances their checkpoint.
use crate::{err, Result, Store};
use fm_google::sync::{Checkpoint, MessageContent, PagePlan, PageStore};
use fm_google::{AccountId, GoogleError};
use mail_parser::MimeHeaders;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

#[cfg(test)]
#[path = "google_store_tests.rs"]
mod tests;

pub struct GooglePageStore<'a> {
    store: &'a mut Store,
}

impl<'a> GooglePageStore<'a> {
    pub fn new(store: &'a mut Store) -> Self {
        Self { store }
    }

    fn stage(
        &mut self,
        expected: &Checkpoint,
        content: MessageContent,
    ) -> std::result::Result<(), GoogleError> {
        self.ensure_checkpoint(expected)?;
        let generation =
            i64::try_from(expected.generation).map_err(|_| GoogleError::StaleCheckpoint)?;
        let revision =
            i64::try_from(expected.revision).map_err(|_| GoogleError::StaleCheckpoint)?;
        if crate::id(&content.id).is_err()
            || crate::id(&content.thread_id).is_err()
            || content.source_mime.len() > 40 * 1024 * 1024
            || content.label_ids.len() > 1000
            || content.label_ids.iter().any(|id| crate::id(id).is_err())
            || content.history_id.parse::<u64>().is_err()
        {
            return Err(GoogleError::InvalidResponse(
                "Invalid staged message identity or bounds",
            ));
        }
        let staged = prepare_message(&content, |bytes| {
            self.store.stage_blob(bytes).map_err(storage_error)
        })?;
        let source_hash = self
            .store
            .stage_blob(&content.source_mime)
            .map_err(storage_error)?;
        // Source and attachment files are fully published before this short write.
        self.ensure_checkpoint(expected)?;
        self.store.db.execute("INSERT INTO google_staged_messages(account,generation,remote_id,expected_revision,json,source_hash,source_size) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(account,generation,remote_id) DO UPDATE SET expected_revision=excluded.expected_revision,json=excluded.json,source_hash=excluded.source_hash,source_size=excluded.source_size",
            params![expected.account.as_str(),generation,staged.remote_id,revision,serde_json::to_string(&staged).map_err(storage_error)?,source_hash,content.source_mime.len() as i64]).map_err(storage_error)?;
        Ok(())
    }

    fn ensure_checkpoint(&self, expected: &Checkpoint) -> std::result::Result<(), GoogleError> {
        self.store
            .require_active_account(expected.account.as_str())
            .map_err(storage_error)?;
        if self
            .store
            .google_checkpoint(expected.account.as_str())
            .map_err(storage_error)?
            .as_ref()
            != Some(expected)
        {
            return Err(GoogleError::StaleCheckpoint);
        }
        Ok(())
    }
}

impl Store {
    pub fn google_map_category(
        &self,
        account: &str,
        category: &str,
        provider_label: &str,
    ) -> Result<()> {
        self.require_active_account(account)?;
        crate::id(account)?;
        crate::id(category)?;
        crate::id(provider_label)?;
        if is_system_label(provider_label) {
            return Err(err(
                "invalid_request",
                "Category mappings cannot target Gmail system labels",
            ));
        }
        self.db.execute("INSERT INTO google_category_labels(account,category_id,provider_label_id) VALUES(?1,?2,?3) ON CONFLICT(account,category_id) DO UPDATE SET provider_label_id=excluded.provider_label_id",params![account,category,provider_label])?;
        Ok(())
    }
    pub fn google_checkpoint(&self, account: &str) -> Result<Option<Checkpoint>> {
        crate::id(account)?;
        let raw: Option<String> = self
            .db
            .query_row(
                "SELECT json FROM google_checkpoints WHERE account=?1",
                [account],
                |r| r.get(0),
            )
            .optional()?;
        raw.map(|json| serde_json::from_str(&json).map_err(Into::into))
            .transpose()
    }

    /// Start a synthetic provider import only for an existing local account.
    /// Repeated setup cannot reset an existing checkpoint or overwrite intent.
    pub fn google_begin_full(&mut self, account: &str, baseline: &str) -> Result<Checkpoint> {
        self.require_active_account(account)?;
        if let Some(checkpoint) = self.google_checkpoint(account)? {
            return Ok(checkpoint);
        }
        let exists: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=?1)",
            [account],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(err("not_found", "Google account is not in this profile"));
        }
        let checkpoint = Checkpoint::begin_full(
            AccountId::new(account)
                .map_err(|_| err("invalid_request", "Invalid Google account identity"))?,
            1,
            baseline.to_owned(),
        )
        .map_err(|_| err("invalid_request", "Invalid Google history baseline"))?;
        self.db.execute(
            "INSERT INTO google_checkpoints(account,json) VALUES(?1,?2)",
            params![account, serde_json::to_string(&checkpoint)?],
        )?;
        Ok(checkpoint)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct StagedAttachment {
    pub(crate) name: String,
    pub(crate) mime_type: String,
    pub(crate) hash: String,
    pub(crate) size: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct StagedMessage {
    pub(crate) remote_id: String,
    pub(crate) remote_thread_id: String,
    pub(crate) history_id: String,
    pub(crate) labels: Vec<String>,
    pub(crate) sender_name: String,
    pub(crate) sender_email: String,
    pub(crate) recipient: String,
    pub(crate) subject: String,
    pub(crate) body: String,
    pub(crate) sent_at: String,
    pub(crate) attachments: Vec<StagedAttachment>,
}

fn storage_error(_: impl std::fmt::Debug) -> GoogleError {
    GoogleError::StoreUnavailable
}
pub(crate) fn is_system_label(label: &str) -> bool {
    let label = label.to_ascii_uppercase();
    [
        "INBOX",
        "SENT",
        "UNREAD",
        "STARRED",
        "SPAM",
        "TRASH",
        "DRAFT",
        "IMPORTANT",
        "CHAT",
    ]
    .contains(&label.as_str())
        || label.starts_with("CATEGORY_")
}
fn bounded_header(value: &str, maximum: usize) -> std::result::Result<String, GoogleError> {
    if value.len() > maximum {
        return Err(GoogleError::InvalidResponse(
            "Normalized header exceeds import bound",
        ));
    }
    Ok(value.to_owned())
}
fn local_id(kind: &str, account: &str, remote: &str) -> String {
    format!(
        "gmail-{kind}-{:x}",
        Sha256::digest(format!("{account}\0{remote}").as_bytes())
    )
}

impl PageStore for GooglePageStore<'_> {
    fn stage_message(
        &mut self,
        expected: &Checkpoint,
        content: MessageContent,
    ) -> std::result::Result<(), GoogleError> {
        self.stage(expected, content)
    }

    fn commit_page(&mut self, plan: &PagePlan) -> std::result::Result<(), GoogleError> {
        let account = plan.expected.account.as_str();
        let generation =
            i64::try_from(plan.expected.generation).map_err(|_| GoogleError::StaleCheckpoint)?;
        let revision =
            i64::try_from(plan.expected.revision).map_err(|_| GoogleError::StaleCheckpoint)?;
        if plan.next.account != plan.expected.account
            || plan.next.revision
                != plan
                    .expected
                    .revision
                    .checked_add(1)
                    .ok_or(GoogleError::StaleCheckpoint)?
            || plan.next.generation < plan.expected.generation
            || plan.next.generation > plan.expected.generation.saturating_add(1)
            || plan.fetch_message_ids.len() > 5000
            || plan.explicitly_deleted_ids.len() > 5000
            || !plan
                .fetch_message_ids
                .is_disjoint(&plan.explicitly_deleted_ids)
        {
            return Err(GoogleError::StaleCheckpoint);
        }
        let fingerprint=format!("{:x}",Sha256::digest(serde_json::to_vec(&json!({"expected":plan.expected,"next":plan.next,
            "observed":plan.observed_message_ids,"fetch":plan.fetch_message_ids,"deleted":plan.explicitly_deleted_ids,
            "inventory_finished":plan.inventory_finished,"caught_up":plan.caught_up})).map_err(storage_error)?));
        let previous:Option<String>=self.store.db.query_row("SELECT fingerprint FROM google_page_commits WHERE account=?1 AND generation=?2 AND expected_revision=?3",params![account,generation,revision],|r|r.get(0)).optional().map_err(storage_error)?;
        if let Some(previous) = previous {
            return if previous == fingerprint {
                Ok(())
            } else {
                Err(GoogleError::StaleCheckpoint)
            };
        }
        self.ensure_checkpoint(&plan.expected)?;
        // Preflight all referenced immutable files outside the SQL transaction.
        // Only one staged message is held in memory at a time.
        for remote in &plan.fetch_message_ids {
            let (staged, hash, size) =
                load_stage(&self.store.db, account, generation, revision, remote)?;
            self.store
                .verified_blob(&hash, size)
                .map_err(storage_error)?;
            for part in staged.attachments {
                self.store
                    .verified_blob(&part.hash, part.size)
                    .map_err(storage_error)?;
            }
        }
        let tx = self
            .store
            .db
            .unchecked_transaction()
            .map_err(storage_error)?;
        let stored: String = tx
            .query_row(
                "SELECT json FROM google_checkpoints WHERE account=?1",
                [account],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        if serde_json::from_str::<Checkpoint>(&stored).map_err(storage_error)? != plan.expected {
            return Err(GoogleError::StaleCheckpoint);
        }
        for remote in &plan.fetch_message_ids {
            let (staged, hash, _) = load_stage(&tx, account, generation, revision, remote)?;
            publish_message(&tx, account, &staged, &hash)?;
        }
        // Scan absence never implies deletion. Even explicit provider deletion
        // hides provider-folder membership only, retaining authored/local data.
        for remote in &plan.explicitly_deleted_ids {
            tx.execute("UPDATE google_message_state SET deleted=1 WHERE account=?1 AND message_id IN (SELECT id FROM messages WHERE account=?1 AND remote_id=?2)",params![account,remote]).map_err(storage_error)?;
            tx.execute("UPDATE messages SET inbox=0,sent=0,revision=revision+1 WHERE account=?1 AND remote_id=?2",params![account,remote]).map_err(storage_error)?;
            tx.execute("UPDATE conversations SET revision=revision+1 WHERE account=?1 AND id IN (SELECT conversation_id FROM messages WHERE account=?1 AND remote_id=?2)",params![account,remote]).map_err(storage_error)?;
        }
        let next = serde_json::to_string(&plan.next).map_err(storage_error)?;
        tx.execute(
            "UPDATE google_checkpoints SET json=?1 WHERE account=?2",
            params![next, account],
        )
        .map_err(storage_error)?;
        tx.execute("INSERT INTO google_page_commits(account,generation,expected_revision,fingerprint,next_json) VALUES(?1,?2,?3,?4,?5)",params![account,generation,revision,fingerprint,next]).map_err(storage_error)?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])
            .map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(())
    }
}

fn load_stage(
    db: &rusqlite::Connection,
    account: &str,
    generation: i64,
    revision: i64,
    remote: &str,
) -> std::result::Result<(StagedMessage, String, u64), GoogleError> {
    let (json,hash,size):(String,String,i64)=db.query_row("SELECT json,source_hash,source_size FROM google_staged_messages WHERE account=?1 AND generation=?2 AND remote_id=?3 AND expected_revision=?4",params![account,generation,remote,revision],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(storage_error)?.ok_or(GoogleError::StoreUnavailable)?;
    if json.len() > 1024 * 1024 {
        return Err(GoogleError::InvalidResponse(
            "Staged normalized record exceeds bound",
        ));
    }
    let staged: StagedMessage = serde_json::from_str(&json).map_err(storage_error)?;
    if staged.remote_id != remote {
        return Err(GoogleError::StaleCheckpoint);
    }
    Ok((staged, hash, u64::try_from(size).map_err(storage_error)?))
}

pub(crate) fn publish_message(
    db: &rusqlite::Connection,
    account: &str,
    staged: &StagedMessage,
    source_hash: &str,
) -> std::result::Result<(), GoogleError> {
    let indexed_source: Option<String> = db
        .query_row(
            "SELECT source_hash FROM messages WHERE account=?1 AND remote_id=?2",
            params![account, staged.remote_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    let existing: Option<(String, String)> = db
        .query_row(
            "SELECT id,conversation_id FROM messages WHERE account=?1 AND remote_id=?2",
            params![account, staged.remote_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(storage_error)?;
    let (mid, cid) = existing.clone().unwrap_or_else(|| {
        (
            local_id("message", account, &staged.remote_id),
            local_id("thread", account, &staged.remote_thread_id),
        )
    });
    let labels: std::collections::BTreeSet<_> = staged.labels.iter().map(String::as_str).collect();
    let pending = |field: &str| -> std::result::Result<Option<bool>, GoogleError> {
        db.query_row("SELECT value FROM google_manual_intents WHERE account=?1 AND message_id=?2 AND field=?3 AND category_id=''",params![account,mid,field],|r|r.get(0)).optional().map_err(storage_error)
    };
    let unread = pending("read")?
        .map(|read| !read)
        .unwrap_or(labels.contains("UNREAD"));
    let starred = pending("starred")?.unwrap_or(labels.contains("STARRED"));
    db.execute(
        "INSERT OR IGNORE INTO conversations(account,id,subject,revision) VALUES(?1,?2,?3,1)",
        params![account, cid, staged.subject],
    )
    .map_err(storage_error)?;
    if existing.is_some() {
        db.execute("UPDATE messages SET sender_name=?1,sender_email=?2,recipient=?3,subject=?4,body=?5,sent_at=?6,unread=?7,starred=?8,inbox=?9,sent=?10,source_hash=?11,revision=revision+1 WHERE account=?12 AND id=?13",
            params![staged.sender_name,staged.sender_email,staged.recipient,staged.subject,staged.body,staged.sent_at,unread,starred,labels.contains("INBOX"),labels.contains("SENT"),source_hash,account,mid]).map_err(storage_error)?;
    } else {
        db.execute("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash,revision) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,1)",
            params![account,mid,cid,staged.remote_id,staged.sender_name,staged.sender_email,staged.recipient,staged.subject,staged.body,staged.sent_at,unread,starred,labels.contains("INBOX"),labels.contains("SENT"),source_hash]).map_err(storage_error)?;
    }
    if indexed_source.as_deref() != Some(source_hash) {
        let rowid: i64 = db
            .query_row(
                "SELECT rowid FROM messages WHERE account=?1 AND id=?2",
                params![account, mid],
                |r| r.get(0),
            )
            .map_err(storage_error)?;
        db.execute("DELETE FROM message_fts WHERE rowid=?1", [rowid])
            .map_err(storage_error)?;
        db.execute(
            "INSERT INTO message_fts(rowid,sender,subject,body) VALUES(?1,?2,?3,?4)",
            params![
                rowid,
                format!("{} {}", staged.sender_name, staged.sender_email),
                staged.subject,
                staged.body
            ],
        )
        .map_err(storage_error)?;
    }
    db.execute("INSERT INTO google_message_state(account,message_id,remote_thread_id,history_id,provider_labels_json,deleted) VALUES(?1,?2,?3,?4,?5,0) ON CONFLICT(account,message_id) DO UPDATE SET remote_thread_id=excluded.remote_thread_id,history_id=excluded.history_id,provider_labels_json=excluded.provider_labels_json,deleted=0",params![account,mid,staged.remote_thread_id,staged.history_id,serde_json::to_string(&staged.labels).map_err(storage_error)?]).map_err(storage_error)?;
    crate::google_live_mutations::project_labels(
        db,
        account,
        &mid,
        &staged.labels.iter().cloned().collect(),
    )
    .map_err(|error| {
        if error.code == "invalid_google_response" {
            GoogleError::InvalidResponse("Category mapping targets a system label")
        } else {
            storage_error(error)
        }
    })?;
    for (index, part) in staged.attachments.iter().enumerate() {
        let aid = local_id(
            "attachment",
            account,
            &format!("{}:{index}:{}", staged.remote_id, part.hash),
        );
        db.execute("INSERT INTO attachments(account,id,message_id,name,mime_type,size,state,blob_hash) VALUES(?1,?2,?3,?4,?5,?6,'available',?7) ON CONFLICT(account,id) DO UPDATE SET name=excluded.name,mime_type=excluded.mime_type,size=excluded.size,state=excluded.state,blob_hash=excluded.blob_hash",params![account,aid,mid,part.name,part.mime_type,part.size,part.hash]).map_err(storage_error)?;
    }
    db.execute(
        "UPDATE conversations SET revision=revision+1 WHERE account=?1 AND id=?2",
        params![account, cid],
    )
    .map_err(storage_error)?;
    Ok(())
}

pub(crate) fn prepare_message(
    content: &MessageContent,
    mut stage: impl FnMut(&[u8]) -> std::result::Result<String, GoogleError>,
) -> std::result::Result<StagedMessage, GoogleError> {
    let parsed = mail_parser::MessageParser::default()
        .parse(&content.source_mime)
        .ok_or(GoogleError::InvalidResponse("Cannot parse source MIME"))?;
    let sender = parsed.from().and_then(|list| list.first());
    let body = fm_mail::extract_parsed_incoming_contribution(&parsed)
        .map_err(|_| GoogleError::InvalidResponse("Cannot extract source text"))?;
    if body.len() > 256 * 1024 {
        return Err(GoogleError::InvalidResponse(
            "Normalized body exceeds the first-slice bound",
        ));
    }
    let mut attachments = Vec::new();
    for part in parsed.attachments() {
        if attachments.len() >= 50 {
            return Err(GoogleError::InvalidResponse("Too many staged attachments"));
        }
        let mime_type = part
            .content_type()
            .map(|ct| {
                format!(
                    "{}/{}",
                    ct.c_type,
                    ct.c_subtype.as_deref().unwrap_or("octet-stream")
                )
            })
            .unwrap_or_else(|| "application/octet-stream".into());
        let bytes = part.contents();
        attachments.push(StagedAttachment {
            name: bounded_header(part.attachment_name().unwrap_or("attachment.bin"), 1024)?,
            mime_type,
            hash: stage(bytes)?,
            size: bytes.len() as u64,
        });
    }
    let staged = StagedMessage {
        remote_id: content.id.clone(),
        remote_thread_id: content.thread_id.clone(),
        history_id: content.history_id.clone(),
        labels: content.label_ids.clone(),
        sender_name: bounded_header(sender.and_then(|s| s.name()).unwrap_or(""), 4096)?,
        sender_email: bounded_header(sender.and_then(|s| s.address()).unwrap_or(""), 4096)?,
        recipient: bounded_header(
            parsed
                .to()
                .and_then(|list| list.first())
                .and_then(|s| s.address())
                .unwrap_or(""),
            4096,
        )?,
        subject: bounded_header(parsed.subject().unwrap_or("(no subject)"), 16 * 1024)?,
        body,
        sent_at: parsed
            .date()
            .and_then(|date| chrono::DateTime::from_timestamp(date.to_timestamp(), 0))
            .map(|date| date.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
            .unwrap_or_default(),
        attachments,
    };
    Ok(staged)
}
