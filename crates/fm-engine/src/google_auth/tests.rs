use super::*;
use fm_google::transport::HttpResponse;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Mutex as StdMutex,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{mpsc, Notify, Semaphore},
    time::timeout,
};

const CLIENT: &str = "synthetic-client.apps.googleusercontent.com";

#[tokio::test]
async fn email_first_login_uses_app_registration_and_verified_identity() {
    let mut h = Harness::new();
    let path = h.config();
    Arc::get_mut(&mut h.runtime.inner).unwrap().app_client = AppClient::configured(path.into());
    let setup = h
        .runtime
        .handle(
            "owner",
            CancellationToken::new(),
            "google_auth_configuration",
            None,
            json!({}),
        )
        .await
        .unwrap();
    assert_eq!(setup, json!({"available":true}));
    assert_eq!(h.http.calls.load(Ordering::SeqCst), 0);
    assert_eq!(h.vault.stores.load(Ordering::SeqCst), 0);
    let reply = h
        .runtime
        .handle(
            "owner",
            CancellationToken::new(),
            "google_auth_begin",
            None,
            json!({"email":"  intended+mail@workspace.example  "}),
        )
        .await
        .unwrap();
    assert_eq!(
        url_field(reply["authorization_url"].as_str().unwrap(), "login_hint"),
        "intended+mail@workspace.example"
    );
    assert_eq!(
        url_field(reply["authorization_url"].as_str().unwrap(), "client_id"),
        CLIENT
    );
    // Google can select another email or rename a Workspace address. Only its
    // signed subject binds the account; the hint is neither an alias nor proof.
    h.signed_callback(
        &reply,
        "verified-subject",
        "chosen@example.invalid",
        &fm_google::auth::GOOGLE_SCOPES,
    )
    .await;
    let result = completed(&h.runtime, "owner", reply["attempt_id"].as_str().unwrap()).await;
    assert_eq!(result["phase"], "connected");
    assert_eq!(
        result["account"],
        fm_core::live_profile::google_account_id("verified-subject").unwrap()
    );
    assert!(!result.to_string().contains("intended+mail"));
    assert_eq!(h.vault.stores.load(Ordering::SeqCst), 1);
    h.close().await;
}

#[tokio::test]
async fn missing_registration_and_invalid_email_start_no_browser_or_provider_work() {
    let h = Harness::new();
    let setup = h
        .runtime
        .handle(
            "owner",
            CancellationToken::new(),
            "google_auth_configuration",
            None,
            json!({}),
        )
        .await
        .unwrap();
    assert_eq!(setup["available"], false);
    assert_eq!(setup["error"]["code"], "google_not_configured");
    let failure = h
        .runtime
        .handle(
            "owner",
            CancellationToken::new(),
            "google_auth_begin",
            None,
            json!({"email":"user@gmail.com"}),
        )
        .await
        .unwrap_err();
    assert_eq!(failure.code, "google_not_configured");
    assert!(h.runtime.inner.state.lock().await.active.is_none());
    for email in [
        json!(""),
        json!("not-an-email"),
        json!("user@corp.example\r\n&scope=other"),
        json!(42),
        json!("a".repeat(321)),
    ] {
        let failure = h
            .runtime
            .handle(
                "owner",
                CancellationToken::new(),
                "google_auth_begin",
                None,
                json!({"email":email}),
            )
            .await
            .unwrap_err();
        assert_eq!(failure.code, "invalid_request");
    }
    assert_eq!(h.http.calls.load(Ordering::SeqCst), 0);
    assert_eq!(h.vault.stores.load(Ordering::SeqCst), 0);
    h.close().await;
}

#[tokio::test]
async fn legacy_profile_can_reconnect_without_selecting_its_client_file_again() {
    let h = Harness::new();
    let account = h.expired_account().await;
    let reply = h
        .runtime
        .handle(
            "owner",
            CancellationToken::new(),
            "google_auth_begin",
            Some(&account),
            json!({"email":"old@example.invalid","reauthorize":true}),
        )
        .await
        .unwrap();
    assert_eq!(
        url_field(reply["authorization_url"].as_str().unwrap(), "prompt"),
        "consent select_account"
    );
    h.signed_callback(
        &reply,
        "expired-subject",
        "renamed@example.invalid",
        &fm_google::auth::GOOGLE_SCOPES,
    )
    .await;
    assert_eq!(
        completed(&h.runtime, "owner", reply["attempt_id"].as_str().unwrap()).await["account"],
        account
    );
    h.close().await;
}

#[tokio::test]
async fn relocated_registration_preserves_client_binding_and_explicit_overrides() {
    let h = Harness::new();
    let original = h.config();
    let moved = h.directory.path().join("relocated-google-client.json");
    std::fs::rename(&original, &moved).unwrap();
    let reference = GoogleClientReference {
        client_id: CLIENT.into(),
        config_path: original.clone(),
    };
    let app = AppClient::configured(moved.clone());
    let (config, path) = app.load_saved(reference.clone()).await.unwrap();
    assert_eq!(config.client_id(), CLIENT);
    assert_eq!(path, moved.to_str().unwrap());
    // A changed registration must never refresh an existing client's credential.
    std::fs::write(&moved, br#"{"installed":{"client_id":"different.apps.googleusercontent.com","client_secret":"synthetic"}}"#).unwrap();
    assert_eq!(
        app.load_saved(reference.clone()).await.err().unwrap().code,
        "client_mismatch"
    );
    // An explicitly configured legacy profile keeps its own valid registration.
    h.config();
    assert_eq!(
        app.load_saved(reference.clone())
            .await
            .unwrap()
            .0
            .client_id(),
        CLIENT
    );
    // Present corrupt data fails; it must not fall through to another client.
    std::fs::write(&original, b"{}").unwrap();
    assert_eq!(
        app.load_saved(reference).await.err().unwrap().code,
        "invalid_client_config"
    );
    h.close().await;
}

#[test]
fn packaged_registration_sits_in_app_resources_on_macos_and_beside_the_engine_elsewhere() {
    let bundled = client_config::packaged_client_path(Path::new(
        "/Applications/fuckMail.app/Contents/MacOS/fm-engine",
    ));
    assert_eq!(
        bundled.unwrap(),
        Path::new("/Applications/fuckMail.app/Contents/Resources/google-oauth-client.json")
    );
    let plain = client_config::packaged_client_path(Path::new("/opt/fuckMail/bin/fm-engine"));
    assert_eq!(
        plain.unwrap(),
        Path::new("/opt/fuckMail/bin/google-oauth-client.json")
    );
}
#[path = "account_lifecycle_tests.rs"]
mod lifecycle_tests;
#[derive(Default)]
struct FixtureHttp {
    calls: AtomicUsize,
    reject: AtomicBool,
    pause: AtomicBool,
    entered: Notify,
    release: Notify,
    success: StdMutex<Option<Vec<u8>>>,
}
impl HttpTransport for FixtureHttp {
    fn execute<'a>(&'a self, request: HttpRequest, _: &'a CancellationToken) -> HttpFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(token_response) = self.success.lock().unwrap().clone() {
                return Ok(HttpResponse {
                    status: 200,
                    body: match request.url() {
                        "https://oauth2.googleapis.com/token" => token_response,
                        "https://www.googleapis.com/oauth2/v3/certs" => {
                            include_bytes!("../../../fm-google/tests/fixtures/auth-v1/jwks.json")
                                .to_vec()
                        }
                        _ => panic!("unexpected OAuth fixture endpoint"),
                    },
                    retry_after_secs: None,
                    etag: None,
                });
            }
            assert_eq!(request.url(), "https://oauth2.googleapis.com/token");
            if self.pause.load(Ordering::SeqCst) {
                self.entered.notify_one();
                self.release.notified().await;
            }
            if self.reject.load(Ordering::SeqCst) {
                return Ok(HttpResponse {
                    status: 400,
                    body: br#"{"error":"invalid_grant"}"#.to_vec(),
                    retry_after_secs: None,
                    etag: None,
                });
            }
            Ok(HttpResponse {
                status: 429,
                body: b"{}".to_vec(),
                retry_after_secs: Some(1),
                etag: None,
            })
        })
    }
}
#[derive(Default)]
struct FixtureVault {
    tokens: StdMutex<BTreeMap<AccountId, StoredToken>>,
    stores: AtomicUsize,
    pause: StdMutex<Option<StorePause>>,
    removals: AtomicUsize,
    remove_failure: AtomicBool,
    remove_pause: StdMutex<Option<StorePause>>,
}
struct StorePause {
    entered: tokio::sync::oneshot::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
}
struct StoreRelease(Option<std::sync::mpsc::Sender<()>>);
impl Drop for StoreRelease {
    fn drop(&mut self) {
        if let Some(send) = self.0.take() {
            let _ = send.send(());
        }
    }
}
impl CredentialVault for FixtureVault {
    fn remove(&self, account: &AccountId) -> std::result::Result<(), GoogleError> {
        self.removals.fetch_add(1, Ordering::SeqCst);
        if let Some(pause) = self.remove_pause.lock().unwrap().take() {
            let _ = pause.entered.send(());
            pause.release.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        if self.remove_failure.load(Ordering::SeqCst) {
            return Err(GoogleError::StoreUnavailable);
        }
        self.tokens.lock().unwrap().remove(account);
        Ok(())
    }
    fn load(&self, account: &AccountId) -> std::result::Result<Option<StoredToken>, GoogleError> {
        Ok(self.tokens.lock().unwrap().get(account).cloned())
    }
    fn store(
        &self,
        account: &AccountId,
        token: StoredToken,
    ) -> std::result::Result<(), GoogleError> {
        self.stores.fetch_add(1, Ordering::SeqCst);
        self.tokens.lock().unwrap().insert(account.clone(), token);
        if let Some(pause) = self.pause.lock().unwrap().take() {
            let _ = pause.entered.send(());
            pause.release.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        Ok(())
    }
}
struct Harness {
    runtime: Runtime,
    http: Arc<FixtureHttp>,
    vault: Arc<FixtureVault>,
    directory: tempfile::TempDir,
    writer: std::thread::JoinHandle<()>,
    core_slots: Arc<Semaphore>,
}
impl Harness {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("profile");
        let (send, mut receive) = mpsc::channel(8);
        let (ready, wait) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            let mut store = fm_core::Store::open_live(&path).unwrap();
            ready.send(()).unwrap();
            while let Some(work) = receive.blocking_recv() {
                if let crate::Work::Provider(call) = work {
                    call.run(&mut store);
                } else {
                    panic!("unexpected test work");
                }
            }
        });
        wait.recv_timeout(Duration::from_secs(10)).unwrap();
        let core_slots = Arc::new(Semaphore::new(8));
        let core = CoreBridge::new(send, core_slots.clone());
        let http = Arc::new(FixtureHttp::default());
        let vault = Arc::new(FixtureVault::default());
        let runtime =
            Runtime::with_services(core, http.clone(), vault.clone(), CancellationToken::new());
        Self {
            runtime,
            http,
            vault,
            directory,
            writer,
            core_slots,
        }
    }
    fn config(&self) -> String {
        let path = self.directory.path().join("desktop.json");
        std::fs::write(&path, serde_json::to_vec(&json!({"installed":{"client_id":CLIENT,"client_secret":"synthetic-secret",
            "auth_uri":"https://not-google.invalid/login", "token_uri":"https://not-google.invalid/token",
            "redirect_uris":["http://remote.invalid/callback"]}})).unwrap()).unwrap();
        path.to_str().unwrap().to_owned()
    }
    async fn expired_account(&self) -> String {
        let path = self.config();
        let account = self
            .runtime
            .inner
            .core
            .call(move |store| {
                store.bind_google_account_with_client(
                    "expired-subject",
                    "expired@example.invalid",
                    &fm_google::auth::GOOGLE_SCOPES
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    None,
                    CLIENT,
                    &path,
                )
            })
            .await
            .unwrap();
        self.vault
            .store(
                &AccountId::new(account.clone()).unwrap(),
                StoredToken {
                    access_token: "expired-access".into(),
                    refresh_token: "expired-refresh".into(),
                    expires_at_secs: 0,
                },
            )
            .unwrap();
        account
    }
    fn pause_after_vault_write(&self) -> (tokio::sync::oneshot::Receiver<()>, StoreRelease) {
        let (entered, waiting) = tokio::sync::oneshot::channel();
        let (release, blocked) = std::sync::mpsc::channel();
        *self.vault.pause.lock().unwrap() = Some(StorePause {
            entered,
            release: blocked,
        });
        (waiting, StoreRelease(Some(release)))
    }
    async fn successful_callback(&self, account: Option<&str>, scopes: &[&str]) -> Value {
        let reply = self
            .runtime
            .handle(
                "owner",
                CancellationToken::new(),
                "google_auth_begin",
                account,
                json!({"client_config_path":self.config()}),
            )
            .await
            .unwrap();
        self.signed_callback(&reply, "expired-subject", "renamed@example.invalid", scopes)
            .await;
        reply
    }
    async fn signed_callback(&self, reply: &Value, subject: &str, email: &str, scopes: &[&str]) {
        let url = reply["authorization_url"].as_str().unwrap();
        let nonce = url_field(url, "nonce");
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
        header.kid = Some("synthetic-rsa-v1".into());
        let now = now_secs();
        let claims = json!({"iss":"https://accounts.google.com", "aud":CLIENT, "sub":subject,
            "iat":now-10, "exp":now+600, "nonce":nonce, "email":email, "email_verified":true});
        let id_token = jsonwebtoken::encode(
            &header,
            &claims,
            &jsonwebtoken::EncodingKey::from_rsa_der(include_bytes!(
                "../../../fm-google/tests/fixtures/auth-v1/synthetic-rsa-private.der"
            )),
        )
        .unwrap();
        *self.http.success.lock().unwrap() = Some(serde_json::to_vec(&json!({
            "access_token":"new-fixture-access", "refresh_token":"new-fixture-refresh", "expires_in":600,
            "token_type":"Bearer", "scope":scopes.join(" "), "id_token":id_token
        })).unwrap());
        callback(reply).await;
    }
    async fn close(self) {
        self.runtime.shutdown().await;
        drop(self.runtime);
        self.writer.join().unwrap();
    }
}

async fn callback(reply: &Value) {
    let url = reply["authorization_url"].as_str().unwrap();
    let redirect = url_field(url, "redirect_uri");
    let state = url_field(url, "state");
    let (host, path) = redirect
        .strip_prefix("http://")
        .unwrap()
        .split_once('/')
        .unwrap();
    let mut stream = tokio::net::TcpStream::connect(host).await.unwrap();
    stream
        .write_all(
            format!(
                "GET /{path}?state={state}&code=synthetic-code HTTP/1.1\r\nHost: {host}\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    assert!(response.starts_with(b"HTTP/1.1 200"));
}

#[tokio::test]
async fn successful_signed_login_and_partial_reconnect_publish_only_durable_verified_grants() {
    let h = Harness::new();
    let reply = h
        .successful_callback(None, &fm_google::auth::GOOGLE_SCOPES)
        .await;
    let status = completed(&h.runtime, "owner", reply["attempt_id"].as_str().unwrap()).await;
    assert_eq!(status["phase"], "connected");
    let account = status["account"].as_str().unwrap().to_owned();
    assert_eq!(
        account,
        fm_core::live_profile::google_account_id("expired-subject").unwrap()
    );
    assert_eq!(h.vault.stores.load(Ordering::SeqCst), 1);
    let old_work = h.runtime.account_work_cancel(&account).await.unwrap();
    let reply = h
        .successful_callback(Some(&account), &["openid", "email"])
        .await;
    assert_eq!(
        completed(&h.runtime, "owner", reply["attempt_id"].as_str().unwrap()).await["phase"],
        "connected"
    );
    assert!(old_work.is_cancelled());
    let bindings = h
        .runtime
        .inner
        .core
        .call(|store| store.google_account_bindings())
        .await
        .unwrap();
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].state, ConnectionState::Connected);
    assert_eq!(bindings[0].connection_generation, 3);
    assert_eq!(
        bindings[0].scopes,
        ["openid".to_owned(), "email".to_owned()].into()
    );
    assert_eq!(h.vault.stores.load(Ordering::SeqCst), 2);
    assert_eq!(h.http.calls.load(Ordering::SeqCst), 4);
    assert_eq!(
        h.runtime
            .access_token(&account, &CancellationToken::new())
            .await
            .unwrap(),
        "new-fixture-access"
    );
    assert!(!h
        .runtime
        .account_work_cancel(&account)
        .await
        .unwrap()
        .is_cancelled());
    h.close().await;
}

#[tokio::test]
async fn cancelled_reconnect_after_vault_write_cannot_restart_with_stale_connected_scopes() {
    let h = Harness::new();
    let account = h.expired_account().await;
    let old_work = h.runtime.account_work_cancel(&account).await.unwrap();
    let (stored, release) = h.pause_after_vault_write();
    let reply = h
        .successful_callback(Some(&account), &["openid", "email"])
        .await;
    timeout(Duration::from_secs(3), stored)
        .await
        .unwrap()
        .unwrap();
    assert!(old_work.is_cancelled());
    let binding = h
        .runtime
        .inner
        .core
        .call(|store| Ok(store.google_account_bindings()?.remove(0)))
        .await
        .unwrap();
    assert_eq!(binding.state, ConnectionState::ReauthorizationRequired);
    assert_eq!(binding.connection_generation, 2);
    h.runtime
        .handle(
            "owner",
            CancellationToken::new(),
            "google_auth_cancel",
            None,
            json!({"attempt_id":reply["attempt_id"]}),
        )
        .await
        .unwrap();
    drop(release);
    assert_eq!(
        completed(&h.runtime, "owner", reply["attempt_id"].as_str().unwrap()).await["phase"],
        "cancelled"
    );
    assert_eq!(
        h.vault
            .load(&AccountId::new(account.clone()).unwrap())
            .unwrap()
            .unwrap()
            .refresh_token,
        "new-fixture-refresh"
    );
    // A new runtime has no cancelled in-memory job tokens to hide a persisted-state mistake.
    let cold = Runtime::with_services(
        h.runtime.inner.core.clone(),
        h.http.clone(),
        h.vault.clone(),
        CancellationToken::new(),
    );
    assert_eq!(
        cold.access_token(&account, &CancellationToken::new())
            .await
            .unwrap_err()
            .code,
        "reauthorization_required"
    );
    cold.shutdown().await;
    drop(cold);
    h.close().await;
}

#[tokio::test]
async fn failed_core_binding_after_vault_write_leaves_reconnect_required() {
    let h = Harness::new();
    let account = h.expired_account().await;
    let db = rusqlite::Connection::open_with_flags(
        h.directory.path().join("profile/store.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    db.execute_batch("CREATE TRIGGER reject_reconnect_commit BEFORE UPDATE ON google_accounts WHEN NEW.connection_state='connected' BEGIN SELECT RAISE(ABORT,'synthetic reconnect commit failure'); END;").unwrap();
    let reply = h
        .successful_callback(Some(&account), &["openid", "email"])
        .await;
    assert_eq!(
        completed(&h.runtime, "owner", reply["attempt_id"].as_str().unwrap()).await["phase"],
        "failed"
    );
    let binding = h
        .runtime
        .inner
        .core
        .call(|store| Ok(store.google_account_bindings()?.remove(0)))
        .await
        .unwrap();
    assert_eq!(binding.state, ConnectionState::ReauthorizationRequired);
    assert_eq!(binding.connection_generation, 2);
    assert_eq!(
        h.vault
            .load(&AccountId::new(account.clone()).unwrap())
            .unwrap()
            .unwrap()
            .refresh_token,
        "new-fixture-refresh"
    );
    let cold = Runtime::with_services(
        h.runtime.inner.core.clone(),
        h.http.clone(),
        h.vault.clone(),
        CancellationToken::new(),
    );
    assert_eq!(
        cold.access_token(&account, &CancellationToken::new())
            .await
            .unwrap_err()
            .code,
        "reauthorization_required"
    );
    cold.shutdown().await;
    drop(cold);
    db.execute_batch("DROP TRIGGER reject_reconnect_commit;")
        .unwrap();
    let retry = h
        .successful_callback(Some(&account), &["openid", "email"])
        .await;
    assert_eq!(
        completed(&h.runtime, "owner", retry["attempt_id"].as_str().unwrap()).await["phase"],
        "connected"
    );
    drop(db);
    h.close().await;
}

#[tokio::test]
async fn rejected_refresh_stops_sibling_account_work_and_keeps_the_vault() {
    let h = Harness::new();
    let account = h.expired_account().await;
    let sibling = h.runtime.account_work_cancel(&account).await.unwrap();
    h.http.reject.store(true, Ordering::SeqCst);
    assert_eq!(
        h.runtime
            .access_token(&account, &CancellationToken::new())
            .await
            .unwrap_err()
            .code,
        "reauthorization_required"
    );
    assert!(sibling.is_cancelled());
    let bindings = h
        .runtime
        .inner
        .core
        .call(|store| store.google_account_bindings())
        .await
        .unwrap();
    assert_eq!(bindings[0].state, ConnectionState::ReauthorizationRequired);
    assert_eq!(bindings[0].connection_generation, 2);
    assert_eq!(h.vault.stores.load(Ordering::SeqCst), 1);
    assert!(h
        .vault
        .load(&AccountId::new(account).unwrap())
        .unwrap()
        .is_some());
    h.close().await;
}

#[tokio::test]
async fn queued_refresh_rejection_cannot_downgrade_a_disconnect() {
    let h = Harness::new();
    let account = h.expired_account().await;
    let work = h.runtime.account_work_cancel(&account).await.unwrap();
    h.http.reject.store(true, Ordering::SeqCst);
    h.http.pause.store(true, Ordering::SeqCst);
    let runtime = h.runtime.clone();
    let selected = account.clone();
    let refresh = tokio::spawn(async move {
        runtime
            .access_token(&selected, &CancellationToken::new())
            .await
    });
    timeout(Duration::from_secs(3), h.http.entered.notified())
        .await
        .unwrap();
    let core = h.runtime.inner.core.clone();
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let blocker = tokio::spawn(async move {
        core.call(move |_| {
            let _ = entered.send(());
            blocked.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(())
        })
        .await
    });
    timeout(Duration::from_secs(3), waiting)
        .await
        .unwrap()
        .unwrap();
    h.http.release.notify_one();
    // One permit belongs to the active blocker, a second to the queued rejection.
    timeout(Duration::from_secs(3), async {
        while h.core_slots.available_permits() != 6 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let runtime = h.runtime.clone();
    let selected = account.clone();
    let disconnect = tokio::spawn(async move {
        runtime
            .handle(
                "owner",
                CancellationToken::new(),
                "google_disconnect",
                Some(&selected),
                json!({}),
            )
            .await
    });
    timeout(Duration::from_secs(3), work.cancelled())
        .await
        .unwrap();
    release.send(()).unwrap();
    blocker.await.unwrap().unwrap();
    assert_eq!(refresh.await.unwrap().unwrap_err().code, "cancelled");
    disconnect.await.unwrap().unwrap();
    let bindings = h
        .runtime
        .inner
        .core
        .call(|store| store.google_account_bindings())
        .await
        .unwrap();
    assert_eq!(bindings[0].state, ConnectionState::Disconnected);
    // Exactly one transition: the stale rejection must not add an intermediate generation.
    assert_eq!(bindings[0].connection_generation, 2);
    assert_eq!(h.vault.stores.load(Ordering::SeqCst), 1);
    h.close().await;
}
async fn begin(h: &Harness, owner: &str, cancel: CancellationToken) -> Value {
    h.runtime
        .handle(
            owner,
            cancel,
            "google_auth_begin",
            None,
            json!({"client_config_path":h.config()}),
        )
        .await
        .unwrap()
}
async fn completed(runtime: &Runtime, owner: &str, id: &str) -> Value {
    timeout(Duration::from_secs(3), async {
        loop {
            let status = runtime
                .handle(
                    owner,
                    CancellationToken::new(),
                    "google_auth_status",
                    None,
                    json!({"attempt_id":id}),
                )
                .await
                .unwrap();
            if terminal(status["phase"].as_str().unwrap())
                && runtime.inner.state.lock().await.active.is_none()
            {
                break status;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}
fn url_field(url: &str, key: &str) -> String {
    let value = url
        .split_once('?')
        .unwrap()
        .1
        .split('&')
        .find_map(|field| {
            let (name, value) = field.split_once('=')?;
            (name == key).then_some(value)
        })
        .unwrap();
    let mut bytes = Vec::new();
    let mut chars = value.as_bytes().iter();
    while let Some(byte) = chars.next() {
        bytes.push(if *byte == b'%' {
            let hex = [*chars.next().unwrap(), *chars.next().unwrap()];
            u8::from_str_radix(std::str::from_utf8(&hex).unwrap(), 16).unwrap()
        } else if *byte == b'+' {
            b' '
        } else {
            *byte
        });
    }
    String::from_utf8(bytes).unwrap()
}

#[tokio::test]
async fn configuration_is_bounded_installed_only_and_never_supplies_endpoints() {
    let h = Harness::new();
    let path = h.config();
    assert_eq!(read_config(path.clone()).await.unwrap().client_id(), CLIENT);
    for value in [
        json!({"web":{"client_id":CLIENT,"client_secret":"secret"}}),
        json!({"installed":{"client_id":"bad-client","client_secret":"secret"}}),
        json!({"installed":{"client_id":CLIENT}}),
        json!({"installed":null}),
    ] {
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(read_config(path.clone()).await.is_err());
    }
    std::fs::write(&path, vec![b'x'; CONFIG_BYTES as usize + 1]).unwrap();
    assert!(read_config(path).await.is_err());
    assert!(read_config("relative.json".into()).await.is_err());
    let result = begin(&h, "owner", CancellationToken::new()).await;
    let url = result["authorization_url"].as_str().unwrap();
    assert!(url.starts_with("https://accounts.google.com/"));
    assert!(url_field(url, "redirect_uri").starts_with("http://127.0.0.1:"));
    assert!(!url.contains("not-google") && !url.contains("synthetic-secret"));
    assert_eq!(h.http.calls.load(Ordering::SeqCst), 0);
    assert_eq!(h.vault.stores.load(Ordering::SeqCst), 0);
    h.close().await;
}

#[tokio::test]
async fn attempts_are_session_owned_cancelled_on_owner_exit_and_status_is_redacted() {
    let h = Harness::new();
    let owner = CancellationToken::new();
    let reply = begin(&h, "first", owner.clone()).await;
    let id = reply["attempt_id"].as_str().unwrap();
    assert_eq!(
        h.runtime
            .handle(
                "second",
                CancellationToken::new(),
                "google_auth_begin",
                None,
                json!({"client_config_path":h.config()})
            )
            .await
            .unwrap_err()
            .code,
        "login_in_progress"
    );
    for method in ["google_auth_status", "google_auth_cancel"] {
        assert_eq!(
            h.runtime
                .handle(
                    "second",
                    CancellationToken::new(),
                    method,
                    None,
                    json!({"attempt_id":id})
                )
                .await
                .unwrap_err()
                .code,
            "not_found"
        );
    }
    let status = h
        .runtime
        .handle(
            "first",
            CancellationToken::new(),
            "google_auth_status",
            None,
            json!({"attempt_id":id}),
        )
        .await
        .unwrap();
    assert_eq!(status["phase"], "waiting_for_browser");
    assert_eq!(status.as_object().unwrap().len(), 4);
    let wire = status.to_string();
    for secret in [
        "authorization_url",
        "access_token",
        "refresh_token",
        "synthetic-secret",
        "code_verifier",
    ] {
        assert!(!wire.contains(secret));
    }
    owner.cancel();
    assert_eq!(
        completed(&h.runtime, "first", id).await["phase"],
        "cancelled"
    );
    assert_eq!(h.vault.stores.load(Ordering::SeqCst), 0);
    assert!(h
        .runtime
        .inner
        .core
        .call(|store| store.google_account_bindings())
        .await
        .unwrap()
        .is_empty());
    h.close().await;
}

#[tokio::test]
async fn aborted_begin_releases_its_reserved_attempt_without_starting_login_later() {
    let h = Harness::new();
    let core = h.runtime.inner.core.clone();
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let blocker = tokio::spawn(async move {
        core.call(move |_| {
            let _ = entered.send(());
            blocked.recv_timeout(Duration::from_secs(3)).unwrap();
            Ok(())
        })
        .await
    });
    timeout(Duration::from_secs(3), waiting)
        .await
        .unwrap()
        .unwrap();
    let runtime = h.runtime.clone();
    let path = h.config();
    let task = tokio::spawn(async move {
        runtime
            .handle(
                "aborted-owner",
                CancellationToken::new(),
                "google_auth_begin",
                None,
                json!({"client_config_path":path}),
            )
            .await
    });
    timeout(Duration::from_secs(2), async {
        while h.runtime.inner.state.lock().await.active.is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    release.send(()).unwrap();
    blocker.await.unwrap().unwrap();
    timeout(Duration::from_secs(2), async {
        while h.runtime.inner.state.lock().await.active.is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(h.http.calls.load(Ordering::SeqCst), 0);
    assert_eq!(h.vault.stores.load(Ordering::SeqCst), 0);
    begin(&h, "next-owner", CancellationToken::new()).await;
    h.close().await;
}

#[tokio::test]
async fn real_loopback_callback_uses_fixed_fixture_transport_and_retains_no_failed_grant() {
    let h = Harness::new();
    let reply = begin(&h, "owner", CancellationToken::new()).await;
    let url = reply["authorization_url"].as_str().unwrap();
    let redirect = url_field(url, "redirect_uri");
    let state = url_field(url, "state");
    let (host, path) = redirect
        .strip_prefix("http://")
        .unwrap()
        .split_once('/')
        .unwrap();
    let mut stream = tokio::net::TcpStream::connect(host).await.unwrap();
    stream
        .write_all(
            format!(
                "GET /{path}?state={state}&code=synthetic-code HTTP/1.1\r\nHost: {host}\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    assert!(response.starts_with(b"HTTP/1.1 200"));
    let status = completed(&h.runtime, "owner", reply["attempt_id"].as_str().unwrap()).await;
    assert_eq!(status["phase"], "failed");
    assert_eq!(status["error"]["code"], "rate_limited");
    assert_eq!(h.http.calls.load(Ordering::SeqCst), 1);
    assert_eq!(h.vault.stores.load(Ordering::SeqCst), 0);
    assert!(h
        .runtime
        .inner
        .core
        .call(|store| store.google_account_bindings())
        .await
        .unwrap()
        .is_empty());
    h.close().await;
}

#[tokio::test]
async fn completed_status_retention_is_bounded_and_shutdown_closes_the_listener() {
    let h = Harness::new();
    let mut oldest = String::new();
    for index in 0..18 {
        let reply = begin(&h, "owner", CancellationToken::new()).await;
        let id = reply["attempt_id"].as_str().unwrap();
        if index == 0 {
            oldest = id.to_owned();
        }
        h.runtime
            .handle(
                "owner",
                CancellationToken::new(),
                "google_auth_cancel",
                None,
                json!({"attempt_id":id}),
            )
            .await
            .unwrap();
        assert_eq!(
            completed(&h.runtime, "owner", id).await["phase"],
            "cancelled"
        );
    }
    assert_eq!(
        h.runtime.inner.state.lock().await.attempts.len(),
        RETAINED_ATTEMPTS
    );
    assert_eq!(
        h.runtime
            .handle(
                "owner",
                CancellationToken::new(),
                "google_auth_status",
                None,
                json!({"attempt_id":oldest})
            )
            .await
            .unwrap_err()
            .code,
        "not_found"
    );
    let reply = begin(&h, "owner", CancellationToken::new()).await;
    let redirect = url_field(reply["authorization_url"].as_str().unwrap(), "redirect_uri");
    h.runtime.shutdown().await;
    let host = redirect
        .strip_prefix("http://")
        .unwrap()
        .split('/')
        .next()
        .unwrap();
    assert!(tokio::net::TcpStream::connect(host).await.is_err());
    h.close().await;
}

#[tokio::test]
async fn saved_client_load_and_disconnect_use_one_shared_vault_without_deletion() {
    let h = Harness::new();
    let path = h.config();
    let saved_path = path.clone();
    let account = fm_core::live_profile::google_account_id("synthetic-subject").unwrap();
    let id = AccountId::new(account.clone()).unwrap();
    h.vault
        .store(
            &id,
            StoredToken {
                access_token: "fixture-cached-access".into(),
                refresh_token: "fixture-refresh".into(),
                expires_at_secs: now_secs() + 600,
            },
        )
        .unwrap();
    let bound = h
        .runtime
        .inner
        .core
        .call(move |store| {
            store.bind_google_account_with_client(
                "synthetic-subject",
                "fixture@example.invalid",
                &fm_google::auth::GOOGLE_SCOPES
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                None,
                CLIENT,
                &saved_path,
            )
        })
        .await
        .unwrap();
    assert_eq!(bound, account);
    assert_eq!(
        h.runtime
            .access_token(&account, &CancellationToken::new())
            .await
            .unwrap(),
        "fixture-cached-access"
    );
    let work = h.runtime.account_work_cancel(&account).await.unwrap();
    let result = h
        .runtime
        .handle(
            "owner",
            CancellationToken::new(),
            "google_disconnect",
            Some(&account),
            json!({}),
        )
        .await
        .unwrap();
    assert_eq!(result["connection_state"], "disconnected");
    assert!(work.is_cancelled());
    assert!(h
        .runtime
        .access_token(&account, &CancellationToken::new())
        .await
        .is_err());
    assert_eq!(
        h.vault.load(&id).unwrap().unwrap().refresh_token,
        "fixture-refresh"
    );
    assert_eq!(h.vault.stores.load(Ordering::SeqCst), 1);
    assert_eq!(h.http.calls.load(Ordering::SeqCst), 0);
    // A cold runtime must validate the persisted client identity before using a grant.
    std::fs::write(path,serde_json::to_vec(&json!({"installed":{"client_id":"different.apps.googleusercontent.com","client_secret":"synthetic"}})).unwrap()).unwrap();
    let reference = GoogleClientReference {
        client_id: CLIENT.into(),
        config_path: h
            .directory
            .path()
            .join("desktop.json")
            .to_str()
            .unwrap()
            .into(),
    };
    h.runtime.inner.state.lock().await.client = None;
    assert_eq!(
        h.runtime.saved_client(reference).await.err().unwrap().code,
        "client_mismatch"
    );
    h.close().await;
}
