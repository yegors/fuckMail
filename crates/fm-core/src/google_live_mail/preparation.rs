//! Off-writer preparation of fetched Gmail messages: size limits, MIME parsing,
//! text extraction and blob staging, with no SQLite access.
use super::*;
use crate::google_store::StagedMessage;
use fm_google::gmail_api::RawOutcome;
use fm_google::transport::CancellationToken;
use std::path::PathBuf;

/// Why a message was not fully imported. The source is still kept whenever it
/// was downloaded and decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemIssue {
    /// The raw source exceeds the Gmail client's download bound.
    TooLarge,
    /// Gmail's reply failed validation or could not be decoded.
    MalformedResponse,
    /// The source could not be parsed as MIME.
    MalformedMime,
    /// Not produced by current preparation.
    IndexTooLarge,
    /// The message has more attachments than one import may stage.
    AttachmentLimit,
    /// Normalized headers exceed the import bound, or the whole record exceeds
    /// [`MAX_PART_BYTES`].
    HeaderLimit,
    /// Gmail's `internalDate` is missing or unusable.
    InvalidInternalDate,
    /// The message carries Gmail's `DRAFT` label; drafts are imported
    /// separately, so only its source is kept here.
    RemoteDraft,
    /// The ID was observed but deliberately not fetched on this page.
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
/// What publication does with a prepared item.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Disposition {
    /// Parsed, indexed and publishable as a message.
    Ready,
    /// Nothing usable was downloaded; `issue` says why.
    Unavailable,
    /// The source is stored but was not turned into a message; `issue` says why.
    SourceRetainedUnindexed,
    /// Gmail reported the message deleted or returned 404.
    Missing,
    /// Seen on the page without being downloaded.
    Observed,
    /// Dated before the retention window; nothing is stored.
    OutsideRetention,
    /// In Gmail's Spam. Spam is read live, never kept; a local copy goes.
    Spam,
}
/// A staged blob named by its SHA-256 hex digest.
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct BlobRef {
    pub hash: String,
    pub size: u64,
}
/// The normalized record of one item's outcome; it references blobs instead
/// of holding source bytes. Its JSON is what is staged, digested and bounded
/// by [`MAX_PART_BYTES`].
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
    /// The size of the normalized manifest, which counts toward a part's
    /// [`MAX_PART_BYTES`].
    pub fn normalized_bytes(&self) -> usize {
        self.json.len()
    }
    /// Why the message was not fully imported, if it was not.
    pub fn issue(&self) -> Option<ItemIssue> {
        self.manifest.issue
    }
}

/// Untrusted recovery manifest; obtaining it from SQLite is not a blob proof.
///
/// Turn it back into a [`PreparedGoogleItem`] with
/// [`verify_staged_google_item`].
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
    prepare_google_item_since(blobs, page, remote_id, outcome, cancel, None)
}

/// Prepares a message Gmail reports changed from its retained source and
/// Gmail's current `state`, exactly as [`prepare_google_item_since`] would
/// from a download of it. `None` when the retained source cannot be read, so
/// the caller downloads it instead.
pub fn prepare_google_item_retained(
    blobs: &BlobStore,
    page: &GooglePageIdentity,
    remote_id: &str,
    retained: &RetainedGoogleSource,
    state: fm_google::gmail_api::GmailMessageState,
    cancel: &CancellationToken,
    since: Option<i64>,
) -> Result<Option<PreparedGoogleItem>> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    cancelled(cancel)?;
    let Ok(source) = crate::local_content::read_source(blobs, &retained.source_hash) else {
        return Ok(None);
    };
    let raw = fm_google::gmail_api::RawItem {
        message: fm_google::sync::RawMessage {
            id: state.id,
            thread_id: state.thread_id,
            label_ids: state.label_ids,
            history_id: state.history_id,
            raw: URL_SAFE_NO_PAD.encode(source),
        },
        internal_date: Some(retained.internal_date_ms.to_string()),
        size_estimate: None,
    };
    prepare_google_item_since(
        blobs,
        page,
        remote_id,
        Some(RawOutcome::Available(raw)),
        cancel,
        since,
    )
    .map(Some)
}
/// Like [`prepare_google_item`], but a message whose Gmail internal date falls
/// before `since` (Unix seconds, the retention start) is marked outside
/// retention and nothing of it is staged.
///
/// Writes the raw source, and any attachments found while parsing, as
/// content-addressed blobs before any SQL refers to them. Fails with
/// `cancelled` when `cancel` fires, and with a storage error if a blob cannot
/// be written. Content problems are not errors: they become an [`ItemIssue`]
/// on the returned item.
pub fn prepare_google_item_since(
    blobs: &BlobStore,
    page: &GooglePageIdentity,
    remote_id: &str,
    outcome: Option<RawOutcome>,
    cancel: &CancellationToken,
    since: Option<i64>,
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
    // Spam and out-of-window mail are recognised before decoding, so neither
    // costs a blob write. Spam comes first: a local copy goes whatever its date.
    if raw.message.label_ids.iter().any(|label| label == "SPAM") {
        if raw.message.id != remote_id {
            return Err(err(
                "google_invalid_response",
                "Unexpected message identity",
            ));
        }
        manifest.remote_thread_id = Some(raw.message.thread_id);
        manifest.history_id = Some(raw.message.history_id);
        manifest.internal_date_ms = date;
        manifest.disposition = Disposition::Spam;
        return proof(blobs, page, manifest);
    }
    if since
        .zip(date)
        .is_some_and(|(since, date)| date < since.saturating_mul(1000))
    {
        if raw.message.id != remote_id {
            return Err(err(
                "google_invalid_response",
                "Unexpected message identity",
            ));
        }
        manifest.remote_thread_id = Some(raw.message.thread_id);
        manifest.history_id = Some(raw.message.history_id);
        manifest.internal_date_ms = date;
        manifest.disposition = Disposition::OutsideRetention;
        return proof(blobs, page, manifest);
    }
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
    let parsed = crate::google_store::prepare_message(&content);
    cancelled(cancel)?;
    match parsed {
        Ok(mut message) => {
            // Dated by Gmail's internal date, as the retention cutoff is, not
            // by the sender-supplied Date header.
            message.sent_at = timestamp.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            manifest.message = Some(message);
            manifest.disposition = Disposition::Ready;
            // A record too large for any part keeps only its source.
            if serde_json::to_vec(&manifest)?.len() > MAX_PART_BYTES {
                manifest.message = None;
                manifest.disposition = Disposition::SourceRetainedUnindexed;
                manifest.issue = Some(ItemIssue::HeaderLimit);
            }
        }
        // These are the exact reasons `google_store::prepare_message` reports.
        Err(fm_google::GoogleError::InvalidResponse(reason)) => {
            manifest.issue = Some(match reason {
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
///
/// Fails if a referenced blob is missing or altered, or with
/// `stale_google_work` if the manifest is inconsistent or exceeds the bounds
/// fresh preparation enforces.
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
    if let Some(message) = &manifest.message {
        if manifest.disposition != Disposition::Ready
            || manifest.source.is_none()
            || message.remote_id != manifest.remote_id
            || message.attachments.len() > crate::google_store::MAX_RECEIVED_ATTACHMENTS
            || message.body.len() > 256 * 1024
        {
            return Err(stale());
        }
        crate::attachment_content::verify_parts(
            blobs,
            &manifest.source.as_ref().unwrap().hash,
            manifest.source.as_ref().unwrap().size,
            message,
        )?;
    } else if manifest.disposition == Disposition::Ready {
        return Err(stale());
    } else if let Some(source) = &manifest.source {
        blobs.verified_blob(&source.hash, source.size)?;
    }
    cancelled(cancel)?;
    proof(blobs, page, manifest)
}
