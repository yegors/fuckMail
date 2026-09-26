//! Explicit version-four recurring actions reuse the conditional Calendar journal.
//! Two-step following edits retain one frozen plan and never retry unknown work.
//!
//! A recurring action has a scope. `series` patches or deletes the master
//! itself. `occurrence` changes one instance, like the single-occurrence path.
//! `following` ("this and following events") has no single Google API call,
//! so it runs as two requests: step 0 trims the master's rule to end before
//! the selected occurrence, and step 1 creates a replacement series from that
//! occurrence onward with a deterministic event ID. Step 1 is never sent
//! automatically after an interruption; the row waits in `partial` until the
//! user resumes, and the trimmed master is re-read first.
use crate::calendar_store::{calendar_error, fingerprint, parse_access};
use crate::live_calendar_occurrences::{
    CalendarOccurrenceSelector, MasterStamp, PreparedCalendarOccurrence,
};
use crate::live_calendar_writes::{self as writes, CalendarWriteLease, CalendarWriteOutcome};
use crate::{err, Result, Store};
use fm_calendar::{Event, EventKey};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

/// User-facing consequences of a "this and following" change. A following
/// request is refused with `confirmation_required` and this text until it
/// sets `reset_future_exceptions: true`.
pub const FOLLOWING_NOTICE: &str = "This changes this and following events. Google resets later exceptions. Earlier exceptions are retained. An existing Google Meet is replaced with a new link. The trim and replacement are separate requests; an interrupted operation may need explicit recovery.";

/// Parses a Google `recurrence` array and checks it is the one authorable
/// shape: a single `RRULE:` line with daily, weekly, monthly or yearly
/// frequency and a small set of parts. `allow_empty` permits `[]`, which
/// removes recurrence from an existing series.
pub fn validate_rule_value(value: &Value, allow_empty: bool) -> Result<Vec<String>> {
    let rules: Vec<String> = serde_json::from_value(value.clone())?;
    fm_calendar::recurring::validate_rule_shape(&rules, allow_empty).map_err(calendar_error)?;
    Ok(rules)
}
/// Validates the `recurring` selector of a version-four request. Series
/// actions must target the master itself with its ETag, rule changes are only
/// allowed on the whole series, and following actions need the explicit
/// exception-reset confirmation.
pub(crate) fn validate_request(request: &Value) -> Result<()> {
    if request.get("occurrence").is_some() {
        return Err(err("invalid_request", "Choose one recurring scope"));
    }
    let kind = crate::string(request, "kind")?;
    if kind == "create" {
        if request.get("recurring").is_some() {
            return Err(err(
                "invalid_request",
                "A new recurring event has no existing series selector",
            ));
        }
        validate_rule_value(&request["fields"]["recurrence"], false)?;
        return Ok(());
    }
    let context = &request["recurring"];
    crate::fields(
        context,
        &[
            "scope",
            "series_id",
            "original_start",
            "expected_master_etag",
            "reset_future_exceptions",
        ],
    )?;
    let scope = crate::string(context, "scope")?;
    if !matches!(scope, "occurrence" | "series" | "following") {
        return Err(err(
            "invalid_request",
            "Choose occurrence, series or following",
        ));
    }
    crate::live_calendar::provider_id(crate::string(context, "series_id")?)?;
    writes::etag(crate::string(context, "expected_master_etag")?)?;
    if scope != "series" || context.get("original_start").is_some() {
        crate::live_calendar_occurrences::parse_original(&context["original_start"])?;
    }
    if scope == "series"
        && (request["event_id"] != context["series_id"]
            || request["expected_etag"] != context["expected_master_etag"])
    {
        return Err(err(
            "scope_mismatch",
            "Series operations target the exact resolved master",
        ));
    }
    if scope == "following" {
        if !matches!(kind, "edit" | "delete") {
            return Err(err(
                "unsupported_operation",
                "Following scope supports organizer changes or cancellation",
            ));
        }
        if context["reset_future_exceptions"] != true {
            return Err(err("confirmation_required", FOLLOWING_NOTICE));
        }
    } else if context.get("reset_future_exceptions").is_some() {
        return Err(err(
            "invalid_request",
            "Exception-reset confirmation belongs only to following changes",
        ));
    }
    if request["fields"].get("recurrence").is_some() && scope != "series" {
        return Err(err(
            "unsupported_operation",
            "Change the recurrence rule using the entire-series action",
        ));
    }
    Ok(())
}
/// Checks a series-scope or recurring-create dispatch before it is frozen: the
/// baseline must be a real recurring master, and when the rule or time kind
/// changes, the patched event must still yield a first occurrence.
pub(crate) fn validate_series_dispatch(request: &Value, dispatch: &Value) -> Result<()> {
    if request["kind"] != "create" {
        if request["recurring"]["scope"] != "series" {
            return Err(err(
                "invalid_request",
                "Occurrence and following actions require a qualified instance",
            ));
        }
        if dispatch["baseline"]["recurrence"]
            .as_array()
            .is_none_or(Vec::is_empty)
            || dispatch["baseline"].get("recurringEventId").is_some()
        {
            return Err(err(
                "scope_mismatch",
                "The selected provider source is not a recurring master",
            ));
        }
    }
    let mut candidate = if request["kind"] == "create" {
        dispatch["body"].clone()
    } else {
        dispatch["baseline"].clone()
    };
    if request["kind"] != "delete"
        && (request["kind"] == "create"
            || request["fields"].get("recurrence").is_some()
            || request["fields"]["convert_time_kind"] == true)
    {
        if let Some(patch) = dispatch["body"].as_object() {
            for (k, v) in patch {
                candidate[k] = v.clone();
            }
        }
        candidate["id"] = dispatch["target"]["event"].clone();
        crate::calendar_editor_options::clear_endpoint_nulls(&mut candidate);
        let event = fm_calendar::normalize("validate", "validate", "UTC", &candidate)
            .map_err(calendar_error)?;
        if let Some(start) = event.start.as_ref() {
            fm_calendar::recurring::validate_rule(&event.recurrence, start)
                .map_err(calendar_error)?;
        }
    }
    Ok(())
}

/// Flat read selector extends the old occurrence RPC only when scope is explicit.
pub fn read_selector(params: &Value) -> Result<(CalendarOccurrenceSelector, String, String)> {
    crate::fields(
        params,
        &[
            "calendar_id",
            "series_id",
            "original_start",
            "event_id",
            "scope",
            "action",
        ],
    )?;
    let scope = crate::string(params, "scope")?;
    let action = crate::string(params, "action")?;
    if !matches!(scope, "occurrence" | "series" | "following")
        || !matches!(action, "edit" | "delete" | "rsvp")
        || (scope == "following" && action == "rsvp")
    {
        return Err(err("unsupported_operation", "Unsupported recurring action"));
    }
    let mut selector = params.clone();
    selector.as_object_mut().unwrap().remove("scope");
    selector.as_object_mut().unwrap().remove("action");
    Ok((
        CalendarOccurrenceSelector::parse(&selector)?,
        scope.into(),
        action.into(),
    ))
}

impl Store {
    /// Loads the retained master named by a qualified stamp, failing with
    /// `stale_revision` if its stamps or bytes changed since qualification.
    fn recurring_master_event(&self, stamp: &MasterStamp) -> Result<Event> {
        self.check_calendar_occurrence_master(stamp)?;
        let raw:String=self.db.query_row("SELECT CASE WHEN length(CAST(json AS BLOB))<=262144 THEN json END FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![stamp.account,stamp.calendar,stamp.series],|r|r.get(0))?;
        let source: Value = serde_json::from_str(&raw)?;
        if fingerprint(&source)? != stamp.source_sha256 {
            return Err(err("stale_revision", "Retained recurring source changed"));
        }
        fm_calendar::normalize(&stamp.account, &stamp.calendar, &stamp.zone, &source)
            .map_err(calendar_error)
    }
    /// Returns the editor row for a recurring `action` (`edit`, `delete` or
    /// `rsvp`) at `scope`, plus the `recurring` selector the later write must
    /// send.
    ///
    /// Series scope shows the master; occurrence and following scopes show
    /// the verified instance. Both the master and the chosen event must pass
    /// the write checks for `action`, and a following preview also checks that
    /// the rule can be split there and the series can be copied into a
    /// replacement. The result includes [`FOLLOWING_NOTICE`] for following
    /// scope.
    pub fn calendar_live_recurring_read_finish(
        &self,
        proof: PreparedCalendarOccurrence,
        scope: &str,
        action: &str,
    ) -> Result<Value> {
        let stamp = &proof.master.stamp;
        writes::live(self, &stamp.account)?;
        let master = self.recurring_master_event(stamp)?;
        writes::initial_event(&master, parse_access(&stamp.access)?, action, 4)?;
        let actor: String = self.db.query_row(
            "SELECT email FROM accounts WHERE id=?1",
            [&stamp.account],
            |r| r.get(0),
        )?;
        let chosen = if scope == "series" {
            &master
        } else {
            &proof.event
        };
        writes::initial_event(chosen, parse_access(&stamp.access)?, action, 4)?;
        writes::etag(&chosen.etag)?;
        if action == "rsvp" {
            writes::live_self_attendee(chosen, &actor)?;
        }
        if scope == "following" {
            fm_calendar::recurring::split_rule(&master, proof.selector().original_start())
                .map_err(calendar_error)?;
            replacement_source(&master.source, "preview")?;
            replacement_instance_source(&proof.event.source, "preview")?;
        }
        let display_zone = self
            .calendar_display_zone()?
            .unwrap_or_else(|| stamp.zone.clone());
        let zone = display_zone
            .parse()
            .map_err(|_| err("invalid_request", "Calendar zone is unavailable"))?;
        let from = chosen
            .start
            .as_ref()
            .ok_or_else(|| err("unsupported_operation", "Event has no start"))?
            .utc(zone)
            .map_err(calendar_error)?;
        let to = chosen
            .end
            .as_ref()
            .ok_or_else(|| err("unsupported_operation", "Event has no end"))?
            .utc(zone)
            .map_err(calendar_error)?;
        let window = fm_calendar::Window::new(from, to, zone).map_err(calendar_error)?;
        // Project the chosen event once, without expanding the master's rule,
        // so the editor gets exactly one row.
        let mut single = chosen.clone();
        single.recurrence.clear();
        let mut rows = fm_calendar::project_event_in_window(
            &single,
            &[],
            &display_zone,
            window,
            parse_access(&stamp.access)?,
        )
        .map_err(calendar_error)?;
        if rows.len() != 1 {
            return Err(err(
                "unsupported_operation",
                "Event cannot be represented by the editor",
            ));
        }
        let mut event = rows.remove(0);
        crate::calendar_editor_options::project(&mut event, &chosen.source);
        event["account"] = stamp.account.clone().into();
        event["calendar_id"] = stamp.calendar.clone().into();
        event["event_id"] = chosen.key.event.clone().into();
        event["etag"] = chosen.etag.clone().into();
        event["recurring"] = true.into();
        event["recurrence"] = serde_json::to_value(&master.recurrence)?;
        event["can_edit"] = (action == "edit").into();
        event["can_delete"] = (action == "delete").into();
        event["can_rsvp"] = (action == "rsvp").into();
        match crate::live_calendar_invitations::guest_projection(&chosen.source) {
            Ok((guests, protected)) => {
                event["can_edit_attendees"] = (action == "edit").into();
                event["attendee_edit_reason"] = Value::Null;
                event["guest_addresses"] = guests.into();
                event["protected_attendees"] = protected.into();
            }
            Err(e) => {
                event["can_edit_attendees"] = false.into();
                event["attendee_edit_reason"] = e.code.into();
                event["guest_addresses"] = json!([]);
                event["protected_attendees"] = json!([]);
            }
        }
        let mut recurring =
            json!({"scope":scope,"series_id":stamp.series,"expected_master_etag":stamp.etag});
        if scope != "series" {
            recurring["original_start"] = serde_json::to_value(proof.selector().original_start())?;
        }
        let mut result = json!({"account":stamp.account,"calendar_id":stamp.calendar,"event_id":chosen.key.event,"etag":chosen.etag,"recurring":recurring,"event":event});
        if scope == "following" {
            result["following_notice"] = FOLLOWING_NOTICE.into();
        }
        Ok(result)
    }

    /// Freezes the dispatch for an occurrence- or following-scope recurring
    /// action from a verified occurrence proof. `revision` is the row's
    /// expected operation revision.
    ///
    /// For following scope the frozen dispatch is step 0 (the master trim),
    /// and the replacement series body is computed and validated now so a
    /// later resume cannot change it. Fails with `stale_revision` when the
    /// resolved instance or master differs from the request, and with
    /// `calendar_operation_pending` if the row was already attempted.
    pub fn calendar_live_stage_recurring(
        &mut self,
        account: &str,
        request_id: &str,
        revision: i64,
        proof: PreparedCalendarOccurrence,
    ) -> Result<Value> {
        let op = self.calendar_live_operation(account, request_id)?;
        if op.intent["version"] != 4
            || op.version != revision
            || !matches!(op.state.as_str(), "prepared" | "ready")
        {
            return Err(err(
                "calendar_operation_pending",
                "Recurring request changed or already attempted",
            ));
        }
        let request = &op.intent["request"];
        let recurring = &request["recurring"];
        let stamp = &proof.master.stamp;
        if stamp.account != account
            || recurring["series_id"] != stamp.series
            || recurring["expected_master_etag"] != stamp.etag
            || request["event_id"] != proof.event.key.event
            || request["expected_etag"] != proof.event.etag
        {
            return Err(err(
                "stale_revision",
                "Resolved recurring event or master changed",
            ));
        }
        let wanted =
            crate::live_calendar_occurrences::parse_original(&recurring["original_start"])?;
        if !crate::live_calendar_occurrences::same_original(
            &wanted,
            proof.selector().original_start(),
        ) {
            return Err(err("scope_mismatch", "Recurring original start differs"));
        }
        if writes::live(self, account)? != stamp.connection_generation {
            return Err(err("stale_revision", "Calendar connection changed"));
        }
        let master = self.recurring_master_event(stamp)?;
        let kind = crate::string(request, "kind")?;
        writes::initial_event(&master, parse_access(&stamp.access)?, kind, 4)?;
        writes::initial_event(&proof.event, parse_access(&stamp.access)?, kind, 4)?;
        let actor: String =
            self.db
                .query_row("SELECT email FROM accounts WHERE id=?1", [account], |r| {
                    r.get(0)
                })?;
        let mut body = match kind {
            "edit" => writes::field_patch(
                request_id,
                &request["fields"],
                Some(&proof.event),
                &stamp.zone,
                &actor,
                4,
            )?,
            "rsvp" => {
                let person = writes::live_self_attendee(&proof.event, &actor)?;
                json!({"attendeesOmitted":true,"attendees":[{"email":person["email"],"responseStatus":request["response"]}]})
            }
            "delete" => Value::Null,
            _ => return Err(err("invalid_request", "Invalid recurring action")),
        };
        if kind == "rsvp" {
            writes::preserve_label(&mut body, &proof.event.source)?;
        }
        let mut dispatch = json!({"version":4,"kind":kind,"target":op.intent["target"],"body":body,"if_match":request["expected_etag"],"send_updates":"all","baseline":proof.event.source,"base_revision":stamp.calendar_revision,"event_label_version":if body.get("eventLabelId").is_some(){1}else{0},"recurring":recurring,"occurrence":{"series_id":stamp.series,"original_start":wanted,"expected_master_etag":stamp.etag},"original_start_provider":proof.event.source["originalStartTime"],"master_proof":stamp,"lifecycle_revision":stamp.lifecycle_revision});
        if recurring["scope"] == "following" {
            let split =
                fm_calendar::recurring::split_rule(&master, &wanted).map_err(calendar_error)?;
            // The replacement ID derives from the request, so a retried create
            // can only ever produce this one series.
            let id = format!(
                "f{}",
                fingerprint(&json!([account, stamp.calendar, request_id, "following"]))?
            );
            replacement_source(&master.source, &id)?;
            let mut replacement = replacement_instance_source(&proof.event.source, &id)?;
            replacement["recurrence"] = split.after.into();
            if kind == "edit" {
                for (k, v) in body.as_object().unwrap() {
                    replacement[k] = v.clone();
                }
            }
            let mut candidate = replacement.clone();
            candidate["id"] = id.clone().into();
            crate::calendar_editor_options::clear_endpoint_nulls(&mut candidate);
            let event = fm_calendar::normalize(account, &stamp.calendar, &stamp.zone, &candidate)
                .map_err(calendar_error)?;
            fm_calendar::recurring::validate_rule(&event.recurrence, event.start.as_ref().unwrap())
                .map_err(calendar_error)?;
            // Step 0 retargets the dispatch at the master: a conditional patch
            // of its rule only. The replacement waits inside `following`.
            let mut trim = json!({"recurrence":split.before});
            writes::preserve_label(&mut trim, &master.source)?;
            dispatch["kind"] = "edit".into();
            dispatch["target"] =
                json!({"account":account,"calendar":stamp.calendar,"event":stamp.series});
            dispatch["event_label_version"] = if trim.get("eventLabelId").is_some() {
                json!(1)
            } else {
                json!(0)
            };
            dispatch["body"] = trim;
            dispatch["if_match"] = stamp.etag.clone().into();
            dispatch["selected_instance_baseline"] = proof.event.source.clone();
            dispatch["baseline"] = master.source;
            dispatch["following"] = json!({"replacement_id":id,"replacement_body":if kind=="edit"{replacement}else{Value::Null},"preceding_count":split.preceding_count});
            dispatch["step"] = 0.into();
        }
        if self.db.execute("UPDATE calendar_write_operations SET state='ready',operation_revision=operation_revision+1,connection_generation=?4,permission_epoch=?5,dispatch_json=?6 WHERE account=?1 AND request_id=?2 AND execution_mode='live' AND state IN ('prepared','ready') AND operation_revision=?3",params![account,request_id,revision,stamp.connection_generation,stamp.permission_epoch,serde_json::to_string(&dispatch)?])?!=1 {return Err(err("stale_revision","Recurring operation changed"));}
        self.calendar_live_status(account, request_id)
    }

    /// Extra dispatch-time checks for version-four attempts: the account
    /// lifecycle and catalog membership are unchanged, a following trim's
    /// master proof still holds, and no other work on the series is pending.
    pub(crate) fn calendar_recurring_check_start(&self, lease: &CalendarWriteLease) -> Result<()> {
        self.check_active_account_revision(
            &lease.account,
            lease.dispatch["lifecycle_revision"]
                .as_i64()
                .ok_or_else(|| err("invalid_request", "Missing recurring lifecycle fence"))?,
        )?;
        let calendar = crate::string(&lease.dispatch["target"], "calendar")?;
        let scope = self.calendar_sync_scope(&lease.account, calendar)?;
        if scope["membership_denied"] == true {
            return Err(err(
                "permission_denied",
                "Calendar is no longer in the current catalog",
            ));
        }
        if lease.dispatch["recurring"]["scope"] == "following" && lease.dispatch["step"] == 0 {
            let stamp: MasterStamp =
                serde_json::from_value(lease.dispatch["master_proof"].clone())?;
            self.check_calendar_occurrence_master(&stamp)?;
        }
        self.calendar_recurring_assert_no_pending(
            &lease.account,
            &lease.request_id,
            &lease.dispatch["target"],
            &lease.dispatch["recurring"],
        )
    }

    /// Fails with `calendar_operation_pending` while any other live write on
    /// the same series (by target, occurrence or recurring selector) is
    /// ready, in flight, unknown, partial, or abandoned while unknown. Such
    /// work may still land at Google, so a new series action could race it.
    pub(crate) fn calendar_recurring_assert_no_pending(
        &self,
        account: &str,
        request_id: &str,
        target: &Value,
        recurring: &Value,
    ) -> Result<()> {
        let series = recurring["series_id"]
            .as_str()
            .unwrap_or_else(|| target["event"].as_str().unwrap_or(""));
        let pending:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM calendar_write_operations WHERE account=?1 AND request_id<>?2 AND execution_mode='live' AND (state IN ('ready','in_flight','unknown','partial') OR (state='abandoned' AND json_extract(outcome_json,'$.kind')='unknown')) AND json_extract(intent_json,'$.target.calendar')=?3 AND (json_extract(intent_json,'$.target.event')=?4 OR json_extract(intent_json,'$.request.occurrence.series_id')=?4 OR json_extract(intent_json,'$.request.recurring.series_id')=?4))",params![account,request_id,target["calendar"].as_str(),series],|r|r.get(0))?;
        if pending {
            return Err(err(
                "calendar_operation_pending",
                "Resolve existing work for this series before starting another recurring action",
            ));
        }
        Ok(())
    }

    /// The same guard for non-recurring and single-occurrence writes, which
    /// only have to wait for unresolved version-four work on the series.
    pub(crate) fn calendar_recurring_guard_legacy(
        &self,
        account: &str,
        request_id: &str,
        target: &Value,
        occurrence: &Value,
    ) -> Result<()> {
        let series = occurrence["series_id"]
            .as_str()
            .or_else(|| target["event"].as_str());
        let pending: bool = self.db.query_row("SELECT EXISTS(SELECT 1 FROM calendar_write_operations WHERE account=?1 AND request_id<>?2 AND execution_mode='live' AND json_extract(intent_json,'$.version')=4 AND (state IN ('ready','in_flight','unknown','partial') OR (state='abandoned' AND json_extract(outcome_json,'$.kind')='unknown')) AND json_extract(intent_json,'$.target.calendar')=?3 AND (json_extract(intent_json,'$.target.event')=?4 OR json_extract(intent_json,'$.request.recurring.series_id')=?4))",params![account,request_id,target["calendar"].as_str(),series],|r|r.get(0))?;
        if pending {
            return Err(err(
                "calendar_operation_pending",
                "Resolve existing work for this series before starting another action",
            ));
        }
        Ok(())
    }

    /// Adds version-four fields to a public status: the recurring selector,
    /// the current dispatch step, `can_resume`, and for following actions a
    /// per-step `steps` list (trim, then replacement) with `following_complete`.
    pub(crate) fn calendar_recurring_status(
        &self,
        mut status: Value,
        account: &str,
        request_id: &str,
    ) -> Result<Value> {
        if status["intent_version"] != 4 {
            return Ok(status);
        }
        let (recurring,step,following,prior): (Option<String>,Option<i64>,bool,Option<String>)=self.db.query_row("SELECT CASE WHEN length(CAST(json_extract(intent_json,'$.request.recurring') AS BLOB))<=4096 THEN json_extract(intent_json,'$.request.recurring') END,json_extract(dispatch_json,'$.step'),json_type(dispatch_json,'$.following')='object',json_extract(recurring_steps_json,'$[0].state') FROM calendar_write_operations WHERE account=?1 AND request_id=?2",params![account,request_id],|r|Ok((r.get(0)?,r.get(1)?,r.get::<_,Option<bool>>(2)?.unwrap_or(false),r.get(3)?)))?;
        if let Some(raw) = recurring {
            status["recurring"] = serde_json::from_str(&raw)?;
        }
        let (dispatch_kind, dispatch_event, step_index):(Option<String>,Option<String>,Option<i64>)=self.db.query_row("SELECT json_extract(dispatch_json,'$.kind'),json_extract(dispatch_json,'$.target.event'),json_extract(dispatch_json,'$.step') FROM calendar_write_operations WHERE account=?1 AND request_id=?2",params![account,request_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        status["dispatch_kind"] = dispatch_kind.into();
        status["dispatch_event_id"] = dispatch_event.into();
        status["step_index"] = step_index.into();
        if status["recurring"]["scope"] == "occurrence" {
            status["occurrence"] = json!({"series_id":status["recurring"]["series_id"],"original_start":status["recurring"]["original_start"],"expected_master_etag":status["recurring"]["expected_master_etag"]});
        }
        status["can_resume"] = matches!(
            status["state"].as_str(),
            Some("prepared" | "ready" | "partial")
        )
        .into();
        if following {
            let replacement:Option<String>=self.db.query_row("SELECT json_extract(dispatch_json,'$.following.replacement_id') FROM calendar_write_operations WHERE account=?1 AND request_id=?2",params![account,request_id],|r|r.get(0))?;
            let state = status["state"].clone();
            let observed: bool = self.db.query_row("SELECT COALESCE(json_extract(outcome_json,'$.kind')='observed_state',0) FROM calendar_write_operations WHERE account=?1 AND request_id=?2", params![account,request_id], |r|r.get(0))?;
            let trim_state = if state == "partial" {
                json!(if observed {
                    "reconciled"
                } else {
                    "acknowledged"
                })
            } else {
                state.clone()
            };
            status["steps"] = if status["kind"] == "delete" {
                json!([{"step":"trim","state":trim_state,"target":status["recurring"]["series_id"]}])
            } else if step == Some(1) {
                json!([{"step":"trim","state":prior.unwrap_or_else(||"acknowledged".into()),"target":status["recurring"]["series_id"]},{"step":"replacement","state":state,"target":replacement}])
            } else {
                json!([{"step":"trim","state":trim_state,"target":status["recurring"]["series_id"]},{"step":"replacement","state":"not_attempted","target":replacement}])
            };
            let complete = matches!(state.as_str(), Some("acknowledged" | "reconciled"))
                && (step == Some(1) || status["kind"] == "delete");
            status["following_complete"] = complete.into();
            if step == Some(0)
                && matches!(
                    state.as_str(),
                    Some("accepted" | "acknowledged" | "partial")
                )
            {
                status["outcome"]["kind"] = "trimmed".into();
                status["outcome"]["notification_delivery"] = "unverified".into();
            }
            status["can_resume"] =
                (state == "partial" || state == "ready" || state == "prepared").into();
        }
        Ok(status)
    }
}

fn replacement_instance_source(source: &Value, request_id: &str) -> Result<Value> {
    // Only the parent/original identity belongs to the old recurrence. All
    // content is qualified from the exact instance shown in the editor.
    let mut source = source.clone();
    let fields = source.as_object_mut().ok_or_else(|| {
        err(
            "unsupported_operation",
            "The selected instance is not a complete event",
        )
    })?;
    fields.remove("recurringEventId");
    fields.remove("originalStartTime");
    replacement_source(&source, request_id)
}

/// Builds the create body for a following replacement series from `source`.
///
/// Refuses sources with fields that cannot be faithfully copied (unknown
/// keys, gadgets, extended properties, private copies) or without complete
/// guest metadata. An existing Google Meet cannot be moved to a new event, so
/// it is replaced with a fresh Meet create request keyed by `request_id`.
fn replacement_source(source: &Value, request_id: &str) -> Result<Value> {
    crate::live_calendar_invitations::guest_projection(source)?;
    let allowed = [
        "kind",
        "id",
        "etag",
        "status",
        "htmlLink",
        "created",
        "updated",
        "summary",
        "description",
        "location",
        "colorId",
        "eventLabelId",
        "creator",
        "organizer",
        "start",
        "end",
        "endTimeUnspecified",
        "recurrence",
        "iCalUID",
        "sequence",
        "attendees",
        "attendeesOmitted",
        "reminders",
        "hangoutLink",
        "conferenceData",
        "gadget",
        "anyoneCanAddSelf",
        "guestsCanInviteOthers",
        "guestsCanModify",
        "guestsCanSeeOtherGuests",
        "privateCopy",
        "locked",
        "attachments",
        "eventType",
        "transparency",
        "visibility",
        "extendedProperties",
    ];
    if source
        .as_object()
        .is_none_or(|map| map.keys().any(|key| !allowed.contains(&key.as_str())))
        || source["endTimeUnspecified"] == true
        || source.get("gadget").is_some()
        || source.get("extendedProperties").is_some()
        || source["privateCopy"] == true
    {
        return Err(err(
            "unsupported_operation",
            "This source has provider semantics that cannot be copied into a replacement series",
        ));
    }
    let mut body = json!({});
    for key in [
        "summary",
        "description",
        "location",
        "start",
        "end",
        "colorId",
        "eventLabelId",
        "reminders",
        "attachments",
        "transparency",
        "visibility",
        "guestsCanInviteOthers",
        "guestsCanModify",
        "guestsCanSeeOtherGuests",
    ] {
        if let Some(value) = source.get(key) {
            body[key] = value.clone();
        }
    }
    let people =
        crate::live_calendar_invitations::unchanged_attendees(source).ok_or_else(|| {
            err(
                "unsupported_attendees",
                "Complete guest metadata is required for a following replacement",
            )
        })?;
    body["attendees"] = people;
    if source.get("conferenceData").is_some_and(|v| !v.is_null()) {
        if source["conferenceData"]["conferenceSolution"]["key"]["type"] != "hangoutsMeet" {
            return Err(err(
                "unsupported_operation",
                "Following replacement supports a fresh Google Meet only",
            ));
        }
        body["conferenceData"] = json!({"createRequest":{"requestId":request_id,"conferenceSolutionKey":{"type":"hangoutsMeet"}}});
    }
    Ok(body)
}

impl Store {
    /// Moves a confirmed following-edit trim to `partial` and returns the
    /// status; other operations are returned unchanged.
    ///
    /// Called after recording/publishing a trim. This exposes a never-attempted
    /// replacement, without authorizing a background continuation.
    pub fn calendar_live_recurring_finish_step(
        &mut self,
        lease: &CalendarWriteLease,
    ) -> Result<Value> {
        if lease.dispatch["version"] == 4
            && lease.dispatch["step"] == 0
            && lease.dispatch["following"]["replacement_body"].is_object()
        {
            let op = self.check_calendar_write_lease(lease)?;
            if matches!(
                op.state.as_str(),
                "accepted" | "acknowledged" | "reconciled"
            ) {
                self.db.execute("UPDATE calendar_write_operations SET state='partial',operation_revision=operation_revision+1 WHERE account=?1 AND request_id=?2",params![lease.account,lease.request_id])?;
            }
        }
        self.calendar_live_status(&lease.account, &lease.request_id)
    }
    /// Freezes step 1 (the replacement create) of a following edit and moves
    /// the row back to `ready`.
    ///
    /// Only a new explicit resume (or the still-running original action) may
    /// advance the unattempted second step after a fresh exact trim readback.
    /// `fresh_master` must equal the trimmed master recorded as the trim's
    /// outcome, or it fails with `stale_revision`. The trim's lease and outcome
    /// move to `recurring_steps_json` so a late reply can still be matched.
    pub fn calendar_live_following_advance(
        &mut self,
        account: &str,
        request_id: &str,
        generation: i64,
        epoch: i64,
        fresh_master: Value,
    ) -> Result<Value> {
        if writes::live(self, account)? != generation {
            return Err(err("stale_revision", "Calendar connection changed"));
        }
        let op = self.calendar_live_operation(account, request_id)?;
        if !matches!(op.state.as_str(), "partial" | "ready") {
            return Err(err(
                "calendar_operation_pending",
                "Only a confirmed trim with an unattempted replacement may advance",
            ));
        }
        let current_lease = op
            .lease
            .ok_or_else(|| err("invalid_request", "Following trim lease is missing"))?;
        let prior_raw:Option<String>=self.db.query_row("SELECT recurring_steps_json FROM calendar_write_operations WHERE account=?1 AND request_id=?2",params![account,request_id],|r|r.get(0))?;
        let prior_existing = prior_raw
            .map(|v| serde_json::from_str::<Value>(&v))
            .transpose()?;
        // A second advance after step 1 was already frozen reads the trim
        // lease back from the step history instead of the current dispatch.
        let lease = if current_lease.dispatch["step"] == 1 && op.state == "ready" {
            serde_json::from_value(
                prior_existing
                    .as_ref()
                    .ok_or_else(|| err("invalid_request", "Missing trim history"))?[0]["lease"]
                    .clone(),
            )?
        } else {
            current_lease.clone()
        };
        if lease.dispatch["version"] != 4
            || lease.dispatch["step"] != 0
            || !lease.dispatch["following"]["replacement_body"].is_object()
        {
            return Err(err(
                "invalid_request",
                "No pending following replacement exists",
            ));
        }
        self.check_active_account_revision(
            account,
            lease.dispatch["lifecycle_revision"]
                .as_i64()
                .ok_or_else(|| err("invalid_request", "Lifecycle fence is missing"))?,
        )?;
        let mut scope = self.calendar_sync_scope(
            account,
            crate::string(&lease.dispatch["target"], "calendar")?,
        )?;
        if scope["permission_epoch"] != epoch
            || scope["membership_denied"] == true
            || scope["full_generation"].as_i64().unwrap_or(0) == 0
            || scope["retained_errors"] != 0
        {
            return Err(err(
                "calendar_not_ready",
                "Refresh Calendar authority before resuming the replacement",
            ));
        }
        let (raw,result):(Option<String>,Option<String>)=self.db.query_row("SELECT outcome_json,result_json FROM calendar_write_operations WHERE account=?1 AND request_id=?2",params![account,request_id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        let outcome: Value = if let Some(prior) = &prior_existing {
            prior[0]["outcome"].clone()
        } else {
            serde_json::from_str(
                raw.as_deref()
                    .ok_or_else(|| err("invalid_request", "Trim outcome is missing"))?,
            )?
        };
        if !matches!(
            outcome["kind"].as_str(),
            Some("accepted" | "observed_state")
        ) || fresh_master != outcome["source"]
        {
            return Err(err(
                "stale_revision",
                "The trimmed master changed; the retained replacement was not submitted",
            ));
        }
        let event = fm_calendar::normalize(
            account,
            crate::string(&lease.dispatch["target"], "calendar")?,
            crate::string(&scope, "time_zone")?,
            &fresh_master,
        )
        .map_err(calendar_error)?;
        writes::initial_event(
            &event,
            parse_access(crate::string(&scope, "access")?)?,
            "edit",
            4,
        )?;
        let calendar = crate::string(&lease.dispatch["target"], "calendar")?;
        let master_id = crate::string(&lease.dispatch["target"], "event")?;
        let current: Option<String> = self.db.query_row("SELECT CASE WHEN length(CAST(json AS BLOB))<=262144 THEN json END FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3", params![account,calendar,master_id], |r|r.get(0)).optional()?.flatten();
        let current = current
            .map(|raw| serde_json::from_str::<Value>(&raw))
            .transpose()?;
        // The confirmed trim may not have reached the cache before cancellation.
        // Only its exact retained baseline or trim may be replaced by this fresh
        // observation. A later full refresh, removal, or third source wins.
        if current
            .as_ref()
            .is_none_or(|value| value != &fresh_master && value != &lease.dispatch["baseline"])
        {
            return Err(err("stale_revision", "The current master changed after the trim; inspect the retained operation before continuing"));
        }
        if current.as_ref() != Some(&fresh_master) {
            self.calendar_assert_no_pending(account, calendar)?;
            let raw = serde_json::to_string(&fresh_master)?;
            let generation = scope["published_generation"].as_i64().unwrap();
            let full = scope["full_generation"].as_i64().unwrap();
            let tx = self.db.unchecked_transaction()?;
            tx.execute("INSERT INTO calendar_sources(account,calendar_id,event_id,json,source_generation) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(account,calendar_id,event_id) DO UPDATE SET json=excluded.json,source_generation=excluded.source_generation",params![account,calendar,master_id,raw,generation])?;
            crate::live_calendar::index_event(&tx, &event, 0, generation)?;
            tx.execute("INSERT INTO calendar_event_state(account,calendar_id,event_id,visible,observed_json,seen_full_generation) VALUES(?1,?2,?3,1,?4,?5) ON CONFLICT(account,calendar_id,event_id) DO UPDATE SET visible=1,observed_json=excluded.observed_json,seen_full_generation=excluded.seen_full_generation",params![account,calendar,master_id,raw,full])?;
            tx.execute("UPDATE calendar_sync_state SET revision=revision+1 WHERE account=?1 AND calendar_id=?2",params![account,calendar])?;
            tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
            tx.commit()?;
            scope = self.calendar_sync_scope(account, calendar)?;
        }
        let prior=prior_existing.unwrap_or(json!([{"lease":lease,"state":if outcome["kind"]=="observed_state"{"reconciled"}else{"acknowledged"},"outcome":outcome,"result":result.map(|v|serde_json::from_str::<Value>(&v)).transpose()?}]));
        let mut dispatch = lease.dispatch.clone();
        dispatch["step"] = 1.into();
        dispatch["kind"] = "create".into();
        dispatch["target"]["event"] = dispatch["following"]["replacement_id"].clone();
        dispatch["body"] = dispatch["following"]["replacement_body"].clone();
        dispatch["baseline"] = Value::Null;
        dispatch["if_match"] = Value::Null;
        dispatch["base_revision"] = scope["revision"].clone();
        // The first dispatch remains immutable in the bounded same-row history.
        // Current dispatch stores only the new create body and metadata guard.
        dispatch.as_object_mut().unwrap().remove("master_proof");
        if self.db.execute("UPDATE calendar_write_operations SET state='ready',operation_revision=operation_revision+1,connection_generation=?4,permission_epoch=?5,dispatch_json=?6,recurring_steps_json=?7,outcome_json=NULL,result_json=NULL WHERE account=?1 AND request_id=?2 AND state IN ('partial','ready') AND operation_revision=?3",params![account,request_id,op.version,generation,epoch,serde_json::to_string(&dispatch)?,serde_json::to_string(&prior)?])?!=1 {return Err(err("stale_revision","Following operation changed"));}
        self.calendar_live_status(account, request_id)
    }

    /// Handles a late reply to a following trim after the row has already
    /// advanced to the replacement step. A matching acceptance is recorded in
    /// the step history; the current step is never touched. Returns `None`
    /// when the lease is not such a superseded trim.
    pub(crate) fn calendar_recurring_late_prior(
        &mut self,
        lease: &CalendarWriteLease,
        outcome: &CalendarWriteOutcome,
    ) -> Result<Option<Value>> {
        if lease.dispatch["version"] != 4 || lease.dispatch["step"] != 0 {
            return Ok(None);
        }
        let row:Option<String>=self.db.query_row("SELECT recurring_steps_json FROM calendar_write_operations WHERE account=?1 AND request_id=?2",params![lease.account,lease.request_id],|r|r.get(0))?;
        let Some(raw) = row else {
            return Ok(None);
        };
        let mut steps: Value = serde_json::from_str(&raw)?;
        let stored: CalendarWriteLease = serde_json::from_value(steps[0]["lease"].clone())?;
        if serde_json::to_value(&stored)? != serde_json::to_value(lease)? {
            return Err(err(
                "stale_revision",
                "Late response does not match the retained trim attempt",
            ));
        }
        if let CalendarWriteOutcome::Accepted(source) = outcome {
            let target: EventKey = serde_json::from_value(lease.dispatch["target"].clone())?;
            writes::bounded_source(source, &target)?;
            writes::etag(crate::string(source, "etag")?)?;
            steps[0]["outcome"] = json!({"kind":"accepted","source":source,"evidence_origin":"provider_response","notification_delivery":"unverified"});
            steps[0]["state"] = "acknowledged".into();
            self.db.execute("UPDATE calendar_write_operations SET recurring_steps_json=?3,operation_revision=operation_revision+1 WHERE account=?1 AND request_id=?2",params![lease.account,lease.request_id,serde_json::to_string(&steps)?])?;
        }
        Ok(Some(
            self.calendar_live_status(&lease.account, &lease.request_id)?,
        ))
    }
}
