//! Reading key files, and keeping secret keys safe at rest and on export.
//!
//! At rest a secret key is never passphrase-protected; the whole key is sealed
//! with AES-256-GCM under the profile's key-encryption key, which lives in the
//! OS keychain. Sealing binds the fingerprint as associated data, so a sealed
//! key can't be swapped for another key's row. On export the key is protected
//! with a passphrase the person chooses, using the iterated and salted SHA-256
//! S2K at its highest count with AES-256, which GnuPG and every other OpenPGP
//! program reads.
//!
//! Imported certificates keep only what their primary key vouches for, so a
//! key file can't fill the keyring with signatures and packets nothing here
//! uses.
use crate::{cert::issued_by, hex, PgpError, Result, MAX_KEY_BYTES};
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use pgp::{
    armor::{self, BlockType},
    composed::{
        ArmorOptions, Deserializable, DetachedSignature, PublicOrSecret, SignedPublicKey,
        SignedSecretKey,
    },
    crypto::{hash::HashAlgorithm, sym::SymmetricKeyAlgorithm},
    packet::{RevocationCode, Signature, SignatureConfig, SignatureType, Subpacket, SubpacketData},
    ser::Serialize,
    types::{
        KeyDetails, KeyVersion, Password, S2kParams, SecretParams, StringToKey, Tag, Timestamp,
    },
};
use rand::RngCore;
use zeroize::Zeroizing;

/// Length of the key-encryption key, in bytes.
pub const KEK_LEN: usize = 32;
/// Shortest passphrase accepted for an exported secret key.
pub const MIN_EXPORT_PASSPHRASE: usize = 12;

/// The iterated S2K's highest count: 65,011,712 bytes of SHA-256 per guess.
pub const EXPORT_S2K_COUNT: u8 = 255;

const SEAL_MAGIC: &[u8] = b"FMPGP1";
const SEAL_CONTEXT: &str = "fuckmail-pgp-secret-v1";
const NONCE_LEN: usize = 12;

/// One key found in an imported file.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)] // Short-lived, one per key in a file.
pub enum ParsedKey {
    Public(SignedPublicKey),
    Secret(SignedSecretKey),
    /// A standalone key revocation, as in a revocation certificate.
    Revocation(Signature),
}

fn invalid_key(message: &str) -> PgpError {
    PgpError::new("invalid_key", message)
}

fn internal(error: pgp::errors::Error) -> PgpError {
    PgpError::new("internal", format!("OpenPGP failed: {error}"))
}

/// A fresh random key-encryption key.
pub fn new_kek() -> Zeroizing<[u8; KEK_LEN]> {
    let mut kek = Zeroizing::new([0u8; KEK_LEN]);
    rand::rngs::OsRng.fill_bytes(kek.as_mut());
    kek
}

/// The armored blocks in `text`, each from its BEGIN line to its END line.
fn armor_blocks(text: &str) -> Vec<&str> {
    let mut blocks = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("-----BEGIN PGP ") {
        let block = &rest[start..];
        let Some(end) = block.find("-----END PGP ") else {
            break;
        };
        let tail = &block[end..];
        let close = tail[5..]
            .find("-----")
            .map(|i| i + 10)
            .unwrap_or(tail.len());
        blocks.push(&block[..end + close]);
        rest = &block[end + close..];
    }
    blocks
}

fn parse_packets(bytes: &[u8], out: &mut Vec<ParsedKey>) -> Result<()> {
    let keys = PublicOrSecret::from_bytes_many(bytes)
        .map_err(|_| invalid_key("This isn't an OpenPGP key"))?;
    let before = out.len();
    for key in keys {
        match key {
            Ok(PublicOrSecret::Public(key)) => out.push(ParsedKey::Public(key)),
            Ok(PublicOrSecret::Secret(key)) => out.push(ParsedKey::Secret(key)),
            Err(_) => {}
        }
    }
    if out.len() == before {
        // A revocation certificate is a bare key revocation signature.
        for sig in DetachedSignature::from_bytes_many(bytes)
            .map_err(|_| invalid_key("This isn't an OpenPGP key"))?
            .flatten()
        {
            if sig.signature.typ() == Some(SignatureType::KeyRevocation) {
                out.push(ParsedKey::Revocation(sig.signature));
            }
        }
    }
    Ok(())
}

/// Reads every key in `input`: one or more armored blocks (with any text
/// around them) or binary packets. Fails with `invalid_key` when nothing in it
/// is a key, and `limit_exceeded` above [`MAX_KEY_BYTES`].
pub fn parse_keys(input: &[u8]) -> Result<Vec<ParsedKey>> {
    if input.len() > MAX_KEY_BYTES {
        return Err(PgpError::new(
            "limit_exceeded",
            "Key files can be at most 1 MiB",
        ));
    }
    let mut keys = Vec::new();
    match std::str::from_utf8(input) {
        Ok(text) if text.contains("-----BEGIN PGP ") => {
            for block in armor_blocks(text) {
                let mut dearmor = armor::Dearmor::new(block.as_bytes());
                let mut bytes = Vec::new();
                if std::io::Read::read_to_end(&mut dearmor, &mut bytes).is_err() {
                    continue;
                }
                if matches!(
                    dearmor.typ,
                    Some(BlockType::PublicKey | BlockType::PrivateKey | BlockType::Signature)
                ) {
                    parse_packets(&bytes, &mut keys)?;
                }
            }
        }
        _ => parse_packets(input, &mut keys)?,
    }
    if keys.is_empty() {
        return Err(invalid_key("No OpenPGP key was found in this file"));
    }
    Ok(keys)
}

/// Whether any secret part of `key` is passphrase-protected.
pub fn is_protected(key: &SignedSecretKey) -> bool {
    matches!(key.primary_key.secret_params(), SecretParams::Encrypted(_))
        || key
            .secret_subkeys
            .iter()
            .any(|sub| matches!(sub.key.secret_params(), SecretParams::Encrypted(_)))
}

/// Removes the passphrase protection from every secret part. Fails with
/// `passphrase_required` when the key is protected and no passphrase was
/// given, `wrong_passphrase` when it doesn't unlock, and
/// `secret_key_incomplete` for a key whose primary secret is kept elsewhere
/// (as GnuPG exports with only subkeys).
pub fn unlock(key: &SignedSecretKey, passphrase: Option<&str>) -> Result<SignedSecretKey> {
    if !is_protected(key) {
        return Ok(key.clone());
    }
    let Some(passphrase) = passphrase.filter(|p| !p.is_empty()) else {
        return Err(PgpError::new(
            "passphrase_required",
            "This secret key is protected. Enter its passphrase.",
        ));
    };
    let password: Password = passphrase.into();
    let mut key = key.clone();
    let wrong = |error: pgp::errors::Error| {
        let text = error.to_string().to_lowercase();
        if text.contains("s2k") || text.contains("unsupported") || text.contains("unknown") {
            PgpError::new(
                "secret_key_incomplete",
                "This file holds only part of the secret key. Export the full key and import that.",
            )
        } else {
            PgpError::new(
                "wrong_passphrase",
                "That passphrase doesn't unlock this key",
            )
        }
    };
    if matches!(key.primary_key.secret_params(), SecretParams::Encrypted(_)) {
        key.primary_key.remove_password(&password).map_err(wrong)?;
    }
    for sub in &mut key.secret_subkeys {
        if matches!(sub.key.secret_params(), SecretParams::Encrypted(_)) {
            sub.key.remove_password(&password).map_err(wrong)?;
        }
    }
    Ok(key)
}

fn aad(fingerprint: &str) -> Vec<u8> {
    format!("{SEAL_CONTEXT}\0{}", fingerprint.to_uppercase()).into_bytes()
}

/// Seals an unprotected secret key under `kek`.
pub fn seal(key: &SignedSecretKey, kek: &[u8; KEK_LEN]) -> Result<Vec<u8>> {
    if is_protected(key) {
        return Err(PgpError::new(
            "internal",
            "A protected key can't be sealed; unlock it first",
        ));
    }
    let plain = Zeroizing::new(key.to_bytes().map_err(internal)?);
    let fingerprint = hex(key.fingerprint().as_bytes());
    let mut nonce = [0u8; NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let cipher =
        Aes256Gcm::new_from_slice(kek).map_err(|_| PgpError::new("internal", "Bad key length"))?;
    let sealed = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &plain,
                aad: &aad(&fingerprint),
            },
        )
        .map_err(|_| PgpError::new("internal", "The key couldn't be sealed"))?;
    let mut out = Vec::with_capacity(SEAL_MAGIC.len() + NONCE_LEN + sealed.len());
    out.extend_from_slice(SEAL_MAGIC);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// Opens a key sealed by [`seal`] for `fingerprint`. Fails with `key_locked`
/// when `kek` is not the one it was sealed with or the blob was altered.
pub fn unseal(blob: &[u8], fingerprint: &str, kek: &[u8; KEK_LEN]) -> Result<SignedSecretKey> {
    let locked = || {
        PgpError::new(
            "key_locked",
            "This secret key can't be opened with the key in the system keychain",
        )
    };
    let body = blob.strip_prefix(SEAL_MAGIC).ok_or_else(locked)?;
    if body.len() < NONCE_LEN + 16 {
        return Err(locked());
    }
    let (nonce, sealed) = body.split_at(NONCE_LEN);
    let cipher = Aes256Gcm::new_from_slice(kek).map_err(|_| locked())?;
    let plain = Zeroizing::new(
        cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: sealed,
                    aad: &aad(fingerprint),
                },
            )
            .map_err(|_| locked())?,
    );
    let key = SignedSecretKey::from_bytes(&plain[..]).map_err(|_| locked())?;
    if !hex(key.fingerprint().as_bytes()).eq_ignore_ascii_case(fingerprint) {
        return Err(locked());
    }
    Ok(key)
}

fn armored(source: &impl Serialize, typ: BlockType) -> Result<String> {
    let mut out = Vec::new();
    armor::write(source, typ, &mut out, None, true).map_err(internal)?;
    String::from_utf8(out).map_err(|_| PgpError::new("internal", "Armor is not text"))
}

/// The public key, ASCII-armored, to give to correspondents.
pub fn export_public(cert: &SignedPublicKey) -> Result<String> {
    cert.to_armored_string(ArmorOptions::default())
        .map_err(internal)
}

/// How an exported secret key part is protected. Version 4 keys use AES-256
/// with the iterated and salted SHA-256 S2K at [`EXPORT_S2K_COUNT`], which
/// every OpenPGP program reads; version 6 keys keep rpgp's Argon2 default.
fn export_s2k(version: KeyVersion) -> S2kParams {
    let mut rng = rand::rngs::OsRng;
    if version == KeyVersion::V6 {
        return S2kParams::new_default(rng, version);
    }
    let sym_alg = SymmetricKeyAlgorithm::AES256;
    let mut iv = vec![0u8; sym_alg.block_size()];
    rng.fill_bytes(&mut iv);
    S2kParams::Cfb {
        sym_alg,
        s2k: StringToKey::new_iterated(rng, HashAlgorithm::Sha256, EXPORT_S2K_COUNT),
        iv: iv.into(),
    }
}

/// The secret key, ASCII-armored and protected with `passphrase`, for a
/// backup or another program. Fails with `weak_passphrase` below
/// [`MIN_EXPORT_PASSPHRASE`] characters.
pub fn export_secret(key: &SignedSecretKey, passphrase: &str) -> Result<Zeroizing<String>> {
    if passphrase.chars().count() < MIN_EXPORT_PASSPHRASE {
        return Err(PgpError::new(
            "weak_passphrase",
            format!("Use a passphrase of at least {MIN_EXPORT_PASSPHRASE} characters"),
        ));
    }
    let mut key = key.clone();
    let password: Password = passphrase.into();
    let version = key.primary_key.version();
    key.primary_key
        .set_password_with_s2k(&password, export_s2k(version))
        .map_err(internal)?;
    for sub in &mut key.secret_subkeys {
        let version = sub.key.version();
        sub.key
            .set_password_with_s2k(&password, export_s2k(version))
            .map_err(internal)?;
    }
    Ok(Zeroizing::new(
        key.to_armored_string(ArmorOptions::default())
            .map_err(internal)?,
    ))
}

/// A revocation certificate for `key`: import it into any OpenPGP program, or
/// send it to correspondents, to mark the key as no longer in use. It is
/// armored as a public key block, as GnuPG expects.
pub fn export_revocation(key: &SignedSecretKey, now: u64) -> Result<String> {
    let mut rng = rand::rngs::OsRng;
    let mut config =
        SignatureConfig::from_key(&mut rng, &key.primary_key, SignatureType::KeyRevocation)
            .map_err(internal)?;
    let made = Timestamp::from_secs(
        u32::try_from(now).map_err(|_| PgpError::new("internal", "The clock is out of range"))?,
    );
    config.hashed_subpackets = vec![
        Subpacket::regular(SubpacketData::SignatureCreationTime(made)).map_err(internal)?,
        Subpacket::regular(SubpacketData::IssuerFingerprint(
            key.primary_key.fingerprint(),
        ))
        .map_err(internal)?,
        Subpacket::regular(SubpacketData::RevocationReason(
            RevocationCode::NoReason,
            "".into(),
        ))
        .map_err(internal)?,
    ];
    if key.primary_key.version() <= pgp::types::KeyVersion::V4 {
        config.unhashed_subpackets =
            vec![
                Subpacket::regular(SubpacketData::IssuerKeyId(key.primary_key.legacy_key_id()))
                    .map_err(internal)?,
            ];
    }
    let sig = config
        .sign_key(
            &key.primary_key,
            &Password::empty(),
            key.primary_key.public_key(),
        )
        .map_err(internal)?;
    armored(&DetachedSignature::new(sig), BlockType::PublicKey)
}

/// Adds the valid key revocations among `sigs` to `cert`. Returns whether any
/// was new.
pub fn revocations(cert: &mut SignedPublicKey, sigs: &[Signature]) -> bool {
    let mut added = false;
    for sig in sigs {
        if sig.typ() == Some(SignatureType::KeyRevocation)
            && sig.verify_key(&cert.primary_key).is_ok()
            && !cert.details.revocation_signatures.contains(sig)
        {
            cert.details.revocation_signatures.push(sig.clone());
            added = true;
        }
    }
    added
}

/// `existing` with everything new in `incoming` added: user IDs, subkeys and
/// their signatures, direct-key signatures and revocations. Both must be the
/// same primary key. The result keeps only what [`clean`] keeps.
pub fn merge(existing: &SignedPublicKey, incoming: &SignedPublicKey) -> SignedPublicKey {
    clean(&merge_all(existing, incoming))
}

/// Keeps only the packets `cert`'s primary key vouches for: its own key
/// revocations and direct-key signatures, user IDs and attributes with their
/// self-signatures and certification revocations, and subkeys with their
/// bindings and revocations, each signature verified. Third-party
/// certifications, which nothing here uses, and anything that doesn't
/// verify are dropped, so an import can't grow a stored key without bound.
pub fn clean(cert: &SignedPublicKey) -> SignedPublicKey {
    let primary = &cert.primary_key;
    let mut out = cert.clone();
    out.details.revocation_signatures.retain(|sig| {
        sig.typ() == Some(SignatureType::KeyRevocation)
            && issued_by(sig, primary)
            && sig.verify_key(primary).is_ok()
    });
    out.details.direct_signatures.retain(|sig| {
        sig.typ() == Some(SignatureType::Key)
            && issued_by(sig, primary)
            && sig.verify_key(primary).is_ok()
    });
    for user in &mut out.details.users {
        let id = user.id.clone();
        user.signatures.retain(|sig| {
            issued_by(sig, primary) && sig.verify_certification(primary, Tag::UserId, &id).is_ok()
        });
    }
    out.details.users.retain(|user| !user.signatures.is_empty());
    for attribute in &mut out.details.user_attributes {
        let attr = attribute.attr.clone();
        attribute.signatures.retain(|sig| {
            issued_by(sig, primary)
                && sig
                    .verify_certification(primary, Tag::UserAttribute, &attr)
                    .is_ok()
        });
    }
    out.details
        .user_attributes
        .retain(|attribute| !attribute.signatures.is_empty());
    for sub in &mut out.public_subkeys {
        let key = sub.key.clone();
        sub.signatures.retain(|sig| {
            issued_by(sig, primary) && sig.verify_subkey_binding(primary, &key).is_ok()
        });
    }
    out.public_subkeys.retain(|sub| !sub.signatures.is_empty());
    out
}

fn merge_all(existing: &SignedPublicKey, incoming: &SignedPublicKey) -> SignedPublicKey {
    let mut merged = existing.clone();
    let add = |into: &mut Vec<Signature>, from: &[Signature]| {
        for sig in from {
            if !into.contains(sig) {
                into.push(sig.clone());
            }
        }
    };
    add(
        &mut merged.details.revocation_signatures,
        &incoming.details.revocation_signatures,
    );
    add(
        &mut merged.details.direct_signatures,
        &incoming.details.direct_signatures,
    );
    for user in &incoming.details.users {
        match merged.details.users.iter_mut().find(|u| u.id == user.id) {
            Some(existing) => add(&mut existing.signatures, &user.signatures),
            None => merged.details.users.push(user.clone()),
        }
    }
    for attribute in &incoming.details.user_attributes {
        match merged
            .details
            .user_attributes
            .iter_mut()
            .find(|a| a.attr == attribute.attr)
        {
            Some(existing) => add(&mut existing.signatures, &attribute.signatures),
            None => merged.details.user_attributes.push(attribute.clone()),
        }
    }
    for sub in &incoming.public_subkeys {
        let fingerprint = sub.key.fingerprint();
        match merged
            .public_subkeys
            .iter_mut()
            .find(|s| s.key.fingerprint() == fingerprint)
        {
            Some(existing) => add(&mut existing.signatures, &sub.signatures),
            None => merged.public_subkeys.push(sub.clone()),
        }
    }
    merged
}

/// The secret key material of `secret` with the user IDs, subkey bindings and
/// revocations of `public`, which is kept more up to date (imports merge into
/// it). Subkeys `secret` has no secret for stay public.
pub fn combine(public: &SignedPublicKey, secret: &SignedSecretKey) -> SignedSecretKey {
    let mut public_subkeys = Vec::new();
    let mut secret_subkeys = Vec::new();
    for sub in &public.public_subkeys {
        let fingerprint = sub.key.fingerprint();
        match secret
            .secret_subkeys
            .iter()
            .find(|s| s.key.fingerprint() == fingerprint)
        {
            Some(own) => secret_subkeys.push(pgp::composed::SignedSecretSubKey {
                key: own.key.clone(),
                signatures: sub.signatures.clone(),
            }),
            None => public_subkeys.push(sub.clone()),
        }
    }
    SignedSecretKey {
        primary_key: secret.primary_key.clone(),
        details: public.details.clone(),
        public_subkeys,
        secret_subkeys,
    }
}
