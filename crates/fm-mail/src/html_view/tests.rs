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

/// The output stylesheet's text.
fn style(html: &str) -> &str {
    html.split_once("<style>")
        .and_then(|(_, rest)| rest.split_once("</style>"))
        .map_or("", |(css, _)| css)
}

fn nested(depth: usize, inner: &str) -> String {
    format!("{}{inner}{}", "<div>".repeat(depth), "</div>".repeat(depth))
}

#[test]
fn ordinary_selectors_are_kept_unchanged() {
    let css = ".a .b td{color:red} table.x > tr > td a{color:blue} div > p.e:first-child{margin:0} \
        h1, h2 .x{font-weight:bold} ul li + li{margin-top:4px} #main td.b:last-child span{color:green} \
        @media (max-width:600px){.a td{display:block!important}}";
    let body = r#"<div class="a" id="main"><table class="x"><tr><td class="b"><a href="https://example.test/">link</a> <span>s</span></td>
        <td colspan="2" rowspan="3">wide</td></tr><tr><td>c</td></tr></table>
        <div><p class="e">first</p><p>second</p></div><ul><li>1</li><li>2</li></ul><h2><span class="x">h</span></h2></div>"#;
    let view = sanitize(&format!("<style>{css}</style>{body}")).unwrap();
    assert_eq!(
        style(&view.html),
        ".a .b td{color:red}table.x > tr > td a{color:blue}div > p.e:first-child{margin:0}\
         h1, h2 .x{font-weight:bold}ul li + li{margin-top:4px}#main td.b:last-child span{color:green}\
         @media (max-width:600px){.a td{display:block !important}}"
    );
    assert!(view
        .html
        .contains(r#"<td colspan="2" rowspan="3">wide</td>"#));
    assert!(!view.limited);
    // What litehtml would not read as a selector goes, as before.
    for bad in ["a > > b", "> a", "a >", ". a", "a ~", "[x]div"] {
        let view = sanitize(&format!("<style>{bad}{{color:red}}</style><p>x</p>")).unwrap();
        assert_eq!(style(&view.html), "", "{bad}");
    }
    let long = vec!["div"; MAX_SELECTOR_PARTS + 1].join(" > ");
    let view = sanitize(&format!("<style>{long}{{color:red}}</style><p>x</p>")).unwrap();
    assert_eq!(style(&view.html), "");
}

#[test]
fn selector_explosion_is_bounded() {
    // litehtml retries each descendant combinator on every ancestor: without
    // a `q` this one costs C(80, 8) checks per element, for many minutes.
    let rule = format!("q {}{{color:red}}", ["div"; 8].join(" "));
    let view = sanitize(&format!("<style>{rule}</style>{}", nested(80, "deep text"))).unwrap();
    assert_eq!(style(&view.html), "");
    assert!(view.limited && view.html.contains("deep text"));
    // A cheap rule over many leaves, repeated: only what fits the budget stays.
    let rules: String = (0..500).map(|i| format!("q{i} div{{color:red}}")).collect();
    let leaves = "<div>leaf</div>".repeat(5_000);
    let view = sanitize(&format!("<style>{rules}</style>{}", nested(100, &leaves))).unwrap();
    let kept = style(&view.html).matches("{color:red}").count();
    assert!((1..100).contains(&kept), "{kept} rules kept");
    assert!(view.limited);
    assert_eq!(view.html.matches("leaf").count(), 5_000);
    // A general sibling chain over many siblings costs n^3; a simple rule stays.
    let siblings = "<p>s</p>".repeat(2_000);
    let view = sanitize(&format!(
        "<style>q ~ p ~ p{{color:red}} p{{margin:0}}</style>{siblings}"
    ))
    .unwrap();
    assert_eq!(style(&view.html), "p{margin:0}");
    // litehtml scans an element's classes for each of a compound's.
    let classes: Vec<String> = (0..60).map(|i| format!("c{i}")).collect();
    let compound: String = classes[..59].iter().map(|c| format!(".{c}")).collect();
    let rules: String = (0..200)
        .map(|i| format!("div{compound}.z{i}{{width:1px}}"))
        .collect();
    let body = format!(r#"<div class="{}">x</div>"#, classes.join(" ")).repeat(5_000);
    let view = sanitize(&format!("<style>{rules}</style>{body}")).unwrap();
    let kept = style(&view.html).matches("{width:1px}").count();
    assert!(kept < 20 && view.limited, "{kept} rules kept");
    // Markup nested past the depth limit is kept as text.
    let view = sanitize(&nested(300, "bottom")).unwrap();
    assert!(view.limited && view.html.contains("bottom"));
    assert_eq!(view.html.matches("<div>").count(), MAX_DEPTH);
}

#[test]
fn table_grids_are_bounded() {
    // One row of wide cells pads every other row to 10,000 columns with spans.
    let bomb = format!(
        "<table><tr>{}</tr>{}</table>",
        r#"<td colspan="1000">x</td>"#.repeat(100),
        "<tr><td>y</td></tr>".repeat(20)
    );
    let view = sanitize(&bomb).unwrap();
    assert!(view.limited && !view.html.contains("colspan"));
    assert!(view.html.contains("<table>"));
    assert_eq!(view.html.matches(">x<").count(), 100);
    // Each cell looks back through every earlier row: 8,000 rows of one
    // cell is 32 million looks, past the budget; 4,000 is a quarter of that.
    let column = |rows| format!("<table>{}</table>", "<tr><td>r</td></tr>".repeat(rows));
    let view = sanitize(&column(8_000)).unwrap();
    assert!(view.limited && !view.html.contains("<table"));
    assert_eq!(view.html.matches(">r<").count(), 8_000);
    let view = sanitize(&column(4_000)).unwrap();
    assert!(!view.limited && view.html.contains("<table>"));
    // Without spans the same padding needs real cells, so the grid goes.
    let wide = format!(
        "<table><tr>{}</tr>{}</table>",
        "<td>w</td>".repeat(400),
        "<tr></tr>".repeat(400)
    );
    let view = sanitize(&wide).unwrap();
    assert!(view.limited && !view.html.contains("<table"));
    assert_eq!(view.html.matches('w').count(), 400);
    // The budget is shared: 9,000 cells each fit eleven times, then tables
    // lose their spans, then their grids.
    let table = format!(
        r#"<table><tr><td colspan="100">t</td></tr>{}</table>"#,
        "<tr><td>r</td></tr>".repeat(89)
    );
    let view = sanitize(&table.repeat(30)).unwrap();
    assert!(view.limited);
    assert_eq!(view.html.matches(r#"colspan="100""#).count(), 11);
    assert_eq!(view.html.matches("<table>").count(), 11 + 11);
    assert_eq!(view.html.matches(">r<").count(), 30 * 89);
    // Ordinary spans stay; huge ones are clamped; spans belong on cells.
    let view = sanitize(
        r#"<table><colgroup><col span="3"></colgroup><tr><td colspan="5000">a</td></tr>
        <tr><td rowspan="2">b</td><td>c</td></tr><tr><td span="4">d</td></tr></table>
        <div colspan="9" rowspan="9" style="display:table-cell">e</div>"#,
    )
    .unwrap();
    assert!(!view.limited, "{}", view.html);
    assert!(view.html.contains(r#"<col span="3">"#));
    assert!(view.html.contains(r#"<td colspan="100">a</td>"#));
    assert!(view.html.contains(r#"<td rowspan="2">b</td>"#));
    assert!(view.html.contains("<td>d</td>") && !view.html.contains("rowspan=\"9\""));
}

/// The first table of `html`, with the document that owns it.
fn first_table(html: &str) -> (RcDom, Handle) {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    let mut stack = vec![dom.document.clone()];
    while let Some(node) = stack.pop() {
        if element_name(&node) == Some("table") {
            return (dom, node);
        }
        stack.extend(node.children.borrow().iter().cloned());
    }
    unreachable!()
}

#[test]
fn table_grids_follow_litehtml_placement() {
    let unlimited = Grid {
        cells: u64::MAX,
        scans: u64::MAX,
    };
    let costs = |html: &str| {
        let (_dom, table) = first_table(html);
        let cost = |spans| grid(&table, spans, unlimited).map(|g| (g.cells, g.scans));
        (cost(true), cost(false))
    };
    // A rowspan shifts the next row's cell right, after one more look back.
    let spanned = "<table><tr><td rowspan=2>a</td><td>b</td></tr><tr><td>c</td></tr></table>";
    assert_eq!(costs(spanned), (Some((4, 2)), Some((4, 1))));
    let shifted = "<table><tr><td rowspan=2>a</td></tr><tr><td>b</td></tr></table>";
    assert_eq!(costs(shifted), (Some((4, 2)), Some((2, 1))));
    // litehtml blocks only a spanning cell's first column below it.
    let wide =
        "<table><tr><td colspan=3 rowspan=2>a</td></tr><tr><td>b</td><td>c</td></tr></table>";
    assert_eq!(costs(wide), (Some((6, 3)), Some((4, 2))));
    // Rows of every row group count, and empty rows are padded too.
    let groups =
        "<table><thead><tr><th>h</th><th>h</th></tr></thead><tbody><tr></tr></tbody></table>";
    assert_eq!(costs(groups), (Some((4, 0)), Some((4, 0))));
    // Each cell looks back through every earlier row.
    let column = format!("<table>{}</table>", "<tr><td>r</td></tr>".repeat(100));
    assert_eq!(costs(&column).0, Some((100, 99 * 100 / 2)));
    let (_dom, table) = first_table("<table><tr><td colspan=50>a</td></tr><tr></tr></table>");
    let limit = |cells| Grid { cells, scans: 0 };
    assert_eq!(grid(&table, true, limit(99)), None);
    assert_eq!(grid(&table, true, limit(100)), Some(limit(100)));
}

#[test]
fn table_display_is_bounded() {
    // Divs made rows and cells get padded grids too: only so many keep it.
    let cells = r#"<div style="display:table-cell">c</div>"#.repeat(300);
    let view = sanitize(&format!(
        r#"<div style="display:table"><div style="display:table-row">{cells}</div></div>"#
    ))
    .unwrap();
    assert!(view.limited);
    assert_eq!(
        view.html.matches("display:table-cell").count(),
        MAX_TABLE_DISPLAY - 1
    );
    assert!(view.html.contains(r#"style="display:table""#));
    assert_eq!(view.html.matches(">c<").count(), 300);
    // A stylesheet that could reach as many loses those values; a rule for
    // a part's own kind keeps them.
    let css = ".c{display:table-cell;color:red} td{display:table-cell} .few{display:table-row}";
    let body = r#"<div class="c">c</div>"#.repeat(300);
    let view = sanitize(&format!(
        r#"<style>{css}</style>{body}<table><tr><td>t</td></tr></table><div class="few">f</div>"#
    ))
    .unwrap();
    assert!(view.limited);
    assert_eq!(style(&view.html), ".c{color:red}td{display:table-cell}");
    let view = sanitize(&format!(
        r#"<style>{css}</style>{}<div class="few">f</div>"#,
        r#"<div class="c">c</div>"#.repeat(20)
    ))
    .unwrap();
    assert!(!view.limited);
    assert_eq!(style(&view.html), css.replace("; ", ";").replace("} ", "}"));
    // `display:table` is not counted: litehtml wraps each run of other
    // children in one anonymous row group, row and cell, so its grid is at
    // most (2 rows + 1) by (2 cells + 1) of the parts that are counted.
    let parts = r#"<span>s</span><div style="display:table-row"><div style="display:table-cell">c</div></div>"#;
    let view = sanitize(&format!(
        r#"<div style="display:table">{}{}</div>"#,
        parts.repeat(150),
        "<span>t</span>".repeat(5_000)
    ))
    .unwrap();
    assert!(view.limited);
    let counted = view.html.matches("display:table-").count();
    assert_eq!(counted, MAX_TABLE_DISPLAY);
    assert_eq!(view.html.matches(">t<").count(), 5_000);
}

#[test]
fn floats_are_bounded() {
    // litehtml places floats in time superlinear in their number: a
    // stylesheet floating 15,000 words took 21 s to lay out.
    let words = "<b>a</b>".repeat(15_000);
    let view = sanitize(&format!(
        "<style>b{{float:left;color:red}} .few{{float:right}}</style>{words}<div class=few>f</div>"
    ))
    .unwrap();
    assert!(view.limited);
    // As with table parts, every floating rule loses its floats together.
    assert_eq!(style(&view.html), "b{color:red}");
    assert_eq!(view.html.matches(">a<").count(), 15_000);
    // Inline floats and floating `align` count together, in document order.
    let inline = r#"<div style="float:left;color:red">d</div>"#.repeat(150);
    let images = r#"<img src="https://example.test/i.png" width="9" height="9" align="right">"#;
    let tables = r#"<table align="left"><tr><td>t</td></tr></table>"#.repeat(100);
    let view = sanitize(&format!("{inline}{}{tables}", images.repeat(10))).unwrap();
    assert!(view.limited);
    assert_eq!(view.html.matches("float:left").count(), 150);
    assert_eq!(view.html.matches(r#"align="right""#).count(), 10);
    assert_eq!(
        view.html.matches(r#"align="left""#).count(),
        MAX_FLOATS - 160
    );
    assert_eq!(view.html.matches("color:red").count(), 150);
    assert_eq!(view.html.matches(">t<").count(), 100);
    // A stylesheet reaching the floats left over is stripped too.
    let view = sanitize(&format!(
        "<style>.c{{float:left}}</style>{}{}",
        r#"<div style="float:left">d</div>"#.repeat(150),
        r#"<p class="c">c</p>"#.repeat(60)
    ))
    .unwrap();
    assert!(view.limited);
    assert_eq!(style(&view.html), "");
    // A newsletter's columns, `float:none` and centred tables are untouched.
    let css = ".col{float:left;width:50%} .stack{float:none} img{float:right}";
    let body = r#"<div class="col">c</div>"#.repeat(40)
        + &r#"<p class="stack">s</p>"#.repeat(500)
        + &r#"<table align="center"><tr><td>t</td></tr></table>"#.repeat(300);
    let view = sanitize(&format!("<style>{css}</style>{body}")).unwrap();
    assert!(!view.limited);
    assert_eq!(style(&view.html), css.replace("; ", ";").replace("} ", "}"));
    assert_eq!(view.html.matches(r#"align="center""#).count(), 300);
}

#[test]
fn background_sizes_are_bounded() {
    // A tiled background is painted at its size; huge sizes cost gigabytes.
    let view = sanitize(
        r#"<div style="background:url(cid:a@example.test);background-size:10000px 10000px">a</div><p style="font-size:200px;background-size:200em 3rem;background-position:9000px 0">b</p><i style="background:url(cid:a@example.test) 0 0/50000px auto;width:9000px">c</i>"#,
    )
    .unwrap();
    assert!(
        view.html.contains("background-size:4000px 4000px"),
        "{}",
        view.html
    );
    assert!(
        view.html.contains("background-size:20em 3rem"),
        "{}",
        view.html
    );
    assert!(
        view.html.contains("background-position:9000px 0"),
        "{}",
        view.html
    );
    assert!(view.html.contains("0 0/4000px auto"), "{}", view.html);
    assert!(view.html.contains("width:9000px"), "{}", view.html);
}

/// Start and end tags in the order a sanitised document writes them.
fn written_tags(html: &str) -> Vec<String> {
    let mut tags = Vec::new();
    for piece in html.split('<').skip(1) {
        if piece.starts_with('!') {
            continue;
        }
        let name: String = piece
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '/')
            .collect();
        tags.push(name);
    }
    tags
}

/// The same for the tree a parser builds from `html`.
fn parsed_tags(html: &str) -> Vec<String> {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    let mut tags = Vec::new();
    let mut stack = vec![(dom.document.clone(), false)];
    while let Some((node, closing)) = stack.pop() {
        let Some(name) = element_name(&node).map(str::to_owned) else {
            stack.extend(
                node.children
                    .borrow()
                    .iter()
                    .rev()
                    .map(|c| (c.clone(), false)),
            );
            continue;
        };
        if closing {
            tags.push(format!("/{name}"));
            continue;
        }
        tags.push(name.clone());
        if !VOID.contains(&name.as_str()) {
            stack.push((node.clone(), true));
        }
        stack.extend(
            node.children
                .borrow()
                .iter()
                .rev()
                .map(|c| (c.clone(), false)),
        );
    }
    tags
}

#[test]
fn output_parses_back_to_the_same_tree() {
    // Each of these keeps elements apart in the source parse only through a
    // `marquee`, a cell of a flattened table, or quirks mode, all of which the
    // output loses. Written as they were, a parser would close and reopen
    // them, and re-open formatting elements for every later paragraph.
    let deep = |open: &str, close: &str| format!("{}x{}", open.repeat(20), close.repeat(20));
    let flattened = format!(
        "<p><b class=\"k\"><table><tr>{}</tr>{}</table>tail</b></p>",
        "<td><p>x</p><a href=\"https://a.test/\">a<table><tr><td><a href=\"https://b.test/\">b</a></td></tr></table></a></td>"
            .repeat(10),
        "<tr></tr>".repeat(20_000)
    );
    let inputs = [
        deep("<p><marquee>", "</marquee></p>"),
        deep("<li><marquee>", "</marquee></li>"),
        deep("<dl><dd><marquee><dt>", "</dt></marquee></dd></dl>"),
        deep(r#"<a href="https://a.test/"><marquee>"#, "</marquee></a>"),
        deep("<h1><marquee>", "</marquee></h1>"),
        format!(
            "<p>{}<marquee>{}</marquee></p>",
            r#"<b class="a"><i class="b"><u class="c">"#,
            "<p>para</p>".repeat(50)
        ),
        "<p>a<table><tr><td>b<p>c</p></td></tr></table>d</p>".to_owned(),
        flattened,
        "<div></p><p>a</p></p><li>b<li>c</div>".to_owned(),
        format!("{}<table><tr><td>t</td></tr></table>", "<div>".repeat(126)),
    ];
    for input in &inputs {
        // Without a doctype the source is read in quirks mode, where a table
        // may sit in a paragraph; the output always has one.
        let view = sanitize(input).unwrap();
        assert_eq!(
            written_tags(&view.html),
            parsed_tags(&view.html),
            "{}",
            view.html
        );
        assert!(!view.html.contains("<marquee"));
    }
    // The paragraphs that held blocks are divs; the innermost stays one.
    let view = sanitize(&inputs[0]).unwrap();
    assert_eq!(view.html.matches("<p>").count(), 1);
    assert_eq!(view.html.matches("<div>").count(), 19);
    // The reviewer's case: a sibling rule over 40 such chains. Reparsed as
    // written before, they were 5,000 sibling paragraphs.
    let chains = deep("<p><marquee>", "</marquee></p>").repeat(40);
    let view = sanitize(&format!("<style>q ~ p ~ p{{color:red}}</style>{chains}")).unwrap();
    assert_eq!(written_tags(&view.html), parsed_tags(&view.html));
    assert!(parsed_tags(&view.html).iter().filter(|t| *t == "p").count() <= 40);
}

#[test]
fn restructured_output_is_estimated_as_parsed() {
    let dom = html5ever::parse_document(RcDom::default(), Default::default())
        .one("<style>.k{color:red}</style><div><span><i>x</i></span></div>");
    // Markup a parser reads as fewer elements than were recorded: kept, and
    // the stylesheet is estimated on what the parser builds.
    let mut walk = walk_document(&dom, false);
    walk.body = r#"<p class="k">x</p>"#.to_owned();
    walk.settle_styles();
    assert!(walk.view.limited);
    assert_eq!(walk.body, r#"<p class="k">x</p>"#);
    assert_eq!(walk.css, ".k{color:red}");
    // Markup a parser grows, here by re-opening formatting elements for
    // every paragraph: only the text is kept.
    let mut walk = walk_document(&dom, false);
    walk.body = format!(
        "<p>{}<p>a</p><p>b</p>{}</p>",
        r#"<b class="k"><i class="k">"#, "</i></b>"
    );
    walk.settle_styles();
    assert!(walk.view.limited);
    assert_eq!(walk.body, "ab");
    assert_eq!(walk.css, "");
}
