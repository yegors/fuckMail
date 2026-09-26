//! Search box grammar shared by conversation and draft search.
//! GUI and CLI send the same text. Core parses it once, reports how it read
//! the text and runs the typed predicates; clients never re-implement it.
//!
//! The grammar: `subject:`, `body:`, `from:` and `to:` take everything up to
//! the next operator, quotes optional; `label:`, `has:attachment` and `is:`
//! take one word. Operators are case-insensitive and count only at the start
//! of the text or after whitespace. Other text is bare words; words typed
//! together form a phrase, and a bare email address names a sender or
//! recipient instead.
//!
//! Mail search runs in two stages. [`fts_phrase`] turns each phrase into an
//! FTS5 phrase query that finds candidates by their tokens. The index
//! tokenizes with `unicode61 remove_diacritics 2`, which drops punctuation,
//! so [`contains_phrase`] then checks the folded text itself for phrases with
//! punctuation or several words. People filters use LIKE, not the index.
use crate::{err, Result};
use serde_json::{json, Value};

/// A query may name at most this many people across from:, to: and bare addresses.
pub(crate) const MAX_FROM: usize = 4;
const MAX_FROM_BYTES: usize = 320;
const OPERATORS: [&str; 7] = ["subject", "body", "from", "to", "label", "has", "is"];

/// How one search text was read. Values keep the user's spelling; matching
/// is case-insensitive.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct SearchText {
    /// Bare words, as typed.
    pub words: Vec<String>,
    /// Bare words typed together, without surrounding quotes. Each phrase must
    /// appear as written in the subject or the message text.
    pub phrases: Vec<String>,
    /// `subject:` values; each must appear as written in the subject.
    pub subject: Vec<String>,
    /// `body:` values; each must appear as written in the message text.
    pub body: Vec<String>,
    /// Sender name or address.
    pub from: Vec<String>,
    /// Recipient (To or Cc) name or address.
    pub to: Vec<String>,
    /// Bare addresses: the sender or a recipient.
    pub people: Vec<String>,
    /// The `label:` name as typed; the caller resolves it to a label.
    pub label: Option<String>,
    /// `has:attachment` (any word starting with `attach`) was given.
    pub has_attachment: bool,
    /// Review state: `review`, `held` (shown as delayed) or `done`.
    pub is: Option<&'static str>,
}

impl SearchText {
    /// Whether the text sets no filter at all. Phrases are not checked: every
    /// phrase is built from bare words.
    pub(crate) fn is_empty(&self) -> bool {
        self.words.is_empty()
            && self.subject.is_empty()
            && self.body.is_empty()
            && self.from.is_empty()
            && self.to.is_empty()
            && self.people.is_empty()
            && self.label.is_none()
            && !self.has_attachment
            && self.is.is_none()
    }
    /// The reading returned to clients, which draw one chip per part. `label`
    /// is the resolved label name when the caller knows it.
    pub(crate) fn terms(&self, label: Option<&str>) -> Value {
        json!({
            "words": self.words.join(" "),
            // What clients highlight: each phrase and where it must appear.
            "phrases": self.phrases.iter().map(|text| json!({"text": text, "field": "any"}))
                .chain(self.subject.iter().map(|text| json!({"text": text, "field": "subject"})))
                .chain(self.body.iter().map(|text| json!({"text": text, "field": "body"})))
                .collect::<Vec<_>>(),
            "subject": self.subject,
            "body": self.body,
            "from": self.from,
            "to": self.to,
            "people": self.people,
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

/// Whether a bare word should filter by person: one `@` with a non-empty
/// local part and a dotted domain. `a@b` and `@example.com` stay words.
pub(crate) fn looks_like_email(word: &str) -> bool {
    let Some((local, domain)) = word.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.contains('@')
        && domain
            .split_once('.')
            .is_some_and(|(host, top)| !host.is_empty() && !top.is_empty())
}

/// Strips one pair of surrounding double quotes, if present.
pub(crate) fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value)
}

/// Splits `text` at each operator that starts the text or follows whitespace.
/// An operator's value runs until the next operator or the end.
fn segments(text: &str) -> Vec<(Option<&'static str>, &str)> {
    segments_with(text, &OPERATORS)
}

/// `segments` with another operator set, such as Calendar search's.
pub(crate) fn segments_with<'a>(
    text: &'a str,
    operators: &[&'static str],
) -> Vec<(Option<&'static str>, &'a str)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let (mut key, mut start) = (None, 0);
    let mut i = 0;
    while i < bytes.len() {
        let boundary = i == 0 || text[..i].ends_with(char::is_whitespace);
        let found = boundary
            .then(|| {
                operators.iter().find(|op| {
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
    // Words typed together form one phrase; an address or an operator ends it.
    let bare = |query: &mut SearchText, text: &str| {
        let mut phrase: Vec<&str> = Vec::new();
        let close = |query: &mut SearchText, phrase: &mut Vec<&str>| {
            let text = phrase.join(" ");
            let text = text.trim_matches('"').trim();
            if !text.is_empty() {
                query.phrases.push(text.to_owned());
            }
            phrase.clear();
        };
        for word in text.split_whitespace() {
            if looks_like_email(word) {
                close(query, &mut phrase);
                query.people.push(word.to_owned());
            } else {
                query.words.push(word.to_owned());
                phrase.push(word);
            }
        }
        close(query, &mut phrase);
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
            "to" => query.to.push(unquote(value).trim().to_owned()),
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
    query.to.retain(|v| !v.is_empty());
    let people = || query.from.iter().chain(&query.to).chain(&query.people);
    if people().count() > MAX_FROM || people().any(|v| v.len() > MAX_FROM_BYTES) {
        return Err(err(
            "invalid_request",
            "Search accepts up to four names or addresses across from:, to: and bare addresses",
        ));
    }
    Ok(query)
}

/// One FTS5 phrase within `columns`: the text's tokens in order, the last as
/// a prefix. It finds candidates; `contains_phrase` then checks the text.
pub(crate) fn fts_phrase(columns: &str, text: &str) -> Option<String> {
    let phrase = tokens(text).collect::<Vec<_>>().join(" ");
    // Tokens hold only letters and digits, so nothing inside the quotes needs
    // escaping. The trailing `*` lets a word still being typed match.
    (!phrase.is_empty()).then(|| format!("{columns} : \"{phrase}\"*"))
}

/// Text for phrase matching: lower case, Latin accents removed as the index
/// does, and each run of whitespace (line breaks included) as one space.
pub(crate) fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    fold_each(text, |c, _| out.push(c));
    out
}

/// The character position in `text` where folded `phrase` first matches from
/// the start of a word, as `contains_phrase` decides.
pub(crate) fn find_phrase(text: &str, phrase: &str) -> Option<usize> {
    let (mut folded, mut origins) = (String::with_capacity(text.len()), Vec::new());
    fold_each(text, |c, origin| {
        folded.push(c);
        origins.push(origin);
    });
    let word_start = phrase.starts_with(char::is_alphanumeric);
    folded
        .match_indices(phrase)
        .find(|(at, _)| !word_start || !folded[..*at].ends_with(char::is_alphanumeric))
        .map(|(at, _)| origins[folded[..at].chars().count()])
}

/// Each folded character with the position of the character it came from.
fn fold_each(text: &str, mut push: impl FnMut(char, usize)) {
    // Base letters for U+0100..U+017F; '.' keeps the character.
    const EXTENDED_A: &[u8; 128] = b"aaaaaaccccccccddddeeeeeeeeeegggggggghhhhiiiiiiiiii..jjkk.llllllllllnnnnnn...oooooo..rrrrrrssssssssttttttuuuuuuuuuuuuwwyyyzzzzzzs";
    let (mut space, mut started) = (None, false);
    for (index, c) in text.chars().enumerate() {
        if c.is_whitespace() {
            space.get_or_insert(index);
            continue;
        }
        if let Some(at) = space.take().filter(|_| started) {
            push(' ', at);
        }
        started = true;
        for c in c.to_lowercase() {
            let base = match c {
                '\u{300}'..='\u{36f}' => continue,
                'à'..='å' => 'a',
                'ç' => 'c',
                'è'..='ë' => 'e',
                'ì'..='ï' => 'i',
                'ñ' => 'n',
                'ò'..='ö' | 'ø' => 'o',
                'ù'..='ü' => 'u',
                'ý' | 'ÿ' => 'y',
                '\u{100}'..='\u{17f}' => match EXTENDED_A[c as usize - 0x100] {
                    b'.' => c,
                    base => base as char,
                },
                _ => c,
            };
            push(base, index);
        }
    }
}

/// Whether folded `text` holds folded `phrase` starting at the beginning of a
/// word. The phrase may end inside a word, so its last word can be partial.
pub(crate) fn contains_phrase(text: &str, phrase: &str) -> bool {
    let word_start = phrase.starts_with(char::is_alphanumeric);
    text.match_indices(phrase)
        .any(|(at, _)| !word_start || !text[..at].ends_with(char::is_alphanumeric))
}

/// Whether a folded phrase needs [`contains_phrase`] after the index match.
/// Only a single word of letters and digits is decided by the index alone.
pub(crate) fn phrase_needs_check(folded: &str) -> bool {
    !folded.chars().all(char::is_alphanumeric)
}

/// `%value%` for LIKE with its wildcards escaped. The query must declare
/// `ESCAPE '\'`.
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
    fn from_and_to_name_one_side_each() {
        let query = read("to:priya@windscribe.com from:Maya deck");
        assert_eq!(query.to, ["priya@windscribe.com"]);
        assert_eq!(query.from, ["Maya deck"]);
        assert_eq!(read("TO:priya").to, ["priya"]);
        assert_eq!(read("tomorrow").words, ["tomorrow"]);
        assert!(parse("c@d.co f@g.co i@j.co from:a to:b").is_err());
        assert!(parse("c@d.co f@g.co from:a to:b").is_ok());
    }

    #[test]
    fn bare_addresses_name_either_side_and_other_words_stay_bare() {
        let query = read("maya@windscribe.com relay");
        assert_eq!(query.people, ["maya@windscribe.com"]);
        assert!(query.from.is_empty() && query.to.is_empty());
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
    fn fts_phrases_keep_token_order_with_a_prefix_last_token() {
        assert_eq!(
            fts_phrase("{subject body}", "re-launch plan").as_deref(),
            Some("{subject body} : \"re launch plan\"*")
        );
        assert_eq!(fts_phrase("subject", "&&").as_deref(), None);
        assert_eq!(like_pattern("50%_a\\b"), "%50\\%\\_a\\\\b%");
    }

    #[test]
    fn words_typed_together_form_one_phrase() {
        assert_eq!(read("Bill C-22").phrases, ["Bill C-22"]);
        assert_eq!(read("\"open  letter\"").phrases, ["open letter"]);
        assert_eq!(read("c-22 from:josh").phrases, ["c-22"]);
        assert_eq!(
            read("invoice maya@windscribe.com march").phrases,
            ["invoice", "march"]
        );
        assert_eq!(read("label:work bill c-22").phrases, ["bill c-22"]);
        assert_eq!(read("bill label:work c-22").phrases, ["bill", "c-22"]);
        assert!(read("\"\"").phrases.is_empty());
    }

    #[test]
    fn phrases_match_as_written_from_a_word_start() {
        let text = fold("Re[2]: Bill C-22\r\n  Option Illustrations for Café Élan");
        assert_eq!(fold("Élodie  ŁÓDŹ\tİstanbul"), "elodie lodz istanbul");
        for (phrase, found) in [
            ("c-22", true),
            ("C-2", true),
            ("bill c-22 option", true),
            ("-22", true),
            ("22", true),
            ("cafe elan", true),
            ("c 22", false),
            ("c22", false),
            ("ill c-22", false),
            ("option bill", false),
        ] {
            assert_eq!(contains_phrase(&text, &fold(phrase)), found, "{phrase}");
        }
        assert!(!contains_phrase(&fold("Sep 22 call"), &fold("c-22")));
        assert_eq!(
            find_phrase("Cover  Élan, the Bill\nC-22 note", &fold("bill c-22")),
            Some(17)
        );
        assert_eq!(find_phrase("Cover note", &fold("c-22")), None);
        let terms = read("Bill C-22 subject:weekly body:\"open letter\"").terms(None);
        assert_eq!(
            terms["phrases"],
            json!([{"text":"Bill C-22","field":"any"},{"text":"weekly","field":"subject"},{"text":"open letter","field":"body"}])
        );
        assert!(!phrase_needs_check(&fold("Invoice")));
        assert!(phrase_needs_check(&fold("c-22")) && phrase_needs_check(&fold("open letter")));
    }
}
