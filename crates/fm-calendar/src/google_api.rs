//! One-request Google Calendar adapter. Core owns durable staging and permissions;
//! the shared scheduler owns capacity, cooldowns and any explicitly safe retry.
use crate::{Access, EventKey, GoogleWrite, Window};
use chrono::{DateTime, Utc};
use fm_google::transport::{
    CancellationToken, HttpMethod, HttpRequest, HttpResponse, HttpTransport, TransportError,
};
use fm_google::{classify_api_error, decode_json, AccountId, GoogleError};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use url::Url;

/// Names the fixed query [`GoogleCalendarApi::calendar_list`] sends. Core
/// stores it with each sync cursor and refuses to resume a cursor recorded
/// under a different value, so changing the query must change this string.
pub const CATALOG_SYNC_FINGERPRINT: &str =
    "google-calendar-catalog-v1:hidden=true;deleted=true;max=250";
/// Names the fixed query [`GoogleCalendarApi::events`] sends, with the same
/// role as [`CATALOG_SYNC_FINGERPRINT`].
pub const EVENT_SYNC_FINGERPRINT: &str =
    "google-calendar-events-v1:single=false;deleted=true;hiddenInvitations=true;max=250";
/// Response cap for list pages and free/busy.
const PAGE_BYTES: usize = 16 * 1024 * 1024;
/// Response and request-body cap for one event.
const EVENT_BYTES: usize = 2 * 1024 * 1024;

mod occurrences;
pub use occurrences::{InstanceIdentity, InstancesPage, OccurrenceResource};

/// Why a Calendar API call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// An argument failed local checks; nothing was sent.
    InvalidRequest,
    /// Google's answer was malformed, oversized, inconsistent with the request,
    /// or had an unexpected status.
    InvalidResponse,
    /// HTTP 410 on a calendar-list or events sync: start again with a full sync.
    SyncTokenExpired,
    /// HTTP 412: the event's ETag no longer matches `If-Match`.
    PreconditionFailed,
    /// HTTP 409, for example an insert whose event ID already exists.
    Conflict,
    /// The transport refused, cancelled or failed the request, or the response
    /// exceeded its size cap.
    Transport(TransportError),
    /// Any other HTTP error, classified by `fm_google::classify_api_error`.
    Provider(GoogleError),
}
impl From<TransportError> for ApiError {
    fn from(error: TransportError) -> Self {
        Self::Transport(error)
    }
}
type Result<T> = std::result::Result<T, ApiError>;

/// Where a list request resumes: `Default` starts a full listing.
///
/// No Debug: cursor material remains native, never diagnostic text.
#[derive(Clone, Copy, Default)]
pub struct SyncCursor<'a> {
    /// `nextSyncToken` of the last complete run, for an incremental listing.
    pub sync_token: Option<&'a str>,
    /// `nextPageToken` of the previous page in this run.
    pub page_token: Option<&'a str>,
}

/// The account's access role on a calendar, from its calendar-list entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarRole {
    /// No access, an unknown role, or a deleted entry.
    None,
    FreeBusy,
    Reader,
    WriterWithoutPrivateAccess,
    Writer,
    Owner,
}
impl CalendarRole {
    /// Maps Google's `accessRole` string. Unknown values become `None`, the
    /// safe default.
    pub fn from_provider(value: &str) -> Self {
        match value {
            "freeBusyReader" => Self::FreeBusy,
            "reader" => Self::Reader,
            "writerWithoutPrivateAccess" => Self::WriterWithoutPrivateAccess,
            "writer" => Self::Writer,
            "owner" => Self::Owner,
            _ => Self::None,
        }
    }
    /// Returns the [`Access`] for one event on this calendar: readers and
    /// limited writers see a private event as free/busy only.
    pub fn event_access(self, private: bool) -> Access {
        match self {
            Self::None => Access::None,
            Self::FreeBusy => Access::FreeBusy,
            Self::Reader if private => Access::FreeBusy,
            Self::Reader => Access::Reader,
            Self::WriterWithoutPrivateAccess if private => Access::FreeBusy,
            Self::WriterWithoutPrivateAccess => Access::WriterWithoutPrivateAccess,
            Self::Writer => Access::Writer,
            Self::Owner => Access::Owner,
        }
    }
}

/// One calendar-list entry.
///
/// Retains complete provider catalog metadata, including fields not displayed.
pub struct CalendarEntry {
    pub id: String,
    /// `None` for a deleted entry, whatever role it last had.
    pub role: CalendarRole,
    /// The user removed this calendar from their list.
    pub deleted: bool,
    /// The complete entry JSON.
    pub source: Value,
}
/// One page of the account's calendar list.
pub struct CatalogPage {
    pub account: AccountId,
    pub items: Vec<CalendarEntry>,
    /// Exactly one of `next_page_token` and `next_sync_token` is set: more
    /// pages follow, or this was the last and the token resumes the next run.
    pub next_page_token: Option<String>,
    pub next_sync_token: Option<String>,
    /// Collection ETag, from the body or the header.
    pub etag: Option<String>,
}
/// One page of a calendar's events. Recurring masters and their exceptions
/// arrive as separate, unexpanded items.
pub struct EventPage {
    pub account: AccountId,
    pub calendar: String,
    /// Retained source, not permission-filtered UI data. Normalize per item in Core.
    pub items: Vec<Value>,
    /// Exactly one continuation is set, as in [`CatalogPage`].
    pub next_page_token: Option<String>,
    pub next_sync_token: Option<String>,
    pub etag: Option<String>,
    /// The calendar's IANA zone, checked against the bundled database.
    pub time_zone: String,
    /// The account's role on this calendar when the page was served.
    pub role: CalendarRole,
    /// The calendar's `defaultReminders` array; `[]` when absent.
    pub default_reminders: Value,
}
/// A complete event with its ETag, as returned by a read, patch or insert.
pub struct EventResource {
    pub key: EventKey,
    /// From the body or the header; always present.
    pub etag: String,
    pub source: Value,
}
/// The result of [`GoogleCalendarApi::get_event`].
pub enum EventFetch {
    /// HTTP 304: the event still has the ETag given as `if_none_match`.
    NotModified,
    Found(EventResource),
}

/// Why Google gave no busy times for one calendar in a free/busy query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BusyError {
    /// The response had no entry for this calendar.
    Missing,
    /// `forbidden` or `permissionDenied`.
    PermissionDenied,
    /// `notFound`.
    NotFound,
    /// `internalError`.
    Internal,
    /// `groupTooBig` or `tooManyCalendarsRequested`.
    Limit,
    /// Any other reason.
    Unknown,
    /// The entry was malformed, or an interval fell outside the window.
    InvalidResponse,
}
/// One busy span in UTC, inside the requested window.
pub struct BusyInterval {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}
/// Free/busy outcome for one calendar.
pub enum CalendarBusy {
    /// Busy spans, possibly none.
    Available(Vec<BusyInterval>),
    /// Google reported errors, or the entry could not be trusted.
    Unavailable(Vec<BusyError>),
}
/// Free/busy results for exactly the requested calendars over one window.
pub struct BusySnapshot {
    pub account: AccountId,
    /// The requested window, which Google echoed back unchanged.
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    /// There are no provider event IDs on this path; never fabricate them.
    pub calendars: BTreeMap<String, CalendarBusy>,
}

/// Google's `sendUpdates`: which guests get email about a change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationPolicy {
    /// Every guest.
    All,
    /// Only guests who do not use Google Calendar.
    ExternalOnly,
    /// No guests.
    None,
}
impl NotificationPolicy {
    fn parameter(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::ExternalOnly => "externalOnly",
            Self::None => "none",
        }
    }
}

/// Calendar API v3 client for one account over an injected transport.
///
/// Each method sends exactly one request and never retries; quota, retry and
/// token refresh belong to the caller and its transport. `access_token` goes
/// out as a bearer `authorization` header, which a wrapping transport may
/// replace. Responses are capped at 16 MiB per page and 2 MiB per event.
pub struct GoogleCalendarApi<T: HttpTransport> {
    account: AccountId,
    http: Arc<T>,
}
impl<T: HttpTransport> GoogleCalendarApi<T> {
    /// Creates a client for `account`. Results carry this account, and writes
    /// to any other account are refused.
    pub fn new(account: AccountId, http: Arc<T>) -> Self {
        Self { account, http }
    }
    /// Returns the account this client acts for.
    pub fn account(&self) -> &AccountId {
        &self.account
    }

    /// Fetches one page of the account's calendar list, hidden and deleted
    /// entries included; with `cursor.sync_token`, only changes since then.
    ///
    /// Fails with [`ApiError::SyncTokenExpired`] when Google has expired the
    /// sync token, and `InvalidResponse` unless exactly one continuation token
    /// comes back, or when the next page token repeats the current one.
    pub async fn calendar_list(
        &self,
        access_token: &str,
        cursor: SyncCursor<'_>,
        cancel: &CancellationToken,
    ) -> Result<CatalogPage> {
        let mut url = endpoint(&["users", "me", "calendarList"])?;
        url.query_pairs_mut()
            .append_pair("maxResults", "250")
            .append_pair("showDeleted", "true")
            .append_pair("showHidden", "true");
        add_cursor(&mut url, cursor)?;
        let response = self
            .execute(
                request(HttpMethod::Get, &url, access_token, PAGE_BYTES)?,
                cancel,
                true,
            )
            .await?;
        let value = parse_json(&response.body, PAGE_BYTES)?;
        let (next_page_token, next_sync_token) = continuation(&value, cursor)?;
        let etag = response_etag(&value, response.etag)?;
        let items = page_items(&value, 250)?
            .into_iter()
            .map(catalog_entry)
            .collect::<Result<_>>()?;
        Ok(CatalogPage {
            account: self.account.clone(),
            items,
            next_page_token,
            next_sync_token,
            etag,
        })
    }

    /// Fetches one calendar-list entry, re-reading the account's current role
    /// and metadata for that calendar.
    ///
    /// Catalog deltas cannot replace this explicit permission/metadata revalidation.
    pub async fn calendar_entry(
        &self,
        access_token: &str,
        calendar: &str,
        cancel: &CancellationToken,
    ) -> Result<CalendarEntry> {
        let url = endpoint(&["users", "me", "calendarList", calendar])?;
        let response = self
            .execute(
                request(HttpMethod::Get, &url, access_token, EVENT_BYTES)?,
                cancel,
                false,
            )
            .await?;
        let entry = catalog_entry(parse_json(&response.body, EVENT_BYTES)?)?;
        // `primary` is an alias; Google answers with the calendar's real ID.
        if calendar != "primary" && entry.id != calendar {
            return Err(ApiError::InvalidResponse);
        }
        Ok(entry)
    }

    /// Fetches one page of a calendar's events for a full or incremental sync.
    ///
    /// Fixed full/delta scope. No expansion, partial fields, attendee cap, time or search filter.
    /// Deleted events and hidden invitations are included. Fails with
    /// [`ApiError::SyncTokenExpired`] on an expired token, and `InvalidResponse`
    /// when the calendar's zone is not a known IANA zone.
    pub async fn events(
        &self,
        access_token: &str,
        calendar: &str,
        cursor: SyncCursor<'_>,
        cancel: &CancellationToken,
    ) -> Result<EventPage> {
        let mut url = endpoint(&["calendars", calendar, "events"])?;
        url.query_pairs_mut()
            .append_pair("maxResults", "250")
            .append_pair("showDeleted", "true")
            .append_pair("singleEvents", "false")
            .append_pair("showHiddenInvitations", "true");
        add_cursor(&mut url, cursor)?;
        let response = self
            .execute(
                request(HttpMethod::Get, &url, access_token, PAGE_BYTES)?,
                cancel,
                true,
            )
            .await?;
        let value = parse_json(&response.body, PAGE_BYTES)?;
        let (next_page_token, next_sync_token) = continuation(&value, cursor)?;
        let etag = response_etag(&value, response.etag)?;
        let time_zone = required_text(&value, "timeZone", 256)?.to_owned();
        time_zone
            .parse::<chrono_tz::Tz>()
            .map_err(|_| ApiError::InvalidResponse)?;
        let role = CalendarRole::from_provider(value["accessRole"].as_str().unwrap_or("none"));
        let default_reminders = value
            .get("defaultReminders")
            .cloned()
            .unwrap_or_else(|| json!([]));
        if !default_reminders.is_array() {
            return Err(ApiError::InvalidResponse);
        }
        Ok(EventPage {
            account: self.account.clone(),
            calendar: calendar.to_owned(),
            items: page_items(&value, 250)?,
            next_page_token,
            next_sync_token,
            etag,
            time_zone,
            role,
            default_reminders,
        })
    }

    /// Fetches one event, conditionally when `if_none_match` holds its last ETag.
    ///
    /// The response must name the requested event and carry an ETag. A 304 is
    /// accepted only for a conditional request.
    pub async fn get_event(
        &self,
        access_token: &str,
        calendar: &str,
        event: &str,
        if_none_match: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<EventFetch> {
        let url = endpoint(&["calendars", calendar, "events", event])?;
        let mut request = request(HttpMethod::Get, &url, access_token, EVENT_BYTES)?;
        if let Some(etag) = if_none_match {
            validate_etag(etag)?;
            request = request.header("if-none-match", etag)?;
        }
        // Execute directly: `check_response` would treat the 304 as an error.
        let response = self.http.execute(request, cancel).await?;
        if response.status == 304 {
            if if_none_match.is_none() {
                return Err(ApiError::InvalidResponse);
            }
            return Ok(EventFetch::NotModified);
        }
        check_response(&response, false)?;
        Ok(EventFetch::Found(event_resource(
            &self.account,
            calendar,
            event,
            response,
        )?))
    }

    /// Queries busy spans for up to 50 distinct calendars over `window`.
    ///
    /// The snapshot has one entry per requested calendar; calendars Google adds
    /// by expanding groups are dropped. Fails with `InvalidRequest` for an
    /// invalid window or an empty, oversized or repeating list, and
    /// `InvalidResponse` when Google echoes a different window.
    pub async fn free_busy(
        &self,
        access_token: &str,
        calendars: &[String],
        window: &Window,
        cancel: &CancellationToken,
    ) -> Result<BusySnapshot> {
        // Revalidate public Window fields; no unbounded arbitrary-provider query.
        Window::new(window.from, window.to, window.calendar_zone)
            .map_err(|_| ApiError::InvalidRequest)?;
        if calendars.is_empty() || calendars.len() > 50 {
            return Err(ApiError::InvalidRequest);
        }
        let mut distinct = BTreeSet::new();
        for id in calendars {
            validate_segment(id)?;
            if !distinct.insert(id) {
                return Err(ApiError::InvalidRequest);
            }
        }
        let url = endpoint(&["freeBusy"])?;
        let body = json!({"timeMin":window.from.to_rfc3339(),"timeMax":window.to.to_rfc3339(),"timeZone":"UTC",
            "calendarExpansionMax":50,"items":calendars.iter().map(|id| json!({"id":id})).collect::<Vec<_>>()});
        let request = request(HttpMethod::Post, &url, access_token, PAGE_BYTES)?
            .body(encode_body(&body)?, "application/json")?;
        let response = self.execute(request, cancel, false).await?;
        let value = parse_json(&response.body, PAGE_BYTES)?;
        if instant(&value["timeMin"])? != window.from || instant(&value["timeMax"])? != window.to {
            return Err(ApiError::InvalidResponse);
        }
        let returned = value["calendars"]
            .as_object()
            .ok_or(ApiError::InvalidResponse)?;
        // Do not expose unsolicited group expansion or calendars beyond the exact request.
        let calendars = calendars
            .iter()
            .map(|id| (id.clone(), busy_calendar(returned.get(id), window)))
            .collect();
        Ok(BusySnapshot {
            account: self.account.clone(),
            from: window.from,
            to: window.to,
            calendars,
        })
    }

    /// Caller has already journaled intent and refreshed permissions/source. Submits once.
    /// Patch preserves fields omitted by the form; arrays in the plan replace whole arrays.
    ///
    /// Fails with `PreconditionFailed` when the event changed since `if_match`.
    pub async fn apply_write(
        &self,
        access_token: &str,
        write: &GoogleWrite,
        cancel: &CancellationToken,
    ) -> Result<EventResource> {
        if write.target.account != self.account.as_str()
            || write.notification_owner != "google_calendar_api"
            || !write.body.is_object()
            || write.body.get("id").is_some()
            || write.body.get("recurringEventId").is_some()
            || write.body.get("originalStartTime").is_some()
        {
            return Err(ApiError::InvalidRequest);
        }
        let policy = match write.send_updates {
            "all" => NotificationPolicy::All,
            "externalOnly" => NotificationPolicy::ExternalOnly,
            "none" => NotificationPolicy::None,
            _ => return Err(ApiError::InvalidRequest),
        };
        validate_etag(&write.if_match)?;
        let mut url = endpoint(&[
            "calendars",
            &write.target.calendar,
            "events",
            &write.target.event,
        ])?;
        write_parameters(&mut url, policy);
        // Event labels are distinct from legacy colorId. Google requires the
        // version alongside an exact retained label ID when editing such events.
        match write.body.get("eventLabelId") {
            None => {}
            Some(Value::String(id)) if !id.is_empty() => {
                url.query_pairs_mut().append_pair("eventLabelVersion", "1");
            }
            Some(_) => return Err(ApiError::InvalidRequest),
        }
        let request = request(HttpMethod::Patch, &url, access_token, EVENT_BYTES)?
            .header("if-match", &write.if_match)?
            .body(encode_body(&write.body)?, "application/json")?;
        let response = self.execute(request, cancel, false).await?;
        event_resource(
            &self.account,
            &write.target.calendar,
            &write.target.event,
            response,
        )
    }

    /// Stable client-generated ID must already be committed in Core's operation journal.
    ///
    /// Google requires 5 to 1024 base32hex characters (`0-9`, `a-v`) for a
    /// client-chosen ID. Reusing it makes a repeated insert fail with `Conflict`
    /// instead of creating a second event.
    pub async fn insert_event(
        &self,
        access_token: &str,
        calendar: &str,
        stable_id: &str,
        body: &Value,
        notifications: NotificationPolicy,
        cancel: &CancellationToken,
    ) -> Result<EventResource> {
        if !(5..=1024).contains(&stable_id.len())
            || !stable_id
                .bytes()
                .all(|v| v.is_ascii_digit() || (b'a'..=b'v').contains(&v))
            || !body.is_object()
            || body.get("id").is_some()
            || body.get("recurringEventId").is_some()
            || body.get("originalStartTime").is_some()
        {
            return Err(ApiError::InvalidRequest);
        }
        let mut body = body.clone();
        body["id"] = stable_id.into();
        let mut url = endpoint(&["calendars", calendar, "events"])?;
        write_parameters(&mut url, notifications);
        match body.get("eventLabelId") {
            None => {}
            Some(Value::String(id)) if !id.is_empty() => {
                url.query_pairs_mut().append_pair("eventLabelVersion", "1");
            }
            Some(_) => return Err(ApiError::InvalidRequest),
        }
        let request = request(HttpMethod::Post, &url, access_token, EVENT_BYTES)?
            .body(encode_body(&body)?, "application/json")?;
        let response = self.execute(request, cancel, false).await?;
        event_resource(&self.account, calendar, stable_id, response)
    }

    /// A successful delete is distinct from 404/410. Reconciliation owns prior outcomes.
    /// Only HTTP 204 counts as success; the delete is conditional on `etag`.
    pub async fn delete_event(
        &self,
        access_token: &str,
        calendar: &str,
        event: &str,
        etag: &str,
        notifications: NotificationPolicy,
        cancel: &CancellationToken,
    ) -> Result<()> {
        validate_etag(etag)?;
        let mut url = endpoint(&["calendars", calendar, "events", event])?;
        url.query_pairs_mut()
            .append_pair("sendUpdates", notifications.parameter());
        let request = request(HttpMethod::Delete, &url, access_token, EVENT_BYTES)?
            .header("if-match", etag)?;
        let response = self.execute(request, cancel, false).await?;
        if response.status != 204 {
            return Err(ApiError::InvalidResponse);
        }
        Ok(())
    }

    async fn execute(
        &self,
        request: HttpRequest,
        cancel: &CancellationToken,
        sync: bool,
    ) -> Result<HttpResponse> {
        let response = self.http.execute(request, cancel).await?;
        check_response(&response, sync)?;
        Ok(response)
    }
}

/// Refuses a path segment that would not survive as one literal segment:
/// empty, `.` or `..`, over 1024 bytes, or with control characters.
fn validate_segment(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 1024
        || matches!(value, "." | "..")
        || value.chars().any(char::is_control)
    {
        return Err(ApiError::InvalidRequest);
    }
    Ok(())
}
fn endpoint(segments: &[&str]) -> Result<Url> {
    let mut url = Url::parse("https://www.googleapis.com/calendar/v3/")
        .map_err(|_| ApiError::InvalidRequest)?;
    {
        let mut path = url
            .path_segments_mut()
            .map_err(|_| ApiError::InvalidRequest)?;
        path.pop_if_empty();
        for segment in segments {
            validate_segment(segment)?;
            path.push(segment);
        }
    }
    Ok(url)
}
fn add_cursor(url: &mut Url, cursor: SyncCursor<'_>) -> Result<()> {
    for (key, value) in [
        ("syncToken", cursor.sync_token),
        ("pageToken", cursor.page_token),
    ] {
        if let Some(value) = value {
            if !bounded_text(value, 8192) {
                return Err(ApiError::InvalidRequest);
            }
            url.query_pairs_mut().append_pair(key, value);
        }
    }
    Ok(())
}
fn request(method: HttpMethod, url: &Url, token: &str, limit: usize) -> Result<HttpRequest> {
    if !bounded_text(token, 8192) || token.chars().any(char::is_whitespace) {
        return Err(ApiError::InvalidRequest);
    }
    Ok(HttpRequest::new(method, url.as_str())?
        .header("authorization", &format!("Bearer {token}"))?
        .header("accept", "application/json")?
        .response_limit(limit))
}
/// Maps an HTTP status to success or an [`ApiError`]. Only sync listings read
/// 410 as an expired token; elsewhere it is an ordinary provider error.
fn check_response(response: &HttpResponse, sync: bool) -> Result<()> {
    match response.status {
        200 | 201 | 204 => Ok(()),
        410 if sync => Err(ApiError::SyncTokenExpired),
        412 => Err(ApiError::PreconditionFailed),
        409 => Err(ApiError::Conflict),
        _ => Err(ApiError::Provider(classify_api_error(
            response.status,
            &response.body,
            response.retry_after_secs,
            false,
        ))),
    }
}
fn parse_json(bytes: &[u8], cap: usize) -> Result<Value> {
    let value: Value = decode_json(bytes, cap).map_err(|_| ApiError::InvalidResponse)?;
    if !value.is_object() {
        return Err(ApiError::InvalidResponse);
    }
    Ok(value)
}
fn bounded_text(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
fn required_text<'a>(value: &'a Value, key: &str, max: usize) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|v| bounded_text(v, max))
        .ok_or(ApiError::InvalidResponse)
}
fn optional_text(value: &Value, key: &str, max: usize) -> Result<Option<String>> {
    if let Some(value) = value.get(key) {
        return value
            .as_str()
            .filter(|v| bounded_text(v, max))
            .map(|v| Some(v.to_owned()))
            .ok_or(ApiError::InvalidResponse);
    }
    Ok(None)
}
/// The page's `(nextPageToken, nextSyncToken)`. Exactly one must be present,
/// and a page must not point back to itself, or the run would loop or lose its
/// final sync token.
fn continuation(value: &Value, cursor: SyncCursor<'_>) -> Result<(Option<String>, Option<String>)> {
    let page = optional_text(value, "nextPageToken", 8192)?;
    let sync = optional_text(value, "nextSyncToken", 8192)?;
    if page.is_some() == sync.is_some()
        || page
            .as_deref()
            .is_some_and(|v| Some(v) == cursor.page_token)
    {
        return Err(ApiError::InvalidResponse);
    }
    Ok((page, sync))
}
fn page_items(value: &Value, limit: usize) -> Result<Vec<Value>> {
    // Google's empty collections may omit items. Malformed values remain a failed page.
    let items = match value.get("items") {
        None => Vec::new(),
        Some(Value::Array(items)) => items.clone(),
        _ => return Err(ApiError::InvalidResponse),
    };
    if items.len() > limit || items.iter().any(|v| !v.is_object()) {
        return Err(ApiError::InvalidResponse);
    }
    Ok(items)
}
fn catalog_entry(source: Value) -> Result<CalendarEntry> {
    let id = required_text(&source, "id", 1024)?.to_owned();
    let deleted = match source.get("deleted") {
        None => false,
        Some(Value::Bool(value)) => *value,
        _ => return Err(ApiError::InvalidResponse),
    };
    // A deleted entry grants nothing, whatever role it still lists.
    let role = if deleted {
        CalendarRole::None
    } else {
        CalendarRole::from_provider(source["accessRole"].as_str().unwrap_or("none"))
    };
    Ok(CalendarEntry {
        id,
        role,
        deleted,
        source,
    })
}
/// Accepts one concrete ETag. `*` or a list would make a conditional request
/// match any version.
fn validate_etag(value: &str) -> Result<()> {
    if !bounded_text(value, 1024) || value == "*" || value.contains(',') {
        return Err(ApiError::InvalidRequest);
    }
    Ok(())
}
/// The ETag from the body or the header, whichever is present; when both are,
/// they must agree.
fn response_etag(value: &Value, header: Option<String>) -> Result<Option<String>> {
    let body = optional_text(value, "etag", 1024)?;
    if header.as_deref().is_some_and(|v| !bounded_text(v, 1024))
        || (header.is_some() && body.is_some() && header != body)
    {
        return Err(ApiError::InvalidResponse);
    }
    Ok(body.or(header))
}
fn event_resource(
    account: &AccountId,
    calendar: &str,
    event: &str,
    response: HttpResponse,
) -> Result<EventResource> {
    let source = parse_json(&response.body, EVENT_BYTES)?;
    if required_text(&source, "id", 1024)? != event {
        return Err(ApiError::InvalidResponse);
    }
    let etag = response_etag(&source, response.etag)?.ok_or(ApiError::InvalidResponse)?;
    Ok(EventResource {
        key: EventKey {
            account: account.as_str().to_owned(),
            calendar: calendar.to_owned(),
            event: event.to_owned(),
        },
        etag,
        source,
    })
}
fn instant(value: &Value) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value.as_str().ok_or(ApiError::InvalidResponse)?)
        .map(|v| v.with_timezone(&Utc))
        .map_err(|_| ApiError::InvalidResponse)
}
fn busy_calendar(value: Option<&Value>, window: &Window) -> CalendarBusy {
    let Some(value) = value else {
        return CalendarBusy::Unavailable(vec![BusyError::Missing]);
    };
    if let Some(errors) = value.get("errors") {
        let Some(errors) = errors.as_array() else {
            return CalendarBusy::Unavailable(vec![BusyError::InvalidResponse]);
        };
        if !errors.is_empty() {
            return CalendarBusy::Unavailable(
                errors
                    .iter()
                    .map(|value| match value["reason"].as_str() {
                        Some("notFound") => BusyError::NotFound,
                        Some("forbidden" | "permissionDenied") => BusyError::PermissionDenied,
                        Some("internalError") => BusyError::Internal,
                        Some("groupTooBig" | "tooManyCalendarsRequested") => BusyError::Limit,
                        _ => BusyError::Unknown,
                    })
                    .collect(),
            );
        }
    }
    let Some(intervals) = value["busy"].as_array().filter(|v| v.len() <= 10_000) else {
        return CalendarBusy::Unavailable(vec![BusyError::InvalidResponse]);
    };
    let mut busy = Vec::new();
    for interval in intervals {
        let (Ok(start), Ok(end)) = (instant(&interval["start"]), instant(&interval["end"])) else {
            return CalendarBusy::Unavailable(vec![BusyError::InvalidResponse]);
        };
        if start >= end || start < window.from || end > window.to {
            return CalendarBusy::Unavailable(vec![BusyError::InvalidResponse]);
        }
        busy.push(BusyInterval { start, end });
    }
    CalendarBusy::Available(busy)
}
fn encode_body(value: &Value) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(value).map_err(|_| ApiError::InvalidRequest)?;
    if bytes.len() > EVENT_BYTES {
        return Err(ApiError::InvalidRequest);
    }
    Ok(bytes)
}
/// Adds the write query parameters. Declaring conference data version 1 and
/// attachment support makes Google apply those fields in a body rather than
/// ignore them.
fn write_parameters(url: &mut Url, policy: NotificationPolicy) {
    url.query_pairs_mut()
        .append_pair("sendUpdates", policy.parameter())
        .append_pair("conferenceDataVersion", "1")
        .append_pair("supportsAttachments", "true");
}

#[cfg(test)]
mod tests;
