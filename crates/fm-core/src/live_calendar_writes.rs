//! Durable human Calendar intent. Provider receipts and visible cache publication
//! are separate: a reconnect or concurrent read never erases a dispatched result.
use crate::{err, Result, Store};
use fm_calendar::{Access, Event, EventKey};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CalendarWriteLease {
    pub account: String,
    pub request_id: String,
    pub attempt: i64,
    pub connection_generation: i64,
    pub permission_epoch: i64,
    pub dispatch: Value,
}
#[derive(Clone, Debug)]
pub struct CalendarWriteOperation {
    pub state: String,
    pub version: i64,
    pub intent: Value,
    pub lease: Option<CalendarWriteLease>,
}
#[derive(Clone, Debug)]
pub enum CalendarWriteOutcome {
    Accepted(Value),
    Deleted,
    Conflict,
    Rejected(String),
    /// The quota transport proves that no provider request was dispatched.
    UndispatchedCooldown {
        retry_after_secs: u64,
    },
    Unknown,
}
type OperationRow = (
    String,
    i64,
    String,
    i64,
    Option<i64>,
    Option<i64>,
    Option<String>,
);

impl Store {
    pub fn calendar_live_prepare(&mut self, account: &str, request: &Value) -> Result<Value> {
        live(self, account)?;
        if serde_json::to_vec(request)?.len() > 256 * 1024 {
            return Err(err(
                "invalid_request",
                "Calendar intent exceeds its size limit",
            ));
        }
        crate::fields(
            request,
            &[
                "request_id",
                "kind",
                "calendar_id",
                "event_id",
                "expected_etag",
                "fields",
                "response",
            ],
        )?;
        let request_id = crate::string(request, "request_id")?;
        crate::id(request_id)?;
        let calendar = crate::string(request, "calendar_id")?;
        crate::live_calendar::provider_id(calendar)?;
        let kind = crate::string(request, "kind")?;
        if !matches!(kind, "create" | "edit" | "delete" | "rsvp") {
            return Err(err(
                "unsupported_operation",
                "Choose create, edit, delete or self RSVP",
            ));
        }
        let hash = crate::calendar_store::fingerprint(request)?;
        let old: Option<(String,String)> = self.db.query_row("SELECT fingerprint,execution_mode FROM calendar_write_operations WHERE account=?1 AND request_id=?2",params![account,request_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((prior, mode)) = old {
            if prior != hash || mode != "live" {
                return Err(err(
                    "request_id_conflict",
                    "Calendar request identity was already used",
                ));
            }
            return self.calendar_live_status(account, request_id);
        }
        let event = if kind == "create" {
            if request.get("event_id").is_some()
                || request.get("expected_etag").is_some()
                || request.get("response").is_some()
            {
                return Err(err(
                    "invalid_request",
                    "Create does not accept an existing event or response",
                ));
            }
            // Hex is a subset of Google's base32hex alphabet. Include account and
            // calendar so stable request IDs in different scopes cannot collide.
            format!(
                "f{}",
                crate::calendar_store::fingerprint(&json!([account, calendar, request_id]))?
            )
        } else {
            let event = crate::string(request, "event_id")?;
            crate::live_calendar::provider_id(event)?;
            etag(crate::string(request, "expected_etag")?)?;
            event.to_owned()
        };
        if matches!(kind, "create" | "edit") {
            if request.get("response").is_some() {
                return Err(err("invalid_request", "An event edit cannot include RSVP"));
            }
            validate_fields(&request["fields"])?;
        } else if request.get("fields").is_some()
            || (kind == "delete" && request.get("response").is_some())
        {
            return Err(err(
                "invalid_request",
                "This operation does not accept event fields",
            ));
        }
        if kind == "rsvp"
            && !matches!(
                request["response"].as_str(),
                Some("accepted" | "tentative" | "declined")
            )
        {
            return Err(err("invalid_request", "Choose an invitation response"));
        }
        let target = EventKey {
            account: account.into(),
            calendar: calendar.into(),
            event,
        };
        let intent = json!({"request":request,"target":target});
        self.db.execute("INSERT INTO calendar_write_operations(account,request_id,fingerprint,intent_json,execution_mode,state) VALUES(?1,?2,?3,?4,'live','prepared')",params![account,request_id,hash,serde_json::to_string(&intent)?])?;
        self.calendar_live_status(account, request_id)
    }

    /// Internal execution lookup. Never return retained intent/source as IPC status.
    pub fn calendar_live_operation(
        &self,
        account: &str,
        request_id: &str,
    ) -> Result<CalendarWriteOperation> {
        crate::id(account)?;
        crate::id(request_id)?;
        let row:Option<OperationRow>=self.db.query_row("SELECT state,operation_revision,intent_json,attempt,connection_generation,permission_epoch,dispatch_json FROM calendar_write_operations WHERE account=?1 AND request_id=?2 AND execution_mode='live'",params![account,request_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional()?;
        let (state, version, intent, attempt, generation, permission, dispatch) =
            row.ok_or_else(|| err("not_found", "Live Calendar operation does not exist"))?;
        let lease = dispatch
            .map(|raw| -> Result<_> {
                Ok(CalendarWriteLease {
                    account: account.into(),
                    request_id: request_id.into(),
                    attempt,
                    connection_generation: generation.ok_or_else(|| {
                        err("invalid_request", "Stored attempt has no connection")
                    })?,
                    permission_epoch: permission.ok_or_else(|| {
                        err("invalid_request", "Stored attempt has no permission")
                    })?,
                    dispatch: serde_json::from_str(&raw)?,
                })
            })
            .transpose()?;
        Ok(CalendarWriteOperation {
            state,
            version,
            intent: serde_json::from_str(&intent)?,
            lease,
        })
    }

    pub fn calendar_live_status(&self, account: &str, request_id: &str) -> Result<Value> {
        let op = self.calendar_live_operation(account, request_id)?;
        Ok(
            json!({"request_id":request_id,"target":op.intent["target"],"kind":op.intent["request"]["kind"],"state":op.state,"revision":op.version,"local_only":false}),
        )
    }

    /// Explicitly abandon retained intent; this never claims to undo a request
    /// which may already have reached Google and never starts provider work.
    pub fn calendar_live_cancel(
        &mut self,
        account: &str,
        request_id: &str,
        expected_version: i64,
    ) -> Result<Value> {
        let op = self.calendar_live_operation(account, request_id)?;
        if op.version != expected_version {
            return Err(err("stale_revision", "Calendar operation changed"));
        }
        if op.state == "in_flight" {
            return Err(err(
                "calendar_operation_pending",
                "Wait for the active Calendar attempt to settle",
            ));
        }
        if matches!(
            op.state.as_str(),
            "prepared" | "ready" | "unknown" | "conflict" | "rejected"
        ) {
            self.db.execute("UPDATE calendar_write_operations SET state='abandoned',operation_revision=operation_revision+1 WHERE account=?1 AND request_id=?2 AND execution_mode='live' AND operation_revision=?3",params![account,request_id,expected_version])?;
        }
        self.calendar_live_status(account, request_id)
    }

    /// The writer owns the crash boundary: callers invoke this once on engine
    /// startup, before admitting new provider tasks, never during active work.
    pub fn calendar_live_recover(&mut self) -> Result<usize> {
        Ok(self.db.execute("UPDATE calendar_write_operations SET state='unknown',operation_revision=operation_revision+1,outcome_json='{\"kind\":\"unknown\"}' WHERE execution_mode='live' AND state='in_flight'",[])?)
    }

    /// Internal convenience for callers which already own mutation admission.
    pub fn calendar_live_claim(
        &mut self,
        account: &str,
        request_id: &str,
        expected_version: i64,
        connection_generation: i64,
        permission_epoch: i64,
        source: Option<Value>,
    ) -> Result<CalendarWriteLease> {
        self.calendar_live_stage(
            account,
            request_id,
            expected_version,
            connection_generation,
            permission_epoch,
            source,
        )?;
        self.calendar_live_start(account, request_id, connection_generation)
    }

    /// Freeze preflight while holding bounded read memory, before waiting for
    /// mutation admission. Ready means no provider mutation has been dispatched.
    pub fn calendar_live_stage(
        &mut self,
        account: &str,
        request_id: &str,
        expected_version: i64,
        connection_generation: i64,
        permission_epoch: i64,
        source: Option<Value>,
    ) -> Result<Value> {
        if live(self, account)? != connection_generation {
            return Err(err("stale_google_work", "Google connection changed"));
        }
        let op = self.calendar_live_operation(account, request_id)?;
        if !matches!(op.state.as_str(), "prepared" | "ready") || op.version != expected_version {
            return Err(err(
                "calendar_operation_pending",
                "This Calendar intent was already attempted",
            ));
        }
        let target: EventKey = serde_json::from_value(op.intent["target"].clone())?;
        let checkpoint = self.calendar_sync_scope(account, &target.calendar)?;
        if checkpoint["permission_epoch"] != permission_epoch
            || checkpoint["full_generation"].as_i64().unwrap_or(0) == 0
            || checkpoint["retained_errors"].as_i64().unwrap_or(1) != 0
        {
            return Err(err(
                "calendar_not_ready",
                "Complete Calendar acquisition and refresh permissions before writing",
            ));
        }
        let access = crate::calendar_store::parse_access(crate::string(&checkpoint, "access")?)?;
        let zone = crate::string(&checkpoint, "time_zone")?;
        let request = &op.intent["request"];
        let kind = crate::string(request, "kind")?;
        let actor: String =
            self.db
                .query_row("SELECT email FROM accounts WHERE id=?1", [account], |r| {
                    r.get(0)
                })?;
        let (body, baseline) = if kind == "create" {
            if source.is_some()
                || !matches!(
                    access,
                    Access::Writer | Access::Owner | Access::WriterWithoutPrivateAccess
                )
            {
                return Err(err(
                    "permission_denied",
                    "Calendar does not permit event creation",
                ));
            }
            let body = field_patch(&request["fields"], None, zone)?;
            let mut candidate = body.clone();
            candidate["id"] = target.event.clone().into();
            candidate["etag"] = "pending".into();
            fm_calendar::normalize(account, &target.calendar, zone, &candidate)
                .map_err(crate::calendar_store::calendar_error)?;
            (body, Value::Null)
        } else {
            let source =
                source.ok_or_else(|| err("not_found", "Refresh this event before writing"))?;
            bounded_source(&source, &target)?;
            let event = fm_calendar::normalize(account, &target.calendar, zone, &source)
                .map_err(crate::calendar_store::calendar_error)?;
            initial_event(&event, access, kind)?;
            let expected = crate::string(request, "expected_etag")?;
            if event.etag != expected {
                return Err(err(
                    "stale_revision",
                    "The event changed; your edit is retained for review",
                ));
            }
            let body = match kind {
                "edit" => field_patch(&request["fields"], Some(&event), zone)?,
                "rsvp" => {
                    let person = event
                        .attendees
                        .iter()
                        .filter(|p| {
                            p["self"] == true
                                && p["email"]
                                    .as_str()
                                    .is_some_and(|email| email.eq_ignore_ascii_case(&actor))
                        })
                        .collect::<Vec<_>>();
                    if person.len() != 1 {
                        return Err(err(
                            "permission_denied",
                            "Only the authenticated attendee can respond",
                        ));
                    }
                    json!({"attendeesOmitted":true,"attendees":[{"email":person[0]["email"],"responseStatus":request["response"]}]})
                }
                "delete" => Value::Null,
                _ => unreachable!(),
            };
            (body, source)
        };
        let dispatch = json!({"kind":kind,"target":target,"body":body,"if_match":request.get("expected_etag"),"send_updates":"all","baseline":baseline,"base_revision":checkpoint["revision"]});
        let changed=self.db.execute("UPDATE calendar_write_operations SET state='ready',operation_revision=operation_revision+1,connection_generation=?4,permission_epoch=?5,dispatch_json=?6 WHERE account=?1 AND request_id=?2 AND execution_mode='live' AND state IN ('prepared','ready') AND operation_revision=?3",params![account,request_id,expected_version,connection_generation,permission_epoch,serde_json::to_string(&dispatch)?])?;
        if changed != 1 {
            return Err(err("stale_revision", "Calendar operation changed"));
        }
        self.calendar_live_status(account, request_id)
    }

    /// Called with mutation quota and one response/source memory seat already
    /// admitted. This is the final durable boundary before the one HTTP attempt.
    pub fn calendar_live_start(
        &mut self,
        account: &str,
        request_id: &str,
        connection_generation: i64,
    ) -> Result<CalendarWriteLease> {
        if live(self, account)? != connection_generation {
            return Err(err("stale_google_work", "Google connection changed"));
        }
        let op = self.calendar_live_operation(account, request_id)?;
        if op.state != "ready" {
            return Err(err(
                "calendar_operation_pending",
                "Calendar operation is not ready for dispatch",
            ));
        }
        let mut lease = op
            .lease
            .ok_or_else(|| err("invalid_request", "Calendar dispatch is missing"))?;
        let calendar = crate::string(&lease.dispatch["target"], "calendar")?;
        let scope = self.calendar_sync_scope(account, calendar)?;
        if lease.connection_generation != connection_generation
            || scope["permission_epoch"] != lease.permission_epoch
            || scope["revision"] != lease.dispatch["base_revision"]
        {
            return Err(err(
                "stale_revision",
                "Calendar changed while waiting; the unsent intent is retained",
            ));
        }
        lease.attempt += 1;
        if self.db.execute("UPDATE calendar_write_operations SET state='in_flight',attempt=?4,operation_revision=operation_revision+1 WHERE account=?1 AND request_id=?2 AND execution_mode='live' AND state='ready' AND operation_revision=?3",params![account,request_id,op.version,lease.attempt])?!=1 {return Err(err("stale_revision","Calendar operation changed"));}
        Ok(lease)
    }

    /// Deliberately independent of current connection/permissions. Evidence from
    /// the exact persisted attempt survives cancellation and reconnect races.
    pub fn calendar_live_record_outcome(
        &mut self,
        lease: &CalendarWriteLease,
        outcome: CalendarWriteOutcome,
    ) -> Result<Value> {
        let op = self.check_calendar_write_lease(lease)?;
        if !matches!(op.state.as_str(), "in_flight" | "unknown") {
            return self.calendar_live_status(&lease.account, &lease.request_id);
        }
        let (state, evidence) = match outcome {
            CalendarWriteOutcome::Accepted(source) => {
                let target: EventKey = serde_json::from_value(lease.dispatch["target"].clone())?;
                bounded_source(&source, &target)?;
                etag(crate::string(&source, "etag")?)?;
                ("accepted", json!({"kind":"accepted","source":source}))
            }
            CalendarWriteOutcome::Deleted => {
                if lease.dispatch["kind"] != "delete" {
                    return Err(err("scope_mismatch", "Delete receipt differs from intent"));
                }
                ("accepted", json!({"kind":"deleted"}))
            }
            CalendarWriteOutcome::Conflict => ("conflict", json!({"kind":"conflict"})),
            CalendarWriteOutcome::Rejected(code) => {
                if !matches!(
                    code.as_str(),
                    "permission_denied"
                        | "invalid_request"
                        | "authentication_required"
                        | "rate_limited"
                        | "not_found"
                ) {
                    return Err(err("invalid_request", "Unsupported Calendar rejection"));
                }
                ("rejected", json!({"kind":"rejected","code":code}))
            }
            CalendarWriteOutcome::UndispatchedCooldown { retry_after_secs } => (
                "ready",
                json!({"kind":"undispatched_cooldown","retry_after_secs":retry_after_secs}),
            ),
            CalendarWriteOutcome::Unknown => ("unknown", json!({"kind":"unknown"})),
        };
        self.db.execute("UPDATE calendar_write_operations SET state=?3,outcome_json=?4,operation_revision=operation_revision+1 WHERE account=?1 AND request_id=?2 AND execution_mode='live'",params![lease.account,lease.request_id,state,serde_json::to_string(&evidence)?])?;
        self.calendar_live_status(&lease.account, &lease.request_id)
    }

    /// A read can establish the intended state without repeating a mutation.
    /// Missing/inaccessible resources alone never prove that a delete succeeded.
    pub fn calendar_live_reconcile_source(
        &mut self,
        lease: &CalendarWriteLease,
        source: Value,
    ) -> Result<Value> {
        let op = self.check_calendar_write_lease(lease)?;
        if !matches!(op.state.as_str(), "in_flight" | "unknown") {
            return self.calendar_live_status(&lease.account, &lease.request_id);
        }
        let target: EventKey = serde_json::from_value(lease.dispatch["target"].clone())?;
        bounded_source(&source, &target)?;
        let matches = if lease.dispatch["kind"] == "delete" {
            source["status"] == "cancelled"
        } else {
            matches_patch(&source, &lease.dispatch["body"])
        };
        if !matches {
            return self.calendar_live_record_outcome(lease, CalendarWriteOutcome::Unknown);
        }
        if lease.dispatch["kind"] == "delete" {
            self.calendar_live_record_outcome(lease, CalendarWriteOutcome::Deleted)
        } else {
            self.calendar_live_record_outcome(lease, CalendarWriteOutcome::Accepted(source))
        }
    }

    /// Fence acquisition which began before an accepted write. Cleanup remains
    /// bounded outside this call, and a newer published revision is never expired.
    pub fn calendar_live_expire_staging(
        &mut self,
        lease: &CalendarWriteLease,
    ) -> Result<Vec<crate::live_calendar::CalendarSyncLease>> {
        let op = self.check_calendar_write_lease(lease)?;
        if op.state != "accepted" || live(self, &lease.account)? != lease.connection_generation {
            return Ok(vec![]);
        }
        let calendar = crate::string(&lease.dispatch["target"], "calendar")?;
        let scope = self.calendar_sync_scope(&lease.account, calendar)?;
        if scope["permission_epoch"] != lease.permission_epoch
            || scope["revision"] != lease.dispatch["base_revision"]
        {
            return Ok(vec![]);
        }
        let mut statement=self.db.prepare("SELECT id,connection_generation FROM calendar_sync_runs WHERE account=?1 AND calendar_id=?2 AND kind='events' AND state='staging' AND base_revision=?3 AND permission_epoch=?4")?;
        let runs = statement
            .query_map(
                params![
                    lease.account,
                    calendar,
                    scope["revision"].as_i64().unwrap(),
                    lease.permission_epoch
                ],
                |r| {
                    Ok(crate::live_calendar::CalendarSyncLease {
                        account: lease.account.clone(),
                        calendar: calendar.into(),
                        run: r.get(0)?,
                        connection_generation: r.get(1)?,
                    })
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(statement);
        for run in &runs {
            self.calendar_expire_sync(run)?;
        }
        Ok(runs)
    }

    /// Publish only when no newer source won the race. Otherwise leave the
    /// provider success visible in the journal and let current sync own the cache.
    pub fn calendar_live_publish(&mut self, lease: &CalendarWriteLease) -> Result<Value> {
        let op = self.check_calendar_write_lease(lease)?;
        if op.state != "accepted" {
            return self.calendar_live_status(&lease.account, &lease.request_id);
        }
        let current_generation = live(self, &lease.account)?;
        let target: EventKey = serde_json::from_value(lease.dispatch["target"].clone())?;
        let scope = self.calendar_sync_scope(&lease.account, &target.calendar)?;
        let authority_unchanged = current_generation == lease.connection_generation
            && scope["permission_epoch"] == lease.permission_epoch;
        if authority_unchanged {
            self.calendar_assert_no_pending(&lease.account, &target.calendar)?;
        }
        let raw: String = self.db.query_row(
            "SELECT outcome_json FROM calendar_write_operations WHERE account=?1 AND request_id=?2",
            params![lease.account, lease.request_id],
            |r| r.get(0),
        )?;
        let outcome: Value = serde_json::from_str(&raw)?;
        let source = if outcome["kind"] == "deleted" {
            json!({"id":target.event,"status":"cancelled"})
        } else {
            outcome["source"].clone()
        };
        let current:Option<String>=self.db.query_row("SELECT json FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![lease.account,target.calendar,target.event],|r|r.get(0)).optional()?;
        let current = current
            .map(|v| serde_json::from_str::<Value>(&v))
            .transpose()?;
        let unchanged = current
            .as_ref()
            .is_none_or(|v| v["etag"] == lease.dispatch["if_match"] || v == &source);
        // A full publication after dispatch can include an even newer event or
        // remove it. No response ordering heuristic can reverse that publication.
        let revision_matches = scope["revision"] == lease.dispatch["base_revision"];
        let publish = authority_unchanged
            && unchanged
            && (revision_matches || current.as_ref() == Some(&source));
        let tx = self.db.unchecked_transaction()?;
        if publish {
            let generation = scope["published_generation"].as_i64().unwrap();
            let full = scope["full_generation"].as_i64().unwrap();
            let event = fm_calendar::normalize(
                &lease.account,
                &target.calendar,
                crate::string(&scope, "time_zone")?,
                &source,
            )
            .map_err(crate::calendar_store::calendar_error)?;
            let raw = serde_json::to_string(&source)?;
            tx.execute("UPDATE calendar_sync_state SET revision=revision+1 WHERE account=?1 AND calendar_id=?2",params![lease.account,target.calendar])?;
            tx.execute("INSERT INTO calendar_sources(account,calendar_id,event_id,json,source_generation) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(account,calendar_id,event_id) DO UPDATE SET json=excluded.json,source_generation=excluded.source_generation",params![lease.account,target.calendar,target.event,raw,generation])?;
            crate::live_calendar::index_event(&tx, &event, 0, generation)?;
            tx.execute("INSERT INTO calendar_event_state(account,calendar_id,event_id,visible,observed_json,seen_full_generation) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(account,calendar_id,event_id) DO UPDATE SET visible=excluded.visible,observed_json=excluded.observed_json,seen_full_generation=excluded.seen_full_generation",params![lease.account,target.calendar,target.event,!event.cancelled,raw,full])?;
            tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        }
        let receipt = json!({"request_id":lease.request_id,"target":target,"state":"acknowledged","cache_published":publish,"local_only":false});
        tx.execute("UPDATE calendar_write_operations SET state='acknowledged',result_json=?3,operation_revision=operation_revision+1 WHERE account=?1 AND request_id=?2",params![lease.account,lease.request_id,serde_json::to_string(&receipt)?])?;
        tx.commit()?;
        self.calendar_live_status(&lease.account, &lease.request_id)
    }

    fn check_calendar_write_lease(
        &self,
        lease: &CalendarWriteLease,
    ) -> Result<CalendarWriteOperation> {
        let op = self.calendar_live_operation(&lease.account, &lease.request_id)?;
        let stored = op
            .lease
            .as_ref()
            .ok_or_else(|| err("invalid_request", "Calendar operation has not dispatched"))?;
        if stored.attempt != lease.attempt
            || stored.connection_generation != lease.connection_generation
            || stored.permission_epoch != lease.permission_epoch
            || stored.dispatch != lease.dispatch
        {
            return Err(err("stale_revision", "Calendar attempt changed"));
        }
        Ok(op)
    }
}

fn live(store: &Store, account: &str) -> Result<i64> {
    crate::id(account)?;
    if store.profile_kind()? != crate::live_profile::ProfileKind::Live {
        return Err(err(
            "profile_kind_mismatch",
            "Live Calendar writes require a live profile",
        ));
    }
    let binding = store
        .google_account_bindings()?
        .into_iter()
        .find(|b| b.account == account)
        .ok_or_else(|| err("not_found", "Google account is unavailable"))?;
    if binding.state != crate::live_profile::ConnectionState::Connected
        || !binding
            .scopes
            .contains("https://www.googleapis.com/auth/calendar.events")
    {
        return Err(err(
            "permission_denied",
            "Google Calendar write scope is unavailable",
        ));
    }
    Ok(binding.connection_generation)
}
fn etag(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 1024
        || value == "*"
        || value.contains(',')
        || value.chars().any(char::is_control)
    {
        Err(err("invalid_request", "A current event ETag is required"))
    } else {
        Ok(())
    }
}
fn bounded_source(source: &Value, target: &EventKey) -> Result<()> {
    if source["id"] != target.event
        || !source.is_object()
        || serde_json::to_vec(source)?.len() > 2 * 1024 * 1024
    {
        Err(err(
            "scope_mismatch",
            "Provider event identity or size differs",
        ))
    } else {
        Ok(())
    }
}
fn initial_event(event: &Event, access: Access, kind: &str) -> Result<()> {
    if !matches!(access.for_event(event), Access::Writer | Access::Owner) {
        return Err(err("permission_denied", "This event copy is not writable"));
    }
    if event.cancelled
        || !event.recurrence.is_empty()
        || event.series_id.is_some()
        || event.source["locked"] == true
        || !matches!(event.source["eventType"].as_str(), None | Some("default"))
    {
        return Err(err(
            "unsupported_operation",
            "This write slice supports ordinary nonrecurring events",
        ));
    }
    if kind != "rsvp"
        && (!event.attendees.is_empty()
            || event.source["attendeesOmitted"] == true
            || event.source["organizer"]["self"] != true)
    {
        return Err(err(
            "unsupported_operation",
            "First Calendar edits require a no-attendee organizer copy",
        ));
    }
    Ok(())
}
fn validate_fields(fields: &Value) -> Result<()> {
    crate::fields(
        fields,
        &[
            "title",
            "start",
            "end",
            "all_day",
            "time_zone",
            "location",
            "description",
            "conference_url",
            "attendees",
        ],
    )?;
    if fields.as_object().is_none_or(|v| v.is_empty()) {
        return Err(err("invalid_request", "Choose event fields to change"));
    }
    Ok(())
}
fn field_patch(fields: &Value, event: Option<&Event>, zone: &str) -> Result<Value> {
    validate_fields(fields)?;
    let mut body = json!({});
    for (input, provider, max) in [
        ("title", "summary", 512),
        ("description", "description", 32768),
        ("location", "location", 2048),
    ] {
        if let Some(value) = fields.get(input) {
            let text = value
                .as_str()
                .ok_or_else(|| err("invalid_request", "Event fields must be text"))?;
            if text.len() > max
                || (input == "title"
                    && (text.trim().is_empty() || text.chars().any(char::is_control)))
            {
                return Err(err("invalid_request", "Event text exceeds bounds"));
            }
            if event.is_none_or(|e| e.source[provider].as_str().unwrap_or("") != text) {
                body[provider] = value.clone();
            }
        } else if input == "title" && event.is_none() {
            return Err(err("invalid_request", "An event title is required"));
        }
    }
    if let Some(attendees) = fields.get("attendees") {
        if !attendees.as_array().is_some_and(Vec::is_empty) {
            return Err(err(
                "unsupported_operation",
                "Attendee changes require the invitation slice",
            ));
        }
    }
    if let Some(conference) = fields.get("conference_url") {
        let conference = conference
            .as_str()
            .ok_or_else(|| err("invalid_request", "Conference link must be text"))?;
        if conference
            != event
                .and_then(|e| e.conference_url.as_deref())
                .unwrap_or("")
        {
            return Err(err(
                "unsupported_operation",
                "Conference changes require a separate operation",
            ));
        }
    }
    if event.is_none()
        || ["start", "end", "all_day", "time_zone"]
            .iter()
            .any(|k| fields.get(k).is_some())
    {
        let all_day = fields["all_day"]
            .as_bool()
            .ok_or_else(|| err("invalid_request", "Supply complete start/end time fields"))?;
        let selected_zone = fields["time_zone"].as_str().unwrap_or(zone);
        let start = crate::string(fields, "start")?;
        let end = crate::string(fields, "end")?;
        let times = |s: &str| {
            if all_day {
                json!({"date":s})
            } else {
                json!({"dateTime":s,"timeZone":selected_zone})
            }
        };
        let start = times(start);
        let end = times(end);
        if event.is_some_and(|e| e.source["start"].get("date").is_some() != all_day) {
            return Err(err(
                "unsupported_operation",
                "Timed/all-day conversion requires the next Calendar slice",
            ));
        }
        let candidate = json!({"id":"validate","start":start,"end":end});
        fm_calendar::normalize("validate", "validate", selected_zone, &candidate)
            .map_err(crate::calendar_store::calendar_error)?;
        if event.is_none_or(|e| e.source["start"] != start) {
            body["start"] = start;
        }
        if event.is_none_or(|e| e.source["end"] != end) {
            body["end"] = end;
        }
    }
    Ok(body)
}
fn matches_patch(source: &Value, patch: &Value) -> bool {
    let Some(fields) = patch.as_object() else {
        return false;
    };
    fields.iter().all(|(key, value)| match key.as_str() {
        "start" | "end" => same_time(&source[key], value),
        "attendeesOmitted" => true,
        "attendees" => value.as_array().is_some_and(|wanted| {
            wanted.iter().all(|p| {
                source["attendees"].as_array().is_some_and(|items| {
                    items.iter().any(|actual| {
                        actual["email"]
                            .as_str()
                            .zip(p["email"].as_str())
                            .is_some_and(|(actual, wanted)| actual.eq_ignore_ascii_case(wanted))
                            && actual["self"] == true
                            && actual["responseStatus"] == p["responseStatus"]
                    })
                })
            })
        }),
        _ => source[key] == *value,
    })
}
fn same_time(actual: &Value, wanted: &Value) -> bool {
    if let Some(date) = wanted["date"].as_str() {
        return actual["date"] == date && actual.get("dateTime").is_none();
    }
    let parsed = actual["dateTime"]
        .as_str()
        .zip(wanted["dateTime"].as_str())
        .and_then(|(a, b)| {
            Some((
                chrono::DateTime::parse_from_rfc3339(a).ok()?,
                chrono::DateTime::parse_from_rfc3339(b).ok()?,
            ))
        });
    parsed.is_some_and(|(a, b)| a == b)
        && wanted
            .get("timeZone")
            .is_none_or(|zone| actual.get("timeZone") == Some(zone))
}

#[cfg(test)]
#[path = "live_calendar_writes_tests.rs"]
mod tests;
