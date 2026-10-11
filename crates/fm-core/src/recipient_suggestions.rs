//! Read-only address hints from bounded recent cached metadata, never source I/O.
//! Each hint carries how much mail went each way, which also orders them.
use crate::{err, fields, string, Result, Store};
use rusqlite::params;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long an account's sent counts serve lookups before they are taken
/// again; new mail reaching the people index retakes them sooner.
const SENT_COUNTS_AGE: Duration = Duration::from_secs(300);

/// Per account, how many sent messages named each address, so typing an
/// address does not walk all sent mail on every key.
pub(crate) type SentCounts = RefCell<HashMap<String, SentTally>>;

pub(crate) struct SentTally {
    indexed_through: i64,
    taken: Instant,
    counts: Arc<HashMap<String, u64>>,
}

struct Candidate {
    name: String,
    email: String,
    account: String,
    account_name: String,
    score: u8,
    sent: u64,
    received: u64,
}

impl Store {
    /// Suggests recipients matching `query` from recent cached mail and drafts
    /// across all active accounts, ranking this account's history first.
    ///
    /// `limit` is 1-10 (default 6); `exclude` lists up to 100 addresses
    /// already chosen. The account's own address is never suggested. Each
    /// suggestion names the account it was seen in, with how many cached
    /// messages that account sent to and received from the address; the
    /// busiest correspondents come first.
    pub(crate) fn recipient_suggestions(&self, account: &str, p: &Value) -> Result<Value> {
        fields(p, &["query", "exclude", "limit"])?;
        self.require_active_account(account)?;
        let query = string(p, "query")?.trim().to_lowercase();
        if query.len() > 320 || query.chars().any(char::is_control) {
            return Err(err(
                "invalid_request",
                "Recipient query must be one line of at most 320 bytes",
            ));
        }
        let limit = p
            .get("limit")
            .map_or(Some(6), Value::as_u64)
            .filter(|n| (1..=10).contains(n))
            .ok_or_else(|| err("invalid_request", "Recipient limit must be 1–10"))?
            as usize;
        let mut excluded = HashSet::new();
        if let Some(value) = p.get("exclude") {
            let rows = value
                .as_array()
                .filter(|rows| rows.len() <= 100)
                .ok_or_else(|| err("invalid_request", "Exclude at most 100 recipient addresses"))?;
            for row in rows {
                let email = row
                    .as_str()
                    .filter(|s| s.len() <= 320 && !s.chars().any(char::is_control))
                    .ok_or_else(|| err("invalid_request", "Invalid excluded recipient"))?;
                excluded.insert(email.to_lowercase());
            }
        }
        if query.is_empty() {
            return Ok(json!({"account":account,"query":query,"suggestions":[]}));
        }
        let own: String =
            self.db
                .query_row("SELECT email FROM accounts WHERE id=?1", [account], |r| {
                    r.get(0)
                })?;
        excluded.insert(own.to_lowercase());
        let mut candidates: HashMap<String, Candidate> = HashMap::new();
        let mut add = |name: &str, email: &str, source: &str, source_name: &str| {
            let key = email.to_lowercase();
            if excluded.contains(&key) || email.len() > 320 || name.len() > 4096 {
                return;
            }
            let address = fm_mail::Address {
                name: Some(name.to_owned()),
                email: email.to_owned(),
            };
            if fm_mail::validate_address(&address).is_err() {
                return;
            }
            let lower_name = name.to_lowercase();
            let rank = if key.starts_with(&query)
                || lower_name
                    .split_whitespace()
                    .any(|word| word.starts_with(&query))
            {
                4
            } else if key.contains(&query) || lower_name.contains(&query) {
                2
            } else {
                return;
            };
            let score = rank + u8::from(source == account);
            if candidates.get(&key).is_some_and(|old| {
                old.score > score || (old.score == score && !old.name.is_empty())
            }) {
                return;
            }
            candidates.insert(
                key,
                Candidate {
                    name: name.to_owned(),
                    email: email.to_owned(),
                    account: source.to_owned(),
                    account_name: source_name.to_owned(),
                    score,
                    sent: 0,
                    received: 0,
                },
            );
        };
        // Selected-account history gets its own allowance, so other accounts
        // cannot crowd it out. Other-account attribution stays visible in UI.
        let others: Vec<String> = {
            let mut statement = self.db.prepare(
                "SELECT id FROM accounts WHERE id<>?1 AND lifecycle_state='active' ORDER BY id",
            )?;
            let rows = statement.query_map([account], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for selected in [true, false] {
            // Each account's newest mail through its (account, sent_at) index;
            // one query across accounts sorted every message they hold.
            let mut recent = Vec::new();
            let sources = if selected {
                vec![account.to_owned()]
            } else {
                others.clone()
            };
            let mut statement = self.db.prepare("SELECT substr(m.sender_name,1,4097),substr(m.sender_email,1,321),substr(m.recipient,1,65537),m.account,a.name,m.sent_at,m.id FROM messages m JOIN accounts a ON a.id=m.account WHERE m.account=?1 AND m.trashed=0 ORDER BY m.sent_at DESC,m.id LIMIT 2000")?;
            for source in &sources {
                let mut rows = statement.query([source])?;
                while let Some(row) = rows.next()? {
                    let row: (String, String, String, String, String, String, String) = (
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    );
                    recent.push(row);
                }
            }
            recent.sort_by(|a, b| b.5.cmp(&a.5).then_with(|| a.6.cmp(&b.6)));
            recent.truncate(2000);
            for (name, email, recipient, source, source_name, _, _) in &recent {
                add(name, email, source, source_name);
                // Stored recipients are decoded text: a list that does not
                // contain the query cannot yield a match, so skip parsing it.
                if recipient.len() > 65536
                    || recipient.contains(['\r', '\n'])
                    || !recipient.to_lowercase().contains(&query)
                {
                    continue;
                }
                let header = format!("To: {recipient}\r\n\r\n");
                if let Some(message) =
                    mail_parser::MessageParser::default().parse(header.as_bytes())
                {
                    if let Some(addresses) = message.to() {
                        for address in addresses.iter().take(100) {
                            if let Some(email) = address.address() {
                                add(address.name().unwrap_or(""), email, source, source_name);
                            }
                        }
                    }
                }
            }
            let predicate = if selected {
                "d.account=?1"
            } else {
                "d.account<>?1"
            };
            let sql = format!("SELECT substr(d.routing_json,1,65537),d.account,a.name FROM drafts d JOIN accounts a ON a.id=d.account WHERE {predicate} AND a.lifecycle_state='active' AND d.discarded=0 ORDER BY d.updated_at DESC,d.draft_id LIMIT 100");
            let mut statement = self.db.prepare(&sql)?;
            let mut rows = statement.query(params![account])?;
            while let Some(row) = rows.next()? {
                let (routing, source, source_name): (Option<String>, String, String) =
                    (row.get(0)?, row.get(1)?, row.get(2)?);
                let Some(routing) = routing.filter(|s| s.len() <= 65536) else {
                    continue;
                };
                let Ok(routing) = serde_json::from_str::<Value>(&routing) else {
                    continue;
                };
                for field in ["to_recipients", "cc_recipients", "bcc_recipients"] {
                    if let Ok(addresses) = crate::composition::recipients(&routing[field]) {
                        for address in addresses {
                            add(
                                address.name.as_deref().unwrap_or(""),
                                &address.email,
                                &source,
                                &source_name,
                            );
                        }
                    }
                }
            }
        }
        let mut candidates: Vec<_> = candidates.into_values().collect();
        self.count_correspondence(&mut candidates)?;
        candidates.sort_by(|a, b| {
            (b.sent + b.received)
                .cmp(&(a.sent + a.received))
                .then_with(|| b.score.cmp(&a.score))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.email.to_lowercase().cmp(&b.email.to_lowercase()))
        });
        let suggestions: Vec<_> = candidates.into_iter().take(limit).map(|c| json!({"name":c.name,"email":c.email,"account":c.account,"account_name":c.account_name,"sent":c.sent,"received":c.received})).collect();
        Ok(
            json!({"account":account,"query":query,"suggestions":suggestions,"scope":"recent_cached_mail_and_drafts"}),
        )
    }

    /// Counts every cached message, in the account each candidate was seen
    /// in, that came from the candidate or that the account sent with them
    /// on From, To or Cc. Received counts read the sender index; sent counts
    /// come from [`Store::sent_counts`]. Mail newer than the people index is
    /// not yet counted.
    fn count_correspondence(&self, candidates: &mut [Candidate]) -> Result<()> {
        let mut received = self.db.prepare_cached(
            "SELECT count(*) FROM messages WHERE account=?1 AND sender_email=?2 COLLATE NOCASE",
        )?;
        let mut sent: HashMap<String, Arc<HashMap<String, u64>>> = HashMap::new();
        for candidate in candidates.iter_mut() {
            candidate.received =
                received.query_row(params![candidate.account, candidate.email], |r| r.get(0))?;
            if !sent.contains_key(&candidate.account) {
                let counts = self.sent_counts(&candidate.account)?;
                sent.insert(candidate.account.clone(), counts);
            }
            candidate.sent = sent[&candidate.account]
                .get(&candidate.email.trim().to_ascii_lowercase())
                .copied()
                .unwrap_or(0);
        }
        Ok(())
    }

    /// How many of the account's sent messages name each address, walking
    /// its own sent mail once through the sender index and its people rows.
    /// Kept until the people index moves on or the counts are five minutes old.
    fn sent_counts(&self, account: &str) -> Result<Arc<HashMap<String, u64>>> {
        let indexed_through = self.indexed_rowid()?;
        if let Some(tally) = self.sent_counts.borrow().get(account) {
            if tally.indexed_through == indexed_through && tally.taken.elapsed() < SENT_COUNTS_AGE {
                return Ok(tally.counts.clone());
            }
        }
        let mut statement = self.db.prepare_cached("SELECT p.email,count(*) FROM accounts a JOIN messages m ON m.account=a.id AND m.sender_email=a.email COLLATE NOCASE JOIN message_people mp ON mp.message_rowid=m.rowid JOIN people p ON p.id=mp.person WHERE a.id=?1 AND m.sent=1 GROUP BY p.id")?;
        let rows = statement.query_map([account], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let counts = Arc::new(rows.collect::<rusqlite::Result<HashMap<String, u64>>>()?);
        self.sent_counts.borrow_mut().insert(
            account.to_owned(),
            SentTally {
                indexed_through,
                taken: Instant::now(),
                counts: counts.clone(),
            },
        );
        Ok(counts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_recipients_rank_validate_exclude_and_leave_mail_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open_synthetic(dir.path()).unwrap();
        let a = "windscribe";
        let b = "personal";
        let seed = |account: &str, offset: i64, name: &str, email: &str, recipient: &str| {
            store.db.execute("UPDATE messages SET sender_name=?3,sender_email=?4,recipient=?5,sent_at='2030-01-01T00:00:00Z',trashed=0 WHERE account=?1 AND id=(SELECT id FROM messages WHERE account=?1 ORDER BY id LIMIT 1 OFFSET ?2)",params![account,offset,name,email,recipient]).unwrap();
        };
        seed(
            a,
            0,
            "Zoë Current",
            "same@example.test",
            "\"Zoë Recipient\" <recipient@example.test>",
        );
        seed(b, 0, "Zoë Elsewhere", "SAME@example.test", "");
        seed(b, 1, "Zoë Outside", "outside@example.test", "");
        seed(a, 1, "Zoë Broken", "not a mailbox", "");
        seed(a, 2, "Zoë\r\nInjected", "injected@example.test", "");
        let routing = json!({"subject":"Suggestion fixture","to_recipients":[],"cc_recipients":[],"bcc_recipients":[{"name":"Zoë Draft","email":"draft@example.test"}]});
        store.db.execute("INSERT INTO drafts(account,draft_id,mode,routing_json,body,revision) VALUES(?1,'suggestion-draft','new',?2,'Private body',1)",params![a,routing.to_string()]).unwrap();
        let changes = store.db.total_changes();
        let result = store
            .handle("recipient_suggestions", Some(a), json!({"query":"zoë"}))
            .unwrap();
        let rows = result["suggestions"].as_array().unwrap();
        let order: Vec<_> = rows
            .iter()
            .map(|r| (r["email"].as_str().unwrap(), r["account"].as_str().unwrap()))
            .collect();
        // One message each from Same and Outside; at equal counts this
        // account's match wins, then name order.
        assert_eq!(
            order,
            [
                ("same@example.test", a),
                ("outside@example.test", b),
                ("draft@example.test", a),
                ("recipient@example.test", a)
            ]
        );
        assert_eq!(
            (rows[0]["sent"].as_u64(), rows[0]["received"].as_u64()),
            (Some(0), Some(1))
        );
        assert_eq!(
            rows.iter()
                .find(|r| r["email"] == "same@example.test")
                .unwrap()["name"],
            "Zoë Current"
        );
        let filtered = store.handle("recipient_suggestions",Some(a),json!({"query":"zoë","exclude":["SAME@example.test","draft@example.test"],"limit":1})).unwrap();
        assert_eq!(filtered["suggestions"][0]["email"], "outside@example.test");
        assert_eq!(
            store.db.total_changes(),
            changes,
            "query must not write or mark messages reviewed"
        );
        for params in [
            json!({"query":"x","limit":0}),
            json!({"query":"x","limit":11}),
            json!({"query":"x","limit":true}),
            json!({"query":"a\nb"}),
            json!({"query":"x".repeat(321)}),
            json!({"query":"x","exclude":vec!["x@example.test";101]}),
            json!({"query":"x","unexpected":true}),
        ] {
            assert_eq!(
                store
                    .handle("recipient_suggestions", Some(a), params)
                    .unwrap_err()
                    .code,
                "invalid_request"
            );
        }
        store
            .db
            .execute(
                "UPDATE accounts SET lifecycle_state='removed' WHERE id=?1",
                [b],
            )
            .unwrap();
        let result = store
            .recipient_suggestions(a, &json!({"query":"zoë"}))
            .unwrap();
        assert!(result["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["account"] == a));
        assert_eq!(
            store
                .recipient_suggestions(b, &json!({"query":"zoë"}))
                .unwrap_err()
                .code,
            "account_removed"
        );
    }

    #[test]
    fn busiest_correspondents_come_first_with_counts_each_way() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open_synthetic(dir.path()).unwrap();
        let a = "windscribe";
        let own: String = store
            .db
            .query_row("SELECT email FROM accounts WHERE id=?1", [a], |r| r.get(0))
            .unwrap();
        let seed = |offset: i64, name: &str, email: &str, recipient: &str, sent: bool| {
            store.db.execute("UPDATE messages SET sender_name=?3,sender_email=?4,recipient=?5,sent=?6,sent_at='2030-01-01T00:00:00Z',trashed=0 WHERE account=?1 AND id=(SELECT id FROM messages WHERE account=?1 ORDER BY id LIMIT 1 OFFSET ?2)",params![a,offset,name,email,recipient,sent]).unwrap();
        };
        // A prefix match heard from once, a substring match heard from twice,
        // and a substring match written to three times.
        seed(0, "Quill Once", "once@example.test", "", false);
        seed(1, "Two Quillers", "twice@example.test", "", false);
        seed(2, "Two Quillers", "TWICE@example.test", "", false);
        for offset in 3..6 {
            seed(
                offset,
                "Me",
                &own.to_uppercase(),
                "Friend Aquilla <friend@example.test>",
                true,
            );
        }
        store.db.execute("INSERT OR IGNORE INTO people(email,name) VALUES('friend@example.test','Friend Aquilla')", []).unwrap();
        store.db.execute("INSERT INTO message_people(message_rowid,person) SELECT m.rowid,p.id FROM messages m,people p WHERE m.account=?1 AND m.sent=1 AND m.sender_email=?2 AND p.email='friend@example.test'", params![a, own.to_uppercase()]).unwrap();
        let result = store
            .recipient_suggestions(a, &json!({"query":"quil"}))
            .unwrap();
        let rows: Vec<_> = result["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["email"].as_str().unwrap().to_owned(),
                    r["sent"].as_u64().unwrap(),
                    r["received"].as_u64().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                ("friend@example.test".to_owned(), 3, 0),
                ("twice@example.test".to_owned(), 0, 2),
                ("once@example.test".to_owned(), 0, 1)
            ]
        );
        // Sent counts are kept until the people index moves on.
        seed(6, "Me", &own, "friend@example.test", true);
        store.db.execute("INSERT INTO message_people(message_rowid,person) SELECT m.rowid,p.id FROM messages m,people p WHERE m.id=(SELECT id FROM messages WHERE account=?1 ORDER BY id LIMIT 1 OFFSET 6) AND m.account=?1 AND p.email='friend@example.test'", [a]).unwrap();
        let friend_sent = |store: &Store| {
            store
                .recipient_suggestions(a, &json!({"query":"aquilla"}))
                .unwrap()["suggestions"][0]["sent"]
                .as_u64()
        };
        assert_eq!(friend_sent(&store), Some(3));
        store.db.execute("INSERT INTO link_index_state VALUES(1,(SELECT max(rowid) FROM messages)) ON CONFLICT(singleton) DO UPDATE SET indexed_rowid=excluded.indexed_rowid", []).unwrap();
        assert_eq!(friend_sent(&store), Some(4));
        let plan: Vec<String> = store
            .db
            .prepare("EXPLAIN QUERY PLAN SELECT p.email,count(*) FROM accounts a JOIN messages m ON m.account=a.id AND m.sender_email=a.email COLLATE NOCASE JOIN message_people mp ON mp.message_rowid=m.rowid JOIN people p ON p.id=mp.person WHERE a.id=?1 AND m.sent=1 GROUP BY p.id")
            .unwrap()
            .query_map([a], |r| r.get(3))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(
            plan.iter().any(|step| step.contains("messages_sender")),
            "sent counts must walk only the account's own mail: {plan:?}"
        );
    }
}
