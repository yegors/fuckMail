//! Native credentials only. No filesystem store, enumerator, or fallback backend.
use crate::auth::{CredentialVault, StoredToken};
use crate::{AccountId, GoogleError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Mutex;
use zeroize::Zeroizing;

/// Core supplies a random, persistent per-profile namespace. It is not a token.
/// Native errors are deliberately collapsed before reaching diagnostics.
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
