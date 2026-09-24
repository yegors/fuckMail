//! Which local messages link to which hosts, for the link hover card and the
//! blocked-domain list. Hosts are stored as reversed labels (`com.example.news`)
//! so a `*.example.com` pattern is one index range. Counts cover every account.
//!
//! An engine background task indexes messages in rowid order, reading retained
//! sources off the Store writer. Rows follow their message: deleting a message
//! cascades to its link rows, and a re-inserted message gets a new rowid.
use crate::content::BlobStore;
use crate::{err, fields, id, string, Result, Store};
use fm_google::transport::CancellationToken;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

pub(crate) const SCHEMA: &str = "CREATE TABLE link_hosts(id INTEGER PRIMARY KEY,rhost TEXT NOT NULL UNIQUE) STRICT;
CREATE TABLE message_link_hosts(host INTEGER NOT NULL REFERENCES link_hosts(id),message_rowid INTEGER NOT NULL REFERENCES messages(rowid) ON DELETE CASCADE,PRIMARY KEY(host,message_rowid)) STRICT, WITHOUT ROWID;
CREATE INDEX message_link_hosts_message ON message_link_hosts(message_rowid);
CREATE TABLE link_index_state(singleton INTEGER PRIMARY KEY CHECK(singleton=1),indexed_rowid INTEGER NOT NULL) STRICT;";

const MAX_HOSTS_PER_MESSAGE: usize = 200;
const MAX_BATCH: i64 = 500;

/// `news.example.com` → `com.example.news`.
pub fn reversed(host: &str) -> String {
    host.rsplit('.').collect::<Vec<_>>().join(".")
}

pub struct IndexPlan {
    blobs: BlobStore,
    items: Vec<(i64, String, String)>,
}

pub struct IndexedBatch {
    rows: Vec<(i64, Vec<String>)>,
    through: i64,
}

impl Store {
    fn indexed_rowid(&self) -> Result<i64> {
        Ok(self
            .db
            .query_row(
                "SELECT indexed_rowid FROM link_index_state WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    /// The next messages to index, or `None` when every message is indexed.
    pub fn link_index_begin(&self, limit: i64) -> Result<Option<IndexPlan>> {
        let limit = limit.clamp(1, MAX_BATCH);
        let after = self.indexed_rowid()?;
        let mut statement = self.db.prepare(
            "SELECT rowid,source_hash,body FROM messages WHERE rowid>?1 ORDER BY rowid LIMIT ?2",
        )?;
        let items = statement
            .query_map(params![after, limit], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<rusqlite::Result<Vec<(i64, String, String)>>>()?;
        if items.is_empty() {
            return Ok(None);
        }
        Ok(Some(IndexPlan {
            blobs: self.google_blob_store(),
            items,
        }))
    }

    pub fn link_index_finish(&self, batch: IndexedBatch) -> Result<usize> {
        let tx = self.db.unchecked_transaction()?;
        let mut indexed = 0;
        for (rowid, hosts) in &batch.rows {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM messages WHERE rowid=?1)",
                [rowid],
                |r| r.get(0),
            )?;
            if !exists {
                continue;
            }
            for host in hosts {
                let rhost = reversed(host);
                tx.execute(
                    "INSERT OR IGNORE INTO link_hosts(rhost) VALUES(?1)",
                    [&rhost],
                )?;
                tx.execute(
                    "INSERT OR IGNORE INTO message_link_hosts(host,message_rowid) SELECT id,?2 FROM link_hosts WHERE rhost=?1",
                    params![rhost, rowid],
                )?;
            }
            indexed += 1;
        }
        tx.execute(
            "INSERT INTO link_index_state VALUES(1,?1) ON CONFLICT(singleton) DO UPDATE SET indexed_rowid=max(indexed_rowid,excluded.indexed_rowid)",
            [batch.through],
        )?;
        tx.commit()?;
        Ok(indexed)
    }

    fn link_index_complete(&self) -> Result<bool> {
        let newest: i64 =
            self.db
                .query_row("SELECT COALESCE(max(rowid),0) FROM messages", [], |r| {
                    r.get(0)
                })?;
        Ok(self.indexed_rowid()? >= newest)
    }

    /// Messages linking to `host`, across all accounts, outside one conversation.
    pub(crate) fn link_host_summary(&self, account: &str, request: &Value) -> Result<Value> {
        fields(request, &["host", "conversation"])?;
        let host = fm_mail::html_view::display_host(&string(request, "host")?.to_ascii_lowercase())
            .to_owned();
        if host.is_empty() || host.len() > 253 {
            return Err(err("invalid_request", "Name a link host"));
        }
        let conversation = match request.get("conversation") {
            Some(Value::String(c)) => id(c)?.to_owned(),
            None => String::new(),
            _ => return Err(err("invalid_request", "Conversation must be an id")),
        };
        let count: i64 = self.db.query_row(
            "SELECT count(*) FROM message_link_hosts l JOIN link_hosts h ON h.id=l.host JOIN messages m ON m.rowid=l.message_rowid
             WHERE h.rhost=?1 AND NOT (m.account=?2 AND m.conversation_id=?3)",
            params![reversed(&host), id(account)?, conversation],
            |r| r.get(0),
        )?;
        Ok(json!({"host":host,"messages":count,"complete":self.link_index_complete()?}))
    }

    /// Messages linking to each blocked pattern, for the Settings list.
    pub(crate) fn link_pattern_counts(&self, request: &Value) -> Result<Value> {
        fields(request, &["patterns"])?;
        let patterns = request["patterns"]
            .as_array()
            .filter(|p| p.len() <= crate::remote_prefs::MAX_BLOCKED_HOSTS)
            .ok_or_else(|| err("invalid_request", "Patterns must be a list"))?;
        let mut counts = serde_json::Map::new();
        for pattern in patterns {
            let pattern = pattern.as_str().unwrap_or_default();
            if fm_mail::html_view::normalize_block_pattern(pattern).as_deref() != Some(pattern) {
                return Err(err(
                    "invalid_request",
                    "Use a domain like tracker.io or *.tracker.io",
                ));
            }
            let count: i64 = match pattern.strip_prefix("*.") {
                Some(base) => {
                    let rbase = reversed(base);
                    self.db.query_row(
                        "SELECT count(DISTINCT l.message_rowid) FROM link_hosts h JOIN message_link_hosts l ON l.host=h.id
                         WHERE h.rhost=?1 OR (h.rhost>=?2 AND h.rhost<?3)",
                        params![rbase, format!("{rbase}."), format!("{rbase}/")],
                        |r| r.get(0),
                    )?
                }
                None => self.db.query_row(
                    "SELECT count(*) FROM link_hosts h JOIN message_link_hosts l ON l.host=h.id WHERE h.rhost=?1",
                    [reversed(pattern)],
                    |r| r.get(0),
                )?,
            };
            counts.insert(pattern.to_owned(), count.into());
        }
        Ok(json!({"counts":counts,"complete":self.link_index_complete()?}))
    }
}

/// Read each message's retained source and collect its link hosts. Messages
/// without a readable source fall back to their stored reading text.
pub fn link_index_prepare(plan: IndexPlan, cancel: &CancellationToken) -> Result<IndexedBatch> {
    let mut rows = Vec::with_capacity(plan.items.len());
    let mut through = 0;
    for (rowid, hash, body) in plan.items {
        if cancel.is_cancelled() {
            break;
        }
        let html = crate::local_content::read_source(&plan.blobs, &hash)
            .ok()
            .and_then(|bytes| {
                let parsed = mail_parser::MessageParser::default().parse(&bytes)?;
                fm_mail::html_view::html_part(&parsed).map(str::to_owned)
            });
        let hosts: Vec<String> = fm_mail::html_view::link_hosts(html.as_deref(), &body)
            .into_iter()
            .filter(|h| h.len() <= 253 && !h.is_empty())
            .take(MAX_HOSTS_PER_MESSAGE)
            .collect();
        rows.push((rowid, hosts));
        through = rowid;
    }
    if rows.is_empty() {
        return Err(err("cancelled", "Link indexing stopped"));
    }
    Ok(IndexedBatch { rows, through })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_all(store: &Store) {
        while let Some(plan) = store.link_index_begin(3).unwrap() {
            let batch = link_index_prepare(plan, &CancellationToken::new()).unwrap();
            store.link_index_finish(batch).unwrap();
        }
    }

    #[test]
    fn link_hosts_are_counted_across_accounts_and_follow_messages() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        let rows: Vec<(String, String, String)> = store
            .db
            .prepare("SELECT account,id,conversation_id FROM messages ORDER BY rowid LIMIT 3")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        for (i, (account, mid, _)) in rows.iter().enumerate() {
            let body = if i == 2 {
                "no links here"
            } else {
                "see https://www.News.Example.com/a and https://ads.tracker.io/x."
            };
            store
                .db
                .execute(
                    "UPDATE messages SET body=?1 WHERE account=?2 AND id=?3",
                    params![body, account, mid],
                )
                .unwrap();
        }
        let html = b"Content-Type: text/html\r\n\r\n<a href=\"https://cdn.tracker.io/p\">x</a><div class=\"gmail_quote\"><a href=\"https://quoted.test/\">q</a></div>";
        let hash = store.stage_blob(html).unwrap();
        store
            .db
            .execute(
                "UPDATE messages SET source_hash=?1 WHERE account=?2 AND id=?3",
                params![hash, rows[2].0, rows[2].1],
            )
            .unwrap();
        index_all(&store);
        let (account, _, conversation) = &rows[0];
        let summary = store
            .link_host_summary(account, &json!({"host":"news.example.com"}))
            .unwrap();
        assert_eq!(summary["messages"], 2);
        assert_eq!(summary["complete"], true);
        let others = store
            .link_host_summary(
                account,
                &json!({"host":"www.news.example.com","conversation":conversation}),
            )
            .unwrap();
        assert!(others["messages"].as_i64().unwrap() < 2);
        let quoted = store
            .link_host_summary(account, &json!({"host":"quoted.test"}))
            .unwrap();
        assert_eq!(quoted["messages"], 0, "quoted history is not counted");
        let counts = store
            .link_pattern_counts(
                &json!({"patterns":["*.tracker.io","tracker.io","news.example.com"]}),
            )
            .unwrap();
        assert_eq!(counts["counts"]["*.tracker.io"], 3);
        assert_eq!(counts["counts"]["tracker.io"], 0);
        assert_eq!(counts["counts"]["news.example.com"], 2);
        assert!(store
            .link_pattern_counts(&json!({"patterns":["Bad Pattern"]}))
            .is_err());
        // A new message makes the index incomplete until it is indexed; deleting
        // it removes its rows through the cascade.
        let newest: i64 = store
            .db
            .query_row("SELECT max(rowid) FROM messages", [], |r| r.get(0))
            .unwrap();
        store
            .db
            .execute_batch(&format!(
                "CREATE TEMP TABLE copy AS SELECT * FROM messages WHERE rowid={};
                 UPDATE copy SET rowid={}, id='link-copy', remote_id='link-copy';
                 INSERT INTO messages SELECT * FROM copy; DROP TABLE copy;",
                store
                    .db
                    .query_row(
                        "SELECT rowid FROM messages WHERE account=?1 AND id=?2",
                        params![rows[0].0, rows[0].1],
                        |r| r.get::<_, i64>(0)
                    )
                    .unwrap(),
                newest + 1
            ))
            .unwrap();
        assert_eq!(
            store
                .link_host_summary(account, &json!({"host":"x.test"}))
                .unwrap()["complete"],
            false
        );
        index_all(&store);
        assert_eq!(
            store
                .link_host_summary(account, &json!({"host":"news.example.com"}))
                .unwrap()["messages"],
            3
        );
        store
            .db
            .execute("DELETE FROM messages WHERE id='link-copy'", [])
            .unwrap();
        assert_eq!(
            store
                .link_host_summary(account, &json!({"host":"news.example.com"}))
                .unwrap()["messages"],
            2
        );
    }
}
