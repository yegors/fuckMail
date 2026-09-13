use super::*;
use crate::google_store::StagedMessage;
use fm_google::gmail_api::RawOutcome;
use fm_google::transport::CancellationToken;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemIssue {
    TooLarge,
    MalformedResponse,
    MalformedMime,
    IndexTooLarge,
    AttachmentLimit,
    HeaderLimit,
    InvalidInternalDate,
    RemoteDraft,
    NotFetched,
}
impl ItemIssue {
    pub(super) fn key(self) -> &'static str {
        match self {
            Self::TooLarge => "item_too_large",
            Self::MalformedResponse => "malformed_response",
            Self::MalformedMime => "malformed_mime",
            Self::IndexTooLarge => "index_too_large",
            Self::AttachmentLimit => "attachment_limit",
            Self::HeaderLimit => "header_limit",
            Self::InvalidInternalDate => "invalid_internal_date",
            Self::RemoteDraft => "remote_draft_pending",
            Self::NotFetched => "not_fetched",
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Disposition {
    Ready,
    Unavailable,
    SourceRetainedUnindexed,
    Missing,
    Observed,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct BlobRef {
    pub hash: String,
    pub size: u64,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Manifest {
    pub remote_id: String,
    pub remote_thread_id: Option<String>,
    pub history_id: Option<String>,
    pub internal_date_ms: Option<i64>,
    pub disposition: Disposition,
    pub issue: Option<ItemIssue>,
    pub source: Option<BlobRef>,
    pub message: Option<StagedMessage>,
}

/// Opaque proof constructed only after content publication/verification. Never
/// derive Deserialize or Debug: normalized content and headers are private.
pub struct PreparedGoogleItem {
    pub(super) profile: PathBuf,
    pub(super) page: GooglePageIdentity,
    pub(super) manifest: Manifest,
    pub(super) json: String,
    pub(super) digest: String,
}
impl PreparedGoogleItem {
    pub fn remote_id(&self) -> &str {
        &self.manifest.remote_id
    }
    pub fn normalized_bytes(&self) -> usize {
        self.json.len()
    }
    pub fn issue(&self) -> Option<ItemIssue> {
        self.manifest.issue
    }
}

/// Untrusted recovery manifest; obtaining it from SQLite is not a blob proof.
pub struct StoredGoogleItem(pub(super) String);

fn cancelled(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        Err(err("cancelled", "Google content preparation was cancelled"))
    } else {
        Ok(())
    }
}
fn proof(
    blobs: &BlobStore,
    page: &GooglePageIdentity,
    manifest: Manifest,
) -> Result<PreparedGoogleItem> {
    let json = serde_json::to_string(&manifest)?;
    if json.len() > MAX_PART_BYTES {
        return Err(err(
            "google_part_limit",
            "Prepared Google record exceeds normalized bound",
        ));
    }
    let digest = hash(json.as_bytes());
    Ok(PreparedGoogleItem {
        profile: blobs.profile.clone(),
        page: page.clone(),
        manifest,
        json,
        digest,
    })
}

/// Call on a bounded blocking preparation worker. It performs no SQLite work.
/// `None` is only for an observed ID deliberately excluded from this page's fetch
/// set. Explicit deletions and messages.get 404 use RawOutcome::Missing.
pub fn prepare_google_item(
    blobs: &BlobStore,
    page: &GooglePageIdentity,
    remote_id: &str,
    outcome: Option<RawOutcome>,
    cancel: &CancellationToken,
) -> Result<PreparedGoogleItem> {
    cancelled(cancel)?;
    crate::id(remote_id)?;
    let mut manifest = Manifest {
        remote_id: remote_id.into(),
        remote_thread_id: None,
        history_id: None,
        internal_date_ms: None,
        disposition: Disposition::Unavailable,
        issue: None,
        source: None,
        message: None,
    };
    let raw = match outcome {
        Some(RawOutcome::Available(raw)) => raw,
        other => {
            match other {
                Some(RawOutcome::Missing) => manifest.disposition = Disposition::Missing,
                Some(RawOutcome::TooLarge) => manifest.issue = Some(ItemIssue::TooLarge),
                Some(RawOutcome::Malformed) => manifest.issue = Some(ItemIssue::MalformedResponse),
                None => {
                    manifest.disposition = Disposition::Observed;
                    manifest.issue = Some(ItemIssue::NotFetched);
                }
                Some(RawOutcome::Available(_)) => unreachable!(),
            }
            return proof(blobs, page, manifest);
        }
    };
    let date = raw
        .internal_date
        .as_ref()
        .and_then(|s| s.parse::<i64>().ok());
    let content = match fm_google::sync::decode_raw_message(raw.message, remote_id) {
        Ok(content) => content,
        Err(_) => {
            manifest.issue = Some(ItemIssue::MalformedResponse);
            return proof(blobs, page, manifest);
        }
    };
    manifest.remote_thread_id = Some(content.thread_id.clone());
    manifest.history_id = Some(content.history_id.clone());
    manifest.internal_date_ms = date;
    // Retain exact source even when normalization or attachment policy rejects it.
    manifest.source = Some(BlobRef {
        hash: blobs.stage_blob(&content.source_mime)?,
        size: content.source_mime.len() as u64,
    });
    manifest.disposition = Disposition::SourceRetainedUnindexed;
    cancelled(cancel)?;
    if content.label_ids.iter().any(|label| label == "DRAFT") {
        manifest.issue = Some(ItemIssue::RemoteDraft);
        return proof(blobs, page, manifest);
    }
    let Some(timestamp) = date.and_then(chrono::DateTime::from_timestamp_millis) else {
        manifest.issue = Some(ItemIssue::InvalidInternalDate);
        return proof(blobs, page, manifest);
    };
    let mut storage_failure = None;
    let parsed = crate::google_store::prepare_message(&content, |bytes| {
        if cancel.is_cancelled() {
            return Err(fm_google::GoogleError::TransportInterrupted);
        }
        blobs.stage_blob(bytes).map_err(|error| {
            storage_failure = Some(error);
            fm_google::GoogleError::StoreUnavailable
        })
    });
    cancelled(cancel)?;
    if let Some(error) = storage_failure {
        return Err(error);
    }
    match parsed {
        Ok(mut message) => {
            message.sent_at = timestamp.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            manifest.message = Some(message);
            manifest.disposition = Disposition::Ready;
            if serde_json::to_vec(&manifest)?.len() > MAX_PART_BYTES {
                manifest.message = None;
                manifest.disposition = Disposition::SourceRetainedUnindexed;
                manifest.issue = Some(ItemIssue::HeaderLimit);
            }
        }
        Err(fm_google::GoogleError::InvalidResponse(reason)) => {
            manifest.issue = Some(match reason {
                "Normalized body exceeds the first-slice bound" => ItemIssue::IndexTooLarge,
                "Too many staged attachments" => ItemIssue::AttachmentLimit,
                "Normalized header exceeds import bound" => ItemIssue::HeaderLimit,
                _ => ItemIssue::MalformedMime,
            })
        }
        Err(_) => {
            return Err(err(
                "google_preparation_failed",
                "Google content preparation failed",
            ))
        }
    }
    proof(blobs, page, manifest)
}

/// After restart/reauthorization, re-hash every referenced source/attachment on
/// the preparation worker and bind the result to the newly admitted page epoch.
pub fn verify_staged_google_item(
    blobs: &BlobStore,
    page: &GooglePageIdentity,
    stored: StoredGoogleItem,
    cancel: &CancellationToken,
) -> Result<PreparedGoogleItem> {
    cancelled(cancel)?;
    if stored.0.len() > MAX_PART_BYTES {
        return Err(stale());
    }
    let manifest: Manifest = serde_json::from_str(&stored.0)?;
    crate::id(&manifest.remote_id)?;
    if let Some(source) = &manifest.source {
        blobs.verified_blob(&source.hash, source.size)?;
    }
    if let Some(message) = &manifest.message {
        if manifest.disposition != Disposition::Ready
            || manifest.source.is_none()
            || message.remote_id != manifest.remote_id
            || message.attachments.len() > 50
            || message.body.len() > 256 * 1024
        {
            return Err(stale());
        }
        for attachment in &message.attachments {
            cancelled(cancel)?;
            blobs.verified_blob(&attachment.hash, attachment.size)?;
        }
    } else if manifest.disposition == Disposition::Ready {
        return Err(stale());
    }
    cancelled(cancel)?;
    proof(blobs, page, manifest)
}
