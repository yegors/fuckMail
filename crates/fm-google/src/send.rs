use crate::{AccountId, GoogleError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SendState {
    Prepared,
    InFlight,
    Accepted {
        message_id: String,
        thread_id: String,
    },
    Rejected {
        reason: String,
    },
    AcceptanceUnknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendOperation {
    pub account: AccountId,
    pub operation_id: String,
    pub mime_sha256: String,
    pub state: SendState,
}

pub enum SubmissionResult {
    Accepted {
        message_id: String,
        thread_id: String,
    },
    /// A response known to have rejected submission, e.g. an explicit permission error.
    Rejected { reason: &'static str },
    /// Timeout, disconnect, server failure or process restart after durable InFlight.
    NoDefinitiveResponse,
}

pub trait SubmissionTransport {
    fn send(&mut self, account: &AccountId, mime: &[u8]) -> SubmissionResult;
}

pub trait SendJournal {
    /// Compare-and-swap the existing operation to prevent simultaneous duplicate send.
    fn transition(
        &mut self,
        expected: &SendOperation,
        next: &SendOperation,
    ) -> Result<(), GoogleError>;
}

impl SendOperation {
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

/// This function never retries. Persist InFlight before allowing the first transport call.
/// No receipt means unknown, even when a timeout is retryable for read-only requests.
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
