//! Developer tool: sanitise received HTML files the way `message_html` does.
//!
//! `cargo run -p fm-mail --example sanitize -- IN.html OUT.html [...]` writes
//! each output next to its input pair. An `.eml` input is parsed as a message
//! and its displayed HTML part is used. Keep real mail it reads outside the
//! repository. Pure transform: no network or store access.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.len() % 2 != 0 {
        eprintln!("usage: sanitize IN.html|IN.eml OUT.html [IN OUT ...]");
        std::process::exit(2);
    }
    for pair in args.chunks(2) {
        let bytes = std::fs::read(&pair[0]).expect("read input");
        let html = if pair[0].ends_with(".eml") {
            let parsed = mail_parser::MessageParser::default().parse(&bytes[..]);
            let part = parsed.as_ref().and_then(fm_mail::html_view::html_part);
            match part {
                Some(html) => html.to_owned(),
                None => {
                    println!("{}\tno html part", pair[0]);
                    continue;
                }
            }
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
        let started = std::time::Instant::now();
        match fm_mail::html_view::sanitize(&html) {
            Some(view) => {
                std::fs::write(&pair[1], view.html).expect("write output");
                println!(
                    "{}\t{} bytes\t{:.1} ms\t{} resources",
                    pair[0],
                    html.len(),
                    started.elapsed().as_secs_f64() * 1000.0,
                    view.resources.len()
                );
            }
            None => println!("{}\ttoo large", pair[0]),
        }
    }
}
