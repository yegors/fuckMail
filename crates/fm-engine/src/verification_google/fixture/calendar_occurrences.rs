//! Fixed recurrence corpus; normal invitation and account fixtures stay unchanged.
use super::calendar_invitations;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(super) const MASTERS: usize = 4;
pub(super) const INSTANCES: usize = 13;
pub(super) const INITIAL_SOURCES: usize = 6;
pub(super) const PAGE_SIZE: usize = 2;

pub(super) fn master(key: &str, index: usize, base: u64) -> Value {
    let mut source = calendar_invitations::source(key, 0, base);
    let id = format!("co{index:04}");
    source["id"] = id.clone().into();
    source["etag"] = format!("\"{id}-v1\"").into();
    source["summary"] = format!("Occurrence {key} {index:04}").into();
    source["description"] = "Preserved generated occurrence details".into();
    source["recurrence"] = json!([format!(
        "RRULE:FREQ=DAILY;COUNT={}",
        match index {
            0 => 6,
            1 => 3,
            _ => 2,
        }
    )]);
    if index == 1 {
        let date = chrono::DateTime::from_timestamp(base as i64, 0)
            .unwrap()
            .date_naive()
            + chrono::Duration::days(1);
        source["start"] = json!({"date":date.to_string()});
        source["end"] = json!({"date":(date+chrono::Duration::days(1)).to_string()});
    } else {
        let start =
            chrono::DateTime::from_timestamp((base + (index as u64 + 1) * 3600) as i64, 0).unwrap();
        source["start"] =
            json!({"dateTime":start.format("%Y-%m-%dT%H:%M:%SZ").to_string(),"timeZone":"UTC"});
        source["end"] = json!({"dateTime":(start+chrono::Duration::minutes(30)).format("%Y-%m-%dT%H:%M:%SZ").to_string(),"timeZone":"UTC"});
    }
    if index == 2 {
        source["status"] = "cancelled".into();
    }
    if index == 3 {
        source["locked"] = true.into();
    }
    source
}

pub(super) fn instance(key: &str, master_index: usize, index: usize, base: u64) -> Value {
    let mut source = master(key, master_index, base);
    let prefix = match master_index {
        0 => "oi",
        1 => "od",
        2 => "oc",
        _ => "ol",
    };
    let id = format!("{prefix}{index:04}");
    source["id"] = id.clone().into();
    source["etag"] = format!("\"{id}-v1\"").into();
    source["status"] = "confirmed".into();
    source["recurringEventId"] = format!("co{master_index:04}").into();
    source.as_object_mut().unwrap().remove("recurrence");
    let days = chrono::Duration::days(index as i64);
    for field in ["start", "end"] {
        if master_index == 1 {
            let date = chrono::NaiveDate::parse_from_str(
                source[field]["date"].as_str().unwrap(),
                "%Y-%m-%d",
            )
            .unwrap()
                + days;
            source[field] = json!({"date":date.to_string()});
        } else {
            let time =
                chrono::DateTime::parse_from_rfc3339(source[field]["dateTime"].as_str().unwrap())
                    .unwrap()
                    + days;
            source[field] =
                json!({"dateTime":time.format("%Y-%m-%dT%H:%M:%SZ").to_string(),"timeZone":"UTC"});
        }
    }
    source["originalStartTime"] = source["start"].clone();
    if master_index == 0 && index == 1 {
        for field in ["start", "end"] {
            let time =
                chrono::DateTime::parse_from_rfc3339(source[field]["dateTime"].as_str().unwrap())
                    .unwrap()
                    + chrono::Duration::days(60);
            source[field] =
                json!({"dateTime":time.format("%Y-%m-%dT%H:%M:%SZ").to_string(),"timeZone":"UTC"});
        }
    }
    if master_index == 1 && index == 1 {
        return cancelled(&source);
    }
    source
}

pub(super) fn cancelled(source: &Value) -> Value {
    json!({"id":source["id"],"status":"cancelled","recurringEventId":source["recurringEventId"],"originalStartTime":source["originalStartTime"]})
}

pub(super) fn sources(key: &str, base: u64) -> BTreeMap<String, Value> {
    let mut result = BTreeMap::new();
    for m in 0..MASTERS {
        let source = master(key, m, base);
        result.insert(source["id"].as_str().unwrap().into(), source);
        for i in 0..match m {
            0 => 6,
            1 => 3,
            _ => 2,
        } {
            let source = instance(key, m, i, base);
            result.insert(source["id"].as_str().unwrap().into(), source);
        }
    }
    result
}

pub(super) fn initially_visible(id: &str) -> bool {
    id.starts_with("co") || matches!(id, "oi0001" | "od0001")
}

/// Extra sources are created only after an explicit POST in the isolated mode.
/// This fixture subset intentionally stays DAILY/WEEKLY COUNT<=32 in UTC.
pub(super) fn materialize(master: &Value) -> Result<Vec<Value>, ()> {
    use sha2::{Digest, Sha256};
    let rule = master["recurrence"]
        .as_array()
        .filter(|v| v.len() == 1)
        .and_then(|v| v[0].as_str())
        .and_then(|s| s.strip_prefix("RRULE:"))
        .ok_or(())?;
    let values = rule
        .split(';')
        .map(|p| p.split_once('=').ok_or(()))
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    if values
        .keys()
        .any(|k| !matches!(*k, "FREQ" | "COUNT" | "INTERVAL"))
    {
        return Err(());
    }
    let unit = match values.get("FREQ") {
        Some(&"DAILY") => 1,
        Some(&"WEEKLY") => 7,
        _ => return Err(()),
    };
    let count = values
        .get("COUNT")
        .ok_or(())?
        .parse::<usize>()
        .ok()
        .filter(|n| (1..=32).contains(n))
        .ok_or(())?;
    let interval = values
        .get("INTERVAL")
        .unwrap_or(&"1")
        .parse::<i64>()
        .ok()
        .filter(|n| (1..=99).contains(n))
        .ok_or(())?;
    let mut output = Vec::with_capacity(count);
    for index in 0..count {
        let mut source = master.clone();
        source.as_object_mut().ok_or(())?.remove("recurrence");
        let days = chrono::Duration::days(index as i64 * unit * interval);
        for field in ["start", "end"] {
            if let Some(date) = master[field]["date"].as_str() {
                let date =
                    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| ())? + days;
                source[field] = json!({"date":date.to_string()});
            } else {
                let time = chrono::DateTime::parse_from_rfc3339(
                    master[field]["dateTime"].as_str().ok_or(())?,
                )
                .map_err(|_| ())?
                    + days;
                source[field] = json!({"dateTime":time.format("%Y-%m-%dT%H:%M:%SZ").to_string(),"timeZone":"UTC"});
            }
        }
        source["originalStartTime"] = source["start"].clone();
        let original = source["originalStartTime"]["date"]
            .as_str()
            .or_else(|| source["originalStartTime"]["dateTime"].as_str())
            .ok_or(())?;
        let hash = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&json!([master["id"], original])).map_err(|_| ())?)
        );
        let id = format!("fi{}", &hash[..40]);
        source["id"] = id.clone().into();
        source["etag"] = format!(
            "\"{id}-{}\"",
            master["etag"].as_str().ok_or(())?.trim_matches('"')
        )
        .into();
        source["recurringEventId"] = master["id"].clone();
        output.push(source);
    }
    Ok(output)
}
