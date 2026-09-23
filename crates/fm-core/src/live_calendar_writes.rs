//! Durable human Calendar intent. Provider receipts and visible cache publication
//! are separate: a reconnect or concurrent read never erases a dispatched result.
use crate::live_calendar_occurrences::{
    same_original, CalendarOccurrenceSelector, MasterStamp, PreparedCalendarOccurrence,
};
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
/// Authority and source ordering captured before an explicit reconciliation GET.
/// This never changes the immutable lease of the original provider mutation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CalendarReadPublicationFence {
    account: String,
    request_id: String,
    attempt: i64,
    calendar: String,
    connection_generation: i64,
    permission_epoch: i64,
    source_revision: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lifecycle_revision: Option<i64>,
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
// Accepted outcomes may contain a full provider event. Read their small public
// meaning from state/kind; only the small rejection/cooldown evidence is parsed.
const PUBLIC_COLUMNS: &str = "request_id,json_extract(intent_json,'$.target.calendar'),json_extract(intent_json,'$.target.event'),json_extract(intent_json,'$.request.kind'),state,operation_revision,CASE WHEN state IN ('ready','rejected','abandoned') THEN json_extract(outcome_json,'$.kind') END,CASE WHEN state IN ('rejected','abandoned') THEN json_extract(outcome_json,'$.code') END,CASE WHEN state IN ('ready','abandoned') THEN json_extract(outcome_json,'$.retry_after_secs') END,CASE WHEN state IN ('acknowledged','reconciled') THEN json_extract(result_json,'$.cache_published') END,coalesce(json_extract(intent_json,'$.version'),1),CASE WHEN state='reconciled' THEN json_extract(outcome_json,'$.observed_etag') END,CASE WHEN state='reconciled' THEN json_extract(outcome_json,'$.event_cancelled') END,CASE WHEN json_extract(intent_json,'$.version')=3 AND length(CAST(json_extract(intent_json,'$.request.occurrence') AS BLOB))<=4096 THEN json_extract(intent_json,'$.request.occurrence') END";
struct OperationProjection {
    request_id: String,
    calendar: String,
    event: String,
    kind: String,
    state: String,
    revision: i64,
    evidence_kind: Option<String>,
    error_code: Option<String>,
    retry_after_secs: Option<u64>,
    cache_published: Option<bool>,
    intent_version: u64,
    observed_etag: Option<String>,
    event_cancelled: Option<bool>,
    occurrence: Option<String>,
}
impl OperationProjection {
    fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            request_id: row.get(0)?,
            calendar: row.get(1)?,
            event: row.get(2)?,
            kind: row.get(3)?,
            state: row.get(4)?,
            revision: row.get(5)?,
            evidence_kind: row.get(6)?,
            error_code: row.get(7)?,
            retry_after_secs: row.get(8)?,
            cache_published: row.get(9)?,
            intent_version: row.get(10)?,
            observed_etag: row.get(11)?,
            event_cancelled: row.get(12)?,
            occurrence: row.get(13)?,
        })
    }
    fn value(self, account: &str) -> Value {
        let evidence = match self.state.as_str() {
            "accepted" | "acknowledged" => Some(if self.kind == "delete" {
                "deleted"
            } else {
                "accepted"
            }),
            "unknown" => Some("unknown"),
            "reconciled" => Some("observed_state"),
            "conflict" => Some("conflict"),
            "rejected" => Some("rejected"),
            "ready" | "abandoned" => self.evidence_kind.as_deref().filter(|kind| {
                matches!(
                    *kind,
                    "unknown" | "conflict" | "rejected" | "undispatched_cooldown"
                )
            }),
            _ => None,
        };
        let mut outcome = evidence
            .map(|kind| json!({"kind":kind}))
            .unwrap_or(Value::Null);
        if evidence == Some("rejected") {
            if let Some(code) = self.error_code.filter(|code| {
                matches!(
                    code.as_str(),
                    "permission_denied"
                        | "invalid_request"
                        | "authentication_required"
                        | "rate_limited"
                        | "not_found"
                )
            }) {
                outcome["code"] = code.into();
            }
        }
        if evidence == Some("undispatched_cooldown") {
            if let Some(seconds) = self.retry_after_secs {
                outcome["retry_after_secs"] = seconds.into();
            }
        }
        if let Some(published) = self.cache_published {
            outcome["cache_published"] = published.into();
        }
        if matches!(self.intent_version, 2..=4) {
            if matches!(evidence, Some("accepted" | "deleted")) {
                outcome["evidence_origin"] = "provider_response".into();
                outcome["notification_delivery"] = "unverified".into();
            } else if evidence == Some("observed_state") {
                outcome["matches_intent"] = true.into();
                outcome["observed_etag"] = self.observed_etag.into();
                outcome["event_cancelled"] = self.event_cancelled.unwrap_or(false).into();
                outcome["notification_delivery"] = "unverified".into();
            }
        }
        if self.intent_version == 3 {
            if evidence == Some("deleted") {
                outcome["source_origin"] = "dispatched_occurrence_identity".into();
            }
            if evidence == Some("observed_state") {
                outcome["source_origin"] = "fetched".into();
            }
        }
        let mut value = json!({"request_id":self.request_id,"target":{"account":account,"calendar":self.calendar,"event":self.event},"kind":self.kind,"state":self.state,"revision":self.revision,"intent_version":self.intent_version,"local_only":false,"outcome":outcome});
        if let Some(occurrence) = self
            .occurrence
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        {
            value["occurrence"] = occurrence;
        }
        value
    }
}

impl Store {
    pub fn calendar_live_prepare(&mut self, account: &str, request: &Value) -> Result<Value> {
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
                "occurrence",
                "recurring",
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
        live(self, account)?;
        let version = if request.get("recurring").is_some()
            || request["fields"].get("recurrence").is_some()
        {
            crate::live_calendar_recurring::validate_request(request)?;
            4
        } else if request.get("occurrence").is_some() {
            if !matches!(kind, "edit" | "delete") {
                return Err(err(
                    "unsupported_operation",
                    "Occurrence scope supports organizer edit or delete only",
                ));
            }
            occurrence_request(request)?;
            3
        } else {
            2
        };
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
            validate_fields_version(&request["fields"], version)?;
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
        if version == 4 {
            self.calendar_recurring_assert_no_pending(
                account,
                request_id,
                &serde_json::to_value(&target)?,
                &request["recurring"],
            )?;
        } else {
            self.calendar_recurring_guard_legacy(
                account,
                request_id,
                &serde_json::to_value(&target)?,
                &request["occurrence"],
            )?;
        }
        let intent = json!({"version":version,"request":request,"target":target});
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
        crate::id(account)?;
        crate::id(request_id)?;
        let value = self.db.query_row(
            &format!("SELECT {PUBLIC_COLUMNS} FROM calendar_write_operations WHERE account=?1 AND request_id=?2 AND execution_mode='live'"),
            params![account,request_id],
            OperationProjection::read,
        ).optional()?.map(|operation|operation.value(account)).ok_or_else(||err("not_found","Live Calendar operation does not exist"))?;
        self.calendar_recurring_status(value, account, request_id)
    }

    /// Private engine input for an explicit resume. No provider source or frozen
    /// dispatch is loaded, and only a known-undispatched intent may be resumed.
    pub fn calendar_live_request(&self, account: &str, request_id: &str) -> Result<Value> {
        crate::id(account)?;
        crate::id(request_id)?;
        let record: Option<(String, Option<String>)> = self.db.query_row(
            "SELECT state,CASE WHEN state IN ('prepared','ready','partial') AND length(CAST(json_extract(intent_json,'$.request') AS BLOB))<=262144 THEN json_extract(intent_json,'$.request') END FROM calendar_write_operations WHERE account=?1 AND request_id=?2 AND execution_mode='live'",
            params![account,request_id],|row|Ok((row.get(0)?,row.get(1)?)),
        ).optional()?;
        let (state, request) =
            record.ok_or_else(|| err("not_found", "Live Calendar operation does not exist"))?;
        if !matches!(state.as_str(), "prepared" | "ready" | "partial") {
            return Err(err(
                "calendar_operation_pending",
                "Only an undispatched prepared or ready Calendar intent can resume",
            ));
        }
        let request: Value = serde_json::from_str(&request.ok_or_else(|| {
            err(
                "invalid_request",
                "Stored Calendar intent exceeds its size limit",
            )
        })?)?;
        if !request.is_object() || request["request_id"] != request_id {
            return Err(err(
                "invalid_request",
                "Stored Calendar request identity is invalid",
            ));
        }
        Ok(request)
    }

    pub(crate) fn calendar_live_operations(
        &self,
        account: &str,
        limit: u64,
        after_acknowledged: i64,
        before_id: i64,
    ) -> Result<Value> {
        crate::id(account)?;
        let mut statement = self.db.prepare(&format!("SELECT {PUBLIC_COLUMNS},rowid,(result_json IS NOT NULL) FROM calendar_write_operations WHERE account=?1 AND execution_mode='live' AND ((result_json IS NOT NULL)>?3 OR ((result_json IS NOT NULL)=?3 AND rowid<?4)) ORDER BY (result_json IS NOT NULL),rowid DESC LIMIT ?2"))?;
        let rows = statement
            .query_map(
                params![account, limit + 1, after_acknowledged, before_id],
                |row| {
                    Ok((
                        OperationProjection::read(row)?,
                        row.get::<_, i64>(14)?,
                        row.get::<_, bool>(15)?,
                    ))
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let has_more = rows.len() > limit as usize;
        let mut continuation = Value::Null;
        let mut operations = Vec::new();
        for (operation, row_id, acknowledged) in rows.into_iter().take(limit as usize) {
            if has_more {
                continuation = json!({"acknowledged":acknowledged,"before_id":row_id});
            }
            let request_id = operation.request_id.clone();
            operations.push(self.calendar_recurring_status(
                operation.value(account),
                account,
                &request_id,
            )?);
        }
        Ok(json!({"operations":operations,"continuation":continuation}))
    }

    /// Explicitly abandon retained intent; this never claims to undo a request
    /// which may already have reached Google and never starts provider work.
    pub fn calendar_live_cancel(
        &mut self,
        account: &str,
        request_id: &str,
        expected_version: i64,
    ) -> Result<Value> {
        self.require_active_account(account)?;
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
            "prepared" | "ready" | "unknown" | "conflict" | "rejected" | "partial"
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
        let intent_version = document_version(&op.intent)?;
        if intent_version == 3 {
            return Err(err(
                "invalid_request",
                "Occurrence writes require a qualified master and instance proof",
            ));
        }
        let kind = crate::string(request, "kind")?;
        let actor: String =
            self.db
                .query_row("SELECT email FROM accounts WHERE id=?1", [account], |r| {
                    r.get(0)
                })?;
        let (body, baseline) = if kind == "create" {
            if source.is_some() || !live_source_writable(access) {
                return Err(err(
                    "permission_denied",
                    "Calendar does not permit event creation",
                ));
            }
            let body = field_patch(
                request_id,
                &request["fields"],
                None,
                zone,
                &actor,
                intent_version,
            )?;
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
            initial_event(&event, access, kind, intent_version)?;
            let expected = crate::string(request, "expected_etag")?;
            if event.etag != expected {
                return Err(err(
                    "stale_revision",
                    "The event changed; your edit is retained for review",
                ));
            }
            let body = match kind {
                "edit" => field_patch(
                    request_id,
                    &request["fields"],
                    Some(&event),
                    zone,
                    &actor,
                    intent_version,
                )?,
                "rsvp" => {
                    let person = live_self_attendee(&event, &actor)?;
                    json!({"attendeesOmitted":true,"attendees":[{"email":person["email"],"responseStatus":request["response"]}]})
                }
                "delete" => Value::Null,
                _ => unreachable!(),
            };
            (body, source)
        };
        let mut body = body;
        if matches!(intent_version, 2 | 4) && kind == "rsvp" {
            preserve_label(&mut body, &baseline)?;
        }
        let mut dispatch = json!({"kind":kind,"target":target,"body":body,"if_match":request.get("expected_etag"),"send_updates":"all","baseline":baseline,"base_revision":checkpoint["revision"]});
        if matches!(intent_version, 2 | 4) {
            dispatch["version"] = intent_version.into();
            dispatch["event_label_version"] = if dispatch["body"].get("eventLabelId").is_some() {
                1.into()
            } else {
                0.into()
            };
        }
        if intent_version == 4 {
            crate::live_calendar_recurring::validate_series_dispatch(request, &dispatch)?;
            dispatch["recurring"] = request["recurring"].clone();
            dispatch["lifecycle_revision"] = self.require_active_account(account)?.into();
        }
        let changed=self.db.execute("UPDATE calendar_write_operations SET state='ready',operation_revision=operation_revision+1,connection_generation=?4,permission_epoch=?5,dispatch_json=?6 WHERE account=?1 AND request_id=?2 AND execution_mode='live' AND state IN ('prepared','ready') AND operation_revision=?3",params![account,request_id,expected_version,connection_generation,permission_epoch,serde_json::to_string(&dispatch)?])?;
        if changed != 1 {
            return Err(err("stale_revision", "Calendar operation changed"));
        }
        self.calendar_live_status(account, request_id)
    }

    pub fn calendar_live_stage_occurrence(
        &mut self,
        account: &str,
        request_id: &str,
        expected_version: i64,
        proof: PreparedCalendarOccurrence,
    ) -> Result<Value> {
        let op = self.calendar_live_operation(account, request_id)?;
        if document_version(&op.intent)? != 3
            || !matches!(op.state.as_str(), "prepared" | "ready")
            || op.version != expected_version
        {
            return Err(err(
                "calendar_operation_pending",
                "This occurrence intent was already attempted or changed",
            ));
        }
        if proof.master.stamp.account != account {
            return Err(err(
                "scope_mismatch",
                "Occurrence belongs to another account",
            ));
        }
        self.check_calendar_occurrence_master(&proof.master.stamp)?;
        if live(self, account)? != proof.master.stamp.connection_generation {
            return Err(err("stale_revision", "Calendar connection changed"));
        }
        proof.require_editable()?;
        let request = &op.intent["request"];
        let selector = occurrence_request(request)?;
        if !proof_matches_selector(&proof, &selector)
            || request["expected_etag"] != proof.event.etag
            || request["occurrence"]["expected_master_etag"] != proof.master.stamp.etag
        {
            return Err(err(
                "stale_revision",
                "The selected instance or recurring master changed; authored fields are retained",
            ));
        }
        let actor: String =
            self.db
                .query_row("SELECT email FROM accounts WHERE id=?1", [account], |r| {
                    r.get(0)
                })?;
        let kind = crate::string(request, "kind")?;
        let body = if kind == "edit" {
            field_patch(
                request_id,
                &request["fields"],
                Some(&proof.event),
                &proof.master.stamp.zone,
                &actor,
                2,
            )?
        } else {
            Value::Null
        };
        let label_version = if body.get("eventLabelId").is_some() {
            1
        } else {
            0
        };
        let stamp = &proof.master.stamp;
        let dispatch = json!({"version":3,"kind":kind,"target":op.intent["target"],"body":body,"if_match":request["expected_etag"],"send_updates":"all","baseline":proof.event.source,"base_revision":stamp.calendar_revision,"event_label_version":label_version,"occurrence":request["occurrence"],"original_start_provider":proof.event.source["originalStartTime"],"master_proof":stamp});
        if self.db.execute("UPDATE calendar_write_operations SET state='ready',operation_revision=operation_revision+1,connection_generation=?4,permission_epoch=?5,dispatch_json=?6 WHERE account=?1 AND request_id=?2 AND execution_mode='live' AND state IN ('prepared','ready') AND operation_revision=?3",params![account,request_id,expected_version,stamp.connection_generation,stamp.permission_epoch,serde_json::to_string(&dispatch)?])?!=1 {return Err(err("stale_revision","Calendar operation changed"));}
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
        if document_version(&op.intent)? != document_version(&lease.dispatch)? {
            return Err(err(
                "invalid_request",
                "Calendar dispatch version differs from its intent",
            ));
        }
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
        if document_version(&lease.dispatch)? == 4 {
            self.calendar_recurring_check_start(&lease)?;
        } else {
            self.calendar_recurring_guard_legacy(
                account,
                request_id,
                &lease.dispatch["target"],
                &lease.dispatch["occurrence"],
            )?;
        }
        if is_instance_dispatch(&lease.dispatch) {
            let stamp: MasterStamp =
                serde_json::from_value(lease.dispatch["master_proof"].clone())?;
            self.check_calendar_occurrence_master(&stamp)?;
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
        if let Some(status) = self.calendar_recurring_late_prior(lease, &outcome)? {
            return Ok(status);
        }
        let op = self.check_calendar_write_lease(lease)?;
        // Abandoning unknown work stops recovery; it cannot erase a response
        // from that exact still-running dispatched attempt. Legacy semantics
        // and never-dispatched abandonment remain unchanged.
        let late_partial = op.state == "partial"
            && lease.dispatch["version"] == 4
            && lease.dispatch["step"] == 0
            && matches!(&outcome, CalendarWriteOutcome::Accepted(_));
        let late_abandoned = op.state == "abandoned"
            && lease.attempt > 0
            && matches!(document_version(&lease.dispatch)?, 2..=4)
            && matches!(
                &outcome,
                CalendarWriteOutcome::Accepted(_) | CalendarWriteOutcome::Deleted
            );
        if (!matches!(op.state.as_str(), "in_flight" | "unknown" | "reconciled")
            && !late_abandoned
            && !late_partial)
            || (op.state == "reconciled"
                && !matches!(
                    &outcome,
                    CalendarWriteOutcome::Accepted(_) | CalendarWriteOutcome::Deleted
                ))
        {
            return self.calendar_live_status(&lease.account, &lease.request_id);
        }
        let (state, mut evidence) = match outcome {
            CalendarWriteOutcome::Accepted(source) => {
                if lease.dispatch["kind"] == "delete" {
                    return Err(err(
                        "scope_mismatch",
                        "An event response cannot acknowledge a delete",
                    ));
                }
                let target: EventKey = serde_json::from_value(lease.dispatch["target"].clone())?;
                bounded_source(&source, &target)?;
                etag(crate::string(&source, "etag")?)?;
                if is_instance_dispatch(&lease.dispatch)
                    && !matches_occurrence_source(&source, &lease.dispatch)?
                {
                    return Err(err(
                        "scope_mismatch",
                        "Provider response differs from the dispatched occurrence",
                    ));
                }
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
        if matches!(document_version(&lease.dispatch)?, 2..=4) && state == "accepted" {
            evidence["evidence_origin"] = "provider_response".into();
            evidence["notification_delivery"] = "unverified".into();
        }
        if is_instance_dispatch(&lease.dispatch) && evidence["kind"] == "deleted" {
            evidence["source_origin"] = "dispatched_occurrence_identity".into();
        }
        let state = if late_partial && state == "accepted" {
            "partial"
        } else {
            state
        };
        // A late exact response strengthens a matching observation. Its cache
        // publication is re-evaluated independently under the original fence.
        self.db.execute("UPDATE calendar_write_operations SET state=?3,outcome_json=?4,result_json=NULL,operation_revision=operation_revision+1 WHERE account=?1 AND request_id=?2 AND execution_mode='live'",params![lease.account,lease.request_id,state,serde_json::to_string(&evidence)?])?;
        self.calendar_live_status(&lease.account, &lease.request_id)
    }

    /// A read can establish the intended state without repeating a mutation.
    /// Missing/inaccessible resources alone never prove that a delete succeeded.
    pub fn calendar_live_reconcile_begin(
        &self,
        lease: &CalendarWriteLease,
        connection_generation: i64,
    ) -> Result<CalendarReadPublicationFence> {
        self.check_calendar_write_lease(lease)?;
        let generation = if matches!(document_version(&lease.dispatch)?, 3 | 4) {
            self.calendar_occurrence_read_connection(&lease.account)?
        } else {
            live(self, &lease.account)?
        };
        if generation != connection_generation {
            return Err(err(
                "stale_revision",
                "Calendar connection changed before observation",
            ));
        }
        let calendar = crate::string(&lease.dispatch["target"], "calendar")?;
        let scope = self.calendar_sync_scope(&lease.account, calendar)?;
        let access = crate::calendar_store::parse_access(crate::string(&scope, "access")?)?;
        if matches!(access, Access::None | Access::FreeBusy) {
            return Err(err("permission_denied", "Calendar details are unavailable"));
        }
        Ok(CalendarReadPublicationFence {
            account: lease.account.clone(),
            request_id: lease.request_id.clone(),
            attempt: lease.attempt,
            calendar: calendar.into(),
            connection_generation,
            permission_epoch: scope["permission_epoch"].as_i64().unwrap(),
            source_revision: scope["revision"].as_i64().unwrap(),
            lifecycle_revision: Some(self.require_active_account(&lease.account)?),
        })
    }

    // Synchronous Core fixtures have no intervening HTTP await. Production must
    // capture the separate fence before GET and pass it to reconcile_read.
    #[cfg(test)]
    pub fn calendar_live_reconcile_source(
        &mut self,
        lease: &CalendarWriteLease,
        source: Value,
    ) -> Result<Value> {
        let fence = self.calendar_live_reconcile_begin(lease, live(self, &lease.account)?)?;
        self.calendar_live_reconcile_read(lease, &fence, source)
    }

    pub fn calendar_live_reconcile_read(
        &mut self,
        lease: &CalendarWriteLease,
        fence: &CalendarReadPublicationFence,
        source: Value,
    ) -> Result<Value> {
        let op = self.check_calendar_write_lease(lease)?;
        fence.check(lease)?;
        if let Some(revision) = fence.lifecycle_revision {
            self.check_active_account_revision(&lease.account, revision)?;
        } else {
            self.require_active_account(&lease.account)?;
        }
        if !matches!(op.state.as_str(), "in_flight" | "unknown") {
            return self.calendar_live_status(&lease.account, &lease.request_id);
        }
        let target: EventKey = serde_json::from_value(lease.dispatch["target"].clone())?;
        bounded_source(&source, &target)?;
        let version = document_version(&lease.dispatch)?;
        if version == 4 {
            let current = self.calendar_sync_scope(&lease.account, &target.calendar)?;
            if self.calendar_occurrence_read_connection(&lease.account)?
                != fence.connection_generation
                || current["membership_denied"] == true
                || current["permission_epoch"] != fence.permission_epoch
                || current["revision"] != fence.source_revision
            {
                return Err(err(
                    "stale_revision",
                    "Recurring read authority or published source changed",
                ));
            }
            let access = crate::calendar_store::parse_access(crate::string(&current, "access")?)?;
            let event = fm_calendar::normalize(
                &lease.account,
                &target.calendar,
                crate::string(&current, "time_zone")?,
                &source,
            )
            .map_err(crate::calendar_store::calendar_error)?;
            if matches!(access.for_event(&event), Access::None | Access::FreeBusy) {
                return Err(err(
                    "permission_denied",
                    "Recurring source details are unavailable",
                ));
            }
        }
        if is_instance_dispatch(&lease.dispatch) {
            return Err(err(
                "invalid_request",
                "Occurrence reconciliation requires a qualified master and instance proof",
            ));
        }
        let matches = if lease.dispatch["kind"] == "delete" {
            source["status"] == "cancelled"
        } else if matches!(version, 2 | 4) {
            source["status"] != "cancelled"
                && source["etag"]
                    .as_str()
                    .is_some_and(|value| etag(value).is_ok())
                && fm_calendar::normalize(&lease.account, &target.calendar, "UTC", &source)
                    .is_ok_and(|event| matches_intent_v2(&event, &op.intent, &lease.dispatch))
        } else {
            matches_patch(&source, &lease.dispatch["body"])
        };
        if !matches {
            return self.calendar_live_record_outcome(lease, CalendarWriteOutcome::Unknown);
        }
        if matches!(version, 2 | 4) {
            let observed_etag = source["etag"].as_str().filter(|v| etag(v).is_ok());
            let evidence = json!({"kind":"observed_state","matches_intent":true,"observed_etag":observed_etag,"event_cancelled":source["status"] == "cancelled","notification_delivery":"unverified","source":source,"publication_fence":fence});
            self.db.execute("UPDATE calendar_write_operations SET state='reconciled',outcome_json=?3,operation_revision=operation_revision+1 WHERE account=?1 AND request_id=?2 AND execution_mode='live'",params![lease.account,lease.request_id,serde_json::to_string(&evidence)?])?;
            return self.calendar_live_status(&lease.account, &lease.request_id);
        }
        if lease.dispatch["kind"] == "delete" {
            self.calendar_live_record_outcome(lease, CalendarWriteOutcome::Deleted)
        } else {
            self.calendar_live_record_outcome(lease, CalendarWriteOutcome::Accepted(source))
        }
    }

    pub fn calendar_live_reconcile_occurrence_read(
        &mut self,
        lease: &CalendarWriteLease,
        fence: &CalendarReadPublicationFence,
        proof: PreparedCalendarOccurrence,
    ) -> Result<Value> {
        let op = self.check_calendar_write_lease(lease)?;
        fence.check(lease)?;
        if !is_instance_dispatch(&lease.dispatch) {
            return Err(err("invalid_request", "This is not an occurrence dispatch"));
        }
        self.check_calendar_occurrence_master(&proof.master.stamp)?;
        let stamp = &proof.master.stamp;
        if stamp.account != lease.account
            || stamp.calendar != fence.calendar
            || stamp.connection_generation != fence.connection_generation
            || stamp.permission_epoch != fence.permission_epoch
            || stamp.calendar_revision != fence.source_revision
            || Some(stamp.lifecycle_revision) != fence.lifecycle_revision
        {
            return Err(err(
                "stale_revision",
                "Occurrence observation and read publication fence differ",
            ));
        }
        if !matches!(op.state.as_str(), "in_flight" | "unknown") {
            return self.calendar_live_status(&lease.account, &lease.request_id);
        }
        if !matches_occurrence_source(&proof.event.source, &lease.dispatch)? {
            return Err(err(
                "scope_mismatch",
                "Observed instance differs from the original occurrence",
            ));
        }
        let cancelled = proof.event.source["status"] == "cancelled";
        let matches = if lease.dispatch["kind"] == "delete" {
            cancelled
        } else {
            !proof.event.cancelled && matches_intent_v2(&proof.event, &op.intent, &lease.dispatch)
        };
        if !matches {
            return self.calendar_live_record_outcome(lease, CalendarWriteOutcome::Unknown);
        }
        let evidence = json!({"kind":"observed_state","matches_intent":true,"observed_etag":proof.event.source["etag"].as_str(),"event_cancelled":cancelled,"notification_delivery":"unverified","source_origin":"fetched","source":proof.event.source,"publication_fence":fence,"master_read_proof":stamp});
        self.db.execute("UPDATE calendar_write_operations SET state='reconciled',outcome_json=?3,operation_revision=operation_revision+1 WHERE account=?1 AND request_id=?2 AND execution_mode='live'",params![lease.account,lease.request_id,serde_json::to_string(&evidence)?])?;
        self.calendar_live_status(&lease.account, &lease.request_id)
    }

    /// Fence acquisition which began before an accepted write. Cleanup remains
    /// bounded outside this call, and a newer published revision is never expired.
    pub fn calendar_live_expire_staging(
        &mut self,
        lease: &CalendarWriteLease,
    ) -> Result<Vec<crate::live_calendar::CalendarSyncLease>> {
        let op = self.check_calendar_write_lease(lease)?;
        if !matches!(op.state.as_str(), "accepted" | "reconciled") {
            return Ok(vec![]);
        }
        let fence = self.calendar_read_publication_fence(lease, &op.state)?;
        let generation = fence
            .as_ref()
            .map_or(lease.connection_generation, |f| f.connection_generation);
        let permission = fence
            .as_ref()
            .map_or(lease.permission_epoch, |f| f.permission_epoch);
        let revision = fence
            .as_ref()
            .map_or(lease.dispatch["base_revision"].as_i64().unwrap(), |f| {
                f.source_revision
            });
        let current_generation =
            if op.state == "reconciled" && matches!(document_version(&lease.dispatch)?, 3 | 4) {
                self.calendar_occurrence_read_connection(&lease.account)?
            } else {
                live(self, &lease.account)?
            };
        if current_generation != generation {
            return Ok(vec![]);
        }
        let calendar = crate::string(&lease.dispatch["target"], "calendar")?;
        let scope = self.calendar_sync_scope(&lease.account, calendar)?;
        if scope["membership_denied"] == true
            || scope["permission_epoch"] != permission
            || scope["revision"] != revision
        {
            return Ok(vec![]);
        }
        if is_instance_dispatch(&lease.dispatch)
            && !self.calendar_occurrence_publication_current(lease, &op.state)?
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
                    permission
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
        if !matches!(op.state.as_str(), "accepted" | "reconciled") {
            return self.calendar_live_status(&lease.account, &lease.request_id);
        }
        let already_published: bool = self.db.query_row("SELECT result_json IS NOT NULL FROM calendar_write_operations WHERE account=?1 AND request_id=?2", params![lease.account,lease.request_id], |r| r.get(0))?;
        if already_published {
            return self.calendar_live_status(&lease.account, &lease.request_id);
        }
        let current_generation =
            if op.state == "reconciled" && matches!(document_version(&lease.dispatch)?, 3 | 4) {
                self.calendar_occurrence_read_connection(&lease.account)?
            } else {
                live(self, &lease.account)?
            };
        let target: EventKey = serde_json::from_value(lease.dispatch["target"].clone())?;
        let scope = self.calendar_sync_scope(&lease.account, &target.calendar)?;
        let fence = self.calendar_read_publication_fence(lease, &op.state)?;
        let generation = fence
            .as_ref()
            .map_or(lease.connection_generation, |f| f.connection_generation);
        let permission = fence
            .as_ref()
            .map_or(lease.permission_epoch, |f| f.permission_epoch);
        let revision = fence
            .as_ref()
            .map_or(lease.dispatch["base_revision"].as_i64().unwrap(), |f| {
                f.source_revision
            });
        // A completed full catalog can omit this calendar without changing its
        // event-scope stamps. That membership loss also fences cache publication.
        let lifecycle_unchanged = document_version(&lease.dispatch)? != 4
            || lease.dispatch["lifecycle_revision"]
                .as_i64()
                .is_some_and(|revision| {
                    self.check_active_account_revision(&lease.account, revision)
                        .is_ok()
                });
        let authority_unchanged = lifecycle_unchanged
            && current_generation == generation
            && scope["permission_epoch"] == permission
            && scope["membership_denied"] != true
            && (!is_instance_dispatch(&lease.dispatch)
                || self.calendar_occurrence_publication_current(lease, &op.state)?);
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
            if is_instance_dispatch(&lease.dispatch) {
                json!({"id":target.event,"status":"cancelled","recurringEventId":lease.dispatch["occurrence"]["series_id"],"originalStartTime":lease.dispatch["original_start_provider"]})
            } else {
                json!({"id":target.event,"status":"cancelled"})
            }
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
        let revision_matches = scope["revision"] == revision;
        let publish = authority_unchanged
            && if fence.is_some() {
                revision_matches
            } else {
                unchanged && (revision_matches || current.as_ref() == Some(&source))
            };
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
            let observed = if is_instance_dispatch(&lease.dispatch) && outcome["kind"] == "deleted"
            {
                serde_json::to_string(
                    &json!({"kind":"acknowledged_occurrence_cancellation","request_id":lease.request_id,"source":source}),
                )?
            } else {
                raw.clone()
            };
            tx.execute("UPDATE calendar_sync_state SET revision=revision+1 WHERE account=?1 AND calendar_id=?2",params![lease.account,target.calendar])?;
            tx.execute("INSERT INTO calendar_sources(account,calendar_id,event_id,json,source_generation) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(account,calendar_id,event_id) DO UPDATE SET json=excluded.json,source_generation=excluded.source_generation",params![lease.account,target.calendar,target.event,raw,generation])?;
            crate::live_calendar::index_event(&tx, &event, 0, generation)?;
            tx.execute("INSERT INTO calendar_event_state(account,calendar_id,event_id,visible,observed_json,seen_full_generation) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(account,calendar_id,event_id) DO UPDATE SET visible=excluded.visible,observed_json=excluded.observed_json,seen_full_generation=excluded.seen_full_generation",params![lease.account,target.calendar,target.event,!event.cancelled,observed,full])?;
            tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        }
        let state = if op.state == "reconciled" {
            "reconciled"
        } else {
            "acknowledged"
        };
        let receipt = json!({"request_id":lease.request_id,"target":target,"state":state,"cache_published":publish,"local_only":false});
        tx.execute("UPDATE calendar_write_operations SET state=?4,result_json=?3,operation_revision=operation_revision+1 WHERE account=?1 AND request_id=?2",params![lease.account,lease.request_id,serde_json::to_string(&receipt)?,state])?;
        tx.commit()?;
        self.calendar_live_status(&lease.account, &lease.request_id)
    }

    fn calendar_read_publication_fence(
        &self,
        lease: &CalendarWriteLease,
        state: &str,
    ) -> Result<Option<CalendarReadPublicationFence>> {
        if state != "reconciled" || !matches!(document_version(&lease.dispatch)?, 2..=4) {
            return Ok(None);
        }
        let raw: Option<String> = self.db.query_row(
            "SELECT json_extract(outcome_json,'$.publication_fence') FROM calendar_write_operations WHERE account=?1 AND request_id=?2",
            params![lease.account, lease.request_id], |row| row.get(0),
        )?;
        let fence = raw
            .map(|raw| serde_json::from_str::<CalendarReadPublicationFence>(&raw))
            .transpose()?;
        if let Some(fence) = &fence {
            fence.check(lease)?;
        }
        Ok(fence)
    }

    fn calendar_occurrence_publication_current(
        &self,
        lease: &CalendarWriteLease,
        state: &str,
    ) -> Result<bool> {
        let stamp: MasterStamp = if state == "reconciled" {
            let raw:String=self.db.query_row("SELECT CASE WHEN length(CAST(json_extract(outcome_json,'$.master_read_proof') AS BLOB))<=16384 THEN json_extract(outcome_json,'$.master_read_proof') END FROM calendar_write_operations WHERE account=?1 AND request_id=?2",params![lease.account,lease.request_id],|r|r.get(0))?;
            serde_json::from_str(&raw)?
        } else {
            serde_json::from_value(lease.dispatch["master_proof"].clone())?
        };
        Ok(self.check_calendar_occurrence_master(&stamp).is_ok())
    }

    pub(crate) fn check_calendar_write_lease(
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

impl CalendarReadPublicationFence {
    fn check(&self, lease: &CalendarWriteLease) -> Result<()> {
        if self.account != lease.account
            || self.request_id != lease.request_id
            || self.attempt != lease.attempt
            || self.calendar != lease.dispatch["target"]["calendar"]
        {
            return Err(err(
                "stale_revision",
                "Calendar observation belongs to another attempt",
            ));
        }
        Ok(())
    }
}

pub(crate) fn is_instance_dispatch(dispatch: &Value) -> bool {
    dispatch["version"] == 3
        || (dispatch["version"] == 4 && dispatch["recurring"]["scope"] == "occurrence")
}

fn occurrence_request(request: &Value) -> Result<CalendarOccurrenceSelector> {
    let occurrence = &request["occurrence"];
    crate::fields(
        occurrence,
        &["series_id", "original_start", "expected_master_etag"],
    )?;
    etag(crate::string(occurrence, "expected_master_etag")?)?;
    CalendarOccurrenceSelector::parse(
        &json!({"calendar_id":request["calendar_id"],"series_id":occurrence["series_id"],"original_start":occurrence["original_start"],"event_id":request["event_id"]}),
    )
}
pub(crate) fn proof_matches_selector(
    proof: &PreparedCalendarOccurrence,
    selector: &CalendarOccurrenceSelector,
) -> bool {
    let stamp = &proof.master.stamp;
    stamp.calendar == selector.calendar_id()
        && stamp.series == selector.series_id()
        && selector.event_id() == Some(proof.event.key.event.as_str())
        && proof
            .event
            .original_start
            .as_ref()
            .is_some_and(|v| same_original(v, selector.original_start()))
}
fn matches_occurrence_source(source: &Value, dispatch: &Value) -> Result<bool> {
    let wanted = crate::live_calendar_occurrences::parse_original(
        &dispatch["occurrence"]["original_start"],
    )?;
    let target: EventKey = serde_json::from_value(dispatch["target"].clone())?;
    if source["id"] != target.event
        || source["recurringEventId"] != dispatch["occurrence"]["series_id"]
    {
        return Ok(false);
    }
    let stamp: MasterStamp = serde_json::from_value(dispatch["master_proof"].clone())?;
    Ok(
        fm_calendar::normalize(&target.account, &target.calendar, &stamp.zone, source).is_ok_and(
            |event| {
                event
                    .original_start
                    .as_ref()
                    .is_some_and(|v| same_original(v, &wanted))
            },
        ),
    )
}

pub(crate) fn live(store: &Store, account: &str) -> Result<i64> {
    store.require_active_account(account)?;
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
        || !fm_google::auth::ScopeSet::new(&binding.scopes).calendar_events_write()
        || !crate::preferences::google_write_enabled(&store.db, account, "calendar")?
    {
        return Err(err(
            "permission_denied",
            "Google Calendar write scope is unavailable",
        ));
    }
    Ok(binding.connection_generation)
}
pub(crate) fn etag(value: &str) -> Result<()> {
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
pub(crate) fn bounded_source(source: &Value, target: &EventKey) -> Result<()> {
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
pub(crate) fn live_source_writable(access: Access) -> bool {
    matches!(
        access,
        Access::Writer | Access::Owner | Access::WriterWithoutPrivateAccess
    )
}
pub(crate) fn live_event_actions(event: &Event, access: Access, actor: &str) -> (bool, bool) {
    let versioned = etag(&event.etag).is_ok();
    (
        versioned && initial_event(event, access, "edit", 2).is_ok(),
        versioned
            && initial_event(event, access, "rsvp", 2).is_ok()
            && live_self_attendee(event, actor).is_ok(),
    )
}
pub(crate) fn live_self_attendee<'a>(event: &'a Event, actor: &str) -> Result<&'a Value> {
    let mut matches = event.attendees.iter().filter(|person| {
        person["self"] == true
            && person["email"]
                .as_str()
                .is_some_and(|email| email.eq_ignore_ascii_case(actor))
    });
    match (matches.next(), matches.next()) {
        (Some(person), None) => Ok(person),
        _ => Err(err(
            "permission_denied",
            "Only the authenticated attendee can respond",
        )),
    }
}
pub(crate) fn initial_event(event: &Event, access: Access, kind: &str, version: u64) -> Result<()> {
    if !matches!(access.for_event(event), Access::Writer | Access::Owner) {
        return Err(err("permission_denied", "This event copy is not writable"));
    }
    if event.cancelled
        || (version != 4 && !event.recurrence.is_empty())
        || (!matches!(version, 3 | 4) && event.series_id.is_some())
        || (version == 3
            && (event.series_id.is_none()
                || event.original_start.is_none()
                || event.series_private.is_none()))
        || event.source["locked"] == true
        || !matches!(event.source["eventType"].as_str(), None | Some("default"))
    {
        return Err(err(
            "unsupported_operation",
            "This write slice supports ordinary nonrecurring events",
        ));
    }
    if kind != "rsvp"
        && ((version == 1
            && (!event.attendees.is_empty() || event.source["attendeesOmitted"] == true))
            || event.source["organizer"]["self"] != true)
    {
        return Err(err(
            "unsupported_operation",
            "This operation requires a supported organizer copy",
        ));
    }
    if matches!(version, 2..=4) && !fm_calendar::ordinary_conference_supported(&event.source) {
        return Err(err(
            "unsupported_operation",
            "This provider meeting cannot be safely modified",
        ));
    }
    Ok(())
}
fn validate_fields_version(fields: &Value, version: u64) -> Result<()> {
    if version == 4 {
        let mut ordinary = fields.clone();
        ordinary
            .as_object_mut()
            .ok_or_else(|| err("invalid_request", "Event fields must be an object"))?
            .remove("recurrence");
        if let Some(rule) = fields.get("recurrence") {
            crate::live_calendar_recurring::validate_rule_value(rule, true)?;
        }
        if ordinary.as_object().is_some_and(|v| v.is_empty()) && fields.get("recurrence").is_some()
        {
            return Ok(());
        }
        return validate_fields_version(&ordinary, 2);
    }
    let mut allowed = vec![
        "title",
        "start",
        "end",
        "all_day",
        "time_zone",
        "location",
        "description",
        "conference_url",
        "attendees",
    ];
    if version >= 2 {
        allowed.extend(crate::calendar_editor_options::FIELDS);
    }
    crate::fields(fields, &allowed)?;
    if fields.as_object().is_none_or(|v| v.is_empty()) {
        return Err(err("invalid_request", "Choose event fields to change"));
    }
    Ok(())
}
pub(crate) fn field_patch(
    request_id: &str,
    fields: &Value,
    event: Option<&Event>,
    zone: &str,
    actor: &str,
    version: u64,
) -> Result<Value> {
    validate_fields_version(fields, version)?;
    let mut body = if version >= 2 {
        crate::calendar_editor_options::patch(fields, request_id)?
    } else {
        json!({})
    };
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
        if matches!(version, 2 | 4) {
            if let Some(patch) = crate::live_calendar_invitations::attendee_patch(
                attendees,
                event.map(|e| &e.source),
                actor,
            )? {
                body["attendees"] = patch;
            }
        } else if !attendees.as_array().is_some_and(Vec::is_empty) {
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
        let mut start = times(start);
        let mut end = times(end);
        let converting = event.is_some_and(|e| e.source["start"].get("date").is_some() != all_day);
        if converting && fields["convert_time_kind"] != true {
            return Err(err(
                "unsupported_operation",
                "Confirm timed/all-day conversion with complete endpoints",
            ));
        }
        let candidate = json!({"id":"validate","start":start,"end":end});
        fm_calendar::normalize("validate", "validate", selected_zone, &candidate)
            .map_err(crate::calendar_store::calendar_error)?;
        if converting {
            // PATCH merges nested objects; clear the opposite kind explicitly.
            for endpoint in [&mut start, &mut end] {
                if all_day {
                    endpoint["dateTime"] = Value::Null;
                    endpoint["timeZone"] = Value::Null;
                } else {
                    endpoint["date"] = Value::Null;
                }
            }
        }
        if event.is_none_or(|e| e.source["start"] != start) {
            body["start"] = start;
        }
        if event.is_none_or(|e| e.source["end"] != end) {
            body["end"] = end;
        }
    }
    if version == 4 {
        if let Some(recurrence) = fields.get("recurrence") {
            crate::live_calendar_recurring::validate_rule_value(recurrence, event.is_some())?;
            if event.is_none_or(|e| e.source["recurrence"] != *recurrence) {
                body["recurrence"] = recurrence.clone();
            }
        }
    }
    if matches!(version, 2 | 4) {
        if let Some(event) = event {
            preserve_label(&mut body, &event.source)?;
        }
    }
    Ok(body)
}

pub(crate) fn document_version(document: &Value) -> Result<u64> {
    match document.get("version") {
        None => Ok(1),
        Some(value) if value == 1 || value == 2 || value == 3 || value == 4 => {
            Ok(value.as_u64().unwrap())
        }
        _ => Err(err(
            "unsupported_operation",
            "This Calendar intent version is not supported",
        )),
    }
}

pub(crate) fn preserve_label(body: &mut Value, source: &Value) -> Result<()> {
    if let Some(label) = source.get("eventLabelId") {
        let text = label
            .as_str()
            .filter(|v| !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control))
            .ok_or_else(|| {
                err(
                    "unsupported_operation",
                    "This provider label cannot be safely preserved",
                )
            })?;
        body["eventLabelId"] = text.into();
    } else if let Some(color) = source.get("colorId") {
        let text = color
            .as_str()
            .filter(|v| !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control))
            .ok_or_else(|| {
                err(
                    "unsupported_operation",
                    "This provider color cannot be safely preserved",
                )
            })?;
        body["colorId"] = text.into();
    }
    Ok(())
}

pub(crate) fn live_guest_actions(
    event: &Event,
    access: Access,
) -> Result<(Vec<Value>, Vec<Value>)> {
    etag(&event.etag)?;
    initial_event(event, access, "edit", 2)?;
    crate::live_calendar_invitations::guest_projection(&event.source)
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
fn matches_patch_v2(source: &Value, patch: &Value) -> bool {
    matches_patch_v2_roster(source, patch, false)
}

fn matches_patch_v2_roster(source: &Value, patch: &Value, created: bool) -> bool {
    let Some(fields) = patch.as_object() else {
        return false;
    };
    fields.iter().all(|(key, value)| match key.as_str() {
        "start" | "end" => same_time(&source[key], value),
        "location" | "description" => match source.get(key) {
            None => value == "",
            Some(actual) => actual == value,
        },
        "conferenceData" if value.is_null() => source.get(key).is_none_or(Value::is_null),
        "reminders" if value["useDefault"] == false && value["overrides"] == json!([]) => {
            source[key]["useDefault"] == false
                && source[key]
                    .get("overrides")
                    .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty))
        }
        "conferenceData" if value["createRequest"].is_object() => {
            source["conferenceData"]["createRequest"]["requestId"]
                == value["createRequest"]["requestId"]
                && source["conferenceData"]["createRequest"]["conferenceSolutionKey"]
                    == value["createRequest"]["conferenceSolutionKey"]
                && source["conferenceData"]["createRequest"]["status"]["statusCode"] == "success"
        }
        "attendeesOmitted" => true,
        "attendees" => {
            if created {
                crate::live_calendar_invitations::matches_created_attendees(source, patch)
            } else {
                crate::live_calendar_invitations::matches_attendees(source, patch)
            }
        }
        _ => source[key] == *value,
    })
}

pub(crate) fn matches_intent_v2(event: &Event, intent: &Value, dispatch: &Value) -> bool {
    let source = &event.source;
    if dispatch["version"] == 4 && dispatch["step"] == 0 {
        return matches_patch_v2(source, &dispatch["body"]);
    }
    let fields = &intent["request"]["fields"];
    let created = dispatch["kind"] == "create";
    let mut comparison = dispatch["body"].clone();
    if !comparison.is_object() {
        return false;
    }
    // Request values may have been omitted from PATCH as unchanged. They still
    // constrain a claim that the observed event matches this recorded intent.
    for (input, provider) in [
        ("title", "summary"),
        ("location", "location"),
        ("description", "description"),
        ("recurrence", "recurrence"),
    ] {
        if let Some(value) = fields.get(input) {
            comparison[provider] = value.clone();
        }
    }
    if ["start", "end", "all_day", "time_zone"]
        .iter()
        .any(|key| fields.get(key).is_some())
    {
        for key in ["start", "end"] {
            if comparison.get(key).is_none() {
                comparison[key] = dispatch["baseline"][key].clone();
            }
        }
    }
    if let Ok(options) = crate::calendar_editor_options::patch(
        fields,
        intent["request"]["request_id"].as_str().unwrap_or(""),
    ) {
        for (key, value) in options.as_object().unwrap() {
            comparison[key] = value.clone();
        }
    } else {
        return false;
    }
    if let Some(url) = fields.get("conference_url") {
        if url.as_str() != Some(event.conference_url.as_deref().unwrap_or("")) {
            return false;
        }
    }
    if comparison.get("attendees").is_none() {
        if created {
            comparison["attendees"] = json!([]);
        } else if fields.get("attendees").is_some() {
            let Some(people) =
                crate::live_calendar_invitations::unchanged_attendees(&dispatch["baseline"])
            else {
                return false;
            };
            comparison["attendees"] = people;
        }
    }
    if dispatch["version"] == 4 && dispatch["kind"] == "rsvp" {
        matches_patch(source, &comparison)
    } else if created {
        matches_patch_v2_roster(source, &comparison, true)
    } else {
        matches_patch_v2(source, &comparison)
    }
}
fn same_time(actual: &Value, wanted: &Value) -> bool {
    if let Some(date) = wanted["date"].as_str() {
        return actual["date"] == date && actual["dateTime"].is_null();
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
#[path = "live_calendar_invitations_tests.rs"]
mod invitation_tests;
#[cfg(test)]
#[path = "live_calendar_occurrences_tests.rs"]
mod occurrence_tests;
#[cfg(test)]
#[path = "live_calendar_writes_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "live_calendar_recurring_tests.rs"]
mod recurring_tests;
