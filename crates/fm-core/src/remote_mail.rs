//! Support for mail read live from Google and not kept: which remote threads
//! are already on this machine, and where a fetched attachment is written.
//! Also the local side of deleting mail for good from Spam or Trash.
use crate::view_cache::ViewCache;
use crate::{err, id, Result, Store};
use rusqlite::params;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::PathBuf;

impl Store {
    /// The Gmail thread ids, of those given, that have a message stored here.
    pub fn google_threads_present(
        &self,
        account: &str,
        threads: &[String],
    ) -> Result<BTreeSet<String>> {
        let mut present = BTreeSet::new();
        let mut stmt = self.db.prepare_cached(
            "SELECT EXISTS(SELECT 1 FROM google_message_state WHERE account=?1 AND remote_thread_id=?2 AND deleted=0)",
        )?;
        for thread in threads {
            if stmt.query_row(params![id(account)?, thread], |r| r.get::<_, bool>(0))? {
                present.insert(thread.clone());
            }
        }
        Ok(present)
    }

    /// The Gmail ids of a stored conversation's messages in Trash.
    pub fn google_trashed_in_conversation(
        &self,
        account: &str,
        conversation: &str,
    ) -> Result<Vec<String>> {
        let mut stmt = self.db.prepare_cached(
            "SELECT remote_id FROM messages WHERE account=?1 AND conversation_id=?2 AND trashed=1 ORDER BY sent_at,id LIMIT 1000",
        )?;
        let ids = stmt
            .query_map(params![id(account)?, id(conversation)?], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        if ids.is_empty() {
            return Err(err("not_found", "This conversation is no longer in Trash"));
        }
        Ok(ids)
    }

    /// Removes the local copies of messages Gmail has deleted for good, with
    /// everything derived from them, as retention pruning does. Returns how
    /// many were stored here.
    pub fn google_forget_deleted(&self, account: &str, remote_ids: &[String]) -> Result<usize> {
        let account = id(account)?;
        let tx = self.db.unchecked_transaction()?;
        tx.execute_batch("CREATE TEMP TABLE IF NOT EXISTS retention_messages(id TEXT PRIMARY KEY,conversation_id TEXT NOT NULL,remote_id TEXT NOT NULL,source_hash TEXT NOT NULL,source_rowid INTEGER NOT NULL); DELETE FROM retention_messages;")?;
        {
            let mut insert = tx.prepare_cached("INSERT OR IGNORE INTO retention_messages SELECT id,conversation_id,remote_id,source_hash,rowid FROM messages WHERE account=?1 AND remote_id=?2")?;
            for remote in remote_ids {
                insert.execute(params![account, remote])?;
            }
        }
        // A message an open draft replies to or forwards stays until the
        // draft is sent or discarded; it is gone from Gmail either way.
        tx.execute("DELETE FROM retention_messages WHERE id IN (SELECT source_message_id FROM drafts WHERE account=?1 AND discarded=0 AND source_message_id IS NOT NULL)",[account])?;
        let forgotten: usize =
            tx.query_row("SELECT count(*) FROM retention_messages", [], |r| r.get(0))?;
        if forgotten > 0 {
            crate::account_retention::purge_messages(&tx, account)?;
            tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        }
        tx.execute_batch("DROP TABLE retention_messages")?;
        tx.commit()?;
        Ok(forgotten)
    }

    /// This profile's disposable view cache.
    pub fn view_cache(&self) -> ViewCache {
        ViewCache::new(&self.profile)
    }
}

/// Write a fetched attachment to the view cache as `<sha256>.<ext>`, returning
/// its path and digest.
pub fn remote_attachment_file(
    cache: &ViewCache,
    name: &str,
    bytes: &[u8],
) -> Result<(PathBuf, String)> {
    let digest = format!("{:x}", Sha256::digest(bytes));
    let extension: String = name
        .rsplit_once('.')
        .map(|(_, ext)| ext)
        .unwrap_or("")
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect::<String>()
        .to_ascii_lowercase();
    let file = if extension.is_empty() {
        digest.clone()
    } else {
        format!("{digest}.{extension}")
    };
    Ok((cache.remote_file(&file, bytes)?, digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deleting_for_good_forgets_trash_but_keeps_draft_sources() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        let (account, conversation): (String, String) = store
            .db
            .query_row(
                "SELECT account,conversation_id FROM messages GROUP BY account,conversation_id HAVING count(*)>=2 LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            store
                .google_trashed_in_conversation(&account, &conversation)
                .unwrap_err()
                .code,
            "not_found"
        );
        store
            .db
            .execute(
                "UPDATE messages SET trashed=1,inbox=0 WHERE account=?1 AND conversation_id=?2",
                params![account, conversation],
            )
            .unwrap();
        let remote = store
            .google_trashed_in_conversation(&account, &conversation)
            .unwrap();
        assert!(remote.len() >= 2);
        // An open draft answering the first message keeps that one.
        let kept: String = store
            .db
            .query_row(
                "SELECT id FROM messages WHERE account=?1 AND remote_id=?2",
                params![account, remote[0]],
                |r| r.get(0),
            )
            .unwrap();
        store
            .db
            .execute(
                "INSERT INTO drafts(account,draft_id,conversation_id,mode,source_message_id,body,revision) VALUES(?1,'keep',?2,'reply',?3,'',1)",
                params![account, conversation, kept],
            )
            .unwrap();
        let forgotten = store.google_forget_deleted(&account, &remote).unwrap();
        assert_eq!(forgotten, remote.len() - 1);
        let left: Vec<String> = store
            .db
            .prepare("SELECT id FROM messages WHERE account=?1 AND conversation_id=?2")
            .unwrap()
            .query_map(params![account, conversation], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(left, vec![kept]);
        // Forgetting again, or ids never stored, changes nothing.
        assert_eq!(store.google_forget_deleted(&account, &remote).unwrap(), 0);
    }
}
