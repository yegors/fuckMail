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

/// [`PageStore`] implementation over the Core store, used by the checkpointed
/// page-at-a-time Gmail importer.
///
/// Messages are staged individually, then one page commit publishes them and
/// advances the account's checkpoint atomically. Every call is fenced by the
/// expected checkpoint, so a stale plan fails with `StaleCheckpoint`.
pub struct GooglePageStore<'a> {
    store: &'a mut Store,
}

impl<'a> GooglePageStore<'a> {
    /// Wraps a store for the duration of one import.
    pub fn new(store: &'a mut Store) -> Self {
        Self { store }
    }

    /// Stages one fetched message: its source becomes a blob
    /// and its normalized record a staged row, not yet visible.
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
            || content.source_mime.len() > fm_google::gmail_api::MAX_RAW_MIME
            || content.label_ids.len() > 1000
            || content.label_ids.iter().any(|id| crate::id(id).is_err())
            || content.history_id.parse::<u64>().is_err()
        {
            return Err(GoogleError::InvalidResponse(
                "Invalid staged message identity or bounds",
            ));
        }
        let staged = prepare_message(&content)?;
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
    /// Links a local category to a Gmail user label, replacing any earlier
    /// link. Gmail system labels (`INBOX`, `CATEGORY_*` and so on) are refused.
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
    /// The account's current page-import checkpoint, if an import was begun.
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

/// An attachment of a staged message, retained inside its original MIME.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct StagedAttachment {
    pub(crate) name: String,
    pub(crate) mime_type: String,
    pub(crate) hash: String,
    pub(crate) size: u64,
    /// Position among the message's parts, so skipping inline images does not
    /// change the ids of the attachments after them.
    #[serde(default)]
    pub(crate) index: Option<usize>,
}

/// A fetched Gmail message normalized for publication. The full source MIME
/// is kept separately as a blob.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct StagedMessage {
    pub(crate) remote_id: String,
    pub(crate) remote_thread_id: String,
    pub(crate) history_id: String,
    /// Gmail label IDs at fetch time.
    pub(crate) labels: Vec<String>,
    pub(crate) sender_name: String,
    pub(crate) sender_email: String,
    /// First To address only.
    pub(crate) recipient: String,
    pub(crate) subject: String,
    /// Extracted reading text, cut to 256 KiB.
    pub(crate) body: String,
    /// RFC 3339 UTC from the Date header, or empty when missing or invalid.
    pub(crate) sent_at: String,
    pub(crate) attachments: Vec<StagedAttachment>,
    /// How the message is protected: 0 none, 1 OpenPGP-signed, 2 encrypted.
    #[serde(default)]
    pub(crate) pgp: i64,
}

fn storage_error(_: impl std::fmt::Debug) -> GoogleError {
    GoogleError::StoreUnavailable
}
/// Reports whether a Gmail label ID is a system label (ignoring case),
/// including the `CATEGORY_*` tabs. Local categories may link only to user
/// labels.
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
/// Stable local ID for a Gmail object, so re-importing it maps to the same row.
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

    /// Publishes a page's staged messages, marks its explicitly deleted ones,
    /// and advances the checkpoint to `plan.next`, all in one transaction.
    /// Committing the same plan again is a no-op; a different plan for the
    /// same checkpoint fails with `StaleCheckpoint`.
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
        // A retried commit after a lost reply must be recognisable as the
        // same page, so each commit is recorded by a fingerprint of its plan.
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
            crate::attachment_content::verify_parts(
                &self.store.google_blob_store(),
                &hash,
                size,
                &staged,
            )
            .map_err(storage_error)?;
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
            tx.execute("UPDATE messages SET inbox=0,sent=0,trashed=0,revision=revision+1 WHERE account=?1 AND remote_id=?2",params![account,remote]).map_err(storage_error)?;
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

/// Loads the staged record for one message of the page planned at `revision`.
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

/// Inserts or updates a message, its conversation, search index entry, Gmail
/// state and attachments from a staged record. Must run inside the caller's
/// transaction.
///
/// Pending local choices (read, star, filing, category) override the fetched
/// labels. The search index is rewritten only when the source changed.
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
        db.execute("UPDATE messages SET sender_name=?1,sender_email=?2,recipient=?3,subject=?4,body=?5,sent_at=?6,unread=?7,starred=?8,inbox=?9,sent=?10,source_hash=?11,pgp=?14,revision=revision+1 WHERE account=?12 AND id=?13",
            params![staged.sender_name,staged.sender_email,staged.recipient,staged.subject,staged.body,staged.sent_at,unread,starred,labels.contains("INBOX"),labels.contains("SENT"),source_hash,account,mid,staged.pgp]).map_err(storage_error)?;
    } else {
        db.execute("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash,revision,pgp) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,1,?16)",
            params![account,mid,cid,staged.remote_id,staged.sender_name,staged.sender_email,staged.recipient,staged.subject,staged.body,staged.sent_at,unread,starred,labels.contains("INBOX"),labels.contains("SENT"),source_hash,staged.pgp]).map_err(storage_error)?;
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
    for (position, part) in staged.attachments.iter().enumerate() {
        let index = part.index.unwrap_or(position);
        let aid = local_id(
            "attachment",
            account,
            &format!("{}:{index}:{}", staged.remote_id, part.hash),
        );
        db.execute("INSERT OR IGNORE INTO archive_blob_cleanup SELECT blob_hash FROM attachments WHERE account=?1 AND id=?2 AND NOT EXISTS(SELECT 1 FROM attachment_sources s WHERE s.account=attachments.account AND s.id=attachments.id)",params![account,aid]).map_err(storage_error)?;
        db.execute("INSERT INTO attachments(account,id,message_id,name,mime_type,size,state,blob_hash) VALUES(?1,?2,?3,?4,?5,?6,'available',?7) ON CONFLICT(account,id) DO UPDATE SET name=excluded.name,mime_type=excluded.mime_type,size=excluded.size,state=excluded.state,blob_hash=excluded.blob_hash",params![account,aid,mid,part.name,part.mime_type,part.size,part.hash]).map_err(storage_error)?;
        db.execute("INSERT INTO attachment_sources(account,id,source_hash,part_index) VALUES(?1,?2,?3,?4) ON CONFLICT(account,id) DO UPDATE SET source_hash=excluded.source_hash,part_index=excluded.part_index",params![account,aid,source_hash,index]).map_err(storage_error)?;
    }
    db.execute(
        "UPDATE conversations SET revision=revision+1 WHERE account=?1 AND id=?2",
        params![account, cid],
    )
    .map_err(storage_error)?;
    Ok(())
}

/// Parses a fetched message into a [`StagedMessage`], hashing its attachments
/// without writing decoded copies. The retained MIME owns their bytes.
///
/// Inline images referenced by the HTML are skipped. Fails with
/// `InvalidResponse` when the MIME cannot be parsed, has more than 50
/// attachments or has an oversized header.
/// Most attachments a received message may list: Gmail's own limit.
pub const MAX_RECEIVED_ATTACHMENTS: usize = 500;

pub(crate) fn prepare_message(
    content: &MessageContent,
) -> std::result::Result<StagedMessage, GoogleError> {
    let parsed = mail_parser::MessageParser::default()
        .parse(&content.source_mime)
        .ok_or(GoogleError::InvalidResponse("Cannot parse source MIME"))?;
    let sender = parsed.from().and_then(|list| list.first());
    let protection = fm_pgp::protection(&parsed);
    let mut body = fm_mail::extract_parsed_incoming_contribution(&parsed)
        .map_err(|_| GoogleError::InvalidResponse("Cannot extract source text"))?;
    // Encrypted text is only ever decrypted for reading, so search and
    // previews see what's around the armor, not ciphertext.
    if protection != fm_pgp::Protection::None {
        body = fm_pgp::readable_text(&body);
    }
    // The reading copy keeps its first 256 KiB; the source MIME stays whole.
    if body.len() > 256 * 1024 {
        let mut end = 256 * 1024;
        while !body.is_char_boundary(end) {
            end -= 1;
        }
        body.truncate(end);
    }
    let mut attachments = Vec::new();
    let inline = fm_mail::html_view::inline_content_ids(&parsed);
    for (index, part) in parsed.attachments().enumerate() {
        // Images the HTML shows (cid:) are part of the body, not attachments.
        if fm_mail::html_view::part_content_id(part).is_some_and(|id| inline.contains(&id)) {
            continue;
        }
        // Nor are a signature or the encrypted payload of PGP/MIME.
        if fm_pgp::is_protection_part(&parsed, part) {
            continue;
        }
        if attachments.len() >= MAX_RECEIVED_ATTACHMENTS {
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
            hash: format!("{:x}", Sha256::digest(bytes)),
            size: bytes.len() as u64,
            index: Some(index),
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
        pgp: protection.code(),
    };
    Ok(staged)
}

impl Store {
    /// Earlier versions listed images the HTML shows (cid:) as attachments.
    /// Removes those rows from stored mail a batch at a time and says whether
    /// more remain. Progress is kept in meta, so each profile is checked once.
    /// Rows a draft uses are kept.
    pub fn repair_inline_attachments(&mut self, batch: usize) -> Result<bool> {
        let after: i64 = self
            .db
            .query_row(
                "SELECT value FROM meta WHERE key='inline_attachment_repair'",
                [],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        if after < 0 {
            return Ok(false);
        }
        let rows: Vec<(i64, String, String, String, String)> = {
            let mut stmt = self.db.prepare("SELECT m.rowid,m.account,m.id,m.conversation_id,m.source_hash FROM messages m WHERE m.rowid>?1 AND EXISTS(SELECT 1 FROM attachments a WHERE a.account=m.account AND a.message_id=m.id AND a.mime_type LIKE 'image/%') ORDER BY m.rowid LIMIT ?2")?;
            let rows = stmt
                .query_map(params![after, batch as i64], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        let blobs = self.google_blob_store();
        let tx = self.db.unchecked_transaction()?;
        let mut last = after;
        let mut removed = 0;
        for (rowid, account, id, conversation, hash) in &rows {
            last = *rowid;
            let Ok(bytes) = crate::local_content::read_source(&blobs, hash) else {
                continue;
            };
            let Some(parsed) = mail_parser::MessageParser::default().parse(&bytes) else {
                continue;
            };
            let inline = fm_mail::html_view::inline_content_ids(&parsed);
            let mut here = 0;
            for part in parsed.attachments() {
                if !fm_mail::html_view::part_content_id(part).is_some_and(|id| inline.contains(&id))
                {
                    continue;
                }
                let blob = format!("{:x}", Sha256::digest(part.contents()));
                here += tx.execute("DELETE FROM attachments WHERE account=?1 AND message_id=?2 AND blob_hash=?3 AND NOT EXISTS(SELECT 1 FROM drafts d,json_each(d.attachments_json) j WHERE d.account=?1 AND j.value=attachments.id)",params![account,id,blob])?;
            }
            if here > 0 {
                tx.execute(
                    "UPDATE conversations SET revision=revision+1 WHERE account=?1 AND id=?2",
                    params![account, conversation],
                )?;
                removed += here;
            }
        }
        let more = rows.len() >= batch;
        tx.execute("INSERT INTO meta(key,value) VALUES('inline_attachment_repair',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[if more { last } else { -1 }])?;
        if removed > 0 {
            tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        }
        tx.commit()?;
        Ok(more)
    }

    /// Mail synced before OpenPGP support has no protection recorded, lists
    /// PGP/MIME plumbing as attachments and indexes inline armor. Repairs
    /// that a batch at a time and says whether more remain. Progress is kept
    /// in meta, so each profile is checked once.
    pub fn repair_pgp_messages(&mut self, batch: usize) -> Result<bool> {
        let after: i64 = self
            .db
            .query_row(
                "SELECT value FROM meta WHERE key='pgp_message_repair'",
                [],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        if after < 0 {
            return Ok(false);
        }
        type Row = (i64, String, String, String, String, String, String, String);
        let rows: Vec<Row> = {
            let mut stmt = self.db.prepare("SELECT m.rowid,m.account,m.id,m.conversation_id,m.source_hash,m.body,m.sender_name||' '||m.sender_email,m.subject FROM messages m WHERE m.rowid>?1 AND m.pgp=0 AND (instr(m.body,'-----BEGIN PGP ')>0 OR EXISTS(SELECT 1 FROM attachments a WHERE a.account=m.account AND a.message_id=m.id AND a.mime_type IN ('application/pgp-encrypted','application/pgp-signature'))) ORDER BY m.rowid LIMIT ?2")?;
            let rows = stmt
                .query_map(params![after, batch as i64], |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        let blobs = self.google_blob_store();
        let tx = self.db.unchecked_transaction()?;
        let mut last = after;
        let mut changed = false;
        for (rowid, account, id, conversation, hash, body, sender, subject) in &rows {
            last = *rowid;
            let Ok(bytes) = crate::local_content::read_source(&blobs, hash) else {
                continue;
            };
            let Some(parsed) = mail_parser::MessageParser::default().parse(&bytes) else {
                continue;
            };
            let protection = fm_pgp::protection(&parsed);
            if protection == fm_pgp::Protection::None {
                continue;
            }
            let readable = fm_pgp::readable_text(body);
            tx.execute(
                "UPDATE messages SET pgp=?3,body=?4,revision=revision+1 WHERE account=?1 AND id=?2",
                params![account, id, protection.code(), readable],
            )?;
            tx.execute("DELETE FROM message_fts WHERE rowid=?1", [rowid])?;
            tx.execute(
                "INSERT INTO message_fts(rowid,sender,subject,body) VALUES(?1,?2,?3,?4)",
                params![rowid, sender, subject, readable],
            )?;
            for part in parsed.attachments() {
                if fm_pgp::is_protection_part(&parsed, part) {
                    let blob = format!("{:x}", Sha256::digest(part.contents()));
                    tx.execute("DELETE FROM attachments WHERE account=?1 AND message_id=?2 AND blob_hash=?3 AND NOT EXISTS(SELECT 1 FROM drafts d,json_each(d.attachments_json) j WHERE d.account=?1 AND j.value=attachments.id)",params![account,id,blob])?;
                }
            }
            tx.execute(
                "UPDATE conversations SET revision=revision+1 WHERE account=?1 AND id=?2",
                params![account, conversation],
            )?;
            changed = true;
        }
        let more = rows.len() >= batch;
        tx.execute("INSERT INTO meta(key,value) VALUES('pgp_message_repair',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[if more { last } else { -1 }])?;
        if changed {
            tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        }
        tx.commit()?;
        Ok(more)
    }
}
