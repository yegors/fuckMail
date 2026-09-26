//! Provider request plans are built here; a single Google adapter submits them.
use crate::{error, Access, Event, EventKey, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// An attendee's answer to an invitation. Serialized in snake_case; [`rsvp`]
/// writes Google's camelCase `responseStatus` values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    Accepted,
    Tentative,
    Declined,
    NeedsAction,
}
/// A conditional PATCH of one event, planned but not yet sent.
#[derive(Debug, Clone, Serialize)]
pub struct GoogleWrite {
    pub target: EventKey,
    /// ETag the event must still have; Google answers 412 otherwise.
    pub if_match: String,
    /// Fields to change. Omitted fields stay as they are; an array replaces the
    /// whole array. `id`, `recurringEventId` and `originalStartTime` are refused.
    pub body: Value,
    /// Google's `sendUpdates`: `all`, `externalOnly` or `none`. Plans built
    /// here use `all`.
    pub send_updates: &'static str,
    /// Always `google_calendar_api`: Google sends any invitation email, never
    /// this client. The adapter refuses any other value.
    pub notification_owner: &'static str,
}
/// An ordinary edit may preserve an existing supported meeting by omission.
/// Unknown solutions cannot safely be round-tripped or modified by this client.
pub fn ordinary_conference_supported(source: &Value) -> bool {
    match source.get("conferenceData") {
        None | Some(Value::Null) => true,
        Some(data) => {
            matches!(
                data["conferenceSolution"]["key"]["type"].as_str(),
                Some("eventHangout" | "eventNamedHangout" | "hangoutsMeet" | "addOn")
            ) && data
                .get("createRequest")
                .is_none_or(|request| request["status"]["statusCode"] == "success")
        }
    }
}
/// Plans a patch of an event's fields, notifying all guests.
///
/// Fails with `permission_denied` unless access resolved for this event is
/// `Writer` or `Owner`, `stale_revision` when the event has no ETag or it
/// differs from `expected_etag`, and `invalid_edit` when `body` is not an
/// object or touches the event's identity.
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
/// Plans a patch that sets the signed-in attendee's `responseStatus`.
///
/// The whole attendee list is sent, since a PATCH replaces arrays; only the
/// entry with `actor_email` and `self: true` changes. Fails with
/// `permission_denied` when access is free/busy or less, or the actor is not
/// that attendee, and `stale_revision` as for [`edit`].
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

/// The two requests of a "this and following" edit.
#[derive(Debug, Clone, Serialize)]
pub struct FollowingEdit {
    /// PATCH that ends the existing series before the edited occurrence.
    pub trim_existing: GoogleWrite,
    /// Body of the new series, inserted as a separate request.
    pub create_following: Value,
    /// Always true: the old series' exceptions must be re-read afterwards.
    pub reconcile_exceptions: bool,
    /// Always false: either request can succeed without the other.
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
