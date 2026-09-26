//! Explicit, frozen Google submission. There is deliberately no owned Send loop.
use super::{drafts, failure, SyncContext};
use fm_core::{
    google_send::{
        GoogleSendEvidence, GoogleSendIntent, GoogleSendObservedMessage, GoogleSendOutcome,
        GoogleSendScope, GoogleSendStep,
    },
    CoreError, Result,
};
use fm_google::{
    gmail_api::{GmailCall, GmailMessageState, GmailMethod, GmailReply},
    send::SubmissionResult,
    GoogleError,
};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    sync::{atomic::Ordering, Arc},
};

/// Handles `google_send`: submits a saved draft once, from a frozen MIME
/// snapshot, with `drafts.send` when it has a linked Gmail draft and
/// `messages.send` otherwise.
///
/// Returns the journaled receipt: accepted, rejected, undispatched, or
/// unknown when Google's answer was lost (`google_send_reconcile` settles it;
/// nothing re-sends automatically). Errors before the attempt is claimed carry
/// `send_invocation_finished_before_dispatch`.
pub(crate) async fn execute(ctx: &SyncContext, request: Value) -> Result<Value> {
    let mut claimed = false;
    let mut result = execute_inner(ctx, request, &mut claimed).await;
    if let Err(error) = &mut result {
        if !claimed {
            // Invocation completion is narrower than historical request
            // certainty: a prior transport-uncertain invocation may exist.
            // Consumers must never use this to erase that prior uncertainty.
            mark_predispatch(error);
        }
    }
    result
}

/// Marks `error` as raised before this invocation dispatched anything.
pub(super) fn predispatch_error(mut error: CoreError) -> CoreError {
    mark_predispatch(&mut error);
    error
}
fn mark_predispatch(error: &mut CoreError) {
    if !error.details.is_object() {
        error.details = serde_json::json!({});
    }
    error.details["send_invocation_finished_before_dispatch"] = Value::Bool(true);
}

/// The send itself. The store re-evaluates the intent after every wait (the
/// read-back, quota admission) and a changed target aborts before dispatch.
/// `claimed` is set once the attempt is journaled and will get an outcome.
async fn execute_inner(ctx: &SyncContext, request: Value, claimed: &mut bool) -> Result<Value> {
    validate("google_send", &request)?;
    let scope = drafts::verified_scope(ctx).await?;
    let initial = match intent(ctx, &scope, &request).await? {
        GoogleSendIntent::Reply(reply) => return Ok(reply),
        GoogleSendIntent::Submit { provider_draft_id } => provider_draft_id,
    };
    if initial.is_some() {
        // A known twin is read before submission. Core compares the frozen
        // baseline again at claim; Google has no conditional-send/CAS promise.
        drafts::observe(ctx, &scope, text(&request, "draft_id")?).await?;
    }
    let target = match intent(ctx, &scope, &request).await? {
        GoogleSendIntent::Reply(reply) => return Ok(reply),
        GoogleSendIntent::Submit { provider_draft_id } => provider_draft_id,
    };
    let method = if target.is_some() {
        GmailMethod::DraftSend
    } else {
        GmailMethod::MessageSend
    };
    // The queued request contains identifiers/revisions only. Admission holds
    // the same 100-unit account mutation lease through the actual HTTP call.
    let (api, permit, dispatched) = ctx.gmail_send_request(method).await?;
    let memory = Arc::new(permit);
    match intent(ctx, &scope, &request).await? {
        GoogleSendIntent::Reply(reply) => return Ok(reply),
        GoogleSendIntent::Submit { provider_draft_id } if provider_draft_id == target => {}
        _ => {
            return Err(failure(
                "stale_google_work",
                "The send target changed while awaiting quota",
            ))
        }
    }
    let frozen_scope = scope.clone();
    let begun = drafts::memory_core(ctx, &memory, move |store| {
        store.google_send_begin(&frozen_scope, &request)
    })
    .await?;
    let prepared = match begun {
        GoogleSendStep::Reply(reply) => reply,
        GoogleSendStep::Prepare(plan) => {
            if plan.provider_draft_id() != target.as_deref() {
                return Err(failure(
                    "stale_google_work",
                    "The send target changed before MIME preparation",
                ));
            }
            let proof = drafts::prepare(ctx, &memory, move |cancel| {
                fm_core::google_send::prepare_google_send(plan, &cancel)
            })
            .await?;
            drafts::memory_core(ctx, &memory, move |store| store.google_send_finish(proof)).await?
        }
    };
    if prepared["state"] != "prepared" || prepared["operation_is_current"] == false {
        return Ok(prepared);
    }
    let request_id = text(&prepared, "request_id")?.to_owned();
    let revision = prepared["revision"].as_i64().ok_or_else(|| {
        failure(
            "invalid_send_state",
            "The prepared send revision is missing",
        )
    })?;
    let lease = drafts::memory_core(ctx, &memory, move |store| {
        store.google_send_claim(&scope, &request_id, revision)
    })
    .await?;
    *claimed = true;
    // Every claimed lease gets its exact outcome, including failures before
    // dispatch. Do not select cancellation against the short writer receipt.
    let plan = lease.take_upload();
    let bytes = drafts::prepare(ctx, &memory, move |cancel| {
        fm_core::google_send::load_google_send_upload(plan, &cancel)
    })
    .await;
    let response = match bytes {
        Ok(bytes) => {
            let call = match lease.provider_draft_id() {
                Some(id) => GmailCall::send_draft(&ctx.account, id, &bytes, lease.thread_id()),
                None => GmailCall::send_message(&ctx.account, &bytes, lease.thread_id()),
            };
            match call {
                Ok(call) if call.method() == method => api
                    .execute(call, "engine-managed", &ctx.cancel)
                    .await
                    .map_err(provider),
                Ok(_) => Err(failure(
                    "stale_google_work",
                    "The send target changed before dispatch",
                )),
                Err(error) => Err(provider(error)),
            }
        }
        Err(error) => Err(error),
    };
    // Classify: anything that failed before the request left the engine is
    // `Undispatched`; after dispatch, only a definite answer is accepted or
    // rejected, and everything else is `Unknown`.
    let reached_http = dispatched.load(Ordering::Acquire);
    let outcome = match response {
        Ok(GmailReply::Submission(submission)) => match submission.result {
            SubmissionResult::Accepted {
                message_id,
                thread_id,
            } => GoogleSendOutcome::Accepted {
                message_id,
                thread_id,
            },
            SubmissionResult::Rejected { .. } => GoogleSendOutcome::Rejected {
                code: submission
                    .error
                    .map(provider)
                    .map(|error| error.code)
                    .unwrap_or_else(|| "google_submission_rejected".into()),
            },
            SubmissionResult::NoDefinitiveResponse => {
                let code = submission
                    .error
                    .map(provider)
                    .map(|error| error.code)
                    .unwrap_or_else(|| "invalid_google_response".into());
                if reached_http {
                    GoogleSendOutcome::Unknown { code }
                } else {
                    GoogleSendOutcome::Undispatched { code }
                }
            }
        },
        Ok(_) if reached_http => GoogleSendOutcome::Unknown {
            code: "invalid_google_response".into(),
        },
        Ok(_) => GoogleSendOutcome::Undispatched {
            code: "invalid_google_response".into(),
        },
        Err(error) if reached_http => GoogleSendOutcome::Unknown { code: error.code },
        Err(error) => GoogleSendOutcome::Undispatched { code: error.code },
    };
    let held = memory.clone();
    // Exact late acceptance survives owner cancellation and reauthorization;
    // Core fences publication separately from this durable historical receipt.
    let (receipt, _held) = ctx
        .core
        .call(move |store| Ok((store.google_send_outcome(&lease, outcome), held)))
        .await?;
    receipt
}

/// Asks the store what the send request means now: a recorded reply, or a
/// submission through a linked Gmail draft or as a new message.
async fn intent(
    ctx: &SyncContext,
    scope: &GoogleSendScope,
    request: &Value,
) -> Result<GoogleSendIntent> {
    let scope = scope.clone();
    let request = request.clone();
    ctx.core_call(move |store| store.google_send_intent(&scope, &request))
        .await
}

/// Handles `google_send_reconcile`: searches Sent for the frozen Message-ID
/// (at most ten candidates), reads their labels, and hands the evidence to
/// the store, which decides the outcome. It never sends again.
pub(crate) async fn reconcile(ctx: &SyncContext, request_id: &str) -> Result<Value> {
    let scope = drafts::verified_scope(ctx).await?;
    let request_id = request_id.to_owned();
    let plan = ctx
        .core_call(move |store| store.google_send_reconcile_begin(&scope, &request_id))
        .await?;
    let query = format!(
        "in:sent rfc822msgid:{}",
        plan.message_id()
            .trim_start_matches('<')
            .trim_end_matches('>')
    );
    let page = match ctx
        .gmail(
            GmailCall::list_messages(&ctx.account, None, Some(&query), false, 10)
                .map_err(provider)?,
        )
        .await
        .map_err(provider)?
    {
        GmailReply::Messages(page) if page.messages.len() <= 10 => page,
        _ => {
            return Err(failure(
                "invalid_google_response",
                "Expected at most ten Sent message candidates",
            ))
        }
    };
    let mut evidence = GoogleSendEvidence {
        query: "sent_message_id".into(),
        draft_observation: "not_checked".into(),
        truncated: page.next_page_token.is_some(),
        matches: Vec::new(),
        acknowledged_message: None,
        missing_ids: Vec::new(),
    };
    let mut ids = BTreeSet::new();
    let mut evidence_bytes = 0usize;
    for candidate in page.messages {
        if !ids.insert(candidate.id.clone()) {
            return Err(failure(
                "invalid_google_response",
                "Duplicate Sent message candidate",
            ));
        }
        match observed(ctx, &candidate.id, &mut evidence_bytes).await? {
            Some(message) => {
                if message.thread_id != candidate.thread_id {
                    return Err(failure(
                        "invalid_google_response",
                        "Sent candidate changed thread identity",
                    ));
                }
                if plan.accepted_message_id() == Some(message.message_id.as_str()) {
                    evidence.acknowledged_message = Some(copy_observation(&message));
                }
                evidence.matches.push(message);
            }
            None => evidence.missing_ids.push(candidate.id),
        }
    }
    if let Some(ack) = plan.accepted_message_id().filter(|id| !ids.contains(*id)) {
        match observed(ctx, ack, &mut evidence_bytes).await? {
            Some(message) => evidence.acknowledged_message = Some(message),
            None => evidence.missing_ids.push(ack.to_owned()),
        }
    }
    // Evidence is deliberately bounded and can be incomplete. Search matches,
    // SENT labels/history IDs and absence never promote unknown to accepted.
    ctx.core_call(move |store| store.google_send_reconcile_finish(plan, evidence))
        .await
}

fn copy_observation(message: &GoogleSendObservedMessage) -> GoogleSendObservedMessage {
    GoogleSendObservedMessage {
        message_id: message.message_id.clone(),
        thread_id: message.thread_id.clone(),
        history_id: message.history_id.clone(),
        labels: message.labels.clone(),
    }
}

/// Reads one message's thread, labels and history id, charging its size to
/// the 24 KiB evidence budget in `total`. `None` means the message is gone.
async fn observed(
    ctx: &SyncContext,
    id: &str,
    total: &mut usize,
) -> Result<Option<GoogleSendObservedMessage>> {
    let result = ctx
        .gmail(GmailCall::message_state(&ctx.account, id).map_err(provider)?)
        .await;
    let message = match result {
        Ok(GmailReply::MessageState(Some(message))) => message,
        Ok(GmailReply::MessageState(None)) | Err(GoogleError::NotFound) => return Ok(None),
        Err(error) => return Err(provider(error)),
        _ => {
            return Err(failure(
                "invalid_google_response",
                "Expected exact message label/history metadata",
            ))
        }
    };
    if message.id != id {
        return Err(failure(
            "invalid_google_response",
            "Sent observation returned another message",
        ));
    }
    let GmailMessageState {
        id,
        thread_id,
        history_id,
        label_ids,
    } = message;
    *total += id.len()
        + thread_id.len()
        + history_id.len()
        + label_ids.iter().map(|label| label.len() + 8).sum::<usize>()
        + 256;
    if *total > 24 * 1024 {
        return Err(failure(
            "limit_exceeded",
            "Send observation exceeds its bounded metadata budget",
        ));
    }
    Ok(Some(GoogleSendObservedMessage {
        message_id: id,
        thread_id,
        history_id: Some(history_id),
        labels: label_ids,
    }))
}

/// Checks a send request has exactly its method's fields: bounded ids, and
/// revisions of at least 1 (`expected_google_revision` may be 0).
pub(crate) fn validate(method: &str, params: &Value) -> Result<()> {
    let fields: &[&str] = match method {
        "google_send" => &[
            "draft_id",
            "expected_revision",
            "expected_google_revision",
            "request_id",
        ],
        "google_send_status" | "google_send_reconcile" => &["request_id"],
        "google_send_abandon" => &["request_id", "expected_revision"],
        _ => return Err(failure("invalid_request", "Unknown Google send operation")),
    };
    let object = params
        .as_object()
        .ok_or_else(|| failure("invalid_request", "Expected send parameters"))?;
    if object.len() != fields.len() || fields.iter().any(|key| !object.contains_key(*key)) {
        return Err(failure(
            "invalid_request",
            "Use the exact Google send parameters",
        ));
    }
    for key in fields {
        if key.starts_with("expected_") {
            if params[*key].as_i64().is_none_or(|value| {
                value
                    < if *key == "expected_google_revision" {
                        0
                    } else {
                        1
                    }
            }) {
                return Err(failure(
                    "invalid_request",
                    "Choose the exact current send revision",
                ));
            }
        } else {
            text(params, key)?;
        }
    }
    Ok(())
}

fn text<'a>(params: &'a Value, key: &str) -> Result<&'a str> {
    params[key]
        .as_str()
        .filter(|value| {
            !value.is_empty()
                && value.len() <= if key == "request_id" { 128 } else { 256 }
                && !value.chars().any(char::is_control)
        })
        .ok_or_else(|| failure("invalid_request", "Choose a bounded send identity"))
}

fn provider(error: GoogleError) -> CoreError {
    let code = match error {
        GoogleError::PermissionDenied => "google_permission_denied",
        GoogleError::ReauthorizationRequired => "google_reauthorization_required",
        GoogleError::RateLimited { .. } => "google_rate_limited",
        GoogleError::NotFound => "google_message_missing",
        GoogleError::TransportInterrupted => "google_transport_interrupted",
        GoogleError::ServerUnavailable => "google_server_unavailable",
        _ => "invalid_google_response",
    };
    failure(code, "Google send did not provide the required response")
}

#[cfg(test)]
mod tests;
