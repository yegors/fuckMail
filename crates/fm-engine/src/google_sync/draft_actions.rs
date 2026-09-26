//! Explicit source adoption and exact-target deletion. No background mutation loop.
use super::{drafts, failure, SyncContext};
use crate::local_content::authoring_uncommitted;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use fm_core::{google_draft_actions::*, Result};
use fm_google::{
    gmail_api::{DraftDeletionResult, GmailCall, GmailDraft, GmailReply},
    GoogleError,
};
use serde_json::Value;
use std::sync::{atomic::Ordering, Arc};

/// Checks that a draft-action request has exactly the method's fields, with
/// bounded identifiers and positive revisions.
pub(crate) fn validate(method: &str, params: &Value) -> Result<()> {
    let keys: &[&str] = match method {
        "google_draft_adopt" | "google_draft_delete" => {
            &["draft_id", "source_revision", "request_id"]
        }
        "google_draft_delete_status" | "google_draft_delete_reconcile" => &["request_id"],
        "google_draft_delete_abandon" => &["request_id", "expected_revision"],
        _ => return Err(failure("unsupported_method", "Unknown draft source action")),
    };
    if params
        .as_object()
        .is_none_or(|p| p.len() != keys.len() || p.keys().any(|k| !keys.contains(&k.as_str())))
    {
        return Err(failure(
            "invalid_request",
            "Use the exact draft action fields",
        ));
    }
    for key in keys {
        if key.ends_with("revision") {
            if params[*key].as_i64().is_none_or(|n| n < 1) {
                return Err(failure(
                    "invalid_request",
                    "Choose a positive current revision",
                ));
            }
        } else if params[*key].as_str().is_none_or(|s| {
            s.is_empty()
                || s.len() > if *key == "draft_id" { 256 } else { 128 }
                || s.chars().any(char::is_control)
        }) {
            return Err(failure(
                "invalid_request",
                "Choose an exact draft or request identity",
            ));
        }
    }
    Ok(())
}

/// Handles `google_draft_adopt`: reads a remote Gmail draft fresh and turns it
/// into an editable local draft. Nothing is written to Google. Errors before
/// the local commit carry the "finished without commit" marker.
pub(crate) async fn adopt(ctx: &SyncContext, params: Value) -> Result<Value> {
    let scope = drafts::verified_scope(ctx)
        .await
        .map_err(authoring_uncommitted)?;
    let step = ctx
        .core_call(move |store| store.google_draft_adopt_begin(&scope, &params))
        .await
        .map_err(authoring_uncommitted)?;
    let plan = match step {
        GoogleDraftAuthoringStep::Reply(reply) => return Ok(reply),
        GoogleDraftAuthoringStep::Prepare(plan) => plan,
    };
    let (remote, memory) = read(ctx, plan.provider_draft_id())
        .await
        .map_err(authoring_uncommitted)?;
    let remote = remote.ok_or_else(|| {
        authoring_uncommitted(failure("not_found", "The selected Google draft is absent"))
    })?;
    let proof = drafts::prepare(ctx, &memory, move |cancel| {
        prepare_google_draft_authoring(plan, Some(decode(remote)?), &cancel)
    })
    .await
    .map_err(authoring_uncommitted)?;
    drafts::memory_core(ctx, &memory, move |store| {
        store.google_draft_authoring_finish(proof)
    })
    .await
    .map_err(|error| {
        if matches!(error.code.as_str(), "cancelled" | "stale_google_work") {
            authoring_uncommitted(error)
        } else {
            error
        }
    })
}

/// The account and connection generation that deletion steps are checked against.
fn scope(ctx: &SyncContext) -> GoogleDraftDeleteScope {
    GoogleDraftDeleteScope {
        account: ctx.account.as_str().into(),
        connection_generation: ctx.connection_generation,
    }
}

/// Marks `error` as raised before any delete request was sent, so the client
/// knows the Google draft is untouched.
pub(crate) fn predispatch_error(mut error: fm_core::CoreError) -> fm_core::CoreError {
    if !error.details.is_object() {
        error.details = serde_json::json!({});
    }
    error.details["draft_delete_invocation_finished_before_dispatch"] = Value::Bool(true);
    error
}

/// Journals the deletion request and checks, against a fresh read, that the
/// remote draft still matches the source revision the user chose.
async fn prepare_delete(ctx: &SyncContext, params: Value) -> Result<Value> {
    let deletion_scope = scope(ctx);
    let step = ctx
        .core_call(move |store| store.google_draft_delete_begin(&deletion_scope, &params))
        .await?;
    let reply = match step {
        GoogleDraftDeleteStep::Reply(reply) => reply,
        GoogleDraftDeleteStep::Prepare(plan) => {
            let (remote, memory) = read(ctx, plan.provider_draft_id()).await?;
            let remote = remote
                .ok_or_else(|| failure("not_found", "The selected Google draft is absent"))?;
            let proof = drafts::prepare(ctx, &memory, move |cancel| {
                prepare_google_draft_delete(plan, decode(remote)?, &cancel)
            })
            .await?;
            drafts::memory_core(ctx, &memory, move |store| {
                store.google_draft_delete_finish(proof)
            })
            .await?
        }
    };
    Ok(reply)
}

/// Handles `google_draft_delete`: sends one `drafts.delete` for exactly the
/// chosen draft and returns the journaled outcome. When Google's answer is
/// lost the state is `unknown`; `google_draft_delete_reconcile` resolves it by
/// reading back, and the delete is never repeated automatically.
pub(crate) async fn delete(ctx: &SyncContext, params: Value) -> Result<Value> {
    let reply = prepare_delete(ctx, params)
        .await
        .map_err(predispatch_error)?;
    if reply["state"] != "prepared" {
        return Ok(reply);
    }
    let request_id = reply["request_id"]
        .as_str()
        .ok_or_else(|| {
            failure(
                "invalid_draft_state",
                "Deletion request identity is missing",
            )
        })?
        .to_owned();
    let revision = reply["revision"]
        .as_i64()
        .ok_or_else(|| failure("invalid_draft_state", "Deletion revision is missing"))?;
    let target = reply["provider_draft_id"]
        .as_str()
        .ok_or_else(|| failure("invalid_draft_state", "Deletion target is missing"))?
        .to_owned();
    // The completed read and its RAW lease are dropped before mutation quota.
    let (api, permit, dispatched) = ctx
        .gmail_delete_request(&target)
        .await
        .map_err(predispatch_error)?;
    let memory = Arc::new(permit);
    let deletion_scope = scope(ctx);
    let lease = drafts::memory_core(ctx, &memory, move |store| {
        store.google_draft_delete_claim(&deletion_scope, &request_id, revision)
    })
    .await
    .map_err(predispatch_error)?;
    let response = match GmailCall::delete_draft(&ctx.account, lease.provider_draft_id()) {
        Ok(call) => api.execute(call, "engine-managed", &ctx.cancel).await,
        Err(error) => Err(error),
    };
    // Only a request that never left the engine counts as undispatched; any
    // failure after dispatch is an unknown outcome.
    let outcome = if !dispatched.load(Ordering::Acquire) {
        GoogleDraftDeleteOutcome::Undispatched {
            code: "google_undispatched".into(),
        }
    } else {
        match response {
            Ok(GmailReply::DraftDeletion(deletion)) => {
                let code = deletion
                    .error
                    .map(drafts::provider)
                    .map(|e| e.code)
                    .unwrap_or_else(|| "google_rejected".into());
                match deletion.result {
                    DraftDeletionResult::Deleted => GoogleDraftDeleteOutcome::Deleted,
                    DraftDeletionResult::Absent => GoogleDraftDeleteOutcome::Absent,
                    DraftDeletionResult::Rejected => GoogleDraftDeleteOutcome::Rejected { code },
                    DraftDeletionResult::NoDefinitiveResponse => {
                        GoogleDraftDeleteOutcome::Unknown { code }
                    }
                }
            }
            Err(error) => GoogleDraftDeleteOutcome::Unknown {
                code: drafts::provider(error).code,
            },
            Ok(_) => GoogleDraftDeleteOutcome::Unknown {
                code: "invalid_google_response".into(),
            },
        }
    };
    // Once claimed, persist the exact lease's outcome even after owner cancellation
    // or connection replacement. Core validates the immutable lease itself.
    ctx.core
        .call(move |store| {
            let _memory = memory;
            store.google_draft_delete_outcome(&lease, outcome)
        })
        .await
}

/// Handles `google_draft_delete_reconcile`: settles an unknown deletion by
/// reading the draft back, never by deleting again.
pub(crate) async fn reconcile(ctx: &SyncContext, request_id: &str) -> Result<Value> {
    let deletion_scope = scope(ctx);
    let request_id = request_id.to_owned();
    let plan = ctx
        .core_call(move |store| {
            store.google_draft_delete_reconcile_begin(&deletion_scope, &request_id)
        })
        .await?;
    let (remote, memory) = read(ctx, plan.provider_draft_id()).await?;
    let proof = drafts::prepare(ctx, &memory, move |cancel| {
        prepare_google_draft_delete_observation(plan, remote.map(decode).transpose()?, &cancel)
    })
    .await?;
    drafts::memory_core(ctx, &memory, move |store| {
        store.google_draft_delete_reconcile_finish(proof)
    })
    .await
}

/// Fetches a draft's RAW source with a preparation permit. `None` means
/// Google reports the draft absent; the permit is still returned.
async fn read(ctx: &SyncContext, target: &str) -> Result<(Option<GmailDraft>, drafts::Memory)> {
    let call = GmailCall::get_draft(&ctx.account, target).map_err(drafts::provider)?;
    match ctx.gmail_prepared(call).await {
        Ok((GmailReply::Draft(remote), permit)) => Ok((Some(remote), Arc::new(permit))),
        Err(GoogleError::NotFound) => Ok((None, Arc::new(ctx.prepare_permit().await?))),
        Err(error) => Err(drafts::provider(error)),
        Ok(_) => Err(failure(
            "invalid_google_response",
            "Expected the exact remote draft source",
        )),
    }
}
/// Decodes the draft's base64url RAW message, tolerating trailing padding.
fn decode(remote: GmailDraft) -> Result<GoogleDraftActionRemote> {
    let raw = URL_SAFE_NO_PAD
        .decode(
            remote
                .message
                .raw
                .as_deref()
                .unwrap_or_default()
                .trim_end_matches('='),
        )
        .map_err(|_| {
            failure(
                "invalid_google_response",
                "Google draft RAW is not valid base64url",
            )
        })?;
    Ok(GoogleDraftActionRemote {
        draft_id: remote.id,
        message_id: remote.message.id,
        thread_id: remote.message.thread_id,
        raw,
    })
}

#[cfg(test)]
mod tests;
