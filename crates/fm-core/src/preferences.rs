//! Typed local presentation preferences and account domain metadata.
use crate::*;
use rusqlite::params;
use sha2::{Digest, Sha256};

fn defaults() -> Value {
    json!({"density":"comfortable","week_start":"monday","calendar_start_hour":7,"calendar_end_hour":22,"timeline_start_hour":7,"timeline_end_hour":22,"selected_calendars":null,"calendar_view":"week","calendar_date":null,"timeline_date":null})
}
impl Store {
    pub(crate) fn preferences(&self) -> Result<Value> {
        let found: Option<(i64, String)> = self
            .db
            .query_row(
                "SELECT revision,json FROM preferences WHERE singleton=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (revision, preferences) = match found {
            Some((r, j)) => {
                let mut merged = defaults();
                let saved: Value = serde_json::from_str(&j)?;
                for (key, value) in saved
                    .as_object()
                    .ok_or_else(|| err("storage_error", "Stored preferences are not an object"))?
                {
                    merged[key] = value.clone();
                }
                (r, merged)
            }
            None => (0, defaults()),
        };
        Ok(json!({"revision":revision,"preferences":preferences}))
    }
    pub(crate) fn preferences_save(&self, p: &Value) -> Result<Value> {
        fields(p, &["preferences", "expected_revision", "request_id"])?;
        let request = string(p, "request_id")?;
        let fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(p)?));
        let tx = self.db.unchecked_transaction()?;
        let prior: Option<(String, String)> = tx
            .query_row(
                "SELECT fingerprint,result_json FROM preference_operations WHERE request_id=?1",
                [request],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((old, response)) = prior {
            if old != fingerprint {
                return Err(err(
                    "request_id_conflict",
                    "Request ID identifies different preferences",
                ));
            }
            return Ok(serde_json::from_str(&response)?);
        }
        let current = self.preferences()?;
        crate::mutations::check_revision(
            required_revision(p)?,
            current["revision"].as_i64().unwrap(),
        )?;
        let patch = &p["preferences"];
        fields(
            patch,
            &[
                "density",
                "week_start",
                "calendar_start_hour",
                "calendar_end_hour",
                "timeline_start_hour",
                "timeline_end_hour",
                "selected_calendars",
                "calendar_view",
                "calendar_date",
                "timeline_date",
            ],
        )?;
        let mut preferences = current["preferences"].clone();
        for (key, value) in patch.as_object().unwrap() {
            preferences[key] = value.clone();
        }
        if ![Some("comfortable"), Some("compact")].contains(&preferences["density"].as_str())
            || ![Some("monday"), Some("sunday")].contains(&preferences["week_start"].as_str())
        {
            return Err(err("invalid_request", "Unknown density or week start"));
        }
        for prefix in ["calendar", "timeline"] {
            let start = preferences[format!("{prefix}_start_hour")].as_i64();
            let end = preferences[format!("{prefix}_end_hour")].as_i64();
            if !matches!((start,end),(Some(s),Some(e)) if s>=0 && s<e && e<=24) {
                return Err(err(
                    "invalid_request",
                    "Visible hours must satisfy 0 <= start < end <= 24",
                ));
            }
        }
        if ![Some("day"), Some("week"), Some("month"), Some("agenda")]
            .contains(&preferences["calendar_view"].as_str())
        {
            return Err(err("invalid_request", "Unknown calendar view"));
        }
        for key in ["calendar_date", "timeline_date"] {
            let value = &preferences[key];
            if !value.is_null()
                && !value.as_str().is_some_and(|date| {
                    date.len() == 10 && chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok()
                })
            {
                return Err(err(
                    "invalid_request",
                    "Calendar and timeline dates must be null or YYYY-MM-DD",
                ));
            }
        }
        if !preferences["selected_calendars"].is_null() {
            let selected = preferences["selected_calendars"]
                .as_array()
                .filter(|v| v.len() <= 100)
                .ok_or_else(|| {
                    err(
                        "invalid_request",
                        "Calendar selection must be null or at most 100 sources",
                    )
                })?;
            let mut seen = std::collections::HashSet::new();
            for source in selected {
                fields(source, &["account", "calendar_id"])?;
                let account = string(source, "account")?;
                let calendar = string(source, "calendar_id")?;
                if !seen.insert((account, calendar)) {
                    return Err(err("invalid_request", "Repeated calendar source"));
                }
                let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM calendar_sync_state WHERE account=?1 AND calendar_id=?2)",params![account,calendar],|r|r.get(0))?;
                if !exists {
                    return Err(err("not_found", "Selected calendar source does not exist"));
                }
            }
        }
        let revision = current["revision"].as_i64().unwrap() + 1;
        let result = json!({"revision":revision,"preferences":preferences});
        tx.execute("INSERT INTO preferences VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET revision=excluded.revision,json=excluded.json",params![revision,serde_json::to_string(&preferences)?])?;
        tx.execute(
            "INSERT INTO preference_operations VALUES(?1,?2,?3)",
            params![request, fingerprint, serde_json::to_string(&result)?],
        )?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(result)
    }
    pub(crate) fn account_update(&self, account: &str, p: &Value) -> Result<Value> {
        fields(
            p,
            &["name", "signature_text", "expected_revision", "request_id"],
        )?;
        let request = string(p, "request_id")?;
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&json!(["account_update", p]))?)
        );
        let tx = self.db.unchecked_transaction()?;
        let prior: Option<(String, String)> = tx
            .query_row(
                "SELECT fingerprint,result_json FROM operations WHERE account=?1 AND request_id=?2",
                params![account, request],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((old, response)) = prior {
            if old != fingerprint {
                return Err(err(
                    "request_id_conflict",
                    "Request ID identifies different account metadata",
                ));
            }
            return Ok(serde_json::from_str(&response)?);
        }
        let name = p["name"]
            .as_str()
            .filter(|v| {
                !v.trim().is_empty() && v.chars().count() <= 80 && !v.chars().any(char::is_control)
            })
            .ok_or_else(|| {
                err(
                    "invalid_request",
                    "Display name must contain 1–80 visible characters",
                )
            })?;
        let signature = p["signature_text"]
            .as_str()
            .filter(|v| v.len() <= 16 * 1024 && !v.contains('\0'))
            .ok_or_else(|| {
                err(
                    "invalid_request",
                    "Signature must be text of at most 16 KiB",
                )
            })?;
        let current: i64 = tx.query_row(
            "SELECT revision FROM account_metadata WHERE account=?1",
            [account],
            |r| r.get(0),
        )?;
        crate::mutations::check_revision(required_revision(p)?, current)?;
        tx.execute(
            "UPDATE accounts SET name=?1 WHERE id=?2",
            params![name, account],
        )?;
        tx.execute(
            "UPDATE account_metadata SET signature_text=?1,revision=revision+1 WHERE account=?2",
            params![signature, account],
        )?;
        let result = self.accounts()?["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == account)
            .unwrap()
            .clone();
        tx.execute(
            "INSERT INTO operations VALUES(?1,?2,?3,?4)",
            params![
                account,
                request,
                fingerprint,
                serde_json::to_string(&result)?
            ],
        )?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(result)
    }
}
pub(crate) fn required_revision(p: &Value) -> Result<i64> {
    p["expected_revision"]
        .as_i64()
        .filter(|v| *v >= 0)
        .ok_or_else(|| {
            err(
                "invalid_request",
                "expected_revision must be a nonnegative integer",
            )
        })
}
