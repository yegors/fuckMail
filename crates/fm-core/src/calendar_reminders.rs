//! Running-client alerts use the same permitted occurrence projection as Calendar.
use crate::*;
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use rusqlite::params;

fn clock(p: &Value) -> Result<DateTime<Utc>> {
    match p.get("now") {
        None => Ok(Utc::now()),
        Some(Value::String(value)) => DateTime::parse_from_rfc3339(value)
            .map(|v| v.with_timezone(&Utc))
            .map_err(|_| err("invalid_request", "now must be an RFC3339 instant")),
        _ => Err(err("invalid_request", "now must be an RFC3339 instant")),
    }
}
fn permitted(event: &Value) -> bool {
    event["busy_only"] != true
        && event["cancelled"] != true
        && event["self_response"] != "declined"
        && !event["attendees"].as_array().is_some_and(|people| {
            people
                .iter()
                .any(|p| p["self"] == true && p["responseStatus"] == "declined")
        })
}
fn bounded<'a>(p: &'a Value, key: &str, max: usize) -> Result<&'a str> {
    p[key]
        .as_str()
        .ok_or_else(|| err("invalid_request", "Missing string parameter"))
        .and_then(|v| {
            if v.is_empty() || v.len() > max || v.chars().any(char::is_control) {
                Err(err("invalid_request", "Invalid reminder identity"))
            } else {
                Ok(v)
            }
        })
}
fn start_instant(event: &Value, window: &fm_calendar::Window) -> Result<Option<DateTime<Utc>>> {
    let start = string(event, "start")?;
    if event["all_day"] == true {
        let day = NaiveDate::parse_from_str(start, "%Y-%m-%d")
            .map_err(|_| err("storage_error", "Invalid all-day reminder date"))?;
        Ok(window
            .calendar_zone
            .from_local_datetime(&day.and_hms_opt(9, 0, 0).unwrap())
            .earliest()
            .map(|v| v.with_timezone(&Utc)))
    } else {
        Ok(Some(
            DateTime::parse_from_rfc3339(start)
                .map_err(|_| err("storage_error", "Invalid reminder start"))?
                .with_timezone(&Utc),
        ))
    }
}
fn due_now(
    event: &Value,
    now: DateTime<Utc>,
    window: &fm_calendar::Window,
    default_lead: i64,
) -> Result<Option<DateTime<Utc>>> {
    let Some(due) = start_instant(event, window)? else {
        return Ok(None);
    };
    let distance = due - now;
    // Null is an explicit "no reminder" (Google useDefault:false without
    // overrides, or the editor's None). Only an absent value uses the default.
    let lead = match event.get("reminder_minutes") {
        Some(Value::Null) => return Ok(None),
        Some(value) => value
            .as_i64()
            .filter(|v| (0..=40320).contains(v))
            .unwrap_or(default_lead),
        None => default_lead,
    };
    Ok((Duration::seconds(-60)..=Duration::minutes(lead))
        .contains(&distance)
        .then_some(due))
}
fn default_lead(preferences: &Value) -> i64 {
    preferences["preferences"]["default_reminder_minutes"]
        .as_i64()
        .unwrap_or(15)
}
impl Store {
    fn default_reminder_minutes(&self) -> Result<i64> {
        Ok(default_lead(&self.preferences()?))
    }
    fn reminder_projection(&self, date: NaiveDate) -> Result<Value> {
        let from = date
            .pred_opt()
            .ok_or_else(|| err("invalid_request", "Date is out of range"))?;
        let to = date
            .checked_add_signed(Duration::days(29))
            .ok_or_else(|| err("invalid_request", "Date is out of range"))?;
        self.calendar_combined(&json!({"from":from.to_string(),"to":to.to_string()}))
    }
    pub(crate) fn calendar_reminders(&self, p: &Value) -> Result<Value> {
        fields(p, &["now"])?;
        let now = clock(p)?;
        let preferences = self.preferences()?;
        if preferences["preferences"]["notifications"] == "off" {
            return Ok(json!({"reminders":[],"coverage":null}));
        }
        let default_lead = default_lead(&preferences);
        let Some(zone) = self.calendar_display_zone()? else {
            return Ok(json!({"reminders":[],"coverage":{"state":"display_zone_required"}}));
        };
        let window = fm_calendar::Window::from_dates(
            now.date_naive(),
            now.date_naive()
                .succ_opt()
                .ok_or_else(|| err("invalid_request", "Date is out of range"))?,
            &zone,
        )
        .map_err(crate::calendar_store::calendar_error)?;
        let projection =
            self.reminder_projection(now.with_timezone(&window.calendar_zone).date_naive())?;
        let mut reminders = Vec::new();
        for event in projection["events"].as_array().unwrap() {
            if !permitted(event) {
                continue;
            }
            let start = string(event, "start")?;
            let receipt: Option<(String,i64)> = self.db.query_row(
                "SELECT state,recorded_at FROM calendar_reminder_receipts WHERE account=?1 AND calendar_id=?2 AND occurrence_id=?3 AND start=?4",
                params![string(event,"account")?,bounded(event,"calendar_id",1024)?,bounded(event,"id",8192)?,start], |r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            if receipt
                .as_ref()
                .is_some_and(|(state, _)| state == "dismissed")
            {
                continue;
            }
            let due = if let Some(due) = due_now(event, now, &window, default_lead)? {
                due
            } else if receipt
                .as_ref()
                .is_some_and(|(state, _)| state == "shown" || state == "snoozed")
            {
                // A reminder already presented while due survives its start. A new
                // client never alerts for an old unpresented event.
                let Some(due) = start_instant(event, &window)? else {
                    continue;
                };
                let expires = receipt.as_ref().and_then(|(state, at)| {
                    (state == "shown" && *at < 0).then(|| at.saturating_neg().saturating_add(3600))
                });
                if expires.map_or(now > due + Duration::hours(1), |until| {
                    now.timestamp() > until
                }) {
                    continue;
                }
                due
            } else {
                continue;
            };
            // A negative recorded_at on a shown receipt stores the absolute
            // snooze deadline without changing the schema-29 state constraint.
            if receipt.as_ref().is_some_and(|(state, until)| {
                state == "shown" && *until < 0 && now.timestamp() < until.saturating_neg()
            }) {
                continue;
            }
            let mut reminder = event.clone();
            reminder["reminder_state"] = receipt
                .and_then(|(state, until)| {
                    if state == "shown" && until < 0 {
                        None
                    } else {
                        Some(Value::String(state))
                    }
                })
                .unwrap_or(Value::Null);
            reminder["timing"] = if due <= now { "Now" } else { "Soon" }.into();
            reminder["due_at"] = due.to_rfc3339().into();
            reminders.push(reminder);
        }
        Ok(json!({"reminders":reminders,"coverage":projection["coverage"],"now":now.to_rfc3339()}))
    }
    pub(crate) fn calendar_reminder_ack(&self, account: Option<&str>, p: &Value) -> Result<Value> {
        fields(
            p,
            &["calendar_id", "occurrence_id", "start", "state", "now"],
        )?;
        let account = id(account.ok_or_else(|| err("account_required", "Choose an account"))?)?;
        if self.account_lifecycle(account)?["state"] != "active" {
            return Err(err("account_removed", "This account is archived"));
        }
        let calendar = bounded(p, "calendar_id", 1024)?;
        let occurrence = bounded(p, "occurrence_id", 8192)?;
        let start = bounded(p, "start", 64)?;
        let state = string(p, "state")?;
        if !matches!(state, "shown" | "dismissed" | "snoozed") {
            return Err(err("invalid_request", "Unknown reminder state"));
        }
        let now = clock(p)?;
        let date = NaiveDate::parse_from_str(start.get(..10).unwrap_or(""), "%Y-%m-%d")
            .map_err(|_| err("invalid_request", "Invalid reminder start"))?;
        let projection = self.reminder_projection(date)?;
        let event = projection["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| {
                event["account"] == account
                    && event["calendar_id"] == calendar
                    && event["id"] == occurrence
                    && event["start"] == start
                    && permitted(event)
            })
            .ok_or_else(|| {
                err(
                    "stale_reminder",
                    "This occurrence changed or is no longer permitted",
                )
            })?;
        if state == "snoozed" {
            let previous:Option<(String,i64)>=self.db.query_row("SELECT state,recorded_at FROM calendar_reminder_receipts WHERE account=?1 AND calendar_id=?2 AND occurrence_id=?3 AND start=?4",params![account,calendar,occurrence,start],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            if !previous
                .as_ref()
                .is_some_and(|(state, at)| state == "shown" && *at >= 0)
            {
                return Err(err(
                    "stale_reminder",
                    "Show this reminder before snoozing it",
                ));
            }
        }
        if state == "shown" {
            let zone = self
                .calendar_display_zone()?
                .ok_or_else(|| err("stale_reminder", "Choose a Calendar display time zone"))?;
            let window = fm_calendar::Window::from_dates(
                date,
                date.succ_opt()
                    .ok_or_else(|| err("invalid_request", "Date is out of range"))?,
                &zone,
            )
            .map_err(crate::calendar_store::calendar_error)?;
            let receipt: Option<(String,i64)> = self.db.query_row("SELECT state,recorded_at FROM calendar_reminder_receipts WHERE account=?1 AND calendar_id=?2 AND occurrence_id=?3 AND start=?4",params![account,calendar,occurrence,start],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            let resumed = receipt.as_ref().is_some_and(|(state, until)| {
                state == "shown" && *until < 0 && now.timestamp() >= -*until
            });
            if self.preferences()?["preferences"]["notifications"] == "off"
                || (!resumed
                    && due_now(event, now, &window, self.default_reminder_minutes()?)?.is_none())
            {
                return Err(err(
                    "stale_reminder",
                    "This occurrence is not currently due for an alert",
                ));
            }
        }
        let recorded_at = if state == "snoozed" {
            -(now.timestamp()
                + self.preferences()?["preferences"]["reminder_snooze_minutes"]
                    .as_i64()
                    .unwrap_or(5)
                    * 60)
        } else {
            now.timestamp()
        };
        let stored_state = if state == "snoozed" { "shown" } else { state };
        self.db.execute("INSERT INTO calendar_reminder_receipts(account,calendar_id,occurrence_id,start,state,recorded_at) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(account,calendar_id,occurrence_id,start) DO UPDATE SET state=CASE WHEN calendar_reminder_receipts.state='dismissed' THEN 'dismissed' ELSE excluded.state END,recorded_at=excluded.recorded_at",
            params![account,calendar,occurrence,start,stored_state,recorded_at])?;
        let actual: String = self.db.query_row("SELECT state FROM calendar_reminder_receipts WHERE account=?1 AND calendar_id=?2 AND occurrence_id=?3 AND start=?4",params![account,calendar,occurrence,start],|r|r.get(0))?;
        Ok(
            json!({"account":account,"calendar_id":calendar,"occurrence_id":occurrence,"start":start,"state":if state == "snoozed" && actual == "shown" { "snoozed" } else { actual.as_str() },"event":event,"remote_data_changed":false}),
        )
    }
    pub(crate) fn mail_notifications(&self, p: &Value) -> Result<Value> {
        fields(p, &["after", "since", "now"])?;
        let max: i64 =
            self.db
                .query_row("SELECT COALESCE(MAX(rowid),0) FROM messages", [], |r| {
                    r.get(0)
                })?;
        let after = match p.get("after") {
            None | Some(Value::Null) => return Ok(json!({"after":max,"messages":[],"more":false})),
            Some(value) => value
                .as_i64()
                .filter(|v| *v >= 0)
                .ok_or_else(|| err("invalid_request", "Invalid notification cursor"))?,
        };
        let now = clock(p)?;
        let since = DateTime::parse_from_rfc3339(string(p, "since")?)
            .map_err(|_| err("invalid_request", "since must be an RFC3339 instant"))?
            .with_timezone(&Utc)
            .max(now - Duration::minutes(5));
        // Scan at most 1,000 newly acquired rows per query, including history
        // which must advance the cursor without creating notifications.
        let mut stmt = self.db.prepare("SELECT m.rowid,m.account,m.id,m.conversation_id,m.sender_name,m.subject,m.sent_at,m.unread,m.inbox,m.sent,m.trashed,a.lifecycle_state FROM messages m JOIN accounts a ON a.id=m.account WHERE m.rowid>?1 ORDER BY m.rowid LIMIT 1000")?;
        let mut rows = stmt.query([after])?;
        let mut next = after;
        let mut messages = Vec::new();
        let enabled = self.preferences()?["preferences"]["notifications"] == "all";
        while let Some(row) = rows.next()? {
            next = row.get(0)?;
            if !enabled
                || !row.get::<_, bool>(7)?
                || !row.get::<_, bool>(8)?
                || row.get::<_, bool>(9)?
                || row.get::<_, bool>(10)?
                || row.get::<_, String>(11)? != "active"
            {
                continue;
            }
            let at: String = row.get(6)?;
            let Ok(at) = DateTime::parse_from_rfc3339(&at) else {
                continue;
            };
            if at.with_timezone(&Utc) < since || at.with_timezone(&Utc) > now + Duration::minutes(1)
            {
                continue;
            }
            messages.push(json!({"account":row.get::<_,String>(1)?,"id":row.get::<_,String>(2)?,"conversation_id":row.get::<_,String>(3)?,"sender":row.get::<_,String>(4)?,"subject":row.get::<_,String>(5)?}));
        }
        let total = messages.len();
        messages.truncate(20);
        Ok(json!({"after":next,"messages":messages,"total":total,"more":next<max}))
    }
}

#[cfg(test)]
#[path = "calendar_reminders_tests.rs"]
mod tests;
