//! How a received message can be unsubscribed from: its `List-Unsubscribe`
//! header (RFC 2369), one-click `List-Unsubscribe-Post` (RFC 8058), or an
//! unsubscribe link in the body. Pure: no I/O.
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

const MAX_URL: usize = 8 * 1024;
const MAX_NODES: usize = 50_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unsubscribe {
    /// An https POST of `List-Unsubscribe=One-Click` to `url`, in the background.
    OneClick { url: String },
    /// An email to `address`, with the subject and body the header asks for.
    Mailto {
        address: String,
        subject: String,
        body: String,
    },
    /// A page the person opens in their browser: a header URL without
    /// one-click, or an unsubscribe link in the body.
    Link { url: String },
}

impl Unsubscribe {
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
    from_headers(header, post).or_else(|| {
        let html = crate::html_view::html_part(message);
        html.and_then(html_link)
            .or_else(|| text_link(plain))
            .map(|url| Unsubscribe::Link { url })
    })
}

/// Parse the header pair. URIs are the `<…>` entries of `List-Unsubscribe`.
pub fn from_headers(list_unsubscribe: &str, list_unsubscribe_post: &str) -> Option<Unsubscribe> {
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
    if let Some(mailto) = uris.iter().find_map(mailto) {
        return Some(mailto);
    }
    uris.iter()
        .find(|u| matches!(u.scheme(), "https" | "http") && u.host_str().is_some())
        .map(|u| Unsubscribe::Link { url: u.to_string() })
}

fn mailto(uri: &url::Url) -> Option<Unsubscribe> {
    if uri.scheme() != "mailto" {
        return None;
    }
    let address = percent_decode(uri.path()).trim().to_owned();
    // One plain address; a list or an address with a display name is refused.
    let (local, domain) = address.split_once('@')?;
    if local.is_empty()
        || domain.is_empty()
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
    if html.len() > crate::html_view::MAX_HTML_BYTES {
        return None;
    }
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    let mut found = None;
    let mut nodes = 0;
    anchors(&dom.document, &mut found, &mut nodes);
    found
}

fn anchors(node: &Handle, found: &mut Option<String>, nodes: &mut usize) {
    *nodes += 1;
    if *nodes > MAX_NODES {
        return;
    }
    if let NodeData::Element { name, attrs, .. } = &node.data {
        if name.local.as_ref().eq_ignore_ascii_case("a") {
            let href = attrs
                .borrow()
                .iter()
                .find(|a| a.name.local.as_ref().eq_ignore_ascii_case("href"))
                .map(|a| a.value.to_string());
            if let Some(url) = href.as_deref().and_then(web_link) {
                let mut text = String::new();
                text_of(node, &mut text);
                if names_unsubscribe(&text) || names_unsubscribe(&url) {
                    *found = Some(url);
                }
            }
            return;
        }
    }
    for child in node.children.borrow().iter() {
        anchors(child, found, nodes);
    }
}

fn text_of(node: &Handle, out: &mut String) {
    if out.len() > 512 {
        return;
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
    for child in node.children.borrow().iter() {
        text_of(child, out);
    }
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
    fn one_click_needs_the_post_header_and_an_https_uri() {
        assert_eq!(
            from_headers(
                "<mailto:leave@list.test?subject=stop>, <https://list.test/u/1?t=a>",
                "List-Unsubscribe=One-Click"
            ),
            Some(Unsubscribe::OneClick {
                url: "https://list.test/u/1?t=a".into()
            })
        );
        // Without the POST header, mailto wins over opening a page.
        assert_eq!(
            from_headers(
                "<https://list.test/u/1>, <mailto:leave@list.test?subject=Stop%20it&body=please>",
                ""
            ),
            Some(Unsubscribe::Mailto {
                address: "leave@list.test".into(),
                subject: "Stop it".into(),
                body: "please".into()
            })
        );
        // An http URI is never POSTed to.
        assert_eq!(
            from_headers("<http://list.test/u>", "List-Unsubscribe=One-Click"),
            Some(Unsubscribe::Link {
                url: "http://list.test/u".into()
            })
        );
        assert_eq!(from_headers("", ""), None);
        assert_eq!(from_headers("<javascript:alert(1)>", ""), None);
    }

    #[test]
    fn mailto_defaults_its_subject_and_refuses_lists() {
        assert_eq!(
            from_headers("<mailto:Leave@List.test>", ""),
            Some(Unsubscribe::Mailto {
                address: "leave@list.test".into(),
                subject: "unsubscribe".into(),
                body: String::new()
            })
        );
        assert_eq!(from_headers("<mailto:a@x.test,b@y.test>", ""), None);
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
}
