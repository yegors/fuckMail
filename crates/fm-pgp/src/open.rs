//! Opening received mail: decrypting PGP/MIME and inline PGP messages, and
//! checking their signatures against the keyring.
//!
//! An opened message is rebuilt as an ordinary RFC 5322 message: the outer
//! header fields with the decrypted (or unwrapped signed) MIME entity in place
//! of the protected body. Callers read it with the same MIME code as any other
//! message, then throw it away; plaintext is never written anywhere here.
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
    packet::Signature,
    types::{KeyDetails, Password, VerifyingKey},
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
    /// The content doesn't match the signature: it was changed after signing.
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
    pub decryption: Option<Decryption>,
    pub signatures: Vec<SignatureCheck>,
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
    let end = entity
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|p| p + 2)
        .or_else(|| entity.windows(2).position(|w| w == b"\n\n").map(|p| p + 1));
    let Some(end) = end else {
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
) -> Option<(&'a SignedPublicKey, pgp::types::Fingerprint, KeyRef<'a>)> {
    let fingerprints = sig.issuer_fingerprint();
    let ids = sig.issuer_key_id();
    let matches = |fingerprint: pgp::types::Fingerprint, id: pgp::types::KeyId| {
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

/// Checks one signature: `verifies` runs the cryptographic check with the
/// signing key once it is found.
fn check(
    sig: &Signature,
    keyring: &Keyring,
    sender: Option<&str>,
    now: u64,
    verifies: impl Fn(KeyRef<'_>) -> bool,
) -> SignatureCheck {
    let key_id = sig
        .issuer_fingerprint()
        .first()
        .map(|f| hex(f.as_bytes()))
        .or_else(|| sig.issuer_key_id().first().map(|id| hex(id.as_ref())));
    let made = created(sig);
    let Some((cert, used, key)) = signer(sig, keyring) else {
        return SignatureCheck {
            status: SignatureStatus::UnknownKey,
            key_id,
            fingerprint: None,
            signer: None,
            sender_matches: false,
            created_at: made,
        };
    };
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
    let status = if !verifies(key) {
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
        sender_matches: sender.is_some_and(|s| addresses.iter().any(|a| a == s)),
        created_at: made,
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
    keyring: &Keyring,
    sender: Option<&str>,
    now: u64,
) -> Vec<SignatureCheck> {
    let Message::Signed { reader, .. } = message else {
        return Vec::new();
    };
    (0..reader.num_signatures())
        .filter_map(|index| {
            let sig = reader.signature(index)?;
            Some(check(sig, keyring, sender, now, |key| {
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

/// Decrypts (or just reads, if only signed) one OpenPGP message.
fn read_pgp_message(
    data: &[u8],
    armored: bool,
    keyring: &Keyring,
    secrets: &mut Secrets<'_>,
    sender: Option<&str>,
    now: u64,
) -> (
    Option<Decryption>,
    Vec<SignatureCheck>,
    Option<Zeroizing<Vec<u8>>>,
) {
    let failed = |message: String| (Some(Decryption::Failed { message }), Vec::new(), None);
    let parsed = if armored {
        Message::from_armor(data).map(|(m, _)| m)
    } else {
        Message::from_bytes(data)
    };
    let Ok(mut message) = parsed else {
        return failed("The encrypted part is damaged".into());
    };
    let mut decryption = None;
    if message.is_encrypted() {
        let to = recipients(&message);
        let Some(secrets) = secrets.as_mut() else {
            return (
                Some(Decryption::Sealed { recipients: to }),
                Vec::new(),
                None,
            );
        };
        let keys = match secrets() {
            Ok(keys) => keys,
            Err(error) => {
                return (
                    Some(Decryption::Unavailable {
                        code: error.code.into(),
                        message: error.message,
                    }),
                    Vec::new(),
                    None,
                )
            }
        };
        if keys.is_empty() {
            return (Some(Decryption::NoKey { recipients: to }), Vec::new(), None);
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
                return (Some(Decryption::NoKey { recipients: to }), Vec::new(), None)
            }
            Err(error) => return failed(format!("It couldn't be decrypted: {error}")),
        };
        decryption = Some(Decryption::Decrypted);
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
    let signatures = message_signatures(&message, keyring, sender, now);
    (decryption, signatures, Some(plain))
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
/// fields, with a signed entity inside it unwrapped and checked.
fn entity_message(
    outer: &[u8],
    entity: &[u8],
    keyring: &Keyring,
    sender: Option<&str>,
    now: u64,
    signatures: &mut Vec<SignatureCheck>,
) -> Zeroizing<Vec<u8>> {
    // A signed entity inside the encryption (RFC 3156 section 6.1).
    let mut inner = Zeroizing::new(outer.to_vec());
    inner.extend_from_slice(&content_only(entity));
    if let Some(opened) = open_signed(&inner, keyring, sender, now) {
        signatures.extend(opened.0);
        return opened.1;
    }
    inner
}

/// Whether decrypted inline content is a MIME entity rather than text: it
/// starts with a `Content-*` or `MIME-Version` field and has a header block.
/// Gmail delivers PGP/MIME encrypted mail rewritten as a text part holding
/// the armored message, so its plaintext is the PGP/MIME entity.
fn is_mime_entity(plain: &[u8]) -> bool {
    let first = plain.split(|b| *b == b'\n').next().unwrap_or_default();
    let lower = first.to_ascii_lowercase();
    (lower.starts_with(b"content-") || lower.starts_with(b"mime-version:"))
        && first.contains(&b':')
        && (plain.windows(4).any(|w| w == b"\r\n\r\n") || plain.windows(2).any(|w| w == b"\n\n"))
}

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
    let sender = sender.as_deref();
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
        let (decryption, mut signatures, plain) =
            read_pgp_message(payload.contents(), true, keyring, &mut secrets, sender, now);
        let message = plain
            .map(|entity| entity_message(&outer, &entity, keyring, sender, now, &mut signatures));
        return Some(Opened {
            encrypted: true,
            inline: false,
            decryption,
            signatures,
            message,
        });
    }
    if let Some((signatures, message)) = open_signed(raw, keyring, sender, now) {
        return Some(Opened {
            encrypted: false,
            inline: false,
            decryption: None,
            signatures,
            message: Some(message),
        });
    }
    // Inline PGP: an armored block inside the first plain-text body.
    let text = parsed.body_text(0)?;
    if let Some(start) = text.find("-----BEGIN PGP MESSAGE-----") {
        let end = text[start..]
            .find("-----END PGP MESSAGE-----")
            .map(|e| start + e + "-----END PGP MESSAGE-----".len())?;
        let (decryption, mut signatures, plain) = read_pgp_message(
            text[start..end].as_bytes(),
            true,
            keyring,
            &mut secrets,
            sender,
            now,
        );
        let entity = plain.as_deref().is_some_and(|p| is_mime_entity(p));
        let message = plain.map(|plain| {
            if entity {
                return entity_message(&outer, &plain, keyring, sender, now, &mut signatures);
            }
            let decrypted = String::from_utf8_lossy(&plain).replace("\r\n", "\n");
            text_message(
                &outer,
                &format!("{}{}{}", &text[..start], decrypted, &text[end..]),
            )
        });
        return Some(Opened {
            encrypted: decryption.is_some(),
            inline: !entity,
            decryption,
            signatures,
            message,
        });
    }
    if let Some(start) = text.find("-----BEGIN PGP SIGNED MESSAGE-----") {
        let tail = &text[start..];
        let end = tail
            .find("-----END PGP SIGNATURE-----")
            .map(|e| start + e + "-----END PGP SIGNATURE-----".len())?;
        let (signed, _) = CleartextSignedMessage::from_string(&text[start..end]).ok()?;
        let normalized = signed.signed_text();
        let signatures = signed
            .signatures()
            .iter()
            .map(|sig| {
                check(sig, keyring, sender, now, |key| {
                    verify_detached(sig, key, normalized.as_bytes())
                })
            })
            .collect();
        let shown = normalized.replace("\r\n", "\n");
        let message = text_message(
            &outer,
            &format!("{}{}{}", &text[..start], shown, &text[end..]),
        );
        return Some(Opened {
            encrypted: false,
            inline: true,
            decryption: None,
            signatures,
            message: Some(message),
        });
    }
    None
}

/// Checks a `multipart/signed` message and unwraps its signed entity.
fn open_signed(
    raw: &[u8],
    keyring: &Keyring,
    sender: Option<&str>,
    now: u64,
) -> Option<(Vec<SignatureCheck>, Zeroizing<Vec<u8>>)> {
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
        }]
    } else {
        sigs.iter()
            .map(|sig| {
                check(&sig.signature, keyring, sender, now, |key| {
                    verify_detached(&sig.signature, key, &signed[..])
                })
            })
            .collect()
    };
    let mut message = Zeroizing::new(outer_headers(raw, root.offset_body as usize));
    message.extend_from_slice(&content_only(signed));
    Some((signatures, message))
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
/// the sender attached: the parts of a PGP/MIME encrypted message, and the
/// signature of a signed one.
pub fn is_protection_part(
    parsed: &mail_parser::Message<'_>,
    part: &mail_parser::MessagePart<'_>,
) -> bool {
    let structural = parsed
        .parts
        .first()
        .and_then(content_type)
        .is_some_and(|(t, s, p)| {
            t == "multipart"
                && ((s == "encrypted" && p.as_deref() == Some("application/pgp-encrypted"))
                    || (s == "signed" && p.as_deref() == Some("application/pgp-signature")))
        });
    structural
        && content_type(part).is_some_and(|(t, s, _)| {
            t == "application"
                && matches!(
                    s.as_str(),
                    "pgp-encrypted" | "pgp-signature" | "octet-stream"
                )
        })
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
