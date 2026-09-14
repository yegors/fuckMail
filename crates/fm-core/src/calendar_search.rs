//! Bounded local search over the same permitted occurrences as Calendar.
use crate::{err, fields, string, Result, Store};
use serde_json::{json, Value};
use std::collections::BTreeSet;

impl Store {
    pub(crate) fn calendar_search(&self, params: &Value) -> Result<Value> {
        fields(params, &["query", "from", "to", "calendars", "limit"])?;
        let query = string(params, "query")?;
        if query.len() > 512 {
            return Err(err(
                "invalid_request",
                "Calendar search query exceeds 512 UTF-8 bytes",
            ));
        }
        let tokens: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
        if tokens.is_empty() || tokens.len() > 32 {
            return Err(err(
                "invalid_request",
                "Calendar search needs 1–32 words in at most 512 UTF-8 bytes",
            ));
        }
        let limit = match params.get("limit") {
            None => 100,
            Some(value) => value
                .as_u64()
                .filter(|v| (1..=100).contains(v))
                .ok_or_else(|| err("invalid_request", "Calendar search limit must be 1–100"))?
                as usize,
        };
        let mut range = json!({"from":params["from"],"to":params["to"]});
        if let Some(calendars) = params.get("calendars") {
            range["calendars"] = calendars.clone();
        }
        let mut result = self.calendar_combined(&range)?;
        let rows = result["events"].as_array().expect("combined Calendar rows");
        let mut identities = BTreeSet::new();
        let mut matches = Vec::new();
        let mut has_more = false;
        for row in rows {
            if row["busy_only"] == true {
                continue;
            }
            let identity = (
                row["account"].as_str(),
                row["calendar_id"].as_str(),
                row["id"].as_str(),
            );
            if identity.0.is_none()
                || identity.1.is_none()
                || identity.2.is_none()
                || !identities.insert(identity)
            {
                continue;
            }
            let mut text = Vec::new();
            for key in [
                "title",
                "location",
                "description",
                "organizer",
                "organizer_name",
            ] {
                if let Some(value) = row[key].as_str() {
                    text.push(value.to_lowercase());
                }
            }
            if let Some(attendees) = row["attendees"].as_array() {
                for attendee in attendees {
                    for key in ["email", "displayName", "name"] {
                        if let Some(value) = attendee[key].as_str() {
                            text.push(value.to_lowercase());
                        }
                    }
                }
            }
            if !tokens
                .iter()
                .all(|token| text.iter().any(|field| field.contains(token)))
            {
                continue;
            }
            if matches.len() == limit {
                has_more = true;
                break;
            }
            matches.push(row.clone());
        }
        result["total"] = matches.len().into();
        result["events"] = matches.into();
        result["query"] = query.into();
        result["limit"] = limit.into();
        result["has_more"] = has_more.into();
        Ok(result)
    }
}

#[cfg(test)]
#[path = "calendar_search_tests.rs"]
mod tests;
