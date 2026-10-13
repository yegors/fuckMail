use super::*;
use html5ever::tendril::TendrilSink;

/// Every element, attribute and URL reference left in a sanitised document.
fn inventory(html: &str) -> (Vec<String>, Vec<(String, String)>) {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    let mut tags = Vec::new();
    let mut attrs = Vec::new();
    let mut stack = vec![dom.document.clone()];
    while let Some(node) = stack.pop() {
        if let NodeData::Element { name, attrs: a, .. } = &node.data {
            tags.push(name.local.to_string());
            for a in a.borrow().iter() {
                attrs.push((a.name.local.to_string(), a.value.to_string()));
            }
        }
        stack.extend(node.children.borrow().iter().cloned());
    }
    (tags, attrs)
}

fn assert_contained(html: &str) {
    let (tags, attrs) = inventory(html);
    for tag in &tags {
        assert!(
            KEEP.contains(&tag.as_str())
                || matches!(tag.as_str(), "html" | "head" | "body" | "style"),
            "unexpected element {tag} in {html}"
        );
    }
    for (name, value) in &attrs {
        assert!(!name.starts_with("on"), "event handler {name}");
        match name.as_str() {
            "src" => assert!(value.starts_with("fm-res:"), "src {value}"),
            "href" => assert!(value.starts_with("fm-link:"), "href {value}"),
            "style" => {
                let lower = value.to_ascii_lowercase();
                assert!(!lower.contains("expression"), "style {value}");
                for part in lower.split("url(").skip(1) {
                    assert!(part.starts_with("fm-res:"), "style url {value}");
                }
            }
            _ => {}
        }
    }
    let lower = html.to_ascii_lowercase();
    for needle in [
        "javascript:",
        "vbscript:",
        "file:",
        "@import",
        "@font-face",
        "<script",
        "http://",
        "https://",
    ] {
        assert!(!lower.contains(needle), "{needle} survived in {html}");
    }
}

#[test]
fn hostile_markup_is_neutralised() {
    let hostile = r##"<html><head><base href="https://evil.test/"><meta http-equiv="refresh" content="0;url=https://evil.test/">
<style>@import url(https://evil.test/a.css); @font-face{font-family:x;src:url(https://evil.test/f.woff)}
.a{color:red;background:url('https://img.test/bg.png') no-repeat;behavior:url(x.htc)}
.b{width:expression(alert(1))} .c{background-image:url(javascript:alert(1))}
.d{background:u\72l(https://evil.test/x.png)} div > p.e:first-child{margin:0 auto}
p:hover{color:blue} @media (max-width:600px){.a{width:100%!important}} @media print{.a{color:red}}</style></head>
<body onload="alert(1)" bgcolor="#ffffff"><script>alert(1)</script><svg><image href="https://evil.test/s.png"/></svg>
<iframe src="https://evil.test/"></iframe><object data="x"></object><form action="https://evil.test/"><input value="secret">Text in form</form>
<a href="javascript:alert(1)" onclick="alert(1)">js link</a> <a href="vbscript:x">vb</a> <a href="file:///etc/passwd">file</a>
<a href="https://ok.test/path?q=1" target="_blank" rel="opener">ok link</a> <a href="mailto:a@b.test">mail</a>
<img src="https://img.test/photo.jpg" width="600" height="200" alt="Hero" onerror="alert(1)" srcset="https://img.test/2x.jpg 2x">
<img src="http://plain.test/old.gif" alt="Old"><img src="file:///C:/x.png"><img src="cid:logo@x"><img src="data:image/png;base64,iVBORw0KGgo=">
<img src="https://track.test/p.gif" width="1" height="1"><img src="https://track.test/q.gif" style="display: none">
<table background="https://img.test/table-bg.png"><tr><td style="color:#333;position:fixed;font-family:'Helvetica Neue',Arial">Cell</td></tr></table>
<p style="color:red;background:url(&quot;https://img.test/inline.png&quot;);-moz-binding:url(x);width:99999px">Styled</p>
</body></html>"##;
    let view = sanitize(hostile).unwrap();
    assert_contained(&view.html);
    assert!(
        view.html.contains("Text in form"),
        "form text kept: {}",
        view.html
    );
    assert!(!view.html.contains("secret"));
    assert!(view.html.contains("js link") && view.html.contains("ok link"));
    assert_eq!(view.links.len(), 2, "{:?}", view.links);
    assert_eq!(view.links[0].href, "https://ok.test/path?q=1");
    assert_eq!(view.links[0].host, "ok.test");
    assert_eq!(view.links[1].scheme, "mailto");
    assert_eq!(view.trackers, 2);
    let kinds: Vec<_> = view
        .resources
        .iter()
        .map(|r| (r.kind.clone(), r.host.clone()))
        .collect();
    assert!(kinds.contains(&(ResourceKind::Remote, "img.test".into())));
    assert!(kinds.contains(&(ResourceKind::Insecure, "plain.test".into())));
    assert!(kinds.iter().any(|k| k.0 == ResourceKind::Cid));
    assert!(kinds.iter().any(|k| k.0 == ResourceKind::Data));
    let hero = view.resources.iter().find(|r| r.alt == "Hero").unwrap();
    assert_eq!((hero.width, hero.height), (Some(600), Some(200)));
    assert!(
        !view
            .resources
            .iter()
            .any(|r| r.source.contains("evil.test")),
        "{:?}",
        view.resources
    );
    assert!(view
        .resources
        .iter()
        .any(|r| r.source == "https://img.test/table-bg.png"));
    assert!(view
        .resources
        .iter()
        .any(|r| r.source == "https://img.test/inline.png"));
    assert!(view
        .resources
        .iter()
        .any(|r| r.source == "https://img.test/bg.png"));
    assert!(
        view.html
            .contains("@media (max-width:600px){.a{width:100% !important}}"),
        "{}",
        view.html
    );
    assert!(!view.html.contains("print"));
    assert!(!view.html.contains(":hover"));
    assert!(
        view.html.contains("div > p.e:first-child{margin:0 auto}"),
        "{}",
        view.html
    );
    assert!(!view.html.contains("position"));
    assert!(
        view.html.contains("width:10000px"),
        "clamped: {}",
        view.html
    );
    assert!(
        view.html
            .contains("font-family:&quot;Helvetica Neue&quot;,Arial"),
        "{}",
        view.html
    );
}

#[test]
fn quoted_history_is_removed_unless_forwarded() {
    let reply = r##"<div>New text</div><div class="gmail_quote"><div class="gmail_attr">On Mon, A wrote:</div><blockquote>Old text</blockquote></div>"##;
    let view = sanitize(reply).unwrap();
    assert!(view.quote_removed);
    assert!(view.html.contains("New text") && !view.html.contains("Old text"));
    let outlook = r##"<p>Answer</p><div id="divRplyFwdMsg">From: A</div><div>Quoted body</div>"##;
    let view = sanitize(outlook).unwrap();
    assert!(view.html.contains("Answer") && !view.html.contains("Quoted body"));
    let forward = r##"<p>FYI</p>---------- Forwarded message ---------<blockquote type="cite">Original</blockquote>"##;
    let view = sanitize(forward).unwrap();
    assert!(!view.quote_removed && view.html.contains("Original"));
    // A message that is all quote shows the quote rather than nothing.
    let only_quote =
        "<div>\u{a0}\u{200b}<br></div><div class=\"gmail_quote\"><p>Dear A, the details</p></div>";
    let view = sanitize(only_quote).unwrap();
    assert!(!view.quote_removed && view.html.contains("Dear A, the details"));
    // Asked for, the history stays, answers written inside the quote included.
    let inline = r##"<p>Answers below.</p><div class="gmail_quote"><p>1. Families?</p><p style="color:red">Yes.</p><img src="https://img.test/q.png"></div>"##;
    assert!(sanitize(inline).unwrap().quote_removed);
    let view = sanitize_with_quotes(inline).unwrap();
    assert!(!view.quote_removed && view.html.contains("Families?") && view.html.contains("Yes."));
    assert_eq!(view.resources.len(), 1);
    let view = sanitize_with_quotes(outlook).unwrap();
    assert!(view.html.contains("Quoted body"));
}

#[test]
fn nesting_and_size_limits_hold() {
    let deep = format!(
        "{}deep text{}",
        "<div>".repeat(10_000),
        "</div>".repeat(10_000)
    );
    let view = sanitize(&deep).unwrap();
    assert!(view.limited);
    assert!(view.html.contains("deep text"));
    let tables = format!(
        "{}inner{}",
        "<table><tr><td>".repeat(40),
        "</td></tr></table>".repeat(40)
    );
    let view = sanitize(&tables).unwrap();
    assert!(view.limited);
    assert!(view.html.contains("inner"));
    assert!(
        inventory(&view.html)
            .0
            .iter()
            .filter(|t| *t == "table")
            .count()
            <= MAX_TABLE_DEPTH
    );
    let wide = "<p>x</p>".repeat(60_000);
    assert!(sanitize(&wide).unwrap().limited);
    assert!(sanitize(&"a".repeat(MAX_HTML_BYTES + 1)).is_none());
    let bomb = format!("<p>{}</p>", "&amp;".repeat(200_000));
    assert!(sanitize(&bomb).is_some());
}

#[test]
fn attribute_values_are_validated() {
    let view = sanitize(r##"<table width="100%" bgcolor="red" border="2" cellpadding="x"><tr><td colspan="0" align="center" valign="evil" class="a b! c_1" id="main">x</td></tr></table><font face="Arial" size="+2" color="#abc">f</font>"##).unwrap();
    assert!(view.html.contains(r##"width="100%""##));
    assert!(view.html.contains(r##"bgcolor="red""##));
    assert!(
        !view.html.contains("cellpadding")
            && !view.html.contains("colspan")
            && !view.html.contains("valign")
    );
    assert!(view.html.contains(r##"class="a c_1""##), "{}", view.html);
    assert!(
        view.html
            .contains(r##"<font face="Arial" size="+2" color="#abc">"##),
        "{}",
        view.html
    );
}

#[test]
fn text_links_trim_sentence_punctuation() {
    let text = "See https://example.com/a?b=1. Or (http://x.test/y), or https://z.test!";
    let links: Vec<_> = text_links(text)
        .into_iter()
        .map(|(s, e)| &text[s..e])
        .collect();
    assert_eq!(
        links,
        [
            "https://example.com/a?b=1",
            "http://x.test/y",
            "https://z.test"
        ]
    );
    assert!(text_links("just https:// alone").is_empty());
    let hosts = link_hosts(
        Some(r##"<a href="https://www.News.Example.com/x">a</a><a href="mailto:a@b">m</a>"##),
        "and http://www.other.test/page",
    );
    assert_eq!(
        hosts.into_iter().collect::<Vec<_>>(),
        ["news.example.com", "other.test"]
    );
}

#[test]
fn html_outside_the_html_body_is_still_found() {
    // The real part has an unknown transfer encoding, so the parser reads it
    // as text; only the empty part is in the HTML body.
    let raw = "MIME-Version: 1.0\r\nContent-Type: multipart/digest; boundary=b\r\n\r\n--b\r\nContent-Type: text/html\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\n--b\r\nContent-Type: text/html;charset=utf-8\r\nContent-Transfer-Encoding: amazonses\r\n\r\n<center><p>Billing notice</p></center>\r\n--b--\r\n";
    let message = mail_parser::MessageParser::default()
        .parse(raw.as_bytes())
        .unwrap();
    assert!(html_part(&message).unwrap().contains("Billing notice"));
    assert_eq!(
        crate::extract_incoming_text(raw.as_bytes()).unwrap().trim(),
        "Billing notice"
    );
}

#[test]
fn plain_only_mail_has_no_html_part() {
    let plain = b"Content-Type: text/plain\r\n\r\nhello";
    let parsed = mail_parser::MessageParser::default()
        .parse(&plain[..])
        .unwrap();
    assert!(
        !parsed.html_body.is_empty(),
        "mail-parser lists the plain part as HTML"
    );
    assert!(html_part(&parsed).is_none());
    let html = b"Content-Type: text/html\r\n\r\n<p>hello</p>";
    let parsed = mail_parser::MessageParser::default()
        .parse(&html[..])
        .unwrap();
    assert_eq!(html_part(&parsed), Some("<p>hello</p>"));
    let empty_first = b"Content-Type: multipart/digest; boundary=\"b\"\r\n\r\n--b\r\nContent-Type: text/html\r\n\r\n\r\n--b\r\nContent-Type: text/html\r\n\r\n<p>real</p>\r\n--b--\r\n";
    let parsed = mail_parser::MessageParser::default()
        .parse(&empty_first[..])
        .unwrap();
    assert_eq!(html_part(&parsed).map(str::trim), Some("<p>real</p>"));
}

#[test]
fn block_patterns_follow_the_prototype_rules() {
    assert_eq!(
        normalize_block_pattern(" https://www.Tracker.io/path?x "),
        Some("tracker.io".into())
    );
    assert_eq!(
        normalize_block_pattern("*.DoubleClick.net"),
        Some("*.doubleclick.net".into())
    );
    assert_eq!(
        normalize_block_pattern("bücher.example"),
        Some("xn--bcher-kva.example".into())
    );
    for bad in [
        "",
        "localhost",
        "*.",
        "a..b",
        "-a.com",
        "x.1",
        "user@host.com",
        "*.*.com",
        "a b.com",
        "host.com:8080",
    ] {
        assert_eq!(normalize_block_pattern(bad), None, "{bad}");
    }
    let patterns = ["mailtrack.io", "*.doubleclick.net"];
    assert_eq!(
        blocked_by(patterns, "www.mailtrack.io"),
        Some("mailtrack.io")
    );
    assert_eq!(blocked_by(patterns, "sub.mailtrack.io"), None);
    assert_eq!(
        blocked_by(patterns, "doubleclick.net"),
        Some("*.doubleclick.net")
    );
    assert_eq!(
        blocked_by(patterns, "ad.x.doubleclick.net"),
        Some("*.doubleclick.net")
    );
    assert_eq!(blocked_by(patterns, "notdoubleclick.net"), None);
}

#[test]
fn legacy_align_centers_nested_tables_like_browsers() {
    let html = sanitize(
        r#"<table><tr><td align="center"><table id="a"><tr><td><table id="b"><tr><td>x</td></tr></table></td></tr></table>
        <div id="c" style="width:200px">y</div><table id="d" style="margin:0">z</table>
        <table id="e" align="left"><tr><td>e</td></tr></table></td>
        <td align="center" style="text-align:center"><table id="f"><tr><td>f</td></tr></table></td>
        <td align="right"><table id="g"><tr><td>g</td></tr></table></td></tr></table>
        <center><table id="h"><tr><td>h</td></tr></table></center>"#,
    )
    .unwrap()
    .html;
    let tag = |id: &str| {
        let start = html.find(&format!("id=\"{id}\"")).unwrap();
        let open = html[..start].rfind('<').unwrap();
        html[open..open + html[open..].find('>').unwrap()].to_owned()
    };
    for id in ["a", "b", "c", "h"] {
        assert!(
            tag(id).contains("margin-left:auto;margin-right:auto"),
            "{id}: {}",
            tag(id)
        );
    }
    // Own margins or align win; an explicit text-align stops the inherited centering.
    for id in ["d", "e", "f"] {
        assert!(!tag(id).contains("margin-left:auto"), "{id}: {}", tag(id));
    }
    assert!(tag("g").contains("margin-left:auto") && !tag("g").contains("margin-right:auto"));
}

#[test]
fn fixed_layout_tables_share_widthless_columns() {
    let html = sanitize(
        r#"<table style="table-layout:fixed;width:100%"><tr><td class="x">a</td><td class="y" colspan="2">b</td></tr></table>
        <table style="table-layout:fixed;width:100%"><tr><td class="z" width="100">c</td><td class="w">d</td></tr></table>
        <table style="width:100%"><tr><td class="v">e</td></tr></table>"#,
    )
    .unwrap()
    .html;
    assert!(
        html.contains(r#"class="x" style="width:33.333%""#),
        "{html}"
    );
    assert!(
        html.contains(r#"class="y" colspan="2" style="width:66.667%""#),
        "{html}"
    );
    // A first row that sizes itself, or an automatic layout, is left alone.
    for class in ["z", "w", "v"] {
        let start = html.find(&format!("class=\"{class}\"")).unwrap();
        assert!(
            !html[start..start + 40].contains("style"),
            "{class}: {html}"
        );
    }
}
