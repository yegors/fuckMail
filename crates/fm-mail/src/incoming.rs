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
    let plain: Vec<&str> = message
        .text_body
        .iter()
        .filter_map(|id| match &message.part(*id)?.body {
            PartType::Text(text) => Some(text.as_ref()),
            _ => None,
        })
        .collect();
    if !plain.is_empty() {
        return Ok(plain.join("\n"));
    }
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
    let text = message
        .html_body
        .iter()
        .filter_map(|id| match &message.part(*id)?.body {
            PartType::Html(html) => Some(html_to_text(&cleaner.clean(html).to_string())),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(text)
}
