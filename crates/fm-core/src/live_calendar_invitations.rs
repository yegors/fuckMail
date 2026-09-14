//! Version-two organizer guest planning. Provider objects are retained exactly;
//! user input changes membership only, never someone else's response metadata.
use crate::{err, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const MAX_GUESTS: usize = 50;

fn email(value: &str) -> Result<String> {
    if value.is_empty() || value.len() > 320 || value.trim() != value {
        return Err(err(
            "invalid_request",
            "Supply a complete guest email address",
        ));
    }
    fm_mail::validate_address(&fm_mail::Address {
        name: None,
        email: value.into(),
    })
    .map_err(|_| err("invalid_request", "Supply a valid guest email address"))?;
    Ok(value.to_lowercase())
}

fn protected(person: &Value) -> bool {
    person["organizer"] == true || person["self"] == true
}

/// Complete bounded roster required only when replacing the attendee array.
fn roster(source: &Value) -> Result<BTreeMap<String, Value>> {
    parse_roster(source, true)
}

fn parse_roster(
    source: &Value,
    include_implicit_organizer: bool,
) -> Result<BTreeMap<String, Value>> {
    if source["attendeesOmitted"] == true {
        return Err(err(
            "incomplete_attendees",
            "Refresh the complete guest list before replacing it",
        ));
    }
    let empty = vec![];
    let items = match source.get("attendees") {
        None => &empty,
        Some(value) => value.as_array().ok_or_else(|| {
            err(
                "unsupported_attendees",
                "The provider guest list is not supported",
            )
        })?,
    };
    if items.len() > MAX_GUESTS {
        return Err(err(
            "too_many_attendees",
            "This editor supports at most 50 participants",
        ));
    }
    let mut output = BTreeMap::new();
    for item in items {
        if item
            .get("asyncOperation")
            .is_some_and(|value| !value.is_null())
        {
            return Err(err(
                "incomplete_attendees",
                "Wait for the provider to finish expanding this guest list",
            ));
        }
        let supported = item.as_object().is_some_and(|fields| {
            fields.iter().all(|(key, value)| match key.as_str() {
                "email" | "id" | "displayName" | "comment" => {
                    value.as_str().is_some_and(|text| text.len() <= 4096)
                }
                "organizer" | "self" | "optional" | "resource" => value.is_boolean(),
                "additionalGuests" => value.as_u64().is_some_and(|count| count <= 50),
                "responseStatus" => matches!(
                    value.as_str(),
                    Some("needsAction" | "accepted" | "declined" | "tentative")
                ),
                "asyncOperation" => value.is_null(),
                _ => false,
            })
        });
        let address = item["email"].as_str().and_then(|v| email(v).ok());
        if !supported || address.is_none() {
            return Err(err(
                "unsupported_attendees",
                "The provider guest metadata cannot be preserved by this editor",
            ));
        }
        if output.insert(address.unwrap(), item.clone()).is_some() {
            return Err(err(
                "unsupported_attendees",
                "The provider guest list contains duplicate identities",
            ));
        }
    }
    if output.len()
        + usize::from(
            include_implicit_organizer && !output.values().any(|p| p["organizer"] == true),
        )
        > MAX_GUESTS
    {
        return Err(err(
            "too_many_attendees",
            "The resulting event exceeds 50 participants including its organizer",
        ));
    }
    Ok(output)
}

pub(crate) fn guest_projection(source: &Value) -> Result<(Vec<Value>, Vec<Value>)> {
    let roster = roster(source)?;
    let mut guests = Vec::new();
    let mut protected_people = Vec::new();
    for person in roster.values() {
        if protected(person) {
            protected_people.push(person.clone());
        } else {
            guests.push(person["email"].clone());
        }
    }
    Ok((guests, protected_people))
}

/// None means the submitted set is unchanged and PATCH must omit attendees.
pub(crate) fn attendee_patch(
    input: &Value,
    source: Option<&Value>,
    actor: &str,
) -> Result<Option<Value>> {
    let values = input.as_array().ok_or_else(|| {
        err(
            "invalid_request",
            "Guests must be an array of email addresses",
        )
    })?;
    if values.len() > MAX_GUESTS {
        return Err(err(
            "too_many_attendees",
            "This editor supports at most 50 participants",
        ));
    }
    let existing = source.map(roster).transpose()?.unwrap_or_default();
    let mut protected_ids: BTreeSet<String> = existing
        .iter()
        .filter(|(_, p)| protected(p))
        .map(|(id, _)| id.clone())
        .collect();
    if let Ok(actor) = email(actor) {
        protected_ids.insert(actor);
    }
    if let Some(address) = source.and_then(|s| s["organizer"]["email"].as_str()) {
        protected_ids.insert(email(address).map_err(|_| {
            err(
                "unsupported_attendees",
                "The organizer identity cannot be preserved",
            )
        })?);
    }
    let mut desired = BTreeSet::new();
    for value in values {
        let address = email(
            value
                .as_str()
                .ok_or_else(|| err("invalid_request", "Guest entries must be email addresses"))?,
        )?;
        if protected_ids.contains(&address) || !desired.insert(address) {
            return Err(err(
                "invalid_request",
                "Guests must be unique and cannot replace the organizer or signed-in participant",
            ));
        }
    }
    let old_guests: BTreeSet<String> = existing
        .iter()
        .filter(|(_, p)| !protected(p))
        .map(|(id, _)| id.clone())
        .collect();
    if desired == old_guests {
        return Ok(None);
    }
    let retained_protected = existing.values().filter(|p| protected(p)).count();
    // Google implicitly adds the organizer on creation even if it is not in
    // attendees. Count that identity when it is absent from the provider array.
    let implicit_organizer = usize::from(!existing.values().any(|p| p["organizer"] == true));
    if desired.len() + retained_protected + implicit_organizer > MAX_GUESTS {
        return Err(err(
            "too_many_attendees",
            "The resulting event exceeds 50 participants",
        ));
    }
    let mut result: Vec<Value> = existing
        .values()
        .filter(|p| protected(p))
        .cloned()
        .collect();
    for address in desired {
        result.push(
            existing
                .get(&address)
                .cloned()
                .unwrap_or_else(|| json!({"email":address,"responseStatus":"needsAction"})),
        );
    }
    for person in &mut result {
        // These fields describe the GET result, not writable guest input. The
        // original roster remains in the frozen baseline and cached source.
        for key in ["id", "self", "organizer", "asyncOperation"] {
            person.as_object_mut().unwrap().remove(key);
        }
    }
    Ok(Some(result.into()))
}

/// RSVP evidence is deliberately distinct from complete roster replacement.
pub(crate) fn matches_attendees(source: &Value, patch: &Value) -> bool {
    if patch["attendeesOmitted"] == true {
        let Some(wanted) = patch["attendees"].as_array().filter(|v| v.len() == 1) else {
            return false;
        };
        let Some(address) = wanted[0]["email"].as_str().and_then(|v| email(v).ok()) else {
            return false;
        };
        let Some(actual) = source["attendees"].as_array() else {
            return false;
        };
        let matching: Vec<_> = actual
            .iter()
            .filter(|p| p["email"].as_str().and_then(|v| email(v).ok()).as_ref() == Some(&address))
            .collect();
        return matching.len() == 1
            && matching[0]["self"] == true
            && matching[0].get("asyncOperation").is_none_or(Value::is_null)
            && matching[0]["responseStatus"] == wanted[0]["responseStatus"];
    }
    matches_roster(source, patch, false)
}

/// A created organizer event may represent its organizer as an extra attendee,
/// or omit that row. Neither representation permits extra guests or self rows.
pub(crate) fn matches_created_attendees(source: &Value, patch: &Value) -> bool {
    matches_roster(source, patch, true)
}

/// The no-op wire omission does not erase the explicitly supplied guest set.
pub(crate) fn unchanged_attendees(baseline: &Value) -> Option<Value> {
    let people = roster(baseline).ok()?;
    let mut people: Vec<_> = people.into_values().collect();
    for person in &mut people {
        for key in ["id", "self", "organizer", "asyncOperation"] {
            person.as_object_mut()?.remove(key);
        }
    }
    Some(people.into())
}

fn matches_roster(source: &Value, patch: &Value, created: bool) -> bool {
    let Ok(mut actual) = roster(source) else {
        return false;
    };
    // This array already contains each protected identity, but deliberately
    // omits read-only role flags. Do not add a second implicit organizer here.
    let Ok(wanted) = parse_roster(&json!({"attendees":patch["attendees"]}), false) else {
        return false;
    };
    if created && actual.len() == wanted.len() + 1 && source["organizer"]["self"] == true {
        if let Some(organizer) = source["organizer"]["email"]
            .as_str()
            .and_then(|v| email(v).ok())
        {
            if !wanted.contains_key(&organizer)
                && actual
                    .get(&organizer)
                    .is_some_and(|p| p["organizer"] == true)
            {
                actual.remove(&organizer);
            }
        }
    }
    if actual.len() != wanted.len() {
        return false;
    }
    wanted.iter().all(|(id, person)| {
        actual.get(id).is_some_and(|actual| {
            // Provider response status may legitimately change after invitations.
            // Membership plus supplied non-response metadata defines the intent.
            person
                .as_object()
                .unwrap()
                .iter()
                .all(|(key, value)| match key.as_str() {
                    "email" | "responseStatus" => true,
                    _ => actual.get(key) == Some(value),
                })
        })
    })
}
