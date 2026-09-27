//! Plain text from sanitised HTML, laid out the way a browser shows it: each
//! block on its own line, paragraphs a blank line apart, one space between
//! inline runs, list items marked, and each blockquote level marked with `> `
//! as plain-text mail quotes. Pure; no I/O.
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

// Deeper nesting is read as plain text, so a hostile document cannot exhaust
// the stack.
const MAX_DEPTH: usize = 256;

/// The readable text of `html`.
pub(crate) fn html_to_text(html: &str) -> String {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    let mut text = Text::default();
    text.children(&dom.document, 0);
    text.finish()
}

#[derive(Default)]
struct Text {
    // Finished lines with the quote depth each was written at.
    lines: Vec<(usize, String)>,
    line: String,
    // Collapsed whitespace waiting to become one space before the next word.
    space: bool,
    // A paragraph ended: a blank line goes before the next text.
    gap: bool,
    quote: usize,
    pre: usize,
    // One entry per open list: the next number, or `None` for bullets.
    lists: Vec<Option<usize>>,
    // A list marker waiting for its item's first text.
    marker: Option<String>,
}

impl Text {
    fn children(&mut self, node: &Handle, depth: usize) {
        for child in node.children.borrow().iter() {
            self.node(child, depth + 1);
        }
    }

    fn node(&mut self, node: &Handle, depth: usize) {
        let tag = match &node.data {
            NodeData::Text { contents } => return self.text(&contents.borrow()),
            NodeData::Element { name, .. } => name.local.to_string(),
            NodeData::Document => return self.children(node, depth),
            _ => return,
        };
        if depth >= MAX_DEPTH {
            return self.flat(node);
        }
        match tag.as_str() {
            "head" | "script" | "style" | "template" | "title" => {}
            "br" => self.end_line(true),
            "blockquote" => {
                self.paragraph();
                self.quote += 1;
                self.children(node, depth);
                self.paragraph();
                self.quote -= 1;
            }
            "pre" => {
                self.paragraph();
                self.pre += 1;
                self.children(node, depth);
                self.pre -= 1;
                self.paragraph();
            }
            "ul" | "ol" => {
                // A nested list continues its item; a list of its own stands apart.
                if self.lists.is_empty() {
                    self.paragraph();
                } else {
                    self.end_line(false);
                }
                self.lists.push((tag == "ol").then_some(1));
                self.children(node, depth);
                self.lists.pop();
                if self.lists.is_empty() {
                    self.paragraph();
                }
            }
            "li" => {
                self.end_line(false);
                let indent = "  ".repeat(self.lists.len().saturating_sub(1));
                self.marker = Some(match self.lists.last_mut() {
                    Some(Some(number)) => {
                        *number += 1;
                        format!("{indent}{}. ", *number - 1)
                    }
                    _ => format!("{indent}• "),
                });
                self.children(node, depth);
                self.marker = None;
                self.end_line(false);
            }
            "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "table" | "hr" => {
                self.paragraph();
                self.children(node, depth);
                self.paragraph();
            }
            "div" | "section" | "article" | "header" | "footer" | "nav" | "aside" | "main"
            | "address" | "center" | "dl" | "dt" | "dd" | "figure" | "figcaption" | "tr"
            | "caption" | "details" | "summary" => {
                self.end_line(false);
                self.children(node, depth);
                self.end_line(false);
            }
            "td" | "th" => {
                // Cells on one row read as words apart.
                self.space = !self.line.is_empty();
                self.children(node, depth);
                self.space = !self.line.is_empty();
            }
            _ => self.children(node, depth),
        }
    }

    // Past the depth bound: the remaining text, in order, without structure.
    fn flat(&mut self, node: &Handle) {
        let mut stack = vec![node.clone()];
        while let Some(node) = stack.pop() {
            if let NodeData::Text { contents } = &node.data {
                self.text(&contents.borrow());
            }
            stack.extend(node.children.borrow().iter().rev().cloned());
        }
    }

    fn text(&mut self, text: &str) {
        for c in text.chars() {
            match c {
                '\n' if self.pre > 0 => self.end_line(true),
                '\r' if self.pre > 0 => {}
                ' ' | '\t' | '\n' | '\r' | '\x0c' if self.pre == 0 => self.space = true,
                // Byte-order marks and zero-width spaces some clients leave.
                '\u{feff}' | '\u{200b}' => {}
                _ => {
                    self.start_line();
                    if self.space && !self.line.is_empty() && !self.line.ends_with(' ') {
                        self.line.push(' ');
                    }
                    self.space = false;
                    self.line.push(if c == '\u{a0}' { ' ' } else { c });
                }
            }
        }
    }

    // Before a line's first character: the pending blank line, then the list marker.
    fn start_line(&mut self) {
        if !self.line.is_empty() {
            return;
        }
        if std::mem::take(&mut self.gap) && !self.lines.is_empty() {
            self.lines.push((self.quote, String::new()));
        }
        if let Some(marker) = self.marker.take() {
            self.line.push_str(&marker);
        }
    }

    // `force` keeps an empty line, as `<br>` does.
    fn end_line(&mut self, force: bool) {
        self.space = false;
        if self.line.is_empty() && !force {
            return;
        }
        if self.line.is_empty() {
            self.gap = false;
        }
        let line = std::mem::take(&mut self.line);
        self.lines.push((self.quote, line.trim_end().to_owned()));
    }

    fn paragraph(&mut self) {
        self.end_line(false);
        self.gap = true;
    }

    fn finish(mut self) -> String {
        self.end_line(false);
        let mut out = String::new();
        let (mut blank, mut previous) = (false, None::<usize>);
        for (depth, line) in &self.lines {
            if line.is_empty() {
                blank = true;
                continue;
            }
            // Blank lines collapse to one, quoted at the shallower of the
            // levels around it, so a quote ends before the text after it.
            if let Some(previous) = previous.filter(|_| blank) {
                out.push_str("> ".repeat(previous.min(*depth)).trim_end());
                out.push('\n');
            }
            blank = false;
            previous = Some(*depth);
            out.push_str(&"> ".repeat(*depth));
            out.push_str(line);
            out.push('\n');
        }
        out.truncate(out.trim_end().len());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_break_lines_and_inline_runs_keep_their_spaces() {
        assert_eq!(
            html_to_text("<div><b>From:</b> Panel &lt;panel@example.test&gt;<br><b>To:</b>  Team</div><div>Thanks,</div><div>Kris</div><div> On Monday, Alex wrote:<br></div>"),
            "From: Panel <panel@example.test>\nTo: Team\nThanks,\nKris\nOn Monday, Alex wrote:"
        );
        assert_eq!(
            html_to_text("<p>One</p><p><br></p><p>Two <i>and</i><b>three</b></p><div><br></div><div>Four</div>"),
            "One\n\nTwo andthree\n\nFour"
        );
        assert_eq!(html_to_text("\u{feff}<div>a&nbsp;&nbsp;b</div>"), "a  b");
    }

    #[test]
    fn quotes_are_marked_per_level_and_blank_lines_follow_the_outer_level() {
        let html = "<div>Begin forwarded message:<br><br></div><blockquote><div>From: A<br>Subject: S</div></blockquote><blockquote><div>Body</div><div>On Monday, B wrote:<br></div><blockquote><p>Hi</p><p>Old</p></blockquote></blockquote><p>After</p>";
        assert_eq!(
            html_to_text(html),
            "Begin forwarded message:\n\n> From: A\n> Subject: S\n>\n> Body\n> On Monday, B wrote:\n>\n> > Hi\n> >\n> > Old\n\nAfter"
        );
    }

    #[test]
    fn lists_tables_and_preformatted_text_read_naturally() {
        assert_eq!(
            html_to_text("<ul><li>One</li><li>Two<ol><li>A</li><li>B</li></ol></li></ul><table><tr><td>Name</td><td>Value</td></tr><tr><td>X</td><td>Y</td></tr></table><pre>a  b\n  c</pre>"),
            "• One\n• Two\n  1. A\n  2. B\n\nName Value\nX Y\n\na  b\n  c"
        );
        assert_eq!(
            html_to_text("<style>p{}</style><script>x</script><p>Kept</p>"),
            "Kept"
        );
    }

    #[test]
    fn deep_nesting_is_bounded_and_keeps_its_text() {
        let html = format!("{}deep{}", "<div>".repeat(5000), "</div>".repeat(5000));
        assert_eq!(html_to_text(&html), "deep");
    }
}
