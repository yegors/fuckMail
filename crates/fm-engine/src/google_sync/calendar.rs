//! Google Calendar read acquisition. All HTTP stays outside Core's one writer.
use super::SyncContext;
use fm_calendar::{
    google_api::{ApiError, SyncCursor},
    Access, Window,
};
use fm_core::{
    live_calendar::{
        prepare_calendar_catalog_chunk, prepare_calendar_catalog_entry, prepare_calendar_chunk,
        prepare_calendar_page, CalendarPage, CalendarSyncLease, CalendarSyncMode,
    },
    CoreError, Result,
};
use fm_google::{scheduler::WorkClass, transport::TransportError, GoogleError};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::OwnedSemaphorePermit;

#[derive(Clone, Copy)]
pub(crate) struct ReadGrants {
    pub(crate) catalog: bool,
    pub(crate) events: bool,
    pub(crate) busy: bool,
}
impl ReadGrants {
    pub(crate) fn from_scopes(scopes: &fm_google::auth::ScopeSet<'_>) -> Self {
        Self {
            catalog: scopes.calendar_catalog_read(),
            events: scopes.calendar_events_read(),
            busy: scopes.calendar_free_busy(),
        }
    }
}

#[derive(Default)]
pub(crate) struct PassSummary {
    pub(crate) more: bool,
    pub(crate) calendars: u64,
    pub(crate) pages: u64,
    pub(crate) failures: u64,
    pub(crate) first_failure: Option<String>,
}

/// One durable event page per calendar gives every calendar a turn before the
/// coordinator immediately schedules another backfill pass. Metadata refresh is
/// independent of event inventory size and deliberately independent of deltas.
pub(crate) async fn run_pass(ctx: &SyncContext, refresh_catalog: bool) -> Result<PassSummary> {
    let mut summary = PassSummary::default();
    let account = ctx.account.as_str().to_owned();
    let checkpoint = ctx
        .core_call(move |s| s.calendar_catalog_checkpoint(&account))
        .await?;
    if ctx.calendar_reads.catalog && (refresh_catalog || checkpoint["sync_token"].is_null()) {
        summary.pages += sync_catalog(ctx).await?;
    }
    let mut after = None;
    loop {
        let account = ctx.account.as_str().to_owned();
        let cursor = after.clone();
        let scopes = ctx
            .core_call(move |s| s.calendar_sync_scopes(&account, cursor.as_deref(), 100))
            .await?;
        for scope in scopes["scopes"].as_array().ok_or_else(invalid_checkpoint)? {
            let calendar = text(scope, "calendar_id")?.to_owned();
            summary.calendars += 1;
            if !ctx.calendar_reads.events {
                continue;
            }
            let result = event_turn(ctx, &calendar, refresh_catalog).await;
            match result {
                Ok((pages, more)) => {
                    summary.pages += pages;
                    summary.more |= more;
                }
                Err(error) if account_failure(&error) => return Err(error),
                Err(error) => {
                    summary.failures += 1;
                    if summary.first_failure.is_none() {
                        summary.first_failure = Some(error.code);
                    }
                }
            }
        }
        after = scopes["continuation"].as_str().map(str::to_owned);
        if after.is_none() {
            break;
        }
    }
    Ok(summary)
}

async fn sync_catalog(ctx: &SyncContext) -> Result<u64> {
    let mut pages = 0;
    let mut force_full = false;
    // One explicit replacement after expiry/replay drift. Persistent failures
    // return to the coordinator's backoff instead of spinning on the provider.
    for replacement in 0..2 {
        let lease = begin(ctx, "", force_full).await?;
        loop {
            let progress = progress(ctx, &lease).await?;
            if progress["final_page_staged"] == true {
                finalize_catalog(ctx, &lease).await?;
                let l = lease.clone();
                ctx.core_call(move |s| s.calendar_publish_sync(&l)).await?;
                return Ok(pages);
            }
            let cursor = cursor(&progress)?;
            let (api, memory) = ctx.calendar_request(WorkClass::DeltaRead).await?;
            let response = api
                .calendar_list("engine-managed", cursor, &ctx.cancel)
                .await;
            let response = match response {
                Ok(page) => page,
                Err(ApiError::SyncTokenExpired) if replacement == 0 => {
                    expire(ctx, &lease).await?;
                    force_full = true;
                    break;
                }
                Err(error) => return Err(api_error(error)),
            };
            let page = CalendarPage {
                input_cursor: cursor.page_token.map(str::to_owned),
                items: response
                    .items
                    .into_iter()
                    .map(|entry| entry.source)
                    .collect(),
                next_page_token: response.next_page_token,
                next_sync_token: response.next_sync_token,
            };
            match stage(ctx, &lease, page, memory).await {
                Ok(()) => pages += 1,
                Err(error) if error.code == "stale_page" && replacement == 0 => {
                    expire(ctx, &lease).await?;
                    force_full = true;
                    break;
                }
                Err(error) => return Err(error),
            }
        }
    }
    Err(failure(
        "calendar_resync_required",
        "Calendar catalog needs another synchronization attempt",
    ))
}

async fn event_turn(
    ctx: &SyncContext,
    calendar: &str,
    refresh_permission: bool,
) -> Result<(u64, bool)> {
    cleanup(ctx, calendar).await?;
    let initial = scope(ctx, calendar).await?;
    if refresh_permission && ctx.calendar_reads.catalog && initial["membership_denied"] != true {
        refresh_permission_for(ctx, calendar).await?;
    }
    let current_scope = scope(ctx, calendar).await?;
    if matches!(text(&current_scope, "access")?, "none" | "free_busy") {
        if let Some(lease) = resume(ctx, calendar).await? {
            expire(ctx, &lease).await?;
        }
        return Ok((0, false));
    }
    if !refresh_permission
        && current_scope["sync_token"].is_string()
        && resume(ctx, calendar).await?.is_none()
    {
        // Immediate continuation passes exist for unfinished inventories. A
        // completed small calendar waits for the ordinary delta cadence.
        return Ok((0, false));
    }
    let mut force_full = false;
    for replacement in 0..2 {
        let lease = begin(ctx, calendar, force_full).await?;
        let progress = progress(ctx, &lease).await?;
        if progress["final_page_staged"] == true {
            publish_events(ctx, &lease).await?;
            return Ok((0, false));
        }
        let cursor = cursor(&progress)?;
        let class = if progress["mode"] == "full" {
            WorkClass::BackfillRead
        } else {
            WorkClass::DeltaRead
        };
        // Capture the policy before HTTP, so an overlapping fresher observation
        // cannot be overwritten by this older response's accessRole or zone.
        let before = scope(ctx, calendar).await?;
        let (api, memory) = ctx.calendar_request(class).await?;
        let response = api
            .events("engine-managed", calendar, cursor, &ctx.cancel)
            .await;
        let response = match response {
            Ok(page) => page,
            Err(ApiError::SyncTokenExpired) if replacement == 0 => {
                expire(ctx, &lease).await?;
                force_full = true;
                continue;
            }
            Err(ApiError::Provider(GoogleError::PermissionDenied | GoogleError::NotFound)) => {
                deny(ctx, calendar, &before).await?;
                expire(ctx, &lease).await?;
                return Ok((0, false));
            }
            Err(error) => return Err(api_error(error)),
        };
        let access = response.role.event_access(false);
        if access_name(access) != before["access"].as_str().unwrap_or("")
            || response.time_zone != before["time_zone"].as_str().unwrap_or("")
        {
            let account = ctx.account.as_str().to_owned();
            let calendar = calendar.to_owned();
            let generation = ctx.connection_generation;
            let epoch = integer(&before, "permission_epoch")?;
            let zone = response.time_zone.clone();
            drop(response);
            ctx.core_call(move |s| {
                let result = s.calendar_observe_permission(
                    &account, &calendar, generation, epoch, access, &zone,
                );
                drop(memory);
                result
            })
            .await?;
            expire(ctx, &lease).await?;
            return Ok((0, access != Access::None && access != Access::FreeBusy));
        }
        let final_page = response.next_sync_token.is_some();
        let page = CalendarPage {
            input_cursor: cursor.page_token.map(str::to_owned),
            items: response.items,
            next_page_token: response.next_page_token,
            next_sync_token: response.next_sync_token,
        };
        match stage(ctx, &lease, page, memory).await {
            Ok(()) => {
                if final_page {
                    publish_events(ctx, &lease).await?;
                }
                return Ok((1, !final_page));
            }
            Err(error) if error.code == "stale_page" && replacement == 0 => {
                expire(ctx, &lease).await?;
                force_full = true;
            }
            Err(error) => return Err(error),
        }
    }
    Err(failure(
        "calendar_resync_required",
        "Calendar needs another synchronization attempt",
    ))
}

async fn refresh_permission_for(ctx: &SyncContext, calendar: &str) -> Result<()> {
    let before = scope(ctx, calendar).await?;
    let (api, memory) = ctx.calendar_request(WorkClass::DeltaRead).await?;
    let response = api
        .calendar_entry("engine-managed", calendar, &ctx.cancel)
        .await;
    match response {
        Ok(entry) => {
            let account = ctx.account.as_str().to_owned();
            let generation = ctx.connection_generation;
            let epoch = integer(&before, "permission_epoch")?;
            let cancel = ctx.cancel.clone();
            let (entry, memory) = blocking_preparation(memory, move || {
                prepare_calendar_catalog_entry(entry.source, &cancel)
            })
            .await?;
            ctx.core_call(move |s| {
                let result =
                    s.calendar_observe_prepared_catalog_entry(&account, &entry, generation, epoch);
                drop(entry);
                drop(memory);
                result
            })
            .await?;
            Ok(())
        }
        Err(ApiError::Provider(GoogleError::PermissionDenied | GoogleError::NotFound)) => {
            deny(ctx, calendar, &before).await
        }
        Err(error) => Err(api_error(error)),
    }
}

async fn deny(ctx: &SyncContext, calendar: &str, before: &Value) -> Result<()> {
    let account = ctx.account.as_str().to_owned();
    let calendar = calendar.to_owned();
    let generation = ctx.connection_generation;
    let epoch = integer(before, "permission_epoch")?;
    let zone = text(before, "time_zone")?.to_owned();
    ctx.core_call(move |s| {
        s.calendar_observe_permission(&account, &calendar, generation, epoch, Access::None, &zone)
    })
    .await?;
    Ok(())
}

async fn begin(ctx: &SyncContext, calendar: &str, force_full: bool) -> Result<CalendarSyncLease> {
    if let Some(lease) = resume(ctx, calendar).await? {
        if !force_full {
            let l = lease.clone();
            match ctx.core_call(move |s| s.calendar_sync_rebind(&l)).await {
                Ok(lease) => return Ok(lease),
                Err(error) if error.code == "stale_revision" => (),
                Err(error) => return Err(error),
            }
        }
        expire(ctx, &lease).await?;
    }
    let account = ctx.account.as_str().to_owned();
    let calendar = calendar.to_owned();
    ctx.core_call(move |s| {
        let checkpoint = if calendar.is_empty() {
            s.calendar_catalog_checkpoint(&account)?
        } else {
            s.calendar_checkpoint(&account, &calendar)?
        };
        let mode = if force_full || checkpoint["sync_token"].is_null() {
            CalendarSyncMode::Full
        } else {
            CalendarSyncMode::Delta
        };
        if calendar.is_empty() {
            s.calendar_catalog_begin(&account, mode)
        } else {
            s.calendar_sync_begin(&account, &calendar, mode)
        }
    })
    .await
}
async fn resume(ctx: &SyncContext, calendar: &str) -> Result<Option<CalendarSyncLease>> {
    let account = ctx.account.as_str().to_owned();
    let calendar = calendar.to_owned();
    ctx.core_call(move |s| s.calendar_sync_resume(&account, &calendar))
        .await
}
async fn scope(ctx: &SyncContext, calendar: &str) -> Result<Value> {
    let account = ctx.account.as_str().to_owned();
    let calendar = calendar.to_owned();
    ctx.core_call(move |s| s.calendar_sync_scope(&account, &calendar))
        .await
}
async fn progress(ctx: &SyncContext, lease: &CalendarSyncLease) -> Result<Value> {
    let lease = lease.clone();
    ctx.core_call(move |s| s.calendar_sync_progress(&lease))
        .await
}
async fn stage(
    ctx: &SyncContext,
    lease: &CalendarSyncLease,
    page: CalendarPage,
    memory: OwnedSemaphorePermit,
) -> Result<()> {
    let cancel = ctx.cancel.clone();
    let l = lease.clone();
    let (page, mut memory) =
        blocking_preparation(memory, move || prepare_calendar_page(l, page, &cancel)).await?;
    let page = Arc::new(page);
    let mut next = 0;
    let mut retries = 0;
    loop {
        let l = lease.clone();
        let p = page.clone();
        let previous = next;
        let (input, returned_memory) = ctx
            .core_call(move |s| Ok((s.calendar_prepare_chunk(&l, &p, previous)?, memory)))
            .await?;
        let p = page.clone();
        let cancel = ctx.cancel.clone();
        let (chunk, returned_memory) = blocking_preparation(returned_memory, move || {
            prepare_calendar_chunk(&p, input, &cancel)
        })
        .await?;
        let l = lease.clone();
        let p = page.clone();
        let (result, returned_memory) = ctx
            .core_call(move |s| {
                Ok((
                    s.calendar_stage_prepared_chunk(&l, &p, &chunk),
                    returned_memory,
                ))
            })
            .await?;
        memory = returned_memory;
        match result {
            Ok(value) => {
                next = value;
                retries = 0;
            }
            Err(error) if error.code == "stale_preparation" && retries < 2 => {
                retries += 1;
                continue;
            }
            Err(error) => return Err(error),
        }
        if next >= page.len() {
            break;
        }
        if next <= previous {
            return Err(invalid_checkpoint());
        }
    }
    let lease = lease.clone();
    ctx.core_call(move |s| {
        let result = s.calendar_seal_prepared_page(&lease, &page);
        drop(page);
        drop(memory);
        result
    })
    .await
}
pub(super) async fn blocking_preparation<T, F, M>(memory: M, operation: F) -> Result<(T, M)>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
    M: Send + 'static,
{
    tokio::task::spawn_blocking(move || operation().map(|value| (value, memory)))
        .await
        .map_err(|_| {
            failure(
                "calendar_preparation_failed",
                "Calendar preparation worker stopped",
            )
        })?
}
async fn finalize_catalog(ctx: &SyncContext, lease: &CalendarSyncLease) -> Result<()> {
    loop {
        let memory = ctx.calendar_prepare_permit().await?;
        let l = lease.clone();
        let (input, memory) = ctx
            .core_call(move |s| Ok((s.calendar_catalog_prepare_finalization(&l)?, memory)))
            .await?;
        let cancel = ctx.cancel.clone();
        let (prepared, memory) = blocking_preparation(memory, move || {
            prepare_calendar_catalog_chunk(input, &cancel)
        })
        .await?;
        let count = ctx
            .core_call(move |s| {
                let result = s.calendar_catalog_finalize_prepared(&prepared);
                drop(prepared);
                drop(memory);
                result
            })
            .await?;
        if count == 0 {
            return Ok(());
        }
    }
}
async fn publish_events(ctx: &SyncContext, lease: &CalendarSyncLease) -> Result<()> {
    let l = lease.clone();
    ctx.core_call(move |s| s.calendar_publish_sync(&l)).await?;
    cleanup(ctx, &lease.calendar).await
}
async fn expire(ctx: &SyncContext, lease: &CalendarSyncLease) -> Result<()> {
    let l = lease.clone();
    ctx.core_call(move |s| s.calendar_expire_sync(&l)).await?;
    if !lease.calendar.is_empty() {
        cleanup(ctx, &lease.calendar).await?;
    }
    Ok(())
}
async fn cleanup(ctx: &SyncContext, calendar: &str) -> Result<()> {
    loop {
        let account = ctx.account.as_str().to_owned();
        let calendar = calendar.to_owned();
        let Some(lease) = ctx
            .core_call(move |s| s.calendar_sync_cleanup(&account, &calendar))
            .await?
        else {
            return Ok(());
        };
        let count = ctx
            .core_call(move |s| s.calendar_finalize_chunk(&lease))
            .await?;
        if count == 0 {
            return Err(invalid_checkpoint());
        }
    }
}

/// Only the requested visible date range is acquired. Per-calendar unavailable
/// responses remain unavailable coverage and never become empty/free time.
pub(crate) async fn refresh_busy(ctx: &SyncContext, window: Window) -> Result<u64> {
    if !ctx.calendar_reads.busy {
        return Err(failure(
            "permission_denied",
            "Google free/busy access was not granted",
        ));
    }
    let mut after = None;
    let mut fetched = 0;
    loop {
        let account = ctx.account.as_str().to_owned();
        let cursor = after.clone();
        let scopes = ctx
            .core_call(move |s| s.calendar_sync_scopes(&account, cursor.as_deref(), 50))
            .await?;
        let mut epochs = BTreeMap::new();
        for scope in scopes["scopes"].as_array().ok_or_else(invalid_checkpoint)? {
            let access = text(scope, "access")?;
            if access == "free_busy"
                || (!ctx.calendar_reads.events
                    && matches!(
                        access,
                        "reader" | "writer" | "writer_without_private_access" | "owner"
                    ))
            {
                epochs.insert(
                    text(scope, "calendar_id")?.to_owned(),
                    integer(scope, "permission_epoch")?,
                );
            }
        }
        if !epochs.is_empty() {
            let calendars = epochs.keys().cloned().collect::<Vec<_>>();
            let (api, memory) = ctx.calendar_request(WorkClass::UserRead).await?;
            let snapshot = api
                .free_busy("engine-managed", &calendars, &window, &ctx.cancel)
                .await
                .map_err(api_error)?;
            let generation = ctx.connection_generation;
            let observed_at = chrono::Utc::now().timestamp_millis();
            ctx.core_call(move |s| {
                let result = s.calendar_busy_commit(&snapshot, observed_at, generation, &epochs);
                drop(snapshot);
                drop(memory);
                result
            })
            .await?;
            fetched += 1;
        }
        after = scopes["continuation"].as_str().map(str::to_owned);
        if after.is_none() {
            return Ok(fetched);
        }
    }
}

fn cursor(progress: &Value) -> Result<SyncCursor<'_>> {
    for name in ["sync_token", "next_page_token"] {
        if !progress[name].is_null() && !progress[name].is_string() {
            return Err(invalid_checkpoint());
        }
    }
    Ok(SyncCursor {
        sync_token: progress["sync_token"].as_str(),
        page_token: progress["next_page_token"].as_str(),
    })
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key].as_str().ok_or_else(invalid_checkpoint)
}
fn integer(value: &Value, key: &str) -> Result<i64> {
    value[key].as_i64().ok_or_else(invalid_checkpoint)
}
fn invalid_checkpoint() -> CoreError {
    failure(
        "calendar_checkpoint_invalid",
        "Calendar synchronization state needs review",
    )
}
fn failure(code: &str, message: &str) -> CoreError {
    CoreError {
        code: code.into(),
        message: message.into(),
        details: Value::Null,
    }
}
fn access_name(access: Access) -> &'static str {
    match access {
        Access::None => "none",
        Access::FreeBusy => "free_busy",
        Access::Reader => "reader",
        Access::WriterWithoutPrivateAccess => "writer_without_private_access",
        Access::Writer => "writer",
        Access::Owner => "owner",
    }
}
fn account_failure(error: &CoreError) -> bool {
    matches!(
        error.code.as_str(),
        "cancelled"
            | "stale_google_work"
            | "engine_stopped"
            | "reauthorization_required"
            | "google_rate_limited"
            | "google_unavailable"
    )
}
fn api_error(error: ApiError) -> CoreError {
    let retry_after_secs = match &error {
        ApiError::Transport(TransportError::Cooldown { retry_after_secs }) => {
            Some(*retry_after_secs)
        }
        ApiError::Provider(GoogleError::RateLimited { retry_after_secs }) => *retry_after_secs,
        _ => None,
    };
    let (code, message) = match error {
        ApiError::Provider(GoogleError::ReauthorizationRequired) => {
            ("reauthorization_required", "Reconnect this Google account")
        }
        ApiError::Provider(GoogleError::RateLimited { .. })
        | ApiError::Transport(TransportError::Cooldown { .. }) => (
            "google_rate_limited",
            "Google is limiting Calendar requests; synchronization will resume",
        ),
        ApiError::Provider(GoogleError::PermissionDenied | GoogleError::NotFound) => (
            "permission_denied",
            "This calendar is unavailable to the account",
        ),
        ApiError::Transport(TransportError::Cancelled) => {
            ("cancelled", "Calendar synchronization stopped")
        }
        ApiError::Provider(GoogleError::ServerUnavailable | GoogleError::TransportInterrupted)
        | ApiError::Transport(TransportError::Network | TransportError::Timeout) => (
            "google_unavailable",
            "Google Calendar is temporarily unavailable",
        ),
        ApiError::SyncTokenExpired => (
            "calendar_resync_required",
            "Calendar needs a fresh inventory",
        ),
        _ => (
            "calendar_response_invalid",
            "Google returned an unusable Calendar response",
        ),
    };
    let mut result = failure(code, message);
    if let Some(seconds) = retry_after_secs {
        result.details = serde_json::json!({"retry_after_secs":seconds});
    }
    result
}

#[cfg(test)]
#[path = "calendar_tests.rs"]
mod tests;
