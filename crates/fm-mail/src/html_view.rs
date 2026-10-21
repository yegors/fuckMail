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
use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;

/// Bump when output or manifest meaning changes, so cached copies are rebuilt.
pub const SANITIZER_VERSION: u32 = 6;
/// Largest HTML input [`sanitize`] accepts, in bytes (4 MiB).
pub const MAX_HTML_BYTES: usize = 4 * 1024 * 1024;
// Bounds on sender-controlled input. Past most of them the excess is dropped
// or flattened and `HtmlView::limited` is set, rather than refusing the message.
const MAX_NODES: usize = 50_000;
/// Element nesting kept as markup: sixteen nested tables with their row
/// groups, rows and cells, with room to spare. Deeper content is kept as text.
const MAX_DEPTH: usize = 128;
const MAX_TABLE_DEPTH: usize = 16;
const MAX_CSS_RULES: usize = 2_000;
/// Selectors over all rules; litehtml keeps about 1.6 KB for each.
const MAX_CSS_SELECTORS: usize = 5_000;
const MAX_CSS_BYTES: usize = 256 * 1024;
const MAX_RESOURCES: usize = 500;
const MAX_LINKS: usize = 2_000;
const MAX_DATA_URI: usize = 2 * 1024 * 1024;
const MAX_URL: usize = 8 * 1024;
/// Largest `colspan`, `rowspan` or `span`; bigger values are clamped to it.
const MAX_SPAN: u32 = 100;
/// What litehtml's table grids may cost across a message. It pads each row
/// to the widest one and gives every spanned column its own cell, at about
/// 70 bytes each, so a few kilobytes of spans could otherwise cost
/// gigabytes. Placing each cell it also looks back through every earlier row
/// for a rowspan, about 1.2 ns a row, so a long narrow table costs rows²/2
/// (16,000 rows: 190 ms). Past either budget a table loses its spans, or then
/// its grid.
const MAX_TABLE_GRID: Grid = Grid {
    cells: 100_000,
    scans: 20_000_000,
};
/// Elements CSS may turn into a kind of table part other than their own, such
/// as a `<div>` made a row or a cell. Those grids are padded the same way.
const MAX_TABLE_DISPLAY: usize = 200;
/// Elements that may float. litehtml places each float against every earlier
/// one in its block, so 15,000 floated words took 21 s to lay out; a
/// newsletter floats a few dozen at most. Past this, `float` declarations and
/// floating `align` values are dropped.
const MAX_FLOATS: usize = 200;
/// Largest `background-size` or `background` length in pixels and in
/// font-relative units, which can reach the 200px font size.
const MAX_BACKGROUND_PX: f32 = 4_000.0;
const MAX_BACKGROUND_EM: f32 = 20.0;
/// Compound selectors in one complex selector, such as `a > b c` (three).
const MAX_SELECTOR_PARTS: usize = 12;
/// What litehtml may spend matching the sender's stylesheet, in units of one
/// compound selector check (about 7 ns). It matches every selector against
/// every element and retries descendant and sibling combinators by plain
/// backtracking, so a short rule over deep or wide markup can take minutes.
/// Selectors past the budget are dropped; ordinary mail uses a small part.
const CSS_MATCH_BUDGET: f64 = 10_000_000.0;
/// Trying a selector on an element its tag rules out (about 1 ns).
const CSS_LOOP_COST: f64 = 0.1;
/// Checking a selector's first class on an element of its tag (about 2 ns).
const CSS_CLASS_COST: f64 = 0.3;
/// Comparing one of a selector's classes with one of an element's, which
/// litehtml does by scanning the element's list (about 0.2 ns).
const CSS_LOOKUP_COST: f64 = 0.03;
/// Stepping over one sibling to find a neighbour or a position (about 1.5 ns).
const CSS_SCAN_COST: f64 = 0.25;
/// Recording and applying one matched rule, about 30 ns and 40 bytes. Set
/// higher than the time alone so the budget also bounds that memory, to 20 MB.
const CSS_MATCH_COST: f64 = 20.0;

/// Where an image in the view comes from, which decides whether and how it loads.
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
    /// Stable lowercase name used in the reader's JSON: `remote`, `insecure`,
    /// `cid` or `data`.
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceKind::Remote => "remote",
            ResourceKind::Insecure => "insecure",
            ResourceKind::Cid => "cid",
            ResourceKind::Data => "data",
        }
    }
}

/// One image the view references as `fm-res:{id}`, from an `<img>`, a legacy
/// `background` attribute or a CSS `url()`.
#[derive(Debug, Clone)]
pub struct Resource {
    /// 1-based, in document order; the `N` of `fm-res:N`.
    pub id: usize,
    pub kind: ResourceKind,
    /// Engine-only: the URL, Content-ID or data URI. Never sent to the GUI.
    pub source: String,
    /// Lowercase host for remote and insecure resources.
    pub host: String,
    /// Declared `<img>` size in pixels, capped at 10,000; `None` when absent,
    /// not a pixel count, or for backgrounds.
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// The `alt` text, cut to 300 characters.
    pub alt: String,
}

/// One anchor target the view references as `fm-link:{id}`.
#[derive(Debug, Clone)]
pub struct Link {
    /// 1-based, in document order; the `N` of `fm-link:N`.
    pub id: usize,
    /// The parsed, absolute URL as re-serialized by the `url` crate.
    pub href: String,
    /// `http`, `https` or `mailto`.
    pub scheme: String,
    /// Lowercase ASCII host; empty for mailto.
    pub host: String,
}

/// The result of [`sanitize`]: display HTML plus the manifests that its
/// `fm-res:N` and `fm-link:N` placeholders index into.
#[derive(Debug, Default)]
pub struct HtmlView {
    /// A complete `<!DOCTYPE html>` document with at most one inline stylesheet.
    pub html: String,
    pub resources: Vec<Resource>,
    pub links: Vec<Link>,
    /// Likely tracking images that were dropped and are never fetched.
    pub trackers: usize,
    /// Quoted reply history, recognized by Gmail, Yahoo, Proton or Outlook
    /// markup or `<blockquote type="cite">`, was left out.
    pub quote_removed: bool,
    /// Some content was dropped or flattened at a limit.
    pub limited: bool,
}

/// The first actual `text/html` part. mail-parser also lists a plain part in
/// `html_body` for plain-only mail, which must not count as HTML.
pub fn html_part<'a>(message: &'a mail_parser::Message<'a>) -> Option<&'a str> {
    use mail_parser::{MimeHeaders, PartType};
    let body = message
        .html_body
        .iter()
        .filter_map(|id| match &message.part(*id)?.body {
            PartType::Html(html) => Some(html.as_ref()),
            _ => None,
        });
    // Then any other part labelled text/html: a digest's parts, or one with an
    // unknown transfer encoding, are read as text outside the HTML body.
    let labelled = message
        .parts
        .iter()
        .filter(|part| part.is_content_type("text", "html"))
        .filter_map(|part| match &part.body {
            PartType::Html(html) | PartType::Text(html) => Some(html.as_ref()),
            _ => None,
        });
    let mut parts = body.chain(labelled);
    let first = parts.next()?;
    // Skip an empty leading part; some senders put the real one second.
    if !first.trim().is_empty() {
        return Some(first);
    }
    Some(parts.find(|html| !html.trim().is_empty()).unwrap_or(first))
}

/// Content-IDs (lowercase, without angle brackets) that the message's HTML
/// shows through `cid:` links. Like Gmail, those parts belong to the body and
/// are not listed as attachments.
pub fn inline_content_ids(message: &mail_parser::Message<'_>) -> BTreeSet<String> {
    use mail_parser::{MimeHeaders, PartType};
    let mut ids = BTreeSet::new();
    for part in message
        .parts
        .iter()
        .filter(|part| part.is_content_type("text", "html"))
    {
        let (PartType::Html(html) | PartType::Text(html)) = &part.body else {
            continue;
        };
        let lower = html.to_ascii_lowercase();
        let mut rest = lower.as_str();
        while let Some(at) = rest.find("cid:") {
            rest = &rest[at + 4..];
            let end = rest
                .find(|c: char| matches!(c, '"' | '\'' | ')' | '>' | '<') || c.is_whitespace())
                .unwrap_or(rest.len());
            let id = rest[..end].trim();
            if !id.is_empty() && id.len() <= 998 {
                ids.insert(id.to_owned());
            }
            rest = &rest[end..];
        }
    }
    ids
}

/// A part's Content-ID in the form `inline_content_ids` returns.
pub fn part_content_id(part: &mail_parser::MessagePart<'_>) -> Option<String> {
    use mail_parser::MimeHeaders;
    part.content_id()
        .map(|id| {
            id.trim()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_ascii_lowercase()
        })
        .filter(|id| !id.is_empty())
}

/// Rebuild `html` for display. Returns `None` when the input exceeds the size limit.
/// HTML built to make parsing slow, such as thousands of nested elements,
/// becomes its text alone, with `limited` set.
pub fn sanitize(html: &str) -> Option<HtmlView> {
    rebuild(html, false)
}

/// [`sanitize`], keeping quoted reply history in place, for a reader who asked
/// to see it. Resource and link ids are numbered over the whole document, so
/// they differ from [`sanitize`]'s when the history holds any.
pub fn sanitize_with_quotes(html: &str) -> Option<HtmlView> {
    rebuild(html, true)
}

fn rebuild(html: &str, keep_quotes: bool) -> Option<HtmlView> {
    if html.len() > MAX_HTML_BYTES {
        return None;
    }
    if !crate::html_cost::parses_cheaply(html) {
        return Some(text_view(html));
    }
    // Parse with the HTML5 algorithm, as a browser would, then rebuild from
    // the tree: nothing from the source text is copied through unparsed.
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    // A forward's quote blocks hold the forwarded message itself; keep them.
    let forwarded = html.to_ascii_lowercase().contains("forwarded message");
    let mut walk = walk_document(&dom, keep_quotes || forwarded);
    if walk.view.quote_removed && !shows_anything(&walk.body) {
        // Nothing but quoted history, as in a reply without new text: showing
        // the quote beats an empty message.
        walk = walk_document(&dom, true);
    }
    walk.settle_styles();
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

/// `html`'s text alone, line by line, without parsing it as a document.
fn text_view(html: &str) -> HtmlView {
    let mut out = String::from("<!DOCTYPE html><html><head></head><body>");
    for (index, line) in crate::html_cost::strip_tags(html).lines().enumerate() {
        if index > 0 {
            out.push_str("<br>");
        }
        escape_text(line, &mut out);
    }
    out.push_str("</body></html>");
    HtmlView {
        html: out,
        limited: true,
        ..HtmlView::default()
    }
}

fn walk_document(dom: &RcDom, keep_quotes: bool) -> Walk {
    let mut walk = Walk {
        view: HtmlView::default(),
        body: String::new(),
        css: String::new(),
        css_len: 0,
        css_rules: 0,
        css_selectors: 0,
        rules: Vec::new(),
        media: Vec::new(),
        nodes: 0,
        table_depth: 0,
        flat_tables: 0,
        table_grid: MAX_TABLE_GRID,
        spanless: Vec::new(),
        open: Open::default(),
        table_display: 0,
        floats: 0,
        stopped: false,
        keep_quotes,
        body_attrs: String::new(),
        align: BlockAlign::Start,
        extra_style: HashMap::new(),
        elements: Vec::new(),
        parent: 0,
        names: Names::default(),
    };
    walk.document(&dom.document);
    walk
}

/// Whether sanitised body markup shows any text or image.
fn shows_anything(body: &str) -> bool {
    if body.contains("<img") {
        return true;
    }
    let mut in_tag = false;
    body.chars().any(|c| match c {
        '<' => {
            in_tag = true;
            false
        }
        '>' => {
            in_tag = false;
            false
        }
        c => {
            !in_tag
                && !c.is_whitespace()
                && !matches!(
                    c,
                    '\u{200b}'..='\u{200d}' | '\u{2060}' | '\u{feff}' | '\u{34f}' | '\u{ad}'
                )
        }
    })
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

/// State of one pass over the parsed document.
struct Walk {
    view: HtmlView,
    /// Rebuilt markup for the inside of `<body>`.
    body: String,
    /// The output stylesheet, written once the body is walked.
    css: String,
    /// Length the accepted rules will take in `css`.
    css_len: usize,
    css_rules: usize,
    css_selectors: usize,
    /// Accepted rules from every `<style>` element, in document order.
    rules: Vec<StyleRule>,
    /// Preludes of the `@media` blocks that `StyleRule::media` refers to.
    media: Vec<String>,
    nodes: usize,
    table_depth: usize,
    /// Tables past the nesting limit whose structure is being unwrapped.
    flat_tables: usize,
    /// Grid budget left for further tables.
    table_grid: Grid,
    /// For each open table, whether its cells lose their spans.
    spanless: Vec<bool>,
    open: Open,
    /// Elements whose inline style makes them another kind of table part.
    table_display: usize,
    /// Elements whose inline style or `align` makes them float.
    floats: usize,
    /// Set at the node limit or an Outlook reply header; the rest is skipped.
    stopped: bool,
    keep_quotes: bool,
    /// Filtered attributes of the source `<body>`, reapplied to the output's.
    body_attrs: String,
    /// Legacy `align` inherited from ancestors, which browsers also apply to
    /// nested blocks such as tables (`text-align: -webkit-center`).
    align: BlockAlign,
    /// Declarations added to particular elements, keyed by node address.
    extra_style: HashMap<usize, String>,
    /// Every element of the output as written, in document order, to check
    /// that a parser reads the output back to the same tree.
    elements: Vec<Element>,
    /// Index in `elements` of the element being filled.
    parent: usize,
    names: Names,
}

/// Tag, class and id names, ASCII-lowercased, as small integers. Matching on
/// lowercase names finds at least every match litehtml makes.
#[derive(Default)]
struct Names(HashMap<String, u32>);

/// What `Names::find` gives a name no selector uses.
const UNNAMED: u32 = u32::MAX;

impl Names {
    fn id(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.0.get(name) {
            return id;
        }
        let next = self.0.len() as u32;
        *self.0.entry(name.to_ascii_lowercase()).or_insert(next)
    }

    /// The id of a name already seen, or `UNNAMED`.
    fn find(&self, name: &str) -> u32 {
        let lower;
        let name = if name.bytes().any(|b| b.is_ascii_uppercase()) {
            lower = name.to_ascii_lowercase();
            &lower
        } else {
            name
        };
        self.0.get(name).copied().unwrap_or(UNNAMED)
    }
}

/// An output element as litehtml's selector matching sees it.
struct Element {
    parent: Option<usize>,
    tag: u32,
    /// Sorted, as are `classes`, which keep repeats as litehtml does.
    ids: Vec<u32>,
    classes: Vec<u32>,
    /// Children as litehtml stores them: each element, and at most one per
    /// character of text, which it splits into words and spaces.
    children: usize,
    /// The most classes any child element has.
    child_classes: usize,
    /// What `own_table_part` gives for its tag.
    part: &'static str,
}

/// Add an element under `parent` to `elements`, with the id and class names
/// of `node`; returns its index. Names no selector uses stay `UNNAMED`.
fn push_element(
    elements: &mut Vec<Element>,
    names: &mut Names,
    parent: Option<usize>,
    tag: &str,
    node: Option<&Handle>,
) -> usize {
    let tokens = |name: &str| {
        let value = node.and_then(|node| attr(node, name)).unwrap_or_default();
        let mut found: Vec<u32> = value
            .split_ascii_whitespace()
            .map(|token| names.find(token))
            .collect();
        found.sort_unstable();
        found
    };
    let (ids, classes) = (tokens("id"), tokens("class"));
    if let Some(parent) = parent {
        let parent = &mut elements[parent];
        parent.children += 1;
        parent.child_classes = parent.child_classes.max(classes.len());
    }
    elements.push(Element {
        parent,
        tag: names.id(tag),
        ids,
        classes,
        children: 0,
        child_classes: 0,
        part: own_table_part(tag),
    });
    elements.len() - 1
}

/// The elements of a parsed document in document order, as litehtml's
/// selector matching sees them.
fn parsed_elements(document: &Handle, names: &mut Names) -> Vec<Element> {
    let mut elements = Vec::new();
    let mut stack = vec![(document.clone(), None)];
    while let Some((node, parent)) = stack.pop() {
        let mut inner = parent;
        match &node.data {
            NodeData::Text { contents } => {
                if let Some(parent) = parent {
                    let element: &mut Element = &mut elements[parent];
                    element.children += contents.borrow().chars().count();
                }
                continue;
            }
            NodeData::Element { name, .. } => {
                inner = Some(push_element(
                    &mut elements,
                    names,
                    parent,
                    &name.local,
                    Some(&node),
                ));
            }
            NodeData::Document => {}
            _ => continue,
        }
        for child in node.children.borrow().iter().rev() {
            stack.push((child.clone(), inner));
        }
    }
    elements
}

/// Whether two element lists form the same tree of the same tags.
fn same_tree(a: &[Element], b: &[Element]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(a, b)| a.tag == b.tag && a.parent == b.parent)
}

/// The deepest element's number of ancestors.
fn tree_depth(elements: &[Element]) -> usize {
    let mut depths = vec![0; elements.len()];
    for (at, element) in elements.iter().enumerate() {
        if let Some(parent) = element.parent {
            depths[at] = depths[parent] + 1;
        }
    }
    depths.into_iter().max().unwrap_or(0)
}

/// A style rule kept from the sender's stylesheet.
struct StyleRule {
    /// The selector list as written to the output.
    prelude: String,
    selectors: Vec<Selector>,
    body: String,
    /// The accepted declarations `body` joins.
    declarations: Vec<Declaration>,
    /// Index in `Walk::media` of the enclosing `@media` block.
    media: Option<usize>,
}

#[derive(Clone, Copy, PartialEq)]
enum BlockAlign {
    Start,
    Center,
    End,
}

/// Elements dropped together with everything inside them: scripts, embedded
/// documents and media, form controls and document metadata. Their content is
/// active, loads something, or is not message text.
const DROP_WITH_CONTENT: &[&str] = &[
    "script", "template", "iframe", "frame", "frameset", "object", "embed", "applet", "param",
    "svg", "math", "video", "audio", "source", "track", "canvas", "input", "button", "select",
    "textarea", "option", "optgroup", "datalist", "output", "meter", "progress", "link", "meta",
    "base", "title", "area", "dialog", "portal", "xml",
];

/// Elements re-emitted with filtered attributes. Any other element, apart from
/// `DROP_WITH_CONTENT`, `head` and `style`, is unwrapped: its tag goes and its
/// children stay.
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

/// Kept elements written without a closing tag.
const VOID: &[&str] = &["br", "col", "hr", "img", "wbr"];

impl Walk {
    fn document(&mut self, document: &Handle) {
        // Collect every style element first: email clients put them in head
        // and body alike, and cascade order follows document order.
        self.collect_styles(document);
        // The output's own frame: html, head, its style element and body.
        push_element(&mut self.elements, &mut self.names, None, "html", None);
        self.parent = self.add_element("head", None);
        self.add_element("style", None);
        self.parent = 0;
        let html = child_element(document, "html");
        let Some(html) = html else { return };
        if let Some(body) = child_element(&html, "body") {
            self.body_attrs = self.attributes("body", &body);
            self.parent = self.add_element("body", Some(&body));
            self.children(&body, 1);
        }
    }

    /// Record an output element under the current parent; returns its index.
    /// Its names are the source's, a superset of those emitted.
    fn add_element(&mut self, tag: &str, node: Option<&Handle>) -> usize {
        push_element(
            &mut self.elements,
            &mut self.names,
            Some(self.parent),
            tag,
            node,
        )
    }

    fn text(&mut self, text: &str) {
        escape_text(text, &mut self.body);
        self.elements[self.parent].children += text.chars().count();
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
            NodeData::Text { contents } => self.text(&contents.borrow()),
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
                // A table whose cells would pass the depth limit is flattened
                // whole, so no text is left directly in a row.
                let mut flatten = self.flat_tables > 0
                    || self.table_depth >= MAX_TABLE_DEPTH
                    || depth + 3 > MAX_DEPTH;
                let mut spanless = false;
                if table && !flatten {
                    // A grid past the budgets loses its spans, or then itself.
                    if let Some(cost) = grid(node, true, self.table_grid) {
                        self.table_grid = self.table_grid.less(cost);
                    } else if let Some(cost) = grid(node, false, self.table_grid) {
                        self.view.limited = true;
                        self.table_grid = self.table_grid.less(cost);
                        spanless = true;
                    } else {
                        flatten = true;
                    }
                }
                if table && flatten {
                    // Past a limit: keep the content, drop the grid.
                    self.view.limited = true;
                    self.flat_tables += 1;
                    self.children(node, depth + 1);
                    self.flat_tables -= 1;
                    return;
                }
                // A table part with no emitted table around it (its table was
                // flattened) is unwrapped too, so no orphan row or cell reaches
                // the renderer. A `<br>` keeps flattened rows on separate lines.
                // So is an element a parser would read as ending an open one,
                // such as an `li` in an `li`, which the source parse kept apart
                // only through a `marquee` or cell this walk removed.
                if !KEEP.contains(&tag.as_str())
                    || (table_part && (self.flat_tables > 0 || self.table_depth == 0))
                    || self.open.ends(&tag)
                {
                    self.children(node, depth + 1);
                    if tag == "tr" && self.flat_tables > 0 {
                        self.body.push_str("<br>");
                        self.add_element("br", None);
                    }
                    return;
                }
                // A paragraph holding a block would end at the block when the
                // output is parsed again, so it is written as a `div`.
                let tag = if tag == "p" && holds_block(node) {
                    "div".to_owned()
                } else {
                    tag
                };
                if matches!(tag.as_str(), "table" | "div") {
                    self.align_block(node);
                }
                if table {
                    self.spanless.push(spanless);
                    self.fixed_layout(node);
                }
                let attrs = self.attributes(&tag, node);
                let _ = write!(self.body, "<{tag}{attrs}>");
                let element = self.add_element(&tag, Some(node));
                if VOID.contains(&tag.as_str()) {
                    return;
                }
                if table {
                    self.table_depth += 1;
                }
                let outer = self.align;
                let outer_parent = std::mem::replace(&mut self.parent, element);
                let outer_open = self.open;
                self.open = outer_open.inside(&tag);
                self.align = inherited_align(&tag, node, outer);
                self.children(node, depth + 1);
                self.align = outer;
                self.parent = outer_parent;
                self.open = outer_open;
                if table {
                    self.table_depth -= 1;
                    self.spanless.pop();
                }
                let _ = write!(self.body, "</{tag}>");
            }
            _ => {}
        }
    }

    /// A table or block inside a legacy `align="center"` or `"right"` ancestor
    /// gets the auto margins browsers give it, unless it places itself.
    fn align_block(&mut self, node: &Handle) {
        let margins = match self.align {
            BlockAlign::Start => return,
            BlockAlign::Center => "margin-left:auto;margin-right:auto",
            BlockAlign::End => "margin-left:auto",
        };
        let style = attr(node, "style").unwrap_or_default();
        if attr(node, "align").is_some()
            || has_declaration(&style, &["margin", "margin-left", "margin-right"])
        {
            return;
        }
        self.add_style(node, margins);
    }

    /// Browsers give the columns of a `table-layout: fixed` table with a width
    /// equal shares when its first row declares none; litehtml sizes them by
    /// content, so the first row's cells get those shares as percentages.
    fn fixed_layout(&mut self, node: &Handle) {
        let style = attr(node, "style").unwrap_or_default().to_ascii_lowercase();
        let fixed = declaration(&style, "table-layout").is_some_and(|v| v == "fixed");
        let sized = attr(node, "width").is_some()
            || declaration(&style, "width").is_some_and(|v| v != "auto");
        if !fixed || !sized {
            return;
        }
        let Some(row) = first_row(node) else { return };
        let cells: Vec<Handle> = row
            .children
            .borrow()
            .iter()
            .filter(|c| matches!(&c.data, NodeData::Element { name, .. } if matches!(&*name.local, "td" | "th")))
            .cloned()
            .collect();
        let spanless = self.spanless.last().copied().unwrap_or(false);
        let span = |cell: &Handle| {
            attr(cell, "colspan")
                .and_then(|v| span_value(&v))
                .filter(|_| !spanless)
                .unwrap_or(1)
        };
        let columns: u32 = cells.iter().map(span).sum();
        let widthless = cells.iter().all(|cell| {
            let style = attr(cell, "style").unwrap_or_default().to_ascii_lowercase();
            attr(cell, "width").is_none()
                && declaration(&style, "width").is_none_or(|v| v == "auto")
        });
        if cells.is_empty() || columns > 100 || !widthless {
            return;
        }
        for cell in &cells {
            let share = f64::from(span(cell)) * 100.0 / f64::from(columns);
            self.add_style(
                cell,
                &format!("width:{}%", (share * 1000.0).round() / 1000.0),
            );
        }
    }

    fn add_style(&mut self, node: &Handle, css: &str) {
        let entry = self
            .extra_style
            .entry(std::rc::Rc::as_ptr(node) as usize)
            .or_default();
        if !entry.is_empty() {
            entry.push(';');
        }
        entry.push_str(css);
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
                NodeData::Text { contents } => self.text(&contents.borrow()),
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
        // Hidden or pixel-sized images are counted as trackers and get no
        // manifest entry, so nothing can ever fetch them.
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
                    if self.align_allowed("img", name, &value) {
                        let _ = write!(attrs, " {name}=\"{value}\"");
                    }
                }
            }
        }
        if let Some(css) = attr(node, "style") {
            let css = self.declarations("img", &css);
            if !css.is_empty() {
                attrs.push_str(" style=\"");
                escape_attr(&css, &mut attrs);
                attrs.push('"');
            }
        }
        let _ = write!(self.body, "<img{attrs}>");
        self.add_element("img", None);
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
        // Only three sources survive: a part of this message by Content-ID, a
        // base64 raster `data:` image (never SVG, which can carry script), and
        // an absolute http(s) URL. Relative, `file:` and other URLs are dropped.
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

    /// Record an anchor target. Returns its manifest id, or `None` to drop the
    /// `href`: only absolute http(s) and mailto URLs are kept. The anchor's
    /// text stays either way.
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

    /// The filtered attribute string for a kept element, with a leading space.
    /// `href` becomes `fm-link:N`, a legacy `background` becomes a CSS
    /// `background-image` on the manifest, and inline `style` is re-validated.
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
                "style" => style = self.declarations(tag, value),
                "background" if matches!(tag, "table" | "td" | "th" | "body" | "tr") => {
                    background = self.resource(value, None, None, "")
                }
                _ => {
                    if let Some(value) = self.attribute_value(tag, &name, value) {
                        if self.align_allowed(tag, &name, &value) {
                            let _ = write!(out, " {name}=\"{value}\"");
                        }
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
        if let Some(extra) = self
            .extra_style
            .remove(&(std::rc::Rc::as_ptr(node) as usize))
        {
            if !style.is_empty() && !style.ends_with(';') {
                style.push(';');
            }
            style.push_str(&extra);
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
        // litehtml reads spans from any element shown as a cell, so they stay
        // on the elements they belong to, and off a table over its budget.
        if matches!(name, "colspan" | "rowspan" | "span") {
            let ok = match name {
                "span" => matches!(tag, "col" | "colgroup"),
                _ => matches!(tag, "td" | "th") && self.spanless.last() != Some(&true),
            };
            return span_value(value).filter(|_| ok).map(|v| v.to_string());
        }
        // Each attribute has its own value grammar. Anything unlisted is
        // dropped, which covers event handlers and every URL-bearing attribute.
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
        // Classes and ids keep their simple ASCII tokens so the sender's
        // stylesheet can still match them.
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

    /// Re-serialize an inline `style` value of a `tag` element, keeping only
    /// allowed declarations. A `display` making it another kind of table
    /// part counts toward `MAX_TABLE_DISPLAY`, past which it is dropped.
    fn declarations(&mut self, tag: &str, css: &str) -> String {
        if css.len() > MAX_CSS_BYTES {
            self.view.limited = true;
            return String::new();
        }
        let mut input = ParserInput::new(css);
        let mut parser = Parser::new(&mut input);
        let mut out = Vec::new();
        let mut decls = Declarations {
            walk: self,
            out: &mut out,
        };
        for _ in RuleBodyParser::new(&mut parser, &mut decls) {}
        let foreign = |declaration: &Declaration| {
            declaration
                .display
                .as_deref()
                .is_some_and(|display| display != own_table_part(tag))
        };
        let mut keep = true;
        if out.iter().any(foreign) {
            keep = self.table_display < MAX_TABLE_DISPLAY;
            if keep {
                self.table_display += 1;
            } else {
                self.view.limited = true;
            }
        }
        let keep_float = !out.iter().any(|d| d.float) || self.may_float();
        join_declarations(&out, |d| (keep || !foreign(d)) && (keep_float || !d.float))
    }

    /// Count one more floating element. False, marking the view limited, once
    /// `MAX_FLOATS` elements already float.
    fn may_float(&mut self) -> bool {
        if self.floats < MAX_FLOATS {
            self.floats += 1;
            true
        } else {
            self.view.limited = true;
            false
        }
    }

    /// Whether a kept `align` attribute may stay: litehtml floats left- and
    /// right-aligned images and tables, which count toward `MAX_FLOATS`.
    fn align_allowed(&mut self, tag: &str, name: &str, value: &str) -> bool {
        let floats = name == "align"
            && matches!(tag, "img" | "table")
            && matches!(value.to_ascii_lowercase().as_str(), "left" | "right");
        !floats || self.may_float()
    }

    /// Collect a `<style>` element's accepted rules for the output stylesheet.
    /// A rule with an unsupported selector, or no allowed declaration, is dropped.
    fn stylesheet(&mut self, css: &str) {
        if self.css_len + css.len() > MAX_CSS_BYTES {
            self.view.limited = true;
            return;
        }
        let mut input = ParserInput::new(css);
        let mut parser = Parser::new(&mut input);
        let mut rules = Rules {
            walk: self,
            media: None,
        };
        for _ in StyleSheetParser::new(&mut parser, &mut rules) {}
    }

    /// Write the output stylesheet now that the output's elements are known.
    /// Selectors that would take litehtml past `CSS_MATCH_BUDGET` are dropped,
    /// in document order, as are `display` values making table parts once
    /// more than `MAX_TABLE_DISPLAY` elements could get one.
    fn settle_styles(&mut self) {
        // Costs are estimated on the tree a parser builds from the output,
        // which is what litehtml matches against. The walk writes markup that
        // parses back to the tree it recorded; should a parser still rebuild
        // some construct, the estimate follows the parser, and if that adds
        // elements, depth or grid, only the text is kept.
        let document = format!(
            "<!DOCTYPE html><html><head><style></style></head><body{}>{}</body></html>",
            self.body_attrs, self.body
        );
        let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(document);
        let parsed = parsed_elements(&dom.document, &mut self.names);
        if !same_tree(&parsed, &self.elements) {
            self.view.limited = true;
            if parsed.len() > self.elements.len()
                || tree_depth(&parsed) > tree_depth(&self.elements)
                || !grids_fit(&dom.document)
            {
                self.body = text_only(&dom.document);
                self.rules.clear();
            }
        }
        let elements = &parsed;
        let index = ElementIndex::new(elements, &self.names);
        // When a media query changes with the width, litehtml matches every
        // matched rule again, on each such layout.
        let rematch = !self.media.is_empty();
        let mut budget = CSS_MATCH_BUDGET;
        // Estimating a selector is work too, whether or not it is kept: each
        // one is charged its fixed costs, the elements it looks at and what it
        // was found to cost. Past twice the budget in all, the remaining
        // selectors are dropped unestimated.
        let mut effort = 2.0 * CSS_MATCH_BUDGET;
        let mut trimmed = false;
        for rule in &mut self.rules {
            let before = rule.selectors.len();
            rule.selectors.retain(|selector| {
                if effort <= 0.0 {
                    return false;
                }
                let (checks, pool) = index.tried(selector);
                let fixed = elements.len() as f64 * CSS_LOOP_COST + checks;
                let limit = budget - fixed;
                let found = matching_cost(selector, elements, pool, rematch, limit);
                effort -= fixed + pool.len() as f64 + found.unwrap_or(limit).max(0.0);
                let Some(cost) = found else { return false };
                budget = limit - cost;
                true
            });
            if rule.selectors.len() != before {
                trimmed = true;
                let texts: Vec<&str> = rule.selectors.iter().map(|s| s.text.as_str()).collect();
                rule.prelude = texts.join(",");
            }
        }
        // Elements a table-part `display` could make another kind of part, and
        // elements a `float` could float, with the rules that could do either.
        let (foreign, strip) = reach(
            &self.rules,
            elements,
            &index,
            MAX_TABLE_DISPLAY.saturating_sub(self.table_display),
            |declaration, element| {
                declaration
                    .display
                    .as_ref()
                    .is_some_and(|display| display != element.part)
            },
        );
        let (floating, strip_floats) = reach(
            &self.rules,
            elements,
            &index,
            MAX_FLOATS.saturating_sub(self.floats),
            |declaration, _| declaration.float,
        );
        self.view.limited |= trimmed || strip || strip_floats;
        let mut css = String::with_capacity(self.css_len);
        let mut open: Option<usize> = None;
        for ((rule, &foreign), &floating) in self.rules.iter().zip(&foreign).zip(&floating) {
            let (strip, strip_floats) = (strip && foreign, strip_floats && floating);
            let stripped;
            let body = if strip || strip_floats {
                stripped = join_declarations(&rule.declarations, |d| {
                    !(strip && d.display.is_some() || strip_floats && d.float)
                });
                &stripped
            } else {
                &rule.body
            };
            if rule.selectors.is_empty() || body.is_empty() {
                continue;
            }
            if open != rule.media {
                if open.is_some() {
                    css.push('}');
                }
                if let Some(media) = rule.media {
                    let _ = write!(css, "@media {}{{", self.media[media]);
                }
                open = rule.media;
            }
            let _ = write!(css, "{}{{{body}}}", rule.prelude);
        }
        if open.is_some() {
            css.push('}');
        }
        self.css = css;
    }
}

/// For each rule, whether one of its declarations could apply `affects` to
/// an element, assuming each selector matches wherever its rightmost compound
/// does; and whether more than `limit` elements could be affected.
fn reach(
    rules: &[StyleRule],
    elements: &[Element],
    index: &ElementIndex,
    limit: usize,
    affects: impl Fn(&Declaration, &Element) -> bool,
) -> (Vec<bool>, bool) {
    let mut reached = std::collections::HashSet::new();
    let mut flagged = vec![false; rules.len()];
    for (rule, flagged) in rules.iter().zip(&mut flagged) {
        if !rule
            .declarations
            .iter()
            .any(|d| d.display.is_some() || d.float)
        {
            continue;
        }
        for selector in &rule.selectors {
            let right = selector.rightmost();
            for &at in index.tried(selector).1 {
                let element = &elements[at];
                if right.may_match(element) && rule.declarations.iter().any(|d| affects(d, element))
                {
                    *flagged = true;
                    if reached.len() <= limit {
                        reached.insert(at);
                    }
                }
            }
        }
    }
    (flagged, reached.len() > limit)
}

/// Output elements by tag and by class name.
struct ElementIndex {
    by_tag: Vec<Vec<usize>>,
    by_class: Vec<Vec<usize>>,
    /// Classes over the elements of each tag.
    tag_classes: Vec<usize>,
    all: Vec<usize>,
    all_classes: usize,
}

impl ElementIndex {
    fn new(elements: &[Element], names: &Names) -> Self {
        let mut index = ElementIndex {
            by_tag: vec![Vec::new(); names.0.len()],
            by_class: vec![Vec::new(); names.0.len()],
            tag_classes: vec![0; names.0.len()],
            all: (0..elements.len()).collect(),
            all_classes: 0,
        };
        for (at, element) in elements.iter().enumerate() {
            index.by_tag[element.tag as usize].push(at);
            index.tag_classes[element.tag as usize] += element.classes.len();
            index.all_classes += element.classes.len();
            for &class in element.classes.iter().filter(|&&class| class != UNNAMED) {
                index.by_class[class as usize].push(at);
            }
        }
        index
    }

    /// Elements to look at for `selector`, with the cost of litehtml's quick
    /// check of its rightmost compound's first class on each element of that
    /// compound's tag. Those that pass `Compound::tried` are the ones
    /// litehtml then matches against; the list is never longer than what the
    /// check costs, so walking it is paid for.
    fn tried(&self, selector: &Selector) -> (f64, &[usize]) {
        let right = selector.rightmost();
        let (tagged, classes) = match right.tag {
            Some(tag) => (&self.by_tag[tag as usize], self.tag_classes[tag as usize]),
            None => (&self.all, self.all_classes),
        };
        let Some(class) = right.first_class else {
            return (0.0, tagged);
        };
        let classed = &self.by_class[class as usize];
        let pool = if classed.len() < tagged.len() {
            classed
        } else {
            tagged
        };
        let cost = tagged.len() as f64 * CSS_CLASS_COST + classes as f64 * CSS_LOOKUP_COST;
        (cost, pool)
    }
}

/// Upper bound of what litehtml spends matching `selector` against the
/// elements of `pool` that it tries it on, or `None` past `limit`. With
/// `rematch`, elements that may match are counted twice.
fn matching_cost(
    selector: &Selector,
    elements: &[Element],
    pool: &[usize],
    rematch: bool,
    limit: f64,
) -> Option<f64> {
    if limit < 0.0 {
        return None;
    }
    let right = selector.rightmost();
    let last = selector.parts.len() - 1;
    // Pseudo-classes are ignored on a first pass and checked on a second.
    let pass = |scans| Matching {
        selector,
        elements,
        scans,
        tries: HashMap::default(),
        chains: HashMap::default(),
    };
    let mut first = pass(false);
    let mut second = selector.pseudo.then(|| pass(true));
    let times = if rematch { 2.0 } else { 1.0 };
    let mut cost = 0.0;
    for &index in pool {
        let element = &elements[index];
        if !right.tried(element) {
            continue;
        }
        // Each element is tried once, so the rightmost part is not memoised.
        let check = right.check(element.classes.len());
        cost += if right.may_match(element) {
            let pseudo = second
                .as_mut()
                .map_or(0.0, |second| second.matched(last, index, check));
            CSS_MATCH_COST + times * (first.matched(last, index, check) + pseudo)
        } else {
            check
        };
        if cost > limit {
            return None;
        }
    }
    Some(cost)
}

/// Memoised costs by selector part and element index.
type Memo = HashMap<(usize, usize), f64, std::hash::BuildHasherDefault<IndexHasher>>;

/// A multiply-rotate hash for keys of small integers, which the default
/// hasher spends most of the estimate on.
#[derive(Default)]
struct IndexHasher(u64);

impl std::hash::Hasher for IndexHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write_u64(u64::from(byte));
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x517c_c1b7_2722_0a95);
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }
}

/// litehtml matches a complex selector right to left by backtracking without
/// memoisation: a descendant combinator retries the rest of the selector on
/// every ancestor until it succeeds, a general sibling one on every earlier
/// sibling, so the work multiplies with nesting. This counts those tries as if
/// each one failed. Siblings off the element's own ancestor chain are not
/// recorded, so they are assumed to match.
struct Matching<'a> {
    selector: &'a Selector,
    elements: &'a [Element],
    /// Counting the pass that checks pseudo-classes.
    scans: bool,
    tries: Memo,
    chains: Memo,
}

impl Matching<'_> {
    /// Checks made matching the selector's parts up to `part` on `element`.
    fn tries(&mut self, part: usize, element: usize) -> f64 {
        if let Some(&cost) = self.tries.get(&(part, element)) {
            return cost;
        }
        let (selector, elements) = (self.selector, self.elements);
        let compound = &selector.parts[part];
        let node = &elements[element];
        // A tag mismatch fails at once; otherwise each class is looked up.
        let cost = if compound.tag.is_some_and(|tag| tag != node.tag) {
            1.0
        } else if compound.may_match(node) {
            let check = compound.check(node.classes.len());
            self.matched(part, element, check)
        } else {
            compound.check(node.classes.len())
        };
        self.tries.insert((part, element), cost);
        cost
    }

    /// The same for an element known or assumed to match `part`, `element`
    /// itself or an unrecorded sibling of it, at `check` for the compound.
    fn matched(&mut self, part: usize, element: usize, check: f64) -> f64 {
        check + self.position(part, element) + self.next(part, element)
    }

    /// `matched` for an unrecorded sibling of `element`.
    fn sibling(&mut self, part: usize, element: usize) -> f64 {
        let classes = self.elements[element]
            .parent
            .map_or(0, |parent| self.elements[parent].child_classes);
        let check = self.selector.parts[part].check(classes);
        self.matched(part, element, check)
    }

    /// Checks made once `part` matched `element` or one of its siblings.
    fn next(&mut self, part: usize, element: usize) -> f64 {
        let Some(parent) = self.elements[element].parent else {
            return 0.0;
        };
        if part == 0 {
            return 0.0;
        }
        let siblings = self.siblings(element);
        match self.selector.combinators[part - 1] {
            Combinator::Descendant => self.chain(part - 1, parent),
            Combinator::Child => self.tries(part - 1, parent),
            Combinator::Adjacent => siblings * CSS_SCAN_COST + self.sibling(part - 1, element),
            Combinator::Sibling => siblings * (CSS_SCAN_COST + self.sibling(part - 1, element)),
        }
    }

    /// Tries of `part` on `element` and on each of its ancestors.
    fn chain(&mut self, part: usize, element: usize) -> f64 {
        let mut pending = Vec::new();
        let mut node = Some(element);
        let mut total = 0.0;
        while let Some(index) = node {
            if let Some(&sum) = self.chains.get(&(part, index)) {
                total = sum;
                break;
            }
            pending.push(index);
            node = self.elements[index].parent;
        }
        for &index in pending.iter().rev() {
            total += self.tries(part, index);
            self.chains.insert((part, index), total);
        }
        total
    }

    /// Siblings stepped over to check a structural pseudo-class, which
    /// litehtml does on the second pass once the rest of the compound matched.
    fn position(&self, part: usize, element: usize) -> f64 {
        if self.scans && self.selector.parts[part].pseudo {
            self.siblings(element) * CSS_SCAN_COST
        } else {
            0.0
        }
    }

    fn siblings(&self, element: usize) -> f64 {
        self.elements[element]
            .parent
            .map_or(0.0, |parent| self.elements[parent].children as f64)
    }
}

/// What a table grid costs litehtml, or what is left of the budget for one.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Grid {
    /// Cells allocated.
    cells: u64,
    /// Earlier rows looked through while placing cells.
    scans: u64,
}

impl Grid {
    fn less(self, cost: Grid) -> Grid {
        Grid {
            cells: self.cells - cost.cells,
            scans: self.scans - cost.scans,
        }
    }
}

/// What litehtml's grid for `table` costs, or `None` past `limit`. Every row
/// is padded to the widest; a cell's colspan adds a cell per column, and its
/// rowspan pushes later rows' cells right, from its first column only, as
/// litehtml places them. Each placement looks through the earlier rows.
/// Without `spans`, every span counts as 1.
fn grid(table: &Handle, spans: bool, limit: Grid) -> Option<Grid> {
    let mut rows = Vec::new();
    for child in table.children.borrow().iter() {
        match element_name(child) {
            Some("tr") => rows.push(child.clone()),
            Some("thead" | "tbody" | "tfoot") => rows.extend(
                child
                    .children
                    .borrow()
                    .iter()
                    .filter(|row| element_name(row) == Some("tr"))
                    .cloned(),
            ),
            _ => {}
        }
    }
    let count = rows.len() as u64;
    let span = |cell: &Handle, name: &str| {
        attr(cell, name)
            .and_then(|v| span_value(&v))
            .filter(|_| spans)
            .unwrap_or(1) as u64
    };
    // For each column, the last row that a rowspan from above reaches.
    let mut reach: Vec<u64> = Vec::new();
    let mut width = 0;
    let mut scans = 0u64;
    for (row, cells) in (0u64..).zip(&rows) {
        let mut column = 0;
        for cell in cells.children.borrow().iter() {
            if !matches!(element_name(cell), Some("td" | "th")) {
                continue;
            }
            let start = column;
            while reach.get(column as usize).is_some_and(|&last| last >= row) {
                column += 1;
            }
            scans = scans.saturating_add((1 + column - start) * row);
            let rowspan = span(cell, "rowspan");
            if rowspan > 1 {
                let at = column as usize;
                if reach.len() <= at {
                    reach.resize(at + 1, 0);
                }
                reach[at] = reach[at].max(row + rowspan - 1);
            }
            column += span(cell, "colspan");
            width = width.max(column);
            if width.saturating_mul(count) > limit.cells || scans > limit.scans {
                return None;
            }
        }
    }
    Some(Grid {
        cells: width * count,
        scans,
    })
}

/// Whether every table in a parsed document fits `MAX_TABLE_GRID` together.
fn grids_fit(document: &Handle) -> bool {
    let mut left = MAX_TABLE_GRID;
    let mut stack = vec![document.clone()];
    while let Some(node) = stack.pop() {
        if element_name(&node) == Some("table") {
            match grid(&node, true, left) {
                Some(cost) => left = left.less(cost),
                None => return false,
            }
        }
        stack.extend(node.children.borrow().iter().cloned());
    }
    true
}

/// The escaped text of a parsed document's body, in document order.
fn text_only(document: &Handle) -> String {
    let mut out = String::new();
    let mut stack = vec![document.clone()];
    while let Some(node) = stack.pop() {
        match &node.data {
            NodeData::Text { contents } => escape_text(&contents.borrow(), &mut out),
            NodeData::Element { name, .. } if &*name.local == "head" => continue,
            _ => {}
        }
        stack.extend(node.children.borrow().iter().rev().cloned());
    }
    out
}

/// Tags that end an open paragraph when a parser reads them.
const BLOCKS: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "center",
    "dd",
    "details",
    "div",
    "dl",
    "dt",
    "figcaption",
    "figure",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "li",
    "main",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "summary",
    "table",
    "ul",
];

/// Kept elements a parser treats as special when it looks for an open `li`,
/// `dd` or `dt` to end: the search stops at them.
const SPECIAL: &[&str] = &[
    "article",
    "aside",
    "blockquote",
    "caption",
    "center",
    "colgroup",
    "details",
    "dl",
    "figcaption",
    "figure",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "main",
    "nav",
    "ol",
    "pre",
    "section",
    "summary",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    "ul",
];

const HEADINGS: &[&str] = &["h1", "h2", "h3", "h4", "h5", "h6"];

/// Whether paragraph `p` holds, in the source, an element that ends an open
/// paragraph when parsed. Its search stops at such elements, so each node is
/// looked at for one paragraph at most.
fn holds_block(p: &Handle) -> bool {
    let mut stack: Vec<Handle> = p.children.borrow().iter().cloned().collect();
    while let Some(node) = stack.pop() {
        let Some(tag) = element_name(&node) else {
            continue;
        };
        if BLOCKS.contains(&tag) {
            return true;
        }
        if tag != "style" && !DROP_WITH_CONTENT.contains(&tag) {
            stack.extend(node.children.borrow().iter().cloned());
        }
    }
    false
}

/// Emitted ancestors a parser would end on reading certain start tags. The
/// source parse keeps them open only across an element this walk removes,
/// such as a `marquee` or the cell of a flattened table.
#[derive(Clone, Copy, Default)]
struct Open {
    /// An `li` reachable without passing a special element but `div`,
    /// `address` or `p`; `dl` the same for `dd` or `dt`.
    li: bool,
    dl: bool,
    /// An `a` with no cell or caption since.
    a: bool,
    /// The parent is a heading.
    heading: bool,
}

impl Open {
    /// Whether a parser reading `tag` here would end an open element.
    fn ends(&self, tag: &str) -> bool {
        match tag {
            "li" => self.li,
            "dd" | "dt" => self.dl,
            "a" => self.a,
            _ => self.heading && HEADINGS.contains(&tag),
        }
    }

    /// The state inside an emitted `tag` element.
    fn inside(self, tag: &str) -> Open {
        let mut open = Open {
            heading: HEADINGS.contains(&tag),
            ..self
        };
        if tag == "li" {
            (open.li, open.dl) = (true, false);
        } else if matches!(tag, "dd" | "dt") {
            (open.li, open.dl) = (false, true);
        } else if SPECIAL.contains(&tag) {
            (open.li, open.dl) = (false, false);
        }
        if tag == "a" {
            open.a = true;
        } else if matches!(tag, "td" | "th" | "caption") {
            open.a = false;
        }
        open
    }
}

fn element_name(node: &Handle) -> Option<&str> {
    match &node.data {
        NodeData::Element { name, .. } => Some(&name.local),
        _ => None,
    }
}

/// A `colspan`, `rowspan` or `span` value as emitted: clamped to `MAX_SPAN`;
/// zero or anything but a number drops the attribute.
fn span_value(value: &str) -> Option<u32> {
    value
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|&v| v > 0)
        .map(|v| v.min(MAX_SPAN))
}

/// The `display` values that make an element a table part.
const TABLE_PARTS: &[&str] = &[
    "table-row",
    "table-cell",
    "table-row-group",
    "table-header-group",
    "table-footer-group",
    "table-column",
    "table-caption",
];

/// The kind of table part `tag` is by itself, as its `display` value, or "".
fn own_table_part(tag: &str) -> &'static str {
    match tag {
        "tr" => "table-row",
        "td" | "th" => "table-cell",
        "tbody" => "table-row-group",
        "thead" => "table-header-group",
        "tfoot" => "table-footer-group",
        "col" => "table-column",
        "caption" => "table-caption",
        _ => "",
    }
}

/// One accepted `property:value` declaration, as written to the output.
struct Declaration {
    text: String,
    /// The `display` value, when it makes a table part.
    display: Option<String>,
    /// Whether it is a `float` other than `none`.
    float: bool,
}

/// Accepted declarations joined for output, keeping those that pass `keep`.
fn join_declarations(declarations: &[Declaration], keep: impl Fn(&Declaration) -> bool) -> String {
    let kept: Vec<&str> = declarations
        .iter()
        .filter(|d| keep(d))
        .map(|d| d.text.as_str())
        .collect();
    kept.join(";")
}

/// The alignment an element passes to its descendants: its own legacy `align`,
/// reset by an explicit `text-align`, which overrides it in browsers.
fn inherited_align(tag: &str, node: &Handle, outer: BlockAlign) -> BlockAlign {
    let style = attr(node, "style").unwrap_or_default();
    if has_declaration(&style, &["text-align"]) {
        return BlockAlign::Start;
    }
    if tag == "center" {
        return BlockAlign::Center;
    }
    if !matches!(
        tag,
        "div"
            | "p"
            | "td"
            | "th"
            | "tr"
            | "tbody"
            | "thead"
            | "tfoot"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
    ) {
        return outer;
    }
    match attr(node, "align")
        .map(|v| v.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("center" | "middle") => BlockAlign::Center,
        Some("right") => BlockAlign::End,
        Some("left" | "justify") => BlockAlign::Start,
        _ => outer,
    }
}

/// The last value of one property in an inline style, lowercase and trimmed.
fn declaration(style: &str, property: &str) -> Option<String> {
    style
        .split(';')
        .filter_map(|d| d.split_once(':'))
        .filter(|(name, _)| name.trim().eq_ignore_ascii_case(property))
        .map(|(_, value)| {
            value
                .trim()
                .trim_end_matches("!important")
                .trim()
                .to_ascii_lowercase()
        })
        .next_back()
}

fn has_declaration(style: &str, properties: &[&str]) -> bool {
    properties.iter().any(|p| declaration(style, p).is_some())
}

/// A table's first row, looking through its row groups in document order.
fn first_row(table: &Handle) -> Option<Handle> {
    for child in table.children.borrow().iter() {
        let NodeData::Element { name, .. } = &child.data else {
            continue;
        };
        match &*name.local {
            "tr" => return Some(child.clone()),
            "thead" | "tbody" | "tfoot" => {
                if let Some(row) = child_element(child, "tr") {
                    return Some(row);
                }
            }
            _ => {}
        }
    }
    None
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
//
// A declaration that fails any check is dropped whole; its siblings survive.
// Output is rebuilt from tokens, never copied from the source text.

/// Layout and typography properties. Nothing here can run script, and the only
/// way to load anything is `url()` in a background, routed through the manifest.
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

/// Declaration-list parser that appends accepted declarations to `out`.
struct Declarations<'w, 'o> {
    walk: &'w mut Walk,
    out: &'o mut Vec<Declaration>,
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
        let value = value.trim();
        if value.is_empty() {
            return Err(input.new_custom_error(()));
        }
        let keyword = value
            .split(' ')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let float = property == "float" && keyword != "none";
        let display =
            (property == "display" && TABLE_PARTS.contains(&keyword.as_str())).then_some(keyword);
        self.out.push(Declaration {
            text: format!("{property}:{value}"),
            display,
            float,
        });
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
                // Legacy script hooks (`expression`, `-moz-binding`) never pass.
                let lower = ident.to_ascii_lowercase();
                if lower.contains("expression") || lower.starts_with("-moz-") {
                    return Err(());
                }
                // No absolute, fixed or sticky positioning, which would take
                // content out of the message's normal flow.
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
                // Clamp lengths so hostile values cannot blow up layout work,
                // or the size a background image is painted at.
                let limit = match property {
                    "font-size" | "font" => 200.0,
                    "background" | "background-size" => {
                        if matches!(unit.as_str(), "em" | "rem" | "ex" | "ch") {
                            MAX_BACKGROUND_EM
                        } else {
                            MAX_BACKGROUND_PX
                        }
                    }
                    _ => 10_000.0,
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
            // Background images join the manifest like `<img>`; any other
            // `url()` rejects the declaration.
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

/// A finite number limited to ±10,000 and three decimals; NaN and infinities become 0.
fn clamp(value: f32) -> f32 {
    if value.is_finite() {
        (value.clamp(-10_000.0, 10_000.0) * 1000.0).round() / 1000.0
    } else {
        0.0
    }
}

/// Stylesheet parser that collects accepted rules in `Walk::rules`. `media`
/// is set inside `@media`, where another at-rule is refused.
struct Rules<'w> {
    walk: &'w mut Walk,
    media: Option<usize>,
}

impl<'i> QualifiedRuleParser<'i> for Rules<'_> {
    type Prelude = (String, Vec<Selector>);
    type QualifiedRule = ();
    type Error = ();

    fn parse_prelude<'t>(
        &mut self,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, ParseError<'i, ()>> {
        selector(input, &mut self.walk.names).map_err(|_| input.new_custom_error(()))
    }

    fn parse_block<'t>(
        &mut self,
        (prelude, selectors): Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<(), ParseError<'i, ()>> {
        if self.walk.css_rules >= MAX_CSS_RULES
            || self.walk.css_selectors + selectors.len() > MAX_CSS_SELECTORS
        {
            self.walk.view.limited = true;
            return Err(input.new_custom_error(()));
        }
        let mut out = Vec::new();
        let mut decls = Declarations {
            walk: self.walk,
            out: &mut out,
        };
        for _ in RuleBodyParser::new(input, &mut decls) {}
        if !out.is_empty() {
            let body = join_declarations(&out, |_| true);
            self.walk.css_rules += 1;
            self.walk.css_selectors += selectors.len();
            self.walk.css_len += prelude.len() + body.len() + 2;
            self.walk.rules.push(StyleRule {
                prelude,
                selectors,
                declarations: out,
                body,
                media: self.media,
            });
        }
        Ok(())
    }
}

impl<'i> AtRuleParser<'i> for Rules<'_> {
    type Prelude = String;
    type AtRule = ();
    type Error = ();

    fn parse_prelude<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
    ) -> Result<String, ParseError<'i, ()>> {
        // Only one level of `@media`. Every other at-rule goes, including
        // `@import` and `@font-face`, which would load remote resources.
        if self.media.is_some() || !name.eq_ignore_ascii_case("media") {
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
        let before = self.walk.rules.len();
        let mut rules = Rules {
            media: Some(self.walk.media.len()),
            walk: self.walk,
        };
        for _ in RuleBodyParser::new(input, &mut rules) {}
        if self.walk.rules.len() > before {
            self.walk.css_len += "@media {}".len() + prelude.len();
            self.walk.media.push(prelude);
        }
        Ok(())
    }
}

impl<'i> DeclarationParser<'i> for Rules<'_> {
    type Declaration = ();
    type Error = ();
}

impl<'i> RuleBodyItemParser<'i, (), ()> for Rules<'_> {
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

/// One complex selector of a list, such as `table.x > td a`, as litehtml
/// matches it: compound selectors from left to right and the combinators
/// between them.
#[derive(Default)]
struct Selector {
    /// As written to the output.
    text: String,
    parts: Vec<Compound>,
    combinators: Vec<Combinator>,
    /// Some part has a pseudo-class.
    pseudo: bool,
}

impl Selector {
    fn rightmost(&self) -> &Compound {
        &self.parts[self.parts.len() - 1]
    }
}

/// The parts of a compound selector that decide which elements it may
/// match, as `Names`. Attribute selectors and pseudo-classes are left out, so
/// it matches at least every element the real selector does.
#[derive(Default)]
struct Compound {
    tag: Option<u32>,
    id: Option<u32>,
    /// Sorted once the compound is read.
    classes: Vec<u32>,
    /// The first class when it is written before any other subclass:
    /// litehtml skips elements without it before matching.
    first_class: Option<u32>,
    subclasses: usize,
    pseudo: bool,
}

impl Compound {
    /// Whether litehtml's quick check by tag and first class lets `element`
    /// through to matching.
    fn tried(&self, element: &Element) -> bool {
        self.tag.is_none_or(|tag| tag == element.tag)
            && self
                .first_class
                .is_none_or(|class| element.classes.binary_search(&class).is_ok())
    }

    fn may_match(&self, element: &Element) -> bool {
        self.tag.is_none_or(|tag| tag == element.tag)
            && self
                .id
                .is_none_or(|id| element.ids.binary_search(&id).is_ok())
            && self
                .classes
                .iter()
                .all(|class| element.classes.binary_search(class).is_ok())
    }

    /// Checking this compound on an element of its tag with `classes`
    /// classes, each of the compound's looked up in the element's list.
    fn check(&self, classes: usize) -> f64 {
        1.0 + (self.classes.len() * classes) as f64 * CSS_LOOKUP_COST
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Combinator {
    Descendant,
    Child,
    Adjacent,
    Sibling,
}

/// Builds `Selector`s token by token. Fails on what litehtml would not parse
/// as a selector, such as a dangling combinator, so the estimate matches
/// what it sees.
struct SelectorList<'n> {
    names: &'n mut Names,
    done: Vec<Selector>,
    current: Selector,
    compound: Option<Compound>,
    combinator: Option<Combinator>,
    /// A `.` waiting for its class name.
    dot: bool,
}

impl SelectorList<'_> {
    /// End the compound being read, at whitespace, a combinator or a comma.
    fn close(&mut self) -> Result<(), ()> {
        if self.dot {
            return Err(());
        }
        if let Some(mut compound) = self.compound.take() {
            compound.classes.sort_unstable();
            self.current.pseudo |= compound.pseudo;
            self.current.parts.push(compound);
        }
        Ok(())
    }

    fn space(&mut self) -> Result<(), ()> {
        self.close()?;
        if !self.current.parts.is_empty() && self.combinator.is_none() {
            self.combinator = Some(Combinator::Descendant);
        }
        Ok(())
    }

    fn combinator(&mut self, combinator: Combinator) -> Result<(), ()> {
        self.close()?;
        if self.current.parts.is_empty()
            || self.combinator.is_some_and(|c| c != Combinator::Descendant)
        {
            return Err(());
        }
        self.combinator = Some(combinator);
        Ok(())
    }

    /// The compound being read, starting one after any pending combinator.
    fn compound(&mut self) -> Result<&mut Compound, ()> {
        if self.dot {
            return Err(());
        }
        if self.compound.is_none() {
            if !self.current.parts.is_empty() {
                let combinator = self.combinator.take().ok_or(())?;
                self.current.combinators.push(combinator);
            }
            if self.current.parts.len() >= MAX_SELECTOR_PARTS {
                return Err(());
            }
            self.compound = Some(Compound::default());
        }
        self.compound.as_mut().ok_or(())
    }

    /// A type selector, `*` as `None`, which must start its compound.
    fn tag(&mut self, tag: Option<u32>) -> Result<(), ()> {
        if self.compound.is_some() {
            return Err(());
        }
        self.compound()?.tag = tag;
        Ok(())
    }

    fn ident(&mut self, name: &str) -> Result<(), ()> {
        let name = self.names.id(name);
        if !self.dot {
            return self.tag(Some(name));
        }
        self.dot = false;
        let compound = self.compound()?;
        if compound.subclasses == 0 {
            compound.first_class = Some(name);
        }
        compound.subclasses += 1;
        compound.classes.push(name);
        Ok(())
    }

    fn id(&mut self, name: &str) -> Result<(), ()> {
        let name = self.names.id(name);
        self.subclass()?.id.get_or_insert(name);
        Ok(())
    }

    fn subclass(&mut self) -> Result<&mut Compound, ()> {
        let compound = self.compound()?;
        compound.subclasses += 1;
        Ok(compound)
    }

    /// End one selector of the list; `text` is how it was written.
    fn end(&mut self, text: &str) -> Result<(), ()> {
        self.close()?;
        if self.current.parts.is_empty()
            || self
                .combinator
                .take()
                .is_some_and(|c| c != Combinator::Descendant)
        {
            return Err(());
        }
        self.current.text = text.trim().to_owned();
        self.done.push(std::mem::take(&mut self.current));
        Ok(())
    }
}

/// Type, class, id, universal, simple attribute selectors, combinators and a
/// few structural pseudo-classes. Anything else rejects the whole rule.
/// Returns the list as written to the output and each selector in it.
fn selector<'i>(
    input: &mut Parser<'i, '_>,
    names: &mut Names,
) -> Result<(String, Vec<Selector>), ()> {
    if escaped(input) {
        return Err(());
    }
    let mut out = String::new();
    let mut list = SelectorList {
        names,
        done: Vec::new(),
        current: Selector::default(),
        compound: None,
        combinator: None,
        dot: false,
    };
    let mut start = 0;
    loop {
        let token = match input.next_including_whitespace() {
            Ok(token) => token.clone(),
            Err(_) => break,
        };
        match token {
            Token::WhiteSpace(_) => {
                out.push(' ');
                list.space()?;
            }
            Token::Comma => {
                list.end(&out[start..])?;
                out.push(',');
                start = out.len();
            }
            Token::Ident(ref name) => {
                out.push_str(&token.to_css_string());
                list.ident(name)?;
            }
            Token::IDHash(ref id) => {
                out.push_str(&token.to_css_string());
                list.id(id)?;
            }
            Token::Delim(c @ ('.' | '*' | '>' | '+' | '~')) => {
                out.push(c);
                match c {
                    '.' => {
                        list.compound()?;
                        list.dot = true;
                    }
                    '*' => list.tag(None)?,
                    '>' => list.combinator(Combinator::Child)?,
                    '+' => list.combinator(Combinator::Adjacent)?,
                    _ => list.combinator(Combinator::Sibling)?,
                }
            }
            Token::Colon => match input.next_including_whitespace() {
                Ok(Token::Ident(p))
                    if PSEUDO_CLASSES.contains(&p.to_ascii_lowercase().as_str()) =>
                {
                    let _ = write!(out, ":{}", p.to_ascii_lowercase());
                    list.subclass()?.pseudo = true;
                }
                _ => return Err(()),
            },
            Token::SquareBracketBlock => {
                list.subclass()?;
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
    list.end(&out[start..])?;
    Ok((out.trim().to_owned(), list.done))
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
