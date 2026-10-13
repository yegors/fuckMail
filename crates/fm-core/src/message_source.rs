//! Explicit, bounded reader content. Blob verification and parsing run off the writer.
use crate::{content::BlobStore, err, fields, id, string, Result, Store};
use fm_google::transport::CancellationToken;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

/// A message's retained source to read off the writer with [`read`].
pub struct ReadPlan {
    blobs: BlobStore,
    account: String,
    message: String,
    version: String,
}

impl Store {
    /// Plans reading a message's retained source. `request` holds `id` and the
    /// `source_version` (source hash) the reader last saw.
    ///
    /// Fails with `stale_source` if the message's source changed since. Does
    /// not mark the message read.
    pub fn message_source_begin(&self, account: &str, request: &Value) -> Result<ReadPlan> {
        fields(request, &["id", "source_version"])?;
        let mid = id(string(request, "id")?)?;
        let version: Option<String> = self.db.query_row(
            "SELECT m.source_hash FROM messages m JOIN accounts a ON a.id=m.account WHERE m.account=?1 AND m.id=?2",
            params![id(account)?, mid], |row| row.get(0)).optional()?;
        let version = version.ok_or_else(|| err("not_found", "Message is outside this account"))?;
        if version != string(request, "source_version")? {
            return Err(err(
                "stale_source",
                "Message content changed. Reopen the message.",
            ));
        }
        Ok(ReadPlan {
            blobs: self.google_blob_store(),
            account: account.into(),
            message: mid.into(),
            version,
        })
    }
}

fn bounded(value: &str) -> (String, bool) {
    bounded_at(value, 256 * 1024)
}

fn bounded_at(value: &str, limit: usize) -> (String, bool) {
    let mut end = value.len().min(limit);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    (value[..end].into(), end < value.len())
}

/// Verifies and parses the source off the writer and returns reader content:
/// address metadata, unsubscribe options, whether it has an invitation, the
/// HTML part, the reading text as this build extracts it (`body_text`, which
/// may be newer than the stored copy), the whole text with the reply history
/// it leaves out (`body_full`, empty when nothing was left out) and the raw
/// source.
///
/// HTML, both texts and raw text are each cut at 256 KiB, then the longest
/// is halved until the reply fits in about 900 KiB of JSON; the `*_truncated`
/// flags say so. Fails
/// with `content_unavailable` if the blob is missing, altered or unparseable.
pub fn read(plan: ReadPlan, cancel: &CancellationToken) -> Result<Value> {
    if cancel.is_cancelled() {
        return Err(err("cancelled", "Message read cancelled"));
    }
    let bytes = crate::local_content::read_source(&plan.blobs, &plan.version)?;
    let parsed = mail_parser::MessageParser::default()
        .parse(&bytes)
        .ok_or_else(|| {
            err(
                "content_unavailable",
                "The retained message cannot be parsed",
            )
        })?;
    let addresses = |label: &str, value: Option<&mail_parser::Address<'_>>| -> Value {
        let people: Vec<Value> = value
            .map(|value| {
                value
                    .iter()
                    .take(50)
                    .filter_map(|a| {
                        a.address()
                            .map(|email| json!({"name":bounded_at(a.name().unwrap_or(""),320).0,"email":bounded_at(email,320).0}))
                    })
                    .collect()
            })
            .unwrap_or_default();
        json!({"label":label,"people":people})
    };
    let metadata = [
        addresses("To", parsed.to()),
        addresses("Cc", parsed.cc()),
        addresses("Bcc", parsed.bcc()),
        addresses("Reply to", parsed.reply_to()),
    ];
    // mail-parser lists a plain part in html_body for plain-only mail and
    // body_html() then synthesizes HTML; expose only an actual HTML part.
    let html = fm_mail::html_view::html_part(&parsed);
    let (body_html, html_truncated) = bounded(html.unwrap_or(""));
    let plain = parsed
        .body_text(0)
        .map(|t| t.into_owned())
        .unwrap_or_default();
    let unsubscribe =
        crate::unsubscribe::describe(fm_mail::unsubscribe::unsubscribe(&parsed, &plain).as_ref());
    // Readers ask message_invite only for mail with a usable calendar part.
    let has_invite = crate::message_invite::message_calendar(&parsed).is_some();
    let text = fm_mail::extract_parsed_incoming_contribution(&parsed).unwrap_or_default();
    let (body_text, text_truncated) = bounded(&text);
    // The whole reading text, quoted history included, for a reader who asks
    // to see it; empty when nothing was left out.
    let full = fm_mail::extract_parsed_incoming_text(&parsed).unwrap_or_default();
    let (body_full, full_truncated) = if full.trim() == text.trim() {
        (String::new(), false)
    } else {
        bounded(&full)
    };
    let (raw, raw_truncated) = bounded(&String::from_utf8_lossy(&bytes));
    if cancel.is_cancelled() {
        return Err(err("cancelled", "Message read cancelled"));
    }
    let mut result = json!({"account":plan.account,"id":plan.message,"source_version":plan.version,
        "metadata":metadata,"unsubscribe":unsubscribe,"has_invite":has_invite,"body_html":body_html,"html_truncated":html_truncated,
        "body_text":body_text,"text_truncated":text_truncated,"body_full":body_full,"full_truncated":full_truncated,"raw":raw,"raw_truncated":raw_truncated,"size":bytes.len()});
    fit_reply(&mut result)?;
    Ok(result)
}

fn fit_reply(result: &mut Value) -> Result<()> {
    // JSON escaping can enlarge even bounded UTF-8. Leave room for the IPC envelope.
    const PARTS: [(&str, &str); 4] = [
        ("raw", "raw_truncated"),
        ("body_html", "html_truncated"),
        ("body_text", "text_truncated"),
        ("body_full", "full_truncated"),
    ];
    while serde_json::to_vec(result)?.len() > 900 * 1024 {
        let (key, flag) = PARTS
            .into_iter()
            .max_by_key(|(key, _)| result[*key].as_str().map_or(0, str::len))
            .unwrap();
        let value = result[key].as_str().unwrap();
        if value.is_empty() {
            return Err(err(
                "limit_exceeded",
                "Message metadata exceeds the reader limit",
            ));
        }
        result[key] = bounded_at(value, value.len() / 2).0.into();
        result[flag] = true.into();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_reader_content_is_exact_scoped_and_read_only() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        let (account, mid): (String, String) = store
            .db
            .query_row("SELECT account,id FROM messages LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        let source = b"From: Sender <sender@example.test>\r\nTo: Team <team@example.test>\r\nCc: Copy <copy@example.test>\r\nSubject: Retained\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>Hello <b>team</b>.</p>";
        let hash = store.stage_blob(source).unwrap();
        store
            .db
            .execute(
                "UPDATE messages SET source_hash=?1 WHERE account=?2 AND id=?3",
                params![hash, account, mid],
            )
            .unwrap();
        let state: (bool, bool, i64) = store
            .db
            .query_row(
                "SELECT unread,starred,revision FROM messages WHERE account=?1 AND id=?2",
                params![account, mid],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        let request = json!({"id":mid,"source_version":hash});
        let result = read(
            store.message_source_begin(&account, &request).unwrap(),
            &CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(
            result["raw"].as_str().unwrap(),
            String::from_utf8_lossy(source)
        );
        assert_eq!(result["body_html"], "<p>Hello <b>team</b>.</p>");
        assert_eq!(result["body_text"], "Hello team.");
        assert_eq!(result["text_truncated"], false);
        assert_eq!(result["body_full"], "");
        assert_eq!(
            result["metadata"][1]["people"][0]["email"],
            "copy@example.test"
        );
        assert_eq!(result["raw_truncated"], false);
        assert_eq!(result["has_invite"], false);
        let after: (bool, bool, i64) = store
            .db
            .query_row(
                "SELECT unread,starred,revision FROM messages WHERE account=?1 AND id=?2",
                params![account, mid],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(state, after);
        assert!(store
            .message_source_begin("different-account", &request)
            .is_err());
        assert!(store
            .message_source_begin(&account, &json!({"id":mid,"source_version":"different"}))
            .is_err());
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(read(
            store.message_source_begin(&account, &request).unwrap(),
            &cancel
        )
        .is_err());
        std::fs::write(temp.path().join("blobs").join(hash), b"changed").unwrap();
        assert!(read(
            store.message_source_begin(&account, &request).unwrap(),
            &CancellationToken::new()
        )
        .is_err());
    }
    #[test]
    fn whole_text_comes_with_left_out_history() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        let (account, mid): (String, String) = store
            .db
            .query_row("SELECT account,id FROM messages LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        let source = b"From: Sender <sender@example.test>\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>Answers inline.</p><blockquote><p>Question one?</p><p>Yes, families.</p></blockquote>";
        let hash = store.stage_blob(source).unwrap();
        store
            .db
            .execute(
                "UPDATE messages SET source_hash=?1 WHERE account=?2 AND id=?3",
                params![hash, account, mid],
            )
            .unwrap();
        let request = json!({"id":mid,"source_version":hash});
        let result = read(
            store.message_source_begin(&account, &request).unwrap(),
            &CancellationToken::new(),
        )
        .unwrap();
        assert!(!result["body_text"]
            .as_str()
            .unwrap()
            .contains("Yes, families."));
        let full = result["body_full"].as_str().unwrap();
        assert!(
            full.contains("Answers inline.") && full.contains("> Yes, families."),
            "{full}"
        );
    }

    #[test]
    fn preview_bounds_preserve_unicode() {
        let text = "日".repeat(100_000);
        let (preview, truncated) = bounded(&text);
        assert!(truncated);
        assert!(preview.len() <= 256 * 1024);
        assert!(text.starts_with(&preview));
        let mut result = json!({"raw":"\u{1}".repeat(256*1024),"body_html":"\u{2}".repeat(256*1024),"body_text":"\u{3}".repeat(256*1024),"body_full":"\u{4}".repeat(256*1024),
            "raw_truncated":false,"html_truncated":false,"text_truncated":false,"full_truncated":false});
        fit_reply(&mut result).unwrap();
        assert!(serde_json::to_vec(&result).unwrap().len() <= 900 * 1024);
        for flag in [
            "raw_truncated",
            "html_truncated",
            "text_truncated",
            "full_truncated",
        ] {
            assert_eq!(result[flag], true);
        }
    }
}
