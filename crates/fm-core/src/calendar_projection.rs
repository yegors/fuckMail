//! Bounded indexed source selection shared by combined Calendar and the legacy query.
use crate::calendar_store::calendar_error;
use crate::{err, Result, Store};
use fm_calendar::{Access, Event, Window};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Maximum retained sources loaded for one calendar projection.
const SOURCE_BUDGET: usize = 2048;
/// Maximum bytes of retained source JSON loaded for one projection.
const SOURCE_BYTES: usize = 8 * 1024 * 1024;
/// Maximum recurring series expanded for one projection.
const SERIES_BUDGET: usize = 128;
/// Projected rows for one calendar. `incomplete` is set whenever a budget
/// cut something off or a source could not be used, so callers report
/// partial coverage instead of showing free time.
pub(crate) struct CalendarProjection {
    pub events: Vec<Value>,
    pub incomplete: bool,
    /// Distinct sources loaded, counted against the combined view's budget.
    pub candidates: usize,
}
impl Store {
    /// Returns the user's chosen Calendar display zone, if one is set.
    pub(crate) fn calendar_display_zone(&self) -> Result<Option<String>> {
        Ok(
            self.preferences()?["preferences"]["calendar_display_time_zone"]
                .as_str()
                .map(str::to_owned),
        )
    }
    /// Projects one calendar's visible sources into `window` using the event
    /// index, with bounded work.
    ///
    /// Candidates are single events overlapping the window plus recurring
    /// masters that start before its end; each master's exceptions and
    /// cancellations are then loaded so expansion is correct. Rows are
    /// filtered by `access` (free/busy calendars show busy-only rows). Live
    /// rows also carry action flags (edit, delete, RSVP, attendees, recurring
    /// scopes), which are only offered once the calendar has a clean full
    /// sync, and archived accounts are always read-only.
    pub(crate) fn calendar_indexed_projection(
        &self,
        account: &str,
        calendar: &str,
        zone: &str,
        access: Access,
        window: Window,
    ) -> Result<CalendarProjection> {
        if access == Access::None {
            return Ok(CalendarProjection {
                events: Vec::new(),
                incomplete: false,
                candidates: 0,
            });
        }
        let archived = self.account_lifecycle(account)?["state"] == "removed";
        let live = self.profile_kind()? == crate::live_profile::ProfileKind::Live;
        let (actor, write_ready) = if live {
            self.db.query_row("SELECT a.email,c.full_generation>0 AND c.retained_errors=0 FROM calendar_sync_state c JOIN accounts a ON a.id=c.account WHERE c.account=?1 AND c.calendar_id=?2",params![account,calendar],|row|Ok((row.get::<_,String>(0)?,row.get::<_,bool>(1)?)))?
        } else {
            (String::new(), false)
        };
        let from = window.from.timestamp_millis();
        let to = window.to.timestamp_millis();
        let from_date = window
            .from
            .with_timezone(&window.calendar_zone)
            .date_naive()
            .to_string();
        let to_date = window
            .to
            .with_timezone(&window.calendar_zone)
            .date_naive()
            .to_string();
        let mut ids = BTreeSet::new();
        let mut master_order = Vec::new();
        let mut incomplete = false;
        // A series whose single RRULE ended (UNTIL) before the window cannot
        // reach it, so it does not spend the series budget. Two days' margin
        // covers UNTIL's zone; series longer than a day are always kept.
        let ended_before = (window.from - chrono::Duration::days(2))
            .format("%Y%m%d")
            .to_string();
        // Three candidate queries: timed events by UTC milliseconds, all-day
        // events by calendar-zone date, and recurring masters. Each asks for one
        // row past its budget so an overflow is detected, not silently dropped.
        // Masters are ranked as in `series_rank`, then by first-seen order, so
        // a flood of unanswered invites with low event IDs (which senders may
        // choose) cannot push the user's own series out of the budget.
        for sql in [
            "SELECT i.event_id FROM calendar_event_index i INDEXED BY calendar_index_timed_end JOIN calendar_visible_sources v ON v.account=i.account AND v.calendar_id=i.calendar_id AND v.event_id=i.event_id AND v.slot=i.slot WHERE i.account=?1 AND i.calendar_id=?2 AND i.end_ms>?3 AND i.start_ms<?4 ORDER BY i.end_ms,i.event_id LIMIT 2049",
            "SELECT i.event_id FROM calendar_event_index i INDEXED BY calendar_index_dates_end JOIN calendar_visible_sources v ON v.account=i.account AND v.calendar_id=i.calendar_id AND v.event_id=i.event_id AND v.slot=i.slot WHERE i.account=?1 AND i.calendar_id=?2 AND i.end_date>?3 AND i.start_date<?4 ORDER BY i.end_date,i.event_id LIMIT 2049",
            "SELECT i.event_id FROM calendar_event_index i INDEXED BY calendar_index_masters JOIN calendar_visible_sources v ON v.account=i.account AND v.calendar_id=i.calendar_id AND v.event_id=i.event_id AND v.slot=i.slot WHERE i.account=?1 AND i.calendar_id=?2 AND i.recurring=1 AND i.series_id IS NULL AND COALESCE(i.start_ms<?3,i.start_date<?4,1) AND NOT (instr(json_extract(v.json,'$.recurrence'),'UNTIL=')>0 AND instr(json_extract(v.json,'$.recurrence'),'RDATE')=0 AND length(json_extract(v.json,'$.recurrence'))-length(replace(json_extract(v.json,'$.recurrence'),'RRULE:',''))=6 AND COALESCE(i.end_ms-i.start_ms,(julianday(i.end_date)-julianday(i.start_date))*86400000,86400001)<=86400000 AND substr(json_extract(v.json,'$.recurrence'),instr(json_extract(v.json,'$.recurrence'),'UNTIL=')+6,8)<?5) ORDER BY CASE WHEN json_extract(v.json,'$.organizer.self')=1 THEN 0 ELSE COALESCE((SELECT CASE json_extract(a.value,'$.responseStatus') WHEN 'accepted' THEN 0 WHEN 'tentative' THEN 0 WHEN 'declined' THEN 2 ELSE 1 END FROM json_each(v.json,'$.attendees') a WHERE json_extract(a.value,'$.self')=1 LIMIT 1),0) END,(SELECT s.rowid FROM calendar_sources s WHERE s.account=i.account AND s.calendar_id=i.calendar_id AND s.event_id=i.event_id),i.event_id LIMIT 129",
        ] {
            let mut statement=self.db.prepare(sql)?;
            let (third,fourth):(rusqlite::types::Value,rusqlite::types::Value)=if sql.contains("end_date") {(from_date.clone().into(),to_date.clone().into())}else{(from.into(),to.into())};
            let mut rows=if sql.contains("i.recurring=1") {statement.query(params![account,calendar,to,to_date,ended_before])?}else{statement.query(params![account,calendar,third,fourth])?};
            let mut count=0;
            while let Some(row)=rows.next()?{
                count+=1;
                let id=row.get::<_,String>(0)?;
                if sql.contains("i.recurring=1"){master_order.push(id.clone());}
                if ids.len()<SOURCE_BUDGET {ids.insert(id);}else{incomplete=true;}
            }
            if count>if sql.contains("i.recurring=1"){SERIES_BUDGET}else{SOURCE_BUDGET}{incomplete=true;}
        }
        let mut events = BTreeMap::<String, Event>::new();
        let mut bytes = 0;
        for id in &ids {
            self.load_calendar_candidate(
                account,
                calendar,
                id,
                zone,
                None,
                &mut events,
                &mut bytes,
                &mut incomplete,
            )?;
        }
        // A moved exception can enter from outside its parent's date range. Always
        // recover its parent before evaluating inherited privacy or cancellation.
        let mut blocked_series = BTreeSet::new();
        let parents = events
            .values()
            .filter_map(|v| v.series_id.clone())
            .collect::<BTreeSet<_>>();
        for parent in parents {
            // A current moved child must not outlive a known cancelled parent
            // merely because that parent is hidden from the ordinary view.
            let cancelled: bool = self.db.query_row("SELECT EXISTS(SELECT 1 FROM calendar_event_index i JOIN calendar_published_sources p ON p.account=i.account AND p.calendar_id=i.calendar_id AND p.event_id=i.event_id AND p.slot=i.slot AND p.generation=i.generation WHERE i.account=?1 AND i.calendar_id=?2 AND i.event_id=?3 AND i.series_id IS NULL AND i.cancelled=1)",params![account,calendar,parent],|r|r.get(0))?;
            if cancelled {
                blocked_series.insert(parent);
                continue;
            }
            self.load_calendar_candidate(
                account,
                calendar,
                &parent,
                zone,
                None,
                &mut events,
                &mut bytes,
                &mut incomplete,
            )?;
        }
        // The series budget goes to the best-ranked series, in the masters
        // query's order; parents recovered through a moved exception follow
        // within their rank. Stable sorting preserves event-id order for ties.
        let priority = |event: &Event| {
            let position = master_order
                .iter()
                .position(|id| *id == event.key.event)
                .unwrap_or(usize::MAX);
            (series_rank(&event.source), position)
        };
        let mut ranked = events
            .values()
            .filter(|v| !v.recurrence.is_empty())
            .collect::<Vec<_>>();
        ranked.sort_by_cached_key(|event| priority(event));
        if ranked.len() > SERIES_BUDGET {
            incomplete = true;
        }
        let allowed_series = ranked
            .iter()
            .take(SERIES_BUDGET)
            .map(|v| v.key.event.clone())
            .collect::<BTreeSet<_>>();
        let masters = ranked
            .iter()
            .take(SERIES_BUDGET)
            .filter(|v| !v.cancelled && v.series_id.is_none())
            .map(|v| v.key.event.clone())
            .collect::<Vec<_>>();
        for master in &masters {
            let remaining = SOURCE_BUDGET.saturating_sub(events.len());
            // The parent came from the strict current view. An old cancellation
            // can suppress its occurrence, but cannot restore an absent parent.
            // Select the effective published slot before considering cancellation.
            let mut statement=self.db.prepare("SELECT i.event_id FROM calendar_event_index i INDEXED BY calendar_index_series JOIN calendar_published_sources p ON p.account=i.account AND p.calendar_id=i.calendar_id AND p.event_id=i.event_id AND p.slot=i.slot AND p.generation=i.generation WHERE i.account=?1 AND i.calendar_id=?2 AND i.series_id=?3 AND ((p.visible=1 AND p.seen_full_generation>=p.full_generation) OR i.cancelled=1) ORDER BY i.event_id LIMIT ?4")?;
            let child_ids = statement
                .query_map(
                    params![account, calendar, master, (remaining + 1).min(1001) as i64],
                    |r| r.get::<_, String>(0),
                )?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            // A series is shown whole or not at all: expanding it without all of
            // its exceptions would show moved or cancelled occurrences wrongly.
            if child_ids.len() > remaining || child_ids.len() > 1000 {
                incomplete = true;
                blocked_series.insert(master.clone());
                continue;
            }
            for id in child_ids {
                if !self.load_calendar_candidate(
                    account,
                    calendar,
                    &id,
                    zone,
                    Some(master),
                    &mut events,
                    &mut bytes,
                    &mut incomplete,
                )? {
                    blocked_series.insert(master.clone());
                }
            }
        }
        let mut children = BTreeMap::<String, Vec<Event>>::new();
        for event in events.values() {
            if blocked_series.contains(&event.key.event)
                || event
                    .series_id
                    .as_ref()
                    .is_some_and(|id| blocked_series.contains(id))
            {
                continue;
            }
            if let Some(parent) = &event.series_id {
                children
                    .entry(parent.clone())
                    .or_default()
                    .push(event.clone());
            }
        }
        let mut output = Vec::new();
        // Keep the same priority at the occurrence cap, including single
        // events. Otherwise low-id invitations can still fill the view first.
        let mut ordered = events.values().collect::<Vec<_>>();
        ordered.sort_by_cached_key(|event| priority(event));
        for event in ordered {
            if blocked_series.contains(&event.key.event)
                || event
                    .series_id
                    .as_ref()
                    .is_some_and(|id| blocked_series.contains(id))
            {
                continue;
            }
            // Exceptions whose master is loaded are projected through the
            // master's expansion; only orphaned instances project on their own.
            if event
                .series_id
                .as_ref()
                .is_some_and(|id| events.contains_key(id))
            {
                continue;
            }
            if !event.recurrence.is_empty() && !allowed_series.contains(&event.key.event) {
                continue;
            }
            match fm_calendar::project_event_in_window(
                event,
                children
                    .get(&event.key.event)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                zone,
                window,
                access,
            ) {
                Ok(rows) => {
                    for mut row in rows {
                        // The stored exception for this occurrence, if any,
                        // supplies its own organizer, options and attendees.
                        let occurrence_source = children
                            .get(&event.key.event)
                            .and_then(|items| {
                                items.iter().find(|child| {
                                    child.original_start.as_ref().is_some_and(|original| {
                                        row["occurrence_key"]["original_start"]
                                            == original.identity()
                                    })
                                })
                            })
                            .unwrap_or(event);
                        if row["busy_only"] != true {
                            row["event_id"] = event.key.event.clone().into();
                            row["etag"] = event.etag.clone().into();
                            if let Some(name) =
                                occurrence_source.source["organizer"]["displayName"].as_str()
                            {
                                row["organizer_name"] = name.into();
                            }
                        }
                        if row["busy_only"] != true {
                            crate::calendar_editor_options::project(
                                &mut row,
                                &occurrence_source.source,
                            );
                        }
                        if live {
                            let (edit, rsvp) = crate::live_calendar_writes::live_event_actions(
                                event, access, &actor,
                            );
                            let available = row["busy_only"] != true && write_ready;
                            let selector = crate::live_calendar_occurrences::projection_selector(
                                event,
                                occurrence_source,
                                &row,
                            );
                            row["can_resolve_occurrence"] =
                                (available && selector.is_some()).into();
                            if available {
                                if let Some(selector) = selector {
                                    row["occurrence_selector"] = selector;
                                }
                            }
                            if available && !event.recurrence.is_empty() {
                                if let Some(selector) =
                                    crate::live_calendar_occurrences::projection_selector_with_role(
                                        event,
                                        occurrence_source,
                                        &row,
                                        false,
                                    )
                                {
                                    let organizer = crate::live_calendar_writes::initial_event(
                                        event, access, "edit", 4,
                                    )
                                    .is_ok();
                                    let attendee = crate::live_calendar_writes::initial_event(
                                        event, access, "rsvp", 4,
                                    )
                                    .is_ok()
                                        && crate::live_calendar_writes::live_self_attendee(
                                            occurrence_source,
                                            &actor,
                                        )
                                        .is_ok();
                                    // "This and following" rewrites the rule, so it
                                    // needs a rule shape the split supports.
                                    let following = organizer
                                        && fm_calendar::recurring::validate_rule_shape(
                                            &event.recurrence,
                                            false,
                                        )
                                        .is_ok();
                                    row["recurring_selector"] = selector;
                                    row["recurring_scopes"] = json!({"edit":if organizer {if following{vec!["occurrence","series","following"]}else{vec!["occurrence","series"]}}else{vec![]},"delete":if organizer {if following{vec!["occurrence","series","following"]}else{vec!["occurrence","series"]}}else{vec![]},"rsvp":if attendee{vec!["occurrence","series"]}else{vec![]}});
                                }
                            }
                            row["can_edit"] = (available && edit).into();
                            row["can_delete"] = (available && edit).into();
                            row["can_rsvp"] = (available && rsvp).into();
                            row["can_edit_attendees"] = false.into();
                            row["attendee_edit_reason"] = "permission_denied".into();
                            row["guest_addresses"] = json!([]);
                            row["protected_attendees"] = json!([]);
                            if row["busy_only"] != true {
                                match crate::live_calendar_writes::live_guest_actions(
                                    occurrence_source,
                                    access,
                                ) {
                                    Ok((guests, protected)) => {
                                        row["can_edit_attendees"] = available.into();
                                        row["attendee_edit_reason"] = if available {
                                            Value::Null
                                        } else {
                                            "calendar_not_ready".into()
                                        };
                                        row["guest_addresses"] = guests.into();
                                        row["protected_attendees"] = protected.into();
                                    }
                                    Err(error) => row["attendee_edit_reason"] = error.code.into(),
                                }
                                if let Ok(person) = crate::live_calendar_writes::live_self_attendee(
                                    occurrence_source,
                                    &actor,
                                ) {
                                    row["self_response"] = person["responseStatus"].clone();
                                }
                            }
                        } else {
                            row["can_edit"] = (row["busy_only"] != true
                                && event.recurrence.is_empty()
                                && event.series_id.is_none()
                                && matches!(
                                    access.for_event(event),
                                    Access::Writer | Access::Owner
                                )
                                && !event.etag.is_empty())
                            .into();
                        }
                        if archived {
                            row["account_archived"] = json!(true);
                            row["read_only"] = json!(true);
                            for key in [
                                "can_edit",
                                "can_delete",
                                "can_rsvp",
                                "can_edit_attendees",
                                "can_resolve_occurrence",
                            ] {
                                row[key] = json!(false);
                            }
                            row["recurring_scopes"] = json!({"edit":[],"delete":[],"rsvp":[]});
                            row["attendee_edit_reason"] = json!("account_removed");
                        }
                        if output.len() < fm_calendar::MAX_OCCURRENCES as usize {
                            output.push(row);
                        } else {
                            incomplete = true;
                        }
                    }
                }
                Err(_) => incomplete = true,
            }
        }
        fm_calendar::sort_projected_events(&mut output, window).map_err(calendar_error)?;
        Ok(CalendarProjection {
            events: output,
            incomplete,
            candidates: events.len(),
        })
    }
    // These bounded accumulators share a single query budget across parent/child loads.
    /// Loads and normalizes one source into `events`, returning false (and
    /// marking the projection incomplete) when it is missing, over budget,
    /// invalid, or not a child of `master` when one is given. Children may be
    /// cancellation tombstones; top-level loads use only visible sources.
    #[allow(clippy::too_many_arguments)]
    fn load_calendar_candidate(
        &self,
        account: &str,
        calendar: &str,
        id: &str,
        zone: &str,
        master: Option<&str>,
        events: &mut BTreeMap<String, Event>,
        bytes: &mut usize,
        incomplete: &mut bool,
    ) -> Result<bool> {
        if let Some(event) = events.get(id) {
            let matches = master.is_none_or(|parent| event.series_id.as_deref() == Some(parent));
            *incomplete |= !matches;
            return Ok(matches);
        }
        if events.len() >= SOURCE_BUDGET || *bytes >= SOURCE_BYTES {
            *incomplete = true;
            return Ok(false);
        }
        let remaining = SOURCE_BYTES.saturating_sub(*bytes);
        let row: Option<(usize, Option<String>)> = if let Some(parent) = master {
            self.db.query_row("SELECT length(CAST(p.json AS BLOB)),CASE WHEN length(CAST(p.json AS BLOB))<=?5 THEN p.json END FROM calendar_published_sources p JOIN calendar_event_index i ON i.account=p.account AND i.calendar_id=p.calendar_id AND i.event_id=p.event_id AND i.slot=p.slot AND i.generation=p.generation WHERE p.account=?1 AND p.calendar_id=?2 AND p.event_id=?3 AND i.series_id=?4 AND ((p.visible=1 AND p.seen_full_generation>=p.full_generation) OR i.cancelled=1)", params![account,calendar,id,parent,remaining], |r| Ok((r.get(0)?,r.get(1)?))).optional()?
        } else {
            self.db.query_row("SELECT length(CAST(json AS BLOB)),CASE WHEN length(CAST(json AS BLOB))<=?4 THEN json END FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3", params![account,calendar,id,remaining], |r| Ok((r.get(0)?,r.get(1)?))).optional()?
        };
        let Some((size, raw)) = row else {
            *incomplete = true;
            return Ok(false);
        };
        // SQLite withheld a source larger than the remaining byte budget;
        // treat the budget as spent.
        let Some(raw) = raw else {
            *bytes = SOURCE_BYTES;
            *incomplete = true;
            return Ok(false);
        };
        *bytes += size;
        match serde_json::from_str::<Value>(&raw)
            .ok()
            .and_then(|value| fm_calendar::normalize(account, calendar, zone, &value).ok())
        {
            Some(event)
                if event.key.event == id
                    && master.is_none_or(|parent| event.series_id.as_deref() == Some(parent)) =>
            {
                events.insert(id.into(), event);
                Ok(true)
            }
            _ => {
                *incomplete = true;
                Ok(false)
            }
        }
    }
    /// Projects busy-only rows for a free/busy calendar from the newest stored
    /// snapshot that covers `window` under the current permission epoch.
    /// Returns `(rows, covered)`; no usable snapshot, a provider-unavailable
    /// one, or reaching the 1,000-row cap means not covered.
    pub(crate) fn calendar_busy_projection(
        &self,
        account: &str,
        calendar: &str,
        window: Window,
    ) -> Result<(Vec<Value>, bool)> {
        let row:Option<(String,bool,i64)>=self.db.query_row("SELECT b.json,b.available,b.observed_at FROM calendar_busy_snapshots b JOIN calendar_sync_state c USING(account,calendar_id) WHERE b.account=?1 AND b.calendar_id=?2 AND b.from_ms<=?3 AND b.to_ms>=?4 AND b.permission_epoch=c.permission_epoch ORDER BY b.observed_at DESC LIMIT 1",params![account,calendar,window.from.timestamp_millis(),window.to.timestamp_millis()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let Some((raw, true, observed_at)) = row else {
            return Ok((Vec::new(), false));
        };
        let source: Value = serde_json::from_str(&raw)?;
        let mut output = Vec::new();
        for interval in source["busy"]
            .as_array()
            .ok_or_else(|| err("storage_error", "Invalid free/busy snapshot"))?
        {
            let start =
                chrono::DateTime::parse_from_rfc3339(interval["start"].as_str().unwrap_or(""))
                    .map_err(|_| err("storage_error", "Invalid busy start"))?;
            let end = chrono::DateTime::parse_from_rfc3339(interval["end"].as_str().unwrap_or(""))
                .map_err(|_| err("storage_error", "Invalid busy end"))?;
            if start < window.to && end > window.from {
                output.push(json!({"id":format!("busy:{}",crate::calendar_store::fingerprint(&json!([account,calendar,start.to_rfc3339(),end.to_rfc3339()]))?),"kind":"free_busy","account":account,"calendar_id":calendar,"title":"Busy","start":start.to_rfc3339(),"end":end.to_rfc3339(),"all_day":false,"busy_only":true,"read_only":true,"can_edit":false,"can_delete":false,"can_rsvp":false,"observed_at":observed_at}));
            }
            if output.len() >= 1000 {
                return Ok((output, false));
            }
        }
        Ok((output, true))
    }
}

/// Orders recurring series for the series budget: 0 for series the user
/// organized, accepted or tentatively accepted, or that has no invitation for
/// the user; 1 for unanswered invitations; 2 for declined ones. Google sets
/// the `self` flags, so a sender cannot claim the user's rank. The masters
/// query computes the same rank in SQL.
fn series_rank(source: &Value) -> u8 {
    if source["organizer"]["self"] == true {
        return 0;
    }
    let response = source["attendees"]
        .as_array()
        .and_then(|attendees| attendees.iter().find(|a| a["self"] == true))
        .map(|attendee| attendee["responseStatus"].as_str().unwrap_or(""));
    match response {
        None | Some("accepted" | "tentative") => 0,
        Some("declined") => 2,
        Some(_) => 1,
    }
}
