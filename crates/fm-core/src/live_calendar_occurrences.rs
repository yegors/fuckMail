//! One admitted source at a time. Only qualified master metadata survives a
//! subsequent quota wait; caller JSON can never construct an authority proof.
use crate::calendar_store::{calendar_error, fingerprint, parse_access};
use crate::{err, Result, Store};
use fm_calendar::{Access, Event, EventTime, RecurringMasterContext};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const SOURCE_LIMIT: usize = 256 * 1024;

pub(crate) fn projection_selector(parent: &Event, source: &Event, row: &Value) -> Option<Value> {
    projection_selector_with_role(parent, source, row, true)
}
pub(crate) fn projection_selector_with_role(
    parent: &Event,
    source: &Event,
    row: &Value,
    organizer: bool,
) -> Option<Value> {
    if row["busy_only"] == true
        || parent.cancelled
        || source.cancelled
        || (organizer && parent.source["organizer"]["self"] != true)
        || parent.source["locked"] == true
        || source.source["locked"] == true
        || !matches!(parent.source["eventType"].as_str(), None | Some("default"))
        || !matches!(source.source["eventType"].as_str(), None | Some("default"))
        || crate::live_calendar_writes::etag(&parent.etag).is_err()
        || !fm_calendar::ordinary_conference_supported(&parent.source)
        || !fm_calendar::ordinary_conference_supported(&source.source)
    {
        return None;
    }
    let original = row["occurrence_key"]["original_start"].as_str()?;
    let original = if let Some(time) = source
        .original_start
        .as_ref()
        .filter(|v| v.identity() == original)
    {
        serde_json::to_value(time).ok()?
    } else {
        match parent.start.as_ref()? {
            EventTime::Date { .. } => json!({"kind":"date","date":original}),
            EventTime::Zoned { time_zone, .. } => {
                json!({"kind":"zoned","instant":original,"time_zone":time_zone})
            }
        }
    };
    let mut selector = json!({"calendar_id":source.key.calendar,"series_id":row["occurrence_key"]["series"],"original_start":original});
    if source.series_id.is_some() {
        selector["event_id"] = source.key.event.clone().into();
    }
    CalendarOccurrenceSelector::parse(&selector).ok()?;
    Some(selector)
}

#[derive(Clone, Debug, Serialize)]
pub struct CalendarOccurrenceSelector {
    calendar_id: String,
    series_id: String,
    original_start: EventTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_id: Option<String>,
}
impl CalendarOccurrenceSelector {
    pub fn parse(value: &Value) -> Result<Self> {
        crate::fields(
            value,
            &["calendar_id", "series_id", "original_start", "event_id"],
        )?;
        let calendar_id =
            crate::live_calendar::provider_id(crate::string(value, "calendar_id")?)?.into();
        let series_id =
            crate::live_calendar::provider_id(crate::string(value, "series_id")?)?.into();
        let event_id = value
            .get("event_id")
            .map(|v| {
                let text = v
                    .as_str()
                    .ok_or_else(|| err("invalid_request", "Instance hint must be text"))?;
                crate::live_calendar::provider_id(text).map(str::to_owned)
            })
            .transpose()?;
        let original_start = parse_original(&value["original_start"])?;
        Ok(Self {
            calendar_id,
            series_id,
            original_start,
            event_id,
        })
    }
    pub fn calendar_id(&self) -> &str {
        &self.calendar_id
    }
    pub fn series_id(&self) -> &str {
        &self.series_id
    }
    pub fn original_start(&self) -> &EventTime {
        &self.original_start
    }
    pub fn event_id(&self) -> Option<&str> {
        self.event_id.as_deref()
    }
}
pub(crate) fn parse_original(value: &Value) -> Result<EventTime> {
    match value["kind"].as_str() {
        Some("date") => crate::fields(value, &["kind", "date"])?,
        Some("zoned") => crate::fields(value, &["kind", "instant", "time_zone"])?,
        _ => {
            return Err(err(
                "invalid_request",
                "Choose a date or zoned original start",
            ))
        }
    }
    if serde_json::to_vec(value)?.len() > 1024 {
        return Err(err("invalid_request", "Original start exceeds bounds"));
    }
    let time: EventTime = serde_json::from_value(value.clone())?;
    if let EventTime::Zoned { time_zone, .. } = &time {
        time.utc(
            time_zone
                .parse()
                .map_err(|_| err("invalid_request", "Original start needs an IANA time zone"))?,
        )
        .map_err(calendar_error)?;
    }
    Ok(time)
}
pub(crate) fn same_original(left: &EventTime, right: &EventTime) -> bool {
    std::mem::discriminant(left) == std::mem::discriminant(right)
        && left.identity() == right.identity()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MasterStamp {
    pub account: String,
    pub calendar: String,
    pub series: String,
    pub etag: String,
    pub source_sha256: String,
    pub recurrence_sha256: String,
    pub source_generation: i64,
    pub calendar_revision: i64,
    pub lifecycle_revision: i64,
    pub connection_generation: i64,
    pub permission_epoch: i64,
    pub zone: String,
    pub access: String,
}
pub struct CalendarOccurrenceReadPlan {
    selector: CalendarOccurrenceSelector,
    stamp: MasterStamp,
    retained_master: String,
}
impl CalendarOccurrenceReadPlan {
    pub fn selector(&self) -> &CalendarOccurrenceSelector {
        &self.selector
    }
}
#[derive(Clone)]
pub struct CalendarOccurrenceMasterProof {
    selector: CalendarOccurrenceSelector,
    pub(crate) stamp: MasterStamp,
    context: RecurringMasterContext,
    all_day: bool,
    editable_master: bool,
}
impl CalendarOccurrenceMasterProof {
    pub fn selector(&self) -> &CalendarOccurrenceSelector {
        &self.selector
    }
}
pub struct PreparedCalendarOccurrence {
    pub(crate) master: CalendarOccurrenceMasterProof,
    pub(crate) event: Event,
}
impl PreparedCalendarOccurrence {
    pub fn selector(&self) -> &CalendarOccurrenceSelector {
        &self.master.selector
    }
    pub(crate) fn occurrence(&self) -> Value {
        json!({"series_id":self.master.stamp.series,"original_start":self.event.original_start,"expected_master_etag":self.master.stamp.etag})
    }
    pub(crate) fn require_editable(&self) -> Result<()> {
        if !self.master.editable_master {
            return Err(err(
                "unsupported_operation",
                "This recurring master cannot be safely modified",
            ));
        }
        crate::live_calendar_writes::etag(&self.event.etag)?;
        crate::live_calendar_writes::initial_event(
            &self.event,
            parse_access(&self.master.stamp.access)?,
            "edit",
            3,
        )
    }
}

pub fn prepare_calendar_occurrence_master(
    plan: CalendarOccurrenceReadPlan,
    fresh_master: Value,
) -> Result<CalendarOccurrenceMasterProof> {
    let retained: Value = serde_json::from_str(&plan.retained_master)?;
    let stamp = &plan.stamp;
    let master =
        fm_calendar::normalize(&stamp.account, &stamp.calendar, &stamp.zone, &fresh_master)
            .map_err(calendar_error)?;
    crate::live_calendar_writes::etag(&master.etag)?;
    let source_sha256 = fingerprint(&fresh_master)?;
    if master.key.event != stamp.series || source_sha256 != fingerprint(&retained)? {
        return Err(err(
            "stale_revision",
            "The recurring master changed; sync and review this occurrence",
        ));
    }
    let context = master.recurring_master_context().map_err(calendar_error)?;
    let access = parse_access(&stamp.access)?;
    if matches!(access.for_event(&master), Access::None | Access::FreeBusy) {
        return Err(err(
            "permission_denied",
            "Recurring master details are unavailable",
        ));
    }
    let all_day = matches!(master.start, Some(EventTime::Date { .. }));
    if all_day != matches!(plan.selector.original_start, EventTime::Date { .. }) {
        return Err(err(
            "scope_mismatch",
            "Original start and recurring master time types differ",
        ));
    }
    let editable_master = !master.cancelled
        && master.source["locked"] != true
        && matches!(master.source["eventType"].as_str(), None | Some("default"))
        && master.source["organizer"]["self"] == true
        && fm_calendar::ordinary_conference_supported(&master.source);
    let mut stamp = plan.stamp;
    stamp.etag = master.etag;
    stamp.source_sha256 = source_sha256;
    stamp.recurrence_sha256 = fingerprint(&fresh_master["recurrence"])?;
    Ok(CalendarOccurrenceMasterProof {
        selector: plan.selector,
        stamp,
        context,
        all_day,
        editable_master,
    })
}

pub fn prepare_calendar_occurrence_instance(
    master: CalendarOccurrenceMasterProof,
    fresh_instance: Value,
) -> Result<PreparedCalendarOccurrence> {
    let stamp = &master.stamp;
    let event = fm_calendar::normalize(
        &stamp.account,
        &stamp.calendar,
        &stamp.zone,
        &fresh_instance,
    )
    .map_err(calendar_error)?;
    crate::live_calendar::provider_id(&event.key.event)?;
    if master
        .selector
        .event_id
        .as_ref()
        .is_some_and(|id| id != &event.key.event)
        || event.series_id.as_deref() != Some(stamp.series.as_str())
        || event
            .original_start
            .as_ref()
            .is_none_or(|time| !same_original(time, &master.selector.original_start))
        || event
            .original_start
            .as_ref()
            .is_some_and(|time| matches!(time, EventTime::Date { .. }) != master.all_day)
    {
        return Err(err(
            "scope_mismatch",
            "Provider instance differs from the selected occurrence",
        ));
    }
    let event = event
        .with_recurring_context(&master.context)
        .map_err(calendar_error)?;
    if matches!(
        parse_access(&stamp.access)?.for_event(&event),
        Access::None | Access::FreeBusy
    ) {
        return Err(err(
            "permission_denied",
            "Occurrence details are unavailable",
        ));
    }
    if !event.cancelled {
        crate::live_calendar_writes::etag(&event.etag)?;
    }
    Ok(PreparedCalendarOccurrence { master, event })
}

impl Store {
    /// Call only after this master GET has quota and one Calendar memory seat.
    pub fn calendar_live_occurrence_begin(
        &self,
        account: &str,
        selector: CalendarOccurrenceSelector,
        connection_generation: i64,
        permission_epoch: i64,
    ) -> Result<CalendarOccurrenceReadPlan> {
        let lifecycle_revision = self.require_active_account(account)?;
        if self.calendar_occurrence_read_connection(account)? != connection_generation {
            return Err(err(
                "stale_revision",
                "Calendar connection changed before master qualification",
            ));
        }
        let scope = self.calendar_sync_scope(account, selector.calendar_id())?;
        if scope["permission_epoch"] != permission_epoch
            || scope["membership_denied"] == true
            || scope["full_generation"].as_i64().unwrap_or(0) == 0
            || scope["retained_errors"].as_i64().unwrap_or(1) != 0
        {
            return Err(err(
                "calendar_not_ready",
                "Refresh Calendar sources and permissions first",
            ));
        }
        let access = crate::string(&scope, "access")?;
        if matches!(parse_access(access)?, Access::None | Access::FreeBusy) {
            return Err(err("permission_denied", "Calendar details are unavailable"));
        }
        let row:Option<(usize,Option<String>,i64)>=self.db.query_row("SELECT length(CAST(json AS BLOB)),CASE WHEN length(CAST(json AS BLOB))<=?4 THEN json END,generation FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![account,selector.calendar_id(),selector.series_id(),SOURCE_LIMIT],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let (_, raw, source_generation) = row.ok_or_else(|| {
            err(
                "not_found",
                "The current recurring master is unavailable; sync first",
            )
        })?;
        let retained_master = raw.ok_or_else(|| {
            err(
                "limit_exceeded",
                "Retained recurring master exceeds 256 KiB",
            )
        })?;
        let stamp = MasterStamp {
            account: account.into(),
            calendar: selector.calendar_id.clone(),
            series: selector.series_id.clone(),
            etag: String::new(),
            source_sha256: String::new(),
            recurrence_sha256: String::new(),
            source_generation,
            calendar_revision: scope["revision"].as_i64().unwrap(),
            lifecycle_revision,
            connection_generation,
            permission_epoch,
            zone: crate::string(&scope, "time_zone")?.into(),
            access: access.into(),
        };
        Ok(CalendarOccurrenceReadPlan {
            selector,
            stamp,
            retained_master,
        })
    }
    pub fn calendar_live_occurrence_finish(
        &self,
        proof: PreparedCalendarOccurrence,
    ) -> Result<Value> {
        self.check_calendar_occurrence_master(&proof.master.stamp)?;
        crate::live_calendar_writes::live(self, &proof.master.stamp.account)?;
        proof.require_editable()?;
        let stamp = &proof.master.stamp;
        let display_zone = self
            .calendar_display_zone()?
            .unwrap_or_else(|| stamp.zone.clone());
        let zone = display_zone
            .parse()
            .map_err(|_| err("invalid_request", "Calendar zone is unavailable"))?;
        let from = proof
            .event
            .start
            .as_ref()
            .ok_or_else(|| err("unsupported_operation", "Occurrence start is unavailable"))?
            .utc(zone)
            .map_err(calendar_error)?;
        let to = proof
            .event
            .end
            .as_ref()
            .ok_or_else(|| err("unsupported_operation", "Occurrence end is unavailable"))?
            .utc(zone)
            .map_err(calendar_error)?;
        let window = fm_calendar::Window::new(from, to, zone).map_err(calendar_error)?;
        let mut rows = fm_calendar::project_event_in_window(
            &proof.event,
            &[],
            &display_zone,
            window,
            parse_access(&stamp.access)?,
        )
        .map_err(calendar_error)?;
        if rows.len() != 1 {
            return Err(err(
                "unsupported_operation",
                "This occurrence cannot be represented by the editor",
            ));
        }
        let mut event = rows.remove(0);
        event["account"] = stamp.account.clone().into();
        event["event_id"] = proof.event.key.event.clone().into();
        event["etag"] = proof.event.etag.clone().into();
        event["recurring"] = true.into();
        event["can_edit"] = true.into();
        event["can_delete"] = true.into();
        event["can_rsvp"] = false.into();
        match crate::live_calendar_invitations::guest_projection(&proof.event.source) {
            Ok((guests, protected)) => {
                event["can_edit_attendees"] = true.into();
                event["attendee_edit_reason"] = Value::Null;
                event["guest_addresses"] = guests.into();
                event["protected_attendees"] = protected.into();
            }
            Err(error) => {
                event["can_edit_attendees"] = false.into();
                event["attendee_edit_reason"] = error.code.into();
                event["guest_addresses"] = json!([]);
                event["protected_attendees"] = json!([]);
            }
        }
        Ok(
            json!({"account":stamp.account,"calendar_id":stamp.calendar,"event_id":proof.event.key.event,"etag":proof.event.etag,"occurrence":proof.occurrence(),"event":event}),
        )
    }
    pub(crate) fn calendar_occurrence_read_connection(&self, account: &str) -> Result<i64> {
        self.require_active_account(account)?;
        if self.profile_kind()? != crate::live_profile::ProfileKind::Live {
            return Err(err(
                "profile_kind_mismatch",
                "Occurrence reads require a live profile",
            ));
        }
        let binding = self
            .google_account_bindings()?
            .into_iter()
            .find(|v| v.account == account)
            .ok_or_else(|| err("not_found", "Google account is unavailable"))?;
        if binding.state != crate::live_profile::ConnectionState::Connected
            || ![
                "https://www.googleapis.com/auth/calendar.events",
                "https://www.googleapis.com/auth/calendar.events.readonly",
                "https://www.googleapis.com/auth/calendar",
                "https://www.googleapis.com/auth/calendar.readonly",
            ]
            .iter()
            .any(|scope| binding.scopes.contains(*scope))
        {
            return Err(err(
                "permission_denied",
                "Google Calendar detail-read scope is unavailable",
            ));
        }
        Ok(binding.connection_generation)
    }
    pub(crate) fn check_calendar_occurrence_master(&self, stamp: &MasterStamp) -> Result<()> {
        self.check_active_account_revision(&stamp.account, stamp.lifecycle_revision)?;
        if self.calendar_occurrence_read_connection(&stamp.account)? != stamp.connection_generation
        {
            return Err(err("stale_revision", "Calendar connection changed"));
        }
        let scope = self.calendar_sync_scope(&stamp.account, &stamp.calendar)?;
        let current:Option<(i64,Option<String>)>=self.db.query_row("SELECT generation,json_extract(json,'$.etag') FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![stamp.account,stamp.calendar,stamp.series],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if scope["membership_denied"] == true
            || scope["permission_epoch"] != stamp.permission_epoch
            || scope["revision"] != stamp.calendar_revision
            || current.as_ref().is_none_or(|(generation, etag)| {
                *generation != stamp.source_generation || etag.as_deref() != Some(&stamp.etag)
            })
        {
            return Err(err(
                "stale_revision",
                "The qualified recurring master or Calendar authority changed",
            ));
        }
        Ok(())
    }
}
