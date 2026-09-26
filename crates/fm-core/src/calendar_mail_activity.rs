//! Daily counts over cached messages, shared by native Calendar and CLI call.
use crate::{err, fields, string, Result, Store};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use rusqlite::params;
use serde_json::{json, Value};

impl Store {
    /// Counts received and sent cached messages per display-zone day over a
    /// 1 to 42 day range, for one active account or all of them.
    ///
    /// Days come from Gmail's internal date when known, otherwise the header
    /// date (counted in `header_date_count`). Provider-deleted mail is
    /// excluded. Without a display zone it returns no days and
    /// `display_zone_required`.
    pub(crate) fn calendar_mail_activity(&self, account: Option<&str>, p: &Value) -> Result<Value> {
        fields(p, &["from", "to"])?;
        let date = |name| -> Result<NaiveDate> {
            let text = string(p, name)?;
            let parsed = NaiveDate::parse_from_str(text, "%Y-%m-%d")
                .map_err(|_| err("invalid_request", "Mail activity needs YYYY-MM-DD dates"))?;
            if parsed.to_string() != text {
                return Err(err(
                    "invalid_request",
                    "Mail activity needs YYYY-MM-DD dates",
                ));
            }
            Ok(parsed)
        };
        let from = date("from")?;
        let to = date("to")?;
        let days = (to - from).num_days();
        if !(1..=42).contains(&days) {
            return Err(err(
                "invalid_request",
                "Mail activity range must be 1–42 days",
            ));
        }
        let accounts = if let Some(id) = account {
            if self.account_lifecycle(id)?["state"] != "active" {
                return Err(err(
                    "account_removed",
                    "Mail activity excludes removed accounts",
                ));
            }
            vec![id.to_owned()]
        } else {
            let mut statement = self.db.prepare(
                "SELECT id FROM accounts WHERE lifecycle_state='active' ORDER BY id LIMIT 101",
            )?;
            let values = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if values.len() > 100 {
                return Err(err(
                    "invalid_request",
                    "Mail activity supports at most 100 accounts",
                ));
            }
            values
        };
        let Some(zone) = self.calendar_display_zone()? else {
            return Ok(
                json!({"from":from.to_string(),"to":to.to_string(),"account":account,"days":[],"state":"display_zone_required","time_zone":null}),
            );
        };
        let window = fm_calendar::Window::from_dates(from, to, &zone)
            .map_err(crate::calendar_store::calendar_error)?;
        let mut counts = vec![(0u64, 0u64); days as usize];
        let mut fallback_dates = 0u64;
        let mut count = |at: DateTime<Utc>, sent: bool, fallback: bool| {
            if at < window.from || at >= window.to {
                return;
            }
            let index = (at.with_timezone(&window.calendar_zone).date_naive() - from).num_days();
            if let Some(day) = counts.get_mut(index as usize) {
                if sent {
                    day.1 += 1;
                } else {
                    day.0 += 1;
                }
                fallback_dates += u64::from(fallback);
            }
        };
        // One range read per account, never a full-history query per day. Filter
        // small provider metadata before joining message identities; never read
        // bodies, recipients, retained MIME or draft/submission payloads.
        let mut provider = self.db.prepare_cached("SELECT c.internal_date_ms,m.sent FROM google_mail_coverage c INDEXED BY google_mail_coverage_by_date JOIN messages m ON m.account=c.account AND m.remote_id=c.remote_id WHERE c.account=?1 AND c.deleted=0 AND c.internal_date_ms>=?2 AND c.internal_date_ms<?3")?;
        // Header dates are normalized UTC in production. The padded indexed
        // range also admits valid offset dates in older/synthetic profiles;
        // parsed instants below determine the actual civil day.
        let lower = from
            .checked_sub_signed(Duration::days(2))
            .ok_or_else(|| err("invalid_request", "Mail activity date is out of range"))?
            .to_string();
        let upper = to
            .checked_add_signed(Duration::days(2))
            .ok_or_else(|| err("invalid_request", "Mail activity date is out of range"))?
            .to_string();
        let mut fallback = self.db.prepare_cached("SELECT m.sent_at,m.sent FROM messages m LEFT JOIN google_mail_coverage c ON c.account=m.account AND c.remote_id=m.remote_id WHERE m.account=?1 AND m.sent_at>=?2 AND m.sent_at<?3 AND c.internal_date_ms IS NULL AND COALESCE(c.deleted,0)=0")?;
        for id in &accounts {
            for row in provider.query_map(
                params![
                    id,
                    window.from.timestamp_millis(),
                    window.to.timestamp_millis()
                ],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, bool>(1)?)),
            )? {
                let (stamp, sent) = row?;
                if let Some(at) = DateTime::from_timestamp_millis(stamp) {
                    count(at, sent, false);
                }
            }
            for row in fallback.query_map(params![id, lower, upper], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?))
            })? {
                let (stamp, sent) = row?;
                if let Ok(at) = DateTime::parse_from_rfc3339(&stamp) {
                    count(at.with_timezone(&Utc), sent, true);
                }
            }
        }
        let rows: Vec<_> = counts.iter().enumerate().map(|(index, (received, sent))|
            json!({"date":(from + Duration::days(index as i64)).to_string(),"received":received,"sent":sent})).collect();
        Ok(
            json!({"from":from.to_string(),"to":to.to_string(),"account":account,"account_count":accounts.len(),"time_zone":zone,"days":rows,"state":"ready","cached_only":true,"header_date_count":fallback_dates,"revision":self.revision()?}),
        )
    }
}

#[cfg(test)]
#[path = "calendar_mail_activity_tests.rs"]
mod tests;
