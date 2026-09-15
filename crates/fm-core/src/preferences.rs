//! Typed local presentation preferences and account domain metadata.
use crate::*;
use rusqlite::params;
use sha2::{Digest, Sha256};

fn defaults() -> Value {
    json!({"density":"comfortable","mail_date_grouping":"grouped","sync_every_minutes":1,"week_start":"monday","calendar_start_hour":7,"calendar_end_hour":22,"timeline_start_hour":7,"timeline_end_hour":22,"selected_calendars":null,"calendar_view":"week","calendar_date":null,"timeline_date":null,
        "theme":"graphite","text_size":"default","startup_view":"mail","last_workspace":"mail","timeline_position":"footer","default_event_duration_minutes":60,"calendar_display_time_zone":null,"calendar_account_visibility":"all","google_sync_limits":null,"google_sync_services":{},"notifications":"events"})
}
impl Store {
    pub fn google_service_sync_enabled(&self, account: &str, service: &str) -> Result<bool> {
        if !matches!(service, "mail" | "calendar") {
            return Err(err("invalid_request", "Choose Mail or Calendar sync"));
        }
        Ok(
            self.preferences()?["preferences"]["google_sync_services"][account][service]
                .as_bool()
                .unwrap_or(true),
        )
    }
    pub fn google_sync_interval_ms(&self) -> Result<u64> {
        let value = self.preferences()?["preferences"]["sync_every_minutes"].as_u64();
        match value {
            Some(minutes @ (1 | 5 | 15)) => Ok(minutes * 60_000),
            _ => Err(err(
                "invalid_request",
                "Sync interval must be 1, 5 or 15 minutes",
            )),
        }
    }
    pub(crate) fn preferences(&self) -> Result<Value> {
        let mut profile_defaults = defaults();
        if self.profile_kind()? == crate::live_profile::ProfileKind::Synthetic {
            profile_defaults["calendar_display_time_zone"] = "America/Toronto".into();
        }
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
                let mut merged = profile_defaults;
                let saved: Value = serde_json::from_str(&j)?;
                for (key, value) in saved
                    .as_object()
                    .ok_or_else(|| err("storage_error", "Stored preferences are not an object"))?
                {
                    merged[key] = value.clone();
                }
                (r, merged)
            }
            None => (0, profile_defaults),
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
                "mail_date_grouping",
                "sync_every_minutes",
                "week_start",
                "calendar_start_hour",
                "calendar_end_hour",
                "timeline_start_hour",
                "timeline_end_hour",
                "selected_calendars",
                "calendar_account_visibility",
                "calendar_view",
                "calendar_date",
                "timeline_date",
                "theme",
                "text_size",
                "startup_view",
                "last_workspace",
                "timeline_position",
                "google_sync_limits",
                "google_sync_services",
                "notifications",
                "default_event_duration_minutes",
                "calendar_display_time_zone",
            ],
        )?;
        let mut preferences = current["preferences"].clone();
        for (key, value) in patch.as_object().unwrap() {
            preferences[key] = value.clone();
        }
        let services = preferences["google_sync_services"]
            .as_object()
            .filter(|v| v.len() <= 200)
            .ok_or_else(|| {
                err(
                    "invalid_request",
                    "Sync services must contain at most 200 account choices",
                )
            })?;
        for (account, choices) in services {
            crate::id(account)?;
            if !self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=?1)",
                [account],
                |r| r.get::<_, bool>(0),
            )? {
                return Err(err("not_found", "Sync account was not found"));
            }
            fields(choices, &["mail", "calendar"])?;
            if !choices["mail"].is_boolean() || !choices["calendar"].is_boolean() {
                return Err(err(
                    "invalid_request",
                    "Choose enabled or disabled for Mail and Calendar",
                ));
            }
        }
        if !matches!(preferences["sync_every_minutes"].as_u64(), Some(1 | 5 | 15)) {
            return Err(err(
                "invalid_request",
                "Sync interval must be 1, 5 or 15 minutes",
            ));
        }
        if !preferences["google_sync_limits"].is_null() {
            fm_google::scheduler::SchedulerConfig::from_confirmed_json(
                &preferences["google_sync_limits"],
            )
            .map_err(|_| {
                err(
                    "invalid_request",
                    "Supply all four actual Google Console quota limits (100–100000000 each)",
                )
            })?;
        }
        if !preferences["calendar_display_time_zone"].is_null() {
            let zone = preferences["calendar_display_time_zone"]
                .as_str()
                .filter(|v| !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control))
                .ok_or_else(|| {
                    err(
                        "invalid_request",
                        "Calendar display zone must be an IANA zone",
                    )
                })?;
            let day = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
            fm_calendar::Window::from_dates(day, day.succ_opt().unwrap(), zone)
                .map_err(crate::calendar_store::calendar_error)?;
        }
        for (key, allowed) in [
            ("mail_date_grouping", &["grouped", "plain"][..]),
            ("calendar_account_visibility", &["all", "current"][..]),
            ("theme", &["graphite", "paper", "terminal", "harbour"][..]),
            ("text_size", &["small", "default", "large"][..]),
            ("startup_view", &["mail", "calendar", "last"][..]),
            ("last_workspace", &["mail", "calendar"][..]),
            ("timeline_position", &["footer", "header"][..]),
            ("notifications", &["all", "events", "off"][..]),
        ] {
            if !preferences[key]
                .as_str()
                .is_some_and(|v| allowed.contains(&v))
            {
                return Err(err("invalid_request", &format!("Unknown {key} preference")));
            }
        }
        if !matches!(
            preferences["default_event_duration_minutes"].as_i64(),
            Some(30 | 45 | 60)
        ) {
            return Err(err(
                "invalid_request",
                "Default event duration must be 30, 45 or 60 minutes",
            ));
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
                let calendar = crate::live_calendar::provider_id(
                    source["calendar_id"].as_str().unwrap_or_default(),
                )?;
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
        if preferences["calendar_display_time_zone"]
            != current["preferences"]["calendar_display_time_zone"]
            || preferences["selected_calendars"] != current["preferences"]["selected_calendars"]
        {
            tx.execute("UPDATE calendar_publication SET revision=revision+1", [])?;
        }
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
        self.require_active_account(account)?;
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

#[cfg(test)]
mod appearance_tests {
    use super::*;
    #[test]
    fn calendar_account_visibility_is_additive_typed_and_durable() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let original = store.preferences().unwrap();
        assert_eq!(
            original["preferences"]["calendar_account_visibility"],
            "all"
        );
        let request = json!({"preferences":{"calendar_account_visibility":"current"},"expected_revision":0,"request_id":"calendar-visibility"});
        let saved = store
            .handle("preferences_save", None, request.clone())
            .unwrap();
        let mut expected = original["preferences"].clone();
        expected["calendar_account_visibility"] = json!("current");
        assert_eq!(saved["preferences"], expected);
        for invalid in [json!("hidden"), json!(false), json!(null), json!(1)] {
            assert_eq!(store.handle("preferences_save",None,json!({"preferences":{"calendar_account_visibility":invalid},"expected_revision":1,"request_id":"bad-visibility"})).unwrap_err().code,"invalid_request");
            assert_eq!(store.preferences().unwrap(), saved);
        }
        drop(store);
        let mut store = Store::open(dir.path()).unwrap();
        assert_eq!(store.preferences().unwrap(), saved);
        assert_eq!(
            store.handle("preferences_save", None, request).unwrap(),
            saved
        );
        let changed=store.handle("preferences_save",None,json!({"preferences":{"theme":"paper"},"expected_revision":1,"request_id":"unrelated-appearance"})).unwrap();
        assert_eq!(
            changed["preferences"]["calendar_account_visibility"],
            "current"
        );
        assert_eq!(
            changed["preferences"]["selected_calendars"],
            original["preferences"]["selected_calendars"]
        );
        assert_eq!(
            changed["preferences"]["notifications"],
            original["preferences"]["notifications"]
        );
    }
    #[test]
    fn account_service_switches_are_independent_durable_and_validated() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        assert!(store
            .google_service_sync_enabled("personal", "mail")
            .unwrap());
        assert!(store
            .google_service_sync_enabled("personal", "calendar")
            .unwrap());
        let command = json!({"preferences":{"google_sync_services":{"personal":{"mail":false,"calendar":true},"windscribe":{"mail":true,"calendar":false}}},"expected_revision":0,"request_id":"service-choices"});
        let saved = store
            .handle("preferences_save", None, command.clone())
            .unwrap();
        assert!(!store
            .google_service_sync_enabled("personal", "mail")
            .unwrap());
        assert!(store
            .google_service_sync_enabled("personal", "calendar")
            .unwrap());
        assert!(store
            .google_service_sync_enabled("windscribe", "mail")
            .unwrap());
        assert!(!store
            .google_service_sync_enabled("windscribe", "calendar")
            .unwrap());
        for bad in [
            json!({"personal":{"mail":"off","calendar":true}}),
            json!({"personal":{"mail":false}}),
            json!({"unknown-account":{"mail":false,"calendar":true}}),
            json!([]),
        ] {
            assert!(store.handle("preferences_save",None,json!({"preferences":{"google_sync_services":bad},"expected_revision":1,"request_id":"invalid-services"})).is_err());
            assert_eq!(store.preferences().unwrap(), saved);
        }
        drop(store);
        let mut store = Store::open(dir.path()).unwrap();
        assert!(!store
            .google_service_sync_enabled("personal", "mail")
            .unwrap());
        assert_eq!(
            store.handle("preferences_save", None, command).unwrap(),
            saved
        );
    }
    #[test]
    fn sync_cadence_is_shared_durable_and_rejects_invalid_values_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        assert_eq!(store.google_sync_interval_ms().unwrap(), 60_000);
        let command = json!({"preferences":{"sync_every_minutes":15},"expected_revision":0,"request_id":"slow-checks"});
        let saved = store
            .handle("preferences_save", None, command.clone())
            .unwrap();
        assert_eq!(store.google_sync_interval_ms().unwrap(), 900_000);
        for value in [json!(0), json!(2), json!(1.5), json!("5"), Value::Null] {
            assert_eq!(store.handle("preferences_save", None, json!({"preferences":{"sync_every_minutes":value,"theme":"paper"},"expected_revision":1,"request_id":"invalid-checks"})).unwrap_err().code, "invalid_request");
            assert_eq!(store.preferences().unwrap(), saved);
        }
        drop(store);
        let mut store = Store::open(dir.path()).unwrap();
        assert_eq!(store.google_sync_interval_ms().unwrap(), 900_000);
        assert_eq!(
            store.handle("preferences_save", None, command).unwrap(),
            saved
        );
    }

    #[test]
    fn mail_date_grouping_is_shared_durable_and_validated() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let before = store.preferences().unwrap();
        assert_eq!(before["preferences"]["mail_date_grouping"], "grouped");
        let request = json!({"request_id":"date-groups","expected_revision":before["revision"],"preferences":{"mail_date_grouping":"plain"}});
        store.handle("preferences_save", None, request).unwrap();
        drop(store);
        let mut reopened = Store::open(dir.path()).unwrap();
        let saved = reopened.preferences().unwrap();
        assert_eq!(saved["preferences"]["mail_date_grouping"], "plain");
        assert!(reopened.handle("preferences_save", None, json!({"request_id":"bad-date-group","expected_revision":saved["revision"],"preferences":{"mail_date_grouping":true}})).is_err());
    }

    #[test]
    fn confirmed_google_limits_are_durable_atomic_preferences_without_accounts() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open_live(dir.path()).unwrap();
        let limits = json!({"gmail_project_units_per_minute":1000,"gmail_account_units_per_minute":100,
            "calendar_project_units_per_minute":2000,"calendar_account_units_per_minute":200});
        let command = json!({"request_id":"confirmed-limits","expected_revision":0,"preferences":{"google_sync_limits":limits}});
        let saved = store
            .handle("preferences_save", None, command.clone())
            .unwrap();
        assert_eq!(saved["preferences"]["google_sync_limits"], limits);
        assert!(store.google_account_bindings().unwrap().is_empty());
        for invalid in [
            json!({}),
            json!(true),
            json!({"gmail_project_units_per_minute":1000}),
        ] {
            assert_eq!(
                store
                    .handle(
                        "preferences_save",
                        None,
                        json!({"request_id":"invalid-limits","expected_revision":1,
                "preferences":{"google_sync_limits":invalid,"theme":"paper"}})
                    )
                    .unwrap_err()
                    .code,
                "invalid_request"
            );
            assert_eq!(store.preferences().unwrap(), saved);
        }
        drop(store);
        let mut store = Store::open_live(dir.path()).unwrap();
        assert_eq!(store.preferences().unwrap(), saved);
        assert_eq!(
            store.handle("preferences_save", None, command).unwrap(),
            saved
        );
        assert!(store.google_account_bindings().unwrap().is_empty());
        let cleared = store
            .handle(
                "preferences_save",
                None,
                json!({"request_id":"clear-limits","expected_revision":1,
            "preferences":{"google_sync_limits":null}}),
            )
            .unwrap();
        assert!(cleared["preferences"]["google_sync_limits"].is_null());
    }

    #[test]
    fn appearance_defaults_merge_old_preferences_and_preserve_receipts() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let old = json!({"density":"compact","timeline_start_hour":8,"timeline_end_hour":22,
            "selected_calendars":[],"calendar_view":"month"});
        let command =
            json!({"preferences":old,"expected_revision":0,"request_id":"old-preferences"});
        let receipt = json!({"revision":1,"preferences":old});
        store
            .db
            .execute("INSERT INTO preferences VALUES(1,1,?1)", [old.to_string()])
            .unwrap();
        store
            .db
            .execute(
                "INSERT INTO preference_operations VALUES(?1,?2,?3)",
                params![
                    "old-preferences",
                    format!(
                        "{:x}",
                        Sha256::digest(serde_json::to_vec(&command).unwrap())
                    ),
                    receipt.to_string()
                ],
            )
            .unwrap();
        let merged = store.preferences().unwrap();
        assert_eq!(merged["revision"], 1);
        assert_eq!(merged["preferences"]["theme"], "graphite");
        assert_eq!(merged["preferences"]["default_event_duration_minutes"], 60);
        for (key, value) in old.as_object().unwrap() {
            assert_eq!(&merged["preferences"][key], value);
        }
        // Old operation responses are immutable even as read defaults expand.
        assert_eq!(
            store
                .handle("preferences_save", None, command.clone())
                .unwrap(),
            receipt
        );
        drop(store);
        let mut store = Store::open(dir.path()).unwrap();
        assert_eq!(store.preferences().unwrap(), merged);
        assert_eq!(
            store.handle("preferences_save", None, command).unwrap(),
            receipt
        );
    }

    #[test]
    fn appearance_preferences_are_typed_atomic_revisioned_and_durable() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let draft_before = store.draft("windscribe", "windscribe-thread-003").unwrap();
        let command = json!({"preferences":{"theme":"paper","text_size":"large","startup_view":"last",
            "last_workspace":"calendar","timeline_position":"header","default_event_duration_minutes":45},
            "expected_revision":0,"request_id":"appearance"});
        let saved = store
            .handle("preferences_save", None, command.clone())
            .unwrap();
        for invalid in [
            json!({"theme":"sepia"}),
            json!({"text_size":15.5}),
            json!({"startup_view":"settings"}),
            json!({"last_workspace":"last"}),
            json!({"timeline_position":"top"}),
            json!({"default_event_duration_minutes":45.0}),
            json!({"default_event_duration_minutes":90}),
            json!({"sync_interval":5}),
        ] {
            let error = store
                .handle(
                    "preferences_save",
                    None,
                    json!({"preferences":invalid,
                "expected_revision":1,"request_id":"invalid"}),
                )
                .unwrap_err();
            assert_eq!(error.code, "invalid_request");
            assert_eq!(store.preferences().unwrap(), saved);
        }
        assert_eq!(
            store
                .handle(
                    "preferences_save",
                    None,
                    json!({"preferences":{"theme":"harbour"},
            "expected_revision":0,"request_id":"stale"})
                )
                .unwrap_err()
                .code,
            "stale_revision"
        );
        drop(store);
        let mut store = Store::open(dir.path()).unwrap();
        assert_eq!(store.preferences().unwrap(), saved);
        assert_eq!(
            store.handle("preferences_save", None, command).unwrap(),
            saved
        );
        assert_eq!(
            store.draft("windscribe", "windscribe-thread-003").unwrap(),
            draft_before
        );
        for (index, theme) in ["terminal", "harbour", "graphite"].iter().enumerate() {
            let next = store
                .handle(
                    "preferences_save",
                    None,
                    json!({"preferences":{"theme":theme},
                "expected_revision":index+1,"request_id":format!("theme-{index}")}),
                )
                .unwrap();
            assert_eq!(next["preferences"]["theme"], *theme);
            assert_eq!(next["preferences"]["timeline_position"], "header");
        }
    }
}
