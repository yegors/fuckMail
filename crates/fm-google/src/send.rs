//! At-most-once message submission. A send is journaled as in flight before
//! the one transport call, and a missing reply is recorded as unknown rather
//! than retried.
use crate::{AccountId, GoogleError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Durable state of one submission. Only `Prepared` may be submitted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SendState {
    /// Journaled and not yet sent.
    Prepared,
    /// Journaled just before the one transport call. Found after a restart,
    /// it becomes `AcceptanceUnknown`, never a resend.
    InFlight,
    /// Google returned a receipt with the new message and thread IDs.
    Accepted {
        message_id: String,
        thread_id: String,
    },
    /// Google definitively refused the submission.
    Rejected { reason: String },
    /// No definitive reply; the message may or may not have been sent.
    AcceptanceUnknown,
}

/// One journaled submission of one exact MIME message, identified by
/// `operation_id` and pinned by the lowercase hex SHA-256 of its bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendOperation {
    pub account: AccountId,
    pub operation_id: String,
    pub mime_sha256: String,
    pub state: SendState,
}

/// What one submission attempt's reply proves.
pub enum SubmissionResult {
    /// Google accepted the message and returned its IDs.
    Accepted {
        message_id: String,
        thread_id: String,
    },
    /// A response known to have rejected submission, e.g. an explicit permission error.
    Rejected { reason: &'static str },
    /// Timeout, disconnect, server failure or process restart after durable InFlight.
    NoDefinitiveResponse,
}

/// Performs the network submission for [`submit_once`]. It must not retry and
/// must report anything uncertain as `NoDefinitiveResponse`.
pub trait SubmissionTransport {
    fn send(&mut self, account: &AccountId, mime: &[u8]) -> SubmissionResult;
}

/// Durable store for [`SendOperation`] transitions.
pub trait SendJournal {
    /// Compare-and-swap the existing operation to prevent simultaneous duplicate send.
    fn transition(
        &mut self,
        expected: &SendOperation,
        next: &SendOperation,
    ) -> Result<(), GoogleError>;
}

impl SendOperation {
    /// Starts a `Prepared` operation for `mime`. Errors with `InvalidResponse`
    /// for an empty or over-128-byte `operation_id` or empty MIME.
    pub fn prepared(
        account: AccountId,
        operation_id: String,
        mime: &[u8],
    ) -> Result<Self, GoogleError> {
        if operation_id.is_empty() || operation_id.len() > 128 || mime.is_empty() {
            return Err(GoogleError::InvalidResponse("invalid submission intent"));
        }
        Ok(Self {
            account,
            operation_id,
            mime_sha256: format!("{:x}", Sha256::digest(mime)),
            state: SendState::Prepared,
        })
    }

    /// Returns the state to resume from after a restart: `InFlight` becomes
    /// `AcceptanceUnknown` and every other state is unchanged.
    pub fn recover(&self) -> Self {
        if self.state == SendState::InFlight {
            Self {
                state: SendState::AcceptanceUnknown,
                ..self.clone()
            }
        } else {
            self.clone()
        }
    }
}

/// Submits a `Prepared` operation at most once and journals the outcome.
///
/// This function never retries. Persist InFlight before allowing the first transport call.
/// No receipt means unknown, even when a timeout is retryable for read-only requests.
/// Errors with `StaleCheckpoint` if the operation is not `Prepared` or `mime`
/// does not match its hash; journal errors pass through.
pub fn submit_once<T: SubmissionTransport, J: SendJournal>(
    operation: &mut SendOperation,
    mime: &[u8],
    transport: &mut T,
    journal: &mut J,
) -> Result<(), GoogleError> {
    if operation.state != SendState::Prepared
        || operation.mime_sha256 != format!("{:x}", Sha256::digest(mime))
    {
        return Err(GoogleError::StaleCheckpoint);
    }
    let in_flight = SendOperation {
        state: SendState::InFlight,
        ..operation.clone()
    };
    journal.transition(operation, &in_flight)?;
    *operation = in_flight;
    let result = transport.send(&operation.account, mime);
    // An acceptance without both IDs cannot be reconciled, so it is unknown.
    let state = match result {
        SubmissionResult::Accepted {
            message_id,
            thread_id,
        } if !message_id.is_empty() && !thread_id.is_empty() => SendState::Accepted {
            message_id,
            thread_id,
        },
        SubmissionResult::Rejected { reason } => SendState::Rejected {
            reason: reason.to_owned(),
        },
        _ => SendState::AcceptanceUnknown,
    };
    let completed = SendOperation {
        state,
        ..operation.clone()
    };
    // If this commit fails, durable InFlight is recovered as unknown; do not resubmit.
    journal.transition(operation, &completed)?;
    *operation = completed;
    Ok(())
}
