//! Plain text from received MIME, for the reader and the search index. HTML-only
//! mail is sanitized before conversion; the original MIME is kept elsewhere.
use crate::html_text::html_to_text;
use ammonia::{Builder, UrlRelative};
use mail_parser::{MessageParser, PartType};
use std::collections::{HashMap, HashSet};

/// Why no text could be extracted from received MIME.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncomingError {
    /// The raw message is over 64 MiB.
    SourceTooLarge,
    /// The bytes do not parse as a MIME message.
    InvalidMime,
}

/// Extract source text for both the reader and FTS. Prefer actual plain body
/// parts, never a text attachment or a generated HTML accessor. HTML-only mail
/// passes through a parser-based sanitizer before conversion to plain text.
/// The original MIME is retained separately by Core; this function performs no I/O.
pub fn extract_incoming_text(raw: &[u8]) -> Result<String, IncomingError> {
    if raw.len() > 64 * 1024 * 1024 {
        return Err(IncomingError::SourceTooLarge);
    }
    let message = MessageParser::default()
        .parse(raw)
        .ok_or(IncomingError::InvalidMime)?;
    extract_parsed_incoming_text(&message)
}

/// Reuse a caller's bounded MIME parse for normalization; performs no I/O.
pub fn extract_parsed_incoming_text(
    message: &mail_parser::Message<'_>,
) -> Result<String, IncomingError> {
    extract(message, false)
}

/// New mail's reading/search copy omits recognizable reply history. Core keeps
/// exact original MIME outside the database for source inspection and recovery.
pub fn extract_parsed_incoming_contribution(
    message: &mail_parser::Message<'_>,
) -> Result<String, IncomingError> {
    extract(message, true)
}

/// Picks the text source in order: real plain body parts, then HTML body parts,
/// then any other part labelled `text/html`, then HTML source found in plain
/// parts. mail-parser has already decoded transfer encodings and charsets, so
/// everything here is Unicode text. `contribution` drops reply history.
fn extract(
    message: &mail_parser::Message<'_>,
    contribution: bool,
) -> Result<String, IncomingError> {
    // Some senders put their HTML source in the text/plain part; that is read
    // as HTML, never shown or indexed as markup.
    let (plain, disguised): (Vec<&str>, Vec<&str>) = message
        .text_body
        .iter()
        .filter_map(|id| match &message.part(*id)?.body {
            PartType::Text(text) => Some(text.as_ref()),
            _ => None,
        })
        .partition(|text| !crate::preview::looks_like_html(text));
    if !plain.is_empty() {
        let text = plain.join("\n");
        return Ok(if contribution {
            crate::reply_contribution(&text)
        } else {
            text
        });
    }
    let mut html: Vec<&str> = message
        .html_body
        .iter()
        .filter_map(|id| match &message.part(*id)?.body {
            PartType::Html(html) => Some(html.as_ref()),
            _ => None,
        })
        .collect();
    // An empty HTML body can sit beside the real HTML part, for example one
    // with an unknown transfer encoding that mail-parser left as plain text.
    if html.iter().all(|html| html.trim().is_empty()) {
        html = crate::html_view::html_part(message)
            .filter(|html| !html.trim().is_empty())
            .into_iter()
            .collect();
    }
    if html.is_empty() {
        html = disguised;
    }
    let mut cleaner = cleaner();
    let text = html
        .into_iter()
        .map(|html| {
            let html = bounded(html);
            // Drop quote bodies before conversion, so HTML-only quotations
            // do not become indistinguishable from new unquoted prose. A
            // forward keeps its blockquotes: they are the forwarded content.
            let omit_quotes =
                contribution && !html.to_ascii_lowercase().contains("forwarded message");
            if omit_quotes {
                cleaner
                    .rm_tags(&["blockquote"])
                    .add_clean_content_tags(&["blockquote"]);
            } else {
                cleaner
                    .rm_clean_content_tags(&["blockquote"])
                    .add_tags(&["blockquote"]);
            }
            let text = clean_text(&cleaner, html);
            if contribution {
                // With the quotes already gone, no `>` lines follow an
                // attribution. The flag makes the text pass skip only the
                // attribution and keep the answers after it, not cut there.
                crate::reply_text::project(
                    &text,
                    omit_quotes && html.to_ascii_lowercase().contains("<blockquote"),
                )
            } else {
                text
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(text)
}

/// Readable text from HTML source, as HTML-only mail is read: scripts, styles
/// and other active content dropped, markup removed. Performs no I/O.
pub fn html_source_text(html: &str) -> String {
    clean_text(&cleaner(), bounded(html))
}

/// Most HTML read for text, in bytes (1 MiB); the rest is left out.
const MAX_TEXT_HTML_BYTES: usize = 1024 * 1024;

/// `html` cut to `MAX_TEXT_HTML_BYTES` at a character boundary.
fn bounded(html: &str) -> &str {
    let mut end = html.len().min(MAX_TEXT_HTML_BYTES);
    while !html.is_char_boundary(end) {
        end -= 1;
    }
    &html[..end]
}

/// Text through `cleaner`, or without structure when the HTML is built to
/// make parsing slow.
fn clean_text(cleaner: &Builder<'_>, html: &str) -> String {
    if !crate::html_cost::parses_cheaply(html) {
        return crate::html_cost::strip_tags(html);
    }
    html_to_text(&cleaner.clean(html).to_string())
}

/// Structure-only allowlist for text extraction. Scripts, styles, embedded
/// documents, forms, SVG and MathML go with their content, so their text never
/// reaches the reader or the search index; every attribute except `a href` goes.
fn cleaner() -> Builder<'static> {
    let mut cleaner = Builder::default();
    cleaner
        .tags(HashSet::from([
            "p",
            "div",
            "span",
            "br",
            "h1",
            "h2",
            "h3",
            "h4",
            "h5",
            "h6",
            "a",
            "strong",
            "b",
            "em",
            "i",
            "u",
            "ul",
            "ol",
            "li",
            "blockquote",
            "pre",
            "code",
            "table",
            "thead",
            "tbody",
            "tr",
            "td",
            "th",
            "hr",
        ]))
        .clean_content_tags(HashSet::from([
            "script", "style", "template", "iframe", "object", "embed", "form", "svg", "math",
        ]))
        .generic_attributes(HashSet::new())
        .tag_attributes(HashMap::from([("a", HashSet::from(["href"]))]))
        .url_schemes(HashSet::from(["http", "https", "mailto"]))
        .url_relative(UrlRelative::Deny);
    cleaner
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contribution(kind: &str, body: &str) -> String {
        let raw = format!("MIME-Version: 1.0\r\nContent-Type: {kind}; charset=utf-8\r\n\r\n{body}");
        let parsed = MessageParser::default().parse(raw.as_bytes()).unwrap();
        extract_parsed_incoming_contribution(&parsed).unwrap()
    }

    #[test]
    fn html_history_is_removed_before_text_conversion_without_losing_inline_answers() {
        let body = "<p>New answer</p><p>On Tuesday, Alex wrote:</p><blockquote><p>Old question</p><blockquote>Older</blockquote></blockquote><p>Inline answer</p>";
        let projected = contribution("text/html", body);
        assert!(
            projected.contains("New answer") && projected.contains("Inline answer"),
            "{projected}"
        );
        assert!(!projected.contains("Old") && !projected.contains("wrote:"));
        let outlook = contribution("text/html", "<p>New answer</p><p>------ Original Message ------<br>From Alex<br>To Sam</p><blockquote>Old answer</blockquote>");
        assert_eq!(outlook.trim(), "New answer");
        let forward = contribution("text/html", "<p>Please review</p><div class='gmail_quote'><p>---------- Forwarded message ----------</p><blockquote>Forwarded content</blockquote></div>");
        assert!(forward.contains("Forwarded content"));
        assert!(contribution(
            "text/plain",
            "Answer\r\n\r\nOn Tuesday, Alex wrote:\r\n> History"
        )
        .contains("Answer"));
    }

    #[test]
    fn html_source_in_a_plain_part_is_read_as_html() {
        let html = "<!DOCTYPE html PUBLIC '-//W3C//DTD XHTML 1.0 Strict//EN'><html><head><style>h1{font-size:28px}</style></head><body><h1>Your hub is ready</h1><script>x()</script></body></html>";
        let raw =
            format!("MIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{html}");
        let text = extract_incoming_text(raw.as_bytes()).unwrap();
        assert!(text.contains("Your hub is ready"), "{text}");
        assert!(
            !text.contains('<') && !text.contains("font-size") && !text.contains("x()"),
            "{text}"
        );
        let raw = format!("MIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary=b\r\n\r\n--b\r\nContent-Type: text/plain\r\n\r\n{html}\r\n--b\r\nContent-Type: text/html\r\n\r\n<p>Real HTML part</p>\r\n--b--\r\n");
        assert_eq!(
            extract_incoming_text(raw.as_bytes()).unwrap().trim(),
            "Real HTML part"
        );
    }

    #[test]
    fn hostile_html_is_read_quickly_and_long_html_is_cut() {
        let start = std::time::Instant::now();
        let nested = format!("<p>Hello</p>{}", "<div>".repeat(100_000));
        assert_eq!(contribution("text/html", &nested).trim(), "Hello");
        let took = start.elapsed();
        assert!(took < std::time::Duration::from_secs(2), "{took:?}");
        let long = format!(
            "<p>Start</p><p>{}</p><p>Past the cut</p>",
            "word ".repeat(300_000)
        );
        let text = contribution("text/html", &long);
        assert!(text.starts_with("Start") && text.contains("word"));
        assert!(!text.contains("Past the cut"));
    }
}
