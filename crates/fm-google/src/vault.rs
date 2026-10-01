//! Native credentials only. No filesystem store, enumerator, or fallback backend.
use crate::auth::{CredentialVault, StoredToken};
use crate::{AccountId, GoogleError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Mutex;
use zeroize::Zeroizing;

/// Wraps a vault and remembers its failures per account, so one refused
/// keychain prompt is not shown again by every worker.
///
/// A native refusal is shared by every worker until an explicit user retry.
/// Calls for one account are serialized through that account's lock, held
/// inside the blocking operation, including caller cancellation. Accounts do
/// not wait for each other: a keychain prompt left open for one account never
/// holds up another, nor [`CredentialVault::retry_access`].
/// After a failed load or remove, or a store the OS refused, the account's
/// loads fail with `StoreUnavailable` without reaching the inner vault, until
/// `retry_access` or a successful `store` or `remove`.
pub struct SessionCredentialVault<V: ?Sized> {
    inner: std::sync::Arc<V>,
    accounts: Mutex<std::collections::BTreeMap<AccountId, std::sync::Arc<Mutex<()>>>>,
    denied: Mutex<std::collections::BTreeSet<AccountId>>,
}
impl<V: CredentialVault + ?Sized> SessionCredentialVault<V> {
    pub fn new(inner: std::sync::Arc<V>) -> Self {
        Self {
            inner,
            accounts: Mutex::new(Default::default()),
            denied: Mutex::new(Default::default()),
        }
    }
    /// The lock serializing `account`'s calls to the inner vault.
    fn account(&self, account: &AccountId) -> Result<std::sync::Arc<Mutex<()>>, GoogleError> {
        Ok(self
            .accounts
            .lock()
            .map_err(|_| GoogleError::StoreUnavailable)?
            .entry(account.clone())
            .or_default()
            .clone())
    }
    /// Records whether `account`'s loads are refused. The set's lock is never
    /// held while the inner vault runs.
    fn set_denied(&self, account: &AccountId, denied: bool) -> Result<(), GoogleError> {
        let mut set = self
            .denied
            .lock()
            .map_err(|_| GoogleError::StoreUnavailable)?;
        if denied {
            set.insert(account.clone());
        } else {
            set.remove(account);
        }
        Ok(())
    }
    fn is_denied(&self, account: &AccountId) -> Result<bool, GoogleError> {
        Ok(self
            .denied
            .lock()
            .map_err(|_| GoogleError::StoreUnavailable)?
            .contains(account))
    }
}
impl<V: CredentialVault + ?Sized> CredentialVault for SessionCredentialVault<V> {
    fn retry_access(&self, account: &AccountId) -> Result<(), GoogleError> {
        self.set_denied(account, false)
    }
    fn load(&self, account: &AccountId) -> Result<Option<StoredToken>, GoogleError> {
        let lock = self.account(account)?;
        let _held = lock.lock().map_err(|_| GoogleError::StoreUnavailable)?;
        if self.is_denied(account)? {
            return Err(GoogleError::StoreUnavailable);
        }
        let result = self.inner.load(account);
        if result.is_err() {
            self.set_denied(account, true)?;
        }
        result
    }
    fn store(&self, account: &AccountId, token: StoredToken) -> Result<(), GoogleError> {
        let lock = self.account(account)?;
        let _held = lock.lock().map_err(|_| GoogleError::StoreUnavailable)?;
        // A verified reconnect may replace the credential even after denial.
        // Refresh always loads first, so it cannot bypass the blocked load.
        let result = self.inner.store(account, token);
        match &result {
            Ok(()) => self.set_denied(account, false)?,
            // Only a refusal by the OS store blocks later loads. A token the
            // inner vault rejected as invalid never reached the store.
            Err(GoogleError::StoreUnavailable) => self.set_denied(account, true)?,
            Err(_) => {}
        }
        result
    }
    fn remove(&self, account: &AccountId) -> Result<(), GoogleError> {
        let lock = self.account(account)?;
        let _held = lock.lock().map_err(|_| GoogleError::StoreUnavailable)?;
        let result = self.inner.remove(account);
        self.set_denied(account, result.is_err())?;
        result
    }
}

/// Stores tokens in the OS credential store: Windows Credential Manager, the
/// macOS Keychain or the Linux Secret Service.
///
/// Core supplies a random, persistent per-profile namespace. It is not a token.
/// Native errors are deliberately collapsed before reaching diagnostics.
/// Entries use the service `fuckmail.google.v1.<sha256 of namespace>` and the
/// SHA-256 of the account ID as the user, so neither email nor account ID is
/// visible in the store. Operations are serialized; on other platforms every
/// call fails with `StoreUnavailable`.
pub struct NativeCredentialVault {
    service: String,
    operations: Mutex<()>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VaultRecord {
    version: u8,
    access_token: String,
    refresh_token: String,
    expires_at_secs: u64,
}

impl NativeCredentialVault {
    /// Errors with `InvalidResponse` unless the namespace is 16–256 ASCII
    /// letters, digits, `-` or `_`.
    pub fn new(profile_namespace: &str) -> Result<Self, GoogleError> {
        if !(16..=256).contains(&profile_namespace.len())
            || !profile_namespace
                .bytes()
                .all(|v| v.is_ascii_alphanumeric() || v == b'-' || v == b'_')
        {
            return Err(GoogleError::InvalidResponse(
                "invalid credential profile namespace",
            ));
        }
        Ok(Self {
            service: format!(
                "fuckmail.google.v1.{:x}",
                Sha256::digest(profile_namespace.as_bytes())
            ),
            operations: Mutex::new(()),
        })
    }

    /// Opaque reference is safe for Core metadata; it contains neither email nor token.
    pub fn reference(&self, account: &AccountId) -> String {
        format!(
            "{}.{:x}",
            self.service,
            Sha256::digest(account.as_str().as_bytes())
        )
    }

    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    fn entry(&self, account: &AccountId) -> Result<keyring::Entry, GoogleError> {
        #[cfg(target_os = "windows")]
        let builder = keyring::windows::default_credential_builder();
        #[cfg(target_os = "macos")]
        let builder = keyring::macos::default_credential_builder();
        #[cfg(target_os = "linux")]
        let builder = keyring::secret_service::default_credential_builder();
        let user = format!("{:x}", Sha256::digest(account.as_str().as_bytes()));
        builder
            .build(None, &self.service, &user)
            .map(keyring::Entry::new_with_credential)
            .map_err(|_| GoogleError::StoreUnavailable)
    }

    /// Explicit credential cleanup only. Disconnect does not call this automatically.
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    pub fn remove(&self, account: &AccountId) -> Result<(), GoogleError> {
        let _guard = self
            .operations
            .lock()
            .map_err(|_| GoogleError::StoreUnavailable)?;
        match self.entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(GoogleError::StoreUnavailable),
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    pub fn remove(&self, _: &AccountId) -> Result<(), GoogleError> {
        Err(GoogleError::StoreUnavailable)
    }
}

impl CredentialVault for NativeCredentialVault {
    fn remove(&self, account: &AccountId) -> Result<(), GoogleError> {
        NativeCredentialVault::remove(self, account)
    }

    fn load(&self, account: &AccountId) -> Result<Option<StoredToken>, GoogleError> {
        let _guard = self
            .operations
            .lock()
            .map_err(|_| GoogleError::StoreUnavailable)?;
        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        {
            let bytes = match self.entry(account)?.get_secret() {
                Ok(bytes) => Zeroizing::new(bytes),
                Err(keyring::Error::NoEntry) => return Ok(None),
                Err(_) => return Err(GoogleError::StoreUnavailable),
            };
            // An oversized, corrupt or unknown-version record is reported as
            // unavailable, not absent: absence would read as signed out.
            if bytes.len() > 32 * 1024 {
                return Err(GoogleError::StoreUnavailable);
            }
            let record: VaultRecord =
                serde_json::from_slice(&bytes).map_err(|_| GoogleError::StoreUnavailable)?;
            if record.version != 1
                || record.access_token.is_empty()
                || record.refresh_token.is_empty()
            {
                return Err(GoogleError::StoreUnavailable);
            }
            Ok(Some(StoredToken {
                access_token: record.access_token,
                refresh_token: record.refresh_token,
                expires_at_secs: record.expires_at_secs,
            }))
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            let _ = account;
            Err(GoogleError::StoreUnavailable)
        }
    }

    fn store(&self, account: &AccountId, token: StoredToken) -> Result<(), GoogleError> {
        let _guard = self
            .operations
            .lock()
            .map_err(|_| GoogleError::StoreUnavailable)?;
        if token.access_token.is_empty()
            || token.refresh_token.is_empty()
            || token.access_token.len() > 8192
            || token.refresh_token.len() > 8192
        {
            return Err(GoogleError::InvalidResponse("invalid token storage record"));
        }
        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        {
            let record = VaultRecord {
                version: 1,
                access_token: token.access_token,
                refresh_token: token.refresh_token,
                expires_at_secs: token.expires_at_secs,
            };
            let bytes = Zeroizing::new(
                serde_json::to_vec(&record).map_err(|_| GoogleError::StoreUnavailable)?,
            );
            self.entry(account)?
                .set_secret(&bytes)
                .map_err(|_| GoogleError::StoreUnavailable)
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            let _ = (account, token);
            Err(GoogleError::StoreUnavailable)
        }
    }
}

/// One app-level secret in the OS vault under its own service name, such as
/// the remote-content proxy password. Never Google credentials.
pub struct NativeSecret {
    service: String,
    user: &'static str,
    operations: Mutex<()>,
}

impl NativeSecret {
    /// The remote-content proxy password entry for this profile namespace,
    /// which is validated as in [`NativeCredentialVault::new`].
    pub fn proxy_password(profile_namespace: &str) -> Result<Self, GoogleError> {
        if !(16..=256).contains(&profile_namespace.len())
            || !profile_namespace
                .bytes()
                .all(|v| v.is_ascii_alphanumeric() || v == b'-' || v == b'_')
        {
            return Err(GoogleError::InvalidResponse(
                "invalid credential profile namespace",
            ));
        }
        Ok(Self {
            service: format!(
                "fuckmail.proxy.v1.{:x}",
                Sha256::digest(profile_namespace.as_bytes())
            ),
            user: "remote-content",
            operations: Mutex::new(()),
        })
    }

    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    fn entry(&self) -> Result<keyring::Entry, GoogleError> {
        #[cfg(target_os = "windows")]
        let builder = keyring::windows::default_credential_builder();
        #[cfg(target_os = "macos")]
        let builder = keyring::macos::default_credential_builder();
        #[cfg(target_os = "linux")]
        let builder = keyring::secret_service::default_credential_builder();
        builder
            .build(None, &self.service, self.user)
            .map(keyring::Entry::new_with_credential)
            .map_err(|_| GoogleError::StoreUnavailable)
    }

    /// Returns the secret, or `None` if no entry exists. `StoreUnavailable`
    /// if the store fails or the entry is over 4 KiB or not UTF-8.
    pub fn load(&self) -> Result<Option<Zeroizing<String>>, GoogleError> {
        let _guard = self
            .operations
            .lock()
            .map_err(|_| GoogleError::StoreUnavailable)?;
        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        {
            let bytes = match self.entry()?.get_secret() {
                Ok(bytes) => Zeroizing::new(bytes),
                Err(keyring::Error::NoEntry) => return Ok(None),
                Err(_) => return Err(GoogleError::StoreUnavailable),
            };
            if bytes.len() > 4096 {
                return Err(GoogleError::StoreUnavailable);
            }
            String::from_utf8(bytes.to_vec())
                .map(|s| Some(Zeroizing::new(s)))
                .map_err(|_| GoogleError::StoreUnavailable)
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        Err(GoogleError::StoreUnavailable)
    }

    /// Saves a secret of 1–1024 bytes, replacing any previous one.
    pub fn store(&self, secret: &str) -> Result<(), GoogleError> {
        let _guard = self
            .operations
            .lock()
            .map_err(|_| GoogleError::StoreUnavailable)?;
        if secret.is_empty() || secret.len() > 1024 {
            return Err(GoogleError::InvalidResponse("invalid secret"));
        }
        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        {
            self.entry()?
                .set_secret(secret.as_bytes())
                .map_err(|_| GoogleError::StoreUnavailable)
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        Err(GoogleError::StoreUnavailable)
    }

    /// Deletes the entry; an absent entry is success.
    pub fn remove(&self) -> Result<(), GoogleError> {
        let _guard = self
            .operations
            .lock()
            .map_err(|_| GoogleError::StoreUnavailable)?;
        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        {
            match self.entry()?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(_) => Err(GoogleError::StoreUnavailable),
            }
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        Err(GoogleError::StoreUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn namespaces_are_profile_and_account_scoped() {
        let one = NativeCredentialVault::new("fixture-profile-one-123456").unwrap();
        let two = NativeCredentialVault::new("fixture-profile-two-123456").unwrap();
        let a = AccountId::new("fixture-one").unwrap();
        let b = AccountId::new("fixture-two").unwrap();
        assert_ne!(one.reference(&a), two.reference(&a));
        assert_ne!(one.reference(&a), one.reference(&b));
        assert!(!one.reference(&a).contains("fixture"));
        assert!(NativeCredentialVault::new("short").is_err());
    }
}

#[cfg(test)]
mod denial_tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    };
    #[derive(Default)]
    struct Vault {
        loads: AtomicUsize,
        denied: AtomicBool,
    }
    impl CredentialVault for Vault {
        fn load(&self, _: &AccountId) -> Result<Option<StoredToken>, GoogleError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            if self.denied.load(Ordering::SeqCst) {
                Err(GoogleError::StoreUnavailable)
            } else {
                Ok(None)
            }
        }
        fn store(&self, _: &AccountId, token: StoredToken) -> Result<(), GoogleError> {
            if token.refresh_token.is_empty() {
                Err(GoogleError::InvalidResponse("invalid secret"))
            } else if self.denied.load(Ordering::SeqCst) {
                Err(GoogleError::StoreUnavailable)
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn concurrent_denial_is_latched_until_explicit_retry_and_other_accounts_work() {
        let inner = Arc::new(Vault::default());
        inner.denied.store(true, Ordering::SeqCst);
        let vault = Arc::new(SessionCredentialVault::new(inner.clone()));
        let account = AccountId::new("denied").unwrap();
        let workers: Vec<_> = (0..12)
            .map(|_| {
                let v = vault.clone();
                let a = account.clone();
                std::thread::spawn(move || {
                    assert!(matches!(v.load(&a), Err(GoogleError::StoreUnavailable)))
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(inner.loads.load(Ordering::SeqCst), 1);
        inner.denied.store(false, Ordering::SeqCst);
        assert!(matches!(
            vault.load(&account),
            Err(GoogleError::StoreUnavailable)
        ));
        assert!(vault
            .load(&AccountId::new("other").unwrap())
            .unwrap()
            .is_none());
        vault.retry_access(&account).unwrap();
        assert!(vault.load(&account).unwrap().is_none());
        assert_eq!(inner.loads.load(Ordering::SeqCst), 3);
    }
    #[test]
    fn a_prompt_left_open_for_one_account_holds_up_no_other() {
        // The inner vault blocks "waiting" until released, as a keychain
        // prompt does until the user answers it.
        struct Prompt {
            release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
        }
        impl CredentialVault for Prompt {
            fn load(&self, account: &AccountId) -> Result<Option<StoredToken>, GoogleError> {
                if account.as_str() == "waiting" {
                    self.release.lock().unwrap().recv().unwrap();
                }
                Ok(None)
            }
            fn store(&self, _: &AccountId, _: StoredToken) -> Result<(), GoogleError> {
                Ok(())
            }
        }
        let (answer, release) = std::sync::mpsc::channel();
        let vault = Arc::new(SessionCredentialVault::new(Arc::new(Prompt {
            release: std::sync::Mutex::new(release),
        })));
        let waiting = {
            let vault = vault.clone();
            std::thread::spawn(move || vault.load(&AccountId::new("waiting").unwrap()))
        };
        std::thread::sleep(std::time::Duration::from_millis(50));
        let other = AccountId::new("other").unwrap();
        let (done, finished) = std::sync::mpsc::channel();
        {
            let vault = vault.clone();
            std::thread::spawn(move || {
                vault.retry_access(&other).unwrap();
                vault
                    .retry_access(&AccountId::new("waiting").unwrap())
                    .unwrap();
                done.send(vault.load(&other).unwrap().is_none()).unwrap();
            });
        }
        assert!(
            finished
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap(),
            "another account and retries go ahead"
        );
        answer.send(()).unwrap();
        assert!(waiting.join().unwrap().unwrap().is_none());
    }
    #[test]
    fn failed_refresh_storage_latches_and_verified_reconnect_clears_it() {
        let inner = Arc::new(Vault::default());
        let vault = SessionCredentialVault::new(inner.clone());
        let account = AccountId::new("account").unwrap();
        let token = || StoredToken {
            access_token: "synthetic".into(),
            refresh_token: "synthetic".into(),
            expires_at_secs: 123,
        };
        inner.denied.store(true, Ordering::SeqCst);
        assert!(vault.store(&account, token()).is_err());
        assert!(vault.load(&account).is_err());
        assert_eq!(inner.loads.load(Ordering::SeqCst), 0);
        inner.denied.store(false, Ordering::SeqCst);
        vault.store(&account, token()).unwrap();
        assert!(vault.load(&account).unwrap().is_none());
    }
    #[test]
    fn rejected_token_does_not_block_later_loads() {
        let inner = Arc::new(Vault::default());
        let vault = SessionCredentialVault::new(inner.clone());
        let account = AccountId::new("account").unwrap();
        let empty = StoredToken {
            access_token: "synthetic".into(),
            refresh_token: String::new(),
            expires_at_secs: 123,
        };
        assert!(matches!(
            vault.store(&account, empty),
            Err(GoogleError::InvalidResponse(_))
        ));
        assert!(vault.load(&account).unwrap().is_none());
        assert_eq!(inner.loads.load(Ordering::SeqCst), 1);
    }
}
