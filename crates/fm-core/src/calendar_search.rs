//! Bounded local search over the same permitted occurrences as Calendar.
use crate::search_query::{contains_phrase, fold, looks_like_email, segments_with, unquote};
use crate::{err, fields, string, Result, Store};
use chrono::{Duration, NaiveDate};
use serde_json::{json, Value};
use std::collections::BTreeSet;

const OPERATORS: [&str; 3] = ["title", "with", "date"];
/// The `around` search looks this far ahead, then this far back.
const UPCOMING_DAYS: i64 = 366;
const PAST_DAYS: i64 = 60;
/// `calendar_combined`'s widest range.
const WINDOW_DAYS: i64 = 42;

/// How Calendar search read its text. Every part must match.
#[derive(Default)]
struct CalendarQuery {
    /// Words typed together, matched as one phrase in the title or notes.
    words: Vec<String>,
    /// `title:` phrases, matched in the title only.
    title: Vec<String>,
    /// `with:` names or addresses, and bare addresses: guests or organizer.
    with: Vec<String>,
    /// `date:`'s value. It moves the Calendar and never filters.
    date: Option<String>,
}

impl CalendarQuery {
    fn parse(text: &str) -> Self {
        let mut query = Self::default();
        let bare = |query: &mut Self, text: &str| {
            let mut phrase: Vec<&str> = Vec::new();
            let close = |query: &mut Self, phrase: &mut Vec<&str>| {
                let text = phrase.join(" ");
                let text = text.trim_matches('"').trim();
                if !text.is_empty() {
                    query.words.push(text.to_owned());
                }
                phrase.clear();
            };
            for word in text.split_whitespace() {
                if looks_like_email(word) {
                    close(query, &mut phrase);
                    query.with.push(word.to_owned());
                } else {
                    phrase.push(word);
                }
            }
            close(query, &mut phrase);
        };
        for (key, value) in segments_with(text, &OPERATORS) {
            let value = value.trim();
            match key {
                None => bare(&mut query, value),
                Some(_) if value.is_empty() => {}
                Some("title") => query.title.push(unquote(value).trim().to_owned()),
                Some(key) => {
                    // with: and date: take one word, or a quoted name; the rest are bare words.
                    let (word, rest) = match value.strip_prefix('"').and_then(|v| v.split_once('"'))
                    {
                        Some((quoted, rest)) if key == "with" => (quoted.trim(), rest),
                        _ => value.split_once(char::is_whitespace).unwrap_or((value, "")),
                    };
                    bare(&mut query, rest);
                    if word.is_empty() {
                        continue;
                    }
                    if key == "with" {
                        query.with.push(word.to_owned());
                    } else {
                        query.date = Some(word.to_owned());
                    }
                }
            }
        }
        query.title.retain(|v| !v.is_empty());
        query
    }

    /// Whether anything filters; `date:` alone does not.
    fn filters(&self) -> bool {
        !self.words.is_empty() || !self.title.is_empty() || !self.with.is_empty()
    }

    fn terms(&self, text: &str) -> Value {
        json!({"text":text,"words":self.words,"title":self.title,"with":self.with,"date":self.date})
    }

    fn matches(&self, row: &Value) -> bool {
        let folded = |key: &str| fold(row[key].as_str().unwrap_or(""));
        let title = folded("title");
        let notes = folded("description");
        let found = |text: &str, phrase: &str| contains_phrase(text, &fold(phrase));
        if !self.title.iter().all(|phrase| found(&title, phrase))
            || !self
                .words
                .iter()
                .all(|phrase| found(&title, phrase) || found(&notes, phrase))
        {
            return false;
        }
        if self.with.is_empty() {
            return true;
        }
        let mut people = vec![folded("organizer"), folded("organizer_name")];
        for attendee in row["attendees"].as_array().into_iter().flatten() {
            for key in ["email", "displayName", "name"] {
                if let Some(value) = attendee[key].as_str() {
                    people.push(fold(value));
                }
            }
        }
        self.with
            .iter()
            .all(|person| people.iter().any(|field| found(field, person)))
    }
}

fn day(value: &Value, name: &str) -> Result<NaiveDate> {
    value
        .as_str()
        .filter(|v| v.len() == 10)
        .and_then(|v| NaiveDate::parse_from_str(v, "%Y-%m-%d").ok())
        .ok_or_else(|| err("invalid_request", &format!("{name} must be YYYY-MM-DD")))
}

impl Store {
    /// Finds occurrences matching Calendar's search text.
    ///
    /// Words typed together match as one phrase in the title or notes;
    /// `title:` matches titles only; `with:` and a bare address match guests
    /// and the organizer by name or address; `date:` is returned in `terms`
    /// and never filters. Matching folds case, Latin accents and spaces and
    /// starts at a word, as mail search does.
    ///
    /// Either `from`/`to` (1–42 days, each occurrence once) or `around`: the
    /// next occurrence of each matching event from `around` up to 366 days
    /// ahead, then the latest from the 60 days before (marked `past`).
    /// Busy-only rows are never searched, so free/busy calendars cannot leak
    /// details. Returns up to `limit` rows (default and maximum 100), with
    /// `has_more` when more matched.
    pub(crate) fn calendar_search(&self, params: &Value) -> Result<Value> {
        fields(
            params,
            &["query", "from", "to", "around", "calendars", "limit"],
        )?;
        let text = string(params, "query")?;
        let words = text.split_whitespace().count();
        if text.len() > 512 || words == 0 || words > 32 {
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
        let query = CalendarQuery::parse(text);
        let range = |from: NaiveDate, to: NaiveDate| {
            let mut range = json!({"from":from.to_string(),"to":to.to_string()});
            if let Some(calendars) = params.get("calendars") {
                range["calendars"] = calendars.clone();
            }
            range
        };
        let mut matches = Vec::new();
        let mut has_more = false;
        let mut result = if let Some(around) = params.get("around") {
            if params.get("from").is_some() || params.get("to").is_some() {
                return Err(err(
                    "invalid_request",
                    "Calendar search takes either around or from and to",
                ));
            }
            let around = day(around, "around")?;
            let mut windows = Vec::new();
            let (end, mut start) = (around + Duration::days(UPCOMING_DAYS), around);
            while start < end {
                let next = (start + Duration::days(WINDOW_DAYS)).min(end);
                windows.push((start, next, false));
                start = next;
            }
            let (first, mut end) = (around - Duration::days(PAST_DAYS), around);
            while end > first {
                let next = (end - Duration::days(WINDOW_DAYS)).max(first);
                windows.push((next, end, true));
                end = next;
            }
            // With nothing to match, one read still reports zone and coverage.
            if !query.filters() {
                windows.truncate(1);
            }
            let mut series = BTreeSet::new();
            let mut merged: Option<Value> = None;
            let (mut complete, mut restricted) = (true, false);
            for (from, to, past) in windows {
                let window = self.calendar_combined(&range(from, to))?;
                let coverage = &window["coverage"];
                complete &= coverage["complete"] == true;
                restricted |= coverage["state"] == "restricted";
                let rows = window["events"].as_array().expect("combined Calendar rows");
                // The past runs backwards, so each event shows its latest time.
                let ordered: Box<dyn Iterator<Item = &Value>> = if past {
                    Box::new(rows.iter().rev())
                } else {
                    Box::new(rows.iter())
                };
                for row in ordered {
                    if !query.filters() {
                        break;
                    }
                    if row["busy_only"] == true {
                        continue;
                    }
                    let event = row["event_id"].as_str().or(row["id"].as_str());
                    let (Some(account), Some(calendar), Some(event)) =
                        (row["account"].as_str(), row["calendar_id"].as_str(), event)
                    else {
                        continue;
                    };
                    let key = (account.to_owned(), calendar.to_owned(), event.to_owned());
                    if series.contains(&key) || !query.matches(row) {
                        continue;
                    }
                    if matches.len() == limit {
                        has_more = true;
                        break;
                    }
                    series.insert(key);
                    let mut row = row.clone();
                    row["past"] = past.into();
                    matches.push(row);
                }
                merged.get_or_insert(window);
                if has_more {
                    break;
                }
            }
            let mut result = merged.expect("at least one window");
            result["coverage"]["complete"] = complete.into();
            if result["coverage"]["state"] != "display_zone_required" {
                result["coverage"]["state"] = if restricted {
                    "restricted"
                } else if complete {
                    "cached"
                } else {
                    "incomplete"
                }
                .into();
            }
            result["around"] = around.to_string().into();
            result["from"] = first.to_string().into();
            result["to"] = (around + Duration::days(UPCOMING_DAYS)).to_string().into();
            result
        } else {
            let from = day(&params["from"], "from")?;
            let to = day(&params["to"], "to")?;
            let result = self.calendar_combined(&range(from, to))?;
            let rows = result["events"].as_array().expect("combined Calendar rows");
            let mut identities = BTreeSet::new();
            for row in rows.iter().filter(|_| query.filters()) {
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
                    || identities.contains(&identity)
                    || !query.matches(row)
                {
                    continue;
                }
                if matches.len() == limit {
                    has_more = true;
                    break;
                }
                identities.insert(identity);
                matches.push(row.clone());
            }
            result
        };
        result["total"] = matches.len().into();
        result["events"] = matches.into();
        result["query"] = text.into();
        result["terms"] = query.terms(text);
        result["limit"] = limit.into();
        result["has_more"] = has_more.into();
        Ok(result)
    }
}

#[cfg(test)]
#[path = "calendar_search_tests.rs"]
mod tests;
