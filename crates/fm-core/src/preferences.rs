//! Typed local presentation preferences and account domain metadata.
//!
//! Preferences are one JSON document shared by the GUI and CLI, stored as a
//! singleton row with a revision. Reads merge the saved keys over current
//! defaults, so keys added later appear without a migration. Saves are
//! patches: they must name the revision they started from (`stale_revision`
//! otherwise), are validated as a whole, and are idempotent per request ID
//! through `preference_operations`.
use crate::*;
use rusqlite::params;
use sha2::{Digest, Sha256};

fn defaults() -> Value {
    json!({"density":"comfortable","show_draft_chips":true,"reply_marks_delayed_done":true,"review_leave_seconds":10,"calendar_recent_searches":[],"sync_every_minutes":1,"week_start":"monday","mail_week_start":"monday","calendar_start_hour":7,"calendar_end_hour":22,"timeline_start_hour":0,"timeline_end_hour":24,"selected_calendars":null,"calendar_view":"week","calendar_date":null,"timeline_date":null,
        "theme":"graphite","text_size":"default","motion":"system","startup_view":"mail","last_workspace":"mail","timeline_position":"footer","default_event_duration_minutes":60,"default_reminder_minutes":15,"reminder_snooze_minutes":5,"calendar_display_time_zone":null,"calendar_account_visibility":"all","calendar_show_declined":false,"google_sync_limits":null,"google_sync_services":{},"notifications":"events","shortcuts":{},
        "blocked_hosts":[],"remote_images_from":[],"html_senders":[],"starred_senders":[],"remote_proxy":{"enabled":false,"host":"","port":443,"username":""},"sender_pictures":true,"key_hints":true})
}
/// The account colour palette: muted swatches shown 8×2 in the picker. A new
/// account takes the first one no other account uses, cycling when all are.
pub const ACCOUNT_COLORS: [&str; 16] = [
    "#C9A36B", "#E0B85A", "#CF8F6A", "#E07F7F", "#D58E9A", "#C98BC9", "#AE97D4", "#8C8FE0",
    "#8AA9D6", "#6FA3C7", "#6FB5B2", "#7FC4A4", "#8FB39A", "#B9C77A", "#C9A58F", "#A3AAB3",
];
impl Store {
    /// Returns whether the user left Google sync for `service` (`mail` or
    /// `calendar`) enabled on `account`. Accounts without a saved choice sync
    /// both services.
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
    /// Returns the background sync cadence in milliseconds (1, 5 or 15
    /// minutes); any other stored value fails with `invalid_request`.
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
    /// Lets polling clients skip reloading unchanged preferences.
    pub(crate) fn preferences_revision(&self) -> Result<i64> {
        Ok(self.db.query_row(
            "SELECT COALESCE((SELECT revision FROM preferences WHERE singleton=1),0)",
            [],
            |r| r.get(0),
        )?)
    }
    /// Returns `{revision, preferences}` with saved keys merged over the
    /// defaults. Revision 0 means nothing was ever saved.
    pub(crate) fn preferences(&self) -> Result<Value> {
        let mut profile_defaults = defaults();
        // The demo fixture's events are in Toronto; give it a zone so the
        // calendar renders without setup.
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
                    // Retired: Sent and Drafts are always grouped by date, and
                    // HTML or plain text is chosen per sender (`html_senders`).
                    if key != "mail_date_grouping" && key != "message_format" {
                        merged[key] = value.clone();
                    }
                }
                (r, merged)
            }
            None => (0, profile_defaults),
        };
        Ok(json!({"revision":revision,"preferences":preferences}))
    }
    /// Applies a preferences patch `{preferences, expected_revision,
    /// request_id}` and returns the new `{revision, preferences}`.
    ///
    /// Only known keys are accepted and the merged document is validated as a
    /// whole, so a rejected save changes nothing. Replaying a request ID
    /// returns its original result even if defaults have grown since. A
    /// change to the display zone or calendar selection also bumps the
    /// calendar publication revision so Calendar views reload.
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
        // Optimistic concurrency: a GUI and CLI saving at once cannot silently
        // overwrite each other; the loser reloads and reapplies.
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
                "show_draft_chips",
                "reply_marks_delayed_done",
                "review_leave_seconds",
                "calendar_recent_searches",
                "sync_every_minutes",
                "week_start",
                "mail_week_start",
                "calendar_start_hour",
                "calendar_end_hour",
                "timeline_start_hour",
                "timeline_end_hour",
                "selected_calendars",
                "calendar_account_visibility",
                "calendar_show_declined",
                "calendar_view",
                "calendar_date",
                "timeline_date",
                "theme",
                "text_size",
                "motion",
                "startup_view",
                "last_workspace",
                "timeline_position",
                "google_sync_limits",
                "google_sync_services",
                "notifications",
                "shortcuts",
                "default_event_duration_minutes",
                "default_reminder_minutes",
                "reminder_snooze_minutes",
                "calendar_display_time_zone",
                "blocked_hosts",
                "remote_images_from",
                "html_senders",
                "starred_senders",
                "remote_proxy",
                "sender_pictures",
                "key_hints",
            ],
        )?;
        // Validate the merged result, not just the patch: some rules span keys
        // (visible hours, shortcut conflicts).
        let mut preferences = current["preferences"].clone();
        for (key, value) in patch.as_object().unwrap() {
            preferences[key] = value.clone();
        }
        validate_shortcuts(&preferences["shortcuts"])?;
        crate::remote_prefs::validate(&preferences)?;
        // Contacts starred from their card, per account.
        crate::remote_prefs::senders(&preferences["starred_senders"], "Starred")?;
        let services = preferences["google_sync_services"]
            .as_object()
            .filter(|v| v.len() <= 200)
            .ok_or_else(|| {
                err(
                    "invalid_request",
                    "Sync services must contain at most 200 account choices",
                )
            })?;
        // A stored choice for an account deleted since then is dropped, so it
        // cannot block every later save. An unknown account the request itself
        // introduces is still refused.
        let stored_services = &current["preferences"]["google_sync_services"];
        let mut stale = Vec::new();
        for account in services.keys() {
            crate::id(account)?;
            if !self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=?1)",
                [account],
                |r| r.get::<_, bool>(0),
            )? {
                if stored_services.get(account).is_none() {
                    return Err(err("not_found", "Sync account was not found"));
                }
                stale.push(account.clone());
            }
        }
        let services = preferences["google_sync_services"].as_object_mut().unwrap();
        for account in &stale {
            services.remove(account);
        }
        let services = preferences["google_sync_services"].as_object().unwrap();
        for (_, choices) in services {
            fields(
                choices,
                &["mail", "calendar", "mail_access", "calendar_access"],
            )?;
            for mode in ["mail_access", "calendar_access"] {
                if choices
                    .get(mode)
                    .is_some_and(|v| !matches!(v.as_str(), Some("read" | "write")))
                {
                    return Err(err("invalid_request", "Choose Read or Read & write"));
                }
            }
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
            ("calendar_account_visibility", &["all", "current"][..]),
            ("theme", &["graphite", "paper", "terminal", "harbour"][..]),
            ("text_size", &["small", "default", "large"][..]),
            // Interface motion: follow the system's reduce-motion setting,
            // or always full, or always reduced.
            ("motion", &["system", "full", "reduced"][..]),
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
        if !preferences["calendar_show_declined"].is_boolean() {
            return Err(err(
                "invalid_request",
                "Show declined events must be true or false",
            ));
        }
        // How long an open To review conversation stays there; 0 keeps it
        // until another is opened.
        if !matches!(
            preferences["review_leave_seconds"].as_i64(),
            Some(0 | 5 | 10 | 30)
        ) {
            return Err(err(
                "invalid_request",
                "Leaving To review must be off or 5, 10 or 30 seconds",
            ));
        }
        if !preferences["reply_marks_delayed_done"].is_boolean() {
            return Err(err(
                "invalid_request",
                "Replying to Delayed mail marks it Done, or not",
            ));
        }
        if !preferences["key_hints"].is_boolean() {
            return Err(err(
                "invalid_request",
                "Show the shortcut bar must be true or false",
            ));
        }
        if !preferences["show_draft_chips"].is_boolean() {
            return Err(err(
                "invalid_request",
                "Show drafts in the corner must be true or false",
            ));
        }
        // Calendar search's Recent: up to six distinct one-line searches.
        let recent = preferences["calendar_recent_searches"]
            .as_array()
            .filter(|v| v.len() <= 6)
            .ok_or_else(|| {
                err(
                    "invalid_request",
                    "Recent calendar searches must be a list of at most six",
                )
            })?;
        let mut seen = std::collections::HashSet::new();
        for search in recent {
            if !search.as_str().is_some_and(|v| {
                !v.trim().is_empty() && v.len() <= 512 && !v.chars().any(char::is_control)
            }) || !seen.insert(search.as_str())
            {
                return Err(err(
                    "invalid_request",
                    "Each recent calendar search is distinct one-line text of at most 512 bytes",
                ));
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
        if !matches!(
            preferences["default_reminder_minutes"].as_i64(),
            Some(5 | 15 | 60)
        ) {
            return Err(err(
                "invalid_request",
                "Default reminder must be 5, 15 or 60 minutes",
            ));
        }
        if !matches!(
            preferences["reminder_snooze_minutes"].as_i64(),
            Some(1 | 5 | 10)
        ) {
            return Err(err(
                "invalid_request",
                "Reminder snooze must be 1, 5 or 10 minutes",
            ));
        }
        if ![Some("comfortable"), Some("compact")].contains(&preferences["density"].as_str())
            || ![Some("monday"), Some("sunday")].contains(&preferences["week_start"].as_str())
        {
            return Err(err("invalid_request", "Unknown density or week start"));
        }
        // Mail's This Week group has its own start day, apart from Calendar's.
        if crate::queries::weekday(preferences["mail_week_start"].as_str().unwrap_or("")).is_none()
        {
            return Err(err(
                "invalid_request",
                "Mail week start must name a weekday",
            ));
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
            let stored_selection = current["preferences"]["selected_calendars"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut kept = Vec::new();
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
                if exists {
                    kept.push(source.clone());
                } else if !stored_selection.contains(source) {
                    return Err(err("not_found", "Selected calendar source does not exist"));
                }
                // A saved source whose account was since deleted is dropped.
            }
            preferences["selected_calendars"] = Value::Array(kept);
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
    /// Updates an account's local metadata: display name, signature, mail
    /// retention, per-service sync and access choices, colour and history
    /// analysis. Returns the account row.
    ///
    /// Revision-checked against the account metadata and idempotent per
    /// request ID. Changing retention restarts mail acquisition and purges
    /// local mail older than the new window in the same transaction (never
    /// Google's copy). Sync choices are written into the
    /// shared preferences, bumping their revision.
    pub(crate) fn account_update(&self, account: &str, p: &Value) -> Result<Value> {
        fields(
            p,
            &[
                "name",
                "signature_text",
                "mail_retention",
                "sync",
                "analyze_history",
                "color",
                "expected_revision",
                "request_id",
            ],
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
        if let Some(retention) = p.get("mail_retention") {
            let retention = retention
                .as_str()
                .ok_or_else(|| err("invalid_request", "Invalid mail retention"))?;
            let since = mail_retention_since(retention)?;
            let old: String = tx.query_row(
                "SELECT mail_retention FROM account_metadata WHERE account=?1",
                [account],
                |r| r.get(0),
            )?;
            if old != retention {
                self.change_mail_retention(account, since)?;
                tx.execute(
                    "UPDATE account_metadata SET mail_retention=?2,mail_since=?3 WHERE account=?1",
                    params![account, retention, since],
                )?;
            }
        }
        if let Some(sync) = p.get("sync") {
            fields(
                sync,
                &["mail", "calendar", "mail_access", "calendar_access"],
            )?;
            if !sync["mail"].is_boolean()
                || !sync["calendar"].is_boolean()
                || (sync["mail"] == false && sync["calendar"] == false)
                || !matches!(sync["mail_access"].as_str(), Some("read" | "write"))
                || !matches!(sync["calendar_access"].as_str(), Some("read" | "write"))
            {
                return Err(err(
                    "invalid_request",
                    "Enable Mail or Calendar and choose Read or Read & write for each",
                ));
            }
            let current = self.preferences()?;
            let mut preferences = current["preferences"].clone();
            preferences["google_sync_services"][account] = sync.clone();
            tx.execute("INSERT INTO preferences(singleton,revision,json) VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET revision=excluded.revision,json=excluded.json",params![current["revision"].as_i64().unwrap()+1,serde_json::to_string(&preferences)?])?;
        }
        if let Some(color) = p.get("color") {
            let color = color
                .as_str()
                .ok_or_else(|| err("invalid_request", "Choose one of the account colours"))?;
            set_account_color(&tx, account, color)?;
        }
        if let Some(analyze) = p.get("analyze_history") {
            let analyze = analyze
                .as_bool()
                .ok_or_else(|| err("invalid_request", "analyze_history must be true or false"))?;
            crate::local_model::set_history_analysis(&tx, account, analyze)?;
        }
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
/// Remove a deleted account's sync choices and calendar selections from the
/// shared preferences, so later saves never refer to it.
pub(crate) fn forget_account(db: &rusqlite::Connection, account: &str) -> Result<()> {
    let stored: Option<(i64, String)> = db
        .query_row(
            "SELECT revision,json FROM preferences WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((revision, raw)) = stored else {
        return Ok(());
    };
    let mut preferences: Value = serde_json::from_str(&raw)?;
    let mut changed = preferences["google_sync_services"]
        .as_object_mut()
        .is_some_and(|services| services.remove(account).is_some());
    if let Some(selected) = preferences["selected_calendars"].as_array_mut() {
        let before = selected.len();
        selected.retain(|source| source["account"] != account);
        changed |= selected.len() != before;
    }
    if changed {
        db.execute(
            "UPDATE preferences SET revision=?1,json=?2 WHERE singleton=1",
            params![revision + 1, serde_json::to_string(&preferences)?],
        )?;
    }
    Ok(())
}
fn color_key(account: &str) -> String {
    format!("account_color:{account}")
}
/// A newly added account takes the first palette colour no other account uses,
/// cycling once all are taken. Stored as a palette index beside the account.
pub(crate) fn assign_account_color(db: &rusqlite::Connection, account: &str) -> Result<()> {
    let used: Vec<i64> = db
        .prepare("SELECT value FROM meta WHERE key LIKE 'account_color:%'")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let index = (0..ACCOUNT_COLORS.len() as i64)
        .find(|i| !used.contains(i))
        .unwrap_or(used.len() as i64 % ACCOUNT_COLORS.len() as i64);
    db.execute(
        "INSERT OR REPLACE INTO meta(key,value) VALUES(?1,?2)",
        params![color_key(account), index],
    )?;
    Ok(())
}
/// The account's colour: its saved palette entry, or palette order by account.
pub(crate) fn account_color(db: &rusqlite::Connection, account: &str) -> Result<&'static str> {
    let saved: Option<i64> = db
        .query_row(
            "SELECT value FROM meta WHERE key=?1",
            [color_key(account)],
            |r| r.get(0),
        )
        .optional()?;
    let index = match saved {
        Some(index) => index,
        None => db.query_row(
            "SELECT count(*) FROM accounts WHERE rowid<(SELECT rowid FROM accounts WHERE id=?1)",
            [account],
            |r| r.get(0),
        )?,
    };
    Ok(ACCOUNT_COLORS[index.rem_euclid(ACCOUNT_COLORS.len() as i64) as usize])
}
/// Saves an explicit palette choice for the account; only [`ACCOUNT_COLORS`]
/// entries are accepted (case-insensitively).
pub(crate) fn set_account_color(
    db: &rusqlite::Connection,
    account: &str,
    color: &str,
) -> Result<()> {
    let index = ACCOUNT_COLORS
        .iter()
        .position(|c| c.eq_ignore_ascii_case(color))
        .ok_or_else(|| err("invalid_request", "Choose one of the account colours"))?;
    db.execute(
        "INSERT OR REPLACE INTO meta(key,value) VALUES(?1,?2)",
        params![color_key(account), index as i64],
    )?;
    Ok(())
}
pub(crate) fn forget_account_color(db: &rusqlite::Connection, account: &str) -> Result<()> {
    db.execute("DELETE FROM meta WHERE key=?1", [color_key(account)])?;
    Ok(())
}
/// Returns whether the user allows provider writes for `service` on
/// `account`: the service is not switched off and its access is not set to
/// read-only. Absent choices allow writes; the OAuth grant is checked
/// separately.
pub(crate) fn google_write_enabled(
    db: &rusqlite::Connection,
    account: &str,
    service: &str,
) -> Result<bool> {
    let saved: Option<String> = db
        .query_row("SELECT json FROM preferences WHERE singleton=1", [], |r| {
            r.get(0)
        })
        .optional()?;
    let saved: Value = saved
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or(json!({}));
    let choice = &saved["google_sync_services"][account];
    Ok(choice[service] != false && choice[format!("{service}_access")] != "read")
}

/// Validates the `shortcuts` override map (action to key, `null` to unbind).
/// Keys must be one supported key or modifier combination, fixed system
/// bindings cannot be reassigned, and no two actions whose scopes overlap
/// (`all` overlaps everything) may share a key, counting defaults.
fn validate_shortcuts(value: &Value) -> Result<()> {
    let choices = value
        .as_object()
        .filter(|m| m.len() <= 32)
        .ok_or_else(|| err("invalid_request", "Shortcuts must be an action-to-key map"))?;
    let defaults = [
        ("next", "mail", "J"),
        ("prev", "mail", "K"),
        ("reply", "mail", "R"),
        ("replyAll", "mail", "A"),
        ("forward", "mail", "F"),
        ("unread", "mail", "U"),
        ("delay", "mail", "D"),
        ("done", "mail", "E"),
        ("compose", "mail", "N"),
        ("scopes", "mail", "S"),
        ("search", "mail", "/"),
        ("delete", "mail", "Delete"),
        ("day", "calendar", "D"),
        ("week", "calendar", "W"),
        ("month", "calendar", "M"),
        ("agenda", "calendar", "A"),
        ("back", "calendar", "Left"),
        ("fwd", "calendar", "Right"),
        ("today", "calendar", "T"),
        ("newEv", "calendar", "N"),
        ("view", "all", "V"),
        ("settings", "all", ","),
        ("undo", "all", "Z"),
    ];
    for (action, key) in choices {
        if !defaults.iter().any(|v| v.0 == action) {
            return Err(err("invalid_request", "Unknown shortcut action"));
        }
        if key.is_null() {
            continue;
        }
        let key = key
            .as_str()
            .filter(|k| {
                !k.is_empty()
                    && k.len() <= 64
                    && k.chars().all(|c| c.is_ascii_graphic() || c == ' ')
            })
            .ok_or_else(|| err("invalid_request", "Choose one key or key combination"))?;
        let mut base = key;
        for prefix in ["Ctrl+", "Alt+", "Shift+", "Meta+"] {
            if let Some(rest) = base.strip_prefix(prefix) {
                base = rest;
            }
        }
        let letter = base.len() == 1
            && base.as_bytes()[0].is_ascii_graphic()
            && !base.as_bytes()[0].is_ascii_lowercase();
        let named = [
            "Left",
            "Right",
            "Up",
            "Down",
            "Space",
            "Delete",
            "Backspace",
            "Return",
            "Home",
            "End",
            "PgUp",
            "PgDown",
        ]
        .contains(&base);
        let function = base
            .strip_prefix('F')
            .and_then(|v| v.parse::<u8>().ok())
            .is_some_and(|n| (1..=35).contains(&n));
        if !letter && !named && !function {
            return Err(err(
                "invalid_request",
                "Choose one supported key combination",
            ));
        }
        if [
            "Ctrl+K",
            "Meta+K",
            "Ctrl+Return",
            "Meta+Return",
            "Esc",
            "Escape",
            "Tab",
            "Shift+Tab",
        ]
        .contains(&key)
        {
            return Err(err("invalid_request", "This shortcut is fixed"));
        }
    }
    for (i, (action, scope, default)) in defaults.iter().enumerate() {
        let key = choices
            .get(*action)
            .map(|v| v.as_str().unwrap_or(""))
            .unwrap_or(default);
        if key.is_empty() {
            continue;
        }
        for (other, other_scope, other_default) in defaults.iter().skip(i + 1) {
            let other_key = choices
                .get(*other)
                .map(|v| v.as_str().unwrap_or(""))
                .unwrap_or(other_default);
            if (scope == other_scope || *scope == "all" || *other_scope == "all")
                && key == other_key
            {
                return Err(err(
                    "invalid_request",
                    "A shortcut must be removed from its previous action before moving it",
                ));
            }
        }
    }
    Ok(())
}

/// Converts a retention choice into its cutoff as Unix seconds from now, or
/// `None` for `all`.
pub(crate) fn mail_retention_since(retention: &str) -> Result<Option<i64>> {
    let months = match retention {
        "1_month" => 1,
        "3_months" => 3,
        "1_year" => 12,
        "2_years" => 24,
        "5_years" => 60,
        "90_days" => {
            return Ok(Some(
                (chrono::Utc::now() - chrono::Duration::days(90)).timestamp(),
            ))
        }
        "all" => return Ok(None),
        _ => {
            return Err(err(
                "invalid_request",
                "Choose 90 days, 1 year, 2 years, 5 years or Everything",
            ))
        }
    };
    Ok(Some(
        (chrono::Utc::now() - chrono::Months::new(months)).timestamp(),
    ))
}

/// Reads the mandatory, nonnegative `expected_revision` from a request.
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
    fn shortcut_scope_conflicts_and_fixed_bindings_are_enforced() {
        assert!(validate_shortcuts(&json!({})).is_ok());
        assert!(validate_shortcuts(&json!({"reply":"D"})).is_err());
        assert!(validate_shortcuts(&json!({"reply":"D","delay":null})).is_ok());
        assert!(validate_shortcuts(&json!({"view":"D","delay":null})).is_err());
        assert!(validate_shortcuts(&json!({"view":"D","delay":null,"day":null})).is_ok());
        for key in ["Ctrl+K", "Meta+K", "Ctrl+Return", "Tab", "Esc"] {
            assert!(validate_shortcuts(&json!({"reply":key})).is_err());
        }
        assert!(validate_shortcuts(&json!({"unknown":"X"})).is_err());
    }
    #[test]
    fn retention_setup_choices_are_durable_and_editable_after_acquisition() {
        for (index, choice) in ["90_days", "1_year", "2_years", "5_years", "all"]
            .iter()
            .enumerate()
        {
            let dir = tempfile::tempdir().unwrap();
            let mut store = Store::open_live(dir.path()).unwrap();
            let account = store
                .bind_google_account(
                    "retention-owner",
                    "owner@example.test",
                    &std::collections::BTreeSet::from([
                        "https://www.googleapis.com/auth/gmail.modify".into(),
                    ]),
                    None,
                )
                .unwrap();
            let before = store.accounts().unwrap()["accounts"][0].clone();
            assert_eq!(before["mail_retention"], "1_year");
            let p = json!({"name":"History","signature_text":"","mail_retention":choice,"expected_revision":before["metadata_revision"],"request_id":format!("history-{index}")});
            let saved = store
                .handle("account_update", Some(&account), p.clone())
                .unwrap();
            assert_eq!(saved["mail_retention"], *choice);
            assert_eq!(saved["mail_since"].is_null(), *choice == "all");
            drop(store);
            let mut store = Store::open_live(dir.path()).unwrap();
            assert_eq!(
                store.handle("account_update", Some(&account), p).unwrap(),
                saved
            );
            store
                .google_live_begin(&account, "100", chrono::Utc::now().timestamp())
                .unwrap();
            let next = if *choice == "all" { "1_month" } else { "all" };
            assert_eq!(store.handle("account_update",Some(&account),json!({"name":"History","signature_text":"","mail_retention":next,"expected_revision":saved["metadata_revision"],"request_id":"edit-later"})).unwrap()["mail_retention"],next);
            assert_eq!(
                store.accounts().unwrap()["accounts"][0]["mail_since"].is_null(),
                next == "all"
            );
        }
    }
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
    fn boolean_display_preferences_default_validate_and_persist() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let original = store.preferences().unwrap();
        assert_eq!(original["preferences"]["calendar_show_declined"], false);
        assert_eq!(original["preferences"]["sender_pictures"], true);
        assert_eq!(original["preferences"]["key_hints"], true);
        assert!(store.remote_content_settings().unwrap().sender_pictures);
        let request = json!({"preferences":{"calendar_show_declined":true,"sender_pictures":false,"key_hints":false},"expected_revision":0,"request_id":"display-booleans"});
        let saved = store
            .handle("preferences_save", None, request.clone())
            .unwrap();
        let mut expected = original["preferences"].clone();
        expected["calendar_show_declined"] = json!(true);
        expected["sender_pictures"] = json!(false);
        expected["key_hints"] = json!(false);
        assert_eq!(saved["preferences"], expected);
        assert!(!store.remote_content_settings().unwrap().sender_pictures);
        for key in ["calendar_show_declined", "sender_pictures", "key_hints"] {
            for invalid in [json!("true"), json!(null), json!(1)] {
                let mut patch = json!({});
                patch[key] = invalid;
                assert_eq!(store.handle("preferences_save",None,json!({"preferences":patch,"expected_revision":1,"request_id":"bad-display-boolean"})).unwrap_err().code,"invalid_request");
                assert_eq!(store.preferences().unwrap(), saved);
            }
        }
        drop(store);
        let mut store = Store::open(dir.path()).unwrap();
        assert_eq!(store.preferences().unwrap(), saved);
        assert_eq!(
            store.handle("preferences_save", None, request).unwrap(),
            saved
        );
        // A profile saved before these keys existed reads their defaults.
        let old = tempfile::tempdir().unwrap();
        let store = Store::open(old.path()).unwrap();
        store
            .db
            .execute(
                "INSERT INTO preferences VALUES(1,1,?1)",
                [r#"{"theme":"paper"}"#],
            )
            .unwrap();
        let read = store.preferences().unwrap();
        assert_eq!(read["preferences"]["calendar_show_declined"], false);
        assert_eq!(read["preferences"]["sender_pictures"], true);
        assert_eq!(read["preferences"]["key_hints"], true);
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
    fn stale_sync_choices_for_deleted_accounts_never_block_saves() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let saved = store
            .handle(
                "preferences_save",
                None,
                json!({"preferences":{"google_sync_services":{"personal":{"mail":false,"calendar":true}}},"expected_revision":0,"request_id":"service-choices"}),
            )
            .unwrap();
        // A choice left behind by an account deleted outside this path is
        // dropped on the next save rather than blocking every save.
        let mut stale = saved["preferences"].clone();
        stale["google_sync_services"]["gone-account"] = json!({"mail":true,"calendar":true});
        store
            .db
            .execute(
                "UPDATE preferences SET json=?1 WHERE singleton=1",
                [stale.to_string()],
            )
            .unwrap();
        let repaired = store
            .handle(
                "preferences_save",
                None,
                json!({"preferences":{"theme":"paper"},"expected_revision":saved["revision"],"request_id":"stale-service"}),
            )
            .unwrap();
        assert!(repaired["preferences"]["google_sync_services"]
            .get("gone-account")
            .is_none());
        assert_eq!(
            repaired["preferences"]["google_sync_services"]["personal"]["mail"],
            false
        );
        assert_eq!(
            store
                .handle(
                    "preferences_save",
                    None,
                    json!({"preferences":{"google_sync_services":{"new-unknown":{"mail":true,"calendar":true}}},"expected_revision":repaired["revision"],"request_id":"unknown-service"}),
                )
                .unwrap_err()
                .code,
            "not_found",
            "an unknown account the request introduces is still refused"
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
    fn draft_chips_and_calendar_recent_are_shared_durable_and_validated() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        // A profile saved while Sent and Drafts could be ungrouped reads without it.
        store
            .db
            .execute(
                "INSERT INTO preferences VALUES(1,1,?1)",
                [r#"{"mail_date_grouping":"plain"}"#],
            )
            .unwrap();
        let before = store.preferences().unwrap();
        assert!(before["preferences"].get("mail_date_grouping").is_none());
        assert_eq!(before["preferences"]["show_draft_chips"], true);
        assert_eq!(before["preferences"]["calendar_recent_searches"], json!([]));
        let request = json!({"request_id":"corner","expected_revision":before["revision"],
            "preferences":{"show_draft_chips":false,"calendar_recent_searches":["with:maya@example.test","title:standup"]}});
        store.handle("preferences_save", None, request).unwrap();
        drop(store);
        let mut reopened = Store::open(dir.path()).unwrap();
        let saved = reopened.preferences().unwrap();
        assert_eq!(saved["preferences"]["show_draft_chips"], false);
        assert_eq!(
            saved["preferences"]["calendar_recent_searches"],
            json!(["with:maya@example.test", "title:standup"])
        );
        assert!(saved["preferences"].get("mail_date_grouping").is_none());
        let stored: String = reopened
            .db
            .query_row("SELECT json FROM preferences", [], |r| r.get(0))
            .unwrap();
        assert!(
            !stored.contains("mail_date_grouping"),
            "the next save drops the retired key"
        );
        for (index, patch) in [
            json!({"show_draft_chips":"yes"}),
            json!({"mail_date_grouping":"plain"}),
            json!({"calendar_recent_searches":"standup"}),
            json!({"calendar_recent_searches":["a","b","c","d","e","f","g"]}),
            json!({"calendar_recent_searches":["same","same"]}),
            json!({"calendar_recent_searches":["two
lines"]}),
            json!({"calendar_recent_searches":[" "]}),
            json!({"calendar_recent_searches":["x".repeat(513)]}),
            json!({"calendar_recent_searches":[7]}),
        ]
        .into_iter()
        .enumerate()
        {
            assert!(reopened.handle("preferences_save", None, json!({"request_id":format!("bad-{index}"),"expected_revision":saved["revision"],"preferences":patch})).is_err(), "{patch}");
        }
    }

    #[test]
    fn html_is_chosen_per_sender() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let before = store.preferences().unwrap();
        // Plain text unless a sender is listed; the old global choice is gone.
        assert_eq!(before["preferences"]["html_senders"], json!([]));
        assert!(before["preferences"].get("message_format").is_none());
        let senders = json!([{"account":"acct-1","email":"news@x.test"}]);
        let request = json!({"request_id":"html","expected_revision":before["revision"],"preferences":{"html_senders":senders}});
        let saved = store.handle("preferences_save", None, request).unwrap();
        assert_eq!(
            store.preferences().unwrap()["preferences"]["html_senders"],
            senders
        );
        assert!(store.handle("preferences_save", None, json!({"request_id":"old-format","expected_revision":saved["revision"],"preferences":{"message_format":"html"}})).is_err());
        // A profile that saved the old global choice reads without it.
        store.db.execute("UPDATE preferences SET json=json_set(json,'$.message_format','html') WHERE singleton=1", []).unwrap();
        let reread = store.preferences().unwrap();
        assert!(reread["preferences"].get("message_format").is_none());
        assert_eq!(reread["preferences"]["html_senders"], senders);
    }

    #[test]
    fn starred_senders_are_per_account_lowercase_and_durable() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let before = store.preferences().unwrap();
        assert_eq!(before["preferences"]["starred_senders"], json!([]));
        let starred = json!([{"account":"acct-1","email":"maya@x.test"},{"account":"acct-2","email":"maya@x.test"}]);
        let saved = store
            .handle("preferences_save", None, json!({"request_id":"star","expected_revision":before["revision"],"preferences":{"starred_senders":starred}}))
            .unwrap();
        assert_eq!(
            store.preferences().unwrap()["preferences"]["starred_senders"],
            starred
        );
        for bad in [
            json!([{"account":"acct-1","email":"Maya@X.test"}]),
            json!([{"account":"acct-1","email":"maya@x.test"},{"account":"acct-1","email":"maya@x.test"}]),
            json!([{"account":"acct-1","email":"no-address"}]),
            json!([{"account":"acct-1"}]),
            json!("maya@x.test"),
        ] {
            assert!(store
                .handle("preferences_save", None, json!({"request_id":"bad-star","expected_revision":saved["revision"],"preferences":{"starred_senders":bad}}))
                .is_err());
        }
        assert_eq!(
            store.preferences().unwrap()["preferences"]["starred_senders"],
            starred
        );
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
        assert_eq!(
            store.preferences().unwrap()["preferences"]["review_leave_seconds"],
            10
        );
        assert_eq!(
            store.preferences().unwrap()["preferences"]["motion"],
            "system"
        );
        let command = json!({"preferences":{"theme":"paper","text_size":"large","motion":"reduced","startup_view":"last",
            "last_workspace":"calendar","timeline_position":"header","default_event_duration_minutes":45,"review_leave_seconds":0},
            "expected_revision":0,"request_id":"appearance"});
        let saved = store
            .handle("preferences_save", None, command.clone())
            .unwrap();
        for invalid in [
            json!({"theme":"sepia"}),
            json!({"text_size":15.5}),
            json!({"motion":"none"}),
            json!({"motion":false}),
            json!({"startup_view":"settings"}),
            json!({"last_workspace":"last"}),
            json!({"timeline_position":"top"}),
            json!({"default_event_duration_minutes":45.0}),
            json!({"default_event_duration_minutes":90}),
            json!({"sync_interval":5}),
            json!({"review_leave_seconds":7}),
            json!({"review_leave_seconds":"10"}),
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
