//! Explicit occurrence lookup/preflight. Only bounded metadata survives between
//! one-request quota admissions; complete sources share the existing two seats.
use super::{calendar::blocking_preparation, calendar_writes, failure, SyncContext};
use calendar_writes::{event_source, memory_core, read_error, Memory};
use fm_calendar::{google_api::EventFetch, EventTime};
use fm_core::{
    live_calendar_occurrences::{
        prepare_calendar_occurrence_instance, prepare_calendar_occurrence_master,
        CalendarOccurrenceMasterProof, CalendarOccurrenceSelector, PreparedCalendarOccurrence,
    },
    Result,
};
use fm_google::scheduler::WorkClass;
use serde_json::{json, Value};
use std::{collections::BTreeSet, sync::Arc};

/// Handles `google_calendar_occurrence_read`: re-reads the calendar's access,
/// then fetches and validates one occurrence of a recurring event. With a
/// `scope` parameter it instead returns what a recurring edit of that scope
/// needs.
pub(crate) async fn read(ctx: &SyncContext, params: Value) -> Result<Value> {
    if params.get("scope").is_some() {
        let (selector, scope, action) = fm_core::live_calendar_recurring::read_selector(&params)?;
        let epoch = calendar_writes::refresh_permission(ctx, selector.calendar_id()).await?;
        let (prepared, memory) = prepare(ctx, selector, epoch).await?;
        return memory_core(ctx, &memory, move |store| {
            store.calendar_live_recurring_read_finish(prepared, &scope, &action)
        })
        .await;
    }
    let selector = CalendarOccurrenceSelector::parse(&params)?;
    let epoch = calendar_writes::refresh_permission(ctx, selector.calendar_id()).await?;
    let (prepared, memory) = prepare(ctx, selector, epoch).await?;
    memory_core(ctx, &memory, move |store| {
        store.calendar_live_occurrence_finish(prepared)
    })
    .await
}

/// Builds a selector from a stored occurrence's series id and original start.
pub(super) fn selector(
    calendar: &str,
    event: &str,
    occurrence: &Value,
) -> Result<CalendarOccurrenceSelector> {
    CalendarOccurrenceSelector::parse(&json!({
        "calendar_id":calendar,"event_id":event,
        "series_id":occurrence["series_id"],"original_start":occurrence["original_start"],
    }))
}

/// Fetches the series master and the selected instance, and returns the
/// validated occurrence with the preparation permit that still holds it.
/// `epoch` is the calendar's permission epoch from the preceding refresh.
pub(super) async fn prepare(
    ctx: &SyncContext,
    selector: CalendarOccurrenceSelector,
    epoch: i64,
) -> Result<(PreparedCalendarOccurrence, Memory)> {
    let master = master(ctx, selector, epoch).await?;
    let event = resolve_instance_id(ctx, master.selector()).await?;
    let calendar = master.selector().calendar_id();
    let series = master.selector().series_id();
    let (api, memory) = ctx.calendar_request(WorkClass::UserRead).await?;
    let source = api
        .get_occurrence("engine-managed", calendar, series, &event, &ctx.cancel)
        .await
        .map_err(read_error)?
        .source;
    instance_source(ctx, master, source, Arc::new(memory)).await
}

/// Fetches the complete series master and checks it against the store's plan,
/// returning the proof the instance step requires. A conditional
/// "not modified" reply is refused: the master must be read fresh.
pub(super) async fn master(
    ctx: &SyncContext,
    selector: CalendarOccurrenceSelector,
    epoch: i64,
) -> Result<CalendarOccurrenceMasterProof> {
    let calendar = selector.calendar_id().to_owned();
    let series = selector.series_id().to_owned();
    // The retained master is loaded only after both admissions, so its bytes
    // cannot sit in a quota waiter or unaccounted writer reply.
    let (api, memory) = ctx.calendar_request(WorkClass::UserRead).await?;
    let memory = Arc::new(memory);
    let account = ctx.account.as_str().to_owned();
    let generation = ctx.connection_generation;
    let plan = memory_core(ctx, &memory, move |store| {
        store.calendar_live_occurrence_begin(&account, selector, generation, epoch)
    })
    .await?;
    let source = match api
        .get_event("engine-managed", &calendar, &series, None, &ctx.cancel)
        .await
        .map_err(read_error)?
    {
        EventFetch::Found(resource) => event_source(resource),
        EventFetch::NotModified => {
            return Err(failure(
                "calendar_response_invalid",
                "A complete fresh master is required",
            ))
        }
    };
    let cancel = ctx.cancel.clone();
    let (proof, memory) = blocking_preparation(memory, move || {
        if cancel.is_cancelled() {
            return Err(failure("cancelled", "Calendar occurrence lookup stopped"));
        }
        prepare_calendar_occurrence_master(plan, source)
    })
    .await?;
    drop(memory);
    if ctx.cancel.is_cancelled() {
        return Err(failure("cancelled", "Calendar occurrence lookup stopped"));
    }
    Ok(proof)
}

/// Validates a fetched instance against the master proof on the blocking pool.
pub(super) async fn instance_source(
    ctx: &SyncContext,
    master: CalendarOccurrenceMasterProof,
    source: Value,
    memory: Memory,
) -> Result<(PreparedCalendarOccurrence, Memory)> {
    let cancel = ctx.cancel.clone();
    blocking_preparation(memory, move || {
        if cancel.is_cancelled() {
            return Err(failure("cancelled", "Calendar occurrence lookup stopped"));
        }
        prepare_calendar_occurrence_instance(master, source)
    })
    .await
}

/// Finds the provider instance id whose original start matches the
/// selector, reading at most four pages of instances. Fails when no instance
/// or more than one matches, or when Google repeats a page token.
async fn resolve_instance_id(
    ctx: &SyncContext,
    selector: &CalendarOccurrenceSelector,
) -> Result<String> {
    // This returns a hint only. The complete GET and opaque Core proof below
    // must still validate every dimension before the ID reaches an editor/write.
    if let Some(event) = selector.event_id() {
        return Ok(event.to_owned());
    }
    let original = selector.original_start();
    let timed = matches!(original, EventTime::Zoned { .. }).then_some(original);
    let mut next = None;
    let mut seen_tokens = BTreeSet::new();
    let mut candidate = None;
    for _ in 0..4 {
        let (api, memory) = ctx.calendar_request(WorkClass::UserRead).await?;
        let page = api
            .instances(
                "engine-managed",
                selector.calendar_id(),
                selector.series_id(),
                timed,
                next.as_deref(),
                &ctx.cancel,
            )
            .await
            .map_err(read_error)?;
        for identity in page.items {
            if same_original(&identity.original_start, original)
                && candidate.replace(identity.key.event).is_some()
            {
                return Err(failure(
                    "calendar_occurrence_ambiguous",
                    "More than one provider instance matches this original start",
                ));
            }
        }
        next = page.next_page_token;
        // Nothing decoded from this page remains across another quota wait.
        drop(memory);
        if ctx.cancel.is_cancelled() {
            return Err(failure("cancelled", "Calendar occurrence lookup stopped"));
        }
        let Some(token) = &next else {
            return candidate.ok_or_else(|| {
                failure(
                    "calendar_occurrence_unresolved",
                    "The provider did not resolve this occurrence",
                )
            });
        };
        if !seen_tokens.insert(token.clone()) {
            return Err(failure(
                "calendar_occurrence_unresolved",
                "The provider repeated an occurrence continuation",
            ));
        }
    }
    Err(failure(
        "calendar_occurrence_unresolved",
        "Occurrence lookup exceeded its bounded page limit",
    ))
}

/// Compares original starts: all-day by date, timed by instant regardless of
/// time zone.
fn same_original(left: &EventTime, right: &EventTime) -> bool {
    match (left, right) {
        (EventTime::Date { date: left }, EventTime::Date { date: right }) => left == right,
        (EventTime::Zoned { instant: left, .. }, EventTime::Zoned { instant: right, .. }) => {
            left == right
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "calendar_occurrences_tests.rs"]
mod tests;
