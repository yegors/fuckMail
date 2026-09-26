//! People offered after a search's `from:` or `to:`: senders of this
//! account's recent mail, or the people it wrote to (`role: recipient`, the
//! default in Sent and Drafts). Read-only metadata.
use crate::{err, fields, Result, Store};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Recent messages scanned per request; bounded like recipient suggestions.
const RECENT: i64 = 5000;

struct Person {
    name: String,
    email: String,
    count: usize,
    /// Position of the newest sighting; smaller is more recent.
    first_seen: usize,
}

impl Store {
    /// Suggests people for a search's `from:` or `to:` term, from this
    /// account's 5000 most recent received messages, or sent messages and
    /// recent drafts for recipients, ranked by match quality, frequency and
    /// recency.
    ///
    /// `folder` picks the default role: recipients in `sent` and `drafts`,
    /// senders elsewhere; `role` overrides it. `limit` is 1-10 (default 5).
    pub(crate) fn search_senders(&self, account: &str, p: &Value) -> Result<Value> {
        fields(p, &["query", "folder", "limit", "role"])?;
        let query = match p.get("query") {
            None => String::new(),
            Some(value) => value
                .as_str()
                .filter(|s| s.len() <= 320 && !s.chars().any(char::is_control))
                .ok_or_else(|| {
                    err(
                        "invalid_request",
                        "Sender query must be one line of at most 320 bytes",
                    )
                })?
                .trim()
                .to_lowercase(),
        };
        let folder = match p.get("folder") {
            None => "inbox",
            Some(value) => value
                .as_str()
                .filter(|f| ["inbox", "all", "sent", "drafts", "trash"].contains(f))
                .ok_or_else(|| err("invalid_request", "Unknown folder"))?,
        };
        let limit = p
            .get("limit")
            .map_or(Some(5), Value::as_u64)
            .filter(|n| (1..=10).contains(n))
            .ok_or_else(|| err("invalid_request", "Sender limit must be 1–10"))?
            as usize;
        let own: String =
            self.db
                .query_row("SELECT email FROM accounts WHERE id=?1", [account], |r| {
                    r.get(0)
                })?;
        let own = own.to_lowercase();
        let mut people: HashMap<String, Person> = HashMap::new();
        let mut seen = 0usize;
        let mut add = |name: &str, email: &str| {
            let key = email.trim().to_lowercase();
            if key.is_empty() || key == own || key.len() > 320 {
                return;
            }
            seen += 1;
            let entry = people.entry(key).or_insert_with(|| Person {
                name: String::new(),
                email: email.trim().to_owned(),
                count: 0,
                first_seen: seen,
            });
            entry.count += 1;
            if entry.name.is_empty() {
                entry.name = name.trim().to_owned();
            }
        };
        let recipients = match p.get("role") {
            None => folder == "sent" || folder == "drafts",
            Some(value) => match value.as_str() {
                Some("sender") => false,
                Some("recipient") => true,
                _ => return Err(err("invalid_request", "role must be sender or recipient")),
            },
        };
        if recipients {
            let mut statement = self.db.prepare_cached(
                "SELECT substr(recipient,1,65537) FROM messages WHERE account=?1 AND sent=1 AND trashed=0 ORDER BY rowid DESC LIMIT ?2",
            )?;
            let mut rows = statement.query(rusqlite::params![account, RECENT])?;
            while let Some(row) = rows.next()? {
                let recipient: String = row.get(0)?;
                if recipient.len() > 65536 || recipient.contains(['\r', '\n']) {
                    continue;
                }
                let header = format!("To: {recipient}\r\n\r\n");
                let Some(message) = mail_parser::MessageParser::default().parse(header.as_bytes())
                else {
                    continue;
                };
                for address in message
                    .to()
                    .into_iter()
                    .flat_map(|list| list.iter())
                    .take(100)
                {
                    if let Some(email) = address.address() {
                        add(address.name().unwrap_or(""), email);
                    }
                }
            }
            let mut statement = self.db.prepare_cached(
                "SELECT substr(routing_json,1,65537) FROM drafts WHERE account=?1 AND discarded=0 ORDER BY updated_at DESC,draft_id LIMIT 200",
            )?;
            let mut rows = statement.query([account])?;
            while let Some(row) = rows.next()? {
                let routing: Option<String> = row.get(0)?;
                let Some(routing) = routing
                    .filter(|s| s.len() <= 65536)
                    .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                else {
                    continue;
                };
                for field in ["to_recipients", "cc_recipients", "bcc_recipients"] {
                    for address in
                        crate::composition::recipients(&routing[field]).unwrap_or_default()
                    {
                        add(address.name.as_deref().unwrap_or(""), &address.email);
                    }
                }
            }
        } else {
            let mut statement = self.db.prepare_cached(
                "SELECT substr(sender_name,1,4097),substr(sender_email,1,321) FROM messages WHERE account=?1 AND sent=0 AND trashed=0 ORDER BY rowid DESC LIMIT ?2",
            )?;
            let mut rows = statement.query(rusqlite::params![account, RECENT])?;
            while let Some(row) = rows.next()? {
                let (name, email): (String, String) = (row.get(0)?, row.get(1)?);
                add(&name, &email);
            }
        }
        let rank = |person: &Person| -> Option<u8> {
            if query.is_empty() {
                return Some(0);
            }
            let name = person.name.to_lowercase();
            let email = person.email.to_lowercase();
            if email.starts_with(&query) || name.split_whitespace().any(|w| w.starts_with(&query)) {
                Some(2)
            } else if email.contains(&query) || name.contains(&query) {
                Some(1)
            } else {
                None
            }
        };
        let mut ranked: Vec<(u8, Person)> = people
            .into_values()
            .filter_map(|person| rank(&person).map(|r| (r, person)))
            .collect();
        ranked.sort_by(|(ra, a), (rb, b)| {
            rb.cmp(ra)
                .then_with(|| b.count.cmp(&a.count))
                .then_with(|| a.first_seen.cmp(&b.first_seen))
                .then_with(|| a.email.to_lowercase().cmp(&b.email.to_lowercase()))
        });
        ranked.truncate(limit);
        // Recipient headers often omit names; use the name the person sends as.
        let mut named = self.db.prepare_cached(
            "SELECT sender_name FROM messages WHERE account=?1 AND sender_email=?2 COLLATE NOCASE AND sender_name<>'' ORDER BY rowid DESC LIMIT 1",
        )?;
        let mut out = Vec::with_capacity(ranked.len());
        for (_, mut person) in ranked {
            if recipients && person.name.is_empty() {
                person.name = named
                    .query_row(rusqlite::params![account, person.email], |r| r.get(0))
                    .unwrap_or_default();
            }
            out.push(json!({"name":person.name,"email":person.email}));
        }
        Ok(
            json!({"account":account,"query":query,"folder":folder,"people":out,
            "scope":if recipients {"recent_recipients"} else {"recent_senders"}}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn senders_rank_by_match_then_frequency_and_sent_offers_recipients() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open_synthetic(dir.path()).unwrap();
        let account = "windscribe";
        let seed = |store: &Store,
                    offset: i64,
                    name: &str,
                    email: &str,
                    sent: bool,
                    recipient: &str| {
            store.db.execute("UPDATE messages SET sender_name=?3,sender_email=?4,sent=?5,recipient=?6,trashed=0 WHERE account=?1 AND id=(SELECT id FROM messages WHERE account=?1 ORDER BY id LIMIT 1 OFFSET ?2)",
                rusqlite::params![account, offset, name, email, sent, recipient]).unwrap();
        };
        seed(&store, 0, "Zyla Chen", "zyla@example.test", false, "");
        seed(&store, 1, "Zyla Chen", "zyla@example.test", false, "");
        seed(&store, 2, "Omar Zyla", "omar@example.test", false, "");
        seed(&store, 3, "Ada", "ada.zyla@example.test", false, "");
        seed(
            &store,
            4,
            "Me",
            "you@windscribe.com",
            true,
            "Bexley Lin <bexley@example.test>",
        );
        let people = |store: &mut Store, params: Value| {
            store
                .handle("search_senders", Some(account), params)
                .unwrap()["people"]
                .clone()
        };
        let found = people(&mut store, json!({"query":"ZYLA"}));
        let emails: Vec<_> = found
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["email"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            &emails[..3],
            [
                "zyla@example.test",
                "omar@example.test",
                "ada.zyla@example.test"
            ]
        );
        assert_eq!(found[0]["name"], "Zyla Chen");
        let sent = people(&mut store, json!({"query":"bexley","folder":"sent"}));
        assert_eq!(
            sent,
            json!([{"name":"Bexley Lin","email":"bexley@example.test"}])
        );
        assert!(
            people(&mut store, json!({"limit":2}))
                .as_array()
                .unwrap()
                .len()
                <= 2
        );
        for bad in [
            json!({"limit":0}),
            json!({"folder":"elsewhere"}),
            json!({"query":"a\nb"}),
            json!({"other":1}),
        ] {
            assert_eq!(
                store
                    .handle("search_senders", Some(account), bad)
                    .unwrap_err()
                    .code,
                "invalid_request"
            );
        }
    }
}
