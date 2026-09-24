use ammonia::{Builder, UrlRelative};
use mail_parser::{decoders::html::html_to_text, MessageParser, PartType};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncomingError {
    SourceTooLarge,
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
            // Drop quote bodies before conversion, so HTML-only quotations
            // do not become indistinguishable from new unquoted prose.
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
            let text = html_to_text(&cleaner.clean(html).to_string());
            if contribution {
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
    html_to_text(&cleaner().clean(html).to_string())
}

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
}
