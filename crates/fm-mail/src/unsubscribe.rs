//! How a received message can be unsubscribed from: its `List-Unsubscribe`
//! header (RFC 2369), one-click `List-Unsubscribe-Post` (RFC 8058), or an
//! unsubscribe link in the body. Pure: no I/O.
use crate::sender_check::SenderCheck;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

const MAX_URL: usize = 8 * 1024;
const MAX_NODES: usize = 50_000;

/// One way to leave a mailing list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unsubscribe {
    /// An https POST of `List-Unsubscribe=One-Click` to `url`, in the background.
    OneClick { url: String },
    /// An email to `address`, with the subject and body the header asks for.
    /// Offered only when Gmail verified the sender and the address is theirs
    /// (see [`SenderCheck::owns_address`]), so a sender cannot have the
    /// person write to someone else.
    Mailto {
        /// One plain address, lowercased.
        address: String,
        /// One line of at most 200 characters; `unsubscribe` when the URI has none.
        subject: String,
        /// At most 2,000 characters; often empty.
        body: String,
    },
    /// A page the person opens in their browser: a header URL without
    /// one-click, or an unsubscribe link in the body.
    Link { url: String },
}

impl Unsubscribe {
    /// Stable snake_case name of the variant: `one_click`, `mailto` or `link`.
    pub fn method(&self) -> &'static str {
        match self {
            Unsubscribe::OneClick { .. } => "one_click",
            Unsubscribe::Mailto { .. } => "mailto",
            Unsubscribe::Link { .. } => "link",
        }
    }
}

/// The best way to unsubscribe, preferring one-click, then mailto, then a
/// header link, then a body link.
pub fn unsubscribe(message: &mail_parser::Message<'_>, plain: &str) -> Option<Unsubscribe> {
    let header = message.header_raw("List-Unsubscribe").unwrap_or("");
    let post = message.header_raw("List-Unsubscribe-Post").unwrap_or("");
    let check = crate::sender_check::check(message);
    from_headers(header, post, &check).or_else(|| {
        let html = crate::html_view::html_part(message);
        html.and_then(html_link)
            .or_else(|| text_link(plain))
            .map(|url| Unsubscribe::Link { url })
    })
}

/// The `mailto:` entry of `List-Unsubscribe`, even when one-click is offered:
/// the way out when a list refuses its own one-click POST.
pub fn mailto_alternative(message: &mail_parser::Message<'_>) -> Option<Unsubscribe> {
    let check = crate::sender_check::check(message);
    message
        .header_raw("List-Unsubscribe")
        .unwrap_or("")
        .split('<')
        .skip(1)
        .filter_map(|rest| rest.split('>').next())
        .map(|uri| uri.split_whitespace().collect::<String>())
        .filter(|uri| !uri.is_empty() && uri.len() <= MAX_URL)
        .filter_map(|uri| url::Url::parse(&uri).ok())
        .find_map(|uri| mailto(&uri, &check))
}

/// Parse the header pair. URIs are the `<…>` entries of `List-Unsubscribe`;
/// a `mailto:` one counts only for an address `check` says the sender owns.
pub fn from_headers(
    list_unsubscribe: &str,
    list_unsubscribe_post: &str,
    check: &SenderCheck,
) -> Option<Unsubscribe> {
    let uris: Vec<url::Url> = list_unsubscribe
        .split('<')
        .skip(1)
        .filter_map(|rest| rest.split('>').next())
        .map(|uri| uri.split_whitespace().collect::<String>())
        .filter(|uri| !uri.is_empty() && uri.len() <= MAX_URL)
        .filter_map(|uri| url::Url::parse(&uri).ok())
        .collect();
    let https = uris
        .iter()
        .find(|u| u.scheme() == "https" && u.host_str().is_some());
    let one_click = list_unsubscribe_post
        .split_whitespace()
        .collect::<String>()
        .eq_ignore_ascii_case("List-Unsubscribe=One-Click");
    if let (true, Some(url)) = (one_click, https) {
        return Some(Unsubscribe::OneClick {
            url: url.to_string(),
        });
    }
    if let Some(mailto) = uris.iter().find_map(|uri| mailto(uri, check)) {
        return Some(mailto);
    }
    uris.iter()
        .find(|u| matches!(u.scheme(), "https" | "http") && u.host_str().is_some())
        .map(|u| Unsubscribe::Link { url: u.to_string() })
}

fn mailto(uri: &url::Url, check: &SenderCheck) -> Option<Unsubscribe> {
    if uri.scheme() != "mailto" {
        return None;
    }
    let address = percent_decode(uri.path()).trim().to_owned();
    // One plain address; a list or an address with a display name is refused.
    let (local, domain) = address.split_once('@')?;
    if local.is_empty()
        || domain.is_empty()
        || !check.owns_address(&address)
        || address.len() > 320
        || address
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, ',' | '<' | '>' | '"'))
    {
        return None;
    }
    let (mut subject, mut body) = (String::new(), String::new());
    for (key, value) in uri.query_pairs() {
        let value: String = value
            .chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .collect();
        match key.to_ascii_lowercase().as_str() {
            "subject" => subject = value.replace('\n', " ").chars().take(200).collect(),
            "body" => body = value.chars().take(2000).collect(),
            _ => {}
        }
    }
    if subject.trim().is_empty() {
        subject = "unsubscribe".into();
    }
    Some(Unsubscribe::Mailto {
        address: address.to_ascii_lowercase(),
        subject,
        body,
    })
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn names_unsubscribe(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("unsubscribe") || lower.contains("opt out") || lower.contains("opt-out")
}

fn web_link(href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty() || href.len() > MAX_URL {
        return None;
    }
    let url = url::Url::parse(href).ok()?;
    (matches!(url.scheme(), "https" | "http") && url.host_str().is_some()).then(|| url.to_string())
}

/// The last `<a href>` whose text or address names unsubscribing; footers put it last.
fn html_link(html: &str) -> Option<String> {
    if html.len() > crate::html_view::MAX_HTML_BYTES || !crate::html_cost::parses_cheaply(html) {
        return None;
    }
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    link_in(&dom.document)
}

/// [`html_link`] over a parsed document, in document order and without
/// recursion, so deep nesting cannot exhaust the stack.
fn link_in(document: &Handle) -> Option<String> {
    let mut found = None;
    let mut nodes = 0;
    let mut stack = vec![document.clone()];
    while let Some(node) = stack.pop() {
        nodes += 1;
        if nodes > MAX_NODES {
            break;
        }
        if let NodeData::Element { name, attrs, .. } = &node.data {
            if name.local.as_ref().eq_ignore_ascii_case("a") {
                let href = attrs
                    .borrow()
                    .iter()
                    .find(|a| a.name.local.as_ref().eq_ignore_ascii_case("href"))
                    .map(|a| a.value.to_string());
                if let Some(url) = href.as_deref().and_then(web_link) {
                    let text = text_of(&node, &mut nodes);
                    if names_unsubscribe(&text) || names_unsubscribe(&url) {
                        found = Some(url);
                    }
                }
                continue;
            }
        }
        stack.extend(node.children.borrow().iter().rev().cloned());
    }
    found
}

/// An anchor's label, its text and image alt text in order, to about 512
/// bytes. Visited nodes count towards `nodes`.
fn text_of(anchor: &Handle, nodes: &mut usize) -> String {
    let mut out = String::new();
    let mut stack = vec![anchor.clone()];
    while let Some(node) = stack.pop() {
        *nodes += 1;
        if out.len() > 512 || *nodes > MAX_NODES {
            break;
        }
        match &node.data {
            NodeData::Text { contents } => out.push_str(&contents.borrow()),
            NodeData::Element { attrs, .. } => {
                // An image button's alt text counts as its label.
                if let Some(alt) = attrs
                    .borrow()
                    .iter()
                    .find(|a| a.name.local.as_ref().eq_ignore_ascii_case("alt"))
                {
                    out.push_str(&alt.value);
                }
            }
            _ => {}
        }
        stack.extend(node.children.borrow().iter().rev().cloned());
    }
    out
}

/// A plain-text URL on a line that names unsubscribing, or that names it itself.
fn text_link(plain: &str) -> Option<String> {
    let mut found = None;
    let mut previous = "";
    for line in plain.lines() {
        for (start, end) in crate::html_view::text_links(line) {
            let url = &line[start..end];
            if names_unsubscribe(line) || names_unsubscribe(url) || names_unsubscribe(previous) {
                if let Some(url) = web_link(url) {
                    found = Some(url);
                }
            }
        }
        if !line.trim().is_empty() {
            previous = line;
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gmail verified news@list.test as the sender.
    fn vouched() -> SenderCheck {
        SenderCheck {
            checked: true,
            verified: true,
            from_address: "news@list.test".into(),
            from_domain: "list.test".into(),
            ..SenderCheck::default()
        }
    }

    fn parse(raw: &str) -> Option<Unsubscribe> {
        let message = mail_parser::MessageParser::default()
            .parse(raw.as_bytes())
            .unwrap();
        let plain = message
            .body_text(0)
            .map(|t| t.into_owned())
            .unwrap_or_default();
        unsubscribe(&message, &plain)
    }

    #[test]
    fn a_one_click_list_can_still_name_its_unsubscribe_address() {
        let raw = "Authentication-Results: mx.google.com; dkim=pass header.d=list.test; dmarc=pass header.from=list.test
From: News <news@list.test>
List-Unsubscribe:
 <mailto:unsubscribe+abc@bounces.list.test?subject=unsubscribe>,
 <https://tracking.list.test/unsubscribe?msgid=abc&c=1>
List-Unsubscribe-Post: List-Unsubscribe=One-Click
Content-Type: text/plain

Hello
";
        assert_eq!(
            parse(raw),
            Some(Unsubscribe::OneClick {
                url: "https://tracking.list.test/unsubscribe?msgid=abc&c=1".into()
            })
        );
        let message = mail_parser::MessageParser::default()
            .parse(raw.as_bytes())
            .unwrap();
        assert_eq!(
            mailto_alternative(&message),
            Some(Unsubscribe::Mailto {
                address: "unsubscribe+abc@bounces.list.test".into(),
                subject: "unsubscribe".into(),
                body: String::new(),
            })
        );
    }

    #[test]
    fn one_click_needs_the_post_header_and_an_https_uri() {
        assert_eq!(
            from_headers(
                "<mailto:leave@list.test?subject=stop>, <https://list.test/u/1?t=a>",
                "List-Unsubscribe=One-Click",
                &vouched()
            ),
            Some(Unsubscribe::OneClick {
                url: "https://list.test/u/1?t=a".into()
            })
        );
        // Without the POST header, mailto wins over opening a page.
        assert_eq!(
            from_headers(
                "<https://list.test/u/1>, <mailto:leave@list.test?subject=Stop%20it&body=please>",
                "",
                &vouched()
            ),
            Some(Unsubscribe::Mailto {
                address: "leave@list.test".into(),
                subject: "Stop it".into(),
                body: "please".into()
            })
        );
        // An http URI is never POSTed to.
        assert_eq!(
            from_headers(
                "<http://list.test/u>",
                "List-Unsubscribe=One-Click",
                &vouched()
            ),
            Some(Unsubscribe::Link {
                url: "http://list.test/u".into()
            })
        );
        assert_eq!(from_headers("", "", &vouched()), None);
        assert_eq!(from_headers("<javascript:alert(1)>", "", &vouched()), None);
    }

    #[test]
    fn mailto_defaults_its_subject_and_refuses_lists() {
        assert_eq!(
            from_headers("<mailto:Leave@List.test>", "", &vouched()),
            Some(Unsubscribe::Mailto {
                address: "leave@list.test".into(),
                subject: "unsubscribe".into(),
                body: String::new()
            })
        );
        assert_eq!(
            from_headers("<mailto:a@x.test,b@y.test>", "", &vouched()),
            None
        );
    }

    #[test]
    fn folded_headers_and_body_links_are_found() {
        let raw = "From: News <news@shop.test>\r\nList-Unsubscribe: <https://shop.test/\r\n u?id=9>\r\nList-Unsubscribe-Post: List-Unsubscribe=One-Click\r\nSubject: Sale\r\n\r\nBuy things";
        assert_eq!(
            parse(raw),
            Some(Unsubscribe::OneClick {
                url: "https://shop.test/u?id=9".into()
            })
        );
        let html = "From: News <news@shop.test>\r\nSubject: Sale\r\nContent-Type: text/html\r\n\r\n<p><a href=\"https://shop.test/p\">Products</a></p><p>Don't want these? <a href=\"https://shop.test/prefs?x=1\">Unsubscribe here</a></p>";
        assert_eq!(
            parse(html),
            Some(Unsubscribe::Link {
                url: "https://shop.test/prefs?x=1".into()
            })
        );
        let plain = "From: News <news@shop.test>\r\nSubject: Sale\r\n\r\nHello.\r\nTo unsubscribe, visit:\r\nhttps://shop.test/stop?u=2.\r\n";
        assert_eq!(
            parse(plain),
            Some(Unsubscribe::Link {
                url: "https://shop.test/stop?u=2".into()
            })
        );
        let none = "From: Friend <friend@home.test>\r\nSubject: Hi\r\n\r\nSee https://home.test/photos\r\n";
        assert_eq!(parse(none), None);
    }

    #[test]
    fn mailto_goes_only_to_a_domain_gmail_authenticated() {
        // A sender cannot have the person email someone else: the mailto
        // entry is skipped and the page link is offered instead.
        assert_eq!(
            from_headers(
                "<mailto:boss@corp.test?subject=Wire%20it&body=Send%20the%20funds>, <https://list.test/u>",
                "",
                &vouched()
            ),
            Some(Unsubscribe::Link {
                url: "https://list.test/u".into()
            })
        );
        // Spoofing the target's own domain fails DMARC, so it vouches for nothing.
        let spoofed = "Authentication-Results: mx.google.com; spf=fail smtp.mailfrom=x@relay.test; dmarc=fail header.from=corp.test\r\nFrom: CEO <ceo@corp.test>\r\nList-Unsubscribe: <mailto:finance@corp.test?subject=Invoice>\r\n\r\nHi\r\n";
        assert_eq!(parse(spoofed), None);
        let message = mail_parser::MessageParser::default()
            .parse(spoofed.as_bytes())
            .unwrap();
        assert_eq!(mailto_alternative(&message), None);
        // Mail without Gmail's verdicts offers no mailto either.
        assert_eq!(
            parse("From: List <a@list.test>\r\nList-Unsubscribe: <mailto:leave@list.test>\r\n\r\nHi\r\n"),
            None
        );
    }

    #[test]
    fn mailto_goes_only_to_the_verified_senders_own_domain() {
        let from = |from: &str, auth: &str, target: &str| {
            parse(&format!(
                "Authentication-Results: mx.google.com; {auth}\r\nFrom: {from}\r\nList-Unsubscribe: <mailto:{target}?subject=Hi&body=Wire%20the%20funds>\r\n\r\nHi\r\n"
            ))
        };
        let gmail = "dkim=pass header.d=gmail.com; spf=pass smtp.mailfrom=attacker@gmail.com; dmarc=pass header.from=gmail.com";
        // Any Gmail user is authenticated as gmail.com: that does not make
        // another gmail.com mailbox theirs.
        assert_eq!(
            from("<attacker@gmail.com>", gmail, "victim@gmail.com"),
            None
        );
        assert_eq!(
            from("<attacker@gmail.com>", gmail, "attacker@gmail.com").map(|u| u.method()),
            Some("mailto")
        );
        let shop = "dkim=pass header.d=shop.example; spf=pass smtp.mailfrom=bounce@esp.example; dmarc=pass header.from=shop.example";
        // The ESP that sent it and a parent domain are someone else.
        assert_eq!(from("<news@shop.example>", shop, "boss@esp.example"), None);
        assert_eq!(
            from("<news@mail.shop.example>", shop, "boss@shop.example"),
            None
        );
        assert_eq!(
            from("<news@shop.example>", shop, "leave@lists.shop.example").map(|u| u.method()),
            Some("mailto")
        );
        // An SPF pass alone does not verify the From domain.
        assert_eq!(
            from(
                "<news@shop.example>",
                "spf=pass smtp.mailfrom=bounce@shop.example",
                "leave@shop.example"
            ),
            None
        );
    }

    /// Runs `f` on a thread with a 2 MiB stack, as tokio's blocking pool does.
    fn on_small_stack(f: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn deep_nesting_cannot_exhaust_the_stack() {
        on_small_stack(|| {
            let parse_html = |html: &str| {
                html5ever::parse_document(RcDom::default(), Default::default()).one(html)
            };
            // The walk itself, over trees far too deep to recurse through.
            let deep = parse_html(&format!("{}x", "<span>".repeat(20_000)));
            assert_eq!(link_in(&deep.document), None);
            // The cell's marker keeps parsing the spans inside the link linear.
            let label = format!(
                "<a href=\"https://e.test/u\"><table><td>{}Unsubscribe</a>",
                "<span>".repeat(100_000)
            );
            assert_eq!(link_in(&parse_html(&label).document), None);
            // A whole HTML-only message without List-Unsubscribe.
            let raw = format!(
                "From: News <news@shop.test>\r\nSubject: Sale\r\nContent-Type: text/html\r\n\r\n{}x",
                "<span>".repeat(20_000)
            );
            assert_eq!(parse(&raw), None);
        });
    }
}
