//! Search box grammar shared by conversation and draft search (Search 7).
//! GUI and CLI send the same text. Core parses it once, reports how it read
//! the text and runs the typed predicates; clients never re-implement it.
use crate::{err, Result};
use serde_json::{json, Value};

/// A query may name at most this many senders (or recipients in Sent).
pub(crate) const MAX_FROM: usize = 4;
const MAX_FROM_BYTES: usize = 320;
const OPERATORS: [&str; 6] = ["subject", "body", "from", "label", "has", "is"];

/// How one search text was read. Values keep the user's spelling; matching
/// is case-insensitive.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct SearchText {
    /// Bare words. Each must appear in the subject or the message text.
    pub words: Vec<String>,
    pub subject: Vec<String>,
    pub body: Vec<String>,
    /// Sender name or address; recipients in Sent and Drafts.
    pub from: Vec<String>,
    pub label: Option<String>,
    pub has_attachment: bool,
    /// Review state: `review`, `held` (shown as delayed) or `done`.
    pub is: Option<&'static str>,
}

impl SearchText {
    pub(crate) fn is_empty(&self) -> bool {
        self.words.is_empty()
            && self.subject.is_empty()
            && self.body.is_empty()
            && self.from.is_empty()
            && self.label.is_none()
            && !self.has_attachment
            && self.is.is_none()
    }
    /// The reading returned to clients, which draw one chip per part. `label`
    /// is the resolved label name when the caller knows it.
    pub(crate) fn terms(&self, label: Option<&str>) -> Value {
        json!({
            "words": self.words.join(" "),
            "subject": self.subject,
            "body": self.body,
            "from": self.from,
            "label": label.or(self.label.as_deref()),
            "has_attachment": self.has_attachment,
            "is": self.is.map(|state| if state == "held" { "delayed" } else { state }),
        })
    }
}

/// Words FTS can match: runs of letters and digits, as the index tokenizes.
pub(crate) fn tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
}

fn looks_like_email(word: &str) -> bool {
    let Some((local, domain)) = word.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.contains('@')
        && domain
            .split_once('.')
            .is_some_and(|(host, top)| !host.is_empty() && !top.is_empty())
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value)
}

/// Splits `text` at each operator that starts the text or follows whitespace.
/// An operator's value runs until the next operator or the end.
fn segments(text: &str) -> Vec<(Option<&'static str>, &str)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let (mut key, mut start) = (None, 0);
    let mut i = 0;
    while i < bytes.len() {
        let boundary = i == 0 || text[..i].ends_with(char::is_whitespace);
        let found = boundary
            .then(|| {
                OPERATORS.iter().find(|op| {
                    let end = i + op.len();
                    end < bytes.len()
                        && bytes[i..end].eq_ignore_ascii_case(op.as_bytes())
                        && bytes[end] == b':'
                })
            })
            .flatten();
        if let Some(op) = found {
            out.push((key, &text[start..i]));
            key = Some(*op);
            start = i + op.len() + 1;
            i = start;
            continue;
        }
        i += text[i..].chars().next().map_or(1, char::len_utf8);
    }
    out.push((key, &text[start..]));
    out
}

/// Parses search text. An operator without a value is still being typed and
/// adds nothing. Unsupported values fail clearly instead of becoming words.
pub(crate) fn parse(text: &str) -> Result<SearchText> {
    let mut query = SearchText::default();
    let bare = |query: &mut SearchText, text: &str| {
        for word in text.split_whitespace() {
            if looks_like_email(word) {
                query.from.push(word.to_owned());
            } else {
                query.words.push(word.to_owned());
            }
        }
    };
    for (key, value) in segments(text) {
        let value = value.trim();
        let Some(key) = key else {
            bare(&mut query, value);
            continue;
        };
        if value.is_empty() {
            continue;
        }
        match key {
            "subject" => query.subject.push(unquote(value).trim().to_owned()),
            "body" => query.body.push(unquote(value).trim().to_owned()),
            "from" => query.from.push(unquote(value).trim().to_owned()),
            _ => {
                // label:, has: and is: take one word; the rest are bare words.
                // A quoted label keeps its spaces.
                let (word, rest) = match value.strip_prefix('"').and_then(|v| v.split_once('"')) {
                    Some((quoted, rest)) if key == "label" => (quoted.trim(), rest),
                    _ => {
                        let (word, rest) =
                            value.split_once(char::is_whitespace).unwrap_or((value, ""));
                        (unquote(word), rest)
                    }
                };
                bare(&mut query, rest);
                match key {
                    "label" => query.label = Some(word.to_owned()),
                    "has" if word.to_lowercase().starts_with("attach") => {
                        query.has_attachment = true
                    }
                    "has" => return Err(err("invalid_request", "has: accepts has:attachment")),
                    _ => {
                        query.is = Some(match word.to_lowercase().as_str() {
                            "review" => "review",
                            "delayed" | "held" => "held",
                            "done" => "done",
                            _ => {
                                return Err(err(
                                    "invalid_request",
                                    "is: accepts review, delayed or done",
                                ))
                            }
                        })
                    }
                }
            }
        }
    }
    query.subject.retain(|v| !v.is_empty());
    query.body.retain(|v| !v.is_empty());
    query.from.retain(|v| !v.is_empty());
    if query.from.len() > MAX_FROM || query.from.iter().any(|v| v.len() > MAX_FROM_BYTES) {
        return Err(err(
            "invalid_request",
            "Search accepts up to four from: names or addresses",
        ));
    }
    Ok(query)
}

/// One FTS5 group requiring every token as a prefix within `columns`.
pub(crate) fn fts_group(columns: &str, text: &str) -> Option<String> {
    let group = tokens(text)
        .map(|token| format!("\"{token}\"*"))
        .collect::<Vec<_>>()
        .join(" AND ");
    (!group.is_empty()).then(|| format!("{columns} : ({group})"))
}

/// `%value%` for LIKE with its wildcards escaped.
pub(crate) fn like_pattern(value: &str) -> String {
    format!(
        "%{}%",
        value
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> SearchText {
        parse(text).unwrap()
    }

    #[test]
    fn operator_values_run_to_the_next_operator_and_quotes_are_optional() {
        let query = read("subject:board deck from:maya");
        assert_eq!(query.subject, ["board deck"]);
        assert_eq!(query.from, ["maya"]);
        assert!(query.words.is_empty());
        assert_eq!(read("Subject:\"board deck\" FROM:maya"), query);
        let query = read("relay outage body:invoice attached");
        assert_eq!(query.words, ["relay", "outage"]);
        assert_eq!(query.body, ["invoice attached"]);
    }

    #[test]
    fn bare_addresses_become_from_and_other_words_stay_bare() {
        let query = read("maya@windscribe.com relay");
        assert_eq!(query.from, ["maya@windscribe.com"]);
        assert_eq!(query.words, ["relay"]);
        assert_eq!(read("a@b").words, ["a@b"]);
        assert_eq!(read("@windscribe.com").words, ["@windscribe.com"]);
    }

    #[test]
    fn label_has_and_is_take_one_word_and_return_the_rest() {
        let query = read("label:work planning has:attachment deck is:Delayed now");
        assert_eq!(query.label.as_deref(), Some("work"));
        assert!(query.has_attachment);
        assert_eq!(query.is, Some("held"));
        assert_eq!(query.words, ["planning", "deck", "now"]);
        assert_eq!(
            read("label:\"Work stuff\" relay").label.as_deref(),
            Some("Work stuff")
        );
        assert_eq!(read("label:\"Work stuff\" relay").words, ["relay"]);
        assert!(read("has:attach").has_attachment);
    }

    #[test]
    fn operators_need_a_boundary_and_empty_values_are_still_being_typed() {
        assert_eq!(read("resubject:x").words, ["resubject:x"]);
        assert_eq!(
            read("https://example.test/a").words,
            ["https://example.test/a"]
        );
        assert!(read("from:").is_empty());
        assert!(read("subject:   is:").is_empty());
        assert_eq!(read("fr").words, ["fr"]);
        assert_eq!(read("Élodie subject:café").subject, ["café"]);
        assert_eq!(read("Élodie subject:café").words, ["Élodie"]);
    }

    #[test]
    fn unsupported_values_fail_clearly() {
        for text in [
            "has:star",
            "is:snoozed",
            "from:a from:b from:c from:d from:e",
        ] {
            assert_eq!(parse(text).unwrap_err().code, "invalid_request", "{text}");
        }
        assert_eq!(
            parse(&format!("from:{}", "a".repeat(321)))
                .unwrap_err()
                .code,
            "invalid_request"
        );
    }

    #[test]
    fn reading_reports_each_part_for_chips() {
        let terms = read("relay subject:board deck is:held").terms(None);
        assert_eq!(terms["words"], "relay");
        assert_eq!(terms["subject"], json!(["board deck"]));
        assert_eq!(terms["is"], "delayed");
        assert_eq!(terms["label"], Value::Null);
        assert_eq!(read("label:wo").terms(Some("Work"))["label"], "Work");
    }

    #[test]
    fn fts_groups_prefix_every_token_within_columns() {
        assert_eq!(
            fts_group("{subject body}", "re-launch plan").as_deref(),
            Some("{subject body} : (\"re\"* AND \"launch\"* AND \"plan\"*)")
        );
        assert_eq!(fts_group("subject", "&&").as_deref(), None);
        assert_eq!(like_pattern("50%_a\\b"), "%50\\%\\_a\\\\b%");
    }
}
