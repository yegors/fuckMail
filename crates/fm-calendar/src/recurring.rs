//! The deliberately small authoring subset. Provider sources remain untouched;
//! unsupported rules are never approximated before a following-series trim.
use crate::{error, Event, EventTime, Result};
use chrono::{Duration, NaiveDate, Utc};
use rrule::RRuleSet;
use std::collections::BTreeMap;

pub struct RuleSplit {
    pub before: Vec<String>,
    pub after: Vec<String>,
    pub preceding_count: usize,
}

fn parts(rules: &[String], allow_empty: bool) -> Result<BTreeMap<String, String>> {
    if allow_empty && rules.is_empty() {
        return Ok(BTreeMap::new());
    }
    if rules.len() != 1 || rules[0].len() > 1024 {
        return Err(error(
            "unsupported_rule",
            "Choose one bounded common recurrence rule",
        ));
    }
    let rule = rules[0]
        .strip_prefix("RRULE:")
        .ok_or_else(|| error("unsupported_rule", "Only one RRULE can be authored"))?;
    let mut values = BTreeMap::new();
    for component in rule.split(';') {
        let (key, value) = component
            .split_once('=')
            .ok_or_else(|| error("unsupported_rule", "Invalid recurrence component"))?;
        if value.is_empty() || values.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(error(
                "unsupported_rule",
                "Duplicate or empty recurrence component",
            ));
        }
    }
    let frequency = values.get("FREQ").map(String::as_str);
    if !matches!(frequency, Some("DAILY" | "WEEKLY" | "MONTHLY" | "YEARLY")) {
        return Err(error(
            "unsupported_rule",
            "Choose daily, weekly, monthly or yearly recurrence",
        ));
    }
    for (key, value) in &values {
        let integer = |min, max| {
            value
                .parse::<u32>()
                .is_ok_and(|v| v >= min && v <= max && v.to_string() == *value)
        };
        let valid = match key.as_str() {
            "FREQ" => true,
            "INTERVAL" => integer(1, 99),
            "COUNT" => integer(1, 1000),
            "UNTIL" => {
                value.len() == 8 && NaiveDate::parse_from_str(value, "%Y%m%d").is_ok()
                    || value.len() == 16
                        && chrono::NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%SZ").is_ok()
            }
            "BYDAY" if frequency == Some("WEEKLY") => {
                let days: Vec<_> = value.split(',').collect();
                days.len() <= 7
                    && days
                        .iter()
                        .all(|d| matches!(*d, "MO" | "TU" | "WE" | "TH" | "FR" | "SA" | "SU"))
                    && days.iter().collect::<std::collections::BTreeSet<_>>().len() == days.len()
            }
            "BYMONTHDAY" if frequency == Some("MONTHLY") => integer(1, 31),
            "BYMONTH" if frequency == Some("YEARLY") => integer(1, 12),
            _ => false,
        };
        if !valid {
            return Err(error(
                "unsupported_rule",
                "This recurrence component is outside the common-rule editor",
            ));
        }
    }
    if values.contains_key("COUNT") && values.contains_key("UNTIL") {
        return Err(error(
            "unsupported_rule",
            "Choose a count or an end date, not both",
        ));
    }
    Ok(values)
}

pub fn validate_rule_shape(rules: &[String], allow_empty: bool) -> Result<()> {
    parts(rules, allow_empty).map(|_| ())
}

fn rule_set(rules: &[String], start: &EventTime) -> Result<RRuleSet> {
    let values = parts(rules, false)?;
    let dtstart = match start {
        EventTime::Date { date } => {
            if values.get("UNTIL").is_some_and(|v| v.len() != 8) {
                return Err(error(
                    "unsupported_rule",
                    "All-day recurrence requires a DATE end",
                ));
            }
            format!("DTSTART:{}T000000Z", date.format("%Y%m%d"))
        }
        EventTime::Zoned { instant, time_zone } => {
            if values.get("UNTIL").is_some_and(|v| v.len() != 16) {
                return Err(error(
                    "unsupported_rule",
                    "Timed recurrence requires a UTC end instant",
                ));
            }
            let zone: chrono_tz::Tz = time_zone
                .parse()
                .map_err(|_| error("unsupported_zone", "A recurrence needs an IANA zone"))?;
            format!(
                "DTSTART;TZID={}:{}",
                time_zone,
                instant.with_timezone(&zone).format("%Y%m%dT%H%M%S")
            )
        }
    };
    let mut carrier = rules[0].clone();
    if matches!(start, EventTime::Date { .. }) {
        if let Some(until) = values.get("UNTIL") {
            carrier = carrier.replace(&format!("UNTIL={until}"), &format!("UNTIL={until}T000000Z"));
        }
    }
    format!("{dtstart}\n{carrier}").parse().map_err(|e| {
        error(
            "unsupported_rule",
            format!("Recurrence cannot be represented: {e}"),
        )
    })
}

pub fn validate_rule(rules: &[String], start: &EventTime) -> Result<()> {
    if rules.is_empty() {
        return Ok(());
    }
    let result = rule_set(rules, start)?.all(1);
    if result.dates.is_empty() {
        return Err(error(
            "unsupported_rule",
            "Recurrence has no first occurrence",
        ));
    }
    Ok(())
}

fn encode(values: &BTreeMap<String, String>) -> Vec<String> {
    vec![format!(
        "RRULE:{}",
        values
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(";")
    )]
}

pub fn split_rule(master: &Event, original: &EventTime) -> Result<RuleSplit> {
    let start = master
        .start
        .as_ref()
        .ok_or_else(|| error("invalid_time", "Recurring master has no start"))?;
    if master.series_id.is_some()
        || std::mem::discriminant(start) != std::mem::discriminant(original)
    {
        return Err(error(
            "scope_mismatch",
            "Split must identify a date or timed occurrence of this master",
        ));
    }
    let values = parts(&master.recurrence, false)?;
    let zone: chrono_tz::Tz = match start {
        EventTime::Date { .. } => chrono_tz::UTC,
        EventTime::Zoned { time_zone, .. } => time_zone
            .parse()
            .map_err(|_| error("unsupported_zone", "Invalid series zone"))?,
    };
    let selected = original.utc(zone)?;
    // Bound both work and ordinal. Never scan an unbounded series to find a split.
    let result = rule_set(&master.recurrence, start)?
        .before((selected + Duration::seconds(1)).with_timezone(&rrule::Tz::UTC))
        .all(1001);
    if result.limited || result.dates.len() > 1000 {
        return Err(error(
            "limit_exceeded",
            "Following split exceeds 1000 preceding occurrences",
        ));
    }
    let ordinal = result
        .dates
        .iter()
        .position(|date| date.with_timezone(&Utc) == selected)
        .ok_or_else(|| {
            error(
                "scope_mismatch",
                "Selected original start is not a member of the current rule",
            )
        })?;
    if ordinal == 0 {
        return Err(error(
            "unsupported_operation",
            "Use the entire-series action for its first occurrence",
        ));
    }
    let mut before = values.clone();
    let mut after = values;
    if let Some(count) = before.remove("COUNT") {
        let count = count
            .parse::<usize>()
            .map_err(|_| error("unsupported_rule", "Invalid recurrence count"))?;
        before.insert("COUNT".into(), ordinal.to_string());
        after.insert(
            "COUNT".into(),
            count
                .checked_sub(ordinal)
                .filter(|v| *v > 0)
                .ok_or_else(|| error("scope_mismatch", "Split is outside the recurrence count"))?
                .to_string(),
        );
    } else {
        let cutoff = match original {
            EventTime::Date { date } => date
                .pred_opt()
                .ok_or_else(|| error("invalid_time", "Split date underflows"))?
                .format("%Y%m%d")
                .to_string(),
            EventTime::Zoned { instant, .. } => (instant.with_timezone(&Utc)
                - Duration::seconds(1))
            .format("%Y%m%dT%H%M%SZ")
            .to_string(),
        };
        before.insert("UNTIL".into(), cutoff);
    }
    Ok(RuleSplit {
        before: encode(&before),
        after: encode(&after),
        preceding_count: ordinal,
    })
}

/// Current common-rule bounds suppress retained exceptions beyond a trimmed
/// series. Unknown provider rules retain the preexisting projection semantics.
pub(crate) fn within_series_end(master: &Event, original: &EventTime) -> Result<bool> {
    if master.recurrence.is_empty() {
        return Ok(false);
    }
    let Ok(values) = parts(&master.recurrence, false) else {
        return Ok(true);
    };
    let Some(start) = master.start.as_ref() else {
        return Ok(false);
    };
    let zone = match start {
        EventTime::Date { .. } => chrono_tz::UTC,
        EventTime::Zoned { time_zone, .. } => time_zone
            .parse()
            .map_err(|_| error("unsupported_zone", "Invalid series zone"))?,
    };
    let selected = original.utc(zone)?;
    if let Some(until) = values.get("UNTIL") {
        let cutoff = if until.len() == 8 {
            NaiveDate::parse_from_str(until, "%Y%m%d")
                .unwrap()
                .and_hms_opt(23, 59, 59)
                .unwrap()
                .and_utc()
        } else {
            chrono::NaiveDateTime::parse_from_str(until, "%Y%m%dT%H%M%SZ")
                .unwrap()
                .and_utc()
        };
        return Ok(selected <= cutoff);
    }
    if values.contains_key("COUNT") {
        let result = rule_set(&master.recurrence, start)?.all(1001);
        return Ok(result
            .dates
            .last()
            .is_some_and(|last| selected <= last.with_timezone(&Utc)));
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn master(rule: &str, date: bool) -> Event {
        let source = if date {
            json!({"id":"master","etag":"v1","start":{"date":"2026-09-12"},"end":{"date":"2026-09-13"},"recurrence":[rule]})
        } else {
            json!({"id":"master","etag":"v1","start":{"dateTime":"2026-09-12T09:00:00Z","timeZone":"UTC"},"end":{"dateTime":"2026-09-12T10:00:00Z","timeZone":"UTC"},"recurrence":[rule]})
        };
        crate::normalize("a", "c", "UTC", &source).unwrap()
    }
    #[test]
    fn count_and_until_splits_preserve_types_and_remaining_count() {
        let event = master("RRULE:FREQ=DAILY;COUNT=5", false);
        let original = EventTime::Zoned {
            instant: "2026-09-14T09:00:00Z".parse().unwrap(),
            time_zone: "UTC".into(),
        };
        let split = split_rule(&event, &original).unwrap();
        assert_eq!(split.preceding_count, 2);
        assert_eq!(split.before, vec!["RRULE:COUNT=2;FREQ=DAILY"]);
        assert_eq!(split.after, vec!["RRULE:COUNT=3;FREQ=DAILY"]);
        let event = master("RRULE:FREQ=DAILY;UNTIL=20260920", true);
        let split = split_rule(
            &event,
            &EventTime::Date {
                date: "2026-09-14".parse().unwrap(),
            },
        )
        .unwrap();
        assert_eq!(split.before, vec!["RRULE:FREQ=DAILY;UNTIL=20260913"]);
        assert_eq!(split.after, event.recurrence);
    }
    #[test]
    fn unsupported_rules_nonmembers_and_first_split_fail_without_a_plan() {
        for rule in [
            "RRULE:FREQ=HOURLY",
            "RRULE:FREQ=DAILY;COUNT=2;UNTIL=20260920",
            "RRULE:FREQ=WEEKLY;BYDAY=MO,MO",
            "RRULE:FREQ=MONTHLY;BYDAY=-1MO",
            "RRULE:FREQ=DAILY;INTERVAL=0",
        ] {
            assert!(
                validate_rule_shape(&[rule.into()], false).is_err(),
                "{rule}"
            );
        }
        let event = master("RRULE:FREQ=DAILY;COUNT=5", true);
        assert!(split_rule(&event, event.start.as_ref().unwrap()).is_err());
        assert!(split_rule(
            &event,
            &EventTime::Date {
                date: "2027-01-01".parse().unwrap()
            }
        )
        .is_err());
    }
    #[test]
    fn trimmed_master_suppresses_retained_later_moved_exception() {
        let event = master("RRULE:FREQ=DAILY;COUNT=1", false);
        let mut source = event.source.clone();
        source.as_object_mut().unwrap().remove("recurrence");
        source["id"] = "moved".into();
        source["recurringEventId"] = "master".into();
        source["originalStartTime"] = json!({"dateTime":"2026-09-14T09:00:00Z"});
        source["start"] = json!({"dateTime":"2026-09-15T09:00:00Z"});
        source["end"] = json!({"dateTime":"2026-09-15T10:00:00Z"});
        let moved = crate::normalize("a", "c", "UTC", &source).unwrap();
        let window = crate::Window::new(
            "2026-09-12T00:00:00Z".parse().unwrap(),
            "2026-09-17T00:00:00Z".parse().unwrap(),
            chrono_tz::UTC,
        )
        .unwrap();
        let rows =
            crate::project_event_in_window(&event, &[moved], "UTC", window, crate::Access::Owner)
                .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
