//! Unsubscribing from a received message's mailing list. Core finds how (from
//! the retained MIME, off the writer) and remembers that it was done, per
//! conversation and sender. The engine performs the one-click POST or the
//! unsubscribe email; a link is opened by the person's browser.
use crate::{content::BlobStore, err, fields, id, string, Result, Store};
use fm_mail::unsubscribe::Unsubscribe;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

/// A received message to unsubscribe from, fixed at the source version the
/// reader showed. Built by [`Store::unsubscribe_begin`].
pub struct UnsubscribePlan {
    blobs: BlobStore,
    pub account: String,
    /// Local message ID.
    pub message: String,
    version: String,
    /// Conversation the remembered state is recorded against.
    pub conversation: String,
    /// Lowercase sender address; the remembered state is keyed by it.
    pub sender: String,
    /// The account's own address and name, used as From for a mailto
    /// unsubscribe.
    pub from_email: String,
    pub from_name: String,
}

impl Store {
    /// A received message of this account, at the version the reader showed.
    pub fn unsubscribe_begin(&self, account: &str, request: &Value) -> Result<UnsubscribePlan> {
        fields(request, &["id", "source_version"])?;
        let mid = id(string(request, "id")?)?;
        let row: Option<(String, String, String, bool, String, String)> = self
            .db
            .query_row(
                "SELECT m.source_hash,m.conversation_id,m.sender_email,m.sent,a.email,a.name FROM messages m JOIN accounts a ON a.id=m.account WHERE m.account=?1 AND m.id=?2",
                params![id(account)?, mid],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .optional()?;
        let (version, conversation, sender, sent, from_email, from_name) =
            row.ok_or_else(|| err("not_found", "Message is outside this account"))?;
        if version != string(request, "source_version")? {
            return Err(err(
                "stale_source",
                "Message content changed. Reopen the message.",
            ));
        }
        if sent {
            return Err(err(
                "invalid_request",
                "You sent this message; there is nothing to unsubscribe from",
            ));
        }
        Ok(UnsubscribePlan {
            blobs: self.google_blob_store(),
            account: account.into(),
            message: mid.into(),
            version,
            conversation,
            sender: sender.trim().to_lowercase(),
            from_email,
            from_name,
        })
    }

    /// Remember an unsubscribe that was done (or a page that was opened).
    pub fn unsubscribe_record(&mut self, plan: &UnsubscribePlan, method: &str) -> Result<Value> {
        if !matches!(method, "one_click" | "mailto" | "link") {
            return Err(err("invalid_request", "Unknown unsubscribe method"));
        }
        let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        self.db.execute(
            "INSERT INTO unsubscribes(account,conversation_id,sender_email,method,unsubscribed_at) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(account,conversation_id,sender_email) DO UPDATE SET method=excluded.method,unsubscribed_at=excluded.unsubscribed_at",
            params![plan.account, plan.conversation, plan.sender, method, at],
        )?;
        Ok(json!({"method":method,"at":at}))
    }

    /// The reader's mailing-list line for a conversation whose latest message
    /// is received: how often this sender wrote in the last 30 and 365 days,
    /// and whether this conversation was already unsubscribed from.
    pub(crate) fn mailing_list(
        &self,
        account: &str,
        conversation: &str,
        sender: &str,
    ) -> Result<Value> {
        let sender = sender.trim().to_lowercase();
        let (month, year): (i64, i64) = self.db.prepare_cached(
            "SELECT COUNT(*) FILTER (WHERE julianday(sent_at)>=julianday('now','-30 days')),COUNT(*) FROM messages WHERE account=?1 AND sender_email=?2 COLLATE NOCASE AND sent=0 AND julianday(sent_at)>=julianday('now','-365 days')",
        )?.query_row(params![account, sender], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let unsubscribed = self
            .db
            .query_row(
                "SELECT method,unsubscribed_at FROM unsubscribes WHERE account=?1 AND conversation_id=?2 AND sender_email=?3",
                params![account, conversation, sender],
                |r| Ok(json!({"method":r.get::<_, String>(0)?,"at":r.get::<_, String>(1)?})),
            )
            .optional()?;
        Ok(json!({"sender_email":sender,"month":month,"year":year,"unsubscribed":unsubscribed}))
    }
}

/// The list's unsubscribe address, when it also has one besides one-click,
/// read from the retained MIME.
pub fn resolve_mailto(plan: &UnsubscribePlan) -> Result<Option<Unsubscribe>> {
    let bytes = crate::local_content::read_source(&plan.blobs, &plan.version)?;
    let parsed = mail_parser::MessageParser::default()
        .parse(&bytes)
        .ok_or_else(|| {
            err(
                "content_unavailable",
                "The retained message cannot be parsed",
            )
        })?;
    Ok(fm_mail::unsubscribe::mailto_alternative(&parsed))
}

/// How this message can be unsubscribed from, read from its retained MIME.
/// Fails with `not_found` when it offers no way, and `content_unavailable`
/// when the source is missing, altered or unparseable.
pub fn resolve(plan: &UnsubscribePlan) -> Result<Unsubscribe> {
    let bytes = crate::local_content::read_source(&plan.blobs, &plan.version)?;
    let parsed = mail_parser::MessageParser::default()
        .parse(&bytes)
        .ok_or_else(|| {
            err(
                "content_unavailable",
                "The retained message cannot be parsed",
            )
        })?;
    let plain = parsed
        .body_text(0)
        .map(|t| t.into_owned())
        .unwrap_or_default();
    fm_mail::unsubscribe::unsubscribe(&parsed, &plain)
        .ok_or_else(|| err("not_found", "This message has no way to unsubscribe"))
}

/// An unsubscribe email goes out only to the address the person was shown
/// and agreed to: `confirmed` must name the address `found` writes to.
pub fn check_confirmed(found: &Unsubscribe, confirmed: Option<&str>) -> Result<()> {
    match (found, confirmed) {
        (Unsubscribe::Mailto { address, .. }, Some(confirmed))
            if confirmed.trim().eq_ignore_ascii_case(address) =>
        {
            Ok(())
        }
        _ => Err(err(
            "invalid_request",
            "Confirm the unsubscribe email's address before it is sent",
        )),
    }
}

/// The reader's description of `unsubscribe`, or null. An email shows its
/// `address`, `subject` and `body` for the person to confirm; one-click
/// carries the list's email as `mailto` (or null), asked about only if the
/// list refuses the POST.
pub(crate) fn describe(found: Option<&Unsubscribe>, alternative: Option<&Unsubscribe>) -> Value {
    let email = |found: Option<&Unsubscribe>| match found {
        Some(Unsubscribe::Mailto {
            address,
            subject,
            body,
        }) => json!({"address":address,"subject":subject,"body":body}),
        _ => Value::Null,
    };
    match found {
        None => Value::Null,
        Some(Unsubscribe::OneClick { url }) => {
            json!({"method":"one_click","url":url,"mailto":email(alternative)})
        }
        Some(mailto @ Unsubscribe::Mailto { .. }) => {
            let mut described = email(Some(mailto));
            described["method"] = "mailto".into();
            described
        }
        Some(Unsubscribe::Link { url }) => json!({"method":"link","url":url}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsubscribes_are_found_remembered_and_counted() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path()).unwrap();
        // A conversation whose newest message was received.
        let (account, conversation, mid, sender): (String, String, String, String) = store
            .db
            .query_row(
                "SELECT m.account,m.conversation_id,m.id,m.sender_email FROM messages m WHERE m.sent=0 AND NOT EXISTS(SELECT 1 FROM messages n WHERE n.account=m.account AND n.conversation_id=m.conversation_id AND (julianday(n.sent_at)>julianday(m.sent_at) OR (n.sent_at=m.sent_at AND n.id>m.id))) LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        let source = format!("From: News <{sender}>\r\nList-Unsubscribe: <mailto:leave@list.test?subject=stop>, <https://list.test/u?id=1>\r\nList-Unsubscribe-Post: List-Unsubscribe=One-Click\r\nSubject: Weekly\r\n\r\nHello");
        let hash = store.stage_blob(source.as_bytes()).unwrap();
        store
            .db
            .execute(
                "UPDATE messages SET source_hash=?1,sent_at=strftime('%Y-%m-%dT%H:%M:%SZ','now','-2 days') WHERE account=?2 AND id=?3",
                params![hash, account, mid],
            )
            .unwrap();
        let request = json!({"id":mid,"source_version":hash});
        let read = crate::message_source::read(
            store.message_source_begin(&account, &request).unwrap(),
            &fm_google::transport::CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(read["unsubscribe"]["method"], "one_click");
        // The source has no Gmail verdicts, so its list address is not offered.
        assert!(read["unsubscribe"]["mailto"].is_null());
        let plan = store.unsubscribe_begin(&account, &request).unwrap();
        assert_eq!(
            resolve(&plan).unwrap(),
            Unsubscribe::OneClick {
                url: "https://list.test/u?id=1".into()
            }
        );
        let before = store.conversation(&account, &conversation).unwrap();
        assert!(before["mailing_list"]["month"].as_i64().unwrap() >= 1);
        assert!(before["mailing_list"]["unsubscribed"].is_null());
        store.unsubscribe_record(&plan, "one_click").unwrap();
        let after = store.conversation(&account, &conversation).unwrap();
        assert_eq!(after["mailing_list"]["unsubscribed"]["method"], "one_click");
        assert_eq!(
            after["mailing_list"]["sender_email"],
            sender.trim().to_lowercase()
        );
        assert!(store
            .unsubscribe_begin(&account, &json!({"id":mid,"source_version":"stale"}))
            .is_err());
    }

    #[test]
    fn an_unsubscribe_email_is_described_and_needs_its_address_confirmed() {
        let email = Unsubscribe::Mailto {
            address: "leave@list.test".into(),
            subject: "stop".into(),
            body: "please".into(),
        };
        assert_eq!(
            describe(Some(&email), None),
            json!({"method":"mailto","address":"leave@list.test","subject":"stop","body":"please"})
        );
        let one_click = Unsubscribe::OneClick {
            url: "https://list.test/u".into(),
        };
        assert_eq!(
            describe(Some(&one_click), Some(&email))["mailto"]["address"],
            "leave@list.test"
        );
        assert!(check_confirmed(&email, Some("Leave@List.test")).is_ok());
        // Nothing confirmed, another address, or not an email: refused.
        for confirmed in [None, Some(""), Some("boss@list.test")] {
            assert_eq!(
                check_confirmed(&email, confirmed).unwrap_err().code,
                "invalid_request"
            );
        }
        assert!(check_confirmed(&one_click, Some("leave@list.test")).is_err());
    }
}
