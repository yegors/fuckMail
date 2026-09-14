//! Fixed identities and real loopback callbacks for isolated account workflows.
//! OAuth signing material is public test material; there is no network fallback.
use super::{
    transport::{decoded_query, FixtureHttp},
    Account, Evidence, CLIENT,
};
use crate::google_auth::VerificationLogin;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use fm_google::{
    auth::{CredentialVault, StoredToken},
    transport::{
        CancellationToken, HttpFuture, HttpMethod, HttpRequest, HttpResponse, HttpTransport,
        TransportError,
    },
    AccountId, GoogleError,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    future::Future,
    io::Read,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const STEP_FILE: &str = "verification-google-account-step.json";
const CALLBACK_HOLD: &str = "verification-google-auth-hold-callback";
const STORE_HOLD: &str = "verification-google-auth-hold-store";
const CLEANUP_HOLD: &str = "verification-google-cleanup-hold";
const CLEANUP_FAIL: &str = "verification-google-cleanup-fail";
const HOLD_TIMEOUT: Duration = Duration::from_secs(20);
const STEP_LIMIT: u64 = 64;

struct Step {
    id: u64,
    account: usize,
    denied: bool,
    readonly: bool,
    renamed: bool,
}
struct Exchange {
    account: usize,
    redirect: String,
    challenge: String,
    nonce: String,
    readonly: bool,
    renamed: bool,
}
#[derive(Default)]
struct State {
    phase: &'static str,
    active_step: Option<u64>,
    consumed_steps: Vec<u64>,
    auth_attempts: u64,
    callbacks_sent: u64,
    callbacks_denied: u64,
    token_exchanges: u64,
    jwks_reads: u64,
    vault_load_attempts: u64,
    vault_store_attempts: u64,
    vault_store_completed: u64,
    vault_remove_attempts: u64,
    vault_remove_completed: u64,
    vault_remove_failed: u64,
    active_callbacks: u64,
    active_vault_calls: u64,
    tokens: BTreeMap<String, StoredToken>,
    exchanges: BTreeMap<String, Exchange>,
    errors: Vec<&'static str>,
}
pub(super) struct AccountsFixture {
    profile: PathBuf,
    accounts: Arc<Vec<Account>>,
    provider: Arc<FixtureHttp>,
    evidence: Arc<Mutex<Evidence>>,
    state: Mutex<State>,
}
impl AccountsFixture {
    pub(super) fn new(
        profile: PathBuf,
        accounts: Arc<Vec<Account>>,
        provider: Arc<FixtureHttp>,
        evidence: Arc<Mutex<Evidence>>,
    ) -> Self {
        Self {
            profile,
            accounts,
            provider,
            evidence,
            state: Mutex::new(State {
                phase: "idle",
                ..State::default()
            }),
        }
    }
    fn error(&self, code: &'static str) {
        let mut state = self.state.lock().unwrap();
        if state.errors.len() < 16 && !state.errors.contains(&code) {
            state.errors.push(code);
        }
        drop(state);
        self.evidence.lock().unwrap().error(code);
    }
    pub(super) fn report(&self) -> Value {
        let s = self.state.lock().unwrap();
        json!({"enabled":true,"mode":"accounts-v1","initial_bound_accounts":0,"step_limit":STEP_LIMIT,
            "hold_timeout_ms":HOLD_TIMEOUT.as_millis(),"phase":s.phase,"active_step":s.active_step,
            "consumed_steps":s.consumed_steps,"auth_attempts":s.auth_attempts,"callbacks_sent":s.callbacks_sent,
            "callbacks_denied":s.callbacks_denied,"token_exchanges":s.token_exchanges,"jwks_reads":s.jwks_reads,
            "vault_load_attempts":s.vault_load_attempts,"vault_store_attempts":s.vault_store_attempts,
            "vault_store_completed":s.vault_store_completed,"vault_remove_attempts":s.vault_remove_attempts,
            "vault_remove_completed":s.vault_remove_completed,"vault_remove_failed":s.vault_remove_failed,
            "credential_accounts":s.tokens.keys().collect::<Vec<_>>(),"active_callbacks":s.active_callbacks,
            "active_vault_calls":s.active_vault_calls,"errors":s.errors})
    }
    pub(super) fn check_drained(&self) {
        let active = {
            let s = self.state.lock().unwrap();
            s.active_callbacks + s.active_vault_calls
        };
        if active != 0 {
            self.error("account_fixture_work_survived_shutdown");
        }
    }
    fn marker(&self, name: &str) -> Result<bool, &'static str> {
        match std::fs::symlink_metadata(self.profile.join(name)) {
            Ok(m) if m.is_file() && m.len() == 0 => Ok(true),
            Ok(_) => Err("invalid_account_fixture_marker"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(_) => Err("account_fixture_marker_unreadable"),
        }
    }
    fn step(&self) -> Result<Step, &'static str> {
        let mut bytes = Vec::new();
        std::fs::File::open(self.profile.join(STEP_FILE))
            .map_err(|_| "account_fixture_step_unreadable")?
            .take(4097)
            .read_to_end(&mut bytes)
            .map_err(|_| "account_fixture_step_unreadable")?;
        if bytes.len() > 4096 {
            return Err("account_fixture_step_too_large");
        }
        let v: Value =
            serde_json::from_slice(&bytes).map_err(|_| "invalid_account_fixture_step")?;
        let fields = v.as_object().ok_or("invalid_account_fixture_step")?;
        if fields.len() != 5
            || !["step", "identity", "outcome", "grant", "renamed"]
                .iter()
                .all(|k| fields.contains_key(*k))
        {
            return Err("invalid_account_fixture_step");
        }
        let id = v["step"]
            .as_u64()
            .filter(|n| (1..=STEP_LIMIT).contains(n))
            .ok_or("invalid_account_fixture_step")?;
        let account = match v["identity"].as_str() {
            Some("personal") => 0,
            Some("workspace") => 1,
            _ => return Err("invalid_account_fixture_step"),
        };
        let denied = match v["outcome"].as_str() {
            Some("success") => false,
            Some("denied") => true,
            _ => return Err("invalid_account_fixture_step"),
        };
        let readonly = match v["grant"].as_str() {
            Some("full") => false,
            Some("mail_readonly") => true,
            _ => return Err("invalid_account_fixture_step"),
        };
        let renamed = v["renamed"]
            .as_bool()
            .ok_or("invalid_account_fixture_step")?;
        let mut s = self.state.lock().unwrap();
        if s.consumed_steps.last().is_some_and(|n| *n >= id) {
            return Err("account_fixture_step_reused");
        }
        s.consumed_steps.push(id);
        s.active_step = Some(id);
        s.auth_attempts += 1;
        Ok(Step {
            id,
            account,
            denied,
            readonly,
            renamed,
        })
    }
    async fn callback_inner(
        &self,
        url: String,
        cancel: &CancellationToken,
    ) -> Result<(), &'static str> {
        let step = self.step()?;
        let query = url
            .strip_prefix("https://accounts.google.com/o/oauth2/v2/auth?")
            .ok_or("invalid_fixture_authorization_origin")?;
        let fields = form(query).ok_or("invalid_fixture_authorization_fields")?;
        if fields.get("client_id").map(String::as_str) != Some(CLIENT)
            || fields.get("code_challenge_method").map(String::as_str) != Some("S256")
        {
            return Err("invalid_fixture_authorization_fields");
        }
        let redirect = fields
            .get("redirect_uri")
            .ok_or("invalid_fixture_authorization_fields")?;
        let address = redirect
            .strip_prefix("http://127.0.0.1:")
            .and_then(|s| s.strip_suffix("/oauth2/callback"))
            .and_then(|s| s.parse::<u16>().ok())
            .filter(|p| *p != 0)
            .ok_or("invalid_fixture_loopback")?;
        let nonce = fields
            .get("nonce")
            .filter(|s| !s.is_empty())
            .ok_or("invalid_fixture_authorization_fields")?;
        let challenge = fields
            .get("code_challenge")
            .ok_or("invalid_fixture_authorization_fields")?;
        let state = fields
            .get("state")
            .filter(|s| {
                s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
            })
            .ok_or("invalid_fixture_authorization_fields")?;
        self.state.lock().unwrap().phase = "callback_held";
        let started = Instant::now();
        while self.marker(CALLBACK_HOLD)? {
            if started.elapsed() >= HOLD_TIMEOUT {
                return Err("account_callback_hold_timeout");
            }
            tokio::select! { _=cancel.cancelled()=>return Ok(()), _=tokio::time::sleep(Duration::from_millis(10))=>{} }
        }
        if cancel.is_cancelled() {
            return Ok(());
        }
        let code = format!("synthetic-account-code-{}", step.id);
        if !step.denied {
            self.state.lock().unwrap().exchanges.insert(
                code.clone(),
                Exchange {
                    account: step.account,
                    redirect: redirect.clone(),
                    challenge: challenge.clone(),
                    nonce: nonce.clone(),
                    readonly: step.readonly,
                    renamed: step.renamed,
                },
            );
        }
        self.state.lock().unwrap().phase = "callback_sent";
        let mut socket = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, address))
            .await
            .map_err(|_| "fixture_loopback_connect_failed")?;
        let outcome = if step.denied {
            "error=access_denied".to_owned()
        } else {
            format!("code={code}")
        };
        socket.write_all(format!("GET /oauth2/callback?state={state}&{outcome} HTTP/1.1\r\nHost: 127.0.0.1:{address}\r\nConnection: close\r\n\r\n").as_bytes()).await.map_err(|_| "fixture_loopback_write_failed")?;
        {
            let mut s = self.state.lock().unwrap();
            s.callbacks_sent += 1;
            if step.denied {
                s.callbacks_denied += 1;
            }
        }
        let mut bytes = Vec::new();
        socket
            .take(8193)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| "fixture_loopback_read_failed")?;
        if bytes.len() > 8192 || !bytes.starts_with(b"HTTP/1.1") {
            return Err("invalid_fixture_loopback_response");
        }
        Ok(())
    }
    fn known(&self, account: &AccountId) -> bool {
        self.accounts.iter().any(|a| a.id == account.as_str())
    }
    fn hold(&self, marker: &str, phase: &'static str) -> Result<(), GoogleError> {
        self.state.lock().unwrap().phase = phase;
        let started = Instant::now();
        loop {
            match self.marker(marker) {
                Ok(false) => return Ok(()),
                Ok(true) if started.elapsed() < HOLD_TIMEOUT => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Ok(true) => {
                    self.error("account_vault_hold_timeout");
                    return Err(GoogleError::StoreUnavailable);
                }
                Err(code) => {
                    self.error(code);
                    return Err(GoogleError::StoreUnavailable);
                }
            }
        }
    }
    fn oauth(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError> {
        let value = if request.url() == "https://www.googleapis.com/oauth2/v3/certs"
            && request.method() == HttpMethod::Get
        {
            self.state.lock().unwrap().jwks_reads += 1;
            serde_json::from_slice(include_bytes!(
                "../../../../fm-google/tests/fixtures/auth-v1/jwks.json"
            ))
            .map_err(|_| TransportError::InvalidRequest)?
        } else if request.url() == "https://oauth2.googleapis.com/token"
            && request.method() == HttpMethod::Post
        {
            let fields = std::str::from_utf8(request.body_bytes())
                .ok()
                .and_then(form)
                .ok_or(TransportError::InvalidRequest)?;
            let exchange = fields
                .get("code")
                .and_then(|code| self.state.lock().unwrap().exchanges.remove(code))
                .ok_or(TransportError::InvalidRequest)?;
            if fields.get("client_id").map(String::as_str) != Some(CLIENT)
                || fields.get("client_secret").map(String::as_str)
                    != Some("synthetic-desktop-secret")
                || fields.get("grant_type").map(String::as_str) != Some("authorization_code")
                || fields.get("redirect_uri") != Some(&exchange.redirect)
                || fields
                    .get("code_verifier")
                    .map(|v| URL_SAFE_NO_PAD.encode(Sha256::digest(v.as_bytes())))
                    != Some(exchange.challenge)
            {
                return Err(TransportError::InvalidRequest);
            }
            let account = &self.accounts[exchange.account];
            let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
            header.kid = Some("synthetic-rsa-v1".into());
            let now = super::unix_ms() / 1000;
            let email = if exchange.renamed {
                format!("renamed-{}@example.test", account.key)
            } else {
                account.email.clone()
            };
            let claims = json!({"iss":"https://accounts.google.com","aud":CLIENT,"sub":account.subject,
                "iat":now-10,"exp":now+600,"nonce":exchange.nonce,"email":email,"email_verified":true});
            let token = jsonwebtoken::encode(
                &header,
                &claims,
                &jsonwebtoken::EncodingKey::from_rsa_der(include_bytes!(
                    "../../../../fm-google/tests/fixtures/auth-v1/synthetic-rsa-private.der"
                )),
            )
            .map_err(|_| TransportError::InvalidRequest)?;
            let scopes = if exchange.readonly {
                vec![
                    "openid",
                    "email",
                    "https://www.googleapis.com/auth/gmail.readonly",
                ]
            } else {
                vec![
                    "openid",
                    "email",
                    "https://www.googleapis.com/auth/gmail.modify",
                    "https://www.googleapis.com/auth/calendar.events",
                    "https://www.googleapis.com/auth/calendar.calendarlist.readonly",
                ]
            };
            self.state.lock().unwrap().token_exchanges += 1;
            json!({"access_token":account.token(),"refresh_token":format!("synthetic-{}-refresh",account.key),"expires_in":3600,"token_type":"Bearer","scope":scopes.join(" "),"id_token":token})
        } else {
            return Err(TransportError::InvalidRequest);
        };
        Ok(HttpResponse {
            status: 200,
            body: serde_json::to_vec(&value).map_err(|_| TransportError::InvalidRequest)?,
            retry_after_secs: None,
            etag: None,
        })
    }
}
fn form(value: &str) -> Option<BTreeMap<String, String>> {
    let mut fields = BTreeMap::new();
    for part in value.split('&') {
        let (key, value) = part.split_once('=')?;
        if fields
            .insert(decoded_query(key)?, decoded_query(value)?)
            .is_some()
        {
            return None;
        }
    }
    Some(fields)
}
impl VerificationLogin for AccountsFixture {
    fn callback<'a>(
        &'a self,
        authorization_url: String,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            self.state.lock().unwrap().active_callbacks += 1;
            let result = tokio::select! { _=cancel.cancelled()=>Ok(()), result=tokio::time::timeout(HOLD_TIMEOUT+Duration::from_secs(5),self.callback_inner(authorization_url,&cancel))=>result.unwrap_or(Err("fixture_callback_timeout")) };
            if let Err(code) = result {
                self.error(code);
                cancel.cancel();
            }
            let mut s = self.state.lock().unwrap();
            s.active_callbacks -= 1;
            if s.active_callbacks == 0 && s.active_vault_calls == 0 {
                s.phase = "idle";
            }
        })
    }
}
impl CredentialVault for AccountsFixture {
    fn load(&self, account: &AccountId) -> Result<Option<StoredToken>, GoogleError> {
        self.evidence.lock().unwrap().counters.vault_loads += 1;
        if !self.known(account) {
            self.error("unknown_account_fixture_vault_identity");
            return Err(GoogleError::StoreUnavailable);
        }
        let mut s = self.state.lock().unwrap();
        s.vault_load_attempts += 1;
        Ok(s.tokens.get(account.as_str()).cloned())
    }
    fn store(&self, account: &AccountId, token: StoredToken) -> Result<(), GoogleError> {
        self.evidence.lock().unwrap().counters.vault_stores += 1;
        if !self.known(account) {
            self.error("unknown_account_fixture_vault_identity");
            return Err(GoogleError::StoreUnavailable);
        }
        {
            let mut s = self.state.lock().unwrap();
            s.vault_store_attempts += 1;
            s.active_vault_calls += 1;
            s.tokens.insert(account.as_str().into(), token);
        }
        let result = self.hold(STORE_HOLD, "store_held_after_write");
        let mut s = self.state.lock().unwrap();
        s.active_vault_calls -= 1;
        if result.is_ok() {
            s.vault_store_completed += 1;
        }
        s.phase = "idle";
        result
    }
    fn remove(&self, account: &AccountId) -> Result<(), GoogleError> {
        if !self.known(account) {
            self.error("unknown_account_fixture_vault_identity");
            return Err(GoogleError::StoreUnavailable);
        }
        {
            let mut s = self.state.lock().unwrap();
            s.vault_remove_attempts += 1;
            s.active_vault_calls += 1;
        }
        let result = self
            .hold(CLEANUP_HOLD, "cleanup_held_before_delete")
            .and_then(|()| match self.marker(CLEANUP_FAIL) {
                Ok(false) => Ok(()),
                Ok(true) => Err(GoogleError::StoreUnavailable),
                Err(code) => {
                    self.error(code);
                    Err(GoogleError::StoreUnavailable)
                }
            });
        let mut s = self.state.lock().unwrap();
        s.active_vault_calls -= 1;
        if result.is_ok() {
            s.tokens.remove(account.as_str());
            s.vault_remove_completed += 1;
        } else {
            s.vault_remove_failed += 1;
        }
        s.phase = "idle";
        result
    }
}
impl HttpTransport for AccountsFixture {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
        cancel: &'a CancellationToken,
    ) -> HttpFuture<'a> {
        Box::pin(async move {
            if cancel.is_cancelled() {
                return Err(TransportError::Cancelled);
            }
            if request.url().starts_with("https://oauth2.googleapis.com/")
                || request
                    .url()
                    .starts_with("https://www.googleapis.com/oauth2/")
            {
                let result = self.oauth(&request);
                if result.is_err() {
                    self.error("unexpected_account_fixture_oauth_request");
                }
                result
            } else {
                self.provider.execute(request, cancel).await
            }
        })
    }
}
