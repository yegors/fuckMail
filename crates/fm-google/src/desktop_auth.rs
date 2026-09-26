//! Explicit desktop login. All network calls use the shared bounded transport.
//! No browser launch, account registration, UI serialization, or token file lives here.
use crate::auth::{
    AsyncRefreshTransport, AuthorizationCode, CredentialVault, DesktopAuthorization, RefreshResult,
    StoredToken,
};
use crate::transport::{CancellationToken, HttpMethod, HttpRequest, HttpTransport};
use crate::{AccountId, GoogleError};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use oauth2::{
    url::{form_urlencoded, Url},
    CsrfToken,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{timeout_at, Instant},
};
use zeroize::Zeroizing;

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const JWKS_URL: &str = "https://www.googleapis.com/oauth2/v3/certs";
const MAX_CALLBACK_BYTES: usize = 16 * 1024;

/// A pending loopback sign-in: an ephemeral listener on `127.0.0.1` and the
/// [`DesktopAuthorization`] whose redirect points at it, valid until a deadline.
///
/// The caller opens [`DesktopLogin::authorization_url`] in the system browser,
/// awaits [`DesktopLogin::wait_for_callback`], then redeems the code with
/// [`GoogleOAuth::exchange`].
pub struct DesktopLogin {
    listener: TcpListener,
    authorization: DesktopAuthorization,
    deadline: Instant,
}

impl DesktopLogin {
    /// Starts a sign-in with default access that expires after `lifetime`.
    pub async fn begin(client_id: String, lifetime: Duration) -> Result<Self, GoogleError> {
        Self::begin_with_login_hint(client_id, lifetime, false, None).await
    }

    /// Only for an explicit reconnect/recovery action when a durable grant is missing.
    pub async fn begin_reauthorization(
        client_id: String,
        lifetime: Duration,
    ) -> Result<Self, GoogleError> {
        Self::begin_with_login_hint(client_id, lifetime, true, None).await
    }

    /// [`DesktopLogin::begin_with_access`] with default access.
    pub async fn begin_with_login_hint(
        client_id: String,
        lifetime: Duration,
        reconsent: bool,
        login_hint: Option<&str>,
    ) -> Result<Self, GoogleError> {
        Self::begin_with_access(
            client_id,
            lifetime,
            reconsent,
            login_hint,
            crate::auth::RequestedAccess::default(),
        )
        .await
    }

    /// Binds an ephemeral IPv4 loopback port and prepares the consent URL for it.
    ///
    /// `lifetime` (up to 10 minutes) bounds the whole wait for the callback.
    /// See [`DesktopAuthorization::with_access`] for `reconsent`, `login_hint`
    /// and `access`. Errors with `TransportInterrupted` if no port can be bound.
    pub async fn begin_with_access(
        client_id: String,
        lifetime: Duration,
        reconsent: bool,
        login_hint: Option<&str>,
        access: crate::auth::RequestedAccess,
    ) -> Result<Self, GoogleError> {
        // Validate before binding so a bad request never opens a port.
        access.scopes()?;
        if let Some(email) = login_hint {
            crate::auth::validate_login_hint(email)?;
        }
        if lifetime.is_zero() || lifetime > Duration::from_secs(600) {
            return Err(GoogleError::InvalidResponse("invalid login timeout"));
        }
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|_| GoogleError::TransportInterrupted)?;
        let port = listener
            .local_addr()
            .map_err(|_| GoogleError::TransportInterrupted)?
            .port();
        Ok(Self {
            listener,
            authorization: DesktopAuthorization::with_access(
                client_id, port, reconsent, login_hint, access,
            )?,
            deadline: Instant::now() + lifetime,
        })
    }

    /// Caller may open this only for an explicit user login action. Never log it.
    pub fn authorization_url(&self) -> &str {
        &self.authorization.url
    }
    pub fn redirect_uri(&self) -> &str {
        &self.authorization.redirect_uri
    }

    /// Serves the loopback redirect until a callback with this attempt's
    /// `state` arrives, and returns its code. Consumes the login.
    ///
    /// Connections from non-loopback peers are dropped. Malformed requests
    /// and callbacks with another `state` get an error page and are ignored.
    /// Errors: `PermissionDenied` if the matching callback carries `error`
    /// (the person declined) or after 32 connections without one, and
    /// `TransportInterrupted` at the deadline or on cancellation.
    pub async fn wait_for_callback(
        self,
        cancel: &CancellationToken,
    ) -> Result<AuthorizationCode, GoogleError> {
        let expected_host = self
            .authorization
            .redirect_uri
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or_default()
            .to_owned();
        // Bound invalid callback traffic as well as elapsed lifetime and each read.
        for _ in 0..32 {
            let (mut stream, peer) = tokio::select! {
                _ = cancel.cancelled() => return Err(GoogleError::TransportInterrupted),
                result = timeout_at(self.deadline, self.listener.accept()) =>
                    result.map_err(|_| GoogleError::TransportInterrupted)?
                    .map_err(|_| GoogleError::TransportInterrupted)?,
            };
            if !peer.ip().is_loopback() {
                continue;
            }
            // Give each connection at most 2 seconds to send its headers, so
            // an idle local client cannot hold up the only listener.
            let request = tokio::select! {
                _ = cancel.cancelled() => return Err(GoogleError::TransportInterrupted),
                result = timeout_at(self.deadline.min(Instant::now() + Duration::from_secs(2)), read_headers(&mut stream)) => result,
            };
            let callback = request
                .ok()
                .and_then(Result::ok)
                .and_then(|bytes| parse_callback(&bytes, &expected_host).ok());
            let Some(callback) = callback else {
                reply(&mut stream, false).await;
                continue;
            };
            // Only this attempt's state may end the wait. A stale browser tab
            // or a local probe cannot make the sign-in fail.
            if !self.authorization.matches_state(&callback.state) {
                reply(&mut stream, false).await;
                continue;
            }
            reply(&mut stream, callback.code.is_some()).await;
            return match callback.code {
                Some(code) => self.authorization.accept_callback(&callback.state, code),
                None => Err(GoogleError::PermissionDenied),
            };
        }
        Err(GoogleError::PermissionDenied)
    }
}

/// Reads up to the end of the request headers. The callback is a GET whose
/// data is all in the query string, so no body is read. The buffer is zeroized
/// because it holds the authorization code.
async fn read_headers(stream: &mut TcpStream) -> Result<Zeroizing<Vec<u8>>, GoogleError> {
    let mut bytes = Zeroizing::new(Vec::new());
    loop {
        let mut block = [0; 1024];
        let count = stream
            .read(&mut block)
            .await
            .map_err(|_| GoogleError::TransportInterrupted)?;
        if count == 0 || bytes.len() + count > MAX_CALLBACK_BYTES {
            return Err(GoogleError::PermissionDenied);
        }
        bytes.extend_from_slice(&block[..count]);
        if bytes.windows(4).any(|v| v == b"\r\n\r\n") {
            return Ok(bytes);
        }
    }
}

struct Callback {
    state: String,
    code: Option<String>,
}

/// Accepts exactly one shape: an HTTP/1.1 `GET /oauth2/callback?...` with a
/// single `Host` equal to the redirect's `127.0.0.1:<port>`, no body, and
/// `state` plus exactly one of `code` or `error`, each at most once.
fn parse_callback(bytes: &[u8], expected_host: &str) -> Result<Callback, GoogleError> {
    let text = std::str::from_utf8(bytes).map_err(|_| GoogleError::PermissionDenied)?;
    let mut lines = text.split("\r\n");
    let request: Vec<_> = lines.next().unwrap_or_default().split(' ').collect();
    if request.len() != 3 || request[0] != "GET" || request[2] != "HTTP/1.1" {
        return Err(GoogleError::PermissionDenied);
    }
    let mut host = None;
    for line in lines.take_while(|v| !v.is_empty()) {
        let (key, value) = line.split_once(':').ok_or(GoogleError::PermissionDenied)?;
        if key.eq_ignore_ascii_case("host") && host.replace(value.trim()).is_some() {
            return Err(GoogleError::PermissionDenied);
        }
        if key.eq_ignore_ascii_case("content-length") && value.trim() != "0" {
            return Err(GoogleError::PermissionDenied);
        }
        if key.eq_ignore_ascii_case("transfer-encoding") {
            return Err(GoogleError::PermissionDenied);
        }
    }
    // The Host check rejects requests a web page sends through a hostname
    // that merely resolves to loopback (DNS rebinding).
    if host != Some(expected_host) || !request[1].starts_with("/oauth2/callback?") {
        return Err(GoogleError::PermissionDenied);
    }
    let url = Url::parse(&format!("http://{expected_host}{}", request[1]))
        .map_err(|_| GoogleError::PermissionDenied)?;
    if url.path() != "/oauth2/callback" || url.fragment().is_some() {
        return Err(GoogleError::PermissionDenied);
    }
    // A repeated parameter could make two parsers disagree on its value, so
    // duplicates are refused; unrelated parameters such as `scope` are ignored.
    let mut fields = BTreeMap::new();
    for (key, value) in url.query_pairs() {
        if matches!(key.as_ref(), "state" | "code" | "error")
            && fields
                .insert(key.into_owned(), value.into_owned())
                .is_some()
        {
            return Err(GoogleError::PermissionDenied);
        }
    }
    let state = fields
        .remove("state")
        .filter(|v| !v.is_empty() && v.len() <= 1024)
        .ok_or(GoogleError::PermissionDenied)?;
    let code = fields.remove("code");
    let denied = fields.remove("error");
    if code.is_some() == denied.is_some()
        || code
            .as_ref()
            .is_some_and(|v| v.is_empty() || v.len() > 4096 || v.chars().any(char::is_control))
    {
        return Err(GoogleError::PermissionDenied);
    }
    Ok(Callback { state, code })
}

/// The static, script-free page the browser shows after the redirect.
fn callback_page(success: bool) -> String {
    let (mark, heading, tone, glow) = if success {
        (
            r#"<svg viewBox="0 0 28 28" aria-hidden="true"><circle cx="14" cy="14" r="12.25"/><path d="M8.5 14.4 12 17.8 19.6 10.2"/></svg>"#,
            "Authorization received.",
            "#C9A36B",
            "rgba(201,163,107,.09)",
        )
    } else {
        (
            r#"<svg viewBox="0 0 28 28" aria-hidden="true"><circle cx="14" cy="14" r="12.25"/><path d="M10 10 18 18M18 10 10 18"/></svg>"#,
            "Authorization was not accepted.",
            "#E0645A",
            "rgba(224,100,90,.10)",
        )
    };
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="color-scheme" content="dark">
<meta name="robots" content="noindex">
<title>fuckMail</title>
<style>
html,body{{height:100%;margin:0}}
body{{
  background:
    radial-gradient(42rem 26rem at 50% 38%,{glow},transparent 68%),
    #0B0C0E;
  color:#E9EAEC;
  font-family:Geist,ui-sans-serif,system-ui,"Segoe UI",sans-serif;
  display:grid;
  place-items:center;
}}
main{{width:min(24rem,calc(100vw - 3.5rem))}}
svg{{
  width:1.75rem;height:1.75rem;display:block;margin:0 0 1.6rem;
  fill:none;stroke:{tone};stroke-width:1.25;stroke-linecap:round;stroke-linejoin:round;
}}
.word{{
  margin:0 0 .7rem;font-size:.68rem;letter-spacing:.24em;text-transform:uppercase;color:{tone};
}}
h1{{margin:0;font-weight:500;font-size:1.85rem;letter-spacing:-.035em;line-height:1.2}}
p{{margin:.9rem 0 0;max-width:22rem;color:rgba(233,234,236,.55);font-size:.95rem;line-height:1.45}}
</style>
</head>
<body>
<main>
{mark}
<p class="word">fuckMail</p>
<h1>{heading}</h1>
<p>Return to fuckMail.</p>
</main>
</body>
</html>
"#
    )
}

/// Answers the browser. The headers forbid caching, scripts and a `Referer`,
/// since the page's own URL still holds the authorization code. The write is
/// capped at 100 ms: the page is a courtesy, not part of the protocol.
async fn reply(stream: &mut TcpStream, success: bool) {
    let status = if success { "200 OK" } else { "400 Bad Request" };
    let body = callback_page(success);
    let response = format!("HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n\r\n{body}", body.len());
    let _ = tokio::time::timeout(
        Duration::from_millis(100),
        stream.write_all(response.as_bytes()),
    )
    .await;
}

/// The client ID and optional client secret of the app's Google Desktop OAuth
/// registration.
///
/// Desktop client configuration is not confidential-client authentication.
/// Google may nevertheless require the provisioned desktop client secret on exchange.
/// The secret is zeroized on drop.
pub struct OAuthClientConfig {
    client_id: String,
    client_secret: Option<Zeroizing<String>>,
}
impl OAuthClientConfig {
    /// Errors with `InvalidResponse` unless the ID is 1–1024 bytes and any
    /// secret 1–4096 bytes, both free of control characters.
    pub fn new(client_id: String, client_secret: Option<String>) -> Result<Self, GoogleError> {
        if client_id.is_empty()
            || client_id.len() > 1024
            || client_id.chars().any(char::is_control)
            || client_secret
                .as_ref()
                .is_some_and(|v| v.is_empty() || v.len() > 4096 || v.chars().any(char::is_control))
        {
            return Err(GoogleError::InvalidResponse(
                "invalid desktop OAuth configuration",
            ));
        }
        Ok(Self {
            client_id,
            client_secret: client_secret.map(Zeroizing::new),
        })
    }
    pub fn client_id(&self) -> &str {
        &self.client_id
    }
}

/// The Google account identity taken from a verified ID token.
///
/// Provider identity is private account metadata; callers must not log it.
#[derive(Clone, PartialEq, Eq)]
pub struct GoogleIdentity {
    /// Google's stable account ID. This, not the email, identifies the account.
    pub sub: String,
    /// The account's email at sign-in; it can change over time.
    pub email: String,
    /// Always `true`: tokens with an unverified email are rejected.
    pub email_verified: bool,
    /// The Workspace domain (`hd` claim); `None` for consumer accounts.
    pub hosted_domain: Option<String>,
}

/// A successful code exchange: the verified identity, the granted scopes and
/// the tokens, which stay private until persisted.
pub struct VerifiedGrant {
    pub identity: GoogleIdentity,
    /// Scopes Google reports as granted, with `userinfo.email` reported as
    /// `email`. Always contains `openid` and `email`.
    pub granted_scopes: BTreeSet<String>,
    token: StoredToken,
}
impl VerifiedGrant {
    pub(crate) async fn persist_guarded<V: CredentialVault + 'static>(
        self,
        account: &AccountId,
        vault: Arc<V>,
        guard: tokio::sync::OwnedMutexGuard<()>,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, GoogleError> {
        let account = account.clone();
        tokio::task::spawn_blocking(move || {
            vault.store(&account, self.token)?;
            Ok(guard)
        })
        .await
        .map_err(|_| GoogleError::StoreUnavailable)?
    }

    /// Stores the grant's tokens for `account` on the blocking pool.
    ///
    /// Core validates reconnect subject and maps identity before requesting persistence.
    /// Unlike [`crate::auth::CredentialAccountGuard::persist`], this does not
    /// take the account's credential lock.
    pub async fn persist<V: CredentialVault + 'static>(
        self,
        account: &AccountId,
        vault: Arc<V>,
    ) -> Result<(), GoogleError> {
        let account = account.clone();
        tokio::task::spawn_blocking(move || vault.store(&account, self.token))
            .await
            .map_err(|_| GoogleError::StoreUnavailable)?
    }
}

/// Google's token endpoint and ID-token verification for one OAuth client.
///
/// It redeems sign-in codes and, as an [`AsyncRefreshTransport`], refreshes
/// tokens for accounts bound with [`GoogleOAuth::bind_account`].
pub struct GoogleOAuth<T: HttpTransport> {
    config: OAuthClientConfig,
    http: Arc<T>,
    subjects: Mutex<BTreeMap<AccountId, String>>,
}
impl<T: HttpTransport> GoogleOAuth<T> {
    pub fn new(config: OAuthClientConfig, http: Arc<T>) -> Self {
        Self {
            config,
            http,
            subjects: Mutex::new(BTreeMap::new()),
        }
    }
    pub fn client_id(&self) -> &str {
        self.config.client_id()
    }

    /// Records the Google subject that refreshes for `account` must match.
    ///
    /// Bind only Core's stored subject or a verified grant; never derive it from email.
    /// Rebinding an account to a different subject is `PermissionDenied`.
    pub fn bind_account(&self, account: AccountId, sub: String) -> Result<(), GoogleError> {
        if !valid_subject(&sub) {
            return Err(GoogleError::InvalidResponse("invalid Google subject"));
        }
        let mut subjects = self
            .subjects
            .lock()
            .map_err(|_| GoogleError::StoreUnavailable)?;
        if subjects.get(&account).is_some_and(|old| old != &sub) {
            return Err(GoogleError::PermissionDenied);
        }
        subjects.insert(account, sub);
        Ok(())
    }

    /// Redeems an authorization code with its PKCE verifier and verifies the
    /// returned ID token against Google's current signing keys.
    ///
    /// `now_secs` is the current Unix time. Errors: `PermissionDenied` if the
    /// code came from another client, `openid` or `email` was not granted, or
    /// the ID token fails verification; `ReauthorizationRequired` if Google
    /// rejects the code or returns no refresh token; `TransportInterrupted` if
    /// no response arrives.
    pub async fn exchange(
        &self,
        code: AuthorizationCode,
        now_secs: u64,
        cancel: &CancellationToken,
    ) -> Result<VerifiedGrant, GoogleError> {
        if code.client_id != self.config.client_id {
            return Err(GoogleError::PermissionDenied);
        }
        let fields = [
            ("grant_type", "authorization_code"),
            ("code", code.code.as_str()),
            ("redirect_uri", code.redirect_uri.as_str()),
            ("code_verifier", code.verifier.secret()),
        ];
        let response = self.token_request(&fields, cancel).await?;
        let scopes = parse_scopes(
            response
                .scope
                .as_deref()
                .ok_or(GoogleError::InvalidResponse("token grant omitted scopes"))?,
        )?;
        if !scopes.contains("openid") || !scopes.contains("email") {
            return Err(GoogleError::PermissionDenied);
        }
        let identity = self
            .verify_identity(
                response
                    .id_token
                    .as_deref()
                    .ok_or(GoogleError::InvalidResponse("token grant omitted identity"))?,
                Some(&code.nonce),
                None,
                &response.access_token,
                now_secs,
                cancel,
            )
            .await?;
        // A grant without a refresh token could not outlive its access token,
        // so it counts as a failed sign-in rather than a partial success.
        let token = StoredToken {
            expires_at_secs: token_expiry(&response, now_secs)?,
            access_token: response.access_token,
            refresh_token: response
                .refresh_token
                .filter(|v| !v.is_empty())
                .ok_or(GoogleError::ReauthorizationRequired)?,
        };
        Ok(VerifiedGrant {
            identity,
            granted_scopes: scopes,
            token,
        })
    }

    async fn token_request(
        &self,
        fields: &[(&str, &str)],
        cancel: &CancellationToken,
    ) -> Result<TokenResponse, GoogleError> {
        let encoded = {
            let mut form = form_urlencoded::Serializer::new(String::new());
            form.append_pair("client_id", &self.config.client_id);
            if let Some(secret) = &self.config.client_secret {
                form.append_pair("client_secret", secret);
            }
            for (key, value) in fields {
                form.append_pair(key, value);
            }
            form.finish()
        };
        let request = HttpRequest::new(HttpMethod::Post, TOKEN_URL)
            .and_then(|v| v.body(encoded.into_bytes(), "application/x-www-form-urlencoded"))
            .map(|v| v.response_limit(64 * 1024))
            .map_err(|_| GoogleError::InvalidResponse("invalid token request"))?;
        let response = self
            .http
            .execute(request, cancel)
            .await
            .map_err(|_| GoogleError::TransportInterrupted)?;
        let bytes = Zeroizing::new(response.body);
        if response.status != 200 {
            // The token endpoint reports a revoked, expired or reused grant
            // (`invalid_grant`) as 400 and a client problem as 401. Neither
            // improves with a retry; both need a new sign-in.
            if response.status == 400 || response.status == 401 {
                return Err(GoogleError::ReauthorizationRequired);
            }
            return Err(crate::classify_api_error(
                response.status,
                &bytes,
                response.retry_after_secs,
                false,
            ));
        }
        let response: TokenResponse = crate::decode_json(&bytes, 64 * 1024)?;
        if response.access_token.is_empty()
            || response.access_token.len() > 8192
            || !response.token_type.eq_ignore_ascii_case("Bearer")
            || response
                .refresh_token
                .as_ref()
                .is_some_and(|v| v.len() > 8192)
            || response
                .id_token
                .as_ref()
                .is_some_and(|v| v.len() > 16 * 1024)
        {
            return Err(GoogleError::InvalidResponse("invalid token grant"));
        }
        Ok(response)
    }

    async fn verify_identity(
        &self,
        id_token: &str,
        nonce: Option<&CsrfToken>,
        expected_sub: Option<&str>,
        access_token: &str,
        now_secs: u64,
        cancel: &CancellationToken,
    ) -> Result<GoogleIdentity, GoogleError> {
        // Fetch only Google's fixed key endpoint, never JWT-controlled jku/x5u URLs.
        // Login frequency is low; no cache is safer than inventing a cache lifetime.
        let request = HttpRequest::new(HttpMethod::Get, JWKS_URL)
            .map(|v| v.response_limit(256 * 1024))
            .map_err(|_| GoogleError::InvalidResponse("invalid identity key request"))?;
        let response = self
            .http
            .execute(request, cancel)
            .await
            .map_err(|_| GoogleError::TransportInterrupted)?;
        if response.status != 200 {
            return Err(crate::classify_api_error(
                response.status,
                &response.body,
                response.retry_after_secs,
                false,
            ));
        }
        verify_id_token(
            id_token,
            &response.body,
            &self.config.client_id,
            nonce,
            expected_sub,
            access_token,
            now_secs,
        )
    }
}

impl<T: HttpTransport> AsyncRefreshTransport for GoogleOAuth<T> {
    fn refresh_async<'a>(
        &'a self,
        account: &'a AccountId,
        refresh_token: &'a str,
        now_secs: u64,
        cancel: &'a CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<RefreshResult, GoogleError>> + Send + 'a>> {
        Box::pin(async move {
            // Refuse to refresh for an account whose Google subject is unknown.
            // If Google includes an ID token, it must name that subject.
            let expected_sub = self
                .subjects
                .lock()
                .map_err(|_| GoogleError::StoreUnavailable)?
                .get(account)
                .cloned()
                .ok_or(GoogleError::PermissionDenied)?;
            let response = self
                .token_request(
                    &[
                        ("grant_type", "refresh_token"),
                        ("refresh_token", refresh_token),
                    ],
                    cancel,
                )
                .await?;
            if let Some(token) = &response.id_token {
                self.verify_identity(
                    token,
                    None,
                    Some(&expected_sub),
                    &response.access_token,
                    now_secs,
                    cancel,
                )
                .await?;
            }
            Ok(RefreshResult {
                expires_at_secs: token_expiry(&response, now_secs)?,
                access_token: response.access_token,
                rotated_refresh_token: response.refresh_token,
            })
        })
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
    refresh_token: Option<String>,
    id_token: Option<String>,
    scope: Option<String>,
}
fn token_expiry(response: &TokenResponse, now: u64) -> Result<u64, GoogleError> {
    // Google access tokens normally last about an hour. A lifetime at or
    // under the 60-second refresh margin, or over a day, is treated as malformed.
    if !(61..=86400).contains(&response.expires_in) {
        return Err(GoogleError::InvalidResponse("invalid token lifetime"));
    }
    now.checked_add(response.expires_in)
        .ok_or(GoogleError::InvalidResponse("invalid token lifetime"))
}
fn parse_scopes(value: &str) -> Result<BTreeSet<String>, GoogleError> {
    if value.len() > 8192 || value.chars().any(char::is_control) {
        return Err(GoogleError::InvalidResponse("invalid token scopes"));
    }
    // Google may report the requested `email` scope by its long name.
    Ok(value
        .split_whitespace()
        .map(|scope| {
            if scope == "https://www.googleapis.com/auth/userinfo.email" {
                "email"
            } else {
                scope
            }
            .to_owned()
        })
        .collect())
}
fn valid_subject(sub: &str) -> bool {
    !sub.is_empty() && sub.len() <= 255 && !sub.chars().any(char::is_control)
}

#[derive(Deserialize)]
struct KeySet {
    keys: Vec<RsaJwk>,
}
#[derive(Deserialize)]
struct RsaJwk {
    kty: String,
    kid: String,
    n: String,
    e: String,
    alg: Option<String>,
    #[serde(rename = "use")]
    usage: Option<String>,
}
#[derive(Deserialize)]
struct Claims {
    sub: String,
    exp: u64,
    iat: u64,
    aud: serde_json::Value,
    azp: Option<String>,
    nonce: Option<String>,
    email: String,
    email_verified: bool,
    hd: Option<String>,
    at_hash: Option<String>,
}
fn verify_id_token(
    token: &str,
    jwks: &[u8],
    client_id: &str,
    nonce: Option<&CsrfToken>,
    expected_sub: Option<&str>,
    access_token: &str,
    now: u64,
) -> Result<GoogleIdentity, GoogleError> {
    if token.len() > 16 * 1024 {
        return Err(GoogleError::PermissionDenied);
    }
    // Only RS256 with a `kid` naming exactly one Google key: the token cannot
    // choose a weaker algorithm or an ambiguous key.
    let header = decode_header(token).map_err(|_| GoogleError::PermissionDenied)?;
    if header.alg != Algorithm::RS256 {
        return Err(GoogleError::PermissionDenied);
    }
    let kid = header.kid.ok_or(GoogleError::PermissionDenied)?;
    let keys: KeySet = crate::decode_json(jwks, 256 * 1024)?;
    if keys.keys.len() > 32 {
        return Err(GoogleError::PermissionDenied);
    }
    let matching: Vec<_> = keys.keys.iter().filter(|v| v.kid == kid).collect();
    if matching.len() != 1 {
        return Err(GoogleError::PermissionDenied);
    }
    let key = matching[0];
    if key.kty != "RSA"
        || key.alg.as_deref().is_some_and(|v| v != "RS256")
        || key.usage.as_deref().is_some_and(|v| v != "sig")
    {
        return Err(GoogleError::PermissionDenied);
    }
    let decoding = DecodingKey::from_rsa_components(&key.n, &key.e)
        .map_err(|_| GoogleError::PermissionDenied)?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[client_id]);
    validation.set_issuer(&["accounts.google.com", "https://accounts.google.com"]);
    validation.set_required_spec_claims(&["iss", "aud", "exp", "sub"]);
    validation.validate_exp = false; // deterministic caller clock checked below, without expiry grace.
    let claims = decode::<Claims>(token, &decoding, &validation)
        .map_err(|_| GoogleError::PermissionDenied)?
        .claims;
    // `iat` may run up to 60 s ahead for clock skew. With several audiences,
    // `azp` must name this client, as OpenID Connect requires.
    if claims.exp <= now
        || claims.iat > now.saturating_add(60)
        || claims.iat >= claims.exp
        || !valid_subject(&claims.sub)
        || expected_sub.is_some_and(|v| v != claims.sub)
        || !claims.email_verified
        || claims.email.is_empty()
        || claims.email.len() > 320
        || claims.email.chars().any(char::is_control)
        || claims.azp.as_deref().is_some_and(|v| v != client_id)
        || (claims.aud.as_array().is_some_and(|v| v.len() > 1)
            && claims.azp.as_deref() != Some(client_id))
    {
        return Err(GoogleError::PermissionDenied);
    }
    if let Some(expected) = nonce {
        if !claims
            .nonce
            .as_ref()
            .is_some_and(|v| CsrfToken::new(v.clone()) == *expected)
        {
            return Err(GoogleError::PermissionDenied);
        }
    }
    // `at_hash` ties the ID token to the access token issued with it: the
    // left half of its SHA-256, base64url-encoded.
    if let Some(hash) = claims.at_hash {
        let digest = Sha256::digest(access_token.as_bytes());
        if CsrfToken::new(hash) != CsrfToken::new(URL_SAFE_NO_PAD.encode(&digest[..16])) {
            return Err(GoogleError::PermissionDenied);
        }
    }
    Ok(GoogleIdentity {
        sub: claims.sub,
        email: claims.email,
        email_verified: claims.email_verified,
        hosted_domain: claims.hd,
    })
}

#[cfg(test)]
mod tests;
