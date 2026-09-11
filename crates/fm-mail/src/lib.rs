//! Shared human-authored MIME construction. No transport, AI, file reads or database.
use ammonia::{Builder as HtmlCleaner, UrlRelative};
use lettre::message::{
    header::{ContentTransferEncoding, ContentType},
    Attachment, Mailbox, Message, MultiPart, SinglePart,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::{Duration, UNIX_EPOCH};

pub const MAX_BODY_BYTES: usize = 1024 * 1024;
pub const MAX_ATTACHMENT_BYTES: usize = 25 * 1024 * 1024;
pub const MAX_WIRE_BYTES: usize = 35 * 1024 * 1024;

#[derive(Clone)]
pub struct Address {
    pub name: Option<String>,
    pub email: String,
}

pub struct AccountIdentity {
    pub account_id: String,
    pub from: Address,
    pub signature_text: String,
    pub signature_html: Option<String>,
}

#[derive(Clone)]
pub struct ComposeRequest {
    pub account_id: String,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub subject: String,
    /// Exact author text, without the account signature. Never replaced with generated text.
    pub body_text: String,
    /// Optional authored rich-text export, kept separate from the source plain text.
    pub body_html: Option<String>,
    /// RFC Message-ID with angle brackets, generated once and persisted with send intent.
    pub message_id: String,
    pub reply_to_message_id: Option<String>,
    pub references: Vec<String>,
    pub timestamp_secs: u64,
}

pub struct StagedAttachment {
    pub account_id: String,
    pub filename: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
    /// Core's retained blob checksum, verified before serialization.
    pub sha256: String,
    /// Bare CID token without angle brackets; present only for an inline attachment.
    pub content_id: Option<String>,
}

/// Submission bytes contain Bcc because the Gmail API has no separate SMTP envelope.
/// This is private draft/submission content, not a public recipient preview.
pub struct BuiltMail {
    pub raw: Vec<u8>,
    pub sha256: String,
    pub html_was_sanitized: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComposeError {
    WrongAccount,
    InvalidHeader(&'static str),
    InvalidAddress,
    InvalidContentType,
    InvalidContentId,
    DuplicateContentId,
    NoRecipients,
    AttachmentNotVerified,
    LimitExceeded(&'static str),
    InvalidDate,
    MimeBuildFailed,
}

/// Build once from a persisted draft and persist `raw` before queuing its immutable send
/// operation. Deterministic boundaries/date/Message-ID make repeated builds reproducible.
pub fn build_mail(
    request: &ComposeRequest,
    identity: &AccountIdentity,
    attachments: &[StagedAttachment],
) -> Result<BuiltMail, ComposeError> {
    if request.account_id.is_empty() || request.account_id != identity.account_id {
        return Err(ComposeError::WrongAccount);
    }
    header(&request.subject, "subject")?;
    message_id(&request.message_id)?;
    if request.timestamp_secs > 253_402_300_799 {
        return Err(ComposeError::InvalidDate);
    }
    if request.references.len() > 100 {
        return Err(ComposeError::LimitExceeded("reply references"));
    }
    for reference in &request.references {
        message_id(reference)?;
    }
    if let Some(reply) = &request.reply_to_message_id {
        message_id(reply)?;
    }
    if request.to.len() + request.cc.len() + request.bcc.len() == 0 {
        return Err(ComposeError::NoRecipients);
    }
    if request.to.len() + request.cc.len() + request.bcc.len() > 100 {
        return Err(ComposeError::LimitExceeded("recipients"));
    }
    if attachments.len() > 50 {
        return Err(ComposeError::LimitExceeded("attachments"));
    }
    for text in [&request.body_text, &identity.signature_text]
        .into_iter()
        .chain(request.body_html.iter())
        .chain(identity.signature_html.iter())
    {
        if text.len() > MAX_BODY_BYTES {
            return Err(ComposeError::LimitExceeded("authored body"));
        }
    }
    let mut total_bytes: usize = 0;
    let mut cids = BTreeSet::new();
    for attachment in attachments {
        if attachment.account_id != request.account_id {
            return Err(ComposeError::WrongAccount);
        }
        header(&attachment.filename, "filename")?;
        if attachment.filename.is_empty() {
            return Err(ComposeError::InvalidHeader("filename"));
        }
        header(&attachment.content_type, "content type")?;
        total_bytes = total_bytes
            .checked_add(attachment.bytes.len())
            .ok_or(ComposeError::LimitExceeded("attachment bytes"))?;
        if total_bytes > MAX_ATTACHMENT_BYTES {
            return Err(ComposeError::LimitExceeded("attachment bytes"));
        }
        if attachment.sha256 != checksum(&attachment.bytes) {
            return Err(ComposeError::AttachmentNotVerified);
        }
        if let Some(cid) = &attachment.content_id {
            if !bare_id(cid) {
                return Err(ComposeError::InvalidContentId);
            }
            if !cids.insert(cid.clone()) {
                return Err(ComposeError::DuplicateContentId);
            }
            if request.body_html.is_none() {
                return Err(ComposeError::InvalidContentId);
            }
        }
    }

    let mut builder = Message::builder()
        .from(mailbox(&identity.from)?)
        .subject(request.subject.clone())
        .date(UNIX_EPOCH + Duration::from_secs(request.timestamp_secs))
        .message_id(Some(request.message_id.clone()))
        .keep_bcc();
    for address in &request.to {
        builder = builder.to(mailbox(address)?);
    }
    for address in &request.cc {
        builder = builder.cc(mailbox(address)?);
    }
    for address in &request.bcc {
        builder = builder.bcc(mailbox(address)?);
    }
    if let Some(reply) = &request.reply_to_message_id {
        builder = builder.in_reply_to(reply.clone());
    }
    let mut references = request.references.clone();
    if let Some(reply) = &request.reply_to_message_id {
        if !references.contains(reply) {
            references.push(reply.clone());
        }
    }
    if references.len() > 100 {
        return Err(ComposeError::LimitExceeded("reply references"));
    }
    if !references.is_empty() {
        builder = builder.references(references.join(" "));
    }

    let plain = with_signature(&request.body_text, &identity.signature_text);
    let plain_part = text_part(plain.into_bytes(), ContentType::TEXT_PLAIN);
    let mut sanitized = false;
    let boundary_key = checksum(request.message_id.as_bytes())[..32].to_owned();
    let mut body = if let Some(html) = &request.body_html {
        let signature = identity
            .signature_html
            .clone()
            .unwrap_or_else(|| escape_html(&identity.signature_text).replace('\n', "<br>"));
        let joined = if signature.is_empty() {
            html.clone()
        } else {
            format!("{html}<div>-- <br>{signature}</div>")
        };
        let safe = sanitize_authored_html(&joined, cids);
        sanitized = safe != joined;
        let mut related = MultiPart::related()
            .boundary(format!("fm-related-{boundary_key}"))
            .singlepart(text_part(safe.into_bytes(), ContentType::TEXT_HTML));
        for attachment in attachments.iter().filter(|part| part.content_id.is_some()) {
            related = related.singlepart(attachment_part(attachment)?);
        }
        MultiPart::alternative()
            .boundary(format!("fm-alternative-{boundary_key}"))
            .singlepart(plain_part)
            .multipart(related)
    } else {
        // Plain messages with attachments need a multipart container; no empty HTML part.
        MultiPart::mixed()
            .boundary(format!("fm-plain-{boundary_key}"))
            .singlepart(plain_part)
    };
    if attachments.iter().any(|part| part.content_id.is_none()) {
        let mut mixed = MultiPart::mixed()
            .boundary(format!("fm-mixed-{boundary_key}"))
            .multipart(body);
        for attachment in attachments.iter().filter(|part| part.content_id.is_none()) {
            mixed = mixed.singlepart(attachment_part(attachment)?);
        }
        body = mixed;
    }
    let raw = builder
        .multipart(body)
        .map_err(|_| ComposeError::MimeBuildFailed)?
        .formatted();
    if raw.len() > MAX_WIRE_BYTES {
        return Err(ComposeError::LimitExceeded("serialized message"));
    }
    Ok(BuiltMail {
        sha256: checksum(&raw),
        raw,
        html_was_sanitized: sanitized,
    })
}

pub fn with_signature(body: &str, signature: &str) -> String {
    if signature.is_empty() {
        body.to_owned()
    } else {
        format!("{body}\n\n-- \n{signature}")
    }
}

fn text_part(bytes: Vec<u8>, content_type: ContentType) -> SinglePart {
    // Byte/base64 transport preserves authored line endings and Unicode exactly.
    SinglePart::builder()
        .header(content_type)
        .header(ContentTransferEncoding::Base64)
        .body(bytes)
}

fn attachment_part(part: &StagedAttachment) -> Result<SinglePart, ComposeError> {
    let content_type: ContentType = part
        .content_type
        .parse()
        .map_err(|_| ComposeError::InvalidContentType)?;
    let builder = if let Some(cid) = &part.content_id {
        Attachment::new_inline_with_name(cid.clone(), part.filename.clone())
    } else {
        Attachment::new(part.filename.clone())
    };
    Ok(builder.body(part.bytes.clone(), content_type))
}

fn sanitize_authored_html(html: &str, content_ids: BTreeSet<String>) -> String {
    let mut cleaner = HtmlCleaner::default();
    cleaner
        .tags(HashSet::from([
            "p",
            "br",
            "b",
            "strong",
            "i",
            "em",
            "u",
            "ul",
            "ol",
            "li",
            "blockquote",
            "a",
            "code",
            "pre",
            "span",
            "div",
            "img",
        ]))
        .tag_attributes(HashMap::from([
            ("a", HashSet::from(["href"])),
            ("img", HashSet::from(["src", "alt"])),
        ]))
        .generic_attributes(HashSet::from(["style"]))
        .filter_style_properties(HashSet::from([
            "font-weight",
            "font-style",
            "text-decoration",
        ]))
        .url_schemes(HashSet::from(["https", "http", "mailto", "cid"]))
        .url_relative(UrlRelative::Deny)
        .attribute_filter(move |tag, attribute, value| {
            if tag == "img" && attribute == "src" {
                value
                    .strip_prefix("cid:")
                    .filter(|cid| content_ids.contains(*cid))
                    .map(|_| value.into())
            } else {
                Some(value.into())
            }
        });
    cleaner.clean(html).to_string()
}

fn mailbox(address: &Address) -> Result<Mailbox, ComposeError> {
    header(&address.email, "email")?;
    if let Some(name) = &address.name {
        header(name, "display name")?;
    }
    Ok(Mailbox::new(
        address.name.clone(),
        address
            .email
            .parse()
            .map_err(|_| ComposeError::InvalidAddress)?,
    ))
}
fn header(value: &str, field: &'static str) -> Result<(), ComposeError> {
    if value.len() > 8192 {
        return Err(ComposeError::LimitExceeded("header bytes"));
    }
    if value.chars().any(char::is_control) {
        return Err(ComposeError::InvalidHeader(field));
    }
    Ok(())
}
fn message_id(value: &str) -> Result<(), ComposeError> {
    header(value, "message identity")?;
    let inner = value
        .strip_prefix('<')
        .and_then(|value| value.strip_suffix('>'));
    if !inner.is_some_and(|value| bare_id(value) && value.contains('@')) {
        return Err(ComposeError::InvalidHeader("message identity"));
    }
    Ok(())
}
fn bare_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._@+-".contains(&byte))
}
fn checksum(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
