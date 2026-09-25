//! Explicit, local-only retention changes. Provider content is never deleted.
use crate::{err, Result, Store};
use rusqlite::params;

impl Store {
    pub(crate) fn change_mail_retention(&self, account: &str, since: Option<i64>) -> Result<()> {
        // The account_update transaction owns this entire reset. Advancing the
        // generation fences prepared pages from the old acquisition window.
        self.db.execute(
            "UPDATE google_accounts SET mail_generation=mail_generation+1 WHERE account=?1",
            [account],
        )?;
        self.db.execute(
            "DELETE FROM google_live_page_items WHERE account=?1",
            [account],
        )?;
        self.db
            .execute("DELETE FROM google_live_pages WHERE account=?1", [account])?;
        self.db
            .execute("DELETE FROM google_live_lanes WHERE account=?1", [account])?;
        if let Some(since) = since {
            self.db.execute_batch("CREATE TEMP TABLE IF NOT EXISTS retention_messages(id TEXT PRIMARY KEY,conversation_id TEXT NOT NULL,remote_id TEXT NOT NULL,source_hash TEXT NOT NULL,source_rowid INTEGER NOT NULL); DELETE FROM retention_messages;")?;
            // Match acquisition's cutoff: Gmail's internal date when known, else
            // the header date. Otherwise edge messages are pruned then refetched.
            self.db.execute("INSERT INTO retention_messages SELECT m.id,m.conversation_id,m.remote_id,m.source_hash,m.rowid FROM messages m LEFT JOIN google_mail_coverage c ON c.account=m.account AND c.remote_id=m.remote_id WHERE m.account=?1 AND COALESCE(c.internal_date_ms,unixepoch(m.sent_at)*1000)<?2*1000",params![account,since])?;
            let pending:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM messages m JOIN google_mail_mutations w ON w.account=m.account AND w.message_id=m.id WHERE m.account=?1 AND m.conversation_id IN (SELECT conversation_id FROM retention_messages) AND w.state IN ('pending','in_flight','reconcile')) OR EXISTS(SELECT 1 FROM drafts d WHERE d.account=?1 AND d.source_message_id IN (SELECT id FROM retention_messages) AND d.discarded=0)",[account],|r|r.get(0))?;
            if pending {
                return Err(err("retention_work_pending","Finish pending mail changes and drafts using older messages before shortening retention"));
            }
            purge_messages(&self.db, account)?;
            self.db.execute_batch("DROP TABLE retention_messages")?;
        }
        self.db.execute("UPDATE google_mail_metrics SET observed=(SELECT count(*) FROM google_mail_coverage WHERE account=?1 AND deleted=0),retained_sources=(SELECT count(*) FROM google_mail_coverage WHERE account=?1 AND deleted=0 AND source_state='retained'),indexed=(SELECT count(*) FROM google_mail_coverage WHERE account=?1 AND deleted=0 AND index_state='indexed'),complete_attachments=(SELECT count(*) FROM google_mail_coverage WHERE account=?1 AND deleted=0 AND attachment_state='complete'),unavailable=(SELECT count(*) FROM google_mail_coverage WHERE account=?1 AND deleted=0) WHERE account=?1",[account])?;
        Ok(())
    }
}

/// Removes the local copies listed in the temporary `retention_messages`
/// table (id, conversation_id, remote_id, source_hash, source_rowid) with
/// everything derived from them. Provider content is untouched; startup
/// cleanup unlinks blobs nothing else references.
pub(crate) fn purge_messages(db: &rusqlite::Connection, account: &str) -> Result<()> {
    // Existing startup cleanup verifies all surviving references before
    // unlinking blobs, including draft snapshots and other accounts.
    db.execute(
        "INSERT OR IGNORE INTO archive_blob_cleanup SELECT source_hash FROM retention_messages",
        [],
    )?;
    db.execute("INSERT OR IGNORE INTO archive_blob_cleanup SELECT blob_hash FROM attachments WHERE account=?1 AND message_id IN (SELECT id FROM retention_messages)",[account])?;
    db.execute(
        "DELETE FROM message_fts WHERE rowid IN (SELECT source_rowid FROM retention_messages)",
        [],
    )?;
    for table in [
        "message_categories",
        "attachments",
        "google_message_state",
        "google_manual_intents",
        "assist_category_intents",
        "local_model_label_overrides",
        "message_sender_kinds",
        "google_mail_mutations",
    ] {
        db.execute(&format!("DELETE FROM {table} WHERE account=?1 AND message_id IN (SELECT id FROM retention_messages)"),[account])?;
    }
    for table in ["assist_results", "local_model_results"] {
        db.execute(&format!("DELETE FROM {table} WHERE account=?1 AND conversation_id IN (SELECT conversation_id FROM retention_messages)"),[account])?;
    }
    // Completed mail receipts contain reader copies. Remove only those
    // whose exact members are evicted; authored draft receipts remain.
    db.execute("DELETE FROM google_mail_mutations WHERE account=?1 AND request_id IN (SELECT request_id FROM operations WHERE account=?1 AND json_type(result_json,'$.messages')='array' AND json_extract(result_json,'$.id') IN (SELECT conversation_id FROM retention_messages))",[account])?;
    db.execute("DELETE FROM operations WHERE account=?1 AND json_type(result_json,'$.messages')='array' AND json_extract(result_json,'$.id') IN (SELECT conversation_id FROM retention_messages)",[account])?;
    db.execute("DELETE FROM google_mail_coverage WHERE account=?1 AND remote_id IN (SELECT remote_id FROM retention_messages)",[account])?;
    db.execute(
        "DELETE FROM messages WHERE account=?1 AND id IN (SELECT id FROM retention_messages)",
        [account],
    )?;
    db.execute("UPDATE conversations SET revision=revision+1 WHERE account=?1 AND id IN (SELECT conversation_id FROM retention_messages)",[account])?;
    // Keep empty conversation identities that are referenced by durable
    // authored work; browsing already requires at least one message.
    db.execute("DELETE FROM conversations WHERE account=?1 AND id IN (SELECT conversation_id FROM retention_messages) AND NOT EXISTS(SELECT 1 FROM messages m WHERE m.account=conversations.account AND m.conversation_id=conversations.id) AND NOT EXISTS(SELECT 1 FROM drafts d WHERE d.account=conversations.account AND d.conversation_id=conversations.id) AND NOT EXISTS(SELECT 1 FROM assist_results r WHERE r.account=conversations.account AND r.conversation_id=conversations.id)",[account])?;
    Ok(())
}
