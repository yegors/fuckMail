//! The OpenPGP keyring runtime, and where the key that seals secret keys lives.
//!
//! Keyring requests begin and finish on the store writer; their OpenPGP work
//! (generating, parsing, sealing, exporting) runs on a blocking worker. The
//! key-encryption key is read from the OS keychain the first time a secret
//! key is used and then kept in memory until the engine exits. Demo profiles
//! keep it in a private file in the profile instead, so tests and demos never
//! touch the keychain; they hold only synthetic data.
use crate::core_tasks::CoreBridge;
use fm_core::{
    pgp::{KekSource, PgpStep},
    CoreError, Result,
};
use fm_google::transport::CancellationToken;
use fm_pgp::KEK_LEN;
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::Semaphore;
use zeroize::Zeroizing;

fn failure(code: &str, message: &str) -> CoreError {
    CoreError {
        code: code.into(),
        message: message.into(),
        details: Value::Null,
    }
}

fn encode(kek: &[u8; KEK_LEN]) -> Zeroizing<String> {
    use base64::Engine as _;
    Zeroizing::new(base64::engine::general_purpose::STANDARD.encode(kek))
}

fn decode(text: &str) -> Result<Zeroizing<[u8; KEK_LEN]>> {
    use base64::Engine as _;
    let bytes = Zeroizing::new(
        base64::engine::general_purpose::STANDARD
            .decode(text.trim())
            .map_err(|_| {
                failure(
                    "key_locked",
                    "The saved key for your secret keys is damaged",
                )
            })?,
    );
    let mut kek = Zeroizing::new([0u8; KEK_LEN]);
    if bytes.len() != KEK_LEN {
        return Err(failure(
            "key_locked",
            "The saved key for your secret keys is damaged",
        ));
    }
    kek.copy_from_slice(&bytes);
    Ok(kek)
}

/// The key in the OS keychain, kept in memory once read.
pub(crate) struct KeychainKek {
    secret: Option<fm_google::vault::NativeSecret>,
    cached: Mutex<Option<Zeroizing<[u8; KEK_LEN]>>>,
}

impl KeychainKek {
    pub(crate) fn new(namespace: Option<String>) -> Self {
        Self {
            secret: namespace.and_then(|n| fm_google::vault::NativeSecret::pgp_keyring(&n).ok()),
            cached: Mutex::new(None),
        }
    }
}

impl KekSource for KeychainKek {
    fn kek(&self, create: bool) -> Result<Option<Zeroizing<[u8; KEK_LEN]>>> {
        let unavailable = || {
            failure(
                "store_unavailable",
                "The system keychain is unavailable or refused access to your secret keys",
            )
        };
        let mut cached = self.cached.lock().map_err(|_| unavailable())?;
        if let Some(kek) = cached.as_ref() {
            return Ok(Some(kek.clone()));
        }
        let secret = self.secret.as_ref().ok_or_else(unavailable)?;
        let kek = match secret.load().map_err(|_| unavailable())? {
            Some(text) => decode(&text)?,
            None if create => {
                let kek = fm_pgp::new_kek();
                secret.store(&encode(&kek)).map_err(|_| unavailable())?;
                // Read it back, so a keychain that drops writes fails now
                // rather than after keys are sealed with it.
                match secret.load().map_err(|_| unavailable())? {
                    Some(text) if decode(&text)?[..] == kek[..] => kek,
                    _ => return Err(unavailable()),
                }
            }
            None => return Ok(None),
        };
        *cached = Some(kek.clone());
        Ok(Some(kek))
    }
}

/// A demo profile's key, in the private file `pgp-demo-key` in the profile.
pub(crate) struct FileKek {
    path: PathBuf,
    cached: Mutex<Option<Zeroizing<[u8; KEK_LEN]>>>,
}

impl FileKek {
    pub(crate) fn new(profile: &std::path::Path) -> Self {
        Self {
            path: profile.join("pgp-demo-key"),
            cached: Mutex::new(None),
        }
    }
}

impl KekSource for FileKek {
    fn kek(&self, create: bool) -> Result<Option<Zeroizing<[u8; KEK_LEN]>>> {
        let unavailable = || failure("store_unavailable", "The demo key file is unavailable");
        let mut cached = self.cached.lock().map_err(|_| unavailable())?;
        if let Some(kek) = cached.as_ref() {
            return Ok(Some(kek.clone()));
        }
        let kek = match std::fs::read_to_string(&self.path) {
            Ok(text) => decode(&text)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => {
                let kek = fm_pgp::new_kek();
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                use std::io::Write;
                let mut file = options.open(&self.path).map_err(|_| unavailable())?;
                file.write_all(encode(&kek).as_bytes())
                    .and_then(|_| file.sync_all())
                    .map_err(|_| unavailable())?;
                kek
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(unavailable()),
        };
        *cached = Some(kek.clone());
        Ok(Some(kek))
    }
}

/// Runs keyring requests: a store begin, OpenPGP work on a blocking worker
/// (two at a time), and a store finish.
pub(crate) struct Runtime {
    core: CoreBridge,
    kek: Arc<dyn KekSource>,
    workers: Arc<Semaphore>,
}

impl Runtime {
    pub(crate) fn new(core: CoreBridge, kek: Arc<dyn KekSource>) -> Self {
        Self {
            core,
            kek,
            workers: Arc::new(Semaphore::new(2)),
        }
    }

    pub(crate) fn handles(method: &str) -> bool {
        fm_core::pgp::METHODS.contains(&method)
    }

    /// Handles one keyring request. `cancel` stops the work before its finish
    /// starts; a finish that has started always records its receipt.
    pub(crate) async fn handle(
        &self,
        cancel: CancellationToken,
        method: &str,
        account: Option<&str>,
        params: Value,
    ) -> Result<Value> {
        let (method, account) = (method.to_owned(), account.map(str::to_owned));
        let step = self
            .core
            .call(move |store| store.pgp_begin(&method, account.as_deref(), &params))
            .await?;
        let plan = match step {
            PgpStep::Reply(reply) => return Ok(reply),
            PgpStep::Prepare(plan) => plan,
        };
        let permit = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(failure("cancelled", "Keyring work cancelled")),
            permit = self.workers.clone().acquire_owned() => permit.map_err(|_| failure("cancelled", "Keyring work cancelled"))?,
        };
        let kek = self.kek.clone();
        let stop = cancel.clone();
        let proof = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            if stop.is_cancelled() {
                return Err(failure("cancelled", "Keyring work cancelled"));
            }
            let input = plan
                .import_path()
                .map(fm_core::pgp::read_key_file)
                .transpose()?;
            fm_core::pgp::pgp_prepare(plan, input, kek.as_ref())
        })
        .await
        .map_err(|_| failure("internal", "Keyring work failed"))??;
        if cancel.is_cancelled() {
            return Err(failure("cancelled", "Keyring work cancelled"));
        }
        self.core.call(move |store| store.pgp_finish(proof)).await
    }
}
