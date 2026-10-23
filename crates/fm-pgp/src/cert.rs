//! Reading a certificate: which user IDs and subkeys are bound by valid
//! self-signatures, whether the key has expired or been revoked, and which
//! key may sign or encrypt at a given time.
//!
//! rpgp verifies individual signatures; the policy here decides which ones
//! count. A self-signature counts when it verifies, uses an acceptable hash,
//! was made by the primary key and has not expired. The newest one wins. MD5
//! and RIPEMD-160 never count; SHA-1 counts only for key bindings made before
//! 2023-02-01, so older keys keep working while new SHA-1 signatures don't.
//! RSA, DSA and ElGamal keys under 2048 bits are weak and never used.
use crate::hex;
use pgp::{
    composed::{SignedPublicKey, SignedPublicSubKey, SignedSecretKey},
    crypto::{hash::HashAlgorithm, public_key::PublicKeyAlgorithm},
    packet::{self, KeyFlags, Signature, SignatureType, SubpacketData},
    ser::Serialize as _,
    types::{EcdhPublicParams, KeyDetails, PublicParams, Tag},
};
use serde::Serialize;

/// SHA-1 key bindings made from this time on (2023-02-01) don't count.
const SHA1_BINDING_CUTOFF: u64 = 1_675_209_600;
/// Self-signatures may be up to a day ahead of the caller's clock.
const CLOCK_SKEW: u64 = 86_400;

/// A key as the keyring holds it: its public certificate, and the secret key
/// when it is one of the person's own.
#[derive(Clone)]
pub struct Cert {
    pub public: SignedPublicKey,
    pub secret: Option<SignedSecretKey>,
}

/// Whether a key can be used now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyStatus {
    Valid,
    Expired,
    Revoked,
    /// No valid self-signature binds it, so nothing about it can be trusted.
    Invalid,
    /// Under 2048 bits of RSA, DSA or ElGamal.
    Weak,
}

/// A key's summary for lists and checks.
#[derive(Clone, Debug, Serialize)]
pub struct KeyInfo {
    /// Uppercase hex: 40 characters for v4 keys, 64 for v6.
    pub fingerprint: String,
    /// The last 16 hex characters of a v4 fingerprint, the first 16 of a v6 one.
    pub key_id: String,
    pub version: u8,
    /// The primary key's algorithm, such as `Ed25519` or `RSA 4096`.
    pub algorithm: String,
    /// User IDs bound by a valid self-signature, the primary one first.
    pub user_ids: Vec<String>,
    /// Lowercase email addresses from `user_ids`, without duplicates.
    pub addresses: Vec<String>,
    pub created_at: u64,
    pub expires_at: Option<u64>,
    pub status: KeyStatus,
    /// A valid key with a usable encryption key now.
    pub can_encrypt: bool,
    /// A valid key with a usable signing key now. For a public-only key this
    /// says whether it can make signatures, not whether we hold its secret.
    pub can_sign: bool,
    pub secret: bool,
    pub subkeys: Vec<SubkeyInfo>,
}

/// One subkey's summary.
#[derive(Clone, Debug, Serialize)]
pub struct SubkeyInfo {
    pub fingerprint: String,
    pub algorithm: String,
    pub created_at: u64,
    pub expires_at: Option<u64>,
    /// Any of `sign`, `encrypt` and `authenticate`.
    pub usage: Vec<&'static str>,
    pub status: KeyStatus,
}

/// What a component key may be used for.
#[derive(Clone, Copy, Default)]
pub(crate) struct Usage {
    pub sign: bool,
    pub encrypt: bool,
    pub authenticate: bool,
}

/// The primary key or one subkey of a certificate.
#[derive(Clone, Copy)]
pub enum KeyRef<'a> {
    Primary(&'a packet::PublicKey),
    Sub(&'a packet::PublicSubkey),
}

/// The primary key's standing, from the newest valid self-signatures.
pub(crate) struct Evaluation<'a> {
    pub users: Vec<(String, &'a Signature)>,
    pub binding: Option<&'a Signature>,
    pub revoked: bool,
    pub weak: bool,
    pub usage: Usage,
    pub expires_at: Option<u64>,
}

/// One subkey's standing.
pub(crate) struct SubEvaluation<'a> {
    pub key: &'a SignedPublicSubKey,
    pub bound: bool,
    pub revoked: bool,
    pub weak: bool,
    pub usage: Usage,
    pub expires_at: Option<u64>,
}

impl SubEvaluation<'_> {
    pub(crate) fn status(&self, now: u64) -> KeyStatus {
        component_status(self.bound, self.revoked, self.weak, self.expires_at, now)
    }
}

fn component_status(
    bound: bool,
    revoked: bool,
    weak: bool,
    expires_at: Option<u64>,
    now: u64,
) -> KeyStatus {
    if revoked {
        KeyStatus::Revoked
    } else if !bound {
        KeyStatus::Invalid
    } else if expires_at.is_some_and(|at| at <= now) {
        KeyStatus::Expired
    } else if weak {
        KeyStatus::Weak
    } else {
        KeyStatus::Valid
    }
}

impl Evaluation<'_> {
    pub(crate) fn status(&self, now: u64) -> KeyStatus {
        component_status(
            self.binding.is_some(),
            self.revoked,
            self.weak,
            self.expires_at,
            now,
        )
    }
}

pub(crate) fn created(sig: &Signature) -> Option<u64> {
    sig.created().map(|t| u64::from(t.as_secs()))
}

/// Whether `sig` names `key` as its issuer, or names no issuer at all.
fn issued_by(sig: &Signature, key: &packet::PublicKey) -> bool {
    let fingerprints = sig.issuer_fingerprint();
    if !fingerprints.is_empty() {
        let own = key.fingerprint();
        return fingerprints.iter().any(|f| **f == own);
    }
    let ids = sig.issuer_key_id();
    let own = key.legacy_key_id();
    ids.is_empty() || ids.iter().any(|id| **id == own)
}

/// Made no later than `now` (with some clock skew) and not yet expired.
fn live(sig: &Signature, now: u64) -> bool {
    let Some(made) = created(sig) else {
        return false;
    };
    if made > now.saturating_add(CLOCK_SKEW) {
        return false;
    }
    match sig
        .signature_expiration_time()
        .map(|d| u64::from(d.as_secs()))
    {
        Some(secs) if secs > 0 => now < made.saturating_add(secs),
        _ => true,
    }
}

/// Whether a signature's hash is strong enough. Key bindings may still use
/// SHA-1 if they were made before the cutoff.
pub(crate) fn acceptable_hash(sig: &Signature, binding: bool) -> bool {
    match sig.hash_alg() {
        Some(
            HashAlgorithm::Sha256
            | HashAlgorithm::Sha384
            | HashAlgorithm::Sha512
            | HashAlgorithm::Sha224
            | HashAlgorithm::Sha3_256
            | HashAlgorithm::Sha3_512,
        ) => true,
        Some(HashAlgorithm::Sha1) => {
            binding && created(sig).is_some_and(|made| made < SHA1_BINDING_CUTOFF)
        }
        _ => false,
    }
}

/// Keeps the newer of two signatures by creation time.
fn newer<'a>(best: Option<&'a Signature>, sig: &'a Signature) -> Option<&'a Signature> {
    match best {
        Some(old) if created(old) > created(sig) => Some(old),
        _ => Some(sig),
    }
}

pub(crate) fn has_key_flags(sig: &Signature) -> bool {
    sig.config().is_some_and(|config| {
        config
            .hashed_subpackets()
            .any(|p| matches!(p.data, SubpacketData::KeyFlags(_)))
    })
}

/// The usage a binding signature grants, or what the algorithm can do when
/// the signature carries no key flags (as in some old keys).
fn usage(sig: Option<&Signature>, algorithm: PublicKeyAlgorithm) -> Usage {
    match sig.filter(|sig| has_key_flags(sig)) {
        Some(sig) => {
            let flags: KeyFlags = sig.key_flags();
            Usage {
                sign: flags.sign() && algorithm.can_sign(),
                encrypt: (flags.encrypt_comms() || flags.encrypt_storage())
                    && algorithm.can_encrypt(),
                authenticate: flags.authentication(),
            }
        }
        None => Usage {
            sign: algorithm.can_sign(),
            encrypt: algorithm.can_encrypt(),
            authenticate: false,
        },
    }
}

fn expiry(sig: Option<&Signature>, created_at: u64) -> Option<u64> {
    sig.and_then(|sig| sig.key_expiration_time())
        .map(|d| u64::from(d.as_secs()))
        .filter(|secs| *secs > 0)
        .map(|secs| created_at.saturating_add(secs))
}

/// A readable algorithm name and whether its size is too small to use.
pub(crate) fn algorithm(params: &PublicParams) -> (String, bool) {
    // RSA, DSA and ElGamal serialize their first MPI first: a two-byte bit
    // count, then the number.
    let bits = || {
        params
            .to_bytes()
            .ok()
            .filter(|bytes| bytes.len() >= 2)
            .map(|bytes| u32::from(u16::from_be_bytes([bytes[0], bytes[1]])))
            .unwrap_or(0)
    };
    match params {
        PublicParams::RSA(_) => {
            let bits = bits();
            (format!("RSA {bits}"), bits < 2048)
        }
        PublicParams::DSA(_) => {
            let bits = bits();
            (format!("DSA {bits}"), bits < 2048)
        }
        PublicParams::Elgamal(_) => {
            let bits = bits();
            (format!("ElGamal {bits}"), bits < 2048)
        }
        PublicParams::EdDSALegacy(_) | PublicParams::Ed25519(_) => ("Ed25519".into(), false),
        PublicParams::Ed448(_) => ("Ed448".into(), false),
        PublicParams::X25519(_) => ("X25519".into(), false),
        PublicParams::X448(_) => ("X448".into(), false),
        PublicParams::ECDH(EcdhPublicParams::Curve25519Legacy { .. }) => {
            ("Curve25519".into(), false)
        }
        PublicParams::ECDH(ecdh) => (format!("ECDH {}", ecdh.curve().name()), false),
        PublicParams::ECDSA(ecdsa) => (format!("ECDSA {}", ecdsa.curve().name()), false),
        #[allow(unreachable_patterns)]
        _ => ("Unknown".into(), true),
    }
}

/// The email address in a user ID such as `Ann <ann@example.test>`, lowercased.
pub(crate) fn address(user_id: &str) -> Option<String> {
    let candidate = match (user_id.rfind('<'), user_id.rfind('>')) {
        (Some(open), Some(close)) if open < close => &user_id[open + 1..close],
        _ => user_id,
    }
    .trim();
    let (local, domain) = candidate.split_once('@')?;
    let valid = !local.is_empty()
        && !domain.is_empty()
        && !candidate
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '<' || c == '>');
    valid.then(|| candidate.to_lowercase())
}

/// Evaluates the primary key: its user IDs, binding, revocation and expiry at `now`.
pub(crate) fn evaluate(cert: &SignedPublicKey, now: u64) -> Evaluation<'_> {
    let primary = &cert.primary_key;
    let revoked = cert.details.revocation_signatures.iter().any(|sig| {
        sig.typ() == Some(SignatureType::KeyRevocation)
            && issued_by(sig, primary)
            && sig.verify_key(primary).is_ok()
    });
    let mut users = Vec::new();
    let mut primary_users = Vec::new();
    for user in &cert.details.users {
        let mut best = None;
        let mut revoked_at = None;
        for sig in &user.signatures {
            if !issued_by(sig, primary) {
                continue;
            }
            match sig.typ() {
                Some(
                    SignatureType::CertGeneric
                    | SignatureType::CertPersona
                    | SignatureType::CertCasual
                    | SignatureType::CertPositive,
                ) if live(sig, now)
                    && acceptable_hash(sig, true)
                    && sig
                        .verify_certification(primary, Tag::UserId, &user.id)
                        .is_ok() =>
                {
                    best = newer(best, sig);
                }
                Some(SignatureType::CertRevocation)
                    if sig
                        .verify_certification(primary, Tag::UserId, &user.id)
                        .is_ok() =>
                {
                    revoked_at = revoked_at.max(created(sig));
                }
                _ => {}
            }
        }
        let Some(best) = best else { continue };
        if revoked_at.is_some_and(|at| Some(at) >= created(best)) {
            continue;
        }
        let Some(text) = user.id.as_str() else {
            continue;
        };
        if best.is_primary() {
            primary_users.push((text.to_owned(), best));
        } else {
            users.push((text.to_owned(), best));
        }
    }
    // The newest primary-flagged self-signature binds the key; without one,
    // the newest of any user ID.
    primary_users.sort_by_key(|(_, sig)| std::cmp::Reverse(created(sig)));
    users.sort_by_key(|(_, sig)| std::cmp::Reverse(created(sig)));
    let mut direct = None;
    for sig in &cert.details.direct_signatures {
        if sig.typ() == Some(SignatureType::Key)
            && issued_by(sig, primary)
            && live(sig, now)
            && acceptable_hash(sig, true)
            && sig.verify_key(primary).is_ok()
        {
            direct = newer(direct, sig);
        }
    }
    let binding = if primary.version() == pgp::types::KeyVersion::V6 {
        direct.or(primary_users.first().or(users.first()).map(|(_, s)| *s))
    } else {
        primary_users
            .first()
            .or(users.first())
            .map(|(_, s)| *s)
            .or(direct)
    };
    let created_at = u64::from(primary.created_at().as_secs());
    let (_, weak) = algorithm(primary.public_params());
    let all: Vec<_> = primary_users.into_iter().chain(users).collect();
    Evaluation {
        usage: usage(binding.or(direct), primary.algorithm()),
        expires_at: expiry(binding, created_at).or_else(|| expiry(direct, created_at)),
        users: all,
        binding,
        revoked,
        weak,
    }
}

/// Evaluates every subkey at `now`.
pub(crate) fn evaluate_subkeys(cert: &SignedPublicKey, now: u64) -> Vec<SubEvaluation<'_>> {
    let primary = &cert.primary_key;
    cert.public_subkeys
        .iter()
        .map(|sub| {
            let mut binding = None;
            let mut revoked = false;
            for sig in &sub.signatures {
                if !issued_by(sig, primary) {
                    continue;
                }
                match sig.typ() {
                    Some(SignatureType::SubkeyBinding)
                        if live(sig, now)
                            && acceptable_hash(sig, true)
                            && sig.verify_subkey_binding(primary, &sub.key).is_ok() =>
                    {
                        binding = newer(binding, sig);
                    }
                    Some(SignatureType::SubkeyRevocation)
                        if sig.verify_subkey_binding(primary, &sub.key).is_ok() =>
                    {
                        revoked = true;
                    }
                    _ => {}
                }
            }
            let mut usage = usage(binding, sub.key.algorithm());
            // A signing subkey must also sign the primary back, or anyone
            // could claim someone else's signing key as their own.
            if usage.sign {
                usage.sign = binding
                    .and_then(|sig| sig.embedded_signature())
                    .is_some_and(|back| {
                        acceptable_hash(back, true)
                            && back.verify_primary_key_binding(&sub.key, primary).is_ok()
                    });
            }
            let (_, weak) = algorithm(sub.key.public_params());
            SubEvaluation {
                key: sub,
                bound: binding.is_some(),
                revoked,
                weak,
                usage,
                expires_at: expiry(binding, u64::from(sub.key.created_at().as_secs())),
            }
        })
        .collect()
}

/// The keys to encrypt to at `now`: every valid encryption subkey, and the
/// primary key if it may encrypt. Empty when the certificate can't be used.
pub fn encryption_keys(cert: &SignedPublicKey, now: u64) -> Vec<KeyRef<'_>> {
    let primary = evaluate(cert, now);
    if primary.status(now) != KeyStatus::Valid {
        return Vec::new();
    }
    let mut keys: Vec<KeyRef<'_>> = evaluate_subkeys(cert, now)
        .into_iter()
        .filter(|sub| sub.status(now) == KeyStatus::Valid && sub.usage.encrypt)
        .map(|sub| KeyRef::Sub(&sub.key.key))
        .collect();
    if primary.usage.encrypt {
        keys.push(KeyRef::Primary(&cert.primary_key));
    }
    keys
}

/// The secret key to sign with at `now`, judged by the certificate `cert`:
/// the primary if it may sign, else the newest valid signing subkey whose
/// secret `key` holds. `None` when the key can't sign.
pub fn signing_key<'a>(
    cert: &SignedPublicKey,
    key: &'a SignedSecretKey,
    now: u64,
) -> Option<SecretRef<'a>> {
    let primary = evaluate(cert, now);
    if primary.status(now) != KeyStatus::Valid {
        return None;
    }
    if primary.usage.sign {
        return Some(SecretRef::Primary(&key.primary_key));
    }
    let mut candidates: Vec<_> = evaluate_subkeys(cert, now)
        .into_iter()
        .filter(|sub| sub.status(now) == KeyStatus::Valid && sub.usage.sign)
        .map(|sub| (sub.key.key.created_at(), sub.key.key.fingerprint()))
        .collect();
    candidates.sort_by_key(|(created, _)| std::cmp::Reverse(*created));
    candidates.into_iter().find_map(|(_, fingerprint)| {
        key.secret_subkeys
            .iter()
            .find(|sub| sub.key.fingerprint() == fingerprint)
            .map(|sub| SecretRef::Sub(&sub.key))
    })
}

/// The secret primary key or one secret subkey.
pub enum SecretRef<'a> {
    Primary(&'a packet::SecretKey),
    Sub(&'a packet::SecretSubkey),
}

/// Summarizes `cert` at `now`. `secret` says whether its secret key is held.
pub fn inspect(cert: &SignedPublicKey, secret: bool, now: u64) -> KeyInfo {
    let primary = evaluate(cert, now);
    let status = primary.status(now);
    let subkeys = evaluate_subkeys(cert, now);
    let mut addresses = Vec::new();
    for (user_id, _) in &primary.users {
        if let Some(address) = address(user_id) {
            if !addresses.contains(&address) {
                addresses.push(address);
            }
        }
    }
    let fingerprint = cert.primary_key.fingerprint();
    let valid = status == KeyStatus::Valid;
    let subkey_can = |pick: fn(&Usage) -> bool| {
        subkeys
            .iter()
            .any(|sub| sub.status(now) == KeyStatus::Valid && pick(&sub.usage))
    };
    KeyInfo {
        fingerprint: hex(fingerprint.as_bytes()),
        key_id: hex(cert.primary_key.legacy_key_id().as_ref()),
        version: match cert.primary_key.version() {
            pgp::types::KeyVersion::V6 => 6,
            pgp::types::KeyVersion::V4 => 4,
            _ => 3,
        },
        algorithm: algorithm(cert.primary_key.public_params()).0,
        user_ids: primary.users.iter().map(|(id, _)| id.clone()).collect(),
        addresses,
        created_at: u64::from(cert.primary_key.created_at().as_secs()),
        expires_at: primary.expires_at,
        status,
        can_encrypt: valid && (primary.usage.encrypt || subkey_can(|u| u.encrypt)),
        can_sign: valid && (primary.usage.sign || subkey_can(|u| u.sign)),
        secret,
        subkeys: subkeys
            .iter()
            .map(|sub| {
                let mut usage = Vec::new();
                if sub.usage.sign {
                    usage.push("sign");
                }
                if sub.usage.encrypt {
                    usage.push("encrypt");
                }
                if sub.usage.authenticate {
                    usage.push("authenticate");
                }
                SubkeyInfo {
                    fingerprint: hex(sub.key.key.fingerprint().as_bytes()),
                    algorithm: algorithm(sub.key.key.public_params()).0,
                    created_at: u64::from(sub.key.key.created_at().as_secs()),
                    expires_at: sub.expires_at,
                    usage,
                    status: sub.status(now),
                }
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::address;

    #[test]
    fn user_id_addresses() {
        assert_eq!(
            address("Ann Example <Ann@Example.test>").as_deref(),
            Some("ann@example.test")
        );
        assert_eq!(
            address("bob@example.test").as_deref(),
            Some("bob@example.test")
        );
        assert_eq!(address("Just A Name"), None);
        assert_eq!(address("Broken <a b@example.test>"), None);
    }
}
