//! Permanent local archive deletion, with shared blob cleanup before worker startup.
use crate::{err, fields, id, string, Result, Store};
use rusqlite::{params, types::ValueRef, Connection, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

/// Quotes an SQL identifier read from `sqlite_master`.
fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
/// Every ordinary table and whether it has an `account` column. Reading the
/// schema instead of keeping a list means a newly added account-scoped table is
/// purged and scanned without changes here. The FTS tables and this module's
/// own bookkeeping tables are left out.
fn tables(db: &Connection) -> Result<Vec<(String, bool)>> {
    let names=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'message_fts%' AND name NOT IN ('deleted_account_archives','archive_blob_cleanup')")?
        .query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    names
        .into_iter()
        .map(|name| {
            let account = db
                .prepare(&format!("PRAGMA table_info({})", quote(&name)))?
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?
                .iter()
                .any(|c| c == "account");
            Ok((name, account))
        })
        .collect()
}
/// Every run of exactly 64 hex digits in `bytes`, lowercased. Blob names are
/// SHA-256 hex digests, and they appear in plain columns and inside JSON alike,
/// so a textual scan finds them all. False positives only keep a blob longer.
fn hashes(bytes: &[u8]) -> impl Iterator<Item = String> + '_ {
    bytes
        .split(|c| !c.is_ascii_hexdigit())
        .filter(|part| part.len() == 64)
        .map(|part| String::from_utf8_lossy(part).to_ascii_lowercase())
}
/// Calls `visit` with every blob-like hash in the text columns of `table`,
/// limited to `account`'s rows when given.
fn scan(
    db: &Connection,
    table: &str,
    account: Option<&str>,
    mut visit: impl FnMut(String),
) -> Result<()> {
    let sql = format!(
        "SELECT * FROM {}{}",
        quote(table),
        if account.is_some() {
            " WHERE account=?1"
        } else {
            ""
        }
    );
    let mut stmt = db.prepare(&sql)?;
    let columns = stmt.column_count();
    let mut rows = stmt.query(rusqlite::params_from_iter(account))?;
    while let Some(row) = rows.next()? {
        for col in 0..columns {
            if let ValueRef::Text(bytes) = row.get_ref(col)? {
                for hash in hashes(bytes) {
                    visit(hash);
                }
            }
        }
    }
    Ok(())
}
impl Store {
    /// Permanently deletes a removed account's local archive: every row in an
    /// account-scoped table, its search index entries and its preferences.
    /// Nothing changes in Google.
    ///
    /// `p` holds `expected_revision` (the archive's lifecycle revision) and
    /// `request_id`. Repeating the same request returns the first result; a
    /// different request for a deleted archive fails with
    /// `request_id_conflict`. Fails with `stale_account_revision` unless the
    /// account is removed at that revision, `account_cleanup_busy` while
    /// credential cleanup is unfinished, and `archive_transfer_pending` while
    /// a draft transfer involving it is open. Blob files are unlinked at the
    /// next startup, once nothing else refers to them.
    pub fn account_archive_delete(&mut self, account: &str, p: &Value) -> Result<Value> {
        fields(p, &["expected_revision", "request_id"])?;
        id(account)?;
        let request = string(p, "request_id")?;
        id(request)?;
        let fingerprint = crate::mutations::fingerprint("account_archive_delete", p)?;
        let prior:Option<(String,String,String)>=self.db.query_row("SELECT request_id,fingerprint,result_json FROM deleted_account_archives WHERE account=?1",[account],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((old, fp, result)) = prior {
            if old != request || fp != fingerprint {
                return Err(err(
                    "request_id_conflict",
                    "This archive was already deleted by another request",
                ));
            }
            return Ok(serde_json::from_str(&result)?);
        }
        let life = self.account_lifecycle(account)?;
        if life["state"] != "removed"
            || life["revision"] != p["expected_revision"]
            || p["expected_revision"].as_i64().is_none_or(|n| n <= 0)
        {
            return Err(err(
                "stale_account_revision",
                "Delete only the current removed account archive",
            ));
        }
        if life["cleanup"]["state"] != "complete" {
            return Err(err(
                "account_cleanup_busy",
                "Finish sign-in credential cleanup before deleting this archive",
            ));
        }
        let subject: Option<String> = self
            .db
            .query_row(
                "SELECT google_subject FROM google_accounts WHERE account=?1",
                [account],
                |r| r.get(0),
            )
            .optional()?;
        // The deleted ID stays reserved by the receipt row below. Signing in
        // again with the same Google identity binds to this derived
        // replacement ID instead (see `validate_google_binding`).
        let base = subject
            .as_deref()
            .map(crate::live_profile::google_account_id)
            .transpose()?
            .unwrap_or_else(|| account.into());
        let replacement = format!(
            "google-{:x}",
            Sha256::digest(format!("{account}:{fingerprint}"))
        );
        let transfers:i64=self.db.query_row("SELECT count(*) FROM draft_transfers WHERE (account=?1 OR target_account=?1) AND state NOT IN ('complete','blocked')",[account],|r|r.get(0))?;
        if transfers > 0 {
            return Err(err(
                "archive_transfer_pending",
                "Finish pending draft transfers before deleting this archive",
            ));
        }
        let layout = tables(&self.db)?;
        let tx = self.db.unchecked_transaction()?;
        // Rows go table by table in no dependency order, so foreign keys are
        // checked only at commit, when every dependent row is gone.
        tx.execute_batch("PRAGMA defer_foreign_keys=ON")?;
        // Queue every blob the account refers to before its rows go. Files are
        // not touched here: another account or a later import may share them.
        for (table, scoped) in &layout {
            if *scoped {
                let mut candidates = BTreeSet::new();
                scan(&tx, table, Some(account), |hash| {
                    candidates.insert(hash);
                })?;
                for hash in candidates {
                    tx.execute(
                        "INSERT OR IGNORE INTO archive_blob_cleanup(hash) VALUES(?1)",
                        [hash],
                    )?;
                }
            }
        }
        // Tables without an `account` column are keyed through a parent row
        // and must go before it.
        tx.execute(
            "DELETE FROM message_fts WHERE rowid IN (SELECT rowid FROM messages WHERE account=?1)",
            [account],
        )?;
        tx.execute("DELETE FROM calendar_sync_items WHERE run IN (SELECT id FROM calendar_sync_runs WHERE account=?1)",[account])?;
        tx.execute("DELETE FROM calendar_sync_pages WHERE run IN (SELECT id FROM calendar_sync_runs WHERE account=?1)",[account])?;
        // Destination drafts remain intact; obsolete transfer recovery links
        // cannot retain foreign keys to either deleted source identity.
        tx.execute(
            "DELETE FROM draft_transfers WHERE account=?1 OR target_account=?1",
            [account],
        )?;
        for (table, scoped) in &layout {
            if *scoped {
                tx.execute(
                    &format!("DELETE FROM {} WHERE account=?1", quote(table)),
                    [account],
                )?;
            }
        }
        for key in [
            crate::local_model::history_cutoff_key(account),
            crate::local_model::catch_up_key(account),
        ]
        .into_iter()
        .chain(crate::google_live_mail::mail_count_keys(account))
        {
            tx.execute("DELETE FROM meta WHERE key=?1", [key])?;
        }
        // Local model settings keep per-account entries inside JSON, including
        // their history, so the account's key is removed from each copy.
        for table in ["local_model_settings", "local_model_settings_history"] {
            let rows = tx
                .prepare(&format!("SELECT rowid,settings_json FROM {table}"))?
                .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for (rowid, raw) in rows {
                let mut settings: Value = serde_json::from_str(&raw)?;
                if settings["accounts"]
                    .as_object_mut()
                    .is_some_and(|accounts| accounts.remove(account).is_some())
                {
                    tx.execute(
                        &format!("UPDATE {table} SET settings_json=?1 WHERE rowid=?2"),
                        params![serde_json::to_string(&settings)?, rowid],
                    )?;
                }
            }
        }
        tx.execute("UPDATE local_model_settings SET revision=revision+1", [])?;
        crate::preferences::forget_account(&tx, account)?;
        crate::preferences::forget_account_color(&tx, account)?;
        tx.execute("DELETE FROM accounts WHERE id=?1", [account])?;
        let result = json!({"account":account,"request_id":request,"deleted":true,"remote_data_changed":false,"file_cleanup":"next_startup"});
        tx.execute(
            "INSERT INTO deleted_account_archives VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                account,
                base,
                replacement,
                request,
                fingerprint,
                serde_json::to_string(&result)?
            ],
        )?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.execute("UPDATE calendar_publication SET revision=revision+1", [])?;
        tx.commit()?;
        Ok(result)
    }
    /// Unlinks queued blob files that no row in any table still names, and
    /// dequeues them. A file that cannot be removed stays queued for the next
    /// startup. Deleted archives and locally purged messages, such as those
    /// outside a shortened retention window, both queue here.
    ///
    /// Startup has no provider/content workers. Imports made after a purge can
    /// reuse content, so recheck every retained reference before unlinking it.
    pub(crate) fn cleanup_deleted_archive_blobs(&self) -> Result<()> {
        let queued = self
            .db
            .prepare("SELECT hash FROM archive_blob_cleanup")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<BTreeSet<_>>>()?;
        if queued.is_empty() {
            return Ok(());
        }
        let mut unused = queued.clone();
        for (table, _) in tables(&self.db)? {
            scan(&self.db, &table, None, |hash| {
                unused.remove(&hash);
            })?;
        }
        for hash in queued {
            if !unused.contains(&hash)
                || crate::content::remove_unused_blob(&self.profile, &hash).is_ok()
            {
                self.db
                    .execute("DELETE FROM archive_blob_cleanup WHERE hash=?1", [hash])?;
            }
        }
        Ok(())
    }
}
