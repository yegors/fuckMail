//! What Gmail's own checks say about who sent a received message, and the
//! warning signs a reader should see. Gmail records SPF, DKIM and DMARC
//! verdicts in the `Authentication-Results` header it adds on arrival; only
//! the topmost one, from `mx.google.com`, is trusted, because a sender can
//! write their own further down. Pure transform: no I/O.
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
    /// The From domain passed DMARC, or a passing DKIM signature belongs to it.
    pub verified: bool,
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
    /// Whether `domain` belongs to a domain Gmail authenticated for this
    /// message: the same domain, or one a subdomain of the other.
    pub fn vouches_for(&self, domain: &str) -> bool {
        let domain = domain.trim().trim_end_matches('.').to_ascii_lowercase();
        self.authenticated
            .iter()
            .any(|known| aligned(known, &domain))
    }
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
/// `None` when another service wrote it.
fn gmail_verdicts(raw: &str) -> Option<Vec<Verdict>> {
    let text = without_comments(raw);
    let mut parts = text.split(';');
    let service = parts.next()?.split_whitespace().next()?;
    if !service.eq_ignore_ascii_case(GMAIL) {
        return None;
    }
    Some(parts.filter_map(verdict).collect())
}

fn verdict(part: &str) -> Option<Verdict> {
    let mut tokens = part.split_whitespace();
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
/// an envelope address, while keeping quoted strings as written.
fn without_comments(raw: &str) -> String {
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
    out
}

fn apply(check: &mut SenderCheck, verdicts: &[Verdict]) {
    let mut dkim_first = None;
    for verdict in verdicts {
        let passed = verdict.result == "pass";
        match verdict.method.as_str() {
            "spf" if check.spf.is_empty() => {
                check.spf = verdict.result.clone();
                if passed && !verdict.domain.is_empty() {
                    check.authenticated.push(verdict.domain.clone());
                }
            }
            "dkim" => {
                dkim_first.get_or_insert_with(|| verdict.result.clone());
                if passed {
                    check.dkim = "pass".into();
                    if !verdict.domain.is_empty() {
                        if aligned(&verdict.domain, &check.from_domain) {
                            check.verified = true;
                        }
                        check.authenticated.push(verdict.domain.clone());
                    }
                }
            }
            "dmarc" if check.dmarc.is_empty() => {
                check.dmarc = verdict.result.clone();
                if passed && !check.from_domain.is_empty() {
                    check.verified = true;
                    check.authenticated.push(check.from_domain.clone());
                }
            }
            _ => {}
        }
    }
    if check.dkim.is_empty() {
        check.dkim = dkim_first.unwrap_or_default();
    }
    check.authenticated.sort();
    check.authenticated.dedup();
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
        assert!(result.vouches_for("list.shop.example") && result.vouches_for("esp.example"));
        assert!(!result.vouches_for("example") && !result.vouches_for("corp.example"));
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
        assert!(!result.vouches_for("bank.example"));
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

    #[test]
    fn domains_align_only_on_label_boundaries() {
        assert!(aligned("mail.shop.example", "shop.example"));
        assert!(!aligned("evilshop.example", "shop.example"));
        assert!(!aligned("shop.example", "example"));
        assert!(!aligned("", "shop.example"));
    }
}
