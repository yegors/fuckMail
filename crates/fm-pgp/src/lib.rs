//! OpenPGP for fuckMail: keys, the rules for when a key may be used, sealing
//! secret keys at rest, and PGP/MIME (RFC 3156) signing, encryption,
//! decryption and verification.
//!
//! Everything here is a pure transform: no network, file, database or keychain
//! access. Callers pass the time, the key-encryption key and the keys
//! themselves. The OpenPGP packets come from rpgp (the `pgp` crate); this crate
//! adds the policy rpgp leaves to applications: which self-signatures count,
//! when a key has expired or been revoked, and which subkeys may sign or
//! encrypt.
mod cert;
mod generate;
mod mime;
mod open;
mod seal;

pub use cert::{
    encryption_keys, inspect, signing_key, Cert, KeyInfo, KeyRef, KeyStatus, SecretRef, SubkeyInfo,
};
pub use generate::{generate, set_expiry, Algorithm, NewKey};
pub use mime::{encrypt_message, sign_message, Recipient};
pub use open::{
    inspect_message, is_protection_part, looks_protected, open_message, protection, readable_text,
    Decryption, HeaderCheck, Keyring, Opened, Protection, SignatureCheck, SignatureStatus,
};
pub use seal::{
    clean, combine, export_public, export_revocation, export_secret, is_protected, merge, new_kek,
    parse_keys, revocations, seal, unlock, unseal, ParsedKey, EXPORT_S2K_COUNT, KEK_LEN,
    MIN_EXPORT_PASSPHRASE,
};

/// Largest key file or pasted key block accepted for import, in bytes (1 MiB).
pub const MAX_KEY_BYTES: usize = 1024 * 1024;
/// Largest decrypted message, in bytes (64 MiB). Bigger plaintext, including
/// a compression bomb, is refused rather than read into memory.
pub const MAX_PLAINTEXT_BYTES: usize = 64 * 1024 * 1024;

/// Why an OpenPGP operation failed. `code` is stable and machine-readable;
/// `message` is a short sentence for people.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgpError {
    pub code: &'static str,
    pub message: String,
}

impl PgpError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for PgpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PgpError {}

pub type Result<T> = std::result::Result<T, PgpError>;

/// Uppercase hex, as fingerprints and key IDs are shown and stored.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}
