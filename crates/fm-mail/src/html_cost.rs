//! What parsing received HTML may cost, judged before html5ever sees it.
//!
//! html5ever's tree builder scans its stack of open elements and its list of
//! active formatting elements on most tags, and compares every attribute of a
//! tag with the ones before it, so deep nesting, piles of unclosed `<b>` or
//! `<font>`, or tags with thousands of attributes make parsing quadratic: a
//! few hundred kilobytes can take minutes. [`parses_cheaply`] runs one linear
//! pass that over-estimates that work; past its bound callers skip the parser
//! and read the text with [`strip_tags`]. Pure; no I/O.

use std::collections::HashMap;

/// Elements open at once, on the modelled stack or counted as not yet closed
/// by their own end tag. Newsletters built from nested tables stay well under
/// a hundred.
const MAX_OPEN: usize = 512;
/// Attributes on one tag; the tokenizer checks each against all before it.
const MAX_ATTRS: usize = 512;
/// Estimated tree-builder steps for a whole document: about a second of
/// parsing in a debug build. Real mail measures a few percent of it.
const MAX_COST: u64 = 10_000_000;
/// Steps charged for each formatting element the parser reopens: a new
/// element with its attributes copied, against a stack step or comparison.
const RECONSTRUCT: u64 = 200;

/// Elements whose content html5ever reads as text up to their own end tag,
/// in HTML. `plaintext` has no end tag.
const RAW_TEXT: &[&str] = &[
    "script",
    "style",
    "textarea",
    "title",
    "xmp",
    "iframe",
    "noembed",
    "noframes",
    "noscript",
    "plaintext",
];
/// Elements that never hold content.
const VOID: &[&str] = &[
    "area", "base", "basefont", "bgsound", "br", "col", "embed", "frame", "hr", "image", "img",
    "input", "keygen", "link", "meta", "param", "source", "track", "wbr",
];
/// Elements that a start tag of the same kind closes rather than nests in.
/// They are left out of the count of unclosed elements; the modelled stack
/// still holds them.
const SELF_CLOSING_KIND: &[&str] = &[
    "p", "li", "dt", "dd", "td", "th", "tr", "tbody", "thead", "tfoot", "caption", "colgroup",
    "option",
];
/// Elements the tree builder keeps in its active formatting list, reopening
/// them after a block closes them.
const FORMATTING: &[&str] = &[
    "a", "b", "big", "code", "em", "font", "i", "nobr", "s", "small", "strike", "strong", "tt", "u",
];
/// Tags a table takes as its own rather than moving them in front of it.
const TABLE_CONTENT: &[&str] = &[
    "table", "caption", "colgroup", "col", "tbody", "thead", "tfoot", "tr", "td", "th", "style",
    "script", "template", "input", "form",
];
/// Elements that start a new run of the active formatting list; closing one
/// drops the formatting opened inside it.
const MARKERS: &[&str] = &[
    "applet", "caption", "marquee", "object", "td", "template", "th",
];
/// HTML elements that end an element search: an end tag beyond one is
/// ignored. SVG and MathML elements with HTML inside do too.
const SCOPE: &[&str] = &[
    "applet", "caption", "html", "marquee", "object", "table", "td", "th", "template",
];
/// HTML elements a list item or definition does not close beyond, but for
/// `address`, `div` and `p`, and where an unmatched end tag stops. SVG and
/// MathML elements with HTML inside are special too.
const SPECIAL: &[&str] = &[
    "address",
    "applet",
    "area",
    "article",
    "aside",
    "base",
    "basefont",
    "bgsound",
    "blockquote",
    "body",
    "br",
    "button",
    "caption",
    "center",
    "col",
    "colgroup",
    "dd",
    "details",
    "dir",
    "div",
    "dl",
    "dt",
    "embed",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "frame",
    "frameset",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hgroup",
    "hr",
    "html",
    "iframe",
    "img",
    "input",
    "keygen",
    "li",
    "link",
    "listing",
    "main",
    "marquee",
    "menu",
    "meta",
    "nav",
    "noembed",
    "noframes",
    "noscript",
    "object",
    "ol",
    "p",
    "param",
    "plaintext",
    "pre",
    "script",
    "search",
    "section",
    "select",
    "source",
    "style",
    "summary",
    "table",
    "tbody",
    "td",
    "template",
    "textarea",
    "tfoot",
    "th",
    "thead",
    "title",
    "tr",
    "track",
    "ul",
    "wbr",
    "xmp",
];
/// Start tags that end an open paragraph.
const CLOSES_P: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "center",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "hr",
    "listing",
    "main",
    "menu",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "summary",
    "table",
    "ul",
];
/// HTML start tags that end SVG or MathML content around them. `font` also
/// does when it carries a color, face or size attribute (checked separately).
const BREAKOUT: &[&str] = &[
    "b",
    "big",
    "blockquote",
    "body",
    "br",
    "center",
    "code",
    "dd",
    "div",
    "dl",
    "dt",
    "em",
    "embed",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "hr",
    "i",
    "img",
    "li",
    "listing",
    "menu",
    "meta",
    "nobr",
    "ol",
    "p",
    "pre",
    "ruby",
    "s",
    "small",
    "span",
    "strong",
    "strike",
    "sub",
    "sup",
    "table",
    "tt",
    "u",
    "ul",
    "var",
];

/// Whether html5ever can parse `html` in bounded time. Linear in its length.
pub(crate) fn parses_cheaply(html: &str) -> bool {
    cost(html) <= MAX_COST
}

/// Where an element belongs. In SVG and MathML `/>` closes an element.
#[derive(Clone, Copy, PartialEq)]
enum Ns {
    Html,
    Svg,
    Math,
}

/// One element on the modelled stack of open elements.
struct Open<'a> {
    name: &'a str,
    ns: Ns,
    /// An SVG or MathML element whose content is read as HTML again.
    html_inside: bool,
    /// Markers at or below it on the stack.
    level: usize,
    /// For a formatting element in the active list: its attribute source.
    listed: Option<&'a [u8]>,
}

/// A formatting element a closing block took off the stack while it stays
/// in the active list: its name, attribute source and marker level.
type Stale<'a> = (&'a str, &'a [u8], usize);

/// Estimated tree-builder steps for `html`; `u64::MAX` past a hard bound.
/// Stops counting once past `MAX_COST`, so it stays linear in `html`.
fn cost(html: &str) -> u64 {
    let source = html.as_bytes();
    let lower = html.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    // Per tag name: one entry per element not yet closed by its own end tag,
    // holding its weight in the formatting list (attributes plus one) or zero.
    // Elements closed some other way still count, so this over-estimates
    // whatever the modelled stack below gets wrong.
    let mut unclosed: HashMap<&str, Vec<u64>> = HashMap::new();
    let (mut depth, mut formatting, mut cost) = (0usize, 0u64, 0u64);
    // The parser's stack of open elements as it would likely stand, and the
    // formatting elements it reopens, each a new element, before the next
    // text or inline tag.
    let mut stack: Vec<Open> = Vec::new();
    let mut stale: Vec<Stale> = Vec::new();
    // Content moved in front of a table ("foster parenting"): the parser
    // finds the table among its parent's children for each, so it adds up.
    let mut fostered = 0u64;
    // The end of a script, style, textarea or similar element's content.
    // html5ever reads it as text, unless the element is in SVG or MathML,
    // where it is markup, so it is read here as both: start tags count,
    // while end tags, comments and quotes, which could hide markup, do not.
    let mut raw_end = None;
    let mut i = 0;
    loop {
        let limit = raw_end.unwrap_or(bytes.len());
        let Some(at) = find(&bytes[..limit], i, b"<") else {
            if raw_end.take().is_none() {
                break;
            }
            i = i.max(limit);
            continue;
        };
        let text = at > i;
        let solid = bytes[i..at].iter().any(|&b| !is_space(b));
        i = at + 1;
        let Some(tag) = Tag::read(&bytes[..limit], at, raw_end.is_some()) else {
            continue;
        };
        i = tag.end;
        if tag.attrs > MAX_ATTRS {
            return u64::MAX;
        }
        let attrs = tag.attrs as u64;
        cost += 1 + (depth + stack.len()) as u64 + formatting + attrs * attrs / 2;
        let in_table = |stack: &[Open]| {
            stack.last().is_some_and(|open| {
                matches!(open.name, "table" | "tbody" | "thead" | "tfoot" | "tr")
            })
        };
        if text {
            if solid && in_table(&stack) {
                fostered += 1;
                cost += fostered;
            }
            cost += reconstruct(&mut stack, &mut stale);
        }
        if cost > MAX_COST {
            return cost;
        }
        let Some(name) = tag.name.and_then(|name| std::str::from_utf8(name).ok()) else {
            continue;
        };
        let is = |list: &[&str]| list.contains(&name);
        let foreign = stack.last().is_some_and(|open| {
            in_foreign(open) && !(open.name == "annotation-xml" && name == "svg")
        });
        if tag.closing && raw_end.is_some() {
            continue;
        }
        if tag.closing {
            let closed = if foreign
                && !matches!(name, "p" | "br")
                && close(&mut stack, &mut stale, &[name], |open| open.ns == Ns::Html)
            {
                true
            } else {
                if foreign && matches!(name, "p" | "br") {
                    break_out(&mut stack);
                }
                match name {
                    // These end no element, or only the one named.
                    "body" | "html" | "head" | "br" => false,
                    _ if name == "form" || is(FORMATTING) => {
                        // A form ends alone; the adoption agency may keep
                        // what opened inside a formatting element open.
                        let level = level(&stack);
                        if let Some(at) = stack.iter().rposition(|open| open.name == name) {
                            stack.remove(at);
                            true
                        } else if let Some(at) =
                            stale.iter().rposition(|s| s.0 == name && s.2 == level)
                        {
                            stale.remove(at);
                            true
                        } else {
                            false
                        }
                    }
                    "table" | "tbody" | "thead" | "tfoot" | "tr" | "td" | "th" | "caption" => {
                        close(&mut stack, &mut stale, &[name], |open| {
                            matches!(open.name, "html" | "table" | "template")
                        })
                    }
                    "li" => close(&mut stack, &mut stale, &[name], |open| {
                        in_scope_list(open) || matches!(open.name, "ol" | "ul")
                    }),
                    "p" => close(&mut stack, &mut stale, &[name], |open| {
                        in_scope_list(open) || open.name == "button"
                    }),
                    _ if SPECIAL.contains(&name) => {
                        close(&mut stack, &mut stale, &[name], in_scope_list)
                    }
                    _ => close(&mut stack, &mut stale, &[name], special),
                }
            };
            // An end tag the parser ignores leaves its element counted.
            if let Some(weight) = unclosed.get_mut(name).filter(|_| closed).and_then(Vec::pop) {
                depth -= 1;
                formatting -= weight;
            }
            // A stray `</p>` or `</br>` makes an element, maybe before a table.
            if !closed && matches!(name, "p" | "br") && in_table(&stack) {
                fostered += 1;
                cost += fostered;
            }
            continue;
        }
        if in_table(&stack) && !is(TABLE_CONTENT) {
            fostered += 1;
            cost += fostered;
        }
        if is(RAW_TEXT) && raw_end.is_none() {
            raw_end = Some(raw_text_end(bytes, i, name.as_bytes()));
        }
        let self_closing = tag.self_closing;
        // Count the element as unclosed when `opened`; whether either count
        // is now past the bound.
        let mut too_deep = |stack: &[Open], opened: bool| -> bool {
            if opened && !is(SELF_CLOSING_KIND) {
                let weight = if is(FORMATTING) { attrs + 1 } else { 0 };
                depth += 1;
                formatting += weight;
                unclosed.entry(name).or_default().push(weight);
            }
            depth > MAX_OPEN || stack.len() > MAX_OPEN
        };
        let breakout = is(BREAKOUT) || (name == "font" && tag.font_breakout);
        if foreign && !breakout {
            // An SVG or MathML element; `/>` closes it.
            if !self_closing {
                let level = level(&stack);
                let ns = stack.last().map_or(Ns::Html, |open| open.ns);
                stack.push(Open {
                    name,
                    ns,
                    html_inside: html_inside(ns, name, tag.html_encoding),
                    level,
                    listed: None,
                });
            }
            if too_deep(&stack, !self_closing) {
                return u64::MAX;
            }
            continue;
        }
        if foreign {
            break_out(&mut stack);
        }
        let table_part = matches!(
            name,
            "td" | "th" | "tr" | "tbody" | "thead" | "tfoot" | "caption" | "colgroup"
        );
        if table_part && !stack.iter().any(|open| open.name == "table") {
            // Ignored outside a table.
            continue;
        }
        // Start tags that end an open element of their kind, searching no
        // further than the parser does.
        let table_scope = |open: &Open| matches!(open.name, "html" | "table" | "template");
        let list_item =
            |open: &Open| special(open) && !matches!(open.name, "address" | "div" | "p");
        match name {
            "li" => close(&mut stack, &mut stale, &["li"], list_item),
            "dt" | "dd" => close(&mut stack, &mut stale, &["dt", "dd"], list_item),
            "td" | "th" => close(&mut stack, &mut stale, &["td", "th"], table_scope),
            "tr" => close(&mut stack, &mut stale, &["tr"], table_scope),
            "tbody" | "thead" | "tfoot" => close(
                &mut stack,
                &mut stale,
                &["tbody", "thead", "tfoot"],
                table_scope,
            ),
            "caption" | "colgroup" => close(&mut stack, &mut stale, &[name], table_scope),
            "option" | "optgroup" => {
                // Only an option just opened; an option group nests outside
                // a select, so it is never ended here.
                if stack.last().is_some_and(|open| open.name == "option") {
                    stack.pop();
                }
                false
            }
            _ => false,
        };
        if CLOSES_P.contains(&name) || matches!(name, "li" | "dt" | "dd") {
            close(&mut stack, &mut stale, &["p"], |open| {
                in_scope_list(open) || open.name == "button"
            });
        } else if !is(SELF_CLOSING_KIND) {
            // Inline content: the parser reopens what a block closed first.
            cost += reconstruct(&mut stack, &mut stale);
        }
        if is(VOID) {
            continue;
        }
        let level = level(&stack) + is(MARKERS) as usize;
        let listed = is(FORMATTING).then_some(&source[tag.attr_range.0..tag.attr_range.1]);
        if let Some(source) = listed {
            // Like the parser, keep at most three identical entries after
            // the last marker.
            let same = |n: &str, s: &[u8], l: usize| n == name && s == source && l == level;
            let count = stack
                .iter()
                .filter(|open| open.listed.is_some_and(|s| same(open.name, s, open.level)))
                .count()
                + stale.iter().filter(|s| same(s.0, s.1, s.2)).count();
            if count >= 3 {
                if let Some(at) = stale.iter().position(|s| same(s.0, s.1, s.2)) {
                    stale.remove(at);
                } else if let Some(open) = stack
                    .iter_mut()
                    .find(|open| open.listed.is_some_and(|s| same(open.name, s, open.level)))
                {
                    open.listed = None;
                }
            }
        }
        let ns = match name {
            "svg" => Ns::Svg,
            "math" => Ns::Math,
            _ => Ns::Html,
        };
        if ns != Ns::Html && self_closing {
            continue;
        }
        stack.push(Open {
            name,
            ns,
            html_inside: false,
            level,
            listed,
        });
        if too_deep(&stack, true) {
            return u64::MAX;
        }
    }
    cost
}

/// Markers on the stack, which bound the active formatting list's last run.
fn level(stack: &[Open]) -> usize {
    stack.last().map_or(0, |open| open.level)
}

/// Pop `stack` through its topmost element named in `target`, unless an
/// element where the parser stops looking comes first. Formatting elements
/// popped on the way stay in the active list unless a marker went with
/// them. Whether one was found.
fn close<'a>(
    stack: &mut Vec<Open<'a>>,
    stale: &mut Vec<Stale<'a>>,
    target: &[&str],
    stop: impl Fn(&Open) -> bool,
) -> bool {
    let mut found = None;
    for (at, open) in stack.iter().enumerate().rev() {
        if target.contains(&open.name) {
            found = Some(at);
            break;
        }
        if stop(open) {
            break;
        }
    }
    let Some(at) = found else {
        return false;
    };
    for open in stack.drain(at..) {
        if let Some(source) = open.listed {
            stale.push((open.name, source, open.level));
        }
    }
    let level = level(stack);
    stale.retain(|s| s.2 <= level);
    true
}

/// Whether `open` ends an element search.
fn in_scope_list(open: &Open) -> bool {
    match open.ns {
        Ns::Html => SCOPE.contains(&open.name),
        _ => foreign_special(open),
    }
}

/// Whether `open` is in the parser's special category.
fn special(open: &Open) -> bool {
    match open.ns {
        Ns::Html => SPECIAL.contains(&open.name),
        _ => foreign_special(open),
    }
}

/// The SVG and MathML elements the parser treats as special.
fn foreign_special(open: &Open) -> bool {
    match open.ns {
        Ns::Svg => matches!(open.name, "foreignobject" | "desc" | "title"),
        Ns::Math => matches!(
            open.name,
            "mi" | "mo" | "mn" | "ms" | "mtext" | "annotation-xml"
        ),
        Ns::Html => false,
    }
}

/// Whether content inside `open` is SVG or MathML.
fn in_foreign(open: &Open) -> bool {
    open.ns != Ns::Html && !open.html_inside
}

/// Whether an SVG or MathML element's content is read as HTML.
fn html_inside(ns: Ns, name: &str, html_encoding: bool) -> bool {
    match ns {
        Ns::Svg => matches!(name, "foreignobject" | "desc" | "title"),
        Ns::Math => {
            matches!(name, "mi" | "mo" | "mn" | "ms" | "mtext")
                || (name == "annotation-xml" && html_encoding)
        }
        Ns::Html => false,
    }
}

/// Pop SVG and MathML elements off the top, as HTML content ends them.
fn break_out(stack: &mut Vec<Open>) {
    while stack.last().is_some_and(in_foreign) {
        stack.pop();
    }
}

/// Put the stale formatting elements of the current run back on the stack,
/// as the parser does before text or an inline tag; their cost.
fn reconstruct<'a>(stack: &mut Vec<Open<'a>>, stale: &mut Vec<Stale<'a>>) -> u64 {
    let level = level(stack);
    let mut reopened = 0;
    stale.retain(|&(name, source, at)| {
        if at != level {
            return true;
        }
        stack.push(Open {
            name,
            ns: Ns::Html,
            html_inside: false,
            level,
            listed: Some(source),
        });
        reopened += 1;
        false
    });
    RECONSTRUCT * reopened
}

/// One `<…>` construct as the tokenizer would read it.
struct Tag<'a> {
    /// Element name; `None` for comments, doctypes and other markup.
    name: Option<&'a [u8]>,
    /// Where its attributes' source lies, to tell identical tags apart.
    attr_range: (usize, usize),
    /// It has `encoding="text/html"` or `encoding="application/xhtml+xml"`.
    html_encoding: bool,
    /// It has a color, face or size attribute, even without a value.
    font_breakout: bool,
    closing: bool,
    self_closing: bool,
    attrs: usize,
    /// Index just past the construct.
    end: usize,
}

impl<'a> Tag<'a> {
    /// The construct at `bytes[at] == b'<'`, or `None` when the `<` is text.
    /// `bytes` is the lowercased source. Where it may be text instead,
    /// `doubtful` reads no comments and ends a quoted value at `>`, so the
    /// construct hides nothing that would be markup.
    fn read(bytes: &'a [u8], at: usize, doubtful: bool) -> Option<Tag<'a>> {
        let rest = &bytes[at + 1..];
        let markup = |end| Tag {
            name: None,
            attr_range: (0, 0),
            html_encoding: false,
            font_breakout: false,
            closing: false,
            self_closing: false,
            attrs: 0,
            end,
        };
        if doubtful && matches!(rest.first(), Some(b'!' | b'?')) {
            return None;
        }
        if rest.starts_with(b"!--") {
            // `<!-->` and `<!--->` are empty comments; others end at `-->` or `--!>`.
            let body = at + 4;
            if bytes[body..].starts_with(b">") {
                return Some(markup(body + 1));
            }
            if bytes[body..].starts_with(b"->") {
                return Some(markup(body + 2));
            }
            let mut i = body;
            while let Some(dashes) = find(bytes, i, b"--") {
                match bytes.get(dashes + 2) {
                    Some(b'>') => return Some(markup(dashes + 3)),
                    Some(b'!') if bytes.get(dashes + 3) == Some(&b'>') => {
                        return Some(markup(dashes + 4))
                    }
                    _ => i = dashes + 1,
                }
            }
            return Some(markup(bytes.len()));
        }
        let (closing, name_at) = match rest.first() {
            Some(b'!' | b'?') => {
                return Some(markup(find(bytes, at, b">").map_or(bytes.len(), |e| e + 1)))
            }
            Some(b'/') => (true, at + 2),
            _ => (false, at + 1),
        };
        if !bytes.get(name_at).is_some_and(u8::is_ascii_alphabetic) {
            // `</` followed by anything but a letter is a bogus comment; `<` is text.
            return closing.then(|| markup(find(bytes, at, b">").map_or(bytes.len(), |e| e + 1)));
        }
        let mut i = name_at;
        while i < bytes.len() && !is_space(bytes[i]) && !matches!(bytes[i], b'/' | b'>') {
            i += 1;
        }
        let name = &bytes[name_at..i];
        let attrs_at = i;
        let (mut attrs, mut self_closing, mut html_encoding) = (0, false, false);
        let mut font_breakout = false;
        loop {
            while i < bytes.len() && (is_space(bytes[i]) || bytes[i] == b'/') {
                self_closing = bytes[i] == b'/';
                i += 1;
            }
            match bytes.get(i) {
                None => break,
                Some(b'>') => {
                    i += 1;
                    break;
                }
                _ => {}
            }
            self_closing = false;
            // An attribute name runs to a space, `/`, `>` or `=`; a leading `=` belongs to it.
            attrs += 1;
            let attr_at = i;
            i += 1;
            while i < bytes.len() && !is_space(bytes[i]) && !matches!(bytes[i], b'/' | b'>' | b'=')
            {
                i += 1;
            }
            let encoding = &bytes[attr_at..i] == b"encoding";
            font_breakout |= matches!(&bytes[attr_at..i], b"color" | b"face" | b"size");
            while i < bytes.len() && is_space(bytes[i]) {
                i += 1;
            }
            if bytes.get(i) != Some(&b'=') {
                continue;
            }
            i += 1;
            while i < bytes.len() && is_space(bytes[i]) {
                i += 1;
            }
            let value = match bytes.get(i) {
                Some(&quote @ (b'"' | b'\'')) => {
                    let start = i + 1;
                    i = start;
                    while i < bytes.len() && bytes[i] != quote && !(doubtful && bytes[i] == b'>') {
                        i += 1;
                    }
                    let value = &bytes[start..i];
                    i += (bytes.get(i) == Some(&quote)) as usize;
                    value
                }
                _ => {
                    let start = i;
                    while i < bytes.len() && !is_space(bytes[i]) && bytes[i] != b'>' {
                        i += 1;
                    }
                    &bytes[start..i]
                }
            };
            html_encoding |= encoding && matches!(value, b"text/html" | b"application/xhtml+xml");
        }
        Some(Tag {
            name: Some(name),
            attr_range: (attrs_at, i),
            html_encoding,
            font_breakout,
            closing,
            self_closing,
            attrs,
            end: i,
        })
    }
}

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')
}

fn find(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from >= bytes.len() {
        return None;
    }
    bytes[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|at| from + at)
}

/// Index of the `</name` that ends a raw-text element opened before `from`.
fn raw_text_end(bytes: &[u8], from: usize, name: &[u8]) -> usize {
    let mut i = from;
    while let Some(at) = find(bytes, i, b"</") {
        let start = at + 2;
        let candidate = bytes.get(start..start + name.len());
        let after = bytes.get(start + name.len()).copied();
        if candidate.is_some_and(|c| c.eq_ignore_ascii_case(name))
            && after.is_none_or(|b| is_space(b) || b == b'/' || b == b'>')
        {
            return at;
        }
        i = start;
    }
    bytes.len()
}

/// Readable text from HTML too costly to parse, in one linear pass: tags
/// removed, blocks on their own lines, scripts, styles and embedded documents
/// dropped with their content, and common character references decoded.
pub(crate) fn strip_tags(html: &str) -> String {
    const DROPPED: &[&str] = &[
        "script", "style", "template", "title", "iframe", "svg", "math", "textarea", "xmp",
        "noembed", "noframes",
    ];
    const BREAKS: &[&str] = &[
        "br",
        "p",
        "div",
        "li",
        "tr",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "blockquote",
        "pre",
        "table",
        "ul",
        "ol",
        "hr",
        "section",
        "article",
        "header",
        "footer",
    ];
    let lower = html.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut text = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let next = find(bytes, i, b"<").unwrap_or(bytes.len());
        text.push_str(&html[i..next]);
        if next == bytes.len() {
            break;
        }
        let Some(tag) = Tag::read(bytes, next, false) else {
            text.push('<');
            i = next + 1;
            continue;
        };
        i = tag.end;
        let Some(name) = tag.name else {
            continue;
        };
        let name_str = std::str::from_utf8(name).unwrap_or("");
        if BREAKS.contains(&name_str) {
            text.push('\n');
        }
        if !tag.closing && !tag.self_closing && DROPPED.contains(&name_str) {
            let end = raw_text_end(bytes, i, name);
            i = find(bytes, end, b">").map_or(bytes.len(), |e| e + 1);
        }
    }
    let text = decode_references(&text);
    // Collapse spaces within lines and blank lines to one.
    let mut out = String::with_capacity(text.len());
    let mut blank = false;
    for line in text.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            blank = !out.is_empty();
            continue;
        }
        if blank {
            out.push('\n');
            blank = false;
        }
        out.push_str(&line);
        out.push('\n');
    }
    out.truncate(out.trim_end().len());
    out
}

/// `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;`, `&nbsp;` and numeric references.
fn decode_references(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest[1..]
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '#')
            .map(|e| e + 1)
            .unwrap_or(rest.len());
        let name = &rest[1..end];
        let decoded = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => name.strip_prefix('#').and_then(|number| {
                let value = match number.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => number.parse().ok(),
                };
                value.and_then(char::from_u32)
            }),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end..];
                rest = rest.strip_prefix(';').unwrap_or(rest);
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Shapes that made html5ever's tree building quadratic, a few hundred
    /// kilobytes each, every one with readable text.
    fn hostile() -> Vec<(&'static str, String)> {
        let distinct = |n: usize| (0..n).map(|i| format!("<b c={i}>")).collect::<String>();
        let attrs = (0..100_000).map(|i| format!("a{i} ")).collect::<String>();
        vec![
            ("nested divs", "<div>".repeat(200_000)),
            ("distinct formatting", distinct(30_000)),
            ("lists in items", "<ul><li>".repeat(20_000)),
            ("lists in definitions", "<dd><li>".repeat(20_000)),
            ("option groups", "<optgroup>".repeat(40_000)),
            (
                "text before a table",
                format!("<table>{}", "x<br>".repeat(100_000)),
            ),
            ("attributes", format!("<b {attrs}>")),
            (
                "reopened formatting",
                format!("<p>{}</p>{}", distinct(500), "<p>x</p>".repeat(50_000)),
            ),
            ("form ends", "<form></span><span></form>".repeat(20_000)),
            (
                "cells in MathML",
                format!("{}{}", "<math>".repeat(300), "<td></option>".repeat(20_000)),
            ),
            (
                "markup in an SVG textarea",
                format!("<svg><font c=1><textarea>{}", "<div>".repeat(100_000)),
            ),
            (
                "markup after a comment in raw text",
                "<textarea><!--</textarea><div>-->".repeat(20_000),
            ),
        ]
        .into_iter()
        .map(|(name, html)| (name, format!("<p>Hello &amp; welcome</p>{html}")))
        .collect()
    }

    #[test]
    fn hostile_html_is_read_as_text_quickly() {
        for (name, html) in hostile() {
            let start = Instant::now();
            assert!(!parses_cheaply(&html), "{name}");
            let view = crate::html_view::sanitize(&html).unwrap();
            assert!(
                view.limited && view.html.contains("Hello &amp; welcome"),
                "{name}"
            );
            assert!(view.links.is_empty() && view.resources.is_empty(), "{name}");
            let text = crate::html_source_text(&html);
            assert!(text.starts_with("Hello & welcome"), "{name}: {text:.80}");
            crate::html_view::link_hosts(Some(&html), "");
            let took = start.elapsed();
            // Generous for debug builds on a busy machine; it was minutes.
            assert!(took < Duration::from_secs(2), "{name} took {took:?}");
        }
    }

    #[test]
    fn foreign_font_breakouts_are_bounded() {
        for container in ["svg", "math"] {
            for attribute in [
                "color=red",
                "FACE='Arial'",
                "size",
                "color=\"\"",
                "/color=red",
            ] {
                let html = format!(
                    "<p>Hello &amp; welcome</p><{container}><font {attribute}>{}",
                    "<g/>".repeat(50_000)
                );
                let start = Instant::now();
                assert!(!parses_cheaply(&html), "{container}: {attribute}");
                let view = crate::html_view::sanitize(&html).unwrap();
                assert!(view.limited && view.html.contains("Hello &amp; welcome"));
                assert!(view.links.is_empty() && view.resources.is_empty());
                assert!(crate::html_source_text(&html).starts_with("Hello & welcome"));
                assert!(crate::html_view::link_hosts(Some(&html), "").is_empty());
                assert!(start.elapsed() < Duration::from_secs(2));
            }
            // Without one of those attributes, the parser stays in the
            // foreign namespace and honors the self-closing elements.
            for attribute in ["", "data-color=red", "title='color=red'"] {
                let html = format!(
                    "<{container}><font {attribute}>{}</font></{container}>",
                    "<g/>".repeat(50_000)
                );
                assert!(parses_cheaply(&html), "{container}: {attribute}");
            }
        }
    }

    #[test]
    fn real_world_mail_stays_far_below_the_bound() {
        // A responsive newsletter as MJML writes it, with Outlook-only tables
        // in conditional comments, repeated to about 250 KB.
        let section = r#"<!--[if mso | IE]><table align="center" border="0" cellpadding="0" cellspacing="0" style="width:600px;" width="600"><tr><td style="line-height:0px;font-size:0px;"><![endif]--><div style="margin:0px auto;max-width:600px;"><table align="center" border="0" cellpadding="0" cellspacing="0" role="presentation" style="width:100%;"><tbody><tr><td style="direction:ltr;font-size:0px;padding:20px 0;text-align:center;"><!--[if mso | IE]><table role="presentation" border="0" cellpadding="0" cellspacing="0"><tr><td style="vertical-align:top;width:600px;"><![endif]--><div class="mj-column-per-100" style="font-size:0px;text-align:left;display:inline-block;vertical-align:top;width:100%;"><table border="0" cellpadding="0" cellspacing="0" role="presentation" width="100%"><tbody><tr><td align="left" style="padding:10px 25px;"><div style="font-family:Arial,sans-serif;font-size:13px;line-height:1.5;color:#000000;">News from <b>Example</b>: <a href="https://example.test/a" style="color:#1a73e8"><span style="text-decoration:underline">read more</span></a></div></td></tr><tr><td align="center"><img alt="" src="https://example.test/i.png" width="550" style="display:block;border:0"></td></tr></tbody></table></div><!--[if mso | IE]></td></tr></table><![endif]--></td></tr></tbody></table></div><!--[if mso | IE]></td></tr></table><![endif]-->"#;
        let newsletter = format!(
            "<!doctype html><html><head><style>p{{margin:0}}</style></head><body><div style=\"background:#fff\">{}</div></body></html>",
            section.repeat(150)
        );
        // Hand-written HTML of the old school: uppercase tags, unclosed
        // paragraphs, items, cells and fonts.
        let legacy = format!(
            "<HTML><BODY><FONT FACE=Arial SIZE=2>{}</BODY></HTML>",
            "<P>Hello<BR>see <A HREF=\"https://example.test\">here</A><UL><LI>one<LI>two</UL><TABLE><TR><TD>a<TD><FONT COLOR=red>b</FONT><TR><TD>c<TD>d</TABLE>".repeat(1_000)
        );
        for (name, html) in [("newsletter", newsletter), ("legacy", legacy)] {
            let estimate = cost(&html);
            assert!(estimate < MAX_COST / 20, "{name}: {estimate}");
            let view = crate::html_view::sanitize(&html).unwrap();
            assert!(!view.links.is_empty());
        }
    }

    #[test]
    fn stripped_text_keeps_lines_and_drops_active_content() {
        assert_eq!(
            strip_tags("<style>p{}</style><p>One &lt;two&gt;</p><div>Three<br>Four&#x21;</div><script>x()</script><svg><text>no</text></svg>Five"),
            "One <two>\n\nThree\nFour!\nFive"
        );
    }
}
