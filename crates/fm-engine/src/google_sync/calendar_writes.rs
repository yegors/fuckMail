//! Explicit Calendar writes. Read reconciliation never repeats a mutation.
use super::{failure, SyncContext};
use fm_calendar::{
    edits::GoogleWrite,
    google_api::{ApiError, EventFetch, EventResource, NotificationPolicy},
};
use fm_core::{
    live_calendar_writes::{CalendarWriteLease, CalendarWriteOutcome},
    Result,
};
use fm_google::{scheduler::WorkClass, transport::TransportError, GoogleError};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::OwnedSemaphorePermit;
type Memory = Arc<OwnedSemaphorePermit>;

pub(crate) async fn execute(ctx: &SyncContext, request: Value) -> Result<Value> {
    let request_id = text(&request, "request_id")?.to_owned();
    let account = ctx.account.as_str().to_owned();
    let prepared = ctx
        .core_call(move |s| s.calendar_live_prepare(&account, &request))
        .await?;
    if !matches!(prepared["state"].as_str(), Some("prepared" | "ready")) {
        return Ok(prepared);
    }
    let target = &prepared["target"];
    let calendar = text(target, "calendar")?.to_owned();
    let event = text(target, "event")?.to_owned();
    let kind = text(&prepared, "kind")?.to_owned();
    let version = prepared["revision"]
        .as_i64()
        .ok_or_else(|| failure("invalid_request", "Calendar intent revision is invalid"))?;
    let epoch = refresh_permission(ctx, &calendar).await?;
    let (source, memory) = if kind == "create" {
        (None, Arc::new(ctx.calendar_prepare_permit().await?))
    } else {
        let (read, memory) = ctx.calendar_request(WorkClass::UserRead).await?;
        let source = match read
            .get_event("engine-managed", &calendar, &event, None, &ctx.cancel)
            .await
            .map_err(read_error)?
        {
            EventFetch::Found(resource) => event_source(resource),
            EventFetch::NotModified => {
                return Err(failure(
                    "calendar_response_invalid",
                    "Fresh Calendar source is required",
                ))
            }
        };
        (Some(source), Arc::new(memory))
    };
    let account = ctx.account.as_str().to_owned();
    let id = request_id.clone();
    let generation = ctx.connection_generation;
    memory_core(ctx, &memory, move |s| {
        s.calendar_live_stage(&account, &id, version, generation, epoch, source)
    })
    .await?;
    drop(memory);
    // No active HTTP lease or decoded event is held during mutation cooldown.
    // Reload the frozen preflight only after both quota and memory admission.
    let api = ctx
        .calendar_mutation_request(if matches!(kind.as_str(), "edit" | "rsvp") {
            3
        } else {
            1
        })
        .await?;
    let memory = Arc::new(ctx.calendar_prepare_permit().await?);
    let account = ctx.account.as_str().to_owned();
    let id = request_id.clone();
    let lease = memory_core(ctx, &memory, move |s| {
        s.calendar_live_start(&account, &id, generation)
    })
    .await?;
    let response = match kind.as_str() {
        "create" => api
            .insert_event(
                "engine-managed",
                &calendar,
                &event,
                &lease.dispatch["body"],
                NotificationPolicy::All,
                &ctx.cancel,
            )
            .await
            .map(|v| CalendarWriteOutcome::Accepted(event_source(v))),
        "delete" => api
            .delete_event(
                "engine-managed",
                &calendar,
                &event,
                text(&lease.dispatch, "if_match")?,
                NotificationPolicy::All,
                &ctx.cancel,
            )
            .await
            .map(|_| CalendarWriteOutcome::Deleted),
        "edit" | "rsvp" => {
            let write = GoogleWrite {
                target: serde_json::from_value(lease.dispatch["target"].clone())
                    .map_err(|_| failure("invalid_request", "Invalid stored Calendar target"))?,
                if_match: text(&lease.dispatch, "if_match")?.into(),
                body: lease.dispatch["body"].clone(),
                send_updates: "all",
                notification_owner: "google_calendar_api",
            };
            api.apply_write("engine-managed", &write, &ctx.cancel)
                .await
                .map(|v| CalendarWriteOutcome::Accepted(event_source(v)))
        }
        _ => {
            return Err(failure(
                "invalid_request",
                "Invalid stored Calendar operation",
            ))
        }
    };
    let outcome = response.unwrap_or_else(write_error);
    let saved = lease.clone();
    // Unlike preflight, this call must finish after the caller cancels or the
    // account reconnects. The exact stored attempt is validated by Core.
    let held = memory.clone();
    let (receipt, _held) = ctx
        .core
        .call(move |s| Ok((s.calendar_live_record_outcome(&saved, outcome), held)))
        .await?;
    publish_if_possible(ctx, &lease, receipt?, &memory).await
}

pub(crate) async fn reconcile(ctx: &SyncContext, request_id: &str) -> Result<Value> {
    let current = status(ctx, request_id).await?;
    if matches!(current["state"].as_str(), Some("accepted" | "reconciled")) {
        let memory = Arc::new(ctx.calendar_prepare_permit().await?);
        let op = operation(ctx, request_id, &memory).await?;
        return match op.lease {
            Some(lease) => publish_if_possible(ctx, &lease, current, &memory).await,
            None => Ok(current),
        };
    }
    if !matches!(current["state"].as_str(), Some("in_flight" | "unknown")) {
        return Ok(current);
    }
    let calendar = text(&current["target"], "calendar")?;
    // A current permission observation is required even for read reconciliation.
    refresh_permission(ctx, calendar).await?;
    let (api, memory) = ctx.calendar_request(WorkClass::UserRead).await?;
    let memory = Arc::new(memory);
    let op = operation(ctx, request_id, &memory).await?;
    let Some(lease) = op.lease else {
        return status(ctx, request_id).await;
    };
    if !matches!(op.state.as_str(), "in_flight" | "unknown") {
        return status(ctx, request_id).await;
    }
    let saved = lease.clone();
    let generation = ctx.connection_generation;
    let fence = memory_core(ctx, &memory, move |s| {
        s.calendar_live_reconcile_begin(&saved, generation)
    })
    .await?;
    let source = match api
        .get_event(
            "engine-managed",
            calendar,
            text(&lease.dispatch["target"], "event")?,
            None,
            &ctx.cancel,
        )
        .await
    {
        Ok(EventFetch::Found(resource)) => event_source(resource),
        Ok(EventFetch::NotModified) => {
            return Err(failure(
                "calendar_response_invalid",
                "Fresh Calendar source is required",
            ))
        }
        Err(ApiError::Provider(GoogleError::NotFound)) => return status(ctx, request_id).await,
        Err(error) => return Err(read_error(error)),
    };
    let saved = lease.clone();
    let held = memory.clone();
    let (receipt, _held) = ctx
        .core
        .call(move |s| Ok((s.calendar_live_reconcile_read(&saved, &fence, source), held)))
        .await?;
    // The read has its own current publication fence; the original mutation's
    // immutable dispatch lease remains unchanged for any late provider reply.
    publish_if_possible(ctx, &lease, receipt?, &memory).await
}

fn event_source(mut event: EventResource) -> Value {
    // The adapter validates matching body/header ETags; retain header-only
    // versions for Core's conditional-write boundary as well.
    if event.source.get("etag").is_none() {
        event.source["etag"] = event.etag.into();
    }
    event.source
}

async fn publish_if_possible(
    ctx: &SyncContext,
    lease: &CalendarWriteLease,
    receipt: Value,
    memory: &Memory,
) -> Result<Value> {
    if !matches!(receipt["state"].as_str(), Some("accepted" | "reconciled"))
        || ctx.cancel.is_cancelled()
    {
        return Ok(receipt);
    }
    let saved = lease.clone();
    let runs = match memory_core(ctx, memory, move |s| s.calendar_live_expire_staging(&saved)).await
    {
        Ok(runs) => runs,
        Err(error)
            if matches!(
                error.code.as_str(),
                "stale_revision" | "stale_google_work" | "permission_denied" | "cancelled"
            ) =>
        {
            return Ok(receipt)
        }
        Err(error) => return Err(error),
    };
    for run in runs {
        loop {
            let run = run.clone();
            match memory_core(ctx, memory, move |s| s.calendar_finalize_chunk(&run)).await {
                Ok(0) => break,
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.code.as_str(),
                        "stale_revision" | "stale_google_work" | "permission_denied" | "cancelled"
                    ) =>
                {
                    return Ok(receipt)
                }
                Err(error) => return Err(error),
            }
        }
    }
    let saved = lease.clone();
    match memory_core(ctx, memory, move |s| s.calendar_live_publish(&saved)).await {
        Ok(value) => Ok(value),
        Err(error)
            if matches!(
                error.code.as_str(),
                "calendar_sync_pending"
                    | "stale_revision"
                    | "stale_google_work"
                    | "permission_denied"
                    | "cancelled"
            ) =>
        {
            Ok(receipt)
        }
        Err(error) => Err(error),
    }
}
async fn refresh_permission(ctx: &SyncContext, calendar: &str) -> Result<i64> {
    let account = ctx.account.as_str().to_owned();
    let id = calendar.to_owned();
    let before = ctx
        .core_call(move |s| s.calendar_sync_scope(&account, &id))
        .await?;
    let epoch = before["permission_epoch"].as_i64().ok_or_else(|| {
        failure(
            "invalid_request",
            "Calendar permission checkpoint is invalid",
        )
    })?;
    let (api, memory) = ctx.calendar_request(WorkClass::UserRead).await?;
    let memory = Arc::new(memory);
    let entry = api
        .calendar_entry("engine-managed", calendar, &ctx.cancel)
        .await
        .map_err(read_error)?;
    let account = ctx.account.as_str().to_owned();
    let generation = ctx.connection_generation;
    memory_core(ctx, &memory, move |s| {
        s.calendar_observe_catalog_entry(&account, &entry.source, generation, epoch)
    })
    .await
}
async fn operation(
    ctx: &SyncContext,
    id: &str,
    memory: &Memory,
) -> Result<fm_core::live_calendar_writes::CalendarWriteOperation> {
    let account = ctx.account.as_str().to_owned();
    let id = id.to_owned();
    memory_core(ctx, memory, move |s| {
        s.calendar_live_operation(&account, &id)
    })
    .await
}

/// Both the queued closure and its reply carry a share of the memory permit.
/// Cancelling the awaiting future cannot release capacity while its source or
/// returned lease still lives on the writer queue or in an undelivered reply.
async fn memory_core<T, F>(ctx: &SyncContext, memory: &Memory, operation: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&mut fm_core::Store) -> Result<T> + Send + 'static,
{
    let held = memory.clone();
    let cancel = ctx.cancel.clone();
    let account = ctx.account.as_str().to_owned();
    let generation = ctx.connection_generation;
    // A cancellation-select against this reply can lose a committed dispatch
    // lease. Complete the short writer call so its exact attempt is recorded.
    let (result, _held) = ctx
        .core
        .call(move |store| {
            if cancel.is_cancelled() {
                return Err(failure("cancelled", "Calendar work stopped"));
            }
            if !store.google_account_bindings()?.iter().any(|binding| {
                binding.account == account
                    && binding.connection_generation == generation
                    && binding.state == fm_core::live_profile::ConnectionState::Connected
            }) {
                return Err(failure("stale_google_work", "Calendar connection changed"));
            }
            Ok((operation(store), held))
        })
        .await?;
    result
}
async fn status(ctx: &SyncContext, id: &str) -> Result<Value> {
    let account = ctx.account.as_str().to_owned();
    let id = id.to_owned();
    ctx.core_call(move |s| s.calendar_live_status(&account, &id))
        .await
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .ok_or_else(|| failure("invalid_request", "Invalid Calendar operation field"))
}
fn write_error(error: ApiError) -> CalendarWriteOutcome {
    match error {
        ApiError::Transport(TransportError::Cooldown { retry_after_secs }) => {
            CalendarWriteOutcome::UndispatchedCooldown { retry_after_secs }
        }
        ApiError::PreconditionFailed => CalendarWriteOutcome::Conflict,
        // A duplicate stable ID can be an earlier successful insert. Reconcile.
        ApiError::Conflict => CalendarWriteOutcome::Unknown,
        ApiError::InvalidRequest => CalendarWriteOutcome::Rejected("invalid_request".into()),
        ApiError::Provider(GoogleError::PermissionDenied) => {
            CalendarWriteOutcome::Rejected("permission_denied".into())
        }
        ApiError::Provider(GoogleError::ReauthorizationRequired) => {
            CalendarWriteOutcome::Rejected("authentication_required".into())
        }
        ApiError::Provider(GoogleError::RateLimited { .. }) => {
            CalendarWriteOutcome::Rejected("rate_limited".into())
        }
        ApiError::Provider(GoogleError::NotFound) => {
            CalendarWriteOutcome::Rejected("not_found".into())
        }
        _ => CalendarWriteOutcome::Unknown,
    }
}
fn read_error(error: ApiError) -> fm_core::CoreError {
    match error {
        ApiError::Provider(GoogleError::PermissionDenied | GoogleError::NotFound) => {
            failure("permission_denied", "Calendar source is unavailable")
        }
        ApiError::Provider(GoogleError::RateLimited { .. })
        | ApiError::Transport(TransportError::Cooldown { .. }) => failure(
            "google_rate_limited",
            "Google is limiting Calendar requests",
        ),
        _ => failure(
            "calendar_response_invalid",
            "Calendar preflight could not complete",
        ),
    }
}

#[cfg(test)]
#[path = "calendar_writes_tests.rs"]
mod tests;
