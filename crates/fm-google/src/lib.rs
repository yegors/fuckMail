//! Google clients: OAuth desktop sign-in and token refresh, Gmail API calls,
//! quota scheduling, and OS keychain storage for credentials.
//!
//! Nothing here touches the database. Sync code turns API pages into
//! account-scoped, replayable plans that `fm-core` commits, and the engine runs
//! the workers that drive them. HTTP and credential storage sit behind traits
//! ([`transport::HttpTransport`], [`auth::CredentialVault`]) so tests can use
//! in-memory stand-ins.
pub mod attachment;
pub mod auth;
pub mod desktop_auth;
pub mod gmail_api;
pub mod scheduler;
pub mod send;
pub mod sync;
pub mod transport;
pub mod vault;

use serde::{Deserialize, Serialize};

/// The app's local account identifier, as stored by `fm-core`.
///
/// It keys credentials, quota lanes and API clients. It is opaque here and is
/// never sent to Google.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AccountId(String);

impl AccountId {
    /// Wraps a local account ID of 1–128 bytes with no control characters.
    pub fn new(value: impl Into<String>) -> Result<Self, GoogleError> {
        let value = value.into();
        if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
            return Err(GoogleError::InvalidResponse(
                "invalid local account identity",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Failure of a Google call, a credential operation or a sync step.
///
/// No variant carries provider or mailbox content, so every value is safe to
/// log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GoogleError {
    /// Local input or a provider response failed validation. The static text
    /// is fixed by this crate; some texts are worded for the person.
    InvalidResponse(&'static str),
    /// Gmail `history.list` returned 404: the start history ID is too old and
    /// the mailbox needs a full resync.
    HistoryExpired,
    /// No usable credential remains (missing entry, HTTP 401, or the token
    /// endpoint refused the grant). Only a new interactive sign-in recovers.
    ReauthorizationRequired,
    /// Google returned 403 for a reason other than quota, or a local identity,
    /// state or account-binding check failed.
    PermissionDenied,
    /// HTTP 404 outside `history.list`.
    NotFound,
    /// HTTP 429, a 403 with a quota reason, or a local cooldown that refused
    /// dispatch. `retry_after_secs` is the provider's `Retry-After`, if any.
    RateLimited { retry_after_secs: Option<u64> },
    /// HTTP 5xx.
    ServerUnavailable,
    /// No complete response arrived (network failure, timeout, cancellation or
    /// a refused local dispatch). A write may or may not have reached Google.
    TransportInterrupted,
    /// The OS credential store is locked, refused access or holds an
    /// unreadable record.
    StoreUnavailable,
    /// A sync checkpoint or send operation is not in the state the call
    /// expects; reload the current state before continuing.
    StaleCheckpoint,
}

#[derive(Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}
#[derive(Deserialize)]
struct ErrorBody {
    #[serde(default)]
    errors: Vec<ErrorReason>,
}
#[derive(Deserialize)]
struct ErrorReason {
    reason: String,
}

/// Classifies a failed API response without carrying Google's message or body into logs.
///
/// `retry_after_secs` is Retry-After already converted to seconds (see
/// [`transport::parse_retry_after`]). A 404 means expiry only on history.list.
/// Only the `reason` codes of a JSON error body (up to 64 KiB) are read, to
/// tell a quota 403 from a permission 403.
pub fn classify_api_error(
    status: u16,
    body: &[u8],
    retry_after_secs: Option<u64>,
    history_request: bool,
) -> GoogleError {
    let quota = decode_json::<ErrorEnvelope>(body, 64 * 1024)
        .ok()
        .is_some_and(|response| {
            response.error.errors.iter().any(|error| {
                matches!(
                    error.reason.as_str(),
                    "rateLimitExceeded" | "userRateLimitExceeded" | "dailyLimitExceeded"
                )
            })
        });
    match status {
        401 => GoogleError::ReauthorizationRequired,
        404 if history_request => GoogleError::HistoryExpired,
        404 => GoogleError::NotFound,
        429 => GoogleError::RateLimited { retry_after_secs },
        403 if quota => GoogleError::RateLimited { retry_after_secs },
        403 => GoogleError::PermissionDenied,
        500..=599 => GoogleError::ServerUnavailable,
        _ => GoogleError::InvalidResponse("unexpected provider failure"),
    }
}

/// Returns how long to wait, in milliseconds, before retrying a failed read.
///
/// A delay recommendation, not a background scheduler or permission to retry writes.
/// Only `RateLimited`, `ServerUnavailable` and `TransportInterrupted` are
/// retryable, and only for `attempt` 0–5. The delay is `1 s << attempt`
/// (capped at 60 s) plus up to 999 ms of caller-supplied jitter, or the
/// provider's `Retry-After` when that is longer. See [`scheduler::retry_delay`]
/// for the class-aware entry point.
pub fn read_retry_delay(error: &GoogleError, attempt: u32, jitter_millis: u64) -> Option<u64> {
    let retry_after = match error {
        GoogleError::RateLimited { retry_after_secs } => {
            retry_after_secs.unwrap_or(0).saturating_mul(1000)
        }
        GoogleError::ServerUnavailable | GoogleError::TransportInterrupted => 0,
        _ => return None,
    };
    if attempt >= 6 {
        return None;
    }
    let exponential = (1000_u64 << attempt).min(60_000);
    Some(retry_after.max(exponential.saturating_add(jitter_millis.min(999))))
}

/// Decodes a bounded provider JSON response without returning its content in diagnostics.
///
/// Errors with `InvalidResponse` if `bytes` exceeds `max_bytes` or is not
/// valid JSON for `T`.
pub fn decode_json<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    max_bytes: usize,
) -> Result<T, GoogleError> {
    if bytes.len() > max_bytes {
        return Err(GoogleError::InvalidResponse("response exceeds byte limit"));
    }
    serde_json::from_slice(bytes).map_err(|_| GoogleError::InvalidResponse("invalid provider JSON"))
}
