//! Explicit user-state delivery. Every POST consumes one durable Core claim.
use super::{mail::Step, SyncContext};
use fm_core::{
    google_live_mutations::{
        MailLabelObservation, MailMutationAction, MailMutationAttemptOutcome, MailMutationDecision,
    },
    CoreError, Result,
};
use fm_google::{
    gmail_api::{GmailCall, GmailReply},
    AccountId, GoogleError,
};
use serde_json::json;
use std::time::Duration;

pub(crate) struct MailMutationWorker {
    account: AccountId,
}
impl MailMutationWorker {
    pub(crate) fn new(account: AccountId) -> Self {
        Self { account }
    }
    pub(crate) async fn step(&mut self, ctx: &SyncContext, now_ms: u64) -> Result<Step> {
        if ctx.account != self.account {
            return Err(failure("stale_google_work", "Google account changed"));
        }
        let account = self.account.as_str().to_owned();
        let generation = ctx.connection_generation;
        let work = ctx
            .core_call(move |store| store.google_mail_mutation_next(&account, generation, now_ms))
            .await?;
        let Some(work) = work else {
            return Ok(Step::Idle(Duration::from_secs(1)));
        };
        let call = GmailCall::message_state(&self.account, &work.remote_id).map_err(provider)?;
        let observed = match ctx.gmail(call).await.map_err(provider)? {
            GmailReply::MessageState(state) => state.map(|state| MailLabelObservation {
                remote_id: state.id,
                remote_thread_id: state.thread_id,
                history_id: state.history_id,
                labels: state.label_ids.into_iter().collect(),
            }),
            _ => {
                return Err(failure(
                    "invalid_google_response",
                    "Expected Google message state",
                ))
            }
        };
        let decision = ctx
            .core_call(move |store| store.google_mail_mutation_observe(&work, observed))
            .await?;
        let attempt = match decision {
            MailMutationDecision::Finished => return Ok(Step::Progress),
            // Core delayed this target. Let the next ready message progress
            // immediately instead of parking the whole account behind it.
            MailMutationDecision::RetryRead => return Ok(Step::Progress),
            MailMutationDecision::Submit(attempt) => attempt,
        };
        let call = match attempt.work.action {
            MailMutationAction::Modify => GmailCall::modify_message(
                &self.account,
                &attempt.work.remote_id,
                &attempt.add_labels,
                &attempt.remove_labels,
            ),
            MailMutationAction::Trash => {
                GmailCall::trash_message(&self.account, &attempt.work.remote_id, false)
            }
            MailMutationAction::Restore => {
                GmailCall::trash_message(&self.account, &attempt.work.remote_id, true)
            }
        }
        .map_err(provider)?;
        // Never loop POST here. Success still needs label readback; a lost
        // response/restart remains durable in_flight/reconcile until that read.
        let outcome = match ctx.gmail_mutation(call).await {
            Ok(GmailReply::Message(message)) if message.id == attempt.work.remote_id => {
                MailMutationAttemptOutcome::Submitted
            }
            Ok(_) => MailMutationAttemptOutcome::Reconcile {
                retry_at_ms: now_ms.saturating_add(1000),
                error_code: "invalid_google_response",
            },
            Err(error) => match error {
                GoogleError::PermissionDenied => MailMutationAttemptOutcome::Failed {
                    error_code: "google_permission_denied",
                },
                GoogleError::NotFound => MailMutationAttemptOutcome::Reconcile {
                    retry_at_ms: now_ms.saturating_add(1000),
                    error_code: "provider_message_missing",
                },
                _ => {
                    let count =
                        u32::try_from(attempt.work.attempt.saturating_sub(1)).unwrap_or(u32::MAX);
                    let retry_after = match &error {
                        GoogleError::RateLimited { retry_after_secs } => {
                            retry_after_secs.unwrap_or(0).saturating_mul(1000)
                        }
                        _ => 0,
                    };
                    let delay = fm_google::read_retry_delay(&error, count, 0)
                        .unwrap_or(300_000)
                        .max(retry_after);
                    MailMutationAttemptOutcome::Reconcile {
                        retry_at_ms: now_ms.saturating_add(delay),
                        error_code: code(&error),
                    }
                }
            },
        };
        ctx.core
            .call(move |store| store.google_mail_mutation_attempt_result(&attempt, outcome))
            .await?;
        Ok(Step::Progress)
    }
}
fn code(error: &GoogleError) -> &'static str {
    match error {
        GoogleError::ReauthorizationRequired => "google_reauthorization_required",
        GoogleError::PermissionDenied => "google_permission_denied",
        GoogleError::RateLimited { .. } => "google_rate_limited",
        GoogleError::ServerUnavailable => "google_server_unavailable",
        GoogleError::TransportInterrupted => "google_transport_interrupted",
        GoogleError::StoreUnavailable => "storage_error",
        GoogleError::StaleCheckpoint => "stale_google_work",
        GoogleError::NotFound => "provider_message_missing",
        _ => "invalid_google_response",
    }
}
fn provider(error: GoogleError) -> CoreError {
    let mut result = failure(code(&error), "Google mail state could not be verified");
    if let GoogleError::RateLimited { retry_after_secs } = error {
        result.details = json!({"retry_after_secs":retry_after_secs});
    }
    result
}
fn failure(code: &str, message: &str) -> CoreError {
    CoreError {
        code: code.into(),
        message: message.into(),
        details: json!({}),
    }
}

#[cfg(test)]
#[path = "mail_mutations/tests.rs"]
mod tests;
