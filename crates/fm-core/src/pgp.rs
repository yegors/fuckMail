//! The OpenPGP keyring: listing, generating, importing, exporting, renewing
//! and deleting keys, each account's key choice, and the key material that
//! reading and sending mail need.
//!
//! The keyring is profile-wide: the person's own keys (with a sealed secret
//! key) and other people's public keys share one table. An account uses at
//! most one of its owner's keys, and only one whose user IDs include the
//! account's address. Requests follow the begin, prepare, finish shape: the
//! writer takes a [`Keyring`] snapshot, OpenPGP work runs off the writer in
//! [`pgp_prepare`], and the writer stores the result. Secret keys are only
//! ever stored sealed under the key-encryption key from a [`KekSource`]; in
//! the engine that is the OS keychain.
//!
//! Encrypted mail is decrypted in memory to be read. Files that have to exist
//! for the reader or another application (an attachment opened or previewed,
//! an inline picture) go under `<profile>/decrypted/`, which
//! [`forget_decrypted`] empties.
use crate::{err, fields, id, string, CoreError, Result, Store};
use fm_pgp::{KeyStatus, ParsedKey, KEK_LEN};
use pgp::{
    composed::{Deserializable, SignedPublicKey, SignedSecretKey},
    ser::Serialize as _,
};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

/// An imported key merged with any stored copy: the key, its newly sealed
/// secret, and the stored copy's `updated_at`, which must not have moved.
type Merged = (SignedPublicKey, Option<Vec<u8>>, Option<i64>);

/// Longest a key may be set to last, in days (about 30 years).
const MAX_EXPIRY_DAYS: u64 = 11_000;

/// The profile folder holding decrypted files; see [`forget_decrypted`].
pub(crate) const DECRYPTED_DIR: &str = "decrypted";

/// Deletes every decrypted file the profile holds: attachments of encrypted
/// mail that were opened or previewed, and its inline pictures. The engine
/// calls this as it starts and as it stops. Copies the person saved
/// elsewhere are theirs and stay.
pub fn forget_decrypted(profile: &std::path::Path) -> Result<()> {
    let path = profile.join(DECRYPTED_DIR);
    match std::fs::symlink_metadata(&path) {
        Ok(meta) if meta.is_dir() => Ok(std::fs::remove_dir_all(path)?),
        Ok(_) => Ok(std::fs::remove_file(path)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Where the profile's key-encryption key lives.
pub trait KekSource: Send + Sync {
    /// The key, or `None` when the profile has none yet. With `create`, a
    /// missing key is made and saved first. Fails with `store_unavailable`
    /// when the keychain is locked, refuses or is missing.
    fn kek(&self, create: bool) -> Result<Option<Zeroizing<[u8; KEK_LEN]>>>;
}

/// The source when no keychain was set: secret keys can't be stored or used.
pub struct NoKeychain;

impl KekSource for NoKeychain {
    fn kek(&self, _create: bool) -> Result<Option<Zeroizing<[u8; KEK_LEN]>>> {
        Err(err(
            "store_unavailable",
            "Secret keys need the system keychain, which isn't available here",
        ))
    }
}

/// A key-encryption key held in memory, for tests and demo profiles' own
/// sources.
pub struct MemoryKek(std::sync::Mutex<Option<Zeroizing<[u8; KEK_LEN]>>>);

impl MemoryKek {
    pub fn new() -> Self {
        Self(std::sync::Mutex::new(None))
    }
}

impl Default for MemoryKek {
    fn default() -> Self {
        Self::new()
    }
}

impl KekSource for MemoryKek {
    fn kek(&self, create: bool) -> Result<Option<Zeroizing<[u8; KEK_LEN]>>> {
        let mut held = self
            .0
            .lock()
            .map_err(|_| err("store_unavailable", "The key store is unavailable"))?;
        if held.is_none() && create {
            *held = Some(fm_pgp::new_kek());
        }
        Ok(held.clone())
    }
}

/// One stored key.
#[derive(Clone)]
pub struct StoredKey {
    pub fingerprint: String,
    pub public: Vec<u8>,
    /// Sealed with [`fm_pgp::seal`]; `None` for other people's keys.
    pub secret: Option<Vec<u8>>,
    pub origin: String,
    pub added_at: i64,
    pub updated_at: i64,
}

/// An account's chosen key.
#[derive(Clone)]
pub struct AccountChoice {
    pub account: String,
    pub email: String,
    pub fingerprint: String,
    pub sign_by_default: bool,
    pub encrypt_by_default: bool,
}

/// A snapshot of the keyring, taken on the writer.
#[derive(Clone, Default)]
pub struct Keyring {
    pub keys: Vec<StoredKey>,
    pub accounts: Vec<AccountChoice>,
}

fn pgp_error(error: fm_pgp::PgpError) -> CoreError {
    err(error.code, &error.message)
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

impl Keyring {
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    fn key(&self, fingerprint: &str) -> Result<&StoredKey> {
        self.keys
            .iter()
            .find(|k| k.fingerprint.eq_ignore_ascii_case(fingerprint))
            .ok_or_else(|| err("not_found", "That key is not in the keyring"))
    }

    /// Every stored public certificate that still parses.
    pub fn certs(&self) -> Vec<SignedPublicKey> {
        self.keys
            .iter()
            .filter_map(|k| SignedPublicKey::from_bytes(&k.public[..]).ok())
            .collect()
    }

    /// The key IDs of every own key and subkey, uppercase hex, as encrypted
    /// mail names its recipients.
    fn own_key_ids(&self) -> Vec<String> {
        use pgp::types::KeyDetails;
        let hex = |id: pgp::types::KeyId| id.as_ref().iter().map(|b| format!("{b:02X}")).collect();
        self.keys
            .iter()
            .filter(|k| k.secret.is_some())
            .filter_map(|k| SignedPublicKey::from_bytes(&k.public[..]).ok())
            .flat_map(|cert| {
                std::iter::once(hex(cert.primary_key.legacy_key_id()))
                    .chain(
                        cert.public_subkeys
                            .iter()
                            .map(|sub| hex(sub.key.legacy_key_id())),
                    )
                    .collect::<Vec<String>>()
            })
            .collect()
    }

    /// Every own secret key, unsealed and combined with its current
    /// certificate. Empty when there are none; reaches the keychain only when
    /// there are.
    pub fn secrets(&self, kek: &dyn KekSource) -> Result<Vec<SignedSecretKey>> {
        let own: Vec<_> = self.keys.iter().filter(|k| k.secret.is_some()).collect();
        if own.is_empty() {
            return Ok(Vec::new());
        }
        let kek = kek.kek(false)?.ok_or_else(|| {
            err(
                "key_locked",
                "The key that protects your secret keys is missing from the system keychain",
            )
        })?;
        own.iter().map(|k| unseal(k, &kek)).collect()
    }

    /// Opens a protected message with this keyring, or `None` for ordinary
    /// mail. Secret keys are unsealed only for encrypted content, and only
    /// with `decrypt`; without it encrypted content stays sealed.
    pub fn open(&self, raw: &[u8], kek: &dyn KekSource, decrypt: bool) -> Option<fm_pgp::Opened> {
        if !fm_pgp::looks_protected(raw) {
            return None;
        }
        let keyring = fm_pgp::Keyring {
            certs: self.certs(),
        };
        if !decrypt {
            return fm_pgp::inspect_message(raw, &keyring, now());
        }
        let mut secrets = || {
            self.secrets(kek).map_err(|e| fm_pgp::PgpError {
                code: if e.code == "store_unavailable" {
                    "store_unavailable"
                } else {
                    "key_locked"
                },
                message: e.message,
            })
        };
        fm_pgp::open_message(raw, &keyring, &mut secrets, now())
    }
}

fn unseal(key: &StoredKey, kek: &[u8; KEK_LEN]) -> Result<SignedSecretKey> {
    let secret = fm_pgp::unseal(
        key.secret.as_deref().unwrap_or_default(),
        &key.fingerprint,
        kek,
    )
    .map_err(pgp_error)?;
    let public = parse_public(key)?;
    Ok(fm_pgp::combine(&public, &secret))
}

fn parse_public(key: &StoredKey) -> Result<SignedPublicKey> {
    SignedPublicKey::from_bytes(&key.public[..])
        .map_err(|_| err("storage_error", "A stored key no longer reads"))
}

/// The `pgp` status of an opened message, as message reads return it.
pub fn opened_json(opened: &fm_pgp::Opened) -> Value {
    json!({
        "encrypted": opened.encrypted,
        "inline": opened.inline,
        "unprotected": opened.unprotected,
        "decryption": opened.decryption,
        "signatures": opened.signatures,
        "headers": opened.headers,
    })
}

/// A key to store, or to update when `expected_updated_at` is set.
pub struct KeyWrite {
    fingerprint: String,
    public: Vec<u8>,
    /// `None` keeps whatever secret is stored.
    secret: Option<Vec<u8>>,
    origin: &'static str,
    addresses: Vec<String>,
    /// The key's `updated_at` when the snapshot was taken; `None` means it
    /// must not exist yet.
    expected_updated_at: Option<i64>,
}

/// What [`Store::pgp_begin`] hands to [`pgp_prepare`].
pub enum PgpStep {
    Reply(Value),
    Prepare(PgpPlan),
}

/// A keyring request frozen with its snapshot.
pub struct PgpPlan {
    method: String,
    account: Option<String>,
    params: Value,
    keyring: Keyring,
    /// The account's address, for methods scoped to one.
    account_email: Option<String>,
    /// Every account's address, lowercased: a public key claiming one of
    /// them needs the person's say-so.
    own_addresses: Vec<String>,
}

impl PgpPlan {
    /// A `path` to read for `pgp_key_import`, which the caller must read off
    /// the writer.
    pub fn import_path(&self) -> Option<&str> {
        (self.method == "pgp_key_import")
            .then(|| self.params["path"].as_str())
            .flatten()
    }

    /// Where `pgp_key_export` should write its key, and whether it may
    /// replace a file there. The engine writes it; the reply then carries the
    /// path instead of the key.
    pub fn export_path(&self) -> Option<(&str, bool)> {
        (self.method == "pgp_key_export")
            .then(|| self.params["path"].as_str())
            .flatten()
            .map(|path| (path, self.params["replace"] == true))
    }
}

/// The prepared result of a keyring request.
pub struct PgpProof {
    plan: PgpPlan,
    writes: Vec<KeyWrite>,
    /// The account to point at a key, or to clear with `None`.
    assign: Option<(String, Option<AccountChoice>)>,
    delete: Option<String>,
    result: Value,
}

/// Methods handled here; all are profile-wide except `pgp_account` and
/// `pgp_recipients`.
pub const METHODS: &[&str] = &[
    "pgp_keys",
    "pgp_key_generate",
    "pgp_key_import",
    "pgp_key_export",
    "pgp_key_expiry",
    "pgp_key_delete",
    "pgp_account",
    "pgp_recipients",
];

/// Whether a request changes the keyring; an import `preview` doesn't.
fn mutating(method: &str, p: &Value) -> bool {
    let preview = method == "pgp_key_import" && p["preview"] == true;
    !preview && !matches!(method, "pgp_keys" | "pgp_key_export" | "pgp_recipients")
}

/// An optional boolean parameter, false when absent.
fn flag(p: &Value, key: &str) -> Result<bool> {
    match p.get(key) {
        None => Ok(false),
        Some(Value::Bool(on)) => Ok(*on),
        Some(_) => Err(err(
            "invalid_request",
            &format!("{key} must be true or false"),
        )),
    }
}

/// The request's receipt fingerprint. Passphrases never enter it, and key
/// text enters only as a hash.
fn receipt_fingerprint(method: &str, p: &Value) -> Result<String> {
    let mut p = p.clone();
    if let Some(o) = p.as_object_mut() {
        o.remove("passphrase");
        if let Some(text) = o.get("text").and_then(Value::as_str) {
            let hashed = format!("{:x}", Sha256::digest(text.as_bytes()));
            o.insert("text".into(), json!(hashed));
        }
    }
    crate::mutations::fingerprint(method, &p)
}

fn boolean(p: &Value, key: &str) -> Result<bool> {
    p[key]
        .as_bool()
        .ok_or_else(|| err("invalid_request", &format!("{key} must be true or false")))
}

fn expires_in(p: &Value) -> Result<Option<u64>> {
    match &p["expires_days"] {
        Value::Null => Ok(None),
        value => {
            let days = value
                .as_u64()
                .filter(|d| (1..=MAX_EXPIRY_DAYS).contains(d))
                .ok_or_else(|| {
                    err(
                        "invalid_request",
                        "expires_days must be null or a number of days from 1 to 11000",
                    )
                })?;
            Ok(Some(days * 86_400))
        }
    }
}

impl Store {
    /// Sets where the key-encryption key for secret keys lives. The engine
    /// sets the OS keychain; without it secret keys can't be used.
    pub fn set_pgp_kek(&mut self, kek: std::sync::Arc<dyn KekSource>) {
        self.pgp_kek = kek;
    }

    /// The key-encryption key source, for work that runs off the writer.
    pub fn pgp_kek(&self) -> std::sync::Arc<dyn KekSource> {
        self.pgp_kek.clone()
    }

    /// The whole keyring and every account's choice. Keys come in the order
    /// they were first stored, which is the order the person accepted them.
    pub fn pgp_keyring(&self) -> Result<Keyring> {
        let mut statement = self.db.prepare(
            "SELECT fingerprint,public_key,secret_key,origin,added_at,updated_at FROM pgp_keys ORDER BY rowid",
        )?;
        let keys = statement
            .query_map([], |r| {
                Ok(StoredKey {
                    fingerprint: r.get(0)?,
                    public: r.get(1)?,
                    secret: r.get(2)?,
                    origin: r.get(3)?,
                    added_at: r.get(4)?,
                    updated_at: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut statement = self.db.prepare(
            "SELECT p.account,a.email,p.fingerprint,p.sign_by_default,p.encrypt_by_default FROM pgp_accounts p JOIN accounts a ON a.id=p.account ORDER BY p.account",
        )?;
        let accounts = statement
            .query_map([], |r| {
                Ok(AccountChoice {
                    account: r.get(0)?,
                    email: r.get(1)?,
                    fingerprint: r.get(2)?,
                    sign_by_default: r.get(3)?,
                    encrypt_by_default: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Keyring { keys, accounts })
    }

    /// The account's key choice as account rows show it, or `null`: the
    /// fingerprint, the defaults for new mail, and the key's status and
    /// expiry now.
    pub(crate) fn pgp_account_json(&self, account: &str) -> Result<Value> {
        let row: Option<(String, bool, bool, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT p.fingerprint,p.sign_by_default,p.encrypt_by_default,k.public_key FROM pgp_accounts p JOIN pgp_keys k ON k.fingerprint=p.fingerprint WHERE p.account=?1",
                [account],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let Some((fingerprint, sign, encrypt, public)) = row else {
            return Ok(Value::Null);
        };
        let info = SignedPublicKey::from_bytes(&public[..])
            .ok()
            .map(|cert| fm_pgp::inspect(&cert, true, now()));
        Ok(json!({
            "fingerprint": fingerprint,
            "sign_by_default": sign,
            "encrypt_by_default": encrypt,
            "status": info.as_ref().map(|i| i.status),
            "expires_at": info.as_ref().and_then(|i| i.expires_at),
        }))
    }

    fn pgp_receipt(&self, method: &str, p: &Value) -> Result<Option<Value>> {
        let request = string(p, "request_id")?;
        let prior: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT fingerprint,result_json FROM preference_operations WHERE request_id=?1",
                [request],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match prior {
            Some((old, _)) if old != receipt_fingerprint(method, p)? => Err(err(
                "request_id_conflict",
                "Request ID identifies a different keyring change",
            )),
            Some((_, result)) => Ok(Some(serde_json::from_str(&result)?)),
            None => Ok(None),
        }
    }

    /// Validates a keyring request and freezes it with a keyring snapshot.
    /// Replays a known `request_id`.
    pub fn pgp_begin(&self, method: &str, account: Option<&str>, p: &Value) -> Result<PgpStep> {
        let allowed: &[&str] = match method {
            "pgp_keys" => &[],
            "pgp_key_generate" => &[
                "name",
                "email",
                "algorithm",
                "expires_days",
                "account",
                "request_id",
            ],
            "pgp_key_import" => &[
                "text",
                "path",
                "passphrase",
                "request_id",
                "preview",
                "public_only",
                "replace",
            ],
            "pgp_key_export" => &["fingerprint", "kind", "passphrase", "path", "replace"],
            "pgp_key_expiry" => &["fingerprint", "expires_days", "request_id"],
            "pgp_key_delete" => &["fingerprint", "request_id"],
            "pgp_account" => &[
                "fingerprint",
                "sign_by_default",
                "encrypt_by_default",
                "request_id",
            ],
            "pgp_recipients" => &["addresses"],
            _ => return Err(err("unsupported_method", "Unknown keyring method")),
        };
        fields(p, allowed)?;
        let scoped = matches!(method, "pgp_account" | "pgp_recipients");
        let account_email = match (scoped, account) {
            (true, None) => return Err(err("account_required", "Choose an account")),
            (true, Some(account)) => Some(
                self.db
                    .query_row(
                        "SELECT email FROM accounts WHERE id=?1",
                        [id(account)?],
                        |r| r.get::<_, String>(0),
                    )
                    .optional()?
                    .ok_or_else(|| err("not_found", "Account not found"))?,
            ),
            (false, Some(_)) => {
                return Err(err(
                    "invalid_request",
                    "Keyring methods are profile-wide; leave out the account",
                ))
            }
            (false, None) => None,
        };
        if method == "pgp_account" {
            self.require_active_account(account.unwrap())?;
        }
        for key in ["preview", "public_only", "replace"] {
            flag(p, key)?;
        }
        if method == "pgp_key_export" {
            if let Some(path) = p.get("path") {
                if !path
                    .as_str()
                    .is_some_and(|path| std::path::Path::new(path).is_absolute())
                {
                    return Err(err("invalid_request", "path must be an absolute file path"));
                }
            }
        }
        if mutating(method, p) {
            if let Some(result) = self.pgp_receipt(method, p)? {
                return Ok(PgpStep::Reply(result));
            }
        }
        if method == "pgp_key_generate" {
            if let Some(target) = p.get("account").filter(|v| !v.is_null()) {
                let target = target
                    .as_str()
                    .ok_or_else(|| err("invalid_request", "account must be an account ID"))?;
                self.require_active_account(id(target)?)?;
            }
        }
        if method == "pgp_key_import" && p.get("text").is_some() == p.get("path").is_some() {
            return Err(err("invalid_request", "Give either text or path"));
        }
        let own_addresses = self
            .db
            .prepare("SELECT DISTINCT lower(email) FROM accounts")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(PgpStep::Prepare(PgpPlan {
            method: method.into(),
            account: account.map(str::to_owned),
            params: p.clone(),
            keyring: self.pgp_keyring()?,
            account_email,
            own_addresses,
        }))
    }

    /// Stores a prepared keyring change with its receipt, in one transaction.
    /// Fails with `stale_keyring` when a key changed since the snapshot.
    pub fn pgp_finish(&mut self, proof: PgpProof) -> Result<Value> {
        let PgpProof {
            plan,
            writes,
            assign,
            delete,
            result,
        } = proof;
        if !mutating(&plan.method, &plan.params) {
            return Ok(result);
        }
        if let Some(result) = self.pgp_receipt(&plan.method, &plan.params)? {
            return Ok(result);
        }
        let stale = || err("stale_keyring", "The keyring changed meanwhile; try again");
        let tx = self.db.unchecked_transaction()?;
        let stamp = now() as i64;
        for write in &writes {
            let current: Option<i64> = tx
                .query_row(
                    "SELECT updated_at FROM pgp_keys WHERE fingerprint=?1",
                    [&write.fingerprint],
                    |r| r.get(0),
                )
                .optional()?;
            if current != write.expected_updated_at {
                return Err(stale());
            }
            match current {
                None => {
                    tx.execute(
                        "INSERT INTO pgp_keys(fingerprint,public_key,secret_key,origin,added_at,updated_at) VALUES(?1,?2,?3,?4,?5,?5)",
                        params![write.fingerprint, write.public, write.secret, write.origin, stamp],
                    )?;
                }
                Some(_) => {
                    // Updates never go back in time, so the next snapshot sees a change.
                    tx.execute(
                        "UPDATE pgp_keys SET public_key=?2,secret_key=COALESCE(?3,secret_key),updated_at=MAX(?4,updated_at+1) WHERE fingerprint=?1",
                        params![write.fingerprint, write.public, write.secret, stamp],
                    )?;
                }
            }
            tx.execute(
                "DELETE FROM pgp_key_addresses WHERE fingerprint=?1",
                [&write.fingerprint],
            )?;
            for address in &write.addresses {
                tx.execute(
                    "INSERT OR IGNORE INTO pgp_key_addresses(address,fingerprint) VALUES(?1,?2)",
                    params![address, write.fingerprint],
                )?;
            }
        }
        if let Some(fingerprint) = &delete {
            let current: Option<i64> = tx
                .query_row(
                    "SELECT updated_at FROM pgp_keys WHERE fingerprint=?1",
                    [fingerprint],
                    |r| r.get(0),
                )
                .optional()?;
            let expected = plan
                .keyring
                .keys
                .iter()
                .find(|k| &k.fingerprint == fingerprint)
                .map(|k| k.updated_at);
            if current != expected {
                return Err(stale());
            }
            tx.execute("DELETE FROM pgp_keys WHERE fingerprint=?1", [fingerprint])?;
        }
        if let Some((account, choice)) = &assign {
            self.require_active_account(account)?;
            match choice {
                None => {
                    tx.execute("DELETE FROM pgp_accounts WHERE account=?1", [account])?;
                }
                Some(choice) => {
                    // Only an own key that names the account's address.
                    let usable: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM pgp_keys k JOIN pgp_key_addresses a ON a.fingerprint=k.fingerprint JOIN accounts c ON c.id=?2 WHERE k.fingerprint=?1 AND k.secret_key IS NOT NULL AND a.address=lower(c.email))",
                        params![choice.fingerprint, account],
                        |r| r.get(0),
                    )?;
                    if !usable {
                        return Err(err(
                            "pgp_key_unusable",
                            "Choose one of your own keys made for this account's address",
                        ));
                    }
                    tx.execute(
                        "INSERT INTO pgp_accounts(account,fingerprint,sign_by_default,encrypt_by_default) VALUES(?1,?2,?3,?4) ON CONFLICT(account) DO UPDATE SET fingerprint=excluded.fingerprint,sign_by_default=excluded.sign_by_default,encrypt_by_default=excluded.encrypt_by_default",
                        params![account, choice.fingerprint, choice.sign_by_default, choice.encrypt_by_default],
                    )?;
                }
            }
        }
        tx.execute(
            "INSERT INTO preference_operations VALUES(?1,?2,?3)",
            params![
                string(&plan.params, "request_id")?,
                receipt_fingerprint(&plan.method, &plan.params)?,
                serde_json::to_string(&result)?
            ],
        )?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(result)
    }

    /// Runs a keyring request in place: begin, prepare and finish on this
    /// thread. The engine runs the prepare step on a worker instead. Only the
    /// engine writes exported keys to a `path`.
    pub fn pgp_handle(&mut self, method: &str, account: Option<&str>, p: &Value) -> Result<Value> {
        let kek = self.pgp_kek();
        match self.pgp_begin(method, account, p)? {
            PgpStep::Reply(value) => Ok(value),
            PgpStep::Prepare(plan) if plan.export_path().is_some() => Err(err(
                "invalid_request",
                "Exported keys are written to a file by the engine",
            )),
            PgpStep::Prepare(plan) => {
                let input = match plan.import_path() {
                    Some(path) => Some(read_key_file(path)?),
                    None => None,
                };
                let proof = pgp_prepare(plan, input, kek.as_ref())?;
                self.pgp_finish(proof)
            }
        }
    }
}

/// Reads a key file for import: a regular file of at most 1 MiB.
pub fn read_key_file(path: &str) -> Result<Vec<u8>> {
    let path = std::path::Path::new(path);
    if !path.is_absolute() {
        return Err(err("invalid_request", "The key file path must be absolute"));
    }
    let meta =
        std::fs::metadata(path).map_err(|_| err("not_found", "That key file can't be read"))?;
    if !meta.is_file() {
        return Err(err("invalid_request", "That is not a file"));
    }
    if meta.len() > fm_pgp::MAX_KEY_BYTES as u64 {
        return Err(err("limit_exceeded", "Key files can be at most 1 MiB"));
    }
    std::fs::read(path).map_err(|_| err("not_found", "That key file can't be read"))
}

fn key_json(key: &StoredKey, keyring: &Keyring, now: u64) -> Option<Value> {
    let cert = SignedPublicKey::from_bytes(&key.public[..]).ok()?;
    let info = fm_pgp::inspect(&cert, key.secret.is_some(), now);
    let mut value = serde_json::to_value(info).ok()?;
    value["origin"] = json!(key.origin);
    value["added_at"] = json!(key.added_at);
    value["updated_at"] = json!(key.updated_at);
    value["accounts"] = json!(keyring
        .accounts
        .iter()
        .filter(|a| a.fingerprint == key.fingerprint)
        .map(|a| a.account.clone())
        .collect::<Vec<_>>());
    Some(value)
}

fn write_for(
    cert: &SignedPublicKey,
    secret: Option<Vec<u8>>,
    origin: &'static str,
    expected_updated_at: Option<i64>,
    now: u64,
) -> Result<KeyWrite> {
    let info = fm_pgp::inspect(cert, secret.is_some(), now);
    let public = cert
        .to_bytes()
        .map_err(|_| err("internal", "The key can't be written"))?;
    // Imports keep only self-signed material, so this bounds what repeated
    // updates of one key can store.
    if public.len() > fm_pgp::MAX_KEY_BYTES {
        return Err(err("limit_exceeded", "A key can be at most 1 MiB"));
    }
    Ok(KeyWrite {
        fingerprint: info.fingerprint,
        public,
        secret,
        origin,
        addresses: info.addresses,
        expected_updated_at,
    })
}

fn require_kek(kek: &dyn KekSource) -> Result<Zeroizing<[u8; KEK_LEN]>> {
    kek.kek(true)?
        .ok_or_else(|| err("store_unavailable", "The system keychain is unavailable"))
}

/// Does a keyring request's OpenPGP work, off the writer. `input` is the
/// key file's bytes when the plan asked for a path.
pub fn pgp_prepare(plan: PgpPlan, input: Option<Vec<u8>>, kek: &dyn KekSource) -> Result<PgpProof> {
    let now = now();
    let p = plan.params.clone();
    let mut proof = PgpProof {
        writes: Vec::new(),
        assign: None,
        delete: None,
        result: Value::Null,
        plan,
    };
    let keyring = &proof.plan.keyring;
    match proof.plan.method.as_str() {
        "pgp_keys" => {
            let keys: Vec<Value> = keyring
                .keys
                .iter()
                .filter_map(|k| key_json(k, keyring, now))
                .collect();
            let accounts: BTreeMap<_, _> = keyring
                .accounts
                .iter()
                .map(|a| {
                    (
                        a.account.clone(),
                        json!({"fingerprint":a.fingerprint,"sign_by_default":a.sign_by_default,"encrypt_by_default":a.encrypt_by_default}),
                    )
                })
                .collect();
            proof.result = json!({"keys":keys,"accounts":accounts});
        }
        "pgp_key_generate" => {
            let algorithm = match p["algorithm"].as_str().unwrap_or("curve25519") {
                "curve25519" => fm_pgp::Algorithm::Curve25519,
                "rsa4096" => fm_pgp::Algorithm::Rsa4096,
                _ => {
                    return Err(err(
                        "invalid_request",
                        "algorithm must be curve25519 or rsa4096",
                    ))
                }
            };
            let request = fm_pgp::NewKey {
                name: p["name"].as_str().unwrap_or_default().into(),
                email: p["email"]
                    .as_str()
                    .ok_or_else(|| err("invalid_request", "Enter the key's email address"))?
                    .into(),
                algorithm,
                expires_in: expires_in(&p)?,
            };
            let kek = require_kek(kek)?;
            let key = fm_pgp::generate(&request).map_err(pgp_error)?;
            let cert = key.to_public_key();
            let sealed = fm_pgp::seal(&key, &kek).map_err(pgp_error)?;
            let write = write_for(&cert, Some(sealed), "generated", None, now)?;
            let fingerprint = write.fingerprint.clone();
            if let Some(account) = p["account"].as_str() {
                proof.assign = Some((
                    account.into(),
                    Some(AccountChoice {
                        account: account.into(),
                        email: String::new(),
                        fingerprint: fingerprint.clone(),
                        sign_by_default: true,
                        encrypt_by_default: false,
                    }),
                ));
            }
            proof.writes.push(write);
            proof.result =
                json!({"fingerprint":fingerprint,"key":fm_pgp::inspect(&cert, true, now)});
        }
        "pgp_key_import" => {
            let bytes = match (p["text"].as_str(), input) {
                (Some(text), _) => text.as_bytes().to_vec(),
                (None, Some(bytes)) => bytes,
                (None, None) => return Err(err("invalid_request", "Give either text or path")),
            };
            let passphrase = p["passphrase"].as_str();
            let preview = p["preview"] == true;
            let parsed = fm_pgp::parse_keys(&bytes).map_err(pgp_error)?;
            // A key file from a message may only add other people's public
            // keys: a secret key in it would become one of the person's own.
            if p["public_only"] == true
                && parsed.iter().any(|key| matches!(key, ParsedKey::Secret(_)))
            {
                return Err(err(
                    "secret_key_not_allowed",
                    "This file holds a secret key. Import your own keys in Settings → Encryption, not from mail.",
                ));
            }
            let mut merged: BTreeMap<String, Merged> = BTreeMap::new();
            let existing = |fingerprint: &str| keyring.key(fingerprint).ok();
            let mut revocations = Vec::new();
            let mut imported = Vec::new();
            let mut with_secret = std::collections::BTreeSet::new();
            let mut kek_cache: Option<Zeroizing<[u8; KEK_LEN]>> = None;
            for key in parsed {
                let (cert, secret) = match key {
                    ParsedKey::Public(cert) => (cert, None),
                    // A preview lists a secret key without unlocking it.
                    ParsedKey::Secret(secret) if preview => {
                        let cert = secret.to_public_key();
                        with_secret.insert(fm_pgp::inspect(&cert, true, now).fingerprint);
                        (cert, None)
                    }
                    ParsedKey::Secret(secret) => {
                        let unlocked = fm_pgp::unlock(&secret, passphrase).map_err(|e| {
                            let mut error = pgp_error(e);
                            if error.code == "passphrase_required" {
                                let info = fm_pgp::inspect(&secret.to_public_key(), true, now);
                                error.details = json!({"fingerprint":info.fingerprint,"user_ids":info.user_ids});
                            }
                            error
                        })?;
                        (unlocked.to_public_key(), Some(unlocked))
                    }
                    ParsedKey::Revocation(sig) => {
                        revocations.push(sig);
                        continue;
                    }
                };
                let info = fm_pgp::inspect(&cert, secret.is_some(), now);
                if info.status == KeyStatus::Invalid {
                    return Err(err(
                        "invalid_key",
                        "A key in this file has no valid self-signature",
                    ));
                }
                let fingerprint = info.fingerprint.clone();
                let entry = merged.entry(fingerprint.clone()).or_insert_with(|| {
                    match existing(&fingerprint) {
                        Some(stored) => (
                            SignedPublicKey::from_bytes(&stored.public[..]).unwrap_or(cert.clone()),
                            None,
                            Some(stored.updated_at),
                        ),
                        None => (cert.clone(), None, None),
                    }
                });
                entry.0 = fm_pgp::merge(&entry.0, &cert);
                if let Some(secret) = secret {
                    with_secret.insert(fingerprint.clone());
                    if kek_cache.is_none() {
                        kek_cache = Some(require_kek(kek)?);
                    }
                    entry.1 = Some(
                        fm_pgp::seal(&secret, kek_cache.as_ref().unwrap()).map_err(pgp_error)?,
                    );
                }
                imported.push(fingerprint);
            }
            let mut revoked = Vec::new();
            for sig in &revocations {
                // A revocation applies to a key in this file or already stored.
                let mut applied = false;
                for (fingerprint, entry) in merged.iter_mut() {
                    if fm_pgp::revocations(&mut entry.0, std::slice::from_ref(sig)) {
                        revoked.push(fingerprint.clone());
                        applied = true;
                    }
                }
                if applied {
                    continue;
                }
                for stored in &keyring.keys {
                    let Ok(mut cert) = parse_public(stored) else {
                        continue;
                    };
                    if fm_pgp::revocations(&mut cert, std::slice::from_ref(sig)) {
                        revoked.push(stored.fingerprint.clone());
                        merged.insert(
                            stored.fingerprint.clone(),
                            (cert, None, Some(stored.updated_at)),
                        );
                    }
                }
            }
            if imported.is_empty() && revoked.is_empty() {
                return Err(err(
                    "invalid_key",
                    "Nothing in this file applies to a key in the keyring",
                ));
            }
            revoked.sort();
            revoked.dedup();
            let conflicts = import_conflicts(&proof.plan, &merged, &with_secret, now);
            if preview {
                let keys: Vec<Value> = merged
                    .iter()
                    .map(|(fingerprint, (cert, _, expected))| {
                        let info = fm_pgp::inspect(cert, with_secret.contains(fingerprint), now);
                        let mut value = serde_json::to_value(info).unwrap_or_default();
                        value["action"] = json!(if expected.is_some() { "update" } else { "add" });
                        value
                    })
                    .collect();
                proof.result = json!({"keys":keys,"revoked":revoked,"conflicts":conflicts});
                return Ok(proof);
            }
            if !conflicts.is_empty() && p["replace"] != true {
                let mut error = err(
                    "pgp_key_conflict",
                    "A key in this file names an address another key already has. Compare its fingerprint with its owner's, then import it to use it instead.",
                );
                error.details = json!({"conflicts":conflicts});
                return Err(error);
            }
            let mut added = Vec::new();
            let mut updated = Vec::new();
            // Keys new together are stored oldest first, so the newest of
            // them is the one their address uses.
            let mut merged: Vec<(String, Merged)> = merged.into_iter().collect();
            merged.sort_by_key(|(_, (cert, _, _))| fm_pgp::inspect(cert, false, now).created_at);
            for (fingerprint, (cert, secret, expected)) in merged {
                if expected.is_some() {
                    updated.push(fingerprint.clone());
                } else {
                    added.push(fingerprint.clone());
                }
                // Without a new secret, a stored one stays as it is.
                proof
                    .writes
                    .push(write_for(&cert, secret, "imported", expected, now)?);
            }
            proof.result = json!({"added":added,"updated":updated,"revoked":revoked});
        }
        "pgp_key_export" => {
            let fingerprint = string(&p, "fingerprint")?;
            let stored = keyring.key(fingerprint)?;
            let cert = parse_public(stored)?;
            let info = fm_pgp::inspect(&cert, stored.secret.is_some(), now);
            let short = &info.key_id[8..];
            let (armored, filename) = match p["kind"].as_str().unwrap_or("public") {
                "public" => (
                    fm_pgp::export_public(&cert).map_err(pgp_error)?,
                    format!("{short}-public.asc"),
                ),
                kind @ ("secret" | "revocation") => {
                    if stored.secret.is_none() {
                        return Err(err("no_secret_key", "This is someone else's key"));
                    }
                    let kek = kek.kek(false)?.ok_or_else(|| {
                        err(
                            "key_locked",
                            "The key that protects your secret keys is missing",
                        )
                    })?;
                    let secret = unseal(stored, &kek)?;
                    if kind == "secret" {
                        let passphrase = p["passphrase"].as_str().ok_or_else(|| {
                            err(
                                "invalid_request",
                                "Choose a passphrase for the exported key",
                            )
                        })?;
                        (
                            fm_pgp::export_secret(&secret, passphrase)
                                .map_err(pgp_error)?
                                .to_string(),
                            format!("{short}-secret.asc"),
                        )
                    } else {
                        (
                            fm_pgp::export_revocation(&secret, now).map_err(pgp_error)?,
                            format!("{short}-revocation.asc"),
                        )
                    }
                }
                _ => {
                    return Err(err(
                        "invalid_request",
                        "kind must be public, secret or revocation",
                    ))
                }
            };
            proof.result =
                json!({"fingerprint":info.fingerprint,"armored":armored,"filename":filename});
        }
        "pgp_key_expiry" => {
            let fingerprint = string(&p, "fingerprint")?;
            let stored = keyring.key(fingerprint)?;
            if stored.secret.is_none() {
                return Err(err(
                    "no_secret_key",
                    "Only your own keys can be renewed; ask the owner for an update",
                ));
            }
            let lifetime = expires_in(&p)?;
            let kek = require_kek(kek)?;
            let secret = unseal(stored, &kek)?;
            let renewed = fm_pgp::set_expiry(&secret, lifetime, now).map_err(pgp_error)?;
            let cert = fm_pgp::merge(&parse_public(stored)?, &renewed.to_public_key());
            // The origin only matters for a new row; an update keeps it.
            let write = write_for(&cert, None, "imported", Some(stored.updated_at), now)?;
            let info = fm_pgp::inspect(&cert, true, now);
            proof.writes.push(write);
            proof.result = json!({"fingerprint":info.fingerprint,"expires_at":info.expires_at});
        }
        "pgp_key_delete" => {
            let fingerprint = string(&p, "fingerprint")?;
            let stored = keyring.key(fingerprint)?;
            proof.delete = Some(stored.fingerprint.clone());
            proof.result = json!({"deleted":stored.fingerprint,"used_by":keyring
                .accounts
                .iter()
                .filter(|a| a.fingerprint == stored.fingerprint)
                .map(|a| a.account.clone())
                .collect::<Vec<_>>()});
        }
        "pgp_account" => {
            let account = proof.plan.account.clone().unwrap();
            let choice = match &p["fingerprint"] {
                Value::Null => None,
                Value::String(fingerprint) => {
                    let stored = keyring.key(fingerprint)?;
                    if stored.secret.is_none() {
                        return Err(err(
                            "pgp_key_unusable",
                            "Choose one of your own keys, with its secret key",
                        ));
                    }
                    let cert = parse_public(stored)?;
                    let info = fm_pgp::inspect(&cert, true, now);
                    let email = proof
                        .plan
                        .account_email
                        .clone()
                        .unwrap_or_default()
                        .to_lowercase();
                    if !info.addresses.contains(&email) {
                        return Err(err(
                            "pgp_key_unusable",
                            "That key isn't made for this account's address",
                        ));
                    }
                    Some(AccountChoice {
                        account: account.clone(),
                        email,
                        fingerprint: stored.fingerprint.clone(),
                        sign_by_default: boolean(&p, "sign_by_default")?,
                        encrypt_by_default: boolean(&p, "encrypt_by_default")?,
                    })
                }
                _ => {
                    return Err(err(
                        "invalid_request",
                        "fingerprint must be a key fingerprint or null",
                    ))
                }
            };
            proof.result = match &choice {
                Some(c) => {
                    json!({"account":account,"pgp":{"fingerprint":c.fingerprint,"sign_by_default":c.sign_by_default,"encrypt_by_default":c.encrypt_by_default}})
                }
                None => json!({"account":account,"pgp":null}),
            };
            proof.assign = Some((account, choice));
        }
        "pgp_recipients" => {
            let addresses = p["addresses"]
                .as_array()
                .filter(|a| a.len() <= 100)
                .ok_or_else(|| err("invalid_request", "addresses must be a list of at most 100"))?;
            let mut out = Vec::new();
            for address in addresses {
                let address = address
                    .as_str()
                    .ok_or_else(|| err("invalid_request", "addresses must be text"))?;
                out.push(recipient_status(keyring, address, now));
            }
            let own = proof
                .plan
                .account
                .as_deref()
                .and_then(|account| keyring.accounts.iter().find(|a| a.account == account));
            proof.result = json!({
                "recipients": out,
                "account": own.map(|a| json!({"fingerprint":a.fingerprint,"sign_by_default":a.sign_by_default,"encrypt_by_default":a.encrypt_by_default})),
            });
        }
        _ => return Err(err("unsupported_method", "Unknown keyring method")),
    }
    Ok(proof)
}

/// The addresses keys in an import would newly name that another stored key
/// already names, or that belong to one of the person's accounts (unless the
/// key is their own, with its secret). Whichever key was imported last is
/// the one mail to an address is encrypted to, so taking an address over
/// needs the person's say-so: `{address, fingerprint, holders, own}`.
fn import_conflicts(
    plan: &PgpPlan,
    merged: &BTreeMap<String, Merged>,
    with_secret: &std::collections::BTreeSet<String>,
    now: u64,
) -> Vec<Value> {
    let stored: Vec<(&StoredKey, Vec<String>)> = plan
        .keyring
        .keys
        .iter()
        .map(|key| {
            let addresses = parse_public(key)
                .map(|cert| fm_pgp::inspect(&cert, false, now).addresses)
                .unwrap_or_default();
            (key, addresses)
        })
        .collect();
    let mut conflicts = Vec::new();
    for (fingerprint, (cert, _, _)) in merged {
        let before = stored
            .iter()
            .find(|(key, _)| &key.fingerprint == fingerprint)
            .map(|(_, addresses)| addresses.clone())
            .unwrap_or_default();
        let own_key = with_secret.contains(fingerprint)
            || stored
                .iter()
                .any(|(key, _)| &key.fingerprint == fingerprint && key.secret.is_some());
        for address in fm_pgp::inspect(cert, false, now).addresses {
            if before.contains(&address) {
                continue;
            }
            let holders: Vec<&str> = stored
                .iter()
                .filter(|(key, addresses)| {
                    &key.fingerprint != fingerprint && addresses.contains(&address)
                })
                .map(|(key, _)| key.fingerprint.as_str())
                .collect();
            let own = !own_key && plan.own_addresses.contains(&address);
            if !holders.is_empty() || own {
                conflicts.push(json!({"address":address,"fingerprint":fingerprint,"holders":holders,"own":own}));
            }
        }
    }
    conflicts
}

/// Certificates for `address` that can encrypt now, the one the person
/// stored last first. A key's own creation time is its maker's to choose, so
/// it only orders keys imported together from one file.
pub(crate) fn encryption_certs(
    keyring: &Keyring,
    address: &str,
    now: u64,
) -> Vec<(String, SignedPublicKey)> {
    let wanted = address.trim().to_lowercase();
    let mut found: Vec<(usize, String, SignedPublicKey)> = keyring
        .keys
        .iter()
        .enumerate()
        .filter_map(|(stored, k)| {
            let cert = SignedPublicKey::from_bytes(&k.public[..]).ok()?;
            let info = fm_pgp::inspect(&cert, k.secret.is_some(), now);
            (info.addresses.contains(&wanted) && info.can_encrypt).then_some((
                stored,
                info.fingerprint,
                cert,
            ))
        })
        .collect();
    found.sort_by_key(|(stored, _, _)| std::cmp::Reverse(*stored));
    found.into_iter().map(|(_, f, c)| (f, c)).collect()
}

/// Whether mail to `address` can be encrypted: `ready` with the key it would
/// use, `unusable` when its keys expired or were revoked, or `missing`.
fn recipient_status(keyring: &Keyring, address: &str, now: u64) -> Value {
    let wanted = address.trim().to_lowercase();
    if let Some((fingerprint, cert)) = encryption_certs(keyring, &wanted, now).into_iter().next() {
        let info = fm_pgp::inspect(&cert, false, now);
        return json!({"address":wanted,"status":"ready","fingerprint":fingerprint,"user_id":info.user_ids.first()});
    }
    let known = keyring.keys.iter().any(|k| {
        SignedPublicKey::from_bytes(&k.public[..])
            .map(|c| fm_pgp::inspect(&c, false, now).addresses.contains(&wanted))
            .unwrap_or(false)
    });
    json!({"address":wanted,"status":if known {"unusable"} else {"missing"}})
}

/// What a send must do: sign with the account's key, encrypt to everyone, or both.
pub struct OutgoingProtection {
    pub sign: bool,
    pub encrypt: bool,
    keyring: Keyring,
    own: String,
    /// Each recipient address and whether it was Bcc.
    recipients: Vec<(String, bool)>,
}

impl Store {
    /// The protection a draft asks for, checked against the keyring. Fails
    /// with `pgp_not_set_up` when the account has no key, and
    /// `pgp_recipient_key_missing` (details: `addresses`) when encrypting to
    /// someone without a key.
    pub(crate) fn pgp_outgoing(
        &self,
        account: &str,
        draft: &Value,
    ) -> Result<Option<OutgoingProtection>> {
        let sign = draft["pgp_sign"] == true;
        let encrypt = draft["pgp_encrypt"] == true;
        if !sign && !encrypt {
            return Ok(None);
        }
        let keyring = self.pgp_keyring()?;
        let own = keyring
            .accounts
            .iter()
            .find(|a| a.account == account)
            .map(|a| a.fingerprint.clone())
            .ok_or_else(|| {
                err(
                    "pgp_not_set_up",
                    "Choose a key for this account in Settings → Encryption, or turn off Sign and Encrypt",
                )
            })?;
        let mut recipients = Vec::new();
        for (field, hidden) in [
            ("to_recipients", false),
            ("cc_recipients", false),
            ("bcc_recipients", true),
        ] {
            for row in draft[field].as_array().into_iter().flatten() {
                if let Some(email) = row["email"].as_str() {
                    recipients.push((email.to_lowercase(), hidden));
                }
            }
        }
        if encrypt {
            let missing: Vec<&str> = recipients
                .iter()
                .filter(|(address, _)| {
                    !keyring.keys.iter().any(|k| {
                        SignedPublicKey::from_bytes(&k.public[..])
                            .map(|c| {
                                fm_pgp::inspect(&c, false, now())
                                    .addresses
                                    .contains(address)
                            })
                            .unwrap_or(false)
                    })
                })
                .map(|(a, _)| a.as_str())
                .collect();
            if !missing.is_empty() {
                let mut error = err(
                    "pgp_recipient_key_missing",
                    "Some recipients have no key in your keyring; import theirs or turn off Encrypt",
                );
                error.details = json!({"addresses":missing});
                return Err(error);
            }
        }
        Ok(Some(OutgoingProtection {
            sign,
            encrypt,
            keyring,
            own,
            recipients,
        }))
    }
}

/// Signs and encrypts built MIME as the draft asked. Runs off the writer.
/// Fails with `pgp_recipient_key_unusable` (details: `addresses`) when a
/// recipient's keys can't encrypt any more, and never sends a weaker message
/// instead.
pub(crate) fn protect_outgoing(
    raw: &[u8],
    protection: &OutgoingProtection,
    kek: &dyn KekSource,
) -> Result<Vec<u8>> {
    let now = now();
    let own = protection.keyring.key(&protection.own)?;
    let own_cert = parse_public(own)?;
    let kek_value = kek.kek(false)?.ok_or_else(|| {
        err(
            "key_locked",
            "The key that protects your secret keys is missing",
        )
    })?;
    let secret = unseal(own, &kek_value)?;
    let signer = if protection.sign {
        Some(fm_pgp::signing_key(&own_cert, &secret, now).ok_or_else(|| {
            err(
                "pgp_key_unusable",
                "Your key can't sign: it has expired or been revoked",
            )
        })?)
    } else {
        None
    };
    if !protection.encrypt {
        return fm_pgp::sign_message(raw, signer.unwrap()).map_err(pgp_error);
    }
    let mut certs = Vec::new();
    let mut unusable = Vec::new();
    for (address, hidden) in &protection.recipients {
        match encryption_certs(&protection.keyring, address, now)
            .into_iter()
            .next()
        {
            Some((_, cert)) => certs.push((cert, *hidden)),
            None => unusable.push(address.clone()),
        }
    }
    if !unusable.is_empty() {
        let mut error = err(
            "pgp_recipient_key_unusable",
            "Some recipients' keys have expired or been revoked",
        );
        error.details = json!({"addresses":unusable});
        return Err(error);
    }
    if fm_pgp::encryption_keys(&own_cert, now).is_empty() {
        return Err(err(
            "pgp_key_unusable",
            "Your key can't encrypt: it has expired or been revoked",
        ));
    }
    // The sender can always read their own sent copy.
    let mut recipients: Vec<fm_pgp::Recipient<'_>> = certs
        .iter()
        .map(|(cert, hidden)| fm_pgp::Recipient {
            cert,
            hidden: *hidden,
        })
        .collect();
    recipients.push(fm_pgp::Recipient {
        cert: &own_cert,
        hidden: false,
    });
    fm_pgp::encrypt_message(raw, &recipients, signer, now).map_err(pgp_error)
}

/// The keyring and key source a read opens protected mail with, off the
/// writer, and whether it decrypts.
#[derive(Clone)]
pub struct ReadKeys {
    keyring: Keyring,
    kek: std::sync::Arc<dyn KekSource>,
    decrypt: bool,
}

/// A message as a reader should see it.
pub struct Readable {
    /// The decrypted or unwrapped message, or the source itself.
    pub bytes: Zeroizing<Vec<u8>>,
    pub opened: Option<fm_pgp::Opened>,
    /// Sealed mail names one of the person's own keys among its recipients
    /// (or hides them), so decrypting it may work.
    pub can_decrypt: bool,
}

impl Readable {
    /// The content came from decrypting, so it must not be cached on disk.
    pub fn decrypted(&self) -> bool {
        self.opened
            .as_ref()
            .is_some_and(|o| matches!(o.decryption, Some(fm_pgp::Decryption::Decrypted)))
    }

    /// The `pgp` status message reads return, or `None` for ordinary mail. A
    /// sealed message's `decryption` says whether it `can_decrypt`.
    pub fn pgp_json(&self) -> Option<Value> {
        let opened = self.opened.as_ref()?;
        let mut out = opened_json(opened);
        if matches!(opened.decryption, Some(fm_pgp::Decryption::Sealed { .. })) {
            out["decryption"]["can_decrypt"] = self.can_decrypt.into();
        }
        Some(out)
    }
}

impl ReadKeys {
    /// Whether reads decrypt; without it encrypted mail reads sealed.
    pub fn decrypting(mut self, decrypt: bool) -> Self {
        self.decrypt = decrypt;
        self
    }

    pub fn decrypts(&self) -> bool {
        self.decrypt
    }

    /// Opens `raw` when it is protected. A message that couldn't be
    /// decrypted, or was left sealed, reads as its source, with the reason in
    /// `opened`.
    pub fn readable(&self, raw: Vec<u8>) -> Readable {
        let mut opened = self.keyring.open(&raw, self.kek.as_ref(), self.decrypt);
        let can_decrypt = match opened.as_ref().and_then(|o| o.decryption.as_ref()) {
            Some(fm_pgp::Decryption::Sealed { recipients }) => {
                let own = self.keyring.own_key_ids();
                !own.is_empty()
                    && recipients
                        .iter()
                        .any(|id| id.chars().all(|c| c == '0') || own.contains(id))
            }
            _ => false,
        };
        let bytes = match opened.as_mut().and_then(|o| o.message.take()) {
            Some(message) => message,
            None => Zeroizing::new(raw),
        };
        Readable {
            bytes,
            opened,
            can_decrypt,
        }
    }
}

impl Store {
    /// Whether mail can be encrypted to every one of `addresses`: there is at
    /// least one, and each has a key that can encrypt now.
    pub(crate) fn pgp_can_encrypt_to(&self, addresses: &[String]) -> Result<bool> {
        if addresses.is_empty() {
            return Ok(false);
        }
        let keyring = self.pgp_keyring()?;
        let now = now();
        Ok(addresses
            .iter()
            .all(|address| !encryption_certs(&keyring, address, now).is_empty()))
    }

    /// A keyring snapshot for reading mail off the writer.
    pub fn pgp_read_keys(&self) -> Result<ReadKeys> {
        Ok(ReadKeys {
            keyring: self.pgp_keyring()?,
            kek: self.pgp_kek(),
            decrypt: true,
        })
    }
}

/// The attachments of a decrypted message, as `conversation` lists stored
/// ones: `{id, name, mime_type, size, state}`. The id names the message, the
/// part and its size and SHA-256, so the `attachment` method can find and
/// verify it again by decrypting.
pub fn decrypted_attachments(message: &str, parsed: &mail_parser::Message<'_>) -> Vec<Value> {
    let inline = fm_mail::html_view::inline_content_ids(parsed);
    parsed
        .attachments()
        .enumerate()
        .filter(|(_, part)| {
            !fm_mail::html_view::part_content_id(part).is_some_and(|id| inline.contains(&id))
        })
        .take(crate::google_store::MAX_RECEIVED_ATTACHMENTS)
        .map(|(index, part)| attachment_entry(message, index, part))
        .collect()
}

fn attachment_entry(message: &str, index: usize, part: &mail_parser::MessagePart<'_>) -> Value {
    use mail_parser::MimeHeaders;
    let bytes = part.contents();
    let mime_type = part
        .content_type()
        .map(|ct| {
            format!(
                "{}/{}",
                ct.c_type,
                ct.c_subtype.as_deref().unwrap_or("octet-stream")
            )
        })
        .unwrap_or_else(|| "application/octet-stream".into());
    let name: String = part
        .attachment_name()
        .unwrap_or("attachment.bin")
        .chars()
        .filter(|c| !c.is_control())
        .take(255)
        .collect();
    json!({
        "id": format!("pgp:{message}:{index}:{}:{:x}", bytes.len(), Sha256::digest(bytes)),
        "name": name,
        "mime_type": mime_type,
        "size": bytes.len(),
        "state": "available",
    })
}

/// A decrypted attachment named by an id from [`decrypted_attachments`]:
/// the message, the part index, and its size and SHA-256.
pub(crate) struct DecryptedPart {
    pub message: String,
    pub index: usize,
    pub size: u64,
    pub hash: String,
}

impl DecryptedPart {
    pub(crate) fn parse(id: &str) -> Option<Self> {
        let rest = id.strip_prefix("pgp:")?;
        let mut fields = rest.rsplitn(4, ':');
        let hash = fields.next()?;
        let size = fields.next()?.parse().ok()?;
        let index = fields.next()?.parse().ok()?;
        let message = fields.next()?;
        (hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) && !message.is_empty())
            .then(|| Self {
                message: message.into(),
                index,
                size,
                hash: hash.to_ascii_lowercase(),
            })
    }
}

/// The name, type and bytes of the decrypted attachment `part` names in
/// `raw`, checked against the size and hash its id promised.
pub(crate) fn decrypted_part(
    keys: &ReadKeys,
    raw: Vec<u8>,
    part: &DecryptedPart,
) -> Result<(String, String, Vec<u8>)> {
    let unavailable = || {
        err(
            "content_unavailable",
            "This attachment is inside an encrypted message that can't be decrypted now",
        )
    };
    let readable = keys.readable(raw);
    if !readable.decrypted() {
        return Err(unavailable());
    }
    let parsed = mail_parser::MessageParser::default()
        .parse(&readable.bytes[..])
        .ok_or_else(unavailable)?;
    let attachment = parsed
        .attachments()
        .nth(part.index)
        .ok_or_else(unavailable)?;
    let entry = attachment_entry(&part.message, part.index, attachment);
    let bytes = attachment.contents();
    if bytes.len() as u64 != part.size || format!("{:x}", Sha256::digest(bytes)) != part.hash {
        return Err(err(
            "content_unavailable",
            "This attachment failed verification",
        ));
    }
    Ok((
        entry["name"].as_str().unwrap_or("attachment.bin").into(),
        entry["mime_type"]
            .as_str()
            .unwrap_or("application/octet-stream")
            .into(),
        bytes.to_vec(),
    ))
}
