//! Signing and encrypting an outgoing RFC 5322 message as PGP/MIME (RFC 3156).
//!
//! The message's own `Content-*` header fields and body become the protected
//! MIME entity; every other header (From, To, Subject, Date, Message-ID,
//! threading) stays outside, so Gmail can thread and deliver it. The Subject
//! is therefore not encrypted. The entity must already be 7-bit with CRLF line
//! endings, as the composer's MIME builder writes it (every part base64).
use crate::{
    cert::{encryption_keys, KeyRef, SecretRef},
    PgpError, Result,
};
use pgp::{
    composed::{ArmorOptions, DetachedSignature, MessageBuilder, SignedPublicKey},
    crypto::{hash::HashAlgorithm, sym::SymmetricKeyAlgorithm},
    types::Password,
};
use rand::RngCore;

/// One recipient of an encrypted message.
pub struct Recipient<'a> {
    pub cert: &'a SignedPublicKey,
    /// A Bcc recipient: their key ID is left out of the message, so other
    /// recipients can't tell who else could read it.
    pub hidden: bool,
}

fn internal(error: pgp::errors::Error) -> PgpError {
    PgpError::new("internal", format!("OpenPGP failed: {error}"))
}

/// Splits a message into its outer header fields and its MIME entity (the
/// `Content-*` fields, a blank line and the body).
fn split(raw: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    let invalid = || PgpError::new("invalid_message", "The message has no header block");
    let end = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(invalid)?;
    let (head, body) = (&raw[..end + 2], &raw[end + 4..]);
    if head
        .iter()
        .enumerate()
        .any(|(i, b)| *b == b'\n' && (i == 0 || head[i - 1] != b'\r'))
    {
        return Err(PgpError::new(
            "invalid_message",
            "The message must use CRLF line endings",
        ));
    }
    let mut outer = Vec::new();
    let mut entity = Vec::new();
    let mut content = false;
    for line in head.split_inclusive(|b| *b == b'\n') {
        if !line.starts_with(b" ") && !line.starts_with(b"\t") {
            content = line.len() >= 8 && line[..8].eq_ignore_ascii_case(b"content-");
        }
        if content {
            entity.extend_from_slice(line);
        } else {
            outer.extend_from_slice(line);
        }
    }
    if entity.is_empty() {
        entity.extend_from_slice(b"Content-Type: text/plain; charset=us-ascii\r\n");
    }
    entity.extend_from_slice(b"\r\n");
    // RFC 3156 signs and encrypts the canonical form: every line ends in CRLF.
    let mut previous = 0u8;
    for &byte in body {
        if byte == b'\n' && previous != b'\r' {
            entity.push(b'\r');
        }
        entity.push(byte);
        previous = byte;
    }
    Ok((outer, entity))
}

/// A boundary that doesn't occur in `entity`.
fn boundary(kind: &str, entity: &[u8]) -> String {
    loop {
        let mut bytes = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        let candidate = format!("fm-{kind}-{}", crate::hex(&bytes).to_lowercase());
        if !entity
            .windows(candidate.len())
            .any(|w| w == candidate.as_bytes())
        {
            return candidate;
        }
    }
}

fn crlf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

/// Signs `raw` with `key` as `multipart/signed`, SHA-512 (`micalg=pgp-sha512`).
pub fn sign_message(raw: &[u8], key: SecretRef<'_>) -> Result<Vec<u8>> {
    let (outer, entity) = split(raw)?;
    let rng = rand::rngs::OsRng;
    let password = Password::empty();
    let signature = match key {
        SecretRef::Primary(key) => DetachedSignature::sign_binary_data(
            rng,
            key,
            &password,
            HashAlgorithm::Sha512,
            &entity[..],
        ),
        SecretRef::Sub(key) => DetachedSignature::sign_binary_data(
            rng,
            key,
            &password,
            HashAlgorithm::Sha512,
            &entity[..],
        ),
    }
    .map_err(internal)?;
    let armored = crlf(
        &signature
            .to_armored_string(ArmorOptions::default())
            .map_err(internal)?,
    );
    let b = boundary("signed", &entity);
    let mut out = outer;
    out.extend_from_slice(
        format!(
            "Content-Type: multipart/signed; micalg=pgp-sha512;\r\n protocol=\"application/pgp-signature\";\r\n boundary=\"{b}\"\r\n\r\nThis is an OpenPGP/MIME signed message (RFC 4880 and 3156)\r\n--{b}\r\n"
        )
        .as_bytes(),
    );
    out.extend_from_slice(&entity);
    out.extend_from_slice(
        format!(
            "\r\n--{b}\r\nContent-Type: application/pgp-signature; name=\"signature.asc\"\r\nContent-Description: OpenPGP digital signature\r\nContent-Disposition: attachment; filename=\"signature.asc\"\r\n\r\n{}\r\n--{b}--\r\n",
            armored.trim_end()
        )
        .as_bytes(),
    );
    Ok(out)
}

/// AES-256 unless some recipient lists preferences without it; then AES-128,
/// which every implementation must support.
fn cipher(recipients: &[Recipient<'_>], now: u64) -> SymmetricKeyAlgorithm {
    let all_take_aes256 = recipients.iter().all(|recipient| {
        let evaluation = crate::cert::evaluate(recipient.cert, now);
        let preferred = evaluation
            .binding
            .map(|sig| sig.preferred_symmetric_algs().to_vec())
            .unwrap_or_default();
        preferred.is_empty() || preferred.contains(&SymmetricKeyAlgorithm::AES256)
    });
    if all_take_aes256 {
        SymmetricKeyAlgorithm::AES256
    } else {
        SymmetricKeyAlgorithm::AES128
    }
}

/// Encrypts `raw` to every recipient's valid encryption keys as
/// `multipart/encrypted`, signing it inside the encryption when `signer` is
/// given. Fails with `recipient_key_unusable` when a recipient has no key that
/// can encrypt now; nothing is ever sent unencrypted instead.
pub fn encrypt_message(
    raw: &[u8],
    recipients: &[Recipient<'_>],
    signer: Option<SecretRef<'_>>,
    now: u64,
) -> Result<Vec<u8>> {
    if recipients.is_empty() {
        return Err(PgpError::new(
            "invalid_request",
            "An encrypted message needs at least one recipient",
        ));
    }
    let (outer, entity) = split(raw)?;
    let mut rng = rand::rngs::OsRng;
    let algorithm = cipher(recipients, now);
    let b = boundary("encrypted", &entity);
    let mut builder = MessageBuilder::from_bytes("", entity).seipd_v1(&mut rng, algorithm);
    for recipient in recipients {
        let keys = encryption_keys(recipient.cert, now);
        if keys.is_empty() {
            return Err(PgpError::new(
                "recipient_key_unusable",
                "A recipient's key can't be used to encrypt",
            ));
        }
        for key in keys {
            match (key, recipient.hidden) {
                (KeyRef::Primary(key), false) => builder.encrypt_to_key(&mut rng, key),
                (KeyRef::Primary(key), true) => builder.encrypt_to_key_anonymous(&mut rng, key),
                (KeyRef::Sub(key), false) => builder.encrypt_to_key(&mut rng, key),
                (KeyRef::Sub(key), true) => builder.encrypt_to_key_anonymous(&mut rng, key),
            }
            .map_err(internal)?;
        }
    }
    match signer {
        Some(SecretRef::Primary(key)) => {
            builder.sign(key, Password::empty(), HashAlgorithm::Sha512);
        }
        Some(SecretRef::Sub(key)) => {
            builder.sign(key, Password::empty(), HashAlgorithm::Sha512);
        }
        None => {}
    }
    let armored = crlf(
        &builder
            .to_armored_string(rng, ArmorOptions::default())
            .map_err(internal)?,
    );
    let mut out = outer;
    out.extend_from_slice(
        format!(
            "Content-Type: multipart/encrypted;\r\n protocol=\"application/pgp-encrypted\";\r\n boundary=\"{b}\"\r\n\r\nThis is an OpenPGP/MIME encrypted message (RFC 4880 and 3156)\r\n--{b}\r\nContent-Type: application/pgp-encrypted\r\nContent-Description: PGP/MIME version identification\r\n\r\nVersion: 1\r\n\r\n--{b}\r\nContent-Type: application/octet-stream; name=\"encrypted.asc\"\r\nContent-Description: OpenPGP encrypted message\r\nContent-Disposition: inline; filename=\"encrypted.asc\"\r\n\r\n{}\r\n--{b}--\r\n",
            armored.trim_end()
        )
        .as_bytes(),
    );
    Ok(out)
}
