//! Message content for fuckMail: plain-text extraction from received MIME,
//! MIME construction for human-written mail, and the display transforms for
//! received HTML ([`html_view`]), images ([`image_view`]), previews
//! ([`preview`]), unsubscribe options ([`unsubscribe`]) and Gmail's sender
//! verdicts ([`sender_check`]).
//!
//! Everything here is a pure transform: no network, file reads, database or
//! model calls.
mod html_text;
pub mod html_view;
pub mod image_view;
mod incoming;
pub mod preview;
mod reply_text;
pub mod sender_check;
pub mod unsubscribe;
use ammonia::{Builder as HtmlCleaner, UrlRelative};
pub use incoming::{
    extract_incoming_text, extract_parsed_incoming_contribution, extract_parsed_incoming_text,
    html_source_text, IncomingError,
};
use lettre::message::{
    header::{self as mime_header, ContentTransferEncoding, ContentType},
    Attachment, Mailbox, Mailboxes, Message, MultiPart, SinglePart,
};
pub use reply_text::reply_contribution;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::{Duration, UNIX_EPOCH};

/// Largest authored body or signature, plain or HTML, in bytes (1 MiB).
pub const MAX_BODY_BYTES: usize = 1024 * 1024;
/// Largest combined size of one message's attachments, in bytes (25 MiB),
/// counted before transfer encoding.
pub const MAX_ATTACHMENT_BYTES: usize = 25 * 1024 * 1024;
/// Largest serialized message, in bytes (35 MiB). This leaves room for
/// [`MAX_ATTACHMENT_BYTES`] after base64 expansion plus the text parts.
pub const MAX_WIRE_BYTES: usize = 35 * 1024 * 1024;

/// A mailbox for an address header: an optional display name and an address.
#[derive(Clone)]
pub struct Address {
    /// Display name, encoded by the MIME writer when it is not plain ASCII.
    pub name: Option<String>,
    /// Bare address such as `ann@example.com`, without angle brackets.
    pub email: String,
}

/// Validate an editable draft address with the same parser used for MIME output.
pub fn validate_address(address: &Address) -> Result<(), ComposeError> {
    mailbox(address).map(|_| ())
}

/// The sending account as the MIME writer sees it: its `From` mailbox and
/// signature.
pub struct AccountIdentity {
    /// Must equal [`ComposeRequest::account_id`].
    pub account_id: String,
    pub from: Address,
    /// Appended to the plain body by [`with_signature`]; empty for none.
    pub signature_text: String,
    /// Appended to the HTML body. When absent, the plain signature is escaped
    /// and its line breaks become `<br>`.
    pub signature_html: Option<String>,
}

/// One authored message to format, as persisted in a draft.
#[derive(Clone)]
pub struct ComposeRequest {
    /// Must match the identity and every attachment.
    pub account_id: String,
    /// `to`, `cc` and `bcc` together hold at most 100 addresses.
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    /// At most 8 KiB, with no control characters.
    pub subject: String,
    /// Exact author text, without the account signature. Never replaced with generated text.
    pub body_text: String,
    /// Optional authored rich-text export, kept separate from the source plain text.
    pub body_html: Option<String>,
    /// RFC Message-ID with angle brackets, generated once and persisted with send intent.
    pub message_id: String,
    /// Message-ID being answered. It becomes `In-Reply-To` and is appended to
    /// `References` when missing there.
    pub reply_to_message_id: Option<String>,
    /// Up to 100 Message-IDs of the thread, oldest first, with angle brackets.
    pub references: Vec<String>,
    /// `Date` header, in Unix seconds (UTC), at most the end of year 9999.
    pub timestamp_secs: u64,
}

/// An attachment's bytes, with the checksum Core recorded when it was staged.
pub struct StagedAttachment {
    pub account_id: String,
    /// Non-empty, with no control characters.
    pub filename: String,
    /// A parseable MIME type such as `image/png`.
    pub content_type: String,
    pub bytes: Vec<u8>,
    /// Core's retained blob checksum, verified before serialization.
    pub sha256: String,
    /// Bare CID token without angle brackets; present only for an inline attachment.
    /// Inline parts go inside the HTML body's `multipart/related`, so they need
    /// [`ComposeRequest::body_html`].
    pub content_id: Option<String>,
}

/// Submission bytes contain Bcc because the Gmail API has no separate SMTP envelope.
/// This is private draft/submission content, not a public recipient preview.
pub struct BuiltMail {
    /// The complete RFC 5322 message.
    pub raw: Vec<u8>,
    /// Lowercase hex SHA-256 of `raw`.
    pub sha256: String,
    /// True when the authored HTML plus signature changed under the authored-HTML
    /// allowlist, so what is sent differs from what the composer exported.
    pub html_was_sanitized: bool,
}

/// Why an authored message could not be formatted. Nothing is produced on error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComposeError {
    /// The account ID is empty, or the identity or an attachment belongs to
    /// another account.
    WrongAccount,
    /// The named header value has a control character, or a Message-ID or
    /// filename is malformed.
    InvalidHeader(&'static str),
    /// An email address does not parse.
    InvalidAddress,
    InvalidContentType,
    /// A Content-ID is not a bare token, or an inline part has no HTML body to show it.
    InvalidContentId,
    DuplicateContentId,
    /// [`build_mail`] needs at least one recipient; [`build_draft_mail`] does not.
    NoRecipients,
    /// Attachment bytes no longer match their recorded checksum.
    AttachmentNotVerified,
    /// The named bound (recipients, attachments, body, header or message size) was exceeded.
    LimitExceeded(&'static str),
    /// `timestamp_secs` is past the end of year 9999.
    InvalidDate,
    /// The MIME writer rejected the assembled message.
    MimeBuildFailed,
}

impl std::fmt::Display for ComposeError {
    /// Says what to fix, in words for the person writing the message.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongAccount => {
                f.write_str("the sender or an attachment belongs to another account")
            }
            Self::InvalidHeader(field) => {
                write!(f, "the {field} contains characters mail cannot carry")
            }
            Self::InvalidAddress => f.write_str("a recipient address is not valid"),
            Self::InvalidContentType => f.write_str("an attachment's file type is not valid"),
            Self::InvalidContentId | Self::DuplicateContentId => {
                f.write_str("an inline image reference is not valid")
            }
            Self::NoRecipients => f.write_str("it has no recipients"),
            Self::AttachmentNotVerified => {
                f.write_str("an attachment changed since it was added; attach it again")
            }
            Self::LimitExceeded(bound) => write!(f, "it has too many {bound}"),
            Self::InvalidDate => f.write_str("its date is out of range"),
            Self::MimeBuildFailed => f.write_str("it could not be assembled"),
        }
    }
}

/// Build once from a persisted draft and persist `raw` before queuing its immutable send
/// operation. Deterministic boundaries/date/Message-ID make repeated builds reproducible.
pub fn build_mail(
    request: &ComposeRequest,
    identity: &AccountIdentity,
    attachments: &[StagedAttachment],
) -> Result<BuiltMail, ComposeError> {
    build_authored_mail(request, identity, attachments, false)
}

/// Format a saved draft, including unfinished drafts with no recipients. The
/// supplied identity, headers and attachment proofs obey the same checks as
/// submission; no recipient is invented to satisfy an SMTP envelope.
pub fn build_draft_mail(
    request: &ComposeRequest,
    identity: &AccountIdentity,
    attachments: &[StagedAttachment],
) -> Result<BuiltMail, ComposeError> {
    build_authored_mail(request, identity, attachments, true)
}

fn build_authored_mail(
    request: &ComposeRequest,
    identity: &AccountIdentity,
    attachments: &[StagedAttachment],
    allow_no_recipients: bool,
) -> Result<BuiltMail, ComposeError> {
    // Check scope, headers, limits and attachment proofs before any MIME is built.
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
    let recipient_count = request.to.len() + request.cc.len() + request.bcc.len();
    if recipient_count == 0 && !allow_no_recipients {
        return Err(ComposeError::NoRecipients);
    }
    if recipient_count > 100 {
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

    let from = mailbox(&identity.from)?;
    // `keep_bcc`: Gmail takes Bcc recipients from the raw message itself.
    let mut builder = Message::builder()
        .from(from.clone())
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
    // Checked again: appending the replied-to ID can push the list past the bound.
    if references.len() > 100 {
        return Err(ComposeError::LimitExceeded("reply references"));
    }
    if !references.is_empty() {
        builder = builder.references(references.join(" "));
    }

    let plain = with_signature(&request.body_text, &identity.signature_text);
    let plain_part = text_part(plain.into_bytes(), ContentType::TEXT_PLAIN);
    let mut sanitized = false;
    // Boundaries derive from the Message-ID, so rebuilding a draft yields identical bytes.
    let boundary_key = checksum(request.message_id.as_bytes())[..32].to_owned();
    let mut body = if let Some(html) = &request.body_html {
        let signature = identity
            .signature_html
            .clone()
            .unwrap_or_else(|| escape_html(&identity.signature_text).replace('\n', "<br>"));
        let joined = if signature.is_empty() {
            html.clone()
        } else {
            format!("{html}<br><br>{signature}")
        };
        let safe = sanitize_authored_html(&joined, cids);
        sanitized = safe != joined;
        // multipart/alternative: the plain text first, then the HTML together
        // with the inline images it references, in a multipart/related.
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
    // Ordinary attachments wrap the body in an outer multipart/mixed.
    if attachments.iter().any(|part| part.content_id.is_none()) {
        let mut mixed = MultiPart::mixed()
            .boundary(format!("fm-mixed-{boundary_key}"))
            .multipart(body);
        for attachment in attachments.iter().filter(|part| part.content_id.is_none()) {
            mixed = mixed.singlepart(attachment_part(attachment)?);
        }
        body = mixed;
    }
    let raw = if recipient_count == 0 {
        // Lettre's Message requires an SMTP envelope. Its public typed headers
        // and multipart formatter also support incomplete API drafts directly.
        let mut headers = mime_header::Headers::new();
        headers.set(mime_header::From::from(Mailboxes::from(from)));
        headers.set(mime_header::Subject::from(request.subject.clone()));
        headers.set(mime_header::Date::new(
            UNIX_EPOCH + Duration::from_secs(request.timestamp_secs),
        ));
        headers.set(mime_header::MessageId::from(request.message_id.clone()));
        if let Some(reply) = &request.reply_to_message_id {
            headers.set(mime_header::InReplyTo::from(reply.clone()));
        }
        if !references.is_empty() {
            headers.set(mime_header::References::from(references.join(" ")));
        }
        headers.set(mime_header::MIME_VERSION_1_0);
        let mut raw = headers.to_string().into_bytes();
        // MultiPart starts with its Content-Type header and owns the separating
        // blank line; inserting another separator would turn that header into text.
        raw.extend_from_slice(&body.formatted());
        raw
    } else {
        builder
            .multipart(body)
            .map_err(|_| ComposeError::MimeBuildFailed)?
            .formatted()
    };
    if raw.len() > MAX_WIRE_BYTES {
        return Err(ComposeError::LimitExceeded("serialized message"));
    }
    Ok(BuiltMail {
        sha256: checksum(&raw),
        raw,
        html_was_sanitized: sanitized,
    })
}

/// Signatures are body text the author sees and can delete. Only a draft saved
/// before that, still holding a separate signature, gets it appended here: as
/// the same plain text its composer showed, with no "-- " delimiter.
pub fn with_signature(body: &str, signature: &str) -> String {
    if signature.is_empty() {
        body.to_owned()
    } else {
        format!("{body}\n\n{signature}")
    }
}

fn text_part(bytes: Vec<u8>, content_type: ContentType) -> SinglePart {
    // Byte/base64 transport preserves authored line endings and Unicode exactly.
    SinglePart::builder()
        .header(content_type)
        .header(ContentTransferEncoding::Base64)
        .body(bytes)
}

/// The media type a file's name implies, for files added without one, or
/// `application/octet-stream` when the extension is not a common one.
pub fn attachment_mime(name: &str) -> &'static str {
    let extension = name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase());
    match extension.as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("heic") => "image/heic",
        Some("bmp") => "image/bmp",
        Some("svg") => "image/svg+xml",
        Some("tif" | "tiff") => "image/tiff",
        Some("mp4" | "m4v") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        Some("mkv") => "video/x-matroska",
        Some("avi") => "video/x-msvideo",
        Some("mp3") => "audio/mpeg",
        Some("m4a") => "audio/mp4",
        Some("wav") => "audio/wav",
        Some("pdf") => "application/pdf",
        Some("zip") => "application/zip",
        Some("txt") => "text/plain",
        Some("csv") => "text/csv",
        Some("ics") => "text/calendar",
        Some("doc") => "application/msword",
        Some("docx") => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Some("xls") => "application/vnd.ms-excel",
        Some("xlsx") => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        Some("ppt") => "application/vnd.ms-powerpoint",
        Some("pptx") => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        _ => "application/octet-stream",
    }
}

fn attachment_part(part: &StagedAttachment) -> Result<SinglePart, ComposeError> {
    // Files added without a type carry the one their name implies, so an
    // image arrives as an image.
    let declared = if part.content_type == "application/octet-stream" {
        attachment_mime(&part.filename)
    } else {
        part.content_type.as_str()
    };
    let content_type: ContentType = declared
        .parse()
        .map_err(|_| ComposeError::InvalidContentType)?;
    let builder = if let Some(cid) = &part.content_id {
        Attachment::new_inline_with_name(cid.clone(), part.filename.clone())
    } else {
        Attachment::new(part.filename.clone())
    };
    Ok(builder.body(part.bytes.clone(), content_type))
}

/// Reduces the composer's rich-text export to basic formatting, links and
/// images. An image survives only as `cid:` naming one of `content_ids`, so
/// outgoing HTML never shows a remote or unattached image.
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
/// A Message-ID or Content-ID without its angle brackets: RFC 5322 atext,
/// dots and `@`. Gmail's own IDs use `=` and `_`, so replies must accept
/// every atext character; spaces, quotes, brackets and controls stay out.
fn bare_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".@!#$%&'*+-/=?^_`{|}~".contains(&byte))
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
