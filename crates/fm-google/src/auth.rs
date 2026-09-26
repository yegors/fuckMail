//! OAuth scopes, desktop authorization requests and access-token refresh.
//! [`TokenCoordinator`] serializes refreshes per account and keeps tokens in a
//! [`CredentialVault`].
use crate::transport::CancellationToken;
use crate::{AccountId, GoogleError};
use oauth2::{
    basic::BasicClient, AuthUrl, ClientId, CsrfToken, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, Scope, TokenUrl,
};
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

/// The scopes a default sign-in requests: OpenID identity, full Gmail access,
/// Calendar events, the calendar list and free/busy. Equal to
/// `RequestedAccess::default().scopes()`.
pub const GOOGLE_SCOPES: [&str; 6] = [
    "openid",
    "email",
    "https://mail.google.com/",
    "https://www.googleapis.com/auth/calendar.events",
    "https://www.googleapis.com/auth/calendar.calendarlist.readonly",
    "https://www.googleapis.com/auth/calendar.events.freebusy",
];

/// How much access to request for one product.
#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestedMode {
    /// Not requested.
    None,
    /// Read-only scopes.
    Read,
    /// Read-write scopes; for mail, full `https://mail.google.com/` access.
    #[default]
    Write,
}
/// The access a sign-in asks for, per product.
///
/// Deserializes from `{"mail": ..., "calendar": ...}` with `none`, `read` or
/// `write`; a missing field means `write` and unknown fields are refused.
#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestedAccess {
    #[serde(default)]
    pub mail: RequestedMode,
    #[serde(default)]
    pub calendar: RequestedMode,
}
impl RequestedAccess {
    /// Returns the OAuth scopes to request.
    ///
    /// `openid` and `email` are always included, and any Calendar access also
    /// asks for the calendar list and free/busy. Errors when both products are
    /// `None`.
    pub fn scopes(self) -> Result<Vec<&'static str>, GoogleError> {
        use RequestedMode::*;
        let mut scopes = vec!["openid", "email"];
        match self.mail {
            None => {}
            Read => scopes.push("https://www.googleapis.com/auth/gmail.readonly"),
            // Full mail access: deleting for good (Spam and Trash) needs it.
            Write => scopes.push("https://mail.google.com/"),
        }
        match self.calendar {
            None => {}
            mode => {
                scopes.push(if matches!(mode, Read) {
                    "https://www.googleapis.com/auth/calendar.events.readonly"
                } else {
                    "https://www.googleapis.com/auth/calendar.events"
                });
                scopes.push("https://www.googleapis.com/auth/calendar.calendarlist.readonly");
                scopes.push("https://www.googleapis.com/auth/calendar.events.freebusy");
            }
        }
        if scopes.len() == 2 {
            return Err(GoogleError::InvalidResponse(
                "Choose Mail or Calendar access",
            ));
        }
        Ok(scopes)
    }
}

/// Shared interpretation of granted scopes for cached and live mail workflows.
///
/// Works on the scopes Google actually granted, which can be narrower than
/// those requested because people may decline some on the consent screen.
/// Broader scopes imply narrower ones, such as `calendar` implying event reads.
pub struct ScopeSet<'a>(&'a BTreeSet<String>);
impl<'a> ScopeSet<'a> {
    pub fn new(scopes: &'a BTreeSet<String>) -> Self {
        Self(scopes)
    }
    /// Whether messages can be read.
    pub fn mail_read(&self) -> bool {
        self.mail_write()
            || self
                .0
                .contains("https://www.googleapis.com/auth/gmail.readonly")
    }
    /// Whether mail can be changed and sent (`gmail.modify` or full access).
    pub fn mail_write(&self) -> bool {
        self.0
            .contains("https://www.googleapis.com/auth/gmail.modify")
            || self.0.contains("https://mail.google.com/")
    }
    /// Deleting mail for good; `gmail.modify` alone cannot.
    pub fn mail_full(&self) -> bool {
        self.0.contains("https://mail.google.com/")
    }
    /// Whether the calendar list can be read.
    pub fn calendar_catalog_read(&self) -> bool {
        self.calendar_all_read()
            || self
                .0
                .contains("https://www.googleapis.com/auth/calendar.calendarlist")
            || self
                .0
                .contains("https://www.googleapis.com/auth/calendar.calendarlist.readonly")
    }
    /// Whether event details can be read.
    pub fn calendar_events_read(&self) -> bool {
        self.calendar_all_read()
            || self
                .0
                .contains("https://www.googleapis.com/auth/calendar.events")
            || self
                .0
                .contains("https://www.googleapis.com/auth/calendar.events.readonly")
    }
    /// Whether free/busy can be queried.
    pub fn calendar_free_busy(&self) -> bool {
        self.calendar_all_read()
            || self
                .0
                .contains("https://www.googleapis.com/auth/calendar.events.freebusy")
            || self
                .0
                .contains("https://www.googleapis.com/auth/calendar.freebusy")
    }
    /// Whether events can be created, changed and deleted.
    pub fn calendar_events_write(&self) -> bool {
        self.0.contains("https://www.googleapis.com/auth/calendar")
            || self
                .0
                .contains("https://www.googleapis.com/auth/calendar.events")
    }
    fn calendar_all_read(&self) -> bool {
        self.0.contains("https://www.googleapis.com/auth/calendar")
            || self
                .0
                .contains("https://www.googleapis.com/auth/calendar.readonly")
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;

    #[test]
    fn requested_access_changes_consent_scopes_without_overstating_grants() {
        assert_eq!(RequestedAccess::default().scopes().unwrap(), GOOGLE_SCOPES);
        for mail in [
            RequestedMode::None,
            RequestedMode::Read,
            RequestedMode::Write,
        ] {
            for calendar in [
                RequestedMode::None,
                RequestedMode::Read,
                RequestedMode::Write,
            ] {
                let access = RequestedAccess { mail, calendar };
                if matches!(mail, RequestedMode::None) && matches!(calendar, RequestedMode::None) {
                    assert!(access.scopes().is_err());
                    continue;
                }
                let login = DesktopAuthorization::with_access(
                    "synthetic-client".into(),
                    42001,
                    false,
                    Some("human@example.test"),
                    access,
                )
                .unwrap();
                let url = reqwest::Url::parse(&login.url).unwrap();
                let parameters: BTreeMap<_, _> = url.query_pairs().into_owned().collect();
                let requested: BTreeSet<String> = parameters["scope"]
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect();
                let scopes = ScopeSet::new(&requested);
                assert_eq!(scopes.mail_read(), !matches!(mail, RequestedMode::None));
                assert_eq!(scopes.mail_write(), matches!(mail, RequestedMode::Write));
                assert_eq!(
                    scopes.calendar_events_read(),
                    !matches!(calendar, RequestedMode::None)
                );
                assert_eq!(
                    scopes.calendar_events_write(),
                    matches!(calendar, RequestedMode::Write)
                );
                assert_eq!(parameters["code_challenge_method"], "S256");
                assert_eq!(parameters["login_hint"], "human@example.test");
            }
        }
        assert!(
            serde_json::from_value::<RequestedAccess>(serde_json::json!({"mail":"all"})).is_err()
        );
        assert!(
            serde_json::from_value::<RequestedAccess>(serde_json::json!({"other":"write"}))
                .is_err()
        );
    }

    #[test]
    fn calendar_grants_distinguish_catalog_details_busy_and_writes() {
        for (scope, expected) in [
            ("calendar", [true, true, true, true]),
            ("calendar.readonly", [true, true, true, false]),
            ("calendar.calendarlist", [true, false, false, false]),
            (
                "calendar.calendarlist.readonly",
                [true, false, false, false],
            ),
            ("calendar.events", [false, true, false, true]),
            ("calendar.events.readonly", [false, true, false, false]),
            ("calendar.events.freebusy", [false, false, true, false]),
            ("calendar.freebusy", [false, false, true, false]),
            ("calendar.events.owned", [false; 4]),
            ("calendar.events.public.readonly", [false; 4]),
            ("gmail.readonly", [false; 4]),
        ] {
            let granted = BTreeSet::from([format!("https://www.googleapis.com/auth/{scope}")]);
            let scopes = ScopeSet::new(&granted);
            assert_eq!(
                [
                    scopes.calendar_catalog_read(),
                    scopes.calendar_events_read(),
                    scopes.calendar_free_busy(),
                    scopes.calendar_events_write()
                ],
                expected,
                "{scope}"
            );
        }
    }

    #[test]
    fn granted_mail_scopes_distinguish_read_access_from_mutation_access() {
        for (scope, read, write) in [
            (
                "https://www.googleapis.com/auth/gmail.readonly",
                true,
                false,
            ),
            ("https://www.googleapis.com/auth/gmail.modify", true, true),
            ("https://mail.google.com/", true, true),
            ("https://www.googleapis.com/auth/gmail.labels", false, false),
            (
                "https://www.googleapis.com/auth/calendar.events",
                false,
                false,
            ),
        ] {
            let granted = BTreeSet::from([scope.to_owned()]);
            let scopes = ScopeSet::new(&granted);
            assert_eq!(scopes.mail_read(), read, "{scope}");
            assert_eq!(scopes.mail_write(), write, "{scope}");
            assert_eq!(
                scopes.mail_full(),
                scope == "https://mail.google.com/",
                "{scope}"
            );
        }
        assert!(!ScopeSet::new(&BTreeSet::new()).mail_read());
    }
}

/// One OAuth authorization-code request for a loopback redirect, with PKCE
/// (S256), a CSRF `state` and an OpenID `nonce`.
///
/// Held in the engine only for one system-browser attempt. Deliberately no Debug.
pub struct DesktopAuthorization {
    /// The Google consent URL to open in the system browser. It carries the
    /// state and code challenge; never log it.
    pub url: String,
    /// `http://127.0.0.1:<port>/oauth2/callback`; the token exchange must send
    /// exactly this value.
    pub redirect_uri: String,
    csrf: CsrfToken,
    verifier: PkceCodeVerifier,
    nonce: CsrfToken,
    client_id: String,
}

impl DesktopAuthorization {
    /// Port must come from a successfully bound ephemeral IPv4 loopback listener;
    /// `desktop_auth` owns the listener, the callback and the token exchange.
    pub fn new(client_id: String, loopback_port: u16) -> Result<Self, GoogleError> {
        Self::with_login_hint(client_id, loopback_port, false, None)
    }

    /// The hint chooses a Google sign-in screen; verified `sub` still owns identity.
    pub fn with_login_hint(
        client_id: String,
        loopback_port: u16,
        reconsent: bool,
        login_hint: Option<&str>,
    ) -> Result<Self, GoogleError> {
        Self::with_access(
            client_id,
            loopback_port,
            reconsent,
            login_hint,
            RequestedAccess::default(),
        )
    }

    /// Builds the consent URL for `access`.
    ///
    /// It asks for offline access (a refresh token) and always shows the
    /// account chooser; `reconsent` also forces the consent screen. Errors
    /// with `InvalidResponse` for no requested product, an invalid login hint,
    /// an empty, oversized or control-character client ID, or port 0.
    pub fn with_access(
        client_id: String,
        loopback_port: u16,
        reconsent: bool,
        login_hint: Option<&str>,
        access: RequestedAccess,
    ) -> Result<Self, GoogleError> {
        let scopes = access.scopes()?;
        if let Some(email) = login_hint {
            validate_login_hint(email)?;
        }
        if client_id.is_empty()
            || client_id.len() > 1024
            || client_id.chars().any(char::is_control)
            || loopback_port == 0
        {
            return Err(GoogleError::InvalidResponse(
                "OAuth client or callback unavailable",
            ));
        }
        let redirect_uri = format!("http://127.0.0.1:{loopback_port}/oauth2/callback");
        let client = BasicClient::new(ClientId::new(client_id.clone()))
            .set_auth_uri(
                AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_owned()).unwrap(),
            )
            .set_token_uri(TokenUrl::new("https://oauth2.googleapis.com/token".to_owned()).unwrap())
            .set_redirect_uri(
                RedirectUrl::new(redirect_uri.clone())
                    .map_err(|_| GoogleError::InvalidResponse("OAuth redirect invalid"))?,
            );
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let request = scopes.iter().fold(
            client.authorize_url(CsrfToken::new_random),
            |request, scope| request.add_scope(Scope::new((*scope).to_owned())),
        );
        // The nonce comes back inside the ID token, binding that token to this
        // attempt. `select_account` makes the person pick the Google account
        // every time; `consent` re-shows the grant, which is how a lost
        // refresh token is reissued.
        let nonce = CsrfToken::new_random();
        let request = request
            .set_pkce_challenge(challenge)
            .add_extra_param("access_type", "offline")
            .add_extra_param(
                "prompt",
                if reconsent {
                    "consent select_account"
                } else {
                    "select_account"
                },
            )
            .add_extra_param("nonce", nonce.secret());
        let request = if let Some(email) = login_hint {
            request.add_extra_param("login_hint", email)
        } else {
            request
        };
        let (url, csrf) = request.url();
        Ok(Self {
            url: url.to_string(),
            redirect_uri,
            csrf,
            verifier,
            nonce,
            client_id,
        })
    }

    /// Consume the attempt so a callback cannot reuse its PKCE verifier.
    ///
    /// Errors with `PermissionDenied` if `state` is not this attempt's or the
    /// code is empty or over 4096 bytes.
    pub fn accept_callback(
        self,
        state: &str,
        code: String,
    ) -> Result<AuthorizationCode, GoogleError> {
        // CsrfToken uses constant-time equality through the upstream secret wrapper.
        if CsrfToken::new(state.to_owned()) != self.csrf || code.is_empty() || code.len() > 4096 {
            return Err(GoogleError::PermissionDenied);
        }
        Ok(AuthorizationCode {
            code,
            verifier: self.verifier,
            redirect_uri: self.redirect_uri,
            nonce: self.nonce,
            client_id: self.client_id,
        })
    }

    pub(crate) fn matches_state(&self, state: &str) -> bool {
        state.len() <= 1024 && CsrfToken::new(state.to_owned()) == self.csrf
    }
}

/// A bounded, bare email hint, never an identity or Workspace-domain restriction.
pub fn validate_login_hint(email: &str) -> Result<(), GoogleError> {
    let valid = email.len() <= 320
        && !email.chars().any(|c| c.is_whitespace() || c.is_control())
        && !email.contains(['<', '>', ',', ';'])
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && !domain.is_empty() && !domain.contains('@')
        });
    if valid {
        Ok(())
    } else {
        Err(GoogleError::InvalidResponse("Enter a Google email address"))
    }
}

/// An authorization code from a verified callback, bound to the PKCE
/// verifier, redirect URI, nonce and client ID of the attempt that produced it.
///
/// Redeem it once with [`crate::desktop_auth::GoogleOAuth::exchange`]; Google
/// codes are single-use and short-lived. No `Debug`.
pub struct AuthorizationCode {
    pub code: String,
    pub verifier: PkceCodeVerifier,
    pub redirect_uri: String,
    pub(crate) nonce: CsrfToken,
    pub(crate) client_id: String,
}

/// The credentials kept per account in a [`CredentialVault`]. No `Debug`.
#[derive(Clone)]
pub struct StoredToken {
    /// Bearer token for API calls, valid until `expires_at_secs`.
    pub access_token: String,
    /// Long-lived token for new access tokens. It changes only when Google
    /// rotates it or the person signs in again.
    pub refresh_token: String,
    /// Access-token expiry in Unix seconds, computed from the grant's
    /// `expires_in` when it was received.
    pub expires_at_secs: u64,
}

/// The outcome of one refresh-token grant.
pub struct RefreshResult {
    pub access_token: String,
    /// A replacement refresh token, if Google issued one; otherwise the stored
    /// refresh token stays in use.
    pub rotated_refresh_token: Option<String>,
    /// Access-token expiry in Unix seconds.
    pub expires_at_secs: u64,
}

/// Per-account storage for [`StoredToken`]s, normally the OS credential store.
///
/// Implementations must keep one entry per account, replace the whole entry on
/// `store`, and never fall back to another store. Calls may block (for
/// example on a keychain prompt), so async callers run them on the blocking
/// pool. [`TokenCoordinator`] serializes its own operations per account, but
/// different accounts can call concurrently.
pub trait CredentialVault: Send + Sync {
    /// Clears a remembered access denial for `account`; the default does
    /// nothing (see [`crate::vault::SessionCredentialVault`]).
    ///
    /// Explicit user retry only; ordinary reads must never clear a denial.
    fn retry_access(&self, _: &AccountId) -> Result<(), GoogleError> {
        Ok(())
    }
    /// Returns the account's credentials, or `Ok(None)` when no entry exists.
    ///
    /// Locked/unavailable storage returns StoreUnavailable, not Ok(None).
    /// The coordinator propagates that failure without another credential source.
    fn load(&self, account: &AccountId) -> Result<Option<StoredToken>, GoogleError>;
    /// Saves `token` for `account`, replacing any previous entry.
    fn store(&self, account: &AccountId, token: StoredToken) -> Result<(), GoogleError>;
    /// Explicit local cleanup. Unsupported vaults fail rather than pretend that
    /// credentials were removed. An already absent exact entry is success.
    fn remove(&self, _: &AccountId) -> Result<(), GoogleError> {
        Err(GoogleError::StoreUnavailable)
    }
}

/// Blocking refresh-token grant used by [`TokenCoordinator::access_token`].
/// Implementations must not retry.
pub trait RefreshTransport: Send + Sync {
    fn refresh(
        &self,
        account: &AccountId,
        refresh_token: &str,
    ) -> Result<RefreshResult, GoogleError>;
}

/// Exchanges a refresh token for a new access token.
///
/// Production transport path. The future must honor the caller's cancellation.
/// It must not retry. `now_secs` is the current Unix time, used to compute the
/// new expiry and to check any ID token. The production implementation is
/// [`crate::desktop_auth::GoogleOAuth`].
pub trait AsyncRefreshTransport: Send + Sync {
    fn refresh_async<'a>(
        &'a self,
        account: &'a AccountId,
        refresh_token: &'a str,
        now_secs: u64,
        cancel: &'a CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<RefreshResult, GoogleError>> + Send + 'a>>;
}

/// Hands out access tokens, refreshing any that expire within 60 seconds.
///
/// One short-lived refresh lock per account, shared by Gmail and Calendar in one engine.
/// Secrets remain with the vault/transport; no token type derives Debug/Serialize.
/// The same lock also guards sign-in persistence and credential removal (see
/// [`CredentialAccountGuard`]), so none of them can interleave with a refresh.
#[derive(Default)]
pub struct TokenCoordinator {
    accounts: Mutex<BTreeMap<AccountId, Arc<tokio::sync::Mutex<()>>>>,
}

/// Engine lifecycle admission is rechecked while this exact-account lock is
/// held. Native finalization takes ownership so cancellation cannot release it
/// while a queued or running credential operation can still change the vault.
pub struct CredentialAccountGuard {
    account: AccountId,
    guard: tokio::sync::OwnedMutexGuard<()>,
}
impl CredentialAccountGuard {
    /// Removes the account's credentials from `vault` on the blocking pool.
    /// The lock is released only when the native call returns, even if this
    /// future is dropped first.
    pub async fn remove<V: CredentialVault + 'static>(
        self,
        vault: Arc<V>,
    ) -> Result<(), GoogleError> {
        tokio::task::spawn_blocking(move || {
            let result = vault.remove(&self.account);
            drop(self.guard);
            result
        })
        .await
        .map_err(|_| GoogleError::StoreUnavailable)?
    }

    /// Stores a verified grant's tokens for this account, then releases the lock.
    pub async fn persist<V: CredentialVault + 'static>(
        self,
        grant: crate::desktop_auth::VerifiedGrant,
        vault: Arc<V>,
    ) -> Result<(), GoogleError> {
        self.persist_retaining(grant, vault).await.map(drop)
    }

    /// Keep the native account lock through the caller's final metadata commit.
    /// An aborted native store still owns the lock until its actual return.
    pub async fn persist_retaining<V: CredentialVault + 'static>(
        self,
        grant: crate::desktop_auth::VerifiedGrant,
        vault: Arc<V>,
    ) -> Result<Self, GoogleError> {
        let guard = grant
            .persist_guarded(&self.account, vault, self.guard)
            .await?;
        Ok(Self {
            account: self.account,
            guard,
        })
    }
}

impl TokenCoordinator {
    /// Waits for the account's credential lock, the same one refresh uses.
    /// Errors with `TransportInterrupted` if `cancel` fires first.
    pub async fn account_guard(
        &self,
        account: &AccountId,
        cancel: &CancellationToken,
    ) -> Result<CredentialAccountGuard, GoogleError> {
        let guard = {
            let mut accounts = self
                .accounts
                .lock()
                .map_err(|_| GoogleError::StoreUnavailable)?;
            accounts.entry(account.clone()).or_default().clone()
        };
        let guard = tokio::select! {
            _ = cancel.cancelled() => return Err(GoogleError::TransportInterrupted),
            guard = guard.lock_owned() => guard,
        };
        if cancel.is_cancelled() {
            return Err(GoogleError::TransportInterrupted);
        }
        Ok(CredentialAccountGuard {
            account: account.clone(),
            guard,
        })
    }

    /// A newly verified login shares refresh finalization's account mutex. The
    /// native store owns the guard even if its awaiting owner is cancelled.
    pub async fn persist_verified_grant<V: CredentialVault + 'static>(
        &self,
        account: &AccountId,
        grant: crate::desktop_auth::VerifiedGrant,
        vault: Arc<V>,
        cancel: &CancellationToken,
    ) -> Result<(), GoogleError> {
        self.account_guard(account, cancel)
            .await?
            .persist(grant, vault)
            .await
    }

    /// Blocking form of [`TokenCoordinator::access_token_async`] for
    /// synchronous callers; it must not run on an async runtime thread.
    ///
    /// Errors with `ReauthorizationRequired` if the account has no complete
    /// credential, and `InvalidResponse` if a refresh returns an unusable
    /// lifetime. Vault and transport errors pass through.
    pub fn access_token<V: CredentialVault, T: RefreshTransport>(
        &self,
        account: &AccountId,
        now_secs: u64,
        vault: &V,
        transport: &T,
    ) -> Result<String, GoogleError> {
        let guard = {
            let mut accounts = self
                .accounts
                .lock()
                .map_err(|_| GoogleError::StoreUnavailable)?;
            accounts.entry(account.clone()).or_default().clone()
        };
        // The synchronous fixture/worker path must not run on a Tokio worker.
        let _refresh = guard.blocking_lock();
        let token = vault
            .load(account)?
            .ok_or(GoogleError::ReauthorizationRequired)?;
        if token.access_token.is_empty() || token.refresh_token.is_empty() {
            return Err(GoogleError::ReauthorizationRequired);
        }
        if token.expires_at_secs > now_secs.saturating_add(60) {
            return Ok(token.access_token);
        }
        let result = transport.refresh(account, &token.refresh_token)?;
        if result.access_token.is_empty()
            || result.expires_at_secs <= now_secs.saturating_add(60)
            || result
                .rotated_refresh_token
                .as_ref()
                .is_some_and(String::is_empty)
        {
            return Err(GoogleError::InvalidResponse(
                "invalid refreshed token lifetime",
            ));
        }
        let updated = StoredToken {
            access_token: result.access_token,
            expires_at_secs: result.expires_at_secs,
            refresh_token: result.rotated_refresh_token.unwrap_or(token.refresh_token),
        };
        let access = updated.access_token.clone();
        vault.store(account, updated)?;
        Ok(access)
    }

    /// Returns a usable access token for `account`, refreshing and storing a
    /// new one if the current one expires within 60 seconds of `now_secs`.
    ///
    /// Native vault calls run on Tokio's blocking pool; provider waits never block it.
    /// The same account mutex also serializes the legacy synchronous worker path.
    /// Errors with `ReauthorizationRequired` if the account has no complete
    /// credential, `InvalidResponse` if a refresh returns an unusable lifetime,
    /// and `TransportInterrupted` on cancellation. Vault and transport errors
    /// pass through.
    pub async fn access_token_async<
        V: CredentialVault + 'static,
        T: AsyncRefreshTransport + ?Sized,
    >(
        &self,
        account: &AccountId,
        now_secs: u64,
        vault: Arc<V>,
        transport: &T,
        cancel: &CancellationToken,
    ) -> Result<String, GoogleError> {
        let guard = {
            let mut accounts = self
                .accounts
                .lock()
                .map_err(|_| GoogleError::StoreUnavailable)?;
            accounts.entry(account.clone()).or_default().clone()
        };
        let refresh_guard = tokio::select! {
            _ = cancel.cancelled() => return Err(GoogleError::TransportInterrupted),
            guard = guard.lock_owned() => guard,
        };
        let load_vault = vault.clone();
        let load_account = account.clone();
        let token = tokio::task::spawn_blocking(move || load_vault.load(&load_account))
            .await
            .map_err(|_| GoogleError::StoreUnavailable)??
            .ok_or(GoogleError::ReauthorizationRequired)?;
        if cancel.is_cancelled() {
            return Err(GoogleError::TransportInterrupted);
        }
        if token.refresh_token.is_empty() || token.access_token.is_empty() {
            return Err(GoogleError::ReauthorizationRequired);
        }
        // Refresh a minute early so a token cannot expire while a request is
        // in flight. Callers that waited on the lock usually find the token a
        // concurrent caller just refreshed.
        if token.expires_at_secs > now_secs.saturating_add(60) {
            return Ok(token.access_token);
        }
        let result = transport
            .refresh_async(account, &token.refresh_token, now_secs, cancel)
            .await?;
        // A token already inside the refresh margin would be refreshed again
        // on the next call, so treat it as malformed. Google usually keeps the
        // refresh token; the stored one stays unless a new one arrives.
        if result.access_token.is_empty()
            || result.expires_at_secs <= now_secs.saturating_add(60)
            || result
                .rotated_refresh_token
                .as_ref()
                .is_some_and(String::is_empty)
        {
            return Err(GoogleError::InvalidResponse(
                "invalid refreshed token lifetime",
            ));
        }
        let updated = StoredToken {
            access_token: result.access_token,
            expires_at_secs: result.expires_at_secs,
            refresh_token: result.rotated_refresh_token.unwrap_or(token.refresh_token),
        };
        let access = updated.access_token.clone();
        let store_account = account.clone();
        // Finalization owns the account lock, including if this awaiting task is
        // aborted. A queued/running native store must finish before another refresh.
        tokio::task::spawn_blocking(move || {
            let result = vault.store(&store_account, updated);
            drop(refresh_guard);
            result
        })
        .await
        .map_err(|_| GoogleError::StoreUnavailable)??;
        if cancel.is_cancelled() {
            return Err(GoogleError::TransportInterrupted);
        }
        Ok(access)
    }
}

#[cfg(test)]
#[path = "auth_cleanup_tests.rs"]
mod cleanup_tests;
