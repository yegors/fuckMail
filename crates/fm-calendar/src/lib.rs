//! Google Calendar feasibility boundary, P0-06 / Calendar draft revision 5.
//! No network, database, feed or notification sender. Core owns persistence.
//! Input source is retained separately from permission-filtered projections.

use chrono::{DateTime, Duration, FixedOffset, NaiveDate, TimeZone, Utc};
use rrule::{RRuleSet, Tz as RuleTz};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub mod edits;
pub mod google_api;
pub mod recurring;
pub mod sync;
/// Zone type shared with Core, which buckets mail by the client's local month.
pub use chrono_tz::Tz;
pub use edits::*;
pub use sync::*;

pub const FIXTURE_REVISION: u32 = 1;
pub const MAX_OCCURRENCES: u16 = 1000;
pub const MAX_WINDOW_DAYS: i64 = 42;
pub const TZDB_VERSION: &str = chrono_tz::IANA_TZDB_VERSION;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CalendarError {
    pub code: &'static str,
    pub message: String,
}
impl std::fmt::Display for CalendarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for CalendarError {}
pub(crate) fn error(code: &'static str, message: impl Into<String>) -> CalendarError {
    CalendarError {
        code,
        message: message.into(),
    }
}
pub type Result<T> = std::result::Result<T, CalendarError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventTime {
    Date {
        date: NaiveDate,
    },
    Zoned {
        instant: DateTime<FixedOffset>,
        time_zone: String,
    },
}
impl EventTime {
    pub fn identity(&self) -> String {
        match self {
            Self::Date { date } => date.to_string(),
            Self::Zoned { instant, .. } => instant.with_timezone(&Utc).to_rfc3339(),
        }
    }
    pub fn utc(&self, calendar_zone: Tz) -> Result<DateTime<Utc>> {
        match self {
            Self::Zoned { instant, .. } => Ok(instant.with_timezone(&Utc)),
            Self::Date { date } => calendar_zone
                .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
                .earliest()
                .map(|time| time.with_timezone(&Utc))
                .ok_or_else(|| {
                    error(
                        "invalid_time",
                        "All-day boundary is not a valid local date boundary",
                    )
                }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventKey {
    pub account: String,
    pub calendar: String,
    pub event: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OccurrenceKey {
    pub account: String,
    pub calendar: String,
    pub series: String,
    /// Original local date or UTC instant; preserved when the instance moves.
    pub original_start: Option<String>,
}
#[derive(Debug, Clone)]
pub struct Event {
    pub key: EventKey,
    pub etag: String,
    pub cancelled: bool,
    pub summary: String,
    pub description: String,
    pub location: String,
    pub conference_url: Option<String>,
    pub organizer: Option<String>,
    pub attendees: Vec<Value>,
    pub private: bool,
    /// None on an unresolved instance; resolved only against its retained master.
    /// This context never changes the original provider source.
    pub series_private: Option<bool>,
    pub series_id: Option<String>,
    pub original_start: Option<EventTime>,
    pub start: Option<EventTime>,
    pub end: Option<EventTime>,
    pub recurrence: Vec<String>,
    /// Retained provider source. Never serialize this into a busy-only projection.
    pub source: Value,
}

/// Source-free recurring-master context, constructed through the same master
/// validation used when the complete normalized source is available.
#[derive(Debug, Clone)]
pub struct RecurringMasterContext {
    key: EventKey,
    private: bool,
    cancelled: bool,
}
impl RecurringMasterContext {
    pub fn key(&self) -> &EventKey {
        &self.key
    }
    pub fn private(&self) -> bool {
        self.private
    }
    pub fn cancelled(&self) -> bool {
        self.cancelled
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    None,
    FreeBusy,
    Reader,
    WriterWithoutPrivateAccess,
    Writer,
    Owner,
}
impl Access {
    /// Resolve source privacy before exposing fields or planning a write. An
    /// instance without its master cannot establish public visibility.
    pub fn for_event(self, event: &Event) -> Self {
        if matches!(self, Self::Reader | Self::WriterWithoutPrivateAccess)
            && (event.private || (event.series_id.is_some() && event.series_private != Some(false)))
        {
            Self::FreeBusy
        } else if self == Self::WriterWithoutPrivateAccess {
            Self::Writer
        } else {
            self
        }
    }
}
impl Event {
    pub fn recurring_master_context(&self) -> Result<RecurringMasterContext> {
        if self.series_id.is_some() || (!self.cancelled && self.recurrence.is_empty()) {
            return Err(error("scope_mismatch", "A recurring master is required"));
        }
        Ok(RecurringMasterContext {
            key: self.key.clone(),
            private: self.private,
            cancelled: self.cancelled,
        })
    }

    /// Bind an instance to its current same-scope master before a direct edit or
    /// RSVP. A public-looking exception cannot override private master access.
    pub fn with_recurring_master(&self, master: &Event) -> Result<Self> {
        self.with_recurring_context(&master.recurring_master_context()?)
    }

    /// Apply the same privacy/cancellation rules after the master source has
    /// been released from a bounded preparation stage.
    pub fn with_recurring_context(&self, master: &RecurringMasterContext) -> Result<Self> {
        if self.key.account != master.key.account
            || self.key.calendar != master.key.calendar
            || self.series_id.as_deref() != Some(master.key.event.as_str())
        {
            return Err(error(
                "scope_mismatch",
                "Instance does not belong to this recurring master",
            ));
        }
        let mut resolved = self.clone();
        resolved.series_private = Some(master.private);
        resolved.cancelled |= master.cancelled;
        Ok(resolved)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Occurrence {
    pub key: OccurrenceKey,
    pub start: EventTime,
    pub end: EventTime,
    pub title: String,
    pub busy_only: bool,
    pub read_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conference_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organizer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attendees: Option<Vec<Value>>,
}

#[derive(Debug, Clone, Copy)]
pub struct Window {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub calendar_zone: Tz,
}
impl Window {
    /// Resolve displayed date boundaries once. Timed event source/recurrence
    /// zones remain independent of the zone defining this visible date range.
    pub fn from_dates(from: NaiveDate, to: NaiveDate, view_zone: &str) -> Result<Self> {
        let zone: Tz = view_zone
            .parse()
            .map_err(|_| error("unsupported_zone", "Invalid view time zone"))?;
        Self::new(
            EventTime::Date { date: from }.utc(zone)?,
            EventTime::Date { date: to }.utc(zone)?,
            zone,
        )
    }
    pub fn new(from: DateTime<Utc>, to: DateTime<Utc>, calendar_zone: Tz) -> Result<Self> {
        // Bounds describe displayed civil dates. A six-week month crossing the
        // autumn offset change legitimately lasts 42 days plus one hour.
        let civil_span = to.with_timezone(&calendar_zone).naive_local()
            - from.with_timezone(&calendar_zone).naive_local();
        if to <= from
            || civil_span > Duration::days(MAX_WINDOW_DAYS)
            || to - from > Duration::days(MAX_WINDOW_DAYS + 1)
        {
            return Err(error(
                "invalid_window",
                "Occurrence windows must be positive and no longer than 42 days",
            ));
        }
        Ok(Self {
            from,
            to,
            calendar_zone,
        })
    }
    fn overlaps(&self, start: &EventTime, end: &EventTime) -> Result<bool> {
        Ok(start.utc(self.calendar_zone)? < self.to && end.utc(self.calendar_zone)? > self.from)
    }
}

fn string<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    value
        .get(name)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 1024)
        .ok_or_else(|| error("invalid_event", format!("Missing or invalid {name}")))
}
fn parse_time(value: &Value, fallback_zone: &str) -> Result<EventTime> {
    match (value.get("date"), value.get("dateTime")) {
        (Some(date), None) => Ok(EventTime::Date {
            date: NaiveDate::parse_from_str(date.as_str().unwrap_or(""), "%Y-%m-%d")
                .map_err(|_| error("invalid_time", "Invalid all-day date"))?,
        }),
        (None, Some(time)) => {
            let zone = value
                .get("timeZone")
                .and_then(Value::as_str)
                .unwrap_or(fallback_zone);
            let _: Tz = zone
                .parse()
                .map_err(|_| error("unsupported_zone", "An IANA time zone is required"))?;
            let instant =
                DateTime::parse_from_rfc3339(time.as_str().unwrap_or("")).map_err(|_| {
                    error(
                        "invalid_time",
                        "Timed Google events require a valid offset-bearing dateTime",
                    )
                })?;
            Ok(EventTime::Zoned {
                instant,
                time_zone: zone.to_owned(),
            })
        }
        _ => Err(error(
            "invalid_time",
            "Use exactly one of date and dateTime",
        )),
    }
}

pub fn normalize(
    account: &str,
    calendar: &str,
    calendar_zone: &str,
    source: &Value,
) -> Result<Event> {
    if account.is_empty() || calendar.is_empty() || account.len() > 256 || calendar.len() > 1024 {
        return Err(error(
            "invalid_scope",
            "An explicit bounded account and calendar are required",
        ));
    }
    let _: Tz = calendar_zone
        .parse()
        .map_err(|_| error("unsupported_zone", "Calendar requires an IANA time zone"))?;
    if serde_json::to_vec(source)
        .map_err(|_| error("invalid_event", "Invalid JSON"))?
        .len()
        > 256 * 1024
    {
        return Err(error("limit_exceeded", "Event source exceeds 256 KiB"));
    }
    let cancelled = source["status"] == "cancelled";
    let start = source
        .get("start")
        .map(|v| parse_time(v, calendar_zone))
        .transpose()?;
    let end = source
        .get("end")
        .map(|v| parse_time(v, calendar_zone))
        .transpose()?;
    if !cancelled && (start.is_none() || end.is_none()) {
        return Err(error("invalid_event", "Active events need start and end"));
    }
    if let (Some(start), Some(end)) = (&start, &end) {
        if std::mem::discriminant(start) != std::mem::discriminant(end)
            || end.utc(calendar_zone.parse().unwrap())?
                <= start.utc(calendar_zone.parse().unwrap())?
        {
            return Err(error(
                "invalid_event",
                "Event end must follow start using the same date/time kind",
            ));
        }
    }
    let series_id = source
        .get("recurringEventId")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let original_start = source
        .get("originalStartTime")
        .map(|v| parse_time(v, calendar_zone))
        .transpose()?;
    if series_id.is_some() != original_start.is_some() {
        return Err(error(
            "invalid_identity",
            "An instance requires both series ID and originalStartTime",
        ));
    }
    let recurrence: Vec<String> = match source.get("recurrence") {
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|_| error("invalid_rule", "Recurrence must be a list of strings"))?,
        None => vec![],
    };
    if recurrence.len() > 8 || recurrence.iter().map(String::len).sum::<usize>() > 8192 {
        return Err(error("limit_exceeded", "Recurrence source is too large"));
    }
    if series_id.is_some() && !recurrence.is_empty() {
        return Err(error(
            "invalid_identity",
            "An instance cannot own a recurrence rule",
        ));
    }
    let conference_url = source["conferenceData"]["entryPoints"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["entryPointType"] == "video")
                .and_then(|item| item["uri"].as_str())
        })
        .map(str::to_owned);
    Ok(Event {
        key: EventKey {
            account: account.into(),
            calendar: calendar.into(),
            event: string(source, "id")?.into(),
        },
        etag: source["etag"].as_str().unwrap_or("").into(),
        cancelled,
        summary: source["summary"].as_str().unwrap_or("(Untitled)").into(),
        description: source["description"].as_str().unwrap_or("").into(),
        location: source["location"].as_str().unwrap_or("").into(),
        organizer: source["organizer"]["email"].as_str().map(str::to_owned),
        attendees: source["attendees"].as_array().cloned().unwrap_or_default(),
        private: source["visibility"] == "private",
        series_private: None,
        conference_url,
        series_id,
        original_start,
        start,
        end,
        recurrence,
        source: source.clone(),
    })
}

fn project(
    event: &Event,
    key: OccurrenceKey,
    start: EventTime,
    end: EventTime,
    access: Access,
) -> Option<Occurrence> {
    if access == Access::None {
        return None;
    }
    let access = access.for_event(event);
    let busy = access == Access::FreeBusy;
    Some(Occurrence {
        key,
        start,
        end,
        title: if busy {
            "Busy".into()
        } else {
            event.summary.clone()
        },
        busy_only: busy,
        read_only: !matches!(access, Access::Writer | Access::Owner),
        description: (!busy).then(|| event.description.clone()),
        location: (!busy).then(|| event.location.clone()),
        conference_url: if busy {
            None
        } else {
            event.conference_url.clone()
        },
        organizer: if busy { None } else { event.organizer.clone() },
        attendees: (!busy).then(|| event.attendees.clone()),
    })
}

pub fn expand(
    event: &Event,
    exceptions: &[Event],
    window: Window,
    access: Access,
) -> Result<Vec<Occurrence>> {
    // Revalidate publicly constructible Window values too.
    Window::new(window.from, window.to, window.calendar_zone)?;
    if event.cancelled || access == Access::None {
        return Ok(vec![]);
    }
    // An exception cannot restore fields hidden by the recurring master's privacy.
    // Keep limited-writer identity for public masters: an individual exception
    // can still be private and must not receive full writer access.
    let access = if access.for_event(event) == Access::FreeBusy {
        Access::FreeBusy
    } else {
        access
    };
    if exceptions.len() > MAX_OCCURRENCES as usize {
        return Err(error(
            "limit_exceeded",
            "Too many exceptions for one bounded projection",
        ));
    }
    let start = event
        .start
        .as_ref()
        .ok_or_else(|| error("invalid_event", "Missing event start"))?;
    let end = event
        .end
        .as_ref()
        .ok_or_else(|| error("invalid_event", "Missing event end"))?;
    let series = event.series_id.as_ref().unwrap_or(&event.key.event);
    let mut overrides = BTreeMap::new();
    for exception in exceptions {
        if exception.key.account != event.key.account
            || exception.key.calendar != event.key.calendar
            || exception.series_id.as_ref() != Some(series)
        {
            return Err(error(
                "scope_mismatch",
                "Exception does not belong to this account/calendar/series",
            ));
        }
        let original = exception
            .original_start
            .as_ref()
            .ok_or_else(|| error("invalid_identity", "Exception is missing originalStartTime"))?
            .identity();
        if overrides.insert(original, exception).is_some() {
            return Err(error(
                "duplicate_exception",
                "Multiple versions of one exception must be reconciled before expansion",
            ));
        }
    }
    let mut dates = vec![start.clone()];
    if !event.recurrence.is_empty() {
        let dtstart = match start {
            EventTime::Zoned { instant, time_zone } => {
                let zone: Tz = time_zone
                    .parse()
                    .map_err(|_| error("unsupported_zone", "Invalid event zone"))?;
                format!(
                    "DTSTART;TZID={}:{}",
                    time_zone,
                    instant.with_timezone(&zone).format("%Y%m%dT%H%M%S")
                )
            }
            EventTime::Date { date } => format!("DTSTART:{}T000000Z", date.format("%Y%m%d")),
        };
        for rule in &event.recurrence {
            if rule.contains('\r')
                || rule.contains('\n')
                || !(rule.starts_with("RRULE:")
                    || rule.starts_with("RDATE")
                    || rule.starts_with("EXDATE"))
            {
                return Err(error(
                    "unsupported_rule",
                    "Only separate RRULE, RDATE and EXDATE properties are supported",
                ));
            }
            if rule.starts_with("RRULE:")
                && ["FREQ=SECONDLY", "FREQ=MINUTELY", "FREQ=HOURLY"]
                    .iter()
                    .any(|freq| rule.contains(freq))
            {
                return Err(error(
                    "unsupported_rule",
                    "Sub-daily recurrence requires a separately bounded implementation",
                ));
            }
        }
        let mut recurrence = event.recurrence.clone();
        if matches!(start, EventTime::Date { .. }) {
            // rrule's DATE exclusions otherwise use a different internal zone
            // from our UTC-midnight date carrier. Normalize the carrier only;
            // the retained Google source and public all-day dates stay unchanged.
            for rule in &mut recurrence {
                if let Some((property, values)) = rule.split_once(";VALUE=DATE:") {
                    if property != "RDATE" && property != "EXDATE" {
                        return Err(error(
                            "unsupported_rule",
                            "Unsupported all-day date property",
                        ));
                    }
                    let dates = values
                        .split(',')
                        .map(|date| {
                            NaiveDate::parse_from_str(date, "%Y%m%d")
                                .map(|date| format!("{}T000000Z", date.format("%Y%m%d")))
                                .map_err(|_| {
                                    error("invalid_rule", "Invalid all-day recurrence date")
                                })
                        })
                        .collect::<Result<Vec<_>>>()?;
                    *rule = format!("{property}:{}", dates.join(","));
                }
            }
        }
        let text = format!("{}\n{}", dtstart, recurrence.join("\n"));
        let rules: RRuleSet = text.parse().map_err(|e| {
            error(
                "unsupported_rule",
                format!("Recurrence cannot be represented faithfully: {e}"),
            )
        })?;
        let duration = end.utc(window.calendar_zone)? - start.utc(window.calendar_zone)?;
        // Include starts before the window whose duration overlaps its first instant.
        let lower = window
            .from
            .checked_sub_signed(duration)
            .and_then(|value| value.checked_sub_signed(Duration::days(1)))
            .ok_or_else(|| {
                error(
                    "limit_exceeded",
                    "Event duration exceeds representable projection boundaries",
                )
            })?;
        let result = rules
            .after(lower.with_timezone(&RuleTz::UTC))
            .before(window.to.with_timezone(&RuleTz::UTC))
            .all(MAX_OCCURRENCES);
        if result.limited {
            return Err(error("limit_exceeded", "Recurrence projection reached its work/output limit; no partial completeness claim"));
        }
        dates = result
            .dates
            .into_iter()
            .map(|date| match start {
                EventTime::Date { .. } => EventTime::Date {
                    date: date.date_naive(),
                },
                EventTime::Zoned { time_zone, .. } => EventTime::Zoned {
                    instant: date.fixed_offset(),
                    time_zone: time_zone.clone(),
                },
            })
            .collect();
    }
    let mut seen = BTreeSet::new();
    let mut output = vec![];
    for candidate in dates {
        let original = event
            .original_start
            .as_ref()
            .map(EventTime::identity)
            .unwrap_or_else(|| candidate.identity());
        seen.insert(original.clone());
        if let Some(exception) = overrides.get(&original) {
            append_exception(
                &mut output,
                &exception.with_recurring_master(event)?,
                window,
                access,
            )?;
            continue;
        }
        let occurrence_end = match (start, end, &candidate) {
            (
                EventTime::Date { date: a },
                EventTime::Date { date: b },
                EventTime::Date { date },
            ) => EventTime::Date {
                date: date
                    .checked_add_signed(*b - *a)
                    .ok_or_else(|| error("invalid_time", "Occurrence end date is out of range"))?,
            },
            (
                EventTime::Zoned { instant: a, .. },
                EventTime::Zoned { instant: b, .. },
                EventTime::Zoned { instant, time_zone },
            ) => {
                let zone: Tz = time_zone
                    .parse()
                    .map_err(|_| error("unsupported_zone", "Invalid occurrence zone"))?;
                EventTime::Zoned {
                    instant: instant
                        .checked_add_signed(*b - *a)
                        .ok_or_else(|| {
                            error("invalid_time", "Occurrence end time is out of range")
                        })?
                        .with_timezone(&zone)
                        .fixed_offset(),
                    time_zone: time_zone.clone(),
                }
            }
            _ => return Err(error("invalid_time", "Mixed event time kinds")),
        };
        if window.overlaps(&candidate, &occurrence_end)? {
            let key = OccurrenceKey {
                account: event.key.account.clone(),
                calendar: event.key.calendar.clone(),
                series: series.clone(),
                original_start: (!event.recurrence.is_empty() || event.series_id.is_some())
                    .then_some(original),
            };
            if let Some(row) = project(event, key, candidate, occurrence_end, access) {
                output.push(row);
            }
        }
    }
    // A moved exception can enter this window from an original start outside it.
    for (original, exception) in overrides {
        if !seen.contains(&original)
            && exception.original_start.as_ref().is_some_and(|original| {
                recurring::within_series_end(event, original).unwrap_or(false)
            })
        {
            append_exception(
                &mut output,
                &exception.with_recurring_master(event)?,
                window,
                access,
            )?;
        }
    }
    output.sort_by_key(|occurrence| occurrence.start.identity());
    if output.len() > MAX_OCCURRENCES as usize {
        return Err(error("limit_exceeded", "Too many occurrences"));
    }
    Ok(output)
}

fn append_exception(
    output: &mut Vec<Occurrence>,
    event: &Event,
    window: Window,
    access: Access,
) -> Result<()> {
    if event.cancelled {
        return Ok(());
    }
    let start = event
        .start
        .clone()
        .ok_or_else(|| error("invalid_event", "Moved instance missing start"))?;
    let end = event
        .end
        .clone()
        .ok_or_else(|| error("invalid_event", "Moved instance missing end"))?;
    if window.overlaps(&start, &end)? {
        let key = OccurrenceKey {
            account: event.key.account.clone(),
            calendar: event.key.calendar.clone(),
            series: event
                .series_id
                .clone()
                .ok_or_else(|| error("invalid_identity", "Missing recurringEventId"))?,
            original_start: event.original_start.as_ref().map(EventTime::identity),
        };
        if let Some(row) = project(event, key, start, end, access) {
            output.push(row);
        }
    }
    Ok(())
}

/// Stable measured capabilities; no live Google behavior is implied.
pub fn capabilities() -> Value {
    json!({"fixture_revision":FIXTURE_REVISION,"tzdb":TZDB_VERSION,"max_window_days":MAX_WINDOW_DAYS,"max_occurrences":MAX_OCCURRENCES,"live_google":false,"external_feeds":false})
}

/// Normalize a Core-owned cached Google event collection, then produce the single
/// permission-aware row shape consumed by the current CLI, week and today strip.
pub fn project_google_events(
    account: &str,
    calendar: &str,
    calendar_zone: &str,
    sources: &[Value],
    from: NaiveDate,
    to: NaiveDate,
    access: Access,
) -> Result<Vec<Value>> {
    project_google_events_in_window(
        account,
        calendar,
        calendar_zone,
        sources,
        Window::from_dates(from, to, calendar_zone)?,
        access,
    )
}

/// Project calendars with distinct source zones into one common display window.
/// `calendar_zone` normalizes provider source and recurrence. `window` determines
/// which displayed dates/instants are visible, including stable all-day dates.
pub fn project_google_events_in_window(
    account: &str,
    calendar: &str,
    calendar_zone: &str,
    sources: &[Value],
    window: Window,
    access: Access,
) -> Result<Vec<Value>> {
    if sources.len() > 5000 {
        return Err(error(
            "limit_exceeded",
            "Too many source events for one projection",
        ));
    }
    Window::new(window.from, window.to, window.calendar_zone)?;
    let _: Tz = calendar_zone
        .parse()
        .map_err(|_| error("unsupported_zone", "Invalid calendar zone"))?;
    let events = sources
        .iter()
        .map(|source| normalize(account, calendar, calendar_zone, source))
        .collect::<Result<Vec<_>>>()?;
    let masters = events
        .iter()
        .filter(|event| event.series_id.is_none())
        .map(|event| event.key.event.as_str())
        .collect::<BTreeSet<_>>();
    let mut exceptions: BTreeMap<&str, Vec<Event>> = BTreeMap::new();
    for event in &events {
        if let Some(series) = event.series_id.as_deref() {
            exceptions.entry(series).or_default().push(event.clone());
        }
    }
    let mut seen = BTreeSet::new();
    let mut output = Vec::new();
    for event in &events {
        if !seen.insert(event.key.event.clone()) {
            return Err(error(
                "duplicate_event",
                "Reconcile event versions before projecting",
            ));
        }
        if event
            .series_id
            .as_deref()
            .is_some_and(|series| masters.contains(series))
        {
            continue;
        }
        let instances = exceptions
            .get(event.key.event.as_str())
            .map_or(&[][..], Vec::as_slice);
        output.extend(project_event_in_window(
            event,
            instances,
            calendar_zone,
            window,
            access,
        )?);
        if output.len() > MAX_OCCURRENCES as usize {
            return Err(error(
                "limit_exceeded",
                "Projected calendar contains too many occurrences",
            ));
        }
    }
    sort_projected_events(&mut output, window)?;
    Ok(output)
}

/// Project one already normalized candidate and its bounded, same-scope
/// instances. Core can isolate a malformed series without rescanning all source.
pub fn project_event_in_window(
    event: &Event,
    exceptions: &[Event],
    calendar_zone: &str,
    window: Window,
    access: Access,
) -> Result<Vec<Value>> {
    let _: Tz = calendar_zone
        .parse()
        .map_err(|_| error("unsupported_zone", "Invalid calendar zone"))?;
    let calendar = &event.key.calendar;
    let mut output = Vec::new();
    for occurrence in expand(event, exceptions, window, access)? {
        let (start, end, all_day, time_zone) = match (&occurrence.start, &occurrence.end) {
            (EventTime::Date { date: start }, EventTime::Date { date: end }) => (
                start.to_string(),
                end.to_string(),
                true,
                calendar_zone.to_owned(),
            ),
            (
                EventTime::Zoned {
                    instant: start,
                    time_zone,
                },
                EventTime::Zoned { instant: end, .. },
            ) => (
                start.to_rfc3339(),
                end.to_rfc3339(),
                false,
                time_zone.clone(),
            ),
            _ => return Err(error("invalid_time", "Mixed projected event types")),
        };
        let mut row = json!({
            "id":serde_json::to_string(&occurrence.key).map_err(|_| error("invalid_identity", "Cannot encode occurrence identity"))?,
            "occurrence_key":occurrence.key,
            "calendar_id":calendar,"title":occurrence.title,"start":start,"end":end,
            "time_zone":time_zone,"all_day":all_day,"read_only":occurrence.read_only,"busy_only":occurrence.busy_only
        });
        if let Some(value) = occurrence.description {
            row["description"] = value.into();
        }
        if let Some(value) = occurrence.location {
            row["location"] = value.into();
        }
        if let Some(value) = occurrence.conference_url {
            row["conference_url"] = value.into();
        }
        if let Some(value) = occurrence.organizer {
            row["organizer"] = value.into();
        }
        if let Some(value) = occurrence.attendees {
            row["attendees"] = value.into();
        }
        output.push(row);
        if output.len() > MAX_OCCURRENCES as usize {
            return Err(error(
                "limit_exceeded",
                "Projected calendar contains too many occurrences",
            ));
        }
    }
    sort_projected_events(&mut output, window)?;
    Ok(output)
}

/// Stable shared ordering: displayed date, all-day before timed on that date,
/// then the actual timed instant and identity. Offset timestamp text is not an
/// ordering key. All-day source dates are never converted into another date.
pub fn sort_projected_events(rows: &mut Vec<Value>, window: Window) -> Result<()> {
    if rows.len() > MAX_OCCURRENCES as usize {
        return Err(error("limit_exceeded", "Too many projected rows to order"));
    }
    let keys = rows
        .iter()
        .map(|row| {
            let start = string(row, "start")?;
            let id = row["id"]
                .as_str()
                .ok_or_else(|| error("invalid_identity", "Projected row has no identity"))?
                .to_owned();
            if row["all_day"] == true {
                let date = NaiveDate::parse_from_str(start, "%Y-%m-%d")
                    .map_err(|_| error("invalid_time", "Invalid projected all-day date"))?;
                Ok((date, false, None, id))
            } else {
                let instant = DateTime::parse_from_rfc3339(start)
                    .map_err(|_| error("invalid_time", "Invalid projected instant"))?;
                Ok((
                    instant.with_timezone(&window.calendar_zone).date_naive(),
                    true,
                    Some(instant),
                    id,
                ))
            }
        })
        .collect::<Result<Vec<_>>>()?;
    let mut keyed = keys
        .into_iter()
        .zip(std::mem::take(rows))
        .collect::<Vec<_>>();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    *rows = keyed.into_iter().map(|(_, row)| row).collect();
    Ok(())
}
