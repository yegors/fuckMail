//! Decoding and length-checking attachment bodies downloaded from Gmail.
use crate::GoogleError;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Reply to `messages.attachments.get`: the body as base64url and its size in
/// bytes.
#[derive(Deserialize)]
pub struct GmailAttachment {
    pub size: u64,
    pub data: String,
}

/// Decodes an attachment body and checks it against `expected_size` bytes.
///
/// No bytes are returned until complete base64url decoding and expected length match.
/// Core then stages/fsyncs/publishes the blob using its existing content store.
/// Errors with `InvalidResponse` on a size mismatch, a size over `max_bytes`,
/// or invalid base64url.
pub fn decode_attachment(
    response: GmailAttachment,
    expected_size: u64,
    max_bytes: usize,
) -> Result<VerifiedAttachment, GoogleError> {
    if expected_size > max_bytes as u64
        || response.size != expected_size
        || response.data.len() > max_bytes.saturating_add(2) / 3 * 4 + 4
    {
        return Err(GoogleError::InvalidResponse(
            "attachment size mismatch or limit",
        ));
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(response.data.trim_end_matches('='))
        .map_err(|_| GoogleError::InvalidResponse("invalid attachment base64url"))?;
    if bytes.len() as u64 != expected_size {
        return Err(GoogleError::InvalidResponse("incomplete attachment"));
    }
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    Ok(VerifiedAttachment { bytes, sha256 })
}

/// Decoded attachment bytes and their lowercase hex SHA-256.
pub struct VerifiedAttachment {
    pub bytes: Vec<u8>,
    pub sha256: String,
}
