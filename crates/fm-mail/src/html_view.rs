//! Received HTML for display: a strict allowlist rebuild of the sender's markup.
//!
//! The output is a complete, reserialised document that only ever references
//! `fm-res:N` images and `fm-link:N` anchors. Remote URLs, `cid:` references and
//! `data:` images stay in the returned manifest, so a renderer cannot reach the
//! network or the file system by itself. Pure transform: no I/O.
use cssparser::{
    AtRuleParser, CowRcStr, DeclarationParser, ParseError, Parser, ParserInput, ParserState,
    QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, StyleSheetParser, ToCss, Token,
};
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use std::collections::BTreeSet;
use std::fmt::Write as _;

/// Bump when output or manifest meaning changes, so cached copies are rebuilt.
pub const SANITIZER_VERSION: u32 = 1;
pub const MAX_HTML_BYTES: usize = 4 * 1024 * 1024;
const MAX_NODES: usize = 50_000;
const MAX_DEPTH: usize = 256;
const MAX_TABLE_DEPTH: usize = 16;
const MAX_CSS_RULES: usize = 2_000;
const MAX_CSS_BYTES: usize = 256 * 1024;
const MAX_RESOURCES: usize = 500;
const MAX_LINKS: usize = 2_000;
const MAX_DATA_URI: usize = 2 * 1024 * 1024;
const MAX_URL: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceKind {
    /// An https image. Only the engine's remote-content path may fetch it.
    Remote,
    /// An `http://` image. Never requested.
    Insecure,
    /// A MIME part of this message, by Content-ID.
    Cid,
    /// An inline `data:image/...;base64` payload.
    Data,
}

impl ResourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceKind::Remote => "remote",
            ResourceKind::Insecure => "insecure",
            ResourceKind::Cid => "cid",
            ResourceKind::Data => "data",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Resource {
    pub id: usize,
    pub kind: ResourceKind,
    /// Engine-only: the URL, Content-ID or data URI. Never sent to the GUI.
    pub source: String,
    /// Lowercase host for remote and insecure resources.
    pub host: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub alt: String,
}

#[derive(Debug, Clone)]
pub struct Link {
    pub id: usize,
    pub href: String,
    pub scheme: String,
    /// Lowercase ASCII host; empty for mailto.
    pub host: String,
}

#[derive(Debug, Default)]
pub struct HtmlView {
    pub html: String,
    pub resources: Vec<Resource>,
    pub links: Vec<Link>,
    /// Likely tracking images that were dropped and are never fetched.
    pub trackers: usize,
    pub quote_removed: bool,
    /// Some content was dropped or flattened at a limit.
    pub limited: bool,
}

/// The first actual `text/html` part. mail-parser also lists a plain part in
/// `html_body` for plain-only mail, which must not count as HTML.
pub fn html_part<'a>(message: &'a mail_parser::Message<'a>) -> Option<&'a str> {
    message
        .html_body
        .iter()
        .find_map(|id| match &message.part(*id)?.body {
            mail_parser::PartType::Html(html) => Some(html.as_ref()),
            _ => None,
        })
}

/// Rebuild `html` for display. Returns `None` when the input exceeds the size limit.
pub fn sanitize(html: &str) -> Option<HtmlView> {
    if html.len() > MAX_HTML_BYTES {
        return None;
    }
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    let keep_quotes = html.to_ascii_lowercase().contains("forwarded message");
    let mut walk = Walk {
        view: HtmlView::default(),
        body: String::new(),
        css: String::new(),
        css_rules: 0,
        nodes: 0,
        table_depth: 0,
        flat_tables: 0,
        stopped: false,
        keep_quotes,
        body_attrs: String::new(),
    };
    walk.document(&dom.document);
    let mut out = String::with_capacity(walk.body.len() + walk.css.len() + 128);
    out.push_str("<!DOCTYPE html><html><head>");
    if !walk.css.is_empty() {
        out.push_str("<style>");
        out.push_str(&walk.css);
        out.push_str("</style>");
    }
    out.push_str("</head><body");
    out.push_str(&walk.body_attrs);
    out.push('>');
    out.push_str(&walk.body);
    out.push_str("</body></html>");
    walk.view.html = out;
    Some(walk.view)
}

/// Link hosts for the link index: every http(s) link host in the displayed
/// HTML plus every URL host in the plain text. Lowercase, without `www.`.
pub fn link_hosts(html: Option<&str>, plain: &str) -> BTreeSet<String> {
    let mut hosts = BTreeSet::new();
    if let Some(view) = html.and_then(sanitize) {
        for link in view.links {
            if !link.host.is_empty() {
                hosts.insert(display_host(&link.host).to_owned());
            }
        }
    }
    for (start, end) in text_links(plain) {
        if let Some(url) = parse_url(&plain[start..end]) {
            if let Some(host) = url.host_str() {
                hosts.insert(display_host(&host.to_ascii_lowercase()).to_owned());
            }
        }
    }
    hosts
}

/// `www.` is not part of the identity people read. Blocklists and counts use this form.
pub fn display_host(host: &str) -> &str {
    host.strip_prefix("www.").unwrap_or(host)
}

/// A blocklist entry as people type it: `tracker.io` blocks that host,
/// `*.tracker.io` blocks it and every subdomain. A pasted URL is reduced to its
/// host; `www.` is ignored; international names are stored as ASCII.
pub fn normalize_block_pattern(input: &str) -> Option<String> {
    let mut value = input.trim().to_lowercase();
    if let Some(index) = value.find("://") {
        value = value[index + 3..].to_owned();
    }
    if let Some(index) = value.find(['/', '?', '#']) {
        value.truncate(index);
    }
    let wildcard = value.starts_with("*.");
    let name = value.trim_start_matches("*.").trim_start_matches("www.");
    if name.is_empty() || name.contains(['*', ':', '@', ' ']) || name.len() > 253 {
        return None;
    }
    let ascii = url::Url::parse(&format!("https://{name}/"))
        .ok()?
        .host_str()?
        .to_owned();
    let labels: Vec<&str> = ascii.split('.').collect();
    let valid = labels.len() >= 2
        && labels.iter().all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
        && labels.last().is_some_and(|tld| {
            tld.len() >= 2
                && tld.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                && !tld.chars().all(|c| c.is_ascii_digit())
        });
    valid.then(|| {
        if wildcard {
            format!("*.{ascii}")
        } else {
            ascii
        }
    })
}

/// The first pattern that blocks `host` (compared without `www.`).
pub fn blocked_by<'a>(patterns: impl IntoIterator<Item = &'a str>, host: &str) -> Option<&'a str> {
    let host = display_host(host);
    patterns
        .into_iter()
        .find(|pattern| match pattern.strip_prefix("*.") {
            Some(base) => {
                host == base
                    || host
                        .strip_suffix(base)
                        .is_some_and(|rest| rest.ends_with('.'))
            }
            None => host == *pattern,
        })
}

/// Byte ranges of `https?://` URLs in plain text. Trailing punctuation that
/// usually ends a sentence is excluded.
pub fn text_links(text: &str) -> Vec<(usize, usize)> {
    let lower = text.to_ascii_lowercase();
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut from = 0;
    while from < text.len() && out.len() < MAX_LINKS {
        let next = [
            lower[from..].find("https://"),
            lower[from..].find("http://"),
        ]
        .into_iter()
        .flatten()
        .min();
        let Some(offset) = next else { break };
        let start = from + offset;
        let mut end = start;
        while end < bytes.len() {
            let c = bytes[end];
            if c.is_ascii_whitespace() || matches!(c, b'<' | b'>' | b'"' | b'\'' | b')') {
                break;
            }
            end += 1;
        }
        while end > start && matches!(bytes[end - 1], b'.' | b',' | b';' | b':' | b'!' | b'?') {
            end -= 1;
        }
        let scheme_end = start
            + if lower[start..].starts_with("https://") {
                8
            } else {
                7
            };
        if end > scheme_end {
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            out.push((start, end));
        }
        from = end.max(scheme_end);
    }
    out
}

fn parse_url(value: &str) -> Option<url::Url> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_URL {
        return None;
    }
    url::Url::parse(value).ok()
}

struct Walk {
    view: HtmlView,
    body: String,
    css: String,
    css_rules: usize,
    nodes: usize,
    table_depth: usize,
    /// Tables past the nesting limit whose structure is being unwrapped.
    flat_tables: usize,
    stopped: bool,
    keep_quotes: bool,
    body_attrs: String,
}

const DROP_WITH_CONTENT: &[&str] = &[
    "script", "template", "iframe", "frame", "frameset", "object", "embed", "applet", "param",
    "svg", "math", "video", "audio", "source", "track", "canvas", "input", "button", "select",
    "textarea", "option", "optgroup", "datalist", "output", "meter", "progress", "link", "meta",
    "base", "title", "area", "dialog", "portal", "xml",
];

const KEEP: &[&str] = &[
    "a",
    "abbr",
    "address",
    "article",
    "aside",
    "b",
    "bdi",
    "bdo",
    "big",
    "blockquote",
    "br",
    "caption",
    "center",
    "cite",
    "code",
    "col",
    "colgroup",
    "dd",
    "del",
    "details",
    "dfn",
    "div",
    "dl",
    "dt",
    "em",
    "figcaption",
    "figure",
    "font",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "i",
    "img",
    "ins",
    "kbd",
    "li",
    "main",
    "mark",
    "nav",
    "ol",
    "p",
    "pre",
    "q",
    "s",
    "samp",
    "section",
    "small",
    "span",
    "strike",
    "strong",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "time",
    "tr",
    "tt",
    "u",
    "ul",
    "var",
    "wbr",
];

const VOID: &[&str] = &["br", "col", "hr", "img", "wbr"];

impl Walk {
    fn document(&mut self, document: &Handle) {
        // Collect every style element first: email clients put them in head
        // and body alike, and cascade order follows document order.
        self.collect_styles(document);
        let html = child_element(document, "html");
        let Some(html) = html else { return };
        if let Some(body) = child_element(&html, "body") {
            self.body_attrs = self.attributes("body", &body);
            self.children(&body, 1);
        }
    }

    fn collect_styles(&mut self, root: &Handle) {
        let mut stack = vec![root.clone()];
        while let Some(node) = stack.pop() {
            if let NodeData::Element { name, .. } = &node.data {
                if &*name.local == "style" {
                    let mut css = String::new();
                    for child in node.children.borrow().iter() {
                        if let NodeData::Text { contents } = &child.data {
                            css.push_str(&contents.borrow());
                        }
                    }
                    self.stylesheet(&css);
                    continue;
                }
                if DROP_WITH_CONTENT.contains(&&*name.local) {
                    continue;
                }
            }
            for child in node.children.borrow().iter().rev() {
                stack.push(child.clone());
            }
        }
    }

    fn children(&mut self, node: &Handle, depth: usize) {
        for child in node.children.borrow().iter() {
            if self.stopped {
                return;
            }
            self.node(child, depth);
        }
    }

    fn node(&mut self, node: &Handle, depth: usize) {
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            self.view.limited = true;
            self.stopped = true;
            return;
        }
        match &node.data {
            NodeData::Text { contents } => escape_text(&contents.borrow(), &mut self.body),
            NodeData::Element { name, .. } => {
                let tag = name.local.as_ref().to_ascii_lowercase();
                if tag == "style" || tag == "head" || DROP_WITH_CONTENT.contains(&tag.as_str()) {
                    return;
                }
                if !self.keep_quotes && is_quote(&tag, node) {
                    self.view.quote_removed = true;
                    if attr(node, "id").is_some_and(|id| id.eq_ignore_ascii_case("divRplyFwdMsg")) {
                        // Outlook puts the quoted message after this header as siblings.
                        self.stopped = true;
                    }
                    return;
                }
                if depth > MAX_DEPTH {
                    self.view.limited = true;
                    self.flat_text(node);
                    return;
                }
                if tag == "img" {
                    self.image(node);
                    return;
                }
                let table = tag == "table";
                let table_part = matches!(
                    tag.as_str(),
                    "tr" | "td"
                        | "th"
                        | "tbody"
                        | "thead"
                        | "tfoot"
                        | "caption"
                        | "col"
                        | "colgroup"
                );
                if table && (self.flat_tables > 0 || self.table_depth >= MAX_TABLE_DEPTH) {
                    // Past the nesting limit: keep the content, drop the grid.
                    self.view.limited = true;
                    self.flat_tables += 1;
                    self.children(node, depth + 1);
                    self.flat_tables -= 1;
                    return;
                }
                if !KEEP.contains(&tag.as_str())
                    || (table_part && (self.flat_tables > 0 || self.table_depth == 0))
                {
                    self.children(node, depth + 1);
                    if tag == "tr" && self.flat_tables > 0 {
                        self.body.push_str("<br>");
                    }
                    return;
                }
                let attrs = self.attributes(&tag, node);
                let _ = write!(self.body, "<{tag}{attrs}>");
                if VOID.contains(&tag.as_str()) {
                    return;
                }
                if table {
                    self.table_depth += 1;
                }
                self.children(node, depth + 1);
                if table {
                    self.table_depth -= 1;
                }
                let _ = write!(self.body, "</{tag}>");
            }
            _ => {}
        }
    }

    /// Text only, iteratively, for content nested past the depth limit.
    fn flat_text(&mut self, node: &Handle) {
        let mut stack = vec![node.clone()];
        while let Some(node) = stack.pop() {
            self.nodes += 1;
            if self.nodes > MAX_NODES {
                self.stopped = true;
                return;
            }
            match &node.data {
                NodeData::Text { contents } => escape_text(&contents.borrow(), &mut self.body),
                NodeData::Element { name, .. } => {
                    let tag = name.local.as_ref().to_ascii_lowercase();
                    if tag == "style" || DROP_WITH_CONTENT.contains(&tag.as_str()) {
                        continue;
                    }
                    for child in node.children.borrow().iter().rev() {
                        stack.push(child.clone());
                    }
                }
                _ => {}
            }
        }
    }

    fn image(&mut self, node: &Handle) {
        let width = attr(node, "width").and_then(|v| pixels(&v));
        let height = attr(node, "height").and_then(|v| pixels(&v));
        let style = attr(node, "style").unwrap_or_default().to_ascii_lowercase();
        let compact: String = style.chars().filter(|c| !c.is_whitespace()).collect();
        let hidden = compact.contains("display:none")
            || compact.contains("visibility:hidden")
            || compact.contains("opacity:0;")
            || compact.ends_with("opacity:0");
        let tiny = matches!((width, height), (Some(w), Some(h)) if w <= 3 && h <= 3)
            || width == Some(0)
            || height == Some(0);
        if hidden || tiny {
            self.view.trackers += 1;
            return;
        }
        let Some(src) = attr(node, "src") else { return };
        let alt = attr(node, "alt").unwrap_or_default();
        let Some(id) = self.resource(&src, width, height, &alt) else {
            return;
        };
        let mut attrs = String::new();
        let _ = write!(attrs, " src=\"fm-res:{id}\"");
        if let Some(w) = width {
            let _ = write!(attrs, " width=\"{w}\"");
        }
        if let Some(h) = height {
            let _ = write!(attrs, " height=\"{h}\"");
        }
        for name in ["alt", "title", "align", "border"] {
            if let Some(value) = attr(node, name) {
                if let Some(value) = self.attribute_value("img", name, &value) {
                    let _ = write!(attrs, " {name}=\"{value}\"");
                }
            }
        }
        if let Some(css) = attr(node, "style") {
            let css = self.declarations(&css);
            if !css.is_empty() {
                attrs.push_str(" style=\"");
                escape_attr(&css, &mut attrs);
                attrs.push('"');
            }
        }
        let _ = write!(self.body, "<img{attrs}>");
    }

    /// Record an image source. Returns its manifest id, or `None` to drop it.
    fn resource(
        &mut self,
        src: &str,
        width: Option<u32>,
        height: Option<u32>,
        alt: &str,
    ) -> Option<usize> {
        let src = src.trim();
        let lower = src.to_ascii_lowercase();
        let (kind, source, host) = if lower.starts_with("cid:") {
            let value = src[4..]
                .trim()
                .trim_start_matches('<')
                .trim_end_matches('>');
            if value.is_empty() || value.len() > 1024 {
                return None;
            }
            (ResourceKind::Cid, value.to_owned(), String::new())
        } else if lower.starts_with("data:") {
            if src.len() > MAX_DATA_URI || !data_image(&lower) {
                return None;
            }
            (ResourceKind::Data, src.to_owned(), String::new())
        } else {
            let url = parse_url(src)?;
            let kind = match url.scheme() {
                "https" => ResourceKind::Remote,
                "http" => ResourceKind::Insecure,
                _ => return None,
            };
            let host = url.host_str()?.to_ascii_lowercase();
            (kind, url.to_string(), host)
        };
        if self.view.resources.len() >= MAX_RESOURCES {
            self.view.limited = true;
            return None;
        }
        let id = self.view.resources.len() + 1;
        self.view.resources.push(Resource {
            id,
            kind,
            source,
            host,
            width,
            height,
            alt: alt.chars().take(300).collect(),
        });
        Some(id)
    }

    fn link(&mut self, href: &str) -> Option<usize> {
        let url = parse_url(href)?;
        let scheme = url.scheme().to_owned();
        if !matches!(scheme.as_str(), "http" | "https" | "mailto") {
            return None;
        }
        let host = if scheme == "mailto" {
            String::new()
        } else {
            url.host_str()?.to_ascii_lowercase()
        };
        if self.view.links.len() >= MAX_LINKS {
            self.view.limited = true;
            return None;
        }
        let id = self.view.links.len() + 1;
        self.view.links.push(Link {
            id,
            href: url.to_string(),
            scheme,
            host,
        });
        Some(id)
    }

    fn attributes(&mut self, tag: &str, node: &Handle) -> String {
        let mut out = String::new();
        let NodeData::Element { attrs, .. } = &node.data else {
            return out;
        };
        let attrs = attrs.borrow();
        let mut background = None;
        let mut style = String::new();
        for a in attrs.iter() {
            let name = a.name.local.as_ref().to_ascii_lowercase();
            let value: &str = &a.value;
            match name.as_str() {
                "href" if tag == "a" => {
                    if let Some(id) = self.link(value) {
                        let _ = write!(out, " href=\"fm-link:{id}\"");
                    }
                }
                "style" => style = self.declarations(value),
                "background" if matches!(tag, "table" | "td" | "th" | "body" | "tr") => {
                    background = self.resource(value, None, None, "")
                }
                _ => {
                    if let Some(value) = self.attribute_value(tag, &name, value) {
                        let _ = write!(out, " {name}=\"{value}\"");
                    }
                }
            }
        }
        if let Some(id) = background {
            if !style.is_empty() && !style.ends_with(';') {
                style.push(';');
            }
            let _ = write!(style, "background-image:url(fm-res:{id})");
        }
        if !style.is_empty() {
            out.push_str(" style=\"");
            escape_attr(&style, &mut out);
            out.push('"');
        }
        out
    }

    /// A validated, escaped attribute value, or `None` to drop the attribute.
    fn attribute_value(&self, tag: &str, name: &str, value: &str) -> Option<String> {
        let value = value.trim();
        let ok = match name {
            "width" | "height" => {
                matches!(
                    tag,
                    "img" | "table" | "td" | "th" | "col" | "colgroup" | "hr"
                ) && length_attr(value)
            }
            "align" => matches!(
                value.to_ascii_lowercase().as_str(),
                "left" | "right" | "center" | "justify" | "top" | "middle" | "bottom"
            ),
            "valign" => matches!(
                value.to_ascii_lowercase().as_str(),
                "top" | "middle" | "bottom" | "baseline"
            ),
            "bgcolor" | "color" => color_attr(value),
            "border" | "cellpadding" | "cellspacing" => small_number(value, 100),
            "colspan" | "rowspan" | "span" => small_number(value, 1000) && value != "0",
            "start" => tag == "ol" && small_number(value, 100_000),
            "type" => matches!(tag, "ol" | "ul" | "li") && value.len() <= 8,
            "size" => tag == "font" && font_size_attr(value),
            "face" => tag == "font" && font_family_attr(value),
            "dir" => matches!(value.to_ascii_lowercase().as_str(), "ltr" | "rtl" | "auto"),
            "nowrap" => matches!(tag, "td" | "th"),
            "class" | "id" => !value.is_empty() && value.len() <= 256,
            "alt" | "title" => value.len() <= 1024,
            "lang" => {
                value.len() <= 35 && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            }
            _ => false,
        };
        if !ok {
            return None;
        }
        let value = if matches!(name, "class" | "id") {
            let tokens: Vec<String> = value
                .split_ascii_whitespace()
                .filter(|t| {
                    t.len() <= 64
                        && t.chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                })
                .map(str::to_owned)
                .collect();
            if tokens.is_empty() {
                return None;
            }
            tokens.join(" ")
        } else {
            value.to_owned()
        };
        let mut escaped = String::new();
        escape_attr(&value, &mut escaped);
        Some(escaped)
    }

    fn declarations(&mut self, css: &str) -> String {
        if css.len() > MAX_CSS_BYTES {
            self.view.limited = true;
            return String::new();
        }
        let mut input = ParserInput::new(css);
        let mut parser = Parser::new(&mut input);
        let mut out = String::new();
        let mut decls = Declarations {
            walk: self,
            out: &mut out,
        };
        for _ in RuleBodyParser::new(&mut parser, &mut decls) {}
        out
    }

    fn stylesheet(&mut self, css: &str) {
        if self.css.len() + css.len() > MAX_CSS_BYTES {
            self.view.limited = true;
            return;
        }
        let mut input = ParserInput::new(css);
        let mut parser = Parser::new(&mut input);
        let mut out = String::new();
        let mut rules = Rules {
            walk: self,
            out: &mut out,
            nested: false,
        };
        for _ in StyleSheetParser::new(&mut parser, &mut rules) {}
        self.css.push_str(&out);
    }
}

fn child_element(node: &Handle, tag: &str) -> Option<Handle> {
    node.children
        .borrow()
        .iter()
        .find(|c| matches!(&c.data, NodeData::Element { name, .. } if &*name.local == tag))
        .cloned()
}

fn attr(node: &Handle, name: &str) -> Option<String> {
    let NodeData::Element { attrs, .. } = &node.data else {
        return None;
    };
    attrs
        .borrow()
        .iter()
        .find(|a| a.name.local.as_ref().eq_ignore_ascii_case(name))
        .map(|a| a.value.to_string())
}

fn is_quote(tag: &str, node: &Handle) -> bool {
    let class = attr(node, "class").unwrap_or_default().to_ascii_lowercase();
    let classes: Vec<&str> = class.split_ascii_whitespace().collect();
    (matches!(tag, "div" | "section" | "blockquote")
        && classes
            .iter()
            .any(|c| matches!(*c, "gmail_quote" | "yahoo_quoted" | "protonmail_quote")))
        || (tag == "blockquote"
            && attr(node, "type").is_some_and(|t| t.eq_ignore_ascii_case("cite")))
        || (tag == "div"
            && attr(node, "id").is_some_and(|id| id.eq_ignore_ascii_case("divRplyFwdMsg")))
}

fn data_image(lower: &str) -> bool {
    ["png", "jpeg", "jpg", "gif", "webp"]
        .iter()
        .any(|kind| lower.starts_with(&format!("data:image/{kind};base64,")))
}

fn pixels(value: &str) -> Option<u32> {
    let value = value.trim().trim_end_matches("px");
    value.parse::<u32>().ok().map(|v| v.min(10_000))
}

fn length_attr(value: &str) -> bool {
    let number = value
        .strip_suffix('%')
        .or_else(|| value.strip_suffix("px"))
        .unwrap_or(value);
    !number.is_empty()
        && number.len() <= 6
        && number.chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn small_number(value: &str, max: u32) -> bool {
    value.parse::<u32>().is_ok_and(|v| v <= max)
}

fn color_attr(value: &str) -> bool {
    if let Some(hex) = value.strip_prefix('#') {
        return matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit());
    }
    !value.is_empty() && value.len() <= 20 && value.chars().all(|c| c.is_ascii_alphabetic())
}

fn font_size_attr(value: &str) -> bool {
    let v = value.trim_start_matches(['+', '-']);
    v.len() == 1 && v.chars().all(|c| ('1'..='7').contains(&c))
}

fn font_family_attr(value: &str) -> bool {
    value.len() <= 200
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | ',' | '-' | '_' | '\'' | '"'))
}

fn escape_text(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\u{0}' => {}
            c => out.push(c),
        }
    }
}

fn escape_attr(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\u{0}' => {}
            c => out.push(c),
        }
    }
}

// ---- CSS: every property and every value token must be on the allowlist ----

const PROPERTIES: &[&str] = &[
    "color",
    "background",
    "background-color",
    "background-image",
    "background-repeat",
    "background-position",
    "background-size",
    "border",
    "border-top",
    "border-right",
    "border-bottom",
    "border-left",
    "border-width",
    "border-style",
    "border-color",
    "border-top-width",
    "border-right-width",
    "border-bottom-width",
    "border-left-width",
    "border-top-style",
    "border-right-style",
    "border-bottom-style",
    "border-left-style",
    "border-top-color",
    "border-right-color",
    "border-bottom-color",
    "border-left-color",
    "border-radius",
    "border-top-left-radius",
    "border-top-right-radius",
    "border-bottom-left-radius",
    "border-bottom-right-radius",
    "border-collapse",
    "border-spacing",
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "width",
    "height",
    "min-width",
    "max-width",
    "min-height",
    "max-height",
    "display",
    "float",
    "clear",
    "vertical-align",
    "text-align",
    "text-decoration",
    "text-decoration-line",
    "text-decoration-color",
    "text-transform",
    "text-indent",
    "font",
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "font-variant",
    "line-height",
    "letter-spacing",
    "word-spacing",
    "white-space",
    "word-break",
    "word-wrap",
    "overflow-wrap",
    "list-style",
    "list-style-type",
    "list-style-position",
    "table-layout",
    "direction",
    "visibility",
    "opacity",
    "overflow",
    "overflow-x",
    "overflow-y",
    "box-sizing",
    "position",
    "top",
    "right",
    "bottom",
    "left",
    "flex",
    "flex-direction",
    "flex-wrap",
    "flex-grow",
    "flex-shrink",
    "flex-basis",
    "justify-content",
    "align-items",
    "align-self",
    "align-content",
    "gap",
    "row-gap",
    "column-gap",
    "caption-side",
    "empty-cells",
    "outline",
    "text-overflow",
    "unicode-bidi",
];

const UNITS: &[&str] = &[
    "px", "em", "rem", "pt", "pc", "cm", "mm", "in", "ex", "ch", "vw", "vh", "vmin", "vmax",
];

const COLOR_FUNCTIONS: &[&str] = &["rgb", "rgba", "hsl", "hsla"];

struct Declarations<'w, 'o> {
    walk: &'w mut Walk,
    out: &'o mut String,
}

impl<'i> DeclarationParser<'i> for Declarations<'_, '_> {
    type Declaration = ();
    type Error = ();

    fn parse_value<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
        _start: &ParserState,
    ) -> Result<(), ParseError<'i, ()>> {
        let property = name.to_ascii_lowercase();
        if !PROPERTIES.contains(&property.as_str()) {
            return Err(input.new_custom_error(()));
        }
        let value = value(self.walk, &property, input).map_err(|_| input.new_custom_error(()))?;
        if value.trim().is_empty() {
            return Err(input.new_custom_error(()));
        }
        if !self.out.is_empty() {
            self.out.push(';');
        }
        let _ = write!(self.out, "{property}:{}", value.trim());
        Ok(())
    }
}

impl<'i> AtRuleParser<'i> for Declarations<'_, '_> {
    type Prelude = ();
    type AtRule = ();
    type Error = ();
}

impl<'i> QualifiedRuleParser<'i> for Declarations<'_, '_> {
    type Prelude = ();
    type QualifiedRule = ();
    type Error = ();
}

impl<'i> RuleBodyItemParser<'i, (), ()> for Declarations<'_, '_> {
    fn parse_declarations(&self) -> bool {
        true
    }
    fn parse_qualified(&self) -> bool {
        false
    }
}

/// True when the rest of `input` is written with CSS escapes, which can spell
/// `url(` or keywords in ways a later parser may read differently. Leaves `input` unmoved.
fn escaped(input: &mut Parser<'_, '_>) -> bool {
    let state = input.state();
    while input.next_including_whitespace_and_comments().is_ok() {}
    let raw = input.slice_from(state.position()).contains('\\');
    input.reset(&state);
    raw
}

/// Validate one declaration value, token by token. Any unknown token rejects it.
fn value<'i>(walk: &mut Walk, property: &str, input: &mut Parser<'i, '_>) -> Result<String, ()> {
    if escaped(input) {
        return Err(());
    }
    let background = matches!(property, "background" | "background-image");
    let mut out = String::new();
    loop {
        let token = match input.next_including_whitespace() {
            Ok(token) => token.clone(),
            Err(_) => break,
        };
        match token {
            Token::WhiteSpace(_) => out.push(' '),
            Token::Comma => out.push(','),
            Token::Ident(ref ident) => {
                let lower = ident.to_ascii_lowercase();
                if lower.contains("expression") || lower.starts_with("-moz-") {
                    return Err(());
                }
                if property == "position" && !matches!(lower.as_str(), "static" | "relative") {
                    return Err(());
                }
                if property == "display"
                    && !matches!(
                        lower.as_str(),
                        "block"
                            | "inline"
                            | "inline-block"
                            | "none"
                            | "table"
                            | "table-row"
                            | "table-cell"
                            | "table-row-group"
                            | "table-header-group"
                            | "table-footer-group"
                            | "table-column"
                            | "table-caption"
                            | "list-item"
                            | "flex"
                            | "inline-flex"
                            | "inline-table"
                    )
                {
                    return Err(());
                }
                out.push_str(&token.to_css_string());
            }
            Token::Number { value, .. } => {
                let _ = write!(out, "{}", clamp(value));
            }
            Token::Percentage { unit_value, .. } => {
                let _ = write!(out, "{}%", clamp(unit_value * 100.0));
            }
            Token::Dimension {
                value, ref unit, ..
            } => {
                let unit = unit.to_ascii_lowercase();
                if !UNITS.contains(&unit.as_str()) {
                    return Err(());
                }
                let limit = if property == "font-size" || property == "font" {
                    200.0
                } else {
                    10_000.0
                };
                let _ = write!(out, "{}{unit}", clamp(value).clamp(-limit, limit));
            }
            Token::Hash(ref hex) | Token::IDHash(ref hex) => {
                if !matches!(hex.len(), 3 | 4 | 6 | 8)
                    || !hex.chars().all(|c| c.is_ascii_hexdigit())
                {
                    return Err(());
                }
                let _ = write!(out, "#{hex}");
            }
            Token::QuotedString(ref s) => {
                if !matches!(property, "font-family" | "font") || !font_family_attr(s) {
                    return Err(());
                }
                let _ = write!(out, "\"{}\"", s.replace('"', ""));
            }
            Token::Delim('/') if matches!(property, "font" | "border-radius" | "background") => {
                out.push('/')
            }
            Token::Delim('!') => match input.next() {
                Ok(Token::Ident(i)) if i.eq_ignore_ascii_case("important") => {
                    out.push_str(" !important")
                }
                _ => return Err(()),
            },
            Token::UnquotedUrl(ref url) if background => {
                let id = walk.resource(url, None, None, "").ok_or(())?;
                let _ = write!(out, "url(fm-res:{id})");
            }
            Token::Function(ref name) => {
                let name = name.to_ascii_lowercase();
                if name == "url" && background {
                    let url = input
                        .parse_nested_block(|p| {
                            let url = p.expect_string().map(|s| s.as_ref().to_owned())?;
                            p.expect_exhausted()?;
                            Ok::<_, ParseError<'i, ()>>(url)
                        })
                        .map_err(|_| ())?;
                    let id = walk.resource(&url, None, None, "").ok_or(())?;
                    let _ = write!(out, "url(fm-res:{id})");
                } else if COLOR_FUNCTIONS.contains(&name.as_str()) {
                    let args = input
                        .parse_nested_block(|p| {
                            let mut args = String::new();
                            loop {
                                match p.next() {
                                    Ok(Token::Number { value, .. }) => {
                                        let _ = write!(args, "{}", clamp(*value));
                                    }
                                    Ok(Token::Percentage { unit_value, .. }) => {
                                        let _ = write!(args, "{}%", clamp(unit_value * 100.0));
                                    }
                                    Ok(Token::Dimension { value, unit, .. })
                                        if unit.eq_ignore_ascii_case("deg") =>
                                    {
                                        let _ = write!(args, "{}deg", clamp(*value));
                                    }
                                    Ok(Token::Comma) => args.push(','),
                                    Ok(Token::Delim('/')) => args.push('/'),
                                    Ok(_) => return Err(p.new_custom_error(())),
                                    Err(_) => break,
                                }
                            }
                            Ok::<_, ParseError<'i, ()>>(args)
                        })
                        .map_err(|_| ())?;
                    let _ = write!(out, "{name}({args})");
                } else {
                    return Err(());
                }
            }
            _ => return Err(()),
        }
    }
    Ok(out)
}

fn clamp(value: f32) -> f32 {
    if value.is_finite() {
        (value.clamp(-10_000.0, 10_000.0) * 1000.0).round() / 1000.0
    } else {
        0.0
    }
}

struct Rules<'w, 'o> {
    walk: &'w mut Walk,
    out: &'o mut String,
    nested: bool,
}

impl<'i> QualifiedRuleParser<'i> for Rules<'_, '_> {
    type Prelude = String;
    type QualifiedRule = ();
    type Error = ();

    fn parse_prelude<'t>(
        &mut self,
        input: &mut Parser<'i, 't>,
    ) -> Result<String, ParseError<'i, ()>> {
        selector(input).map_err(|_| input.new_custom_error(()))
    }

    fn parse_block<'t>(
        &mut self,
        prelude: String,
        _start: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<(), ParseError<'i, ()>> {
        if self.walk.css_rules >= MAX_CSS_RULES {
            self.walk.view.limited = true;
            return Err(input.new_custom_error(()));
        }
        let mut body = String::new();
        let mut decls = Declarations {
            walk: self.walk,
            out: &mut body,
        };
        for _ in RuleBodyParser::new(input, &mut decls) {}
        if !body.is_empty() {
            self.walk.css_rules += 1;
            let _ = write!(self.out, "{prelude}{{{body}}}");
        }
        Ok(())
    }
}

impl<'i> AtRuleParser<'i> for Rules<'_, '_> {
    type Prelude = String;
    type AtRule = ();
    type Error = ();

    fn parse_prelude<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
    ) -> Result<String, ParseError<'i, ()>> {
        if self.nested || !name.eq_ignore_ascii_case("media") {
            return Err(input.new_custom_error(()));
        }
        media_query(input).map_err(|_| input.new_custom_error(()))
    }

    fn parse_block<'t>(
        &mut self,
        prelude: String,
        _start: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<(), ParseError<'i, ()>> {
        let mut body = String::new();
        let mut rules = Rules {
            walk: self.walk,
            out: &mut body,
            nested: true,
        };
        for _ in RuleBodyParser::new(input, &mut rules) {}
        if !body.is_empty() {
            let _ = write!(self.out, "@media {prelude}{{{body}}}");
        }
        Ok(())
    }
}

impl<'i> DeclarationParser<'i> for Rules<'_, '_> {
    type Declaration = ();
    type Error = ();
}

impl<'i> RuleBodyItemParser<'i, (), ()> for Rules<'_, '_> {
    fn parse_declarations(&self) -> bool {
        false
    }
    fn parse_qualified(&self) -> bool {
        true
    }
}

const PSEUDO_CLASSES: &[&str] = &[
    "first-child",
    "last-child",
    "only-child",
    "first-of-type",
    "last-of-type",
];

/// Type, class, id, universal, simple attribute selectors, combinators and a
/// few structural pseudo-classes. Anything else rejects the whole rule.
fn selector<'i>(input: &mut Parser<'i, '_>) -> Result<String, ()> {
    if escaped(input) {
        return Err(());
    }
    let mut out = String::new();
    loop {
        let token = match input.next_including_whitespace() {
            Ok(token) => token.clone(),
            Err(_) => break,
        };
        match token {
            Token::WhiteSpace(_) => out.push(' '),
            Token::Comma => out.push(','),
            Token::Ident(_) | Token::IDHash(_) => out.push_str(&token.to_css_string()),
            Token::Delim(c @ ('.' | '*' | '>' | '+' | '~')) => out.push(c),
            Token::Colon => match input.next_including_whitespace() {
                Ok(Token::Ident(p))
                    if PSEUDO_CLASSES.contains(&p.to_ascii_lowercase().as_str()) =>
                {
                    let _ = write!(out, ":{}", p.to_ascii_lowercase());
                }
                _ => return Err(()),
            },
            Token::SquareBracketBlock => {
                let inner = input
                    .parse_nested_block(|p| {
                        let name = p.expect_ident().map(|s| s.to_ascii_lowercase())?;
                        if p.is_exhausted() {
                            return Ok::<_, ParseError<'i, ()>>(name);
                        }
                        p.expect_delim('=')?;
                        let value = match p.next()? {
                            Token::Ident(v) | Token::QuotedString(v) => v.to_string(),
                            _ => return Err(p.new_custom_error(())),
                        };
                        p.expect_exhausted()?;
                        if !font_family_attr(&value) {
                            return Err(p.new_custom_error(()));
                        }
                        Ok(format!("{name}=\"{}\"", value.replace(['"', '\''], "")))
                    })
                    .map_err(|_| ())?;
                let _ = write!(out, "[{inner}]");
            }
            _ => return Err(()),
        }
    }
    let out = out.trim().to_owned();
    if out.is_empty() {
        Err(())
    } else {
        Ok(out)
    }
}

/// `screen`, `all`, `only`, `and`, `not` and width/colour-scheme features.
fn media_query<'i>(input: &mut Parser<'i, '_>) -> Result<String, ()> {
    if escaped(input) {
        return Err(());
    }
    let mut out = String::new();
    loop {
        let token = match input.next_including_whitespace() {
            Ok(token) => token.clone(),
            Err(_) => break,
        };
        match token {
            Token::WhiteSpace(_) => out.push(' '),
            Token::Comma => out.push(','),
            Token::Ident(ref i)
                if matches!(
                    i.to_ascii_lowercase().as_str(),
                    "screen" | "all" | "only" | "and" | "not"
                ) =>
            {
                out.push_str(&i.to_ascii_lowercase())
            }
            Token::ParenthesisBlock => {
                let feature = input
                    .parse_nested_block(|p| {
                        let name = p.expect_ident().map(|s| s.to_ascii_lowercase())?;
                        p.expect_colon()?;
                        let value = match p.next()?.clone() {
                            Token::Dimension { value, unit, .. }
                                if matches!(name.as_str(), "max-width" | "min-width")
                                    && UNITS.contains(&unit.to_ascii_lowercase().as_str()) =>
                            {
                                format!("{}{}", clamp(value), unit.to_ascii_lowercase())
                            }
                            Token::Ident(v)
                                if name == "prefers-color-scheme"
                                    && matches!(
                                        v.to_ascii_lowercase().as_str(),
                                        "light" | "dark"
                                    ) =>
                            {
                                v.to_ascii_lowercase()
                            }
                            _ => return Err(p.new_custom_error(())),
                        };
                        p.expect_exhausted()?;
                        Ok::<_, ParseError<'i, ()>>(format!("({name}:{value})"))
                    })
                    .map_err(|_| ())?;
                out.push_str(&feature);
            }
            _ => return Err(()),
        }
    }
    let out = out.trim().to_owned();
    if out.is_empty() {
        Err(())
    } else {
        Ok(out)
    }
}

#[cfg(test)]
mod tests;
