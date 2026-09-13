//! Provider request plans are built here; a single Google adapter submits them.
use crate::{error, Access, Event, EventKey, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    Accepted,
    Tentative,
    Declined,
    NeedsAction,
}
#[derive(Debug, Clone, Serialize)]
pub struct GoogleWrite {
    pub target: EventKey,
    pub if_match: String,
    pub body: Value,
    pub send_updates: &'static str,
    pub notification_owner: &'static str,
}
pub fn edit(
    event: &Event,
    access: Access,
    expected_etag: &str,
    body: Value,
) -> Result<GoogleWrite> {
    let access = access.for_event(event);
    if !matches!(access, Access::Writer | Access::Owner) {
        return Err(error("permission_denied", "This calendar is not writable"));
    }
    if event.etag.is_empty() || event.etag != expected_etag {
        return Err(error(
            "stale_revision",
            "Refresh the current event before editing",
        ));
    }
    if !body.is_object()
        || body.get("id").is_some()
        || body.get("recurringEventId").is_some()
        || body.get("originalStartTime").is_some()
    {
        return Err(error(
            "invalid_edit",
            "Event identity cannot be changed by an edit",
        ));
    }
    Ok(GoogleWrite {
        target: event.key.clone(),
        if_match: expected_etag.into(),
        body,
        send_updates: "all",
        notification_owner: "google_calendar_api",
    })
}
pub fn rsvp(
    event: &Event,
    access: Access,
    actor_email: &str,
    expected_etag: &str,
    response: ResponseStatus,
) -> Result<GoogleWrite> {
    let access = access.for_event(event);
    if matches!(access, Access::None | Access::FreeBusy) {
        return Err(error(
            "permission_denied",
            "Calendar access no longer permits invitation details",
        ));
    }
    if event.etag.is_empty() || event.etag != expected_etag {
        return Err(error(
            "stale_revision",
            "Refresh the invitation before responding",
        ));
    }
    let mut attendees = event.attendees.clone();
    let actor = attendees
        .iter_mut()
        .find(|person| {
            person["email"].as_str() == Some(actor_email) && person["self"].as_bool() == Some(true)
        })
        .ok_or_else(|| {
            error(
                "permission_denied",
                "Only the signed-in attendee's response can be changed",
            )
        })?;
    actor["responseStatus"] = match response {
        ResponseStatus::Accepted => "accepted",
        ResponseStatus::Tentative => "tentative",
        ResponseStatus::Declined => "declined",
        ResponseStatus::NeedsAction => "needsAction",
    }
    .into();
    Ok(GoogleWrite {
        target: event.key.clone(),
        if_match: expected_etag.into(),
        body: json!({"attendees":attendees}),
        send_updates: "all",
        notification_owner: "google_calendar_api",
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct FollowingEdit {
    pub trim_existing: GoogleWrite,
    pub create_following: Value,
    pub reconcile_exceptions: bool,
    pub atomic: bool,
}
/// Planning boundary for Google's two-request following edit, never fake atomicity.
/// The caller supplies reviewed recurrence bodies; unusual rules aren't rewritten.
pub fn following(
    event: &Event,
    access: Access,
    expected_etag: &str,
    truncated_rule: Vec<String>,
    new_series: Value,
) -> Result<FollowingEdit> {
    if event.recurrence.is_empty() || event.series_id.is_some() {
        return Err(error(
            "invalid_edit",
            "Following edits target a recurring master",
        ));
    }
    if truncated_rule.is_empty()
        || new_series
            .get("recurrence")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
    {
        return Err(error(
            "invalid_edit",
            "Both halves of a following edit need explicit recurrence",
        ));
    }
    let trim_existing = edit(
        event,
        access,
        expected_etag,
        json!({"recurrence":truncated_rule}),
    )?;
    Ok(FollowingEdit {
        trim_existing,
        create_following: new_series,
        reconcile_exceptions: true,
        atomic: false,
    })
}
