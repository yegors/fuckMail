//! One-line conversation previews built from message text. Pure; no I/O.
use regex::Regex;
use std::borrow::Cow;
use std::sync::OnceLock;

/// HTML source read for a preview is cut to this many bytes first.
const MAX_PREVIEW_HTML: usize = 64 * 1024;

/// Whether a text body is really HTML source, as some senders' text/plain
/// parts are: it starts with a document start or a common element tag.
pub fn looks_like_html(text: &str) -> bool {
    let head: String = text
        .trim_start_matches(['\u{feff}', ' ', '\t', '\r', '\n'])
        .chars()
        .take(16)
        .collect::<String>()
        .to_ascii_lowercase();
    if ["<!doctype html", "<?xml"]
        .iter()
        .any(|start| head.starts_with(start))
    {
        return true;
    }
    let Some(rest) = head.strip_prefix('<') else {
        return false;
    };
    let name: String = rest
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect();
    let after = rest[name.len()..].chars().next();
    matches!(after, Some('>' | ' ' | '/' | '\t' | '\r' | '\n'))
        && matches!(
            name.as_str(),
            "html"
                | "head"
                | "body"
                | "meta"
                | "style"
                | "table"
                | "div"
                | "p"
                | "span"
                | "br"
                | "a"
                | "b"
                | "strong"
                | "font"
                | "center"
                | "h1"
                | "h2"
                | "h3"
                | "ul"
                | "img"
        )
}

/// Text a preview can be cut from: HTML source becomes its readable text.
pub fn preview_source(body: &str) -> Cow<'_, str> {
    if !looks_like_html(body) {
        return Cow::Borrowed(body);
    }
    let mut end = body.len().min(MAX_PREVIEW_HTML);
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    Cow::Owned(crate::html_source_text(&body[..end]))
}

/// Preview text without web addresses: a link, a short lead-in before it
/// ("Review your quote: https://…") and a bracketed link go, with the stray
/// punctuation and spaces they leave. Whitespace collapses to single spaces.
pub fn without_links(text: &str) -> String {
    static ANGLED: OnceLock<Regex> = OnceLock::new();
    static BRACKETED: OnceLock<Regex> = OnceLock::new();
    static LINK: OnceLock<Regex> = OnceLock::new();
    static BEFORE_PUNCTUATION: OnceLock<Regex> = OnceLock::new();
    static DANGLING_COLON: OnceLock<Regex> = OnceLock::new();
    // "<https://…>" goes with its brackets, as does an empty pair left behind.
    let angled =
        ANGLED.get_or_init(|| Regex::new(r"\s*<(?:(?:https?://|mailto:)[^>\s]*)?>").unwrap());
    // "(also on https://…)" goes whole, with its lead-in.
    let bracketed =
        BRACKETED.get_or_init(|| Regex::new(r"\s*\([^()]{0,40}?https?://[^\s()]+\)").unwrap());
    let link = LINK.get_or_init(|| {
        Regex::new(r"\s*\(?(?:[A-Z][A-Za-z0-9_ ’']{0,30}: )?https?://[^\s)]+\)?").unwrap()
    });
    let before = BEFORE_PUNCTUATION.get_or_init(|| Regex::new(r"\s+([,.;:])").unwrap());
    let dangling = DANGLING_COLON.get_or_init(|| Regex::new(r":\s*(—|$)").unwrap());
    let text = angled.replace_all(text, "");
    let text = bracketed.replace_all(&text, "");
    let text = link.replace_all(&text, "");
    let text = before.replace_all(&text, "$1");
    let text = dangling.replace_all(&text, "$1");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angle_bracketed_links_go_with_their_brackets() {
        assert_eq!(
            without_links("Watch <https://youtu.be/x> ----- Hi, <> views changed"),
            "Watch ----- Hi, views changed"
        );
        assert!(looks_like_html("<p>Dear <strong>you</strong>,</p>"));
        assert!(looks_like_html("  <span>i</span><span>C</span>"));
        assert!(!looks_like_html("<ann@example.com> wrote:"));
        assert!(!looks_like_html("<3 thanks"));
    }

    #[test]
    fn links_leave_previews_with_their_lead_in_and_punctuation() {
        assert_eq!(
            without_links("Review your quote: https://q.example.com/a?b=1 before Friday."),
            "before Friday."
        );
        assert_eq!(
            without_links("Deck attached (also on https://www.rightscon.org/sponsor-2027). My recommendation below"),
            "Deck attached. My recommendation below"
        );
        assert_eq!(
            without_links("See https://x.test/a , then reply"),
            "See, then reply"
        );
        assert_eq!(
            without_links("Agenda:  https://docs.example.com/d"),
            "Agenda"
        );
        assert_eq!(
            without_links("Plain   text\r\nwith lines"),
            "Plain text with lines"
        );
    }

    #[test]
    fn html_source_in_a_text_body_previews_as_its_text() {
        let body = "<!DOCTYPE html PUBLIC '-//W3C//DTD XHTML 1.0 Strict//EN'><html><head><style>p{color:red}</style></head><body><p>Your hub is ready</p></body></html>";
        assert!(looks_like_html(body));
        assert_eq!(without_links(&preview_source(body)), "Your hub is ready");
        assert!(!looks_like_html("Hello <b>there</b>"));
        assert_eq!(preview_source("Hello"), "Hello");
    }
}
