//! Opening received mail: decrypting PGP/MIME and inline PGP messages, and
//! checking their signatures against the keyring.
//!
//! An opened message is rebuilt as an ordinary RFC 5322 message: the outer
//! header fields with the decrypted (or unwrapped signed) MIME entity in place
//! of the protected body. Callers read it with the same MIME code as any other
//! message, then throw it away; plaintext is never written anywhere here.
//!
//! Only protected content is shown as protected. Text around an inline armored
//! block is left out of the opened message, and [`Opened::unprotected`] says
//! so, rather than appearing under the block's signature or encryption. When
//! the sender used header protection (RFC 9788), outer header fields changed
//! after sending are reported in [`HeaderCheck::altered`].
//!
//! Signatures are only as trustworthy as the keyring: a key counts as the
//! sender's when the person imported it and its user IDs include the sender's
//! address. There is no web of trust and no key lookup on the network.
use crate::{
    cert::{acceptable_hash, address, created, evaluate, evaluate_subkeys, KeyRef, KeyStatus},
    hex, PgpError, Result, MAX_PLAINTEXT_BYTES,
};
use mail_parser::{MessageParser, MimeHeaders, PartType};
use pgp::{
    composed::{
        CleartextSignedMessage, DecryptionOptions, Deserializable, DetachedSignature, Esk, Message,
        SignedPublicKey, SignedSecretKey, TheRing,
    },
    packet::{Signature, SignatureType, SubpacketData},
    types::{Fingerprint, KeyDetails, Password, VerifyingKey},
};
use serde::Serialize;
use std::io::Read;
use zeroize::Zeroizing;

/// The public keys known to the person, for checking signatures.
pub struct Keyring {
    pub certs: Vec<SignedPublicKey>,
}

/// How a signature checked out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignatureStatus {
    /// It verifies with a key in the keyring that could sign when it was made.
    Good,
    /// The content doesn't match the signature: it was changed after signing,
    /// or the signature isn't a signature over a document at all.
    Bad,
    /// No key in the keyring made it.
    UnknownKey,
    /// The key had expired when the signature was made.
    KeyExpired,
    /// The key has been revoked.
    KeyRevoked,
    /// It uses MD5, SHA-1 or another hash too weak to trust.
    Weak,
    /// The key isn't bound by a valid self-signature, or may not sign.
    InvalidKey,
}

/// One signature's result.
#[derive(Clone, Debug, Serialize)]
pub struct SignatureCheck {
    pub status: SignatureStatus,
    /// The issuer's key ID, uppercase hex, when the signature names one.
    pub key_id: Option<String>,
    /// The signing certificate's primary fingerprint, when it is in the keyring.
    pub fingerprint: Option<String>,
    /// The signing certificate's primary user ID.
    pub signer: Option<String>,
    /// The sender's address is one of the signing key's user IDs.
    pub sender_matches: bool,
    pub created_at: Option<u64>,
    /// The signature names the recipients it was made for (RFC 9580 intended
    /// recipients), and none of the keys that decrypted the message is among
    /// them: someone may have re-encrypted another person's signed message.
    pub forwarded: bool,
}

/// What header protection (RFC 9788) says about the outer header fields.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HeaderCheck {
    /// The Subject the sender protected, when there was one.
    pub subject: Option<String>,
    /// Outer header fields that differ from what the sender protected: any of
    /// `subject`, `from`, `to`, `cc` and `date`.
    pub altered: Vec<&'static str>,
}

/// What happened when decrypting.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Decryption {
    Decrypted,
    /// Left encrypted, because the reader didn't ask to decrypt it yet.
    /// `recipients` are the key IDs it was encrypted to.
    Sealed {
        recipients: Vec<String>,
    },
    /// None of the person's secret keys was among its recipients.
    NoKey {
        recipients: Vec<String>,
    },
    /// The secret keys couldn't be opened, such as when the system keychain
    /// refused.
    Unavailable {
        code: String,
        message: String,
    },
    /// It was damaged, not integrity-protected, or too large.
    Failed {
        message: String,
    },
}

/// The result of [`open_message`].
pub struct Opened {
    pub encrypted: bool,
    /// The protection was inline armor in a text part, not PGP/MIME.
    pub inline: bool,
    /// An inline message also has text or files outside its signed or
    /// encrypted block. The text is left out of `message`; the files aren't
    /// covered by the block's protection.
    pub unprotected: bool,
    pub decryption: Option<Decryption>,
    pub signatures: Vec<SignatureCheck>,
    /// Header protection, when the sender used it.
    pub headers: Option<HeaderCheck>,
    /// The message to read in place of the source, when there is one.
    pub message: Option<Zeroizing<Vec<u8>>>,
}

/// A cheap byte check for whether `raw` may hold OpenPGP content at all.
pub fn looks_protected(raw: &[u8]) -> bool {
    let has = |needle: &[u8]| {
        raw.windows(needle.len())
            .any(|w| w.eq_ignore_ascii_case(needle))
    };
    has(b"application/pgp-") || has(b"-----BEGIN PGP ")
}

fn content_type(part: &mail_parser::MessagePart<'_>) -> Option<(String, String, Option<String>)> {
    let ct = part.content_type()?;
    Some((
        ct.ctype().to_ascii_lowercase(),
        ct.subtype().unwrap_or_default().to_ascii_lowercase(),
        ct.attribute("protocol").map(|p| p.to_ascii_lowercase()),
    ))
}

/// The header fields header protection compares: the subject and date, and
/// the From, To and Cc addresses (lowercased, sorted, without names).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Fields {
    subject: Option<String>,
    from: Vec<String>,
    to: Vec<String>,
    cc: Vec<String>,
    date: Option<i64>,
}

impl Fields {
    fn of(message: &mail_parser::Message<'_>) -> Self {
        let addresses = |list: Option<&mail_parser::Address<'_>>| {
            let mut out: Vec<String> = list
                .into_iter()
                .flat_map(|list| list.iter())
                .filter_map(|a| a.address())
                .map(str::to_lowercase)
                .collect();
            out.sort();
            out.dedup();
            out
        };
        Self {
            subject: message
                .subject()
                .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" ")),
            from: addresses(message.from()),
            to: addresses(message.to()),
            cc: addresses(message.cc()),
            date: message.date().map(|d| d.to_timestamp()),
        }
    }

    /// The fields that differ between `self` and `outer`: those `self` has,
    /// or with `all`, those either has.
    fn differences(&self, outer: &Fields, all: bool) -> Vec<&'static str> {
        let mut out = Vec::new();
        let mut compare = |name: &'static str, mine: bool, theirs: bool, same: bool| {
            if (mine || (all && theirs)) && !same {
                out.push(name);
            }
        };
        compare(
            "subject",
            self.subject.is_some(),
            outer.subject.is_some(),
            self.subject == outer.subject,
        );
        compare(
            "from",
            !self.from.is_empty(),
            !outer.from.is_empty(),
            self.from == outer.from,
        );
        compare(
            "to",
            !self.to.is_empty(),
            !outer.to.is_empty(),
            self.to == outer.to,
        );
        compare(
            "cc",
            !self.cc.is_empty(),
            !outer.cc.is_empty(),
            self.cc == outer.cc,
        );
        compare(
            "date",
            self.date.is_some(),
            outer.date.is_some(),
            self.date == outer.date,
        );
        out
    }
}

/// Where an entity's header section ends: the index of the blank line that
/// closes it (0 when the entity starts with one), or `None` without one.
fn header_end(entity: &[u8]) -> Option<usize> {
    if entity.starts_with(b"\r\n") || entity.starts_with(b"\n") {
        return Some(0);
    }
    let crlf = entity
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|p| p + 2);
    let lf = entity.windows(2).position(|w| w == b"\n\n").map(|p| p + 1);
    match (crlf, lf) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// The `HP-Outer` fields of a protected header section, read as a header
/// section of their own: the outer fields as the sender sent them.
fn hp_outer(entity: &[u8]) -> Option<Fields> {
    let head = &entity[..header_end(entity)?];
    let mut fields: Vec<Vec<u8>> = Vec::new();
    let mut current: Option<Vec<u8>> = None;
    for line in head.split_inclusive(|b| *b == b'\n') {
        if line.starts_with(b" ") || line.starts_with(b"\t") {
            if let Some(field) = current.as_mut() {
                field.extend_from_slice(line);
            }
            continue;
        }
        fields.extend(current.take());
        if line.len() > 9 && line[..9].eq_ignore_ascii_case(b"hp-outer:") {
            current = Some(line[9..].trim_ascii_start().to_vec());
        }
    }
    fields.extend(current);
    if fields.is_empty() {
        return None;
    }
    let mut section = Vec::new();
    for field in fields {
        section.extend_from_slice(field.trim_ascii_end());
        section.extend_from_slice(b"\r\n");
    }
    section.extend_from_slice(b"\r\n");
    let parsed = MessageParser::default().parse(&section)?;
    Some(Fields::of(&parsed))
}

/// Checks the header protection (RFC 9788) of a cryptographic payload whose
/// root carries an `hp` parameter, against the `outer` fields. Signed-only
/// (`clear`) payloads protect copies that must match the outer fields; for
/// encrypted (`cipher`) ones the `HP-Outer` fields record what was sent
/// outside, since the sender may have hidden the subject there.
fn header_check(outer: &Fields, payload: &[u8]) -> Option<HeaderCheck> {
    let parsed = MessageParser::default().parse(payload)?;
    let hp = parsed
        .parts
        .first()?
        .content_type()?
        .attribute("hp")?
        .to_ascii_lowercase();
    let inner = Fields::of(&parsed);
    let mut altered = match hp.as_str() {
        "clear" => inner.differences(outer, false),
        "cipher" => hp_outer(payload)
            .map(|sent| sent.differences(outer, true))
            .unwrap_or_default(),
        _ => return None,
    };
    // A From that isn't the protected one is never the sender's to trust.
    if !inner.from.is_empty() && inner.from != outer.from && !altered.contains(&"from") {
        altered.push("from");
    }
    Some(HeaderCheck {
        subject: inner.subject,
        altered,
    })
}

/// The outer header block without its `Content-*` and `MIME-Version`
/// fields, ending in CRLF.
fn outer_headers(raw: &[u8], body_offset: usize) -> Vec<u8> {
    let head = &raw[..body_offset.min(raw.len())];
    let mut out = Vec::new();
    let mut keep = true;
    for line in head.split_inclusive(|b| *b == b'\n') {
        if line == b"\r\n" || line == b"\n" {
            break;
        }
        if !line.starts_with(b" ") && !line.starts_with(b"\t") {
            let lower = line.to_ascii_lowercase();
            keep = !lower.starts_with(b"content-") && !lower.starts_with(b"mime-version:");
        }
        if keep {
            out.extend_from_slice(line);
            if !line.ends_with(b"\r\n") {
                out.pop();
                out.extend_from_slice(b"\r\n");
            }
        }
    }
    out.extend_from_slice(b"MIME-Version: 1.0\r\n");
    out
}

/// Drops the non-`Content-*` header fields an inner entity carries (protected
/// headers), so the outer ones stay authoritative for routing.
fn content_only(entity: &[u8]) -> Vec<u8> {
    let Some(end) = header_end(entity) else {
        return entity.to_vec();
    };
    let mut out = Vec::with_capacity(entity.len());
    let mut keep = false;
    for line in entity[..end].split_inclusive(|b| *b == b'\n') {
        if !line.starts_with(b" ") && !line.starts_with(b"\t") {
            keep = line.len() >= 8 && line[..8].eq_ignore_ascii_case(b"content-");
        }
        if keep {
            out.extend_from_slice(line);
        }
    }
    if out.is_empty() {
        out.extend_from_slice(b"Content-Type: text/plain; charset=us-ascii\r\n");
    }
    out.extend_from_slice(&entity[end..]);
    out
}

fn text_message(outer: &[u8], text: &str) -> Zeroizing<Vec<u8>> {
    use base64::Engine;
    let mut out = outer.to_vec();
    out.extend_from_slice(
        b"Content-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: base64\r\n\r\n",
    );
    let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    for chunk in encoded.as_bytes().chunks(76) {
        out.extend_from_slice(chunk);
        out.extend_from_slice(b"\r\n");
    }
    Zeroizing::new(out)
}

/// Finds which certificate and component key made `sig`.
fn signer<'a>(
    sig: &Signature,
    keyring: &'a Keyring,
) -> Option<(&'a SignedPublicKey, Fingerprint, KeyRef<'a>)> {
    let fingerprints = sig.issuer_fingerprint();
    let ids = sig.issuer_key_id();
    let matches = |fingerprint: Fingerprint, id: pgp::types::KeyId| {
        fingerprints.iter().any(|f| **f == fingerprint) || ids.iter().any(|i| **i == id)
    };
    for cert in &keyring.certs {
        if matches(
            cert.primary_key.fingerprint(),
            cert.primary_key.legacy_key_id(),
        ) {
            return Some((
                cert,
                cert.primary_key.fingerprint(),
                KeyRef::Primary(&cert.primary_key),
            ));
        }
        for sub in &cert.public_subkeys {
            if matches(sub.key.fingerprint(), sub.key.legacy_key_id()) {
                return Some((cert, sub.key.fingerprint(), KeyRef::Sub(&sub.key)));
            }
        }
    }
    None
}

/// Verifies a detached or cleartext signature over `data`.
fn verify_detached(sig: &Signature, key: KeyRef<'_>, data: &[u8]) -> bool {
    match key {
        KeyRef::Primary(key) => sig.verify(key, data).is_ok(),
        KeyRef::Sub(key) => sig.verify(key, data).is_ok(),
    }
}

/// What every signature check needs: the keyring, the sender's address and
/// the outer header fields, and the time.
struct Context<'a> {
    keyring: &'a Keyring,
    sender: Option<&'a str>,
    outer: Fields,
    now: u64,
}

/// Checks one signature: `verifies` runs the cryptographic check with the
/// signing key once it is found. `own` are the primary fingerprints of the
/// keys that decrypted the content, or empty when it wasn't encrypted.
fn check(
    sig: &Signature,
    cx: &Context<'_>,
    own: &[Fingerprint],
    verifies: impl Fn(KeyRef<'_>) -> bool,
) -> SignatureCheck {
    let key_id = sig
        .issuer_fingerprint()
        .first()
        .map(|f| hex(f.as_bytes()))
        .or_else(|| sig.issuer_key_id().first().map(|id| hex(id.as_ref())));
    let made = created(sig);
    let intended: Vec<&Fingerprint> = sig
        .config()
        .into_iter()
        .flat_map(|config| config.hashed_subpackets())
        .filter_map(|p| match &p.data {
            SubpacketData::IntendedRecipientFingerprint(fingerprint) => Some(fingerprint),
            _ => None,
        })
        .collect();
    let forwarded =
        !own.is_empty() && !intended.is_empty() && !intended.iter().any(|f| own.contains(f));
    let Some((cert, used, key)) = signer(sig, cx.keyring) else {
        return SignatureCheck {
            status: SignatureStatus::UnknownKey,
            key_id,
            fingerprint: None,
            signer: None,
            sender_matches: false,
            created_at: made,
            forwarded,
        };
    };
    let now = cx.now;
    let current = evaluate(cert, now);
    let addresses: Vec<String> = current
        .users
        .iter()
        .filter_map(|(id, _)| address(id))
        .collect();
    let at = made.unwrap_or(now).min(now);
    let then = evaluate(cert, at);
    let usable = if used == cert.primary_key.fingerprint() {
        then.usage.sign
    } else {
        evaluate_subkeys(cert, at).iter().any(|sub| {
            sub.key.key.fingerprint() == used
                && sub.usage.sign
                && sub.status(at) == KeyStatus::Valid
        })
    };
    // Only a signature over a document counts here; a key certification or
    // revocation that happens to verify over the same bytes doesn't.
    let document = matches!(sig.typ(), Some(SignatureType::Binary | SignatureType::Text));
    let status = if !document || !verifies(key) {
        SignatureStatus::Bad
    } else if !acceptable_hash(sig, false) {
        SignatureStatus::Weak
    } else if current.revoked {
        SignatureStatus::KeyRevoked
    } else {
        match then.status(at) {
            KeyStatus::Expired => SignatureStatus::KeyExpired,
            KeyStatus::Valid if usable => SignatureStatus::Good,
            _ => SignatureStatus::InvalidKey,
        }
    };
    SignatureCheck {
        status,
        key_id,
        fingerprint: Some(hex(cert.primary_key.fingerprint().as_bytes())),
        signer: current.users.first().map(|(id, _)| id.clone()),
        sender_matches: cx.sender.is_some_and(|s| addresses.iter().any(|a| a == s)),
        created_at: made,
        forwarded,
    }
}

/// Reads a decrypted or signed OpenPGP message's literal data, refusing more
/// than [`MAX_PLAINTEXT_BYTES`].
fn read_limited(message: &mut Message<'_>) -> Result<Zeroizing<Vec<u8>>> {
    let mut data = Zeroizing::new(Vec::new());
    message
        .take(MAX_PLAINTEXT_BYTES as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|e| PgpError::new("invalid_message", format!("The message is damaged: {e}")))?;
    if data.len() > MAX_PLAINTEXT_BYTES {
        return Err(PgpError::new(
            "limit_exceeded",
            "The decrypted message is larger than 64 MiB",
        ));
    }
    Ok(data)
}

/// Signatures inside an OpenPGP message that has been read to its end.
fn message_signatures(
    message: &Message<'_>,
    cx: &Context<'_>,
    own: &[Fingerprint],
) -> Vec<SignatureCheck> {
    let Message::Signed { reader, .. } = message else {
        return Vec::new();
    };
    (0..reader.num_signatures())
        .filter_map(|index| {
            let sig = reader.signature(index)?;
            Some(check(sig, cx, own, |key| {
                let key: &dyn VerifyingKey = match key {
                    KeyRef::Primary(key) => key,
                    KeyRef::Sub(key) => key,
                };
                message.verify_nested_explicit(index, key).is_ok()
            }))
        })
        .collect()
}

/// The key IDs an encrypted message was encrypted to.
fn recipients(message: &Message<'_>) -> Vec<String> {
    let Message::Encrypted { esk, .. } = message else {
        return Vec::new();
    };
    esk.iter()
        .filter_map(|esk| match esk {
            Esk::PublicKeyEncryptedSessionKey(pkesk) => pkesk.id().ok().map(|id| hex(id.as_ref())),
            Esk::SymKeyEncryptedSessionKey(_) => None,
        })
        .collect()
}

/// Where the secret keys for decrypting come from: `None` leaves encrypted
/// content sealed.
type Secrets<'a> = Option<&'a mut dyn FnMut() -> Result<Vec<SignedSecretKey>>>;

/// One OpenPGP message, decrypted or just read.
#[derive(Default)]
struct Unwrapped {
    decryption: Option<Decryption>,
    signatures: Vec<SignatureCheck>,
    plain: Option<Zeroizing<Vec<u8>>>,
    /// The primary fingerprints of the keys it was decrypted with.
    own: Vec<Fingerprint>,
}

/// Decrypts (or just reads, if only signed) one OpenPGP message.
fn read_pgp_message(
    data: &[u8],
    armored: bool,
    cx: &Context<'_>,
    secrets: &mut Secrets<'_>,
) -> Unwrapped {
    let failed = |message: String| Unwrapped {
        decryption: Some(Decryption::Failed { message }),
        ..Default::default()
    };
    let only = |decryption: Decryption| Unwrapped {
        decryption: Some(decryption),
        ..Default::default()
    };
    let parsed = if armored {
        Message::from_armor(data).map(|(m, _)| m)
    } else {
        Message::from_bytes(data)
    };
    let Ok(mut message) = parsed else {
        return failed("The encrypted part is damaged".into());
    };
    let mut decryption = None;
    let mut own = Vec::new();
    if message.is_encrypted() {
        let to = recipients(&message);
        let Some(secrets) = secrets.as_mut() else {
            return only(Decryption::Sealed { recipients: to });
        };
        let keys = match secrets() {
            Ok(keys) => keys,
            Err(error) => {
                return only(Decryption::Unavailable {
                    code: error.code.into(),
                    message: error.message,
                })
            }
        };
        if keys.is_empty() {
            return only(Decryption::NoKey { recipients: to });
        }
        let password = Password::empty();
        // GnuPG 2.4 encrypts to keys that announce it with its own OCB packet
        // (LibrePGP), which is authenticated like SEIPD. Unprotected legacy
        // encryption stays refused.
        let ring = TheRing {
            secret_keys: keys.iter().collect(),
            key_passwords: vec![&password; keys.len()],
            decrypt_options: DecryptionOptions::new().enable_gnupg_aead(),
            ..Default::default()
        };
        message = match message
            .decrypt_the_ring(ring, true)
            .map(|(message, _)| message)
        {
            Ok(message) => message,
            Err(pgp::errors::Error::MissingKey) => {
                return only(Decryption::NoKey { recipients: to })
            }
            Err(error) => return failed(format!("It couldn't be decrypted: {error}")),
        };
        decryption = Some(Decryption::Decrypted);
        own = keys
            .iter()
            .map(|key| key.primary_key.fingerprint())
            .collect();
    }
    if message.is_compressed() {
        message = match message.decompress() {
            Ok(message) => message,
            Err(_) => return failed("The compressed content is damaged".into()),
        };
    }
    let plain = match read_limited(&mut message) {
        Ok(plain) => plain,
        Err(error) => return failed(error.message),
    };
    let signatures = message_signatures(&message, cx, &own);
    Unwrapped {
        decryption,
        signatures,
        plain: Some(plain),
        own,
    }
}

/// The exact bytes of a multipart's sub-parts, split at `boundary`, with line
/// endings canonicalized to CRLF for signature checks (RFC 3156 section 5).
fn multipart_parts(body: &[u8], boundary: &str) -> Vec<Vec<u8>> {
    let delimiter = format!("--{boundary}").into_bytes();
    let mut closing = delimiter.clone();
    closing.extend_from_slice(b"--");
    let mut parts = Vec::new();
    let mut current: Option<Vec<&[u8]>> = None;
    for line in body.split(|b| *b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let mut trimmed = line;
        while let Some(rest) = trimmed
            .strip_suffix(b" ")
            .or_else(|| trimmed.strip_suffix(b"\t"))
        {
            trimmed = rest;
        }
        if trimmed == delimiter.as_slice() || trimmed == closing.as_slice() {
            if let Some(lines) = current.take() {
                parts.push(lines.join(&b"\r\n"[..]));
            }
            if trimmed == closing.as_slice() {
                break;
            }
            current = Some(Vec::new());
        } else if let Some(lines) = current.as_mut() {
            lines.push(line);
        }
    }
    parts
}

/// Opens `raw` when it is OpenPGP-protected, and `None` when it is not.
///
/// `secrets` is called only for encrypted content, so a signed message never
/// reaches the keychain. Signing keys are matched against the From address.
/// `now` is the current time in Unix seconds.
pub fn open_message(
    raw: &[u8],
    keyring: &Keyring,
    secrets: &mut dyn FnMut() -> Result<Vec<SignedSecretKey>>,
    now: u64,
) -> Option<Opened> {
    open(raw, keyring, Some(secrets), now)
}

/// Like [`open_message`], but leaves encrypted content sealed
/// ([`Decryption::Sealed`]): signed mail is still checked, and nothing
/// reaches the secret keys.
pub fn inspect_message(raw: &[u8], keyring: &Keyring, now: u64) -> Option<Opened> {
    open(raw, keyring, None, now)
}

/// A decrypted MIME `entity` as a whole message under the `outer` header
/// fields, with a signed entity inside it unwrapped and checked. `own` are
/// the keys that decrypted it; `headers` gets its header protection.
fn entity_message(
    outer: &[u8],
    entity: &[u8],
    cx: &Context<'_>,
    own: &[Fingerprint],
    signatures: &mut Vec<SignatureCheck>,
    headers: &mut Option<HeaderCheck>,
) -> Zeroizing<Vec<u8>> {
    // A signed entity inside the encryption (RFC 3156 section 6.1); its
    // signed part is then the payload that carries header protection.
    let mut inner = Zeroizing::new(outer.to_vec());
    inner.extend_from_slice(&content_only(entity));
    if let Some(signed) = open_signed(&inner, cx, own) {
        signatures.extend(signed.signatures);
        *headers = signed.headers.or_else(|| header_check(&cx.outer, entity));
        return signed.message;
    }
    *headers = header_check(&cx.outer, entity);
    inner
}

/// Whether decrypted inline content is a MIME entity rather than text: a
/// header section of well-formed fields, one of them `Content-Type`, then a
/// blank line. Gmail delivers PGP/MIME encrypted mail rewritten as a text
/// part holding the armored message, so its plaintext is the PGP/MIME entity,
/// which may start with protected header fields.
fn is_mime_entity(plain: &[u8]) -> bool {
    let Some(end) = header_end(plain).filter(|end| *end > 0) else {
        return false;
    };
    let mut typed = false;
    for line in plain[..end].split_inclusive(|b| *b == b'\n') {
        if line.starts_with(b" ") || line.starts_with(b"\t") {
            continue;
        }
        let Some(colon) = line.iter().position(|b| *b == b':') else {
            return false;
        };
        let name = &line[..colon];
        if name.is_empty() || !name.iter().all(u8::is_ascii_graphic) {
            return false;
        }
        typed |= name.eq_ignore_ascii_case(b"content-type");
    }
    typed
}

const BEGIN_MESSAGE: &str = "-----BEGIN PGP MESSAGE-----";
const END_MESSAGE: &str = "-----END PGP MESSAGE-----";
const BEGIN_SIGNED: &str = "-----BEGIN PGP SIGNED MESSAGE-----";
const END_SIGNATURE: &str = "-----END PGP SIGNATURE-----";

fn open(raw: &[u8], keyring: &Keyring, mut secrets: Secrets<'_>, now: u64) -> Option<Opened> {
    if !looks_protected(raw) {
        return None;
    }
    let parsed = MessageParser::default().parse(raw)?;
    let sender = parsed
        .from()
        .and_then(|from| from.first())
        .and_then(|a| a.address())
        .map(str::to_lowercase);
    let cx = Context {
        keyring,
        sender: sender.as_deref(),
        outer: Fields::of(&parsed),
        now,
    };
    let root = parsed.parts.first()?;
    let outer = outer_headers(raw, root.offset_body as usize);
    let (ctype, subtype, protocol) = content_type(root)?;
    if ctype == "multipart"
        && subtype == "encrypted"
        && protocol.as_deref() == Some("application/pgp-encrypted")
    {
        let PartType::Multipart(children) = &root.body else {
            return None;
        };
        let payload = children
            .iter()
            .filter_map(|i| parsed.parts.get(*i as usize))
            .find(|part| {
                content_type(part)
                    .is_some_and(|(t, s, _)| t == "application" && s == "octet-stream")
            })?;
        let read = read_pgp_message(payload.contents(), true, &cx, &mut secrets);
        let (mut signatures, mut headers) = (read.signatures, None);
        let message = read.plain.map(|entity| {
            entity_message(
                &outer,
                &entity,
                &cx,
                &read.own,
                &mut signatures,
                &mut headers,
            )
        });
        return Some(Opened {
            encrypted: true,
            inline: false,
            unprotected: false,
            decryption: read.decryption,
            signatures,
            headers,
            message,
        });
    }
    if let Some(signed) = open_signed(raw, &cx, &[]) {
        return Some(Opened {
            encrypted: false,
            inline: false,
            unprotected: false,
            decryption: None,
            signatures: signed.signatures,
            headers: signed.headers,
            message: Some(signed.message),
        });
    }
    // Inline PGP: an armored block inside the first plain-text body. Only the
    // block is shown; other text parts and files are outside its protection.
    let text = parsed.body_text(0)?;
    let other_parts = parsed.text_body_count() > 1
        || parsed.attachments().any(|part| {
            // Gmail's rewrite of PGP/MIME keeps the version part as a file.
            !content_type(part).is_some_and(|(t, s, _)| t == "application" && s == "pgp-encrypted")
        });
    let outside = |start: usize, end: usize| {
        !text[..start].trim().is_empty() || !text[end..].trim().is_empty()
    };
    if let Some(start) = text.find(BEGIN_MESSAGE) {
        let end = text[start..]
            .find(END_MESSAGE)
            .map(|e| start + e + END_MESSAGE.len())?;
        let read = read_pgp_message(text[start..end].as_bytes(), true, &cx, &mut secrets);
        let entity = read.plain.as_deref().is_some_and(|p| is_mime_entity(p));
        let (mut signatures, mut headers) = (read.signatures, None);
        let message = read.plain.map(|plain| {
            if entity {
                return entity_message(
                    &outer,
                    &plain,
                    &cx,
                    &read.own,
                    &mut signatures,
                    &mut headers,
                );
            }
            text_message(
                &outer,
                &String::from_utf8_lossy(&plain).replace("\r\n", "\n"),
            )
        });
        // A decrypted entity brings its own files, so only stray text counts.
        let unprotected = message.is_some() && (outside(start, end) || (!entity && other_parts));
        return Some(Opened {
            encrypted: read.decryption.is_some(),
            inline: !entity,
            unprotected,
            decryption: read.decryption,
            signatures,
            headers,
            message,
        });
    }
    if let Some(start) = text.find(BEGIN_SIGNED) {
        let end = text[start..]
            .find(END_SIGNATURE)
            .map(|e| start + e + END_SIGNATURE.len())?;
        let (signed, _) = CleartextSignedMessage::from_string(&text[start..end]).ok()?;
        let normalized = signed.signed_text();
        let signatures = signed
            .signatures()
            .iter()
            .map(|sig| {
                check(sig, &cx, &[], |key| {
                    verify_detached(sig, key, normalized.as_bytes())
                })
            })
            .collect();
        let message = text_message(&outer, &normalized.replace("\r\n", "\n"));
        return Some(Opened {
            encrypted: false,
            inline: true,
            unprotected: outside(start, end) || other_parts,
            decryption: None,
            signatures,
            headers: None,
            message: Some(message),
        });
    }
    None
}

/// A `multipart/signed` message's signature results, its signed entity as
/// a message, and that entity's header protection.
struct Signed {
    signatures: Vec<SignatureCheck>,
    message: Zeroizing<Vec<u8>>,
    headers: Option<HeaderCheck>,
}

/// Checks a `multipart/signed` message and unwraps its signed entity. `own`
/// are the keys that decrypted it, when it was inside encryption.
fn open_signed(raw: &[u8], cx: &Context<'_>, own: &[Fingerprint]) -> Option<Signed> {
    let parsed = MessageParser::default().parse(raw)?;
    let root = parsed.parts.first()?;
    let (ctype, subtype, protocol) = content_type(root)?;
    if ctype != "multipart"
        || subtype != "signed"
        || protocol.as_deref() != Some("application/pgp-signature")
    {
        return None;
    }
    let boundary = root.content_type()?.attribute("boundary")?;
    let body = raw.get(root.offset_body as usize..root.offset_end as usize)?;
    let parts = multipart_parts(body, boundary);
    let (signed, signature) = (parts.first()?, parts.get(1)?);
    let signature_body = signature
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|p| &signature[p + 4..])?;
    let sigs: Vec<DetachedSignature> = DetachedSignature::from_armor_many(signature_body)
        .ok()
        .map(|(sigs, _)| sigs.flatten().collect())
        .unwrap_or_default();
    let signatures = if sigs.is_empty() {
        vec![SignatureCheck {
            status: SignatureStatus::Bad,
            key_id: None,
            fingerprint: None,
            signer: None,
            sender_matches: false,
            created_at: None,
            forwarded: false,
        }]
    } else {
        sigs.iter()
            .map(|sig| {
                check(&sig.signature, cx, own, |key| {
                    verify_detached(&sig.signature, key, &signed[..])
                })
            })
            .collect()
    };
    let mut message = Zeroizing::new(outer_headers(raw, root.offset_body as usize));
    message.extend_from_slice(&content_only(signed));
    Some(Signed {
        signatures,
        message,
        headers: header_check(&cx.outer, signed),
    })
}

/// How a message is protected, judged from its structure alone (no keys).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protection {
    None,
    Signed,
    Encrypted,
}

impl Protection {
    /// The stored code: 0 none, 1 signed, 2 encrypted.
    pub fn code(self) -> i64 {
        match self {
            Protection::None => 0,
            Protection::Signed => 1,
            Protection::Encrypted => 2,
        }
    }
}

/// Whether `parsed` is PGP/MIME encrypted or signed, or carries inline armor
/// in its first text body.
pub fn protection(parsed: &mail_parser::Message<'_>) -> Protection {
    let Some((ctype, subtype, protocol)) = parsed.parts.first().and_then(content_type) else {
        return Protection::None;
    };
    if ctype == "multipart" {
        match (subtype.as_str(), protocol.as_deref()) {
            ("encrypted", Some("application/pgp-encrypted")) => return Protection::Encrypted,
            ("signed", Some("application/pgp-signature")) => return Protection::Signed,
            _ => {}
        }
    }
    match parsed.body_text(0) {
        Some(text) if text.contains("-----BEGIN PGP MESSAGE-----") => Protection::Encrypted,
        Some(text) if text.contains("-----BEGIN PGP SIGNED MESSAGE-----") => Protection::Signed,
        _ => Protection::None,
    }
}

/// Whether an attachment of `parsed` is OpenPGP plumbing rather than a file
/// the sender attached: the version part and encrypted payload directly under
/// a PGP/MIME encrypted message, or the signature directly under a signed
/// one. Files inside the signed content are the sender's, whatever their type.
pub fn is_protection_part(
    parsed: &mail_parser::Message<'_>,
    part: &mail_parser::MessagePart<'_>,
) -> bool {
    let Some(root) = parsed.parts.first() else {
        return false;
    };
    let (PartType::Multipart(children), Some((t, s, p))) = (&root.body, content_type(root)) else {
        return false;
    };
    let Some(index) = parsed
        .parts
        .iter()
        .position(|candidate| std::ptr::eq(candidate, part))
    else {
        return false;
    };
    let Some(position) = children.iter().position(|child| *child as usize == index) else {
        return false;
    };
    let Some((kind, subtype, _)) = content_type(part) else {
        return false;
    };
    if t != "multipart" || kind != "application" {
        return false;
    }
    match (s.as_str(), p.as_deref()) {
        ("encrypted", Some("application/pgp-encrypted")) => {
            matches!(subtype.as_str(), "pgp-encrypted" | "octet-stream")
        }
        ("signed", Some("application/pgp-signature")) => position > 0 && subtype == "pgp-signature",
        _ => false,
    }
}

/// Text for search and previews with inline armor taken out: encrypted
/// blocks are dropped and a clear-signed block keeps only its text.
pub fn readable_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let message = rest.find("-----BEGIN PGP MESSAGE-----");
        let signed = rest.find("-----BEGIN PGP SIGNED MESSAGE-----");
        let (start, end_marker, keep_signed) = match (message, signed) {
            (Some(m), Some(s)) if s < m => (s, "-----END PGP SIGNATURE-----", true),
            (Some(m), _) => (m, "-----END PGP MESSAGE-----", false),
            (None, Some(s)) => (s, "-----END PGP SIGNATURE-----", true),
            (None, None) => break,
        };
        out.push_str(&rest[..start]);
        let block = &rest[start..];
        let Some(end) = block.find(end_marker).map(|e| e + end_marker.len()) else {
            rest = "";
            break;
        };
        if keep_signed {
            // Skip the armor header lines up to the blank line, then keep
            // the dash-unescaped text up to the signature.
            let body = block
                .split_once("\n\n")
                .or_else(|| block.split_once("\r\n\r\n"))
                .map(|(_, b)| b)
                .unwrap_or("");
            let text = body
                .split("-----BEGIN PGP SIGNATURE-----")
                .next()
                .unwrap_or("");
            for line in text.lines() {
                out.push_str(line.strip_prefix("- ").unwrap_or(line));
                out.push('\n');
            }
        }
        rest = &block[end..];
    }
    out.push_str(rest);
    // Where a block was, keep at most one blank line.
    let mut tidy = String::with_capacity(out.len());
    let mut newlines = 0;
    for c in out.replace("\r\n", "\n").trim().chars() {
        newlines = if c == '\n' { newlines + 1 } else { 0 };
        if newlines <= 2 {
            tidy.push(c);
        }
    }
    tidy
}
