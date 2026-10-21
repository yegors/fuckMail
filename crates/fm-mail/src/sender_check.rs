//! What Gmail's own checks say about who sent a received message, and the
//! warning signs a reader should see. Gmail records SPF, DKIM and DMARC
//! verdicts in the `Authentication-Results` header it adds on arrival; only
//! the topmost one, from `mx.google.com`, is trusted, because a sender can
//! write their own further down. Values inside it such as the envelope
//! address are sender-chosen, so quoted strings never split a verdict and a
//! method reported both ways counts as failed. Pure transform: no I/O.
use mail_parser::Message;

/// The authentication service whose verdicts are trusted.
const GMAIL: &str = "mx.google.com";

/// One reason to be careful with a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// Stable snake_case code: `dmarc_fail`, `unauthenticated`,
    /// `name_address` or `lookalike_domain`.
    pub code: &'static str,
    /// One plain sentence for the reader.
    pub text: String,
}

/// Gmail's verdicts on a message's sender, and the warnings they lead to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SenderCheck {
    /// Gmail recorded verdicts on this message. Without them nothing is
    /// verified and no verdict warning is given.
    pub checked: bool,
    /// The From domain passed DMARC, or a passing DKIM signature belongs to
    /// it and DMARC did not fail.
    pub verified: bool,
    /// Lowercase From address; empty when there is none.
    pub from_address: String,
    /// Lowercase domain of the From address; empty when there is none.
    pub from_domain: String,
    /// Domains Gmail authenticated: passing DKIM signers, the envelope
    /// sender's domain when SPF passed, and the From domain when DMARC passed.
    pub authenticated: Vec<String>,
    /// SPF verdict (`pass`, `fail`, `softfail`, `neutral`, `none`, …), empty when absent.
    pub spf: String,
    /// `pass` when any DKIM signature passed, else the first verdict, else empty.
    pub dkim: String,
    /// DMARC verdict, empty when absent (the domain may publish no policy).
    pub dmarc: String,
    pub warnings: Vec<Warning>,
}

impl SenderCheck {
    /// Whether mail to `address` stays with the verified sender, so the
    /// reader may write to it for them: the From domain is verified, and
    /// `address` is on that domain or one of its subdomains. At a shared
    /// mailbox provider, where anyone can hold an address, only the From
    /// address itself qualifies.
    pub fn owns_address(&self, address: &str) -> bool {
        let address = address.trim().to_ascii_lowercase();
        let Some((local, domain)) = address.rsplit_once('@') else {
            return false;
        };
        let domain = domain.trim_end_matches('.');
        if !self.verified || local.is_empty() || self.from_domain.is_empty() {
            return false;
        }
        if shared_provider(&self.from_domain) {
            return address == self.from_address;
        }
        domain == self.from_domain
            || domain
                .strip_suffix(self.from_domain.as_str())
                .is_some_and(|sub| sub.len() > 1 && sub.ends_with('.'))
    }
}

/// Mailbox providers whose addresses belong to anyone who signs up, matched
/// by name under any country suffix, or exactly.
const SHARED_NAMES: [&str; 9] = [
    "gmail",
    "googlemail",
    "outlook",
    "hotmail",
    "live",
    "yahoo",
    "ymail",
    "gmx",
    "yandex",
];
const SHARED_DOMAINS: [&str; 10] = [
    "msn.com",
    "icloud.com",
    "me.com",
    "mac.com",
    "aol.com",
    "proton.me",
    "protonmail.com",
    "pm.me",
    "mail.ru",
    "zoho.com",
];

fn shared_provider(domain: &str) -> bool {
    if SHARED_DOMAINS.contains(&domain) {
        return true;
    }
    // `gmail.com`, `hotmail.co.uk`, `gmx.net`: the name and a one- or
    // two-label suffix.
    domain.split_once('.').is_some_and(|(name, suffix)| {
        SHARED_NAMES.contains(&name) && suffix.split('.').count() <= 2
    })
}

/// Reads Gmail's verdicts and the From header of `message`.
pub fn check(message: &Message<'_>) -> SenderCheck {
    let from = message.from().and_then(|from| from.first());
    let address = from
        .and_then(|from| from.address())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let name = from.and_then(|from| from.name()).unwrap_or("");
    let from_domain = address
        .rsplit_once('@')
        .map(|(_, domain)| domain.trim_end_matches('.').to_owned())
        .unwrap_or_default();
    let mut check = SenderCheck {
        from_address: address.clone(),
        from_domain,
        ..SenderCheck::default()
    };
    // header_raw() answers with the last copy of a header, which is the one a
    // sender can write; Gmail's is the first.
    let topmost = message
        .headers()
        .iter()
        .find(|header| {
            header
                .name
                .as_str()
                .eq_ignore_ascii_case("Authentication-Results")
        })
        .and_then(|header| {
            message
                .raw_message()
                .get(header.offset_start as usize..header.offset_end as usize)
        })
        .and_then(|raw| std::str::from_utf8(raw).ok());
    if let Some(verdicts) = topmost.and_then(gmail_verdicts) {
        check.checked = true;
        apply(&mut check, &verdicts);
    }
    warn(&mut check, name, &address);
    check
}

/// One `method=result` entry with its properties.
#[derive(Debug, Default)]
struct Verdict {
    method: String,
    result: String,
    domain: String,
}

/// The verdicts of an `Authentication-Results` value written by Gmail, or
/// `None` when another service wrote it. A value whose quotes or comments
/// never close cannot be split safely, so it yields no verdicts at all.
fn gmail_verdicts(raw: &str) -> Option<Vec<Verdict>> {
    let service = raw
        .split(|c: char| c == ';' || c == '(' || c.is_whitespace())
        .find(|token| !token.is_empty())?;
    if !service.eq_ignore_ascii_case(GMAIL) {
        return None;
    }
    let Some(text) = without_comments(raw) else {
        return Some(Vec::new());
    };
    let parts = split_unquoted(&text, |c| c == ';');
    Some(
        parts
            .iter()
            .skip(1)
            .filter_map(|part| verdict(part))
            .collect(),
    )
}

/// Splits `text` where `at` matches outside a quoted string (RFC 8601 values
/// may be quoted, and a quoted envelope address may hold `;` or spaces).
/// Empty pieces are dropped.
fn split_unquoted(text: &str, at: impl Fn(char) -> bool) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut piece = String::new();
    let (mut quoted, mut escaped) = (false, false);
    for c in text.chars() {
        if escaped {
            escaped = false;
        } else if quoted && c == '\\' {
            escaped = true;
        } else if c == '"' {
            quoted = !quoted;
        } else if !quoted && at(c) {
            if !piece.trim().is_empty() {
                pieces.push(std::mem::take(&mut piece));
            }
            piece.clear();
            continue;
        }
        piece.push(c);
    }
    if !piece.trim().is_empty() {
        pieces.push(piece);
    }
    pieces
}

fn verdict(part: &str) -> Option<Verdict> {
    let tokens = split_unquoted(part, char::is_whitespace);
    let mut tokens = tokens.iter();
    let (method, result) = tokens.next()?.split_once('=')?;
    // A method may carry a version, as in `dkim/1`.
    let method = method.split('/').next().unwrap_or("").to_ascii_lowercase();
    let mut verdict = Verdict {
        method,
        result: result.trim_matches('"').to_ascii_lowercase(),
        domain: String::new(),
    };
    for token in tokens {
        let Some((key, value)) = token.split_once('=') else {
            continue;
        };
        let value = value.trim_matches('"');
        let domain = value.rsplit_once('@').map_or(value, |(_, domain)| domain);
        // The signing domain beats the identity, the envelope sender the HELO name.
        let wanted = match (verdict.method.as_str(), key) {
            ("dkim", "header.d") | ("spf", "smtp.mailfrom") | ("dmarc", "header.from") => true,
            ("dkim", "header.i") | ("spf", "smtp.helo") => verdict.domain.is_empty(),
            _ => false,
        };
        if wanted {
            verdict.domain = domain.trim_end_matches('.').to_ascii_lowercase();
        }
    }
    Some(verdict)
}

/// Drops parenthesised comments, which may hold sender-chosen text such as
/// an envelope address, while keeping quoted strings as written. `None` when
/// a quoted string or comment is left open.
fn without_comments(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    for c in raw.chars() {
        if escaped {
            escaped = false;
            if depth == 0 {
                out.push(c);
            }
            continue;
        }
        match c {
            '\\' => {
                escaped = true;
                if depth == 0 {
                    out.push(c);
                }
            }
            '"' if depth == 0 => {
                quoted = !quoted;
                out.push(c);
            }
            '(' if !quoted => depth += 1,
            ')' if !quoted && depth > 0 => depth -= 1,
            '\r' | '\n' | '\t' if depth == 0 => out.push(' '),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    (depth == 0 && !quoted && !escaped).then_some(out)
}

/// Folds the verdicts into `check`, failing closed: a method reported more
/// than once with different results (DKIM: for the same signing domain)
/// takes its first non-passing result, and a pass names no domain unless it
/// came with one. A DMARC pass counts only for the From domain.
fn apply(check: &mut SenderCheck, verdicts: &[Verdict]) {
    let of = |method: &'static str| verdicts.iter().filter(move |v| v.method == method);

    check.spf = combined(of("spf"));
    if check.spf == "pass" {
        for verdict in of("spf").filter(|v| !v.domain.is_empty()) {
            check.authenticated.push(verdict.domain.clone());
        }
    }

    let mut dkim_aligned = false;
    for verdict in of("dkim").filter(|v| v.result == "pass" && !v.domain.is_empty()) {
        let domain = &verdict.domain;
        if combined(of("dkim").filter(|v| &v.domain == domain)) == "pass" {
            dkim_aligned |= aligned(domain, &check.from_domain);
            check.dkim = "pass".into();
            check.authenticated.push(domain.clone());
        }
    }
    if check.dkim.is_empty() {
        check.dkim = of("dkim")
            .find(|v| v.result != "pass")
            .map(|v| v.result.clone())
            .unwrap_or_default();
    }

    // Gmail names the From domain in its DMARC verdict; a pass for any other
    // domain, or for none, is not about this sender.
    let from = check.from_domain.clone();
    check.dmarc = combined(
        of("dmarc").filter(|v| v.result != "pass" || (!from.is_empty() && v.domain == from)),
    );
    if check.dmarc == "pass" {
        check.authenticated.push(from);
    }
    check.verified = check.dmarc == "pass" || (dkim_aligned && check.dmarc != "fail");
    check.authenticated.sort();
    check.authenticated.dedup();
}

/// One result for several reports of a method: the first that is not a
/// pass, else `pass`, else empty.
fn combined<'a>(verdicts: impl Iterator<Item = &'a Verdict>) -> String {
    let mut result = String::new();
    for verdict in verdicts {
        if verdict.result != "pass" {
            return verdict.result.clone();
        }
        result = verdict.result.clone();
    }
    result
}

fn warn(check: &mut SenderCheck, name: &str, address: &str) {
    let domain = if check.from_domain.is_empty() {
        "the sender's domain".to_owned()
    } else {
        check.from_domain.clone()
    };
    if check.checked {
        if check.dmarc == "fail" {
            check.warnings.push(Warning {
                code: "dmarc_fail",
                text: format!("Gmail could not confirm this came from {domain}: it failed DMARC."),
            });
        } else if check.spf != "pass" && check.dkim != "pass" {
            check.warnings.push(Warning {
                code: "unauthenticated",
                text: "Gmail could not verify the sender: neither SPF nor DKIM passed.".into(),
            });
        }
    }
    if let Some(shown) = address_in_name(name) {
        if shown != address {
            check.warnings.push(Warning {
                code: "name_address",
                text: format!(
                    "The sender's name shows {shown}, but the message is from {address}."
                ),
            });
        }
    }
    if !check.from_domain.is_ascii()
        || check
            .from_domain
            .split('.')
            .any(|label| label.starts_with("xn--"))
    {
        check.warnings.push(Warning {
            code: "lookalike_domain",
            text: format!(
                "The sender's domain {} uses characters that can imitate another domain.",
                check.from_domain
            ),
        });
    }
}

/// The first email address written inside a display name, lowercased.
fn address_in_name(name: &str) -> Option<String> {
    name.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '<' | '>' | '(' | ')' | '"' | '\'' | ',' | ';' | '[' | ']'
            )
    })
    .map(|word| word.trim_matches(|c: char| matches!(c, '.' | ':')))
    .find(|word| {
        word.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.starts_with('.')
        })
    })
    .map(str::to_ascii_lowercase)
}

/// Whether two domains are the same, or one is a subdomain of the other. The
/// shorter must have at least two labels, so a bare suffix never matches.
fn aligned(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let (short, long) = if a.len() < b.len() { (a, b) } else { (b, a) };
    short.contains('.') && long.ends_with(short) && long[..long.len() - short.len()].ends_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checked(raw: &str) -> SenderCheck {
        let message = mail_parser::MessageParser::default()
            .parse(raw.as_bytes())
            .unwrap();
        check(&message)
    }

    #[test]
    fn gmail_verdicts_verify_an_aligned_sender() {
        let result = checked(
            "Authentication-Results: mx.google.com;\r
       dkim=pass header.i=@news.shop.example header.s=s1 header.b=abc;\r
       spf=pass (google.com: domain of bounce@mail.esp.example designates 192.0.2.1 as permitted sender) smtp.mailfrom=bounce@mail.esp.example;\r
       dmarc=pass (p=REJECT sp=REJECT dis=NONE) header.from=shop.example\r
From: Shop <hello@shop.example>\r
\r
Hi\r
",
        );
        assert!(result.checked && result.verified);
        assert_eq!(
            (
                result.spf.as_str(),
                result.dkim.as_str(),
                result.dmarc.as_str()
            ),
            ("pass", "pass", "pass")
        );
        assert_eq!(
            result.authenticated,
            ["mail.esp.example", "news.shop.example", "shop.example"]
        );
        assert!(
            result.owns_address("leave@list.shop.example")
                && result.owns_address("hello@shop.example")
        );
        // The envelope sender's ESP and a parent domain are not the sender.
        assert!(
            !result.owns_address("leave@mail.esp.example") && !result.owns_address("a@example")
        );
        assert!(
            !result.owns_address("a@corp.example") && !result.owns_address("a@evilshop.example")
        );
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn a_forged_lower_header_and_comment_text_are_ignored() {
        // Gmail's own verdicts come first; the sender's copy below says pass.
        let result = checked(
            "Authentication-Results: mx.google.com; spf=softfail (google.com: domain of x;dmarc=pass@evil.example) smtp.mailfrom=x@evil.example; dmarc=fail (p=NONE) header.from=bank.example\r
Authentication-Results: mx.google.com; dkim=pass header.d=bank.example; dmarc=pass header.from=bank.example\r
From: Bank <alerts@bank.example>\r
\r
Hi\r
",
        );
        assert!(result.checked && !result.verified);
        assert_eq!(result.dmarc, "fail");
        assert!(!result.owns_address("alerts@bank.example"));
        assert_eq!(result.warnings[0].code, "dmarc_fail");
        assert!(result.warnings[0].text.contains("bank.example"));
    }

    #[test]
    fn another_service_or_no_header_verifies_nothing_and_warns_of_nothing() {
        for raw in [
            "Authentication-Results: spoof.example; dmarc=pass header.from=bank.example\r\nFrom: a@bank.example\r\n\r\nHi\r\n",
            "From: a@bank.example\r\n\r\nHi\r\n",
        ] {
            let result = checked(raw);
            assert!(!result.checked && !result.verified && result.authenticated.is_empty());
            assert!(result.warnings.is_empty());
        }
    }

    #[test]
    fn unauthenticated_mail_and_disguised_senders_warn() {
        let result = checked(
            "Authentication-Results: mx.google.com; spf=neutral smtp.mailfrom=a@xn--pple-43d.example; dkim=none\r
From: \"support@apple.example\" <a@xn--pple-43d.example>\r
\r
Hi\r
",
        );
        let codes: Vec<_> = result.warnings.iter().map(|w| w.code).collect();
        assert_eq!(
            codes,
            ["unauthenticated", "name_address", "lookalike_domain"]
        );
        assert!(result.warnings[1].text.contains("support@apple.example"));
        // A name repeating the real address is not a disguise.
        let plain = checked(
            "Authentication-Results: mx.google.com; spf=pass smtp.mailfrom=a@shop.example; dkim=pass header.d=shop.example\r
From: \"a@shop.example\" <a@shop.example>\r
\r
Hi\r
",
        );
        assert!(plain.verified && plain.warnings.is_empty());
    }

    /// Gmail's verdicts for mail from `bank.example` that fails DMARC, with
    /// `envelope` written as the envelope sender.
    fn spoofed_bank(envelope: &str) -> SenderCheck {
        checked(&format!(
            "Authentication-Results: mx.google.com; spf=pass (google.com: domain of x@evil.example designates 192.0.2.1 as permitted sender) {envelope}; dmarc=fail (p=NONE sp=NONE dis=NONE) header.from=bank.example\r
From: Bank <alerts@bank.example>\r
\r
Hi\r
"
        ))
    }

    #[test]
    fn verdicts_hidden_in_the_envelope_address_are_not_gmails() {
        let baseline = spoofed_bank("smtp.mailfrom=x@evil.example");
        assert!(!baseline.verified && baseline.dmarc == "fail");
        assert_eq!(baseline.warnings[0].code, "dmarc_fail");
        // A quoted local part may hold `;` and spaces; it is one value.
        let quoted = spoofed_bank(r#"smtp.mailfrom="x; dmarc=pass x"@evil.example"#);
        assert_eq!(quoted, baseline);
        let signed = spoofed_bank(
            r#"smtp.mailfrom="x; dkim=pass header.d=bank.example; dmarc=pass header.from=bank.example"@evil.example"#,
        );
        assert_eq!(signed, baseline);
        // Unquoted, a split-off pass names no From domain or conflicts with
        // Gmail's own fail; either way the fail stands.
        for envelope in [
            "smtp.helo=evil.example;dmarc=pass",
            "smtp.helo=evil.example;dmarc=pass header.from=bank.example",
            "smtp.helo=evil.example;dkim=pass",
        ] {
            let result = spoofed_bank(envelope);
            assert!(!result.verified && result.dmarc == "fail", "{envelope}");
            assert_eq!(result.warnings[0].code, "dmarc_fail", "{envelope}");
            assert!(!result.owns_address("alerts@bank.example"), "{envelope}");
        }
        // A quote left open could hide Gmail's verdicts: nothing is trusted.
        let open = spoofed_bank(r#"smtp.mailfrom=x"@evil.example"#);
        assert!(open.checked && !open.verified && open.authenticated.is_empty());
        assert_eq!(open.warnings[0].code, "unauthenticated");
    }

    #[test]
    fn a_method_reported_both_ways_fails() {
        let result = checked(
            "Authentication-Results: mx.google.com; dmarc=pass header.from=bank.example; dkim=pass header.d=bank.example; dkim=fail header.d=bank.example; spf=pass smtp.mailfrom=bank.example; spf=softfail smtp.mailfrom=bank.example; dmarc=fail header.from=bank.example\r
From: Bank <alerts@bank.example>\r
\r
Hi\r
",
        );
        assert_eq!(
            (
                result.spf.as_str(),
                result.dkim.as_str(),
                result.dmarc.as_str()
            ),
            ("softfail", "fail", "fail")
        );
        assert!(!result.verified && result.authenticated.is_empty());
        // A DMARC pass for another domain says nothing about this sender.
        let other = checked(
            "Authentication-Results: mx.google.com; dmarc=pass header.from=evil.example\r
From: Bank <alerts@bank.example>\r
\r
Hi\r
",
        );
        assert!(!other.verified && other.dmarc.is_empty());
    }

    #[test]
    fn a_dmarc_fail_is_never_verified() {
        // DKIM signed by the From domain, yet DMARC failed: not verified.
        let result = checked(
            "Authentication-Results: mx.google.com; dkim=pass header.d=mail.bank.example; dmarc=fail header.from=bank.example\r
From: Bank <alerts@bank.example>\r
\r
Hi\r
",
        );
        assert!(!result.verified);
        assert_eq!(result.warnings[0].code, "dmarc_fail");
    }

    #[test]
    fn only_the_sender_owns_an_address_at_a_shared_provider() {
        // Anyone can sign mail as gmail.com; only the From address is theirs.
        let result = checked(
            "Authentication-Results: mx.google.com; dkim=pass header.d=gmail.com; spf=pass smtp.mailfrom=someone@gmail.com; dmarc=pass header.from=gmail.com\r
From: Someone <someone@gmail.com>\r
\r
Hi\r
",
        );
        assert!(result.verified);
        assert!(result.owns_address("Someone@Gmail.com"));
        assert!(
            !result.owns_address("victim@gmail.com") && !result.owns_address("a@mail.gmail.com")
        );
        for domain in [
            "hotmail.co.uk",
            "gmx.net",
            "yandex.ru",
            "icloud.com",
            "proton.me",
        ] {
            assert!(shared_provider(domain), "{domain}");
        }
        for domain in [
            "shop.example",
            "gmail.shop.example.test",
            "mailbox.org.example",
        ] {
            assert!(!shared_provider(domain), "{domain}");
        }
    }

    #[test]
    fn domains_align_only_on_label_boundaries() {
        assert!(aligned("mail.shop.example", "shop.example"));
        assert!(!aligned("evilshop.example", "shop.example"));
        assert!(!aligned("shop.example", "example"));
        assert!(!aligned("", "shop.example"));
    }
}
