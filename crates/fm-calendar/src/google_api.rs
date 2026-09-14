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

pub const CATALOG_SYNC_FINGERPRINT: &str =
    "google-calendar-catalog-v1:hidden=true;deleted=true;max=250";
pub const EVENT_SYNC_FINGERPRINT: &str =
    "google-calendar-events-v1:single=false;deleted=true;hiddenInvitations=true;max=250";
const PAGE_BYTES: usize = 16 * 1024 * 1024;
const EVENT_BYTES: usize = 2 * 1024 * 1024;

mod occurrences;
pub use occurrences::{InstanceIdentity, InstancesPage, OccurrenceResource};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    InvalidRequest,
    InvalidResponse,
    SyncTokenExpired,
    PreconditionFailed,
    Conflict,
    Transport(TransportError),
    Provider(GoogleError),
}
impl From<TransportError> for ApiError {
    fn from(error: TransportError) -> Self {
        Self::Transport(error)
    }
}
type Result<T> = std::result::Result<T, ApiError>;

/// No Debug: cursor material remains native, never diagnostic text.
#[derive(Clone, Copy, Default)]
pub struct SyncCursor<'a> {
    pub sync_token: Option<&'a str>,
    pub page_token: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarRole {
    None,
    FreeBusy,
    Reader,
    WriterWithoutPrivateAccess,
    Writer,
    Owner,
}
impl CalendarRole {
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
    /// Per-event mapping keeps legacy projection/edit consumers permission safe.
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
    pub fn has_event_details(self) -> bool {
        matches!(
            self,
            Self::Reader | Self::WriterWithoutPrivateAccess | Self::Writer | Self::Owner
        )
    }
}

/// Retains complete provider catalog metadata, including fields not displayed.
pub struct CalendarEntry {
    pub id: String,
    pub role: CalendarRole,
    pub deleted: bool,
    pub source: Value,
}
pub struct CatalogPage {
    pub account: AccountId,
    pub items: Vec<CalendarEntry>,
    pub next_page_token: Option<String>,
    pub next_sync_token: Option<String>,
    pub etag: Option<String>,
}
pub struct EventPage {
    pub account: AccountId,
    pub calendar: String,
    /// Retained source, not permission-filtered UI data. Normalize per item in Core.
    pub items: Vec<Value>,
    pub next_page_token: Option<String>,
    pub next_sync_token: Option<String>,
    pub etag: Option<String>,
    pub time_zone: String,
    pub role: CalendarRole,
    pub default_reminders: Value,
}
pub struct EventResource {
    pub key: EventKey,
    pub etag: String,
    pub source: Value,
}
pub enum EventFetch {
    NotModified,
    Found(EventResource),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BusyError {
    Missing,
    PermissionDenied,
    NotFound,
    Internal,
    Limit,
    Unknown,
    InvalidResponse,
}
pub struct BusyInterval {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}
pub enum CalendarBusy {
    Available(Vec<BusyInterval>),
    Unavailable(Vec<BusyError>),
}
pub struct BusySnapshot {
    pub account: AccountId,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    /// There are no provider event IDs on this path; never fabricate them.
    pub calendars: BTreeMap<String, CalendarBusy>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationPolicy {
    All,
    ExternalOnly,
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

pub struct GoogleCalendarApi<T: HttpTransport> {
    account: AccountId,
    http: Arc<T>,
}
impl<T: HttpTransport> GoogleCalendarApi<T> {
    pub fn new(account: AccountId, http: Arc<T>) -> Self {
        Self { account, http }
    }
    pub fn account(&self) -> &AccountId {
        &self.account
    }

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
        if calendar != "primary" && entry.id != calendar {
            return Err(ApiError::InvalidResponse);
        }
        Ok(entry)
    }

    /// Fixed full/delta scope. No expansion, partial fields, attendee cap, time or search filter.
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
fn validate_etag(value: &str) -> Result<()> {
    if !bounded_text(value, 1024) || value == "*" || value.contains(',') {
        return Err(ApiError::InvalidRequest);
    }
    Ok(())
}
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
fn write_parameters(url: &mut Url, policy: NotificationPolicy) {
    url.query_pairs_mut()
        .append_pair("sendUpdates", policy.parameter())
        .append_pair("conferenceDataVersion", "1")
        .append_pair("supportsAttachments", "true");
}

#[cfg(test)]
mod tests;
