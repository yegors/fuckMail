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

pub const GOOGLE_SCOPES: [&str; 6] = [
    "openid",
    "email",
    "https://www.googleapis.com/auth/gmail.modify",
    "https://www.googleapis.com/auth/calendar.events",
    "https://www.googleapis.com/auth/calendar.calendarlist.readonly",
    "https://www.googleapis.com/auth/calendar.events.freebusy",
];

/// Shared interpretation of granted scopes for cached and live mail workflows.
pub struct ScopeSet<'a>(&'a BTreeSet<String>);
impl<'a> ScopeSet<'a> {
    pub fn new(scopes: &'a BTreeSet<String>) -> Self {
        Self(scopes)
    }
    pub fn mail_read(&self) -> bool {
        self.mail_write()
            || self
                .0
                .contains("https://www.googleapis.com/auth/gmail.readonly")
    }
    pub fn mail_write(&self) -> bool {
        self.0
            .contains("https://www.googleapis.com/auth/gmail.modify")
            || self.0.contains("https://mail.google.com/")
    }
    pub fn calendar_catalog_read(&self) -> bool {
        self.calendar_all_read()
            || self
                .0
                .contains("https://www.googleapis.com/auth/calendar.calendarlist")
            || self
                .0
                .contains("https://www.googleapis.com/auth/calendar.calendarlist.readonly")
    }
    pub fn calendar_events_read(&self) -> bool {
        self.calendar_all_read()
            || self
                .0
                .contains("https://www.googleapis.com/auth/calendar.events")
            || self
                .0
                .contains("https://www.googleapis.com/auth/calendar.events.readonly")
    }
    pub fn calendar_free_busy(&self) -> bool {
        self.calendar_all_read()
            || self
                .0
                .contains("https://www.googleapis.com/auth/calendar.events.freebusy")
            || self
                .0
                .contains("https://www.googleapis.com/auth/calendar.freebusy")
    }
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
        }
        assert!(!ScopeSet::new(&BTreeSet::new()).mail_read());
    }
}

/// Held in the engine only for one system-browser attempt. Deliberately no Debug.
pub struct DesktopAuthorization {
    pub url: String,
    pub redirect_uri: String,
    csrf: CsrfToken,
    verifier: PkceCodeVerifier,
    nonce: CsrfToken,
    client_id: String,
}

impl DesktopAuthorization {
    /// Port must come from a successfully bound ephemeral IPv4 loopback listener.
    /// Binding, callback timeout and the actual token exchange remain adapter work.
    pub fn new(client_id: String, loopback_port: u16) -> Result<Self, GoogleError> {
        Self::with_login_hint(client_id, loopback_port, false, None)
    }

    /// Explicit recovery after a missing/revoked refresh token; never a refresh retry.
    pub fn new_reauthorization(client_id: String, loopback_port: u16) -> Result<Self, GoogleError> {
        Self::with_login_hint(client_id, loopback_port, true, None)
    }

    /// The hint chooses a Google sign-in screen; verified `sub` still owns identity.
    pub fn with_login_hint(
        client_id: String,
        loopback_port: u16,
        reconsent: bool,
        login_hint: Option<&str>,
    ) -> Result<Self, GoogleError> {
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
        let request = GOOGLE_SCOPES.iter().fold(
            client.authorize_url(CsrfToken::new_random),
            |request, scope| request.add_scope(Scope::new((*scope).to_owned())),
        );
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

pub struct AuthorizationCode {
    pub code: String,
    pub verifier: PkceCodeVerifier,
    pub redirect_uri: String,
    pub(crate) nonce: CsrfToken,
    pub(crate) client_id: String,
}

#[derive(Clone)]
pub struct StoredToken {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at_secs: u64,
}

pub struct RefreshResult {
    pub access_token: String,
    pub rotated_refresh_token: Option<String>,
    pub expires_at_secs: u64,
}

pub trait CredentialVault: Send + Sync {
    /// Locked/unavailable storage returns StoreUnavailable, not Ok(None).
    /// The coordinator propagates that failure without another credential source.
    fn load(&self, account: &AccountId) -> Result<Option<StoredToken>, GoogleError>;
    fn store(&self, account: &AccountId, token: StoredToken) -> Result<(), GoogleError>;
    /// Explicit local cleanup. Unsupported vaults fail rather than pretend that
    /// credentials were removed. An already absent exact entry is success.
    fn remove(&self, _: &AccountId) -> Result<(), GoogleError> {
        Err(GoogleError::StoreUnavailable)
    }
}

pub trait RefreshTransport: Send + Sync {
    fn refresh(
        &self,
        account: &AccountId,
        refresh_token: &str,
    ) -> Result<RefreshResult, GoogleError>;
}

/// Production transport path. The future must honor the caller's cancellation.
pub trait AsyncRefreshTransport: Send + Sync {
    fn refresh_async<'a>(
        &'a self,
        account: &'a AccountId,
        refresh_token: &'a str,
        now_secs: u64,
        cancel: &'a CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<RefreshResult, GoogleError>> + Send + 'a>>;
}

/// One short-lived refresh lock per account, shared by Gmail and Calendar in one engine.
/// Secrets remain with the vault/transport; no token type derives Debug/Serialize.
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

    /// Native vault calls run on Tokio's blocking pool; provider waits never block it.
    /// The same account mutex also serializes the legacy synchronous worker path.
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
        if token.expires_at_secs > now_secs.saturating_add(60) {
            return Ok(token.access_token);
        }
        let result = transport
            .refresh_async(account, &token.refresh_token, now_secs, cancel)
            .await?;
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
