//! Bounded identity discovery is separate from complete event acquisition.
use super::*;
use crate::EventTime;

/// A discovery result is only an identity hint, never a writable event source.
#[derive(Debug, Clone)]
pub struct InstanceIdentity {
    pub key: EventKey,
    pub series_id: String,
    pub original_start: EventTime,
    pub etag: Option<String>,
    pub cancelled: bool,
}

pub struct InstancesPage {
    pub items: Vec<InstanceIdentity>,
    pub next_page_token: Option<String>,
}

/// Cancelled exceptions may have only their immutable identity. An absent ETag
/// is permitted only for read evidence and cannot become an EventResource.
pub struct OccurrenceResource {
    pub identity: InstanceIdentity,
    pub source: Value,
}

impl<T: HttpTransport> GoogleCalendarApi<T> {
    /// No actual-time window: a moved exception must remain discoverable.
    /// All-day originalStart query serialization is intentionally not assumed.
    pub async fn instances(
        &self,
        access_token: &str,
        calendar: &str,
        series: &str,
        original_start: Option<&EventTime>,
        page_token: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<InstancesPage> {
        let mut url = endpoint(&["calendars", calendar, "events", series, "instances"])?;
        url.query_pairs_mut()
            .append_pair("maxResults", "250")
            .append_pair("showDeleted", "true");
        if let Some(original) = original_start {
            let EventTime::Zoned { instant, time_zone } = original else {
                return Err(ApiError::InvalidRequest);
            };
            time_zone
                .parse::<chrono_tz::Tz>()
                .map_err(|_| ApiError::InvalidRequest)?;
            url.query_pairs_mut()
                .append_pair("originalStart", &instant.to_rfc3339());
        }
        if let Some(token) = page_token {
            if !bounded_text(token, 8192) {
                return Err(ApiError::InvalidRequest);
            }
            url.query_pairs_mut().append_pair("pageToken", token);
        }
        let response = self
            .execute(
                request(HttpMethod::Get, &url, access_token, PAGE_BYTES)?,
                cancel,
                false,
            )
            .await?;
        if response.status != 200 {
            return Err(ApiError::InvalidResponse);
        }
        let mut value = parse_json(&response.body, PAGE_BYTES)?;
        let next_page_token = optional_text(&value, "nextPageToken", 8192)?;
        // Instances has no sync-token request contract. Validate any supplied
        // terminal metadata without importing it into the acquisition cursor.
        let sync = optional_text(&value, "nextSyncToken", 8192)?;
        if next_page_token
            .as_deref()
            .is_some_and(|next| Some(next) == page_token)
            || (next_page_token.is_some() && sync.is_some())
        {
            return Err(ApiError::InvalidResponse);
        }
        let sources = match value.get_mut("items").map(Value::take) {
            None => Vec::new(),
            Some(Value::Array(items)) if items.len() <= 250 => items,
            _ => return Err(ApiError::InvalidResponse),
        };
        let items = sources
            .into_iter()
            .map(|source| instance_identity(&self.account, calendar, series, &source, None))
            .collect::<Result<_>>()?;
        Ok(InstancesPage {
            items,
            next_page_token,
        })
    }

    /// Always fetch a complete current instance. No conditional 304, attendee
    /// truncation, partial fields or inferred IDs are accepted by this path.
    pub async fn get_occurrence(
        &self,
        access_token: &str,
        calendar: &str,
        series: &str,
        event: &str,
        cancel: &CancellationToken,
    ) -> Result<OccurrenceResource> {
        let url = endpoint(&["calendars", calendar, "events", event])?;
        let response = self
            .execute(
                request(HttpMethod::Get, &url, access_token, EVENT_BYTES)?,
                cancel,
                false,
            )
            .await?;
        if response.status != 200 {
            return Err(ApiError::InvalidResponse);
        }
        let mut source = parse_json(&response.body, EVENT_BYTES)?;
        let identity = instance_identity(&self.account, calendar, series, &source, response.etag)?;
        if identity.key.event != event {
            return Err(ApiError::InvalidResponse);
        }
        // Match the ordinary fresh-event boundary for header-only ETags.
        if source.get("etag").is_none() {
            if let Some(etag) = &identity.etag {
                source["etag"] = etag.clone().into();
            }
        }
        Ok(OccurrenceResource { identity, source })
    }
}

fn instance_identity(
    account: &AccountId,
    calendar: &str,
    series: &str,
    source: &Value,
    header_etag: Option<String>,
) -> Result<InstanceIdentity> {
    let event = required_text(source, "id", 1024)?;
    if required_text(source, "recurringEventId", 1024)? != series || event == series {
        return Err(ApiError::InvalidResponse);
    }
    let cancelled = match source.get("status") {
        Some(Value::String(status)) if status == "cancelled" => true,
        Some(Value::String(status)) if matches!(status.as_str(), "confirmed" | "tentative") => {
            false
        }
        None => false,
        _ => return Err(ApiError::InvalidResponse),
    };
    let etag = response_etag(source, header_etag)?;
    if etag.is_none() && !cancelled {
        return Err(ApiError::InvalidResponse);
    }
    if let Some(zone) = source["originalStartTime"].get("timeZone") {
        zone.as_str()
            .and_then(|zone| zone.parse::<chrono_tz::Tz>().ok())
            .ok_or(ApiError::InvalidResponse)?;
    }
    let original_start = crate::parse_time(&source["originalStartTime"], "Etc/UTC")
        .map_err(|_| ApiError::InvalidResponse)?;
    Ok(InstanceIdentity {
        key: EventKey {
            account: account.as_str().into(),
            calendar: calendar.into(),
            event: event.into(),
        },
        series_id: series.into(),
        original_start,
        etag,
        cancelled,
    })
}
