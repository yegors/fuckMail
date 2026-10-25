//! New keys and expiry changes.
//!
//! New keys are version 4, which every current OpenPGP program reads
//! (GnuPG does not read version 6). The default is an Ed25519 primary key
//! that certifies and signs with a Curve25519 encryption subkey, as GnuPG,
//! Thunderbird and Proton create. RSA 4096 is for older software. Both prefer
//! AES-256 and SHA-512 and announce MDC (SEIPDv1) support.
use crate::{PgpError, Result};
use pgp::{
    composed::{
        EncryptionCaps, KeyType, SecretKeyParamsBuilder, SignedSecretKey, SubkeyParamsBuilder,
    },
    crypto::{ecc_curve::ECCCurve, hash::HashAlgorithm, sym::SymmetricKeyAlgorithm},
    packet::{KeyFlags, SignatureConfig, SignatureType, Subpacket, SubpacketData},
    types::{CompressionAlgorithm, Duration, KeyDetails, Password, Tag, Timestamp},
};
use smallvec::smallvec;

/// The key algorithm for a new key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algorithm {
    /// Ed25519 signing and certification with a Curve25519 encryption subkey.
    Curve25519,
    /// RSA 4096 for both, for software that predates elliptic-curve keys.
    Rsa4096,
}

/// What a new key is for.
pub struct NewKey {
    /// The display name in the user ID; may be empty.
    pub name: String,
    pub email: String,
    pub algorithm: Algorithm,
    /// Seconds from now until the key expires, or `None` for never.
    pub expires_in: Option<u64>,
}

fn invalid(message: &str) -> PgpError {
    PgpError::new("invalid_request", message)
}

fn internal(error: pgp::errors::Error) -> PgpError {
    PgpError::new("internal", format!("OpenPGP failed: {error}"))
}

/// The user ID `Name <email>`, or `<email>` without a name.
fn user_id(name: &str, email: &str) -> Result<String> {
    let name = name.trim();
    if name.chars().count() > 80
        || name
            .chars()
            .any(|c| c.is_control() || matches!(c, '<' | '>' | '"'))
    {
        return Err(invalid(
            "The name must be at most 80 characters, without <, > or quotes",
        ));
    }
    let email = email.trim();
    if email.len() > 254 || crate::cert::address(email).is_none_or(|a| a != email.to_lowercase()) {
        return Err(invalid("Enter a valid email address"));
    }
    Ok(if name.is_empty() {
        format!("<{email}>")
    } else {
        format!("{name} <{email}>")
    })
}

/// Expiry durations are 32-bit seconds from the key's creation.
fn duration(created: u64, expires_in: Option<u64>, now: u64) -> Result<Option<Duration>> {
    let Some(expires_in) = expires_in else {
        return Ok(None);
    };
    if expires_in < 86_400 {
        return Err(invalid("A key must stay valid for at least a day"));
    }
    let seconds = now
        .saturating_add(expires_in)
        .checked_sub(created)
        .and_then(|s| u32::try_from(s).ok())
        .ok_or_else(|| invalid("That expiry date is too far away"))?;
    Ok(Some(Duration::from_secs(seconds)))
}

/// Generates a new secret key. Its secret parts are not passphrase-protected;
/// seal it with [`crate::seal`] before storing it.
pub fn generate(request: &NewKey) -> Result<SignedSecretKey> {
    let uid = user_id(&request.name, &request.email)?;
    let (primary, encryption) = match request.algorithm {
        Algorithm::Curve25519 => (
            KeyType::Ed25519Legacy,
            KeyType::ECDH(ECCCurve::Curve25519Legacy),
        ),
        Algorithm::Rsa4096 => (KeyType::Rsa(4096), KeyType::Rsa(4096)),
    };
    let mut subkey = SubkeyParamsBuilder::default();
    subkey.key_type(encryption).can_encrypt(EncryptionCaps::All);
    let mut params = SecretKeyParamsBuilder::default();
    params
        .key_type(primary)
        .can_certify(true)
        .can_sign(true)
        .primary_user_id(uid)
        .preferred_symmetric_algorithms(smallvec![
            SymmetricKeyAlgorithm::AES256,
            SymmetricKeyAlgorithm::AES192,
            SymmetricKeyAlgorithm::AES128,
        ])
        .preferred_hash_algorithms(smallvec![
            HashAlgorithm::Sha512,
            HashAlgorithm::Sha384,
            HashAlgorithm::Sha256,
            HashAlgorithm::Sha224,
        ])
        .preferred_compression_algorithms(smallvec![
            CompressionAlgorithm::ZLIB,
            CompressionAlgorithm::BZip2,
            CompressionAlgorithm::ZIP,
            CompressionAlgorithm::Uncompressed,
        ])
        .subkey(subkey.build().map_err(|e| invalid(&e.to_string()))?);
    let params = params.build().map_err(|e| invalid(&e.to_string()))?;
    let key = params.generate(rand::rngs::OsRng).map_err(internal)?;
    let created = u64::from(key.primary_key.created_at().as_secs());
    set_expiry(&key, request.expires_in, created)
}

/// Re-signs `key`'s user IDs and subkey bindings so the whole key expires
/// `expires_in` seconds after `now`, or never with `None`. Other self-signature
/// contents (key flags, preferences, features) are kept. The secret parts must
/// not be passphrase-protected.
pub fn set_expiry(
    key: &SignedSecretKey,
    expires_in: Option<u64>,
    now: u64,
) -> Result<SignedSecretKey> {
    let mut key = key.clone();
    let mut rng = rand::rngs::OsRng;
    let password = Password::empty();
    let created = u64::from(key.primary_key.created_at().as_secs());
    let lifetime = duration(created, expires_in, now)?;
    let made = Timestamp::from_secs(
        u32::try_from(now.max(created)).map_err(|_| invalid("The clock is out of range"))?,
    );
    let public = key.to_public_key();
    let current = crate::cert::evaluate(&public, now);
    let primary_id = current.users.first().map(|(id, _)| id.clone());
    let primary_key = key.primary_key.clone();
    let issuer = |config: &mut SignatureConfig| -> Result<()> {
        if primary_key.version() <= pgp::types::KeyVersion::V4 {
            config.unhashed_subpackets =
                vec![
                    Subpacket::regular(SubpacketData::IssuerKeyId(primary_key.legacy_key_id()))
                        .map_err(internal)?,
                ];
        }
        Ok(())
    };
    for user in &mut key.details.users {
        let Some(text) = user.id.as_str().map(str::to_owned) else {
            continue;
        };
        // Only user IDs that are currently bound are renewed.
        let Some((_, old)) = current.users.iter().find(|(id, _)| *id == text) else {
            continue;
        };
        let mut config =
            SignatureConfig::from_key(&mut rng, &primary_key, SignatureType::CertPositive)
                .map_err(internal)?;
        let mut hashed =
            vec![Subpacket::regular(SubpacketData::SignatureCreationTime(made)).map_err(internal)?];
        for packet in old
            .config()
            .map(|c| c.hashed_subpackets())
            .into_iter()
            .flatten()
        {
            match packet.data {
                SubpacketData::SignatureCreationTime(_)
                | SubpacketData::KeyExpirationTime(_)
                | SubpacketData::SignatureExpirationTime(_)
                | SubpacketData::IsPrimary(_) => {}
                _ => hashed.push(packet.clone()),
            }
        }
        if !hashed
            .iter()
            .any(|p| matches!(p.data, SubpacketData::IssuerFingerprint(_)))
        {
            hashed.push(
                Subpacket::regular(SubpacketData::IssuerFingerprint(primary_key.fingerprint()))
                    .map_err(internal)?,
            );
        }
        if Some(&text) == primary_id.as_ref() {
            hashed.push(Subpacket::regular(SubpacketData::IsPrimary(true)).map_err(internal)?);
        }
        if let Some(lifetime) = lifetime {
            hashed.push(
                Subpacket::regular(SubpacketData::KeyExpirationTime(lifetime)).map_err(internal)?,
            );
        }
        config.hashed_subpackets = hashed;
        issuer(&mut config)?;
        let sig = config
            .sign_certification(
                &primary_key,
                primary_key.public_key(),
                &password,
                Tag::UserId,
                &user.id,
            )
            .map_err(internal)?;
        // Replace the earlier self-signatures; others' certifications and
        // revocations stay.
        user.signatures
            .retain(|old| !is_self_certification(old, &primary_key));
        user.signatures.push(sig);
    }
    let subkeys = crate::cert::evaluate_subkeys(&public, now);
    for sub in &mut key.secret_subkeys {
        let fingerprint = sub.key.fingerprint();
        let Some(evaluated) = subkeys
            .iter()
            .find(|s| s.key.key.fingerprint() == fingerprint && s.bound && !s.revoked)
        else {
            continue;
        };
        let old = evaluated
            .key
            .signatures
            .iter()
            .filter(|sig| sig.typ() == Some(SignatureType::SubkeyBinding))
            .max_by_key(|sig| crate::cert::created(sig));
        let mut config =
            SignatureConfig::from_key(&mut rng, &primary_key, SignatureType::SubkeyBinding)
                .map_err(internal)?;
        let mut hashed = vec![
            Subpacket::regular(SubpacketData::SignatureCreationTime(made)).map_err(internal)?,
            Subpacket::regular(SubpacketData::IssuerFingerprint(primary_key.fingerprint()))
                .map_err(internal)?,
        ];
        let flags = match old.filter(|s| crate::cert::has_key_flags(s)) {
            Some(old) => old.key_flags(),
            None => {
                let mut flags = KeyFlags::default();
                flags.set_sign(evaluated.usage.sign);
                flags.set_encrypt_comms(evaluated.usage.encrypt);
                flags.set_encrypt_storage(evaluated.usage.encrypt);
                flags.set_authentication(evaluated.usage.authenticate);
                flags
            }
        };
        hashed.push(Subpacket::regular(SubpacketData::KeyFlags(flags)).map_err(internal)?);
        if let Some(lifetime) = lifetime {
            hashed.push(
                Subpacket::regular(SubpacketData::KeyExpirationTime(lifetime)).map_err(internal)?,
            );
        }
        // A signing subkey signs the primary back again.
        if evaluated.usage.sign {
            let back = sub
                .key
                .sign_primary_key_binding(&mut rng, primary_key.public_key(), &password)
                .map_err(internal)?;
            hashed.push(
                Subpacket::regular(SubpacketData::EmbeddedSignature(Box::new(back)))
                    .map_err(internal)?,
            );
        }
        config.hashed_subpackets = hashed;
        issuer(&mut config)?;
        let sig = config
            .sign_subkey_binding(
                &primary_key,
                primary_key.public_key(),
                &password,
                sub.key.public_key(),
            )
            .map_err(internal)?;
        sub.signatures
            .retain(|old| old.typ() != Some(SignatureType::SubkeyBinding));
        sub.signatures.push(sig);
    }
    Ok(key)
}

fn is_self_certification(sig: &pgp::packet::Signature, primary: &pgp::packet::SecretKey) -> bool {
    let certification = matches!(
        sig.typ(),
        Some(
            SignatureType::CertGeneric
                | SignatureType::CertPersona
                | SignatureType::CertCasual
                | SignatureType::CertPositive
        )
    );
    let fingerprints = sig.issuer_fingerprint();
    let own = if fingerprints.is_empty() {
        sig.issuer_key_id()
            .iter()
            .any(|id| **id == primary.legacy_key_id())
    } else {
        fingerprints.iter().any(|f| **f == primary.fingerprint())
    };
    certification && own
}
