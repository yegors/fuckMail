//! Received attachments resolved from verified original MIME, and incremental
//! conversion of older decoded copies. Authored files stay standalone blobs.
use crate::{content::BlobStore, err, local_content, Result, Store};
use fm_google::transport::CancellationToken;
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourcePart {
    pub hash: String,
    pub index: usize,
}

impl Store {
    pub(crate) fn attachment_source(&self, account: &str, id: &str) -> Result<Option<SourcePart>> {
        Ok(self
            .db
            .query_row(
                "SELECT source_hash,part_index FROM attachment_sources WHERE account=?1 AND id=?2",
                params![account, id],
                |r| {
                    Ok(SourcePart {
                        hash: r.get(0)?,
                        index: r.get(1)?,
                    })
                },
            )
            .optional()?)
    }
}

/// Read exactly the named decoded part, checking both the source and result.
pub(crate) fn read(
    blobs: &BlobStore,
    hash: &str,
    size: u64,
    source: Option<&SourcePart>,
) -> Result<Vec<u8>> {
    let Some(source) = source else {
        return local_content::read_blob(blobs, hash, size);
    };
    let raw = local_content::read_source(blobs, &source.hash)?;
    let parsed = mail_parser::MessageParser::default()
        .parse(&raw)
        .ok_or_else(unavailable)?;
    let part = parsed
        .attachments()
        .nth(source.index)
        .ok_or_else(unavailable)?;
    let bytes = part.contents();
    if bytes.len() as u64 != size || format!("{:x}", Sha256::digest(bytes)) != hash {
        return Err(unavailable());
    }
    Ok(bytes.to_vec())
}

fn unavailable() -> crate::CoreError {
    err(
        "content_unavailable",
        "Retained attachment failed verification",
    )
}

/// Authored operations reuse the same disposable decoded copy as previews.
/// This avoids decoding a large forwarded attachment on every autosave.
pub(crate) fn read_cached(
    blobs: &BlobStore,
    hash: &str,
    size: u64,
    name: &str,
    source: Option<&SourcePart>,
) -> Result<Vec<u8>> {
    if source.is_none() {
        return local_content::read_blob(blobs, hash, size);
    }
    let path = blobs.export_part(hash, size, name, source)?;
    use std::io::Read;
    let mut bytes = Vec::new();
    crate::content::open_regular(&path)?
        .take(size + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 != size || format!("{:x}", Sha256::digest(&bytes)) != hash {
        return Err(unavailable());
    }
    Ok(bytes)
}

pub(crate) fn verify(
    blobs: &BlobStore,
    hash: &str,
    size: u64,
    name: &str,
    source: Option<&SourcePart>,
) -> Result<()> {
    if source.is_some() {
        blobs.export_part(hash, size, name, source)?;
    } else {
        blobs.verified_blob(hash, size)?;
    }
    Ok(())
}

/// One parse verifies all attachment references when resuming a staged import.
pub(crate) fn verify_parts(
    blobs: &BlobStore,
    source: &str,
    size: u64,
    message: &crate::google_store::StagedMessage,
) -> Result<()> {
    let raw = local_content::read_blob(blobs, source, size)?;
    let parsed = mail_parser::MessageParser::default()
        .parse(&raw)
        .ok_or_else(unavailable)?;
    let parts: Vec<_> = parsed.attachments().collect();
    for (position, attachment) in message.attachments.iter().enumerate() {
        let bytes = parts
            .get(attachment.index.unwrap_or(position))
            .ok_or_else(unavailable)?
            .contents();
        if bytes.len() as u64 != attachment.size
            || format!("{:x}", Sha256::digest(bytes)) != attachment.hash
        {
            return Err(unavailable());
        }
    }
    Ok(())
}

struct Candidate {
    account: String,
    id: String,
    hash: String,
    size: u64,
}

/// One message selected on the writer, verified on a worker and then rechecked.
pub struct MigrationPlan {
    blobs: BlobStore,
    rowid: i64,
    source: String,
    candidates: Vec<Candidate>,
}

/// Verified replacement references; no files have been removed.
pub struct PreparedMigration {
    plan: MigrationPlan,
    parts: Vec<(usize, usize)>,
}

impl Store {
    /// Select the next older message without reading its files on the writer.
    pub fn attachment_migration_begin(&self) -> Result<Option<MigrationPlan>> {
        let after: i64 = self.db.query_row(
            "SELECT COALESCE((SELECT value FROM meta WHERE key='attachment_sources_after'),0)",
            [],
            |r| r.get(0),
        )?;
        if after < 0 {
            return Ok(None);
        }
        let message: Option<(i64, String, String, String)> = self.db.query_row(
            "SELECT m.rowid,m.account,m.id,m.source_hash FROM messages m WHERE m.rowid>?1 AND EXISTS(SELECT 1 FROM attachments a WHERE a.account=m.account AND a.message_id=m.id AND a.state='available' AND NOT EXISTS(SELECT 1 FROM attachment_sources s WHERE s.account=a.account AND s.id=a.id)) ORDER BY m.rowid LIMIT 1",
            [after], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
        ).optional()?;
        let Some((rowid, account, message, source)) = message else {
            self.db.execute("INSERT INTO meta VALUES('attachment_sources_after',-1) ON CONFLICT(key) DO UPDATE SET value=-1", [])?;
            return Ok(None);
        };
        let candidates = self.db.prepare(
            "SELECT a.id,a.blob_hash,a.size FROM attachments a WHERE a.account=?1 AND a.message_id=?2 AND a.state='available' AND NOT EXISTS(SELECT 1 FROM attachment_sources s WHERE s.account=a.account AND s.id=a.id)",
        )?.query_map(params![account,message], |r| Ok(Candidate {
            account: account.clone(), id:r.get(0)?,hash:r.get(1)?,size:r.get(2)?,
        }))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Some(MigrationPlan {
            blobs: self.google_blob_store(),
            rowid,
            source,
            candidates,
        }))
    }

    /// Publish only references whose message and attachment still match.
    pub fn attachment_migration_finish(&self, prepared: PreparedMigration) -> Result<()> {
        let plan = prepared.plan;
        let tx = self.db.unchecked_transaction()?;
        for (candidate, index) in prepared.parts {
            let candidate = &plan.candidates[candidate];
            let changed = tx.execute(
                "INSERT OR IGNORE INTO attachment_sources(account,id,source_hash,part_index) SELECT a.account,a.id,m.source_hash,?6 FROM attachments a JOIN messages m ON m.account=a.account AND m.id=a.message_id WHERE a.account=?1 AND a.id=?2 AND a.blob_hash=?3 AND a.size=?4 AND a.state='available' AND m.source_hash=?5",
                params![candidate.account,candidate.id,candidate.hash,candidate.size,plan.source,index],
            )?;
            if changed > 0 {
                tx.execute(
                    "INSERT OR IGNORE INTO archive_blob_cleanup VALUES(?1)",
                    [&candidate.hash],
                )?;
            }
        }
        tx.execute("INSERT INTO meta VALUES('attachment_sources_after',?1) ON CONFLICT(key) DO UPDATE SET value=max(value,excluded.value)", [plan.rowid])?;
        tx.commit()?;
        Ok(())
    }
}

/// Off-writer verification. A missing/corrupt original keeps its standalone copies.
pub fn prepare_migration(
    plan: MigrationPlan,
    cancel: &CancellationToken,
) -> Result<PreparedMigration> {
    let mut parts = Vec::new();
    if let Ok(raw) = local_content::read_source(&plan.blobs, &plan.source) {
        if let Some(parsed) = mail_parser::MessageParser::default().parse(&raw) {
            for (index, part) in parsed.attachments().enumerate() {
                if cancel.is_cancelled() {
                    return Err(err("cancelled", "Attachment conversion stopped"));
                }
                let bytes = part.contents();
                let hash = format!("{:x}", Sha256::digest(bytes));
                for (candidate, entry) in plan.candidates.iter().enumerate() {
                    if entry.hash == hash && entry.size == bytes.len() as u64 {
                        parts.push((candidate, index));
                    }
                }
            }
        }
    }
    Ok(PreparedMigration { plan, parts })
}
