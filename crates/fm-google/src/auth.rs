use crate::{AccountId, GoogleError};
use oauth2::{
    basic::BasicClient, AuthUrl, ClientId, CsrfToken, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, Scope, TokenUrl,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub const GOOGLE_SCOPES: [&str; 3] = [
    "https://www.googleapis.com/auth/gmail.modify",
    "https://www.googleapis.com/auth/calendar.events",
    "https://www.googleapis.com/auth/calendar.calendarlist.readonly",
];

/// Held in the engine only for one system-browser attempt. Deliberately no Debug.
pub struct DesktopAuthorization {
    pub url: String,
    pub redirect_uri: String,
    csrf: CsrfToken,
    verifier: PkceCodeVerifier,
}

impl DesktopAuthorization {
    /// Port must come from a successfully bound ephemeral IPv4 loopback listener.
    /// Binding, callback timeout and the actual token exchange remain adapter work.
    pub fn new(client_id: String, loopback_port: u16) -> Result<Self, GoogleError> {
        if client_id.is_empty() || loopback_port == 0 {
            return Err(GoogleError::InvalidResponse(
                "OAuth client or callback unavailable",
            ));
        }
        let redirect_uri = format!("http://127.0.0.1:{loopback_port}/oauth2/callback");
        let client = BasicClient::new(ClientId::new(client_id))
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
        let (url, csrf) = request
            .set_pkce_challenge(challenge)
            .add_extra_param("access_type", "offline")
            .url();
        Ok(Self {
            url: url.to_string(),
            redirect_uri,
            csrf,
            verifier,
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
        })
    }
}

pub struct AuthorizationCode {
    pub code: String,
    pub verifier: PkceCodeVerifier,
    pub redirect_uri: String,
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
    fn load(&self, account: &AccountId) -> Result<Option<StoredToken>, GoogleError>;
    fn store(&self, account: &AccountId, token: StoredToken) -> Result<(), GoogleError>;
}

pub trait RefreshTransport: Send + Sync {
    fn refresh(
        &self,
        account: &AccountId,
        refresh_token: &str,
    ) -> Result<RefreshResult, GoogleError>;
}

/// One short-lived refresh lock per account, shared by Gmail and Calendar in one engine.
/// Secrets remain with the vault/transport; no token type derives Debug/Serialize.
#[derive(Default)]
pub struct TokenCoordinator {
    accounts: Mutex<BTreeMap<AccountId, Arc<Mutex<()>>>>,
}

impl TokenCoordinator {
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
        let _refresh = guard.lock().map_err(|_| GoogleError::StoreUnavailable)?;
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
        if result.access_token.is_empty() || result.expires_at_secs <= now_secs.saturating_add(60) {
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
}
