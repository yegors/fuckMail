//! Explicit session-owned Google login and shared, vault-backed token refresh.
//! No browser launch, import, send, token serialization or credential-file store.
use crate::core_tasks::CoreBridge;
use fm_core::{
    live_profile::{ConnectionState, GoogleClientReference, ProfileKind},
    CoreError, Result,
};
use fm_google::{
    auth::{CredentialVault, StoredToken, TokenCoordinator},
    desktop_auth::{DesktopLogin, GoogleOAuth, OAuthClientConfig},
    transport::{CancellationToken, HttpFuture, HttpRequest, HttpTransport},
    AccountId, GoogleError,
};
#[cfg(not(feature = "verification-google"))]
use fm_google::{
    transport::{ReqwestTransport, TransportLimits},
    vault::NativeCredentialVault,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    io::Read,
    path::Path,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{sync::Mutex, task::JoinHandle};

const LOGIN_SECONDS: u64 = 300;
const RETAINED_ATTEMPTS: usize = 16;
const CONFIG_BYTES: u64 = 64 * 1024;
mod account_lifecycle;

struct SharedHttp(Arc<dyn HttpTransport>);
impl HttpTransport for SharedHttp {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
        cancel: &'a CancellationToken,
    ) -> HttpFuture<'a> {
        self.0.execute(request, cancel)
    }
}
struct SharedVault(Arc<dyn CredentialVault>);
impl CredentialVault for SharedVault {
    fn remove(&self, account: &AccountId) -> std::result::Result<(), GoogleError> {
        self.0.remove(account)
    }

    fn load(&self, account: &AccountId) -> std::result::Result<Option<StoredToken>, GoogleError> {
        self.0.load(account)
    }
    fn store(
        &self,
        account: &AccountId,
        token: StoredToken,
    ) -> std::result::Result<(), GoogleError> {
        self.0.store(account, token)
    }
}
type OAuth = GoogleOAuth<SharedHttp>;
struct LoadedClient {
    oauth: Arc<OAuth>,
    path: String,
}
struct Attempt {
    id: String,
    owner: String,
    account: Option<String>,
    phase: &'static str,
    error: Option<Value>,
    cancel: CancellationToken,
    task: Option<JoinHandle<()>>,
}
impl Attempt {
    fn status(&self) -> Value {
        json!({"attempt_id":self.id,"phase":self.phase,"account":self.account,"error":self.error})
    }
}
struct AccountWork {
    #[allow(dead_code)] // Read by the shared token API when provider workers are connected.
    generation: i64,
    cancel: CancellationToken,
}
#[derive(Default)]
struct State {
    active: Option<String>,
    attempts: VecDeque<Attempt>,
    client: Option<Arc<LoadedClient>>,
    accounts: BTreeMap<String, AccountWork>,
    stopping: bool,
}
struct Inner {
    #[cfg(feature = "verification-google")]
    verification: bool,
    #[cfg(feature = "verification-google")]
    verification_login: Option<Arc<dyn VerificationLogin>>,
    core: CoreBridge,
    http: Arc<SharedHttp>,
    vault: Arc<SharedVault>,
    coordinator: TokenCoordinator,
    cancel: CancellationToken,
    state: Mutex<State>,
}
#[cfg(feature = "verification-google")]
pub(crate) trait VerificationLogin: Send + Sync {
    fn callback<'a>(
        &'a self,
        authorization_url: String,
        cancel: CancellationToken,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>>;
}
#[derive(Clone)]
pub(crate) struct Runtime {
    inner: Arc<Inner>,
}
struct BeginGuard {
    runtime: Runtime,
    id: String,
    cancel: CancellationToken,
    armed: bool,
}
impl Drop for BeginGuard {
    fn drop(&mut self) {
        if self.armed {
            self.cancel.cancel();
            let runtime = self.runtime.clone();
            let id = self.id.clone();
            tokio::spawn(async move {
                runtime.finish_attempt(&id, Err(cancelled())).await;
            });
        }
    }
}
impl Runtime {
    #[cfg(not(feature = "verification-google"))]
    pub(crate) fn new(
        core: CoreBridge,
        namespace: String,
        cancel: CancellationToken,
    ) -> Result<Arc<Self>> {
        let http = ReqwestTransport::new(TransportLimits::default()).map_err(|_| {
            error(
                "transport_unavailable",
                "Google HTTPS could not be initialized",
            )
        })?;
        let vault = NativeCredentialVault::new(&namespace).map_err(google_error)?;
        Ok(Arc::new(Self::with_services(
            core,
            Arc::new(http),
            Arc::new(vault),
            cancel,
        )))
    }
    #[cfg(test)]
    pub(crate) fn for_test(
        core: CoreBridge,
        http: Arc<dyn HttpTransport>,
        vault: Arc<dyn CredentialVault>,
        cancel: CancellationToken,
    ) -> Arc<Self> {
        Arc::new(Self::with_services(core, http, vault, cancel))
    }
    #[cfg(feature = "verification-google")]
    pub(crate) fn for_verification(
        core: CoreBridge,
        http: Arc<dyn HttpTransport>,
        vault: Arc<dyn CredentialVault>,
        cancel: CancellationToken,
    ) -> Arc<Self> {
        let mut runtime = Self::with_services(core, http, vault, cancel);
        Arc::get_mut(&mut runtime.inner).unwrap().verification = true;
        Arc::new(runtime)
    }
    #[cfg(feature = "verification-google")]
    pub(crate) fn for_verification_accounts(
        core: CoreBridge,
        http: Arc<dyn HttpTransport>,
        vault: Arc<dyn CredentialVault>,
        cancel: CancellationToken,
        observer: Arc<dyn VerificationLogin>,
    ) -> Arc<Self> {
        let mut runtime = Self::with_services(core, http, vault, cancel);
        let inner = Arc::get_mut(&mut runtime.inner).unwrap();
        inner.verification = true;
        inner.verification_login = Some(observer);
        Arc::new(runtime)
    }
    fn with_services(
        core: CoreBridge,
        http: Arc<dyn HttpTransport>,
        vault: Arc<dyn CredentialVault>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                #[cfg(feature = "verification-google")]
                verification: false,
                #[cfg(feature = "verification-google")]
                verification_login: None,
                core,
                http: Arc::new(SharedHttp(http)),
                vault: Arc::new(SharedVault(vault)),
                coordinator: TokenCoordinator::default(),
                cancel,
                state: Mutex::new(State::default()),
            }),
        }
    }
    pub(crate) fn handles(method: &str) -> bool {
        matches!(
            method,
            "google_auth_begin"
                | "google_auth_restore_begin"
                | "google_auth_status"
                | "google_auth_cancel"
                | "google_disconnect"
                | "account_remove"
        ) || (cfg!(feature = "verification-google") && method == "verification_google_auth_fixture")
    }
    /// Mail and Calendar reuse the engine's bounded pooled HTTPS transport.
    #[allow(dead_code)] // Used when the provider worker module is connected.
    pub(crate) fn http_transport(&self) -> Arc<dyn HttpTransport> {
        self.inner.http.clone()
    }
    pub(crate) async fn handle(
        &self,
        owner: &str,
        owner_cancel: CancellationToken,
        method: &str,
        account: Option<&str>,
        params: Value,
    ) -> Result<Value> {
        #[cfg(feature = "verification-google")]
        if self.inner.verification
            && self.inner.verification_login.is_none()
            && matches!(method, "google_auth_begin" | "google_auth_restore_begin")
        {
            return Err(error(
                "verification_refused",
                "OAuth is unavailable in the isolated provider fixture",
            ));
        }
        if owner.is_empty()
            || owner.len() > 256
            || owner_cancel.is_cancelled()
            || self.inner.cancel.is_cancelled()
        {
            return Err(cancelled());
        }
        match method {
            #[cfg(feature = "verification-google")]
            "verification_google_auth_fixture" => {
                fields(&params, &[])?;
                if account.is_some() || self.inner.verification_login.is_none() {
                    return Err(error(
                        "verification_refused",
                        "Account verification is unavailable in this profile",
                    ));
                }
                Ok(json!({"mode":"accounts-v1","browser_stub_allowed":true}))
            }
            "google_auth_begin" | "google_auth_restore_begin" => {
                let restore = method == "google_auth_restore_begin";
                fields(
                    &params,
                    if restore {
                        &["client_config_path", "reauthorize", "expected_revision"]
                    } else {
                        &["client_config_path", "reauthorize"]
                    },
                )?;
                let restore_revision = if restore {
                    if account.is_none() {
                        return Err(error(
                            "account_required",
                            "Choose an archived account to restore",
                        ));
                    }
                    Some(
                        params["expected_revision"]
                            .as_i64()
                            .filter(|v| *v > 0)
                            .ok_or_else(|| {
                                error(
                                    "invalid_request",
                                    "A current lifecycle revision is required",
                                )
                            })?,
                    )
                } else {
                    None
                };
                let path = params["client_config_path"]
                    .as_str()
                    .ok_or_else(invalid_config)?
                    .to_owned();
                let reauthorize = match params.get("reauthorize") {
                    None => false,
                    Some(Value::Bool(value)) => *value,
                    _ => return Err(error("invalid_request", "reauthorize must be a boolean")),
                };
                self.begin(
                    owner,
                    owner_cancel,
                    account.map(str::to_owned),
                    path,
                    reauthorize,
                    restore_revision,
                )
                .await
            }
            "account_remove" => {
                let account = account
                    .ok_or_else(|| error("account_required", "Choose an account to remove"))?;
                self.remove_account(account, params, &owner_cancel).await
            }
            "google_auth_status" | "google_auth_cancel" => {
                fields(&params, &["attempt_id"])?;
                if account.is_some() {
                    return Err(error(
                        "invalid_request",
                        "Login status is scoped to its session attempt",
                    ));
                }
                let id = params["attempt_id"]
                    .as_str()
                    .filter(|v| !v.is_empty() && v.len() <= 128)
                    .ok_or_else(|| error("invalid_request", "An attempt_id is required"))?;
                let mut state = self.inner.state.lock().await;
                let attempt = state
                    .attempts
                    .iter_mut()
                    .find(|a| a.id == id && a.owner == owner)
                    .ok_or_else(|| {
                        error(
                            "not_found",
                            "Login attempt is not available to this session",
                        )
                    })?;
                if method == "google_auth_cancel" && !terminal(attempt.phase) {
                    attempt.cancel.cancel();
                    attempt.phase = "cancelled";
                }
                Ok(attempt.status())
            }
            "google_disconnect" => {
                fields(&params, &[])?;
                let account = account
                    .ok_or_else(|| {
                        error("invalid_request", "Choose a Google account to disconnect")
                    })?
                    .to_owned();
                let state = self.inner.state.lock().await;
                for attempt in &state.attempts {
                    if attempt.account.as_deref() == Some(&account) {
                        attempt.cancel.cancel();
                    }
                }
                if let Some(work) = state.accounts.get(&account) {
                    work.cancel.cancel();
                }
                let selected = account.clone();
                self.inner
                    .core
                    .call(move |store| {
                        store.set_google_connection_state(&selected, ConnectionState::Disconnected)
                    })
                    .await?;
                Ok(json!({"account":account,"connection_state":"disconnected"}))
            }
            _ => Err(error(
                "unknown_method",
                "Unknown Google authentication method",
            )),
        }
    }
    async fn begin(
        &self,
        owner: &str,
        owner_cancel: CancellationToken,
        account: Option<String>,
        path: String,
        reauthorize: bool,
        restore_revision: Option<i64>,
    ) -> Result<Value> {
        let id = fm_ipc::random_token();
        let cancel = self.inner.cancel.child_token();
        {
            let mut state = self.inner.state.lock().await;
            if state.stopping {
                return Err(cancelled());
            }
            if state.active.is_some() {
                return Err(error(
                    "login_in_progress",
                    "Finish or cancel the current Google login first",
                ));
            }
            state.active = Some(id.clone());
            state.attempts.push_back(Attempt {
                id: id.clone(),
                owner: owner.to_owned(),
                account: account.clone(),
                phase: "waiting_for_browser",
                error: None,
                cancel: cancel.clone(),
                task: None,
            });
            while state.attempts.len() > RETAINED_ATTEMPTS + 1 {
                state.attempts.pop_front();
            }
        }
        let mut reservation = BeginGuard {
            runtime: self.clone(),
            id: id.clone(),
            cancel: cancel.clone(),
            armed: true,
        };
        let prepare = async {
            let expected = account.clone();
            let references = self
                .inner
                .core
                .call(move |store| {
                    if store.profile_kind()? != ProfileKind::Live {
                        return Err(error(
                            "profile_kind_mismatch",
                            "Connect Google accounts in a separate live profile",
                        ));
                    }
                    let bindings = store.google_account_bindings()?;
                    if let Some(account) = expected.as_deref() {
                        if let Some(revision) = restore_revision {
                            store.google_restore_check(account, revision)?;
                        } else {
                            store.require_active_account(account)?;
                        }
                    }
                    if expected
                        .as_ref()
                        .is_some_and(|id| !bindings.iter().any(|b| &b.account == id))
                    {
                        return Err(error(
                            "not_found",
                            "Reconnect account is not in this profile",
                        ));
                    }
                    bindings
                        .iter()
                        .map(|b| store.google_account_client(&b.account))
                        .collect::<Result<Vec<_>>>()
                })
                .await?;
            let config = read_config(path.clone()).await?;
            if references
                .iter()
                .flatten()
                .any(|reference| reference.client_id != config.client_id())
            {
                return Err(error(
                    "client_mismatch",
                    "Use the desktop OAuth client already connected to this profile",
                ));
            }
            if owner_cancel.is_cancelled() || cancel.is_cancelled() {
                return Err(cancelled());
            }
            let login = if reauthorize {
                DesktopLogin::begin_reauthorization(
                    config.client_id().to_owned(),
                    Duration::from_secs(LOGIN_SECONDS),
                )
                .await
            } else {
                DesktopLogin::begin(
                    config.client_id().to_owned(),
                    Duration::from_secs(LOGIN_SECONDS),
                )
                .await
            }
            .map_err(google_error)?;
            let client = Arc::new(LoadedClient {
                oauth: Arc::new(GoogleOAuth::new(config, self.inner.http.clone())),
                path,
            });
            Ok((login, client))
        }
        .await;
        let (login, client) = match prepare {
            Ok(value) => value,
            Err(failure) => {
                self.finish_attempt(&id, Err(error(&failure.code, &failure.message)))
                    .await;
                reservation.armed = false;
                return Err(failure);
            }
        };
        let url = login.authorization_url().to_owned();
        let runtime = self.clone();
        let task_id = id.clone();
        let task_cancel = cancel.clone();
        #[cfg(feature = "verification-google")]
        let fixture_url = url.clone();
        let mut state = self.inner.state.lock().await;
        if owner_cancel.is_cancelled() || cancel.is_cancelled() || state.stopping {
            return Err(cancelled());
        }
        let task = tokio::spawn(async move {
            let login = runtime.complete_login(
                &task_id,
                login,
                client,
                account,
                restore_revision,
                &task_cancel,
            );
            #[cfg(not(feature = "verification-google"))]
            let operation = login;
            #[cfg(feature = "verification-google")]
            let operation = async {
                if let Some(observer) = runtime.inner.verification_login.clone() {
                    let (result, ()) =
                        tokio::join!(login, observer.callback(fixture_url, task_cancel.clone()));
                    return result;
                }
                login.await
            };
            tokio::pin!(operation);
            let result = tokio::select! {
                result = &mut operation => result,
                _ = owner_cancel.cancelled() => { task_cancel.cancel(); operation.await },
            };
            runtime.finish_attempt(&task_id, result).await;
        });
        if let Some(attempt) = state.attempts.iter_mut().find(|a| a.id == id) {
            attempt.task = Some(task);
        }
        reservation.armed = false;
        Ok(json!({"attempt_id":id,"authorization_url":url,"expires_in_seconds":LOGIN_SECONDS}))
    }
    async fn complete_login(
        &self,
        id: &str,
        login: DesktopLogin,
        client: Arc<LoadedClient>,
        expected: Option<String>,
        restore_revision: Option<i64>,
        cancel: &CancellationToken,
    ) -> Result<String> {
        let code = login
            .wait_for_callback(cancel)
            .await
            .map_err(google_error)?;
        self.phase(id, "exchanging").await;
        let grant = client
            .oauth
            .exchange(code, now_secs(), cancel)
            .await
            .map_err(google_error)?;
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        if let Some(revision) = restore_revision {
            return self
                .complete_restore(
                    id,
                    client,
                    expected.as_deref().ok_or_else(|| {
                        error("account_required", "Choose an archived account to restore")
                    })?,
                    revision,
                    grant,
                    cancel,
                )
                .await;
        }
        let subject = grant.identity.sub.clone();
        let email = grant.identity.email.clone();
        let scopes = grant.granted_scopes.clone();
        let check_subject = subject.clone();
        let check_email = email.clone();
        let check_scopes = scopes.clone();
        let check_expected = expected.clone();
        let client_id = client.oauth.client_id().to_owned();
        let check_client = client_id.clone();
        let (account, generation) = self
            .inner
            .core
            .call(move |store| {
                let account = store.check_google_connection(
                    &check_subject,
                    &check_email,
                    &check_scopes,
                    check_expected.as_deref(),
                    &check_client,
                )?;
                let generation = store
                    .google_account_bindings()?
                    .into_iter()
                    .find(|b| b.account == account)
                    .map(|b| b.connection_generation);
                Ok((account, generation))
            })
            .await?;
        let account_id = AccountId::new(account.clone()).map_err(google_error)?;
        {
            let mut state = self.inner.state.lock().await;
            if cancel.is_cancelled() {
                return Err(cancelled());
            }
            if let Some(attempt) = state.attempts.iter_mut().find(|a| a.id == id) {
                attempt.account = Some(account.clone());
                attempt.phase = "persisting";
            }
            if let Some(work) = state.accounts.get(&account) {
                work.cancel.cancel();
            }
        }
        // Vault and SQLite cannot commit together. Persist a fail-closed state
        // before replacing an existing grant so cancellation, disk failure or
        // process exit cannot reopen new tokens against old Connected/scopes.
        // Recheck the generation only after acquiring the same native credential
        // lock as removal. A completed removal must never precede this store.
        let credential_guard = self
            .inner
            .coordinator
            .account_guard(&account_id, cancel)
            .await
            .map_err(google_error)?;
        let selected = account.clone();
        let persist_cancel = cancel.clone();
        let generation = self
            .inner
            .core
            .call(move |store| {
                if persist_cancel.is_cancelled() {
                    return Err(cancelled());
                }
                let current = store
                    .google_account_bindings()?
                    .into_iter()
                    .find(|binding| binding.account == selected)
                    .map(|binding| binding.connection_generation);
                if current != generation {
                    return Err(error(
                        "stale_connection",
                        "Google connection changed during login; reconnect explicitly",
                    ));
                }
                if current.is_some() {
                    store.set_google_connection_state(
                        &selected,
                        ConnectionState::ReauthorizationRequired,
                    )?;
                    Ok(store
                        .google_account_bindings()?
                        .into_iter()
                        .find(|binding| binding.account == selected)
                        .map(|binding| binding.connection_generation))
                } else {
                    Ok(None)
                }
            })
            .await?;
        // Fence any old refresh's final native store, including an aborted caller.
        let _credential_guard = credential_guard
            .persist_retaining(grant, self.inner.vault.clone())
            .await
            .map_err(google_error)?;
        let mut state = self.inner.state.lock().await;
        let commit_cancel = cancel.clone();
        let path = client.path.clone();
        let check_account = account.clone();
        let bound_subject = subject.clone();
        let committed = self
            .inner
            .core
            .call(move |store| {
                if commit_cancel.is_cancelled() {
                    return Err(cancelled());
                }
                let current = store
                    .google_account_bindings()?
                    .into_iter()
                    .find(|b| b.account == check_account)
                    .map(|b| b.connection_generation);
                if current != generation {
                    return Err(error(
                        "stale_connection",
                        "Google connection changed during login; reconnect explicitly",
                    ));
                }
                store.bind_google_account_with_client(
                    &subject,
                    &email,
                    &scopes,
                    expected.as_deref(),
                    &client_id,
                    &path,
                )
            })
            .await?;
        client
            .oauth
            .bind_account(account_id, bound_subject)
            .map_err(google_error)?;
        state.client = Some(client);
        state.accounts.remove(&committed);
        Ok(committed)
    }
    async fn phase(&self, id: &str, phase: &'static str) {
        if let Some(attempt) = self
            .inner
            .state
            .lock()
            .await
            .attempts
            .iter_mut()
            .find(|a| a.id == id)
        {
            if !attempt.cancel.is_cancelled() {
                attempt.phase = phase;
            }
        }
    }
    async fn finish_attempt(&self, id: &str, result: Result<String>) {
        let mut state = self.inner.state.lock().await;
        if let Some(attempt) = state.attempts.iter_mut().find(|a| a.id == id) {
            match result {
                Ok(account) => {
                    attempt.phase = "connected";
                    attempt.account = Some(account);
                }
                Err(failure) => {
                    attempt.phase = if attempt.cancel.is_cancelled() || failure.code == "cancelled"
                    {
                        "cancelled"
                    } else {
                        "failed"
                    };
                    attempt.error = Some(json!({"code":failure.code,"message":failure.message}));
                }
            }
        }
        if state.active.as_deref() == Some(id) {
            state.active = None;
        }
        while state.attempts.len() > RETAINED_ATTEMPTS {
            state.attempts.pop_front();
        }
    }
    /// Use this cancellation token for the whole account job, including provider
    /// I/O after access_token returns. Disconnect cancels all such children.
    #[allow(dead_code)] // Explicit shared API for the upcoming mail/calendar worker wiring.
    pub(crate) async fn account_work_cancel(&self, account: &str) -> Result<CancellationToken> {
        let mut state = self.inner.state.lock().await;
        if state.stopping || self.inner.cancel.is_cancelled() {
            return Err(cancelled());
        }
        let selected = account.to_owned();
        let binding = self
            .inner
            .core
            .call(move |store| {
                store
                    .google_account_bindings()?
                    .into_iter()
                    .find(|b| b.account == selected)
                    .ok_or_else(|| error("not_found", "Google account is not in this profile"))
            })
            .await?;
        if binding.state != ConnectionState::Connected {
            return Err(error(
                "reauthorization_required",
                "Connect this Google account before using its provider",
            ));
        }
        let work = state
            .accounts
            .entry(account.to_owned())
            .or_insert_with(|| AccountWork {
                generation: binding.connection_generation,
                cancel: self.inner.cancel.child_token(),
            });
        if work.generation != binding.connection_generation {
            work.cancel.cancel();
            *work = AccountWork {
                generation: binding.connection_generation,
                cancel: self.inner.cancel.child_token(),
            };
        }
        if work.cancel.is_cancelled() {
            return Err(cancelled());
        }
        Ok(work.cancel.child_token())
    }
    #[allow(dead_code)] // Explicit shared API; fixture tests exercise the current boundary.
    pub(crate) async fn access_token(
        &self,
        account: &str,
        cancel: &CancellationToken,
    ) -> Result<String> {
        let work_cancel = self.account_work_cancel(account).await?;
        let selected = account.to_owned();
        let (reference, subject) = self
            .inner
            .core
            .call(move |store| {
                let reference = store.google_account_client(&selected)?.ok_or_else(|| {
                    error(
                        "reauthorization_required",
                        "Reconnect to choose this profile's desktop OAuth client",
                    )
                })?;
                let subject = store
                    .google_account_bindings()?
                    .into_iter()
                    .find(|b| b.account == selected)
                    .ok_or_else(|| error("not_found", "Google account is not in this profile"))?
                    .subject;
                Ok((reference, subject))
            })
            .await?;
        let client = self.saved_client(reference).await?;
        let account_id = AccountId::new(account.to_owned()).map_err(google_error)?;
        client
            .oauth
            .bind_account(account_id.clone(), subject)
            .map_err(google_error)?;
        let operation = self.inner.coordinator.access_token_async(
            &account_id,
            now_secs(),
            self.inner.vault.clone(),
            client.oauth.as_ref(),
            &work_cancel,
        );
        tokio::pin!(operation);
        let result = tokio::select! {
            result = &mut operation => result,
            _ = cancel.cancelled() => { work_cancel.cancel(); operation.await },
        };
        if cancel.is_cancelled() || work_cancel.is_cancelled() {
            return Err(cancelled());
        }
        if result == Err(GoogleError::ReauthorizationRequired) {
            let selected = account.to_owned();
            let caller_cancel = cancel.clone();
            let account_cancel = work_cancel.clone();
            self.inner
                .core
                .call(move |store| {
                    // A queued refresh failure must not downgrade a reconnect
                    // or disconnect that cancelled its account work meanwhile.
                    if caller_cancel.is_cancelled() || account_cancel.is_cancelled() {
                        return Err(cancelled());
                    }
                    store.set_google_connection_state(
                        &selected,
                        ConnectionState::ReauthorizationRequired,
                    )
                })
                .await?;
            let state = self.inner.state.lock().await;
            // Stop sibling mail/calendar I/O only for this rejected generation.
            // A reconnect cancels this child before replacing its account work.
            if !work_cancel.is_cancelled() {
                if let Some(work) = state.accounts.get(account) {
                    work.cancel.cancel();
                }
            }
        }
        result.map_err(google_error)
    }
    #[allow(dead_code)] // Called by the shared token API above.
    async fn saved_client(&self, reference: GoogleClientReference) -> Result<Arc<LoadedClient>> {
        let mut state = self.inner.state.lock().await;
        if let Some(client) = &state.client {
            if client.oauth.client_id() != reference.client_id {
                return Err(error(
                    "client_mismatch",
                    "Stored Google client differs from this profile's active client",
                ));
            }
            if client.path == reference.config_path {
                return Ok(client.clone());
            }
        }
        let config = read_config(reference.config_path.clone()).await?;
        if config.client_id() != reference.client_id {
            return Err(error(
                "client_mismatch",
                "Desktop OAuth file no longer matches the connected client",
            ));
        }
        let client = Arc::new(LoadedClient {
            oauth: Arc::new(GoogleOAuth::new(config, self.inner.http.clone())),
            path: reference.config_path,
        });
        state.client = Some(client.clone());
        Ok(client)
    }
    pub(crate) async fn shutdown(&self) {
        self.inner.cancel.cancel();
        let tasks = {
            let mut state = self.inner.state.lock().await;
            state.stopping = true;
            for work in state.accounts.values() {
                work.cancel.cancel();
            }
            state
                .attempts
                .iter_mut()
                .filter_map(|attempt| {
                    attempt.cancel.cancel();
                    attempt.task.take()
                })
                .collect::<Vec<_>>()
        };
        for task in tasks {
            let _ = task.await;
        }
    }
}

async fn read_config(path: String) -> Result<OAuthClientConfig> {
    tokio::task::spawn_blocking(move || {
        if path.chars().any(char::is_control) {
            return Err(invalid_config());
        }
        let path = Path::new(&path);
        if !path.is_absolute() || path.as_os_str().len() > 32768 {
            return Err(invalid_config());
        }
        let metadata = std::fs::symlink_metadata(path).map_err(|_| invalid_config())?;
        if !metadata.is_file() || metadata.len() > CONFIG_BYTES {
            return Err(invalid_config());
        }
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|_| invalid_config())?
            .take(CONFIG_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid_config())?;
        let decoded = if bytes.len() <= CONFIG_BYTES as usize {
            serde_json::from_slice::<Value>(&bytes).ok()
        } else {
            None
        };
        bytes.fill(0);
        let mut value = decoded.ok_or_else(invalid_config)?;
        if value.get("web").is_some() {
            return Err(invalid_config());
        }
        let installed = value
            .get_mut("installed")
            .and_then(Value::as_object_mut)
            .ok_or_else(invalid_config)?;
        let id = installed
            .remove("client_id")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(invalid_config)?;
        if !id.ends_with(".apps.googleusercontent.com")
            || !id
                .bytes()
                .all(|v| v.is_ascii_alphanumeric() || matches!(v, b'.' | b'-' | b'_'))
        {
            return Err(invalid_config());
        }
        let secret = installed
            .remove("client_secret")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(invalid_config)?;
        // Endpoint and redirect fields are deliberately ignored. The tested
        // OAuth component owns fixed Google endpoints and a freshly bound loopback.
        OAuthClientConfig::new(id, Some(secret)).map_err(|_| invalid_config())
    })
    .await
    .map_err(|_| invalid_config())?
}
fn fields(params: &Value, allowed: &[&str]) -> Result<()> {
    if !params.is_object()
        || params
            .as_object()
            .unwrap()
            .keys()
            .any(|key| !allowed.contains(&key.as_str()))
    {
        return Err(error(
            "invalid_request",
            "Unsupported authentication parameter",
        ));
    }
    Ok(())
}
fn terminal(phase: &str) -> bool {
    matches!(phase, "connected" | "cancelled" | "failed")
}
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn error(code: &str, message: &str) -> CoreError {
    CoreError {
        code: code.into(),
        message: message.into(),
        details: Value::Null,
    }
}
fn invalid_config() -> CoreError {
    error(
        "invalid_client_config",
        "Choose a bounded Google installed-desktop client JSON file at an absolute path",
    )
}
fn cancelled() -> CoreError {
    error("cancelled", "Google authentication was cancelled")
}
fn google_error(failure: GoogleError) -> CoreError {
    match failure {
        GoogleError::ReauthorizationRequired => error(
            "reauthorization_required",
            "Google requires an explicit reconnect",
        ),
        GoogleError::PermissionDenied => error(
            "permission_denied",
            "Google authorization or identity was not accepted",
        ),
        GoogleError::StoreUnavailable => error(
            "vault_unavailable",
            "The operating-system credential vault is unavailable",
        ),
        GoogleError::RateLimited { .. } => error(
            "rate_limited",
            "Google authentication is temporarily rate limited",
        ),
        GoogleError::TransportInterrupted => error(
            "auth_interrupted",
            "Google authentication was interrupted or expired",
        ),
        _ => error(
            "invalid_auth_response",
            "Google authentication did not return a valid identity and grant",
        ),
    }
}

#[cfg(test)]
mod tests;
