//! Locally authored Google twins. One durable attempt, then identity-bound RAW.
use super::{failure, mail::Step, SyncContext};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use fm_core::{
    google_drafts::{
        GoogleDraftIntent, GoogleDraftOutcome, GoogleDraftRemote, GoogleDraftScope, GoogleDraftStep,
    },
    CoreError, Result,
};
use fm_google::{
    gmail_api::{GmailCall, GmailMethod, GmailReply},
    transport::CancellationToken,
    GoogleError,
};
use serde_json::{json, Value};
use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use tokio::sync::OwnedSemaphorePermit;

/// A shared Gmail preparation permit; store calls and replies that carry
/// draft bytes hold a clone.
pub(super) type Memory = Arc<OwnedSemaphorePermit>;

/// A changed draft uploads once its text has rested this long: typing never
/// uploads every few keystrokes, and leaving a draft uploads it shortly after.
const QUIET_MS: i64 = if cfg!(test) { 0 } else { 30_000 };

/// One step of the draft worker, in priority order: a due account-transfer
/// cleanup, then the next due draft (an upload, or a read-back of an
/// acknowledged one), then a discarded draft to delete in Gmail, then a
/// conflict to resolve.
pub(crate) async fn step(ctx: &SyncContext) -> Result<Step> {
    if super::draft_transfers::step(ctx).await? {
        return Ok(Step::Progress);
    }
    let account = ctx.account.as_str().to_owned();
    let generation = ctx.connection_generation;
    let due = ctx
        .core_call(move |store| store.google_draft_due_after(&account, generation, 1, QUIET_MS))
        .await?;
    let Some(due) = due.into_iter().next() else {
        if delete_discarded(ctx).await? || resolve_conflict(ctx).await? {
            return Ok(Step::Progress);
        }
        return Ok(Step::Idle(Duration::from_secs(1)));
    };
    if due.state == "acknowledged" {
        reconcile(ctx, &due.draft_id).await?;
    } else {
        let request = due.request.unwrap_or_else(|| {
            json!({"draft_id":due.draft_id,"expected_revision":due.revision,
            "request_id":fm_ipc::random_token()})
        });
        let reply = match execute(ctx, request).await {
            Ok(reply) => reply,
            // Edited again while its upload was being prepared: the newer
            // text uploads once it rests. Not a sync failure.
            Err(error) if error.code == "stale_revision" => return Ok(Step::Progress),
            Err(error) => return Err(error),
        };
        if reply["state"] == "prepared" {
            return Ok(Step::Idle(Duration::from_secs(1)));
        }
    }
    Ok(Step::Progress)
}

/// A draft discarded here is deleted in Gmail once its Undo has passed.
async fn delete_discarded(ctx: &SyncContext) -> Result<bool> {
    let account = ctx.account.as_str().to_owned();
    let Some((draft, target)) = ctx
        .core_call(move |store| store.google_draft_discard_due(&account))
        .await?
    else {
        return Ok(false);
    };
    let (api, _permit, dispatched) = ctx.gmail_delete_request(&target).await?;
    let call = GmailCall::delete_draft(&ctx.account, &target).map_err(provider)?;
    let reply = api.execute(call, "engine-managed", &ctx.cancel).await;
    let gone = dispatched.load(Ordering::Acquire)
        && matches!(
            reply,
            Ok(GmailReply::DraftDeletion(ref d))
                if matches!(d.result, fm_google::gmail_api::DraftDeletionResult::Deleted
                    | fm_google::gmail_api::DraftDeletionResult::Absent)
        );
    if !gone {
        // Try again on a later pass; the draft stays discarded here either way.
        return match reply {
            Err(error) => Err(provider(error)),
            _ => Err(failure(
                "google_draft_delete_failed",
                "Gmail did not delete a discarded draft",
            )),
        };
    }
    let account = ctx.account.as_str().to_owned();
    ctx.core_call(move |store| store.google_draft_remote_deleted(&account, &draft, &target))
        .await?;
    Ok(true)
}

/// Local edits win a conflict: they replace the changed Gmail copy, or, when
/// Gmail no longer has the draft, upload as a new one.
async fn resolve_conflict(ctx: &SyncContext) -> Result<bool> {
    let account = ctx.account.as_str().to_owned();
    let Some(conflict) = ctx
        .core_call(move |store| store.google_draft_conflicted(&account))
        .await?
    else {
        return Ok(false);
    };
    let draft = text(&conflict, "draft_id")?.to_owned();
    let google_revision = conflict["google_revision"].as_i64().unwrap_or(0);
    if conflict["observed"] == true {
        execute(
            ctx,
            json!({"draft_id":draft,"expected_revision":conflict["revision"],
                "request_id":fm_ipc::random_token(),"expected_google_revision":google_revision,
                "action":"replace_local"}),
        )
        .await?;
    } else {
        let account = ctx.account.as_str().to_owned();
        ctx.core_call(move |store| {
            store.google_draft_forget_target(&account, &draft, google_revision)
        })
        .await?;
    }
    Ok(true)
}

/// Creates or updates the Gmail copy of a local draft with one durable
/// attempt, then reads it back. Serves `google_draft_sync`,
/// `google_draft_resolve` and the draft worker's own uploads.
///
/// A replayed `request_id` returns its recorded outcome. An upload whose
/// reply is lost is left `unknown` for read-back, never sent twice.
pub(crate) async fn execute(ctx: &SyncContext, request: Value) -> Result<Value> {
    validate(&request, true)?;
    let id = text(&request, "draft_id")?.to_owned();
    let scope = verified_scope(ctx).await?;
    // Validate exact request identity before considering current twin state.
    // This metadata-only call retains no authored body while quota is queued.
    let initial_target = match intent(ctx, &scope, &request).await? {
        GoogleDraftIntent::Reply(reply) => return replay_reply(ctx, &scope, &request, reply).await,
        GoogleDraftIntent::Upload { provider_draft_id } => provider_draft_id,
    };
    // A linked Gmail copy is read first. If it changed there, that is a
    // conflict, unless this request explicitly replaces it with the local copy.
    let current = status(ctx, &id).await?;
    if initial_target.is_some() || provider_id(&current).is_some() {
        let current = observe(ctx, &scope, &id).await?;
        if current["state"] == "conflict" && request["action"] != "replace_local" {
            return Ok(current);
        }
    }
    let target = match intent(ctx, &scope, &request).await? {
        GoogleDraftIntent::Reply(reply) => return replay_reply(ctx, &scope, &request, reply).await,
        GoogleDraftIntent::Upload { provider_draft_id } => provider_draft_id,
    };
    let method = if target.is_some() {
        GmailMethod::DraftUpdate
    } else {
        GmailMethod::DraftCreate
    };
    let (api, permit, dispatched) = ctx.gmail_draft_request(method).await?;
    let memory = Arc::new(permit);
    // Another owned caller may have completed work while this admission waited.
    // Refuse a different target before freezing bytes or claiming an operation.
    match intent(ctx, &scope, &request).await? {
        GoogleDraftIntent::Reply(reply) => return Ok(reply),
        GoogleDraftIntent::Upload { provider_draft_id } if provider_draft_id == target => {}
        _ => {
            return Err(failure(
                "stale_google_work",
                "The draft target changed while awaiting quota",
            ))
        }
    }
    let frozen_scope = scope.clone();
    let begin_request = request.clone();
    let google_revision = status(ctx, &id).await?["revision"]
        .as_i64()
        .ok_or_else(|| {
            failure(
                "invalid_draft_state",
                "Google draft has no current revision",
            )
        })?;
    // Freeze the MIME and journal the operation. A draft that can never be
    // uploaded is recorded as failed; a transient local error defers it.
    let begun = memory_core(ctx, &memory, move |store| {
        store.google_draft_begin(&frozen_scope, &begin_request)
    })
    .await?;
    let prepared = match begun {
        GoogleDraftStep::Reply(reply) => reply,
        GoogleDraftStep::Prepare(plan) => {
            if plan.provider_draft_id() != target.as_deref() {
                return Err(failure(
                    "stale_google_work",
                    "The draft target changed before MIME preparation",
                ));
            }
            let prepared_result = prepare(ctx, &memory, move |cancel| {
                fm_core::google_drafts::prepare_google_draft(plan, &cancel)
            })
            .await;
            let proof = match prepared_result {
                Ok(proof) => proof,
                Err(error)
                    if request["action"] != "replace_local"
                        && matches!(error.code.as_str(), "invalid_draft" | "limit_exceeded") =>
                {
                    let scope = scope.clone();
                    let code = error.code;
                    return memory_core(ctx, &memory, move |store| {
                        store.google_draft_prepare_failed(&scope, &request, google_revision, &code)
                    })
                    .await;
                }
                Err(error)
                    if request["action"] != "replace_local" && local_retry_error(&error.code) =>
                {
                    let scope = scope.clone();
                    let code = error.code;
                    return memory_core(ctx, &memory, move |store| {
                        store.google_draft_defer(&scope, &request, google_revision, &code, 1)
                    })
                    .await;
                }
                Err(error) => return Err(error),
            };
            memory_core(ctx, &memory, move |store| store.google_draft_finish(proof)).await?
        }
    };
    if prepared["state"] != "prepared" {
        return Ok(prepared);
    }
    let operation_revision = prepared["operation"]["revision"].as_i64().ok_or_else(|| {
        failure(
            "invalid_draft_state",
            "The prepared Google operation has no revision",
        )
    })?;
    let request_id = text(&prepared["operation"], "request_id")?.to_owned();
    let claim_scope = scope.clone();
    let lease = memory_core(ctx, &memory, move |store| {
        store.google_draft_claim(&claim_scope, &request_id, operation_revision)
    })
    .await?;
    // The exact lease must receive an outcome even if preparation/token loading
    // fails after its claim. It must never acquire a second quota reservation.
    let plan = lease.take_upload();
    let uploaded = prepare(ctx, &memory, move |cancel| {
        fm_core::google_drafts::load_google_draft_upload(plan, &cancel)
    })
    .await;
    let deferred_code = uploaded
        .as_ref()
        .err()
        .filter(|error| local_retry_error(&error.code))
        .map(|error| error.code.clone());
    let response = match uploaded {
        Ok(bytes) => {
            let call = if let Some(provider) = lease.provider_draft_id() {
                GmailCall::update_draft(&ctx.account, provider, &bytes, lease.thread_id())
            } else {
                GmailCall::create_draft(&ctx.account, &bytes, lease.thread_id())
            };
            match call {
                Ok(call) if call.method() == method => api
                    .execute(call, "engine-managed", &ctx.cancel)
                    .await
                    .map_err(provider),
                Ok(_) => Err(failure(
                    "stale_google_work",
                    "The draft target changed before dispatch",
                )),
                Err(error) => Err(provider(error)),
            }
        }
        Err(error) => Err(error),
    };
    // Nothing sent is `Undispatched`; a permission refusal is `Rejected`;
    // anything else after dispatch without a draft reply is `Unknown`.
    let outcome = match response {
        Ok(GmailReply::Draft(draft)) => GoogleDraftOutcome::Acknowledged {
            draft_id: draft.id,
            message_id: draft.message.id,
        },
        Ok(_) => GoogleDraftOutcome::Unknown {
            code: "invalid_google_response".into(),
        },
        Err(error) if !dispatched.load(Ordering::Acquire) => {
            GoogleDraftOutcome::Undispatched { code: error.code }
        }
        Err(error) if error.code == "google_permission_denied" => {
            GoogleDraftOutcome::Rejected { code: error.code }
        }
        Err(error) => GoogleDraftOutcome::Unknown { code: error.code },
    };
    // Record through the plain bridge, not `memory_core`: the claimed attempt's
    // outcome must be stored even if the job was cancelled or the account
    // reconnected meanwhile. The store validates the lease itself.
    let saved = lease.clone();
    let held = memory.clone();
    let (receipt, _held) = ctx
        .core
        .call(move |store| Ok((store.google_draft_outcome(&saved, outcome), held)))
        .await?;
    let mut receipt = receipt?;
    drop(_held);
    if let Some(code) = deferred_code {
        if receipt["state"] == "prepared" && !ctx.cancel.is_cancelled() {
            let scope = scope.clone();
            let request = json!({"draft_id":id,"expected_revision":receipt["local_revision"]});
            let revision = receipt["revision"].as_i64().ok_or_else(|| {
                failure(
                    "invalid_draft_state",
                    "The current Google twin revision is missing",
                )
            })?;
            receipt = memory_core(ctx, &memory, move |store| {
                store.google_draft_defer(&scope, &request, revision, &code, 1)
            })
            .await?;
        }
    }
    drop(memory);
    drop(api);
    if receipt["state"] == "acknowledged" && !ctx.cancel.is_cancelled() {
        // Read failures leave the durable ACK for read-only owned recovery.
        match observe(ctx, &scope, &id).await {
            Ok(value) => Ok(value),
            Err(error) if matches!(error.code.as_str(), "cancelled" | "stale_google_work") => {
                Ok(receipt)
            }
            Err(error) => Err(error),
        }
    } else {
        Ok(receipt)
    }
}

/// Local failures that defer the draft briefly instead of failing the sync.
fn local_retry_error(code: &str) -> bool {
    matches!(code, "content_unavailable" | "storage_error")
}

/// Asks the store what `request` means now: a recorded reply to replay, or an
/// upload to a known Gmail draft (update) or a new one (create).
async fn intent(
    ctx: &SyncContext,
    scope: &GoogleDraftScope,
    request: &Value,
) -> Result<GoogleDraftIntent> {
    let scope = scope.clone();
    let request = request.clone();
    ctx.core_call(move |store| store.google_draft_intent(&scope, &request))
        .await
}

/// Returns a replayed request's recorded reply, reading the Gmail copy back
/// first when that upload was acknowledged or a later request has synced it.
async fn replay_reply(
    ctx: &SyncContext,
    scope: &GoogleDraftScope,
    request: &Value,
    reply: Value,
) -> Result<Value> {
    if reply["operation_is_current"] == false {
        return Ok(reply);
    }
    if reply["state"] == "acknowledged"
        || (reply["state"] == "synced" && reply["operation"]["request_id"] != request["request_id"])
    {
        observe(ctx, scope, text(request, "draft_id")?).await
    } else {
        Ok(reply)
    }
}

/// Handles `google_draft_reconcile`: reads the Gmail copy back to settle the
/// draft's state.
pub(crate) async fn reconcile(ctx: &SyncContext, draft_id: &str) -> Result<Value> {
    let scope = verified_scope(ctx).await?;
    observe(ctx, &scope, draft_id).await
}

/// Reads the linked Gmail draft's RAW source and records what Google holds,
/// which settles an acknowledged or unknown upload or reveals a conflict.
/// Fails with `reconciliation_unavailable` when a create never learned its
/// Gmail draft id.
pub(super) async fn observe(
    ctx: &SyncContext,
    scope: &GoogleDraftScope,
    draft_id: &str,
) -> Result<Value> {
    let observe_scope = scope.clone();
    let id = draft_id.to_owned();
    let (plan, local_revision, google_revision) = ctx
        .core_call(move |store| {
            let plan = store.google_draft_observe_begin(&observe_scope, &id)?;
            let stamp = store.google_draft_status(&observe_scope.account, &id)?;
            let local = stamp["local_revision"].as_i64().ok_or_else(|| {
                failure(
                    "invalid_draft_state",
                    "The observed local draft revision is missing",
                )
            })?;
            let google = stamp["revision"].as_i64().ok_or_else(|| {
                failure(
                    "invalid_draft_state",
                    "The observed Google draft revision is missing",
                )
            })?;
            Ok((plan, local, google))
        })
        .await?;
    let remote_id = plan
        .provider_draft_id()
        .ok_or_else(|| {
            failure(
                "reconciliation_unavailable",
                "The unconfirmed create has no known Google draft identity",
            )
        })?
        .to_owned();
    let call = GmailCall::get_draft(&ctx.account, &remote_id).map_err(provider)?;
    let (remote, permit) = match ctx.gmail_prepared(call).await {
        Ok((GmailReply::Draft(draft), permit)) => (Some(draft), permit),
        Ok(_) => {
            return Err(failure(
                "invalid_google_response",
                "Expected the exact Google draft RAW response",
            ))
        }
        Err(GoogleError::NotFound) => (None, ctx.prepare_permit().await?),
        Err(error) => return Err(provider(error)),
    };
    let memory = Arc::new(permit);
    let prepared = prepare(ctx, &memory, move |cancel| {
        let remote = remote
            .map(|draft| {
                let raw = URL_SAFE_NO_PAD
                    .decode(
                        draft
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
                Ok::<_, CoreError>(GoogleDraftRemote {
                    draft_id: draft.id,
                    message_id: draft.message.id,
                    thread_id: draft.message.thread_id,
                    raw,
                })
            })
            .transpose()?;
        fm_core::google_drafts::prepare_google_draft_observation(plan, remote, &cancel)
    })
    .await;
    let proof = match prepared {
        Ok(proof) => proof,
        Err(error) if local_retry_error(&error.code) => {
            let current = status(ctx, draft_id).await?;
            if matches!(
                current["state"].as_str(),
                Some("local" | "synced" | "prepared" | "acknowledged")
            ) {
                let scope = scope.clone();
                let code = error.code.clone();
                let request = json!({"draft_id":draft_id,"expected_revision":local_revision});
                // Keep the exact ACK/unsent operation. A short deferral longer
                // than the ordinary error retry lets other local drafts run.
                memory_core(ctx, &memory, move |store| {
                    store.google_draft_defer(&scope, &request, google_revision, &code, 5)
                })
                .await?;
            }
            return Err(error);
        }
        Err(error) => return Err(error),
    };
    memory_core(ctx, &memory, move |store| {
        store.google_draft_observe_finish(proof)
    })
    .await
}

/// Reads the Gmail profile so draft work is bound to the mailbox's current
/// primary address as well as this connection generation.
pub(super) async fn verified_scope(ctx: &SyncContext) -> Result<GoogleDraftScope> {
    let profile = match ctx
        .gmail(GmailCall::profile(&ctx.account).map_err(provider)?)
        .await
        .map_err(provider)?
    {
        GmailReply::Profile(profile) => profile,
        _ => {
            return Err(failure(
                "invalid_google_response",
                "Expected the primary Gmail mailbox identity",
            ))
        }
    };
    Ok(GoogleDraftScope {
        account: ctx.account.as_str().to_owned(),
        connection_generation: ctx.connection_generation,
        primary_email: profile.email_address,
    })
}

async fn status(ctx: &SyncContext, id: &str) -> Result<Value> {
    let account = ctx.account.as_str().to_owned();
    let id = id.to_owned();
    ctx.core_call(move |store| store.google_draft_status(&account, &id))
        .await
}

/// The Gmail draft id linked to this draft, if any: from the latest
/// acknowledgement, the synced baseline, or the effective target.
fn provider_id(status: &Value) -> Option<&str> {
    status["operation"]["ack"]["draft_id"]
        .as_str()
        .or_else(|| status["baseline"]["draft_id"].as_str())
        .or_else(|| status["effective_target"]["provider_draft_id"].as_str())
}

/// Runs CPU-bound draft preparation on the blocking pool while holding a
/// share of `memory`. A failed blocking task maps to `content_unavailable`.
pub(super) async fn prepare<T, F>(ctx: &SyncContext, memory: &Memory, operation: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(CancellationToken) -> Result<T> + Send + 'static,
{
    let held = memory.clone();
    let cancel = ctx.cancel.clone();
    tokio::task::spawn_blocking(move || {
        let _held = held;
        if cancel.is_cancelled() {
            return Err(failure("cancelled", "Google draft preparation stopped"));
        }
        operation(cancel)
    })
    .await
    .map_err(|_| failure("content_unavailable", "Google draft preparation failed"))?
}

/// Like [`SyncContext::core_call`], with the same cancellation and generation
/// checks on the writer, but the call and its reply hold a share of `memory`,
/// and a queued call is always awaited rather than abandoned on cancellation.
pub(super) async fn memory_core<T, F>(ctx: &SyncContext, memory: &Memory, operation: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&mut fm_core::Store) -> Result<T> + Send + 'static,
{
    let held = memory.clone();
    let cancel = ctx.cancel.clone();
    let account = ctx.account.as_str().to_owned();
    let generation = ctx.connection_generation;
    // Once queued, await this short writer call's actual result. Selecting
    // cancellation against the reply could discard a committed claim's lease
    // before the worker records its guaranteed-undispatched outcome.
    let (result, _held) = ctx
        .core
        .call(move |store| {
            if cancel.is_cancelled() {
                return Err(failure("cancelled", "Google draft work stopped"));
            }
            if !store.google_account_bindings()?.iter().any(|b| {
                b.account == account
                    && b.connection_generation == generation
                    && b.state == fm_core::live_profile::ConnectionState::Connected
            }) {
                return Err(failure(
                    "stale_google_work",
                    "Google draft connection changed",
                ));
            }
            Ok((operation(store), held))
        })
        .await?;
    result
}

/// Checks draft request fields: `draft_id` alone for a status read; with
/// `write`, also `expected_revision` and `request_id`, plus `action:
/// "replace_local"` and `expected_google_revision` to resolve a conflict.
pub(crate) fn validate(request: &Value, write: bool) -> Result<()> {
    let fields = request.as_object().ok_or_else(|| {
        failure(
            "invalid_request",
            "Google draft parameters must be an object",
        )
    })?;
    let resolving = fields.contains_key("action");
    let expected = if !write {
        1
    } else if resolving {
        5
    } else {
        3
    };
    let allowed = if !write {
        &["draft_id"][..]
    } else if resolving {
        &[
            "draft_id",
            "expected_revision",
            "request_id",
            "action",
            "expected_google_revision",
        ][..]
    } else {
        &["draft_id", "expected_revision", "request_id"][..]
    };
    if fields.len() != expected || fields.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(failure(
            "invalid_request",
            "Unexpected Google draft parameters",
        ));
    }
    text(request, "draft_id")?;
    if write {
        text(request, "request_id")?;
        if request["expected_revision"]
            .as_i64()
            .is_none_or(|revision| revision <= 0)
            || (resolving
                && (request["action"] != "replace_local"
                    || request["expected_google_revision"]
                        .as_i64()
                        .is_none_or(|revision| revision < 0)))
        {
            return Err(failure(
                "invalid_request",
                "Choose the current saved draft and conflict revision",
            ));
        }
    }
    Ok(())
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
        .ok_or_else(|| failure("invalid_request", "Choose an exact draft/request identity"))
}
/// Maps a Gmail error to a draft error code, keeping any retry-after hint.
pub(super) fn provider(error: GoogleError) -> CoreError {
    let code = match &error {
        GoogleError::ReauthorizationRequired => "google_reauthorization_required",
        GoogleError::PermissionDenied => "google_permission_denied",
        GoogleError::RateLimited { .. } => "google_rate_limited",
        GoogleError::ServerUnavailable => "google_server_unavailable",
        GoogleError::TransportInterrupted => "google_transport_interrupted",
        GoogleError::NotFound => "google_draft_missing",
        _ => "invalid_google_response",
    };
    let mut result = failure(code, "The Google draft result could not be verified");
    if let GoogleError::RateLimited { retry_after_secs } = error {
        result.details = json!({"retry_after_secs":retry_after_secs});
    }
    result
}

#[cfg(test)]
mod tests;
