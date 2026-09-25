//! Read-only link from an invitation email to its cached Calendar event. No
//! provider calls and no writes: the writer selects bounded inputs, a
//! preparation worker reads and parses the retained blobs, and the event is
//! the combined Calendar's own projection row.
use crate::{content::BlobStore, err, fields, id, string, Result, Store};
use chrono::{DateTime, Days, NaiveDate, TimeDelta, Utc};
use fm_calendar::{Access, Event, EventTime, Tz, Window};
use fm_google::transport::CancellationToken;
use ics::{Invite, RecurrenceId, Times, When};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::collections::BTreeSet;

mod ics;

/// Earlier messages considered from each of the conversation and the sender.
const EARLIER_PER_SOURCE: i64 = 30;
/// Occurrences tried when a series invitation names no instance.
const SERIES_TRIES: usize = 4;
const CALENDAR_ATTACHMENT: &str =
    "(lower(a.mime_type) IN ('text/calendar','application/ics') OR lower(a.name) LIKE '%.ics')";

pub struct InvitePlan {
    blobs: BlobStore,
    account: String,
    message: String,
    version: String,
    /// Earlier calendar attachments (hash, size), most recently sent first.
    earlier: Vec<(String, u64)>,
}

pub struct PreparedInvite {
    account: String,
    message: String,
    version: String,
    invite: Option<Invite>,
    was: Option<Times>,
}

/// The first usable calendar part: text/calendar before an application/ics
/// or `.ics` attachment. `message_source` reports the same as `has_invite`.
pub(crate) fn message_calendar(message: &mail_parser::Message<'_>) -> Option<Invite> {
    use mail_parser::{MimeHeaders, PartType};
    let mut parts: Vec<_> = message
        .parts
        .iter()
        .filter_map(|part| {
            let (kind, subtype) = part
                .content_type()
                .map(|ct| {
                    (
                        ct.ctype().to_ascii_lowercase(),
                        ct.subtype().unwrap_or("").to_ascii_lowercase(),
                    )
                })
                .unwrap_or_default();
            let rank = match (kind.as_str(), subtype.as_str()) {
                ("text", "calendar") => 0,
                ("application", "ics") => 1,
                _ if part
                    .attachment_name()
                    .is_some_and(|name| name.to_ascii_lowercase().ends_with(".ics")) =>
                {
                    2
                }
                _ => return None,
            };
            Some((rank, part))
        })
        .collect();
    parts.sort_by_key(|(rank, _)| *rank);
    parts.into_iter().take(4).find_map(|(_, part)| {
        if part.contents().len() > ics::MAX_PART_BYTES {
            return None;
        }
        let text = match &part.body {
            PartType::Text(text) | PartType::Html(text) => text.clone(),
            PartType::Binary(bytes) | PartType::InlineBinary(bytes) => {
                String::from_utf8_lossy(bytes)
            }
            _ => return None,
        };
        ics::parse(
            &text,
            part.content_type().and_then(|ct| ct.attribute("method")),
        )
    })
}

impl Store {
    pub fn message_invite_begin(&self, account: &str, request: &Value) -> Result<InvitePlan> {
        fields(request, &["id", "source_version"])?;
        let mid = id(string(request, "id")?)?;
        let row: Option<(String, String, String, String)> = self.db.query_row(
            "SELECT m.source_hash,m.conversation_id,m.sender_email,m.sent_at FROM messages m JOIN accounts a ON a.id=m.account WHERE m.account=?1 AND m.id=?2",
            params![id(account)?, mid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).optional()?;
        let (version, conversation, sender, sent_at) =
            row.ok_or_else(|| err("not_found", "Message is outside this account"))?;
        if version != string(request, "source_version")? {
            return Err(err(
                "stale_source",
                "Message content changed. Reopen the message.",
            ));
        }
        // Earlier invitations sent before this one: the rest of this
        // conversation, then the sender's recent mail with a calendar file.
        let mut seen = BTreeSet::new();
        let mut earlier = Vec::new();
        for (sql, key) in [
            (format!("SELECT m.id,julianday(m.sent_at) FROM messages m WHERE m.account=?1 AND m.conversation_id=?2 AND m.id<>?3 AND julianday(m.sent_at)<julianday(?4) AND EXISTS(SELECT 1 FROM attachments a WHERE a.account=m.account AND a.message_id=m.id AND {CALENDAR_ATTACHMENT}) ORDER BY julianday(m.sent_at) DESC,m.id LIMIT ?5"), &conversation),
            (format!("SELECT m.id,julianday(m.sent_at) FROM messages m WHERE m.account=?1 AND m.sender_email=?2 COLLATE NOCASE AND m.id<>?3 AND julianday(m.sent_at)<julianday(?4) AND EXISTS(SELECT 1 FROM attachments a WHERE a.account=m.account AND a.message_id=m.id AND {CALENDAR_ATTACHMENT}) ORDER BY m.sent_at DESC,m.id LIMIT ?5"), &sender),
        ] {
            if key.is_empty() {
                continue;
            }
            let rows = self
                .db
                .prepare(&sql)?
                .query_map(params![account, key, mid, sent_at, EARLIER_PER_SOURCE], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            for (message, sent) in rows {
                if !seen.insert(message.clone()) {
                    continue;
                }
                let part: Option<(String, i64)> = self.db.query_row(
                    &format!("SELECT a.blob_hash,a.size FROM attachments a WHERE a.account=?1 AND a.message_id=?2 AND a.state='available' AND a.size BETWEEN 1 AND ?3 AND {CALENDAR_ATTACHMENT} ORDER BY lower(a.mime_type)='text/calendar' DESC,lower(a.mime_type)='application/ics' DESC,a.id LIMIT 1"),
                    params![account, message, ics::MAX_PART_BYTES as i64], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
                if let Some((hash, size)) = part {
                    earlier.push((sent, hash, size as u64));
                }
            }
        }
        earlier.sort_by(|a, b| b.0.total_cmp(&a.0));
        Ok(InvitePlan {
            blobs: self.google_blob_store(),
            account: account.into(),
            message: mid.into(),
            version,
            earlier: earlier
                .into_iter()
                .map(|(_, hash, size)| (hash, size))
                .collect(),
        })
    }

    pub fn message_invite_finish(&self, prepared: PreparedInvite) -> Result<Value> {
        let current: Option<String> = self
            .db
            .query_row(
                "SELECT source_hash FROM messages WHERE account=?1 AND id=?2",
                params![prepared.account, prepared.message],
                |r| r.get(0),
            )
            .optional()?;
        match current {
            None => return Err(err("not_found", "Message is outside this account")),
            Some(version) if version != prepared.version => {
                return Err(err(
                    "stale_source",
                    "Message content changed. Reopen the message.",
                ))
            }
            _ => {}
        }
        let Some(invite) = prepared.invite else {
            return Ok(json!({"invite":null}));
        };
        let target = self.invite_target(&prepared.account, &invite)?;
        let event = match &target {
            Some(target) => self.invite_projection(&prepared.account, target)?,
            None => None,
        };
        // Google's copy of the event wins over the time in this email.
        let times = event
            .as_ref()
            .and_then(row_times)
            .or_else(|| target.as_ref().and_then(|target| target.stored))
            .or(invite.times);
        let was = prepared.was;
        Ok(json!({"invite":{
            "uid":invite.uid,"method":invite.method,"sequence":invite.sequence,
            "recurrence_id":match invite.recurrence {RecurrenceId::Known(at)=>json!(at.public()),_=>Value::Null},
            "summary":invite.summary,
            "start":times.map(|t|t.start.public()),"end":times.map(|t|t.end.public()),
            "all_day":times.is_some_and(|t|t.start.all_day()),
            "was_start":was.map(|t|t.start.public()),"was_end":was.map(|t|t.end.public()),
            "was_all_day":was.map(|t|t.start.all_day()),
            "event":event}}))
    }

    /// The linked occurrence: same-account events whose iCalUID is the
    /// invitation's UID. A calendar where the account is a guest wins, then
    /// the primary calendar.
    fn invite_target(&self, account: &str, invite: &Invite) -> Result<Option<Target>> {
        if invite.recurrence == RecurrenceId::Unknown {
            return Ok(None);
        }
        // instr() on the JSON-encoded UID keeps the scan cheap before json_extract.
        let needle = serde_json::to_string(&invite.uid)?;
        let rows = self.db.prepare("SELECT v.calendar_id,v.json,i.series_id IS NOT NULL,c.time_zone,v.calendar_id='primary' OR COALESCE((SELECT json_extract(k.json,'$.primary') FROM calendar_catalog k WHERE k.account=v.account AND k.calendar_id=v.calendar_id),0)=1 FROM calendar_visible_sources v JOIN calendar_event_index i ON i.account=v.account AND i.calendar_id=v.calendar_id AND i.event_id=v.event_id AND i.slot=v.slot JOIN calendar_sync_state c ON c.account=v.account AND c.calendar_id=v.calendar_id WHERE v.account=?1 AND i.cancelled=0 AND instr(v.json,?2)>0 AND json_extract(v.json,'$.iCalUID')=?3 ORDER BY i.series_id IS NOT NULL,v.calendar_id,v.event_id LIMIT 64")?
            .query_map(params![account, needle, invite.uid], |r| {
                Ok(Linked {
                    calendar: r.get(0)?,
                    source: r.get(1)?,
                    instance: r.get(2)?,
                    zone: r.get(3)?,
                    primary: r.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let rows: Vec<(Linked, Value)> = rows
            .into_iter()
            .filter_map(|row| {
                let source = serde_json::from_str(&row.source).ok()?;
                Some((row, source))
            })
            .collect();
        let calendars: BTreeSet<&str> = rows.iter().map(|(row, _)| row.calendar.as_str()).collect();
        let Some(calendar) = calendars.into_iter().min_by_key(|calendar| {
            let mut own = false;
            let mut primary = false;
            let mut anchored = false;
            for (row, source) in rows.iter().filter(|(row, _)| row.calendar == *calendar) {
                own |= source["attendees"]
                    .as_array()
                    .is_some_and(|people| people.iter().any(|p| p["self"] == true));
                primary |= row.primary;
                anchored |= !row.instance;
            }
            (!own, !primary, !anchored, *calendar)
        }) else {
            return Ok(None);
        };
        let mut events = Vec::new();
        let mut zone = None;
        for (row, source) in rows.iter().filter(|(row, _)| row.calendar == calendar) {
            zone = zone.or_else(|| row.zone.parse::<Tz>().ok());
            if let Ok(event) = fm_calendar::normalize(account, calendar, &row.zone, source) {
                events.push(event);
            }
        }
        let Some(zone) = zone else {
            return Ok(None);
        };
        let now = self.invite_now()?;
        if let Some(anchor) = events.iter().find(|event| event.series_id.is_none()) {
            if anchor.recurrence.is_empty() {
                let (Some(start), Some(end)) = (&anchor.start, &anchor.end) else {
                    return Ok(None);
                };
                let stored = Times {
                    start: event_when(start),
                    end: event_when(end),
                };
                return Ok(Some(Target {
                    calendar: calendar.into(),
                    series: anchor.key.event.clone(),
                    original: None,
                    anchor: stored.start,
                    stored: Some(stored),
                }));
            }
            let originals = match invite.recurrence {
                RecurrenceId::Known(at) => vec![at],
                _ => series_starts(anchor, zone, now),
            };
            for original in originals {
                if let Some(target) = self.occurrence_target(account, calendar, anchor, original)? {
                    return Ok(Some(target));
                }
            }
            return Ok(None);
        }
        // Invited to single instances only: their series is not in this calendar.
        let mut instances: Vec<&Event> = events.iter().filter(|e| e.series_id.is_some()).collect();
        instances.sort_by_key(|e| e.start.as_ref().and_then(|start| start.utc(zone).ok()));
        let chosen = match invite.recurrence {
            RecurrenceId::Known(at) => instances
                .iter()
                .find(|e| e.original_start.as_ref().map(event_when) == Some(at)),
            _ => instances
                .iter()
                .find(|e| {
                    e.start
                        .as_ref()
                        .and_then(|start| start.utc(zone).ok())
                        .is_some_and(|at| at >= now)
                })
                .or(instances.first()),
        };
        Ok(chosen.and_then(|event| {
            let stored = Times {
                start: event_when(event.start.as_ref()?),
                end: event_when(event.end.as_ref()?),
            };
            Some(Target {
                calendar: calendar.into(),
                series: event.series_id.clone()?,
                original: Some(event_when(event.original_start.as_ref()?)),
                anchor: stored.start,
                stored: Some(stored),
            })
        }))
    }

    /// One occurrence of a stored series. Its exception, if any, supplies the
    /// displayed time; a cancelled occurrence has no event.
    fn occurrence_target(
        &self,
        account: &str,
        calendar: &str,
        master: &Event,
        original: When,
    ) -> Result<Option<Target>> {
        let (ms, date) = match original {
            When::Instant(at) => (Some(at.timestamp_millis()), None),
            When::Date(date) => (None, Some(date.to_string())),
        };
        let exception: Option<(bool, String)> = self.db.query_row("SELECT i.cancelled,p.json FROM calendar_event_index i INDEXED BY calendar_index_series JOIN calendar_published_sources p ON p.account=i.account AND p.calendar_id=i.calendar_id AND p.event_id=i.event_id AND p.slot=i.slot AND p.generation=i.generation WHERE i.account=?1 AND i.calendar_id=?2 AND i.series_id=?3 AND ((p.visible=1 AND p.seen_full_generation>=p.full_generation) OR i.cancelled=1) AND (i.original_ms=?4 OR i.original_date=?5) ORDER BY i.cancelled DESC LIMIT 1",
            params![account, calendar, master.key.event, ms, date], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        let stored = match exception {
            Some((true, _)) => return Ok(None),
            Some((false, source)) => {
                let Some(stored) = serde_json::from_str::<Value>(&source)
                    .ok()
                    .and_then(|source| {
                        Some(Times {
                            start: google_when(&source["start"])?,
                            end: google_when(&source["end"])?,
                        })
                    })
                else {
                    return Ok(None);
                };
                Some(stored)
            }
            None => None,
        };
        Ok(Some(Target {
            calendar: calendar.into(),
            series: master.key.event.clone(),
            original: Some(original),
            anchor: stored.map_or(original, |stored| stored.start),
            stored,
        }))
    }

    /// The combined Calendar's own row for the target, from a short window
    /// around its displayed date. Busy-only rows carry no invitation details.
    fn invite_projection(&self, account: &str, target: &Target) -> Result<Option<Value>> {
        let Some(zone) = self
            .calendar_display_zone()?
            .and_then(|zone| zone.parse::<Tz>().ok())
        else {
            return Ok(None);
        };
        let day = match target.anchor {
            When::Date(date) => date,
            When::Instant(at) => at.with_timezone(&zone).date_naive(),
        };
        let (Some(from), Some(to)) = (
            day.checked_sub_days(Days::new(1)),
            day.checked_add_days(Days::new(2)),
        ) else {
            return Ok(None);
        };
        let request = json!({"from":from.to_string(),"to":to.to_string(),
            "calendars":[{"account":account,"calendar_id":target.calendar}]});
        let combined = match self.calendar_combined_for(&request, Some(account)) {
            Ok(combined) => combined,
            Err(error) if error.code == "storage_error" => return Err(error),
            Err(_) => return Ok(None),
        };
        let original = target.original.map(|original| original.identity());
        Ok(combined["events"].as_array().and_then(|rows| {
            rows.iter()
                .find(|row| {
                    row["busy_only"] != true
                        && row["calendar_id"] == target.calendar.as_str()
                        && row["occurrence_key"]["series"] == target.series.as_str()
                        && row["occurrence_key"]["original_start"].as_str() == original.as_deref()
                })
                .cloned()
        }))
    }

    fn invite_now(&self) -> Result<DateTime<Utc>> {
        if self.profile_kind()? == crate::live_profile::ProfileKind::Synthetic {
            // Demo profiles show a fixed today.
            return format!("{}T00:00:00Z", crate::FIXTURE_TODAY)
                .parse()
                .map_err(|_| err("storage_error", "Invalid fixture date"));
        }
        Ok(Utc::now())
    }
}

/// Read and parse off the writer: this message's retained source, then at
/// most the bounded earlier calendar attachments.
pub fn prepare(plan: InvitePlan, cancel: &CancellationToken) -> Result<PreparedInvite> {
    let stopped = || err("cancelled", "Message read cancelled");
    if cancel.is_cancelled() {
        return Err(stopped());
    }
    let bytes = crate::local_content::read_source(&plan.blobs, &plan.version)?;
    let invite = mail_parser::MessageParser::default()
        .parse(&bytes)
        .and_then(|message| message_calendar(&message));
    let mut was = None;
    // The previous time of an updated invitation: the most recent earlier
    // request for the same occurrence whose start differs from this one.
    if let Some(invite) = invite
        .as_ref()
        .filter(|invite| invite.method == "REQUEST" && invite.recurrence != RecurrenceId::Unknown)
    {
        if let Some(current) = invite.times {
            for (hash, size) in &plan.earlier {
                if cancel.is_cancelled() {
                    return Err(stopped());
                }
                // A missing or changed attachment only loses the earlier time.
                let Ok(bytes) = crate::local_content::read_blob(&plan.blobs, hash, *size) else {
                    continue;
                };
                let Some(earlier) = ics::parse(&String::from_utf8_lossy(&bytes), None) else {
                    continue;
                };
                if earlier.uid != invite.uid
                    || earlier.recurrence != invite.recurrence
                    || !matches!(earlier.method.as_str(), "REQUEST" | "")
                {
                    continue;
                }
                if let Some(times) = earlier.times.filter(|t| t.start != current.start) {
                    was = Some(times);
                    break;
                }
            }
        }
    }
    if cancel.is_cancelled() {
        return Err(stopped());
    }
    Ok(PreparedInvite {
        account: plan.account,
        message: plan.message,
        version: plan.version,
        invite,
        was,
    })
}

struct Linked {
    calendar: String,
    source: String,
    instance: bool,
    zone: String,
    primary: bool,
}

struct Target {
    calendar: String,
    /// The projection's occurrence series: the event itself when single.
    series: String,
    /// The occurrence's original start; None for a single event.
    original: Option<When>,
    /// Where the occurrence is displayed, to choose the projection window.
    anchor: When,
    /// A stored event's own times; None for a computed series occurrence.
    stored: Option<Times>,
}

/// Original starts to try for a series invitation that names no instance:
/// the next occurrences at or after now, else the first ones.
fn series_starts(master: &Event, zone: Tz, now: DateTime<Utc>) -> Vec<When> {
    let step = TimeDelta::days(fm_calendar::MAX_WINDOW_DAYS);
    let occurrences = |from: DateTime<Utc>| -> Vec<EventTime> {
        from.checked_add_signed(step)
            .and_then(|to| Window::new(from, to, zone).ok())
            .and_then(|window| fm_calendar::expand(master, &[], window, Access::Owner).ok())
            .map(|found| {
                found
                    .into_iter()
                    .map(|occurrence| occurrence.start)
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut starts = Vec::new();
    let mut seen = BTreeSet::new();
    let mut from = now;
    // About a year ahead, in the widest windows the projection accepts.
    for _ in 0..9 {
        for start in occurrences(from) {
            if start.utc(zone).is_ok_and(|at| at >= now) && seen.insert(start.identity()) {
                starts.push(event_when(&start));
            }
        }
        match from.checked_add_signed(step) {
            Some(next) if starts.len() < SERIES_TRIES => from = next,
            _ => break,
        }
    }
    if starts.is_empty() {
        if let Some(first) = master
            .start
            .as_ref()
            .and_then(|start| start.utc(zone).ok())
            .and_then(|at| at.checked_sub_signed(TimeDelta::days(1)))
        {
            starts = occurrences(first).iter().map(event_when).collect();
        }
    }
    starts.truncate(SERIES_TRIES);
    starts
}

fn event_when(time: &EventTime) -> When {
    match time {
        EventTime::Date { date } => When::Date(*date),
        EventTime::Zoned { instant, .. } => When::Instant(instant.with_timezone(&Utc)),
    }
}

fn google_when(value: &Value) -> Option<When> {
    if let Some(date) = value["date"].as_str() {
        return NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .ok()
            .map(When::Date);
    }
    DateTime::parse_from_rfc3339(value["dateTime"].as_str()?)
        .ok()
        .map(|at| When::Instant(at.with_timezone(&Utc)))
}

fn row_times(row: &Value) -> Option<Times> {
    let all_day = row["all_day"] == true;
    let parse = |key: &str| -> Option<When> {
        let text = row[key].as_str()?;
        if all_day {
            NaiveDate::parse_from_str(text, "%Y-%m-%d")
                .ok()
                .map(When::Date)
        } else {
            DateTime::parse_from_rfc3339(text)
                .ok()
                .map(|at| When::Instant(at.with_timezone(&Utc)))
        }
    };
    Some(Times {
        start: parse("start")?,
        end: parse("end")?,
    })
}

#[cfg(test)]
#[path = "message_invite_tests.rs"]
mod tests;
