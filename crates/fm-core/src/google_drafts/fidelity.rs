//! Compare decoded authored content, not transport-added headers or MIME encoding.
use mail_parser::{Address, MessageParser, MimeHeaders, PartType};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn normalized(bytes: &[u8]) -> Option<Value> {
    let mail = MessageParser::default().parse(bytes)?;
    let mut routing = Vec::new();
    for (header, addresses) in [
        ("From", mail.from()),
        ("To", mail.to()),
        ("Cc", mail.cc()),
        ("Bcc", mail.bcc()),
        ("Reply-To", mail.reply_to()),
    ] {
        if matches!(addresses, Some(Address::Group(_))) {
            return None;
        }
        let mut values = Vec::new();
        for address in addresses.into_iter().flat_map(|v| v.iter()) {
            values.push(json!([address.name().unwrap_or(""), address.address()?]));
        }
        if values.is_empty()
            && mail
                .header_raw(header)
                .is_some_and(|v| !v.trim().is_empty())
        {
            return None;
        }
        routing.push(json!([header, values]));
    }
    let mut parts = Vec::new();
    for part in &mail.parts {
        if part.is_encoding_problem {
            return None;
        }
        let body = match &part.body {
            PartType::Text(text) | PartType::Html(text)
                if part.attachment_name().is_some()
                    || part
                        .content_disposition()
                        .is_some_and(|v| v.c_type.eq_ignore_ascii_case("attachment")) =>
            {
                json!(text)
            }
            PartType::Text(text) | PartType::Html(text) => {
                json!(text.replace("\r\n", "\n").trim_end_matches('\n'))
            }
            PartType::Binary(data) | PartType::InlineBinary(data) => {
                json!(format!("{:x}", Sha256::digest(data)))
            }
            PartType::Multipart(children) => json!(children),
            PartType::Message(_) => return None,
        };
        let kind = part
            .content_type()
            .map(|v| {
                format!("{}/{}", v.c_type, v.c_subtype.as_deref().unwrap_or(""))
                    .to_ascii_lowercase()
            })
            .unwrap_or_else(|| "text/plain".into());
        let mut parameters: Vec<_> = part
            .content_type()
            .and_then(|v| v.attributes.as_ref())
            .into_iter()
            .flatten()
            .filter(|v| {
                !v.name.eq_ignore_ascii_case("boundary") && !v.name.eq_ignore_ascii_case("charset")
            })
            .map(|v| (v.name.to_ascii_lowercase(), v.value.as_ref()))
            .collect();
        parameters.sort_unstable();
        parts.push(json!([
            kind,
            parameters,
            part.attachment_name(),
            part.content_disposition().map(|v| v.c_type.as_ref()),
            part.content_id(),
            body
        ]));
    }
    // Parsed message-id lists preserve reply routing while accepting folding.
    Some(json!([
        routing,
        mail.subject().unwrap_or(""),
        mail.in_reply_to().as_text_list(),
        mail.references().as_text_list(),
        parts
    ]))
}
pub(super) fn equivalent(upload: &[u8], observed: &[u8]) -> Option<bool> {
    if upload == observed {
        return Some(true);
    }
    Some(normalized(upload)? == normalized(observed)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn attachments_and_reply_routing_are_part_of_content() {
        let a = "From: me@example.test\r\nTo: you@example.test\r\nIn-Reply-To: <parent@example.test>\r\nReferences: <parent@example.test>\r\nContent-Type: multipart/mixed; boundary=one\r\n\r\n--one\r\nContent-Type: text/plain\r\n\r\nBody\r\n--one\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=sample.bin\r\nContent-Transfer-Encoding: base64\r\n\r\nYWJj\r\n--one--\r\n";
        let normalized = a.replace("one", "two");
        assert_eq!(equivalent(a.as_bytes(), normalized.as_bytes()), Some(true));
        for (old, new) in [
            ("YWJj", "YWJk"),
            ("sample.bin", "changed.bin"),
            ("parent@example.test", "other@example.test"),
        ] {
            assert_eq!(
                equivalent(a.as_bytes(), normalized.replace(old, new).as_bytes()),
                Some(false)
            );
        }
    }
    #[test]
    fn text_attachment_newlines_and_rendering_parameters_are_not_discarded() {
        let attachment = b"Content-Type: text/plain\r\nContent-Disposition: attachment; filename=note.txt\r\n\r\nFile text\r\n";
        let mut changed = attachment.to_vec();
        changed.extend_from_slice(b"\r\n");
        assert_eq!(equivalent(attachment, &changed), Some(false));
        let fixed = b"Content-Type: text/plain\r\n\r\nBody\r\n";
        let flowed = b"Content-Type: text/plain; format=flowed\r\n\r\nBody\r\n";
        assert_eq!(equivalent(fixed, flowed), Some(false));
    }
    #[test]
    fn transport_headers_encoding_and_newlines_preserve_content_but_edits_do_not() {
        let a=b"From: Me <me@example.test>\r\nTo: You <you@example.test>\r\nSubject: Hello\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nHello world\r\n";
        let b=b"Received: by Google\r\nMessage-ID: <generated@example.test>\r\nFrom: Me <me@example.test>\r\nTo: You <you@example.test>\r\nSubject: Hello\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\nHello=20world\r\n\r\n";
        assert_eq!(equivalent(a, b), Some(true));
        for (old, new) in [
            ("Hello=20world", "Changed"),
            ("you@example.test", "else@example.test"),
            ("Subject: Hello", "Subject: Changed"),
        ] {
            assert_eq!(
                equivalent(a, String::from_utf8_lossy(b).replace(old, new).as_bytes()),
                Some(false)
            );
        }
    }
}
