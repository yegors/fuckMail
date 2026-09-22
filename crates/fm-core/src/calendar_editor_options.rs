//! Small authored editor options, shared by GUI and CLI through Calendar writes.
use crate::{err, Result};
use serde_json::{json, Value};

pub(crate) const FIELDS: &[&str] = &[
    "reminder_minutes",
    "availability",
    "conference",
    "convert_time_kind",
];

pub(crate) fn patch(fields: &Value, request_id: &str) -> Result<Value> {
    let mut body = json!({});
    if let Some(minutes) = fields.get("reminder_minutes") {
        if !minutes.is_null() && minutes.as_u64().is_none_or(|v| v > 40320) {
            return Err(err(
                "invalid_request",
                "Choose no reminder or 0–40320 minutes before the event",
            ));
        }
        body["reminders"] = json!({"useDefault":false,"overrides":if minutes.is_null() {
            json!([])
        } else { json!([{"method":"popup","minutes":minutes}]) }});
    }
    if let Some(value) = fields.get("availability") {
        body["transparency"] = match value.as_str() {
            Some("busy") => "opaque",
            Some("free") => "transparent",
            _ => return Err(err("invalid_request", "Availability must be busy or free")),
        }
        .into();
    }
    if let Some(value) = fields.get("conference") {
        body["conferenceData"] = match value.as_str() {
            Some("none") => Value::Null,
            Some("google_meet") => {
                crate::id(request_id)?;
                json!({"createRequest":{"requestId":request_id,"conferenceSolutionKey":{"type":"hangoutsMeet"}}})
            }
            _ => return Err(err("invalid_request", "Choose Google Meet or no meeting")),
        };
        if fields.get("conference_url").is_some() {
            return Err(err("invalid_request", "Choose one conference change"));
        }
    }
    if let Some(value) = fields.get("convert_time_kind") {
        if value != true
            || fields.get("all_day").is_none()
            || fields.get("start").is_none()
            || fields.get("end").is_none()
        {
            return Err(err(
                "invalid_request",
                "Time conversion needs both complete endpoints",
            ));
        }
    }
    Ok(body)
}

/// Only call with a permitted full source; never enrich busy-only rows.
pub(crate) fn project(row: &mut Value, source: &Value) {
    row["availability"] = if source["transparency"] == "transparent" {
        "free"
    } else {
        "busy"
    }
    .into();
    let reminders = &source["reminders"];
    if reminders["useDefault"] == false {
        let overrides = reminders["overrides"].as_array();
        if overrides.is_none_or(Vec::is_empty) {
            row["reminder_minutes"] = Value::Null;
        } else if let Some(items) = overrides.filter(|items| items.len() == 1) {
            let reminder = &items[0];
            if reminder["method"] == "popup"
                && reminder["minutes"].as_u64().is_some_and(|v| v <= 40320)
            {
                row["reminder_minutes"] = reminder["minutes"].clone();
            }
        }
    }
    let conference = &source["conferenceData"];
    row["conference_status"] = conference["createRequest"]["status"]["statusCode"].clone();
    row["has_conference"] = (conference.is_object()
        && (conference["createRequest"].is_object()
            || conference["entryPoints"]
                .as_array()
                .is_some_and(|v| !v.is_empty())))
    .into();
}

/// Normalize a prospective full event after merging an authored PATCH.
pub(crate) fn clear_endpoint_nulls(candidate: &mut Value) {
    for key in ["start", "end"] {
        if let Some(endpoint) = candidate[key].as_object_mut() {
            endpoint.retain(|_, value| !value.is_null());
        }
    }
}
