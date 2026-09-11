//! Google API feasibility boundary. No HTTP client, database or real credentials.
//!
//! API-shaped inputs become account-scoped, replayable plans for Core's one writer.
//! Transport and vault traits let fixtures exercise decisions before live OAuth.
pub mod attachment;
pub mod auth;
pub mod send;
pub mod sync;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AccountId(String);

impl AccountId {
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GoogleError {
    InvalidResponse(&'static str),
    HistoryExpired,
    ReauthorizationRequired,
    PermissionDenied,
    NotFound,
    RateLimited { retry_after_secs: Option<u64> },
    ServerUnavailable,
    TransportInterrupted,
    StoreUnavailable,
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

/// Classify bounded API failures without propagating Google's message/body into logs.
/// Retry-After supplied here is the parsed delta-seconds form; HTTP-date handling is
/// a responsibility of the future HTTP adapter. A 404 means expiry only on history.list.
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

/// A delay recommendation, not a background scheduler or permission to retry writes.
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

/// Decode a bounded provider JSON response without returning its content in diagnostics.
pub fn decode_json<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    max_bytes: usize,
) -> Result<T, GoogleError> {
    if bytes.len() > max_bytes {
        return Err(GoogleError::InvalidResponse("response exceeds byte limit"));
    }
    serde_json::from_slice(bytes).map_err(|_| GoogleError::InvalidResponse("invalid provider JSON"))
}
