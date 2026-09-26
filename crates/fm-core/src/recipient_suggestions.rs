//! Read-only address hints from bounded recent cached metadata, never source I/O.
use crate::{err, fields, string, Result, Store};
use rusqlite::params;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

struct Candidate {
    name: String,
    email: String,
    account: String,
    account_name: String,
    score: u8,
}

impl Store {
    /// Suggests recipients matching `query` from recent cached mail and drafts
    /// across all active accounts, ranking this account's history first.
    ///
    /// `limit` is 1-10 (default 6); `exclude` lists up to 100 addresses
    /// already chosen. The account's own address is never suggested. Each
    /// suggestion names the account it was seen in.
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
                },
            );
        };
        // Selected-account history gets its own allowance, so other accounts
        // cannot crowd it out. Other-account attribution stays visible in UI.
        for selected in [true, false] {
            let predicate = if selected {
                "m.account=?1"
            } else {
                "m.account<>?1"
            };
            let sql = format!("SELECT substr(m.sender_name,1,4097),substr(m.sender_email,1,321),substr(m.recipient,1,65537),m.account,a.name FROM messages m JOIN accounts a ON a.id=m.account WHERE {predicate} AND a.lifecycle_state='active' AND m.trashed=0 ORDER BY m.sent_at DESC,m.id LIMIT 2000");
            let mut statement = self.db.prepare(&sql)?;
            let mut rows = statement.query([account])?;
            while let Some(row) = rows.next()? {
                let (name, email, recipient, source, source_name): (
                    String,
                    String,
                    String,
                    String,
                    String,
                ) = (
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                );
                add(&name, &email, &source, &source_name);
                if recipient.len() > 65536 || recipient.contains(['\r', '\n']) {
                    continue;
                }
                let header = format!("To: {recipient}\r\n\r\n");
                if let Some(message) =
                    mail_parser::MessageParser::default().parse(header.as_bytes())
                {
                    if let Some(addresses) = message.to() {
                        for address in addresses.iter().take(100) {
                            if let Some(email) = address.address() {
                                add(address.name().unwrap_or(""), email, &source, &source_name);
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
        candidates.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.email.to_lowercase().cmp(&b.email.to_lowercase()))
        });
        let suggestions: Vec<_> = candidates.into_iter().take(limit).map(|c| json!({"name":c.name,"email":c.email,"account":c.account,"account_name":c.account_name})).collect();
        Ok(
            json!({"account":account,"query":query,"suggestions":suggestions,"scope":"recent_cached_mail_and_drafts"}),
        )
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
        assert_eq!(rows.len(), 4);
        assert!(rows.iter().take(3).all(|r| r["account"] == a));
        assert_eq!(rows[3]["account"], b);
        assert_eq!(
            rows.iter()
                .find(|r| r["email"] == "same@example.test")
                .unwrap()["name"],
            "Zoë Current"
        );
        let filtered = store.handle("recipient_suggestions",Some(a),json!({"query":"zoë","exclude":["SAME@example.test","draft@example.test"],"limit":1})).unwrap();
        assert_eq!(
            filtered["suggestions"][0]["email"],
            "recipient@example.test"
        );
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
}
