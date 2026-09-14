//! Completed Google-shaped batches commit into Core's existing calendar cache.
//! Fake write acknowledgements use the same source, ACL and durable intent journal.
use crate::{err, Result, Store};
use fm_calendar::{Access, CompletedBatch, Event, GoogleWrite, ResponseStatus, SyncScope};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[cfg(test)]
#[path = "calendar_store_tests.rs"]
mod tests;

impl Store {
    pub(crate) fn project_cached_calendars(
        &self,
        account: &str,
        from: chrono::NaiveDate,
        to: chrono::NaiveDate,
    ) -> Result<Vec<Value>> {
        let catalog = self.calendar_sources_for(Some(account))?;
        let selected = catalog["calendars"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["account"] == account)
            .map(|v| json!({"account":account,"calendar_id":v["calendar_id"]}))
            .collect::<Vec<_>>();
        let combined = self.calendar_combined_for(
            &json!({"from":from.to_string(),"to":to.to_string(),"calendars":selected}),
            Some(account),
        )?;
        if combined["coverage"]["incomplete_calendars"]
            .as_u64()
            .unwrap_or(0)
            > 0
            || combined["coverage"]["state"] == "display_zone_required"
        {
            return Err(err(
                "calendar_incomplete",
                "Calendar projection is incomplete; inspect combined Calendar coverage",
            ));
        }
        Ok(combined["events"].as_array().unwrap().clone())
    }
    /// Explicit local setup. Changing ACL/zone invalidates an in-flight batch.
    /// A different query scope requires an explicit full-resync workflow; it
    /// cannot silently reuse a cursor bound to the old query.
    pub fn calendar_configure(
        &mut self,
        scope: &SyncScope,
        zone: &str,
        access: Access,
    ) -> Result<Value> {
        self.require_active_account(&scope.account)?;
        validate_scope(scope)?;
        let existing:Option<String>=self.db.query_row("SELECT query_fingerprint FROM calendar_sync_state WHERE account=?1 AND calendar_id=?2",params![scope.account,scope.calendar],|r|r.get(0)).optional()?;
        if existing
            .as_ref()
            .is_some_and(|query| query != &scope.query_fingerprint)
        {
            return Err(err(
                "scope_mismatch",
                "Changing calendar query scope requires explicit resynchronization",
            ));
        }
        fm_calendar::normalize(
            &scope.account,
            &scope.calendar,
            zone,
            &json!({"id":"zone-validation","status":"cancelled"}),
        )
        .map_err(calendar_error)?;
        let access = access_name(access);
        let tx = self.db.unchecked_transaction()?;
        let changed=tx.execute("INSERT INTO calendar_sync_state(account,calendar_id,query_fingerprint,time_zone,access,sync_token,revision) VALUES(?1,?2,?3,?4,?5,NULL,0) ON CONFLICT(account,calendar_id) DO UPDATE SET query_fingerprint=excluded.query_fingerprint,time_zone=excluded.time_zone,access=excluded.access,revision=calendar_sync_state.revision+1 WHERE calendar_sync_state.query_fingerprint<>excluded.query_fingerprint OR calendar_sync_state.time_zone<>excluded.time_zone OR calendar_sync_state.access<>excluded.access",params![scope.account,scope.calendar,scope.query_fingerprint,zone,access])?;
        if changed != 0 {
            tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        }
        let checkpoint = self.calendar_checkpoint(&scope.account, &scope.calendar)?;
        tx.commit()?;
        Ok(checkpoint)
    }

    pub fn calendar_checkpoint(&self, account: &str, calendar: &str) -> Result<Value> {
        crate::id(account)?;
        crate::live_calendar::provider_id(calendar)?;
        self.db.query_row("SELECT query_fingerprint,time_zone,access,sync_token,revision,permission_epoch,published_generation,full_generation,retained_errors FROM calendar_sync_state WHERE account=?1 AND calendar_id=?2",params![account,calendar],|r|Ok(json!({"account":account,"calendar_id":calendar,"query_fingerprint":r.get::<_,String>(0)?,"time_zone":r.get::<_,String>(1)?,"access":r.get::<_,String>(2)?,"sync_token":r.get::<_,Option<String>>(3)?,"revision":r.get::<_,i64>(4)?,"permission_epoch":r.get::<_,i64>(5)?,"published_generation":r.get::<_,i64>(6)?,"full_generation":r.get::<_,i64>(7)?,"retained_errors":r.get::<_,i64>(8)?}))).optional()?.ok_or_else(||err("not_found","Calendar scope is not configured"))
    }

    /// Partial SyncBatch pages cannot call this API. Exactly one completed batch
    /// receipt is accepted for a given account/calendar/revision; exact retries
    /// return the same receipt without replaying source changes.
    pub fn calendar_commit_batch(
        &mut self,
        expected_revision: i64,
        batch: &CompletedBatch,
    ) -> Result<Value> {
        validate_scope(&batch.scope)?;
        if expected_revision < 0
            || batch.replacements.len() > 10_000
            || batch.next_sync_token.is_empty()
            || batch.next_sync_token.len() > 8192
        {
            return Err(err("invalid_request", "Calendar batch exceeds bounds"));
        }
        let account = &batch.scope.account;
        let calendar = &batch.scope.calendar;
        let fingerprint = fingerprint(
            &json!({"expected_revision":expected_revision,"account":account,"calendar":calendar,"query":batch.scope.query_fingerprint,"replacements":batch.replacements,"token":batch.next_sync_token,"full_replace":batch.full_replace}),
        )?;
        let prior:Option<(String,String)>=self.db.query_row("SELECT fingerprint,result_json FROM calendar_batch_commits WHERE account=?1 AND calendar_id=?2 AND expected_revision=?3",params![account,calendar,expected_revision],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((old, result)) = prior {
            return if old == fingerprint {
                Ok(serde_json::from_str(&result)?)
            } else {
                Err(err(
                    "stale_revision",
                    "A different batch already consumed this revision",
                ))
            };
        }
        self.require_active_account(account)?;
        let checkpoint = self.calendar_checkpoint(account, calendar)?;
        self.calendar_assert_no_pending(account, calendar)?;
        check_batch_scope(&checkpoint, expected_revision, &batch.scope)?;
        if checkpoint["sync_token"].is_null() && !batch.full_replace {
            return Err(err(
                "invalid_request",
                "An initial calendar batch must be complete inventory",
            ));
        }
        let zone = checkpoint["time_zone"].as_str().unwrap();
        // Normalize every replacement before SQL mutation. Raw cancelled sources
        // retain their exact observation separately from sparse merged identity.
        let mut replacements = Vec::new();
        for (id, observed) in &batch.replacements {
            crate::live_calendar::provider_id(id)?;
            if observed["id"] != *id {
                return Err(err("scope_mismatch", "Event ID differs from its batch key"));
            }
            let mut source = observed.clone();
            if observed["status"] == "cancelled" {
                let previous:Option<String>=self.db.query_row("SELECT json FROM calendar_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![account,calendar,id],|r|r.get(0)).optional()?;
                if let Some(previous) = previous {
                    source = serde_json::from_str(&previous)?;
                    for (key, value) in observed
                        .as_object()
                        .ok_or_else(|| err("invalid_request", "Event source must be an object"))?
                    {
                        source[key] = value.clone();
                    }
                }
            }
            let normalized =
                fm_calendar::normalize(account, calendar, zone, &source).map_err(calendar_error)?;
            replacements.push((
                id,
                serde_json::to_string(&source)?,
                serde_json::to_string(observed)?,
                normalized,
            ));
        }
        let next_revision = expected_revision
            .checked_add(1)
            .ok_or_else(|| err("stale_revision", "Calendar revision exhausted"))?;
        let result = json!({"account":account,"calendar_id":calendar,"revision":next_revision,"sync_token":batch.next_sync_token,"applied_events":replacements.len(),"full_replace":batch.full_replace});
        let tx = self.db.unchecked_transaction()?;
        let changed=tx.execute("UPDATE calendar_sync_state SET sync_token=?1,revision=?2 WHERE account=?3 AND calendar_id=?4 AND revision=?5 AND query_fingerprint=?6",params![batch.next_sync_token,next_revision,account,calendar,expected_revision,batch.scope.query_fingerprint])?;
        if changed != 1 {
            return Err(err(
                "stale_revision",
                "Calendar checkpoint changed before commit",
            ));
        }
        if batch.full_replace {
            tx.execute(
                "UPDATE calendar_event_state SET visible=0 WHERE account=?1 AND calendar_id=?2",
                params![account, calendar],
            )?;
        }
        for (id, source, observed, normalized) in replacements {
            tx.execute("INSERT INTO calendar_sources(account,calendar_id,event_id,json) VALUES(?1,?2,?3,?4) ON CONFLICT(account,calendar_id,event_id) DO UPDATE SET json=excluded.json",params![account,calendar,id,source])?;
            tx.execute("INSERT INTO calendar_event_state(account,calendar_id,event_id,visible,observed_json) VALUES(?1,?2,?3,1,?4) ON CONFLICT(account,calendar_id,event_id) DO UPDATE SET visible=1,observed_json=excluded.observed_json",params![account,calendar,id,observed])?;
            let generation:i64=tx.query_row("SELECT source_generation FROM calendar_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![account,calendar,id],|r|r.get(0))?;
            crate::live_calendar::index_event(&tx, &normalized, 0, generation)?;
        }
        tx.execute("INSERT INTO calendar_batch_commits(account,calendar_id,expected_revision,fingerprint,result_json) VALUES(?1,?2,?3,?4,?5)",params![account,calendar,expected_revision,fingerprint,serde_json::to_string(&result)?])?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(result)
    }

    pub(crate) fn stored_calendar_event(
        &self,
        account: &str,
        calendar: &str,
        event: &str,
    ) -> Result<(Event, Access)> {
        crate::live_calendar::provider_id(event)?;
        let scope = self.calendar_checkpoint(account, calendar)?;
        let raw:Option<String>=self.db.query_row("SELECT json FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![account,calendar,event],|r|r.get(0)).optional()?;
        let raw = raw.ok_or_else(|| err("not_found", "Event is not visible in this calendar"))?;
        let mut event = fm_calendar::normalize(
            account,
            calendar,
            scope["time_zone"].as_str().unwrap(),
            &serde_json::from_str(&raw)?,
        )
        .map_err(calendar_error)?;
        if let Some(parent) = &event.series_id {
            let raw:Option<String>=self.db.query_row("SELECT json FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![account,calendar,parent],|r|r.get(0)).optional()?;
            if let Some(raw) = raw {
                let master = fm_calendar::normalize(
                    account,
                    calendar,
                    scope["time_zone"].as_str().unwrap(),
                    &serde_json::from_str(&raw)?,
                )
                .map_err(calendar_error)?;
                event = event
                    .with_recurring_master(&master)
                    .map_err(calendar_error)?;
            }
        }
        let mut access = parse_access(scope["access"].as_str().unwrap())?;
        if self.calendar_membership_denied(account, calendar)? {
            access = Access::None;
        }
        Ok((event, access))
    }

    /// Prepare a fake provider write from current retained source and ACL.
    /// The human-selected operation is journaled before any acknowledgement.
    pub fn calendar_prepare_edit(
        &mut self,
        target: &fm_calendar::EventKey,
        request_id: &str,
        expected_etag: &str,
        body: Value,
    ) -> Result<Value> {
        let request =
            json!({"kind":"edit","target":target,"expected_etag":expected_etag,"body":body});
        self.calendar_prepare(target, request_id, request, |event, access, _| {
            fm_calendar::edit(event, access, expected_etag, body).map_err(calendar_error)
        })
    }

    pub fn calendar_prepare_rsvp(
        &mut self,
        target: &fm_calendar::EventKey,
        request_id: &str,
        expected_etag: &str,
        response: ResponseStatus,
    ) -> Result<Value> {
        let request = json!({"kind":"rsvp","target":target,"expected_etag":expected_etag,"response":response});
        self.calendar_prepare(target, request_id, request, |event, access, actor| {
            fm_calendar::rsvp(event, access, actor, expected_etag, response).map_err(calendar_error)
        })
    }

    pub(crate) fn calendar_prepare(
        &mut self,
        target: &fm_calendar::EventKey,
        request_id: &str,
        request: Value,
        build: impl FnOnce(&Event, Access, &str) -> Result<GoogleWrite>,
    ) -> Result<Value> {
        crate::id(request_id)?;
        if serde_json::to_vec(&request)?.len() > 256 * 1024 {
            return Err(err(
                "invalid_request",
                "Calendar write exceeds source bound",
            ));
        }
        let fingerprint = fingerprint(&request)?;
        let old:Option<(String,String,Option<String>)>=self.db.query_row("SELECT fingerprint,intent_json,result_json FROM calendar_write_operations WHERE account=?1 AND request_id=?2 AND execution_mode='local'",params![target.account,request_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((old, intent, result)) = old {
            if old != fingerprint {
                return Err(err(
                    "request_id_conflict",
                    "Request ID belongs to another calendar write",
                ));
            }
            return Ok(
                json!({"request_id":request_id,"intent":serde_json::from_str::<Value>(&intent)?,"result":result.map(|raw|serde_json::from_str::<Value>(&raw)).transpose()?}),
            );
        }
        self.require_active_account(&target.account)?;
        let (event, access) =
            self.stored_calendar_event(&target.account, &target.calendar, &target.event)?;
        if event.cancelled {
            return Err(err(
                "invalid_edit",
                "Cancelled events cannot be edited by this fake boundary",
            ));
        }
        let actor: String = self.db.query_row(
            "SELECT email FROM accounts WHERE id=?1",
            [&target.account],
            |r| r.get(0),
        )?;
        let plan = build(&event, access, &actor)?;
        let mut intent = json!({"request":request,"write":plan,"actor":actor});
        if let Some(public_fingerprint) = request.get("public_fingerprint") {
            intent["public_fingerprint"] = public_fingerprint.clone();
        }
        self.db.execute("INSERT INTO calendar_write_operations(account,request_id,fingerprint,intent_json,result_json) VALUES(?1,?2,?3,?4,NULL)",params![target.account,request_id,fingerprint,serde_json::to_string(&intent)?])?;
        Ok(json!({"request_id":request_id,"intent":intent,"result":null}))
    }

    /// A deterministic provider acknowledgement. No HTTP request is issued here.
    /// Current ETag/permissions are rechecked before the new source is published.
    pub fn calendar_acknowledge(
        &mut self,
        target: &fm_calendar::EventKey,
        request_id: &str,
        provider_source: Value,
    ) -> Result<Value> {
        crate::id(request_id)?;
        crate::id(&target.account)?;
        crate::live_calendar::provider_id(&target.calendar)?;
        crate::live_calendar::provider_id(&target.event)?;
        let (intent,previous):(String,Option<String>)=self.db.query_row("SELECT intent_json,result_json FROM calendar_write_operations WHERE account=?1 AND request_id=?2 AND execution_mode='local'",params![target.account,request_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?.ok_or_else(||err("not_found","Calendar write intent does not exist"))?;
        let intent: Value = serde_json::from_str(&intent)?;
        if intent["write"]["target"] != serde_json::to_value(target)?
            || provider_source["id"] != target.event
        {
            return Err(err(
                "scope_mismatch",
                "Acknowledgement differs from the intended calendar event",
            ));
        }
        let source_hash = fingerprint(&provider_source)?;
        if let Some(previous) = previous {
            let previous: Value = serde_json::from_str(&previous)?;
            if previous["source_sha256"] != source_hash {
                return Err(err(
                    "request_id_conflict",
                    "Acknowledgement changed after commit",
                ));
            }
            return Ok(previous);
        }
        self.calendar_assert_no_pending(&target.account, &target.calendar)?;
        self.require_active_account(&target.account)?;
        let (event, access) =
            self.stored_calendar_event(&target.account, &target.calendar, &target.event)?;
        let etag = intent["write"]["if_match"]
            .as_str()
            .ok_or_else(|| err("invalid_request", "Stored write has no ETag"))?;
        let current_actor: String = self.db.query_row(
            "SELECT email FROM accounts WHERE id=?1",
            [&target.account],
            |r| r.get(0),
        )?;
        let request = &intent["request"];
        let fresh = if request["kind"] == "rsvp" {
            fm_calendar::rsvp(
                &event,
                access,
                &current_actor,
                etag,
                serde_json::from_value(request["response"].clone())?,
            )
            .map_err(calendar_error)?
        } else {
            fm_calendar::edit(&event, access, etag, request["body"].clone())
                .map_err(calendar_error)?
        };
        if serde_json::to_value(&fresh)? != intent["write"] {
            return Err(err(
                "stale_revision",
                "Calendar write intent no longer matches current source",
            ));
        }
        for (field, value) in fresh
            .body
            .as_object()
            .ok_or_else(|| err("invalid_edit", "Write body is not an object"))?
        {
            if provider_source[field] != *value {
                return Err(err(
                    "invalid_response",
                    "Provider acknowledgement does not contain the intended edit",
                ));
            }
        }
        let checkpoint = self.calendar_checkpoint(&target.account, &target.calendar)?;
        let acknowledged = fm_calendar::normalize(
            &target.account,
            &target.calendar,
            checkpoint["time_zone"].as_str().unwrap(),
            &provider_source,
        )
        .map_err(calendar_error)?;
        if acknowledged.series_id != event.series_id
            || acknowledged.original_start != event.original_start
        {
            return Err(err(
                "scope_mismatch",
                "Acknowledgement changed recurrence identity",
            ));
        }
        if acknowledged.etag.is_empty() || acknowledged.etag == etag || acknowledged.cancelled {
            return Err(err(
                "invalid_response",
                "Acknowledgement requires a new nonempty ETag and active event",
            ));
        }
        let revision = checkpoint["revision"].as_i64().unwrap();
        let next = revision
            .checked_add(1)
            .ok_or_else(|| err("stale_revision", "Calendar revision exhausted"))?;
        let result = json!({"request_id":request_id,"target":target,"etag":acknowledged.etag,"revision":next,"state":"acknowledged","source_sha256":source_hash});
        let tx = self.db.unchecked_transaction()?;
        if tx.execute("UPDATE calendar_sync_state SET revision=?1 WHERE account=?2 AND calendar_id=?3 AND revision=?4",params![next,target.account,target.calendar,revision])?!=1 {return Err(err("stale_revision","Calendar changed before acknowledgement"));}
        let source = serde_json::to_string(&provider_source)?;
        tx.execute("UPDATE calendar_sources SET json=?1 WHERE account=?2 AND calendar_id=?3 AND event_id=?4",params![source,target.account,target.calendar,target.event])?;
        let generation:i64=tx.query_row("SELECT source_generation FROM calendar_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",params![target.account,target.calendar,target.event],|r|r.get(0))?;
        crate::live_calendar::index_event(&tx, &acknowledged, 0, generation)?;
        tx.execute("INSERT INTO calendar_event_state(account,calendar_id,event_id,visible,observed_json) VALUES(?1,?2,?3,1,?4) ON CONFLICT(account,calendar_id,event_id) DO UPDATE SET visible=1,observed_json=excluded.observed_json",params![target.account,target.calendar,target.event,source])?;
        tx.execute("UPDATE calendar_write_operations SET result_json=?1 WHERE account=?2 AND request_id=?3 AND result_json IS NULL",params![serde_json::to_string(&result)?,target.account,request_id])?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(result)
    }
    pub(crate) fn calendar_assert_no_pending(&self, account: &str, calendar: &str) -> Result<()> {
        let pending:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM calendar_sync_runs WHERE account=?1 AND calendar_id=?2 AND kind='events' AND state='staging') OR EXISTS(SELECT 1 FROM calendar_sources WHERE account=?1 AND calendar_id=?2 AND pending_generation IS NOT NULL)",params![account,calendar],|r|r.get(0))?;
        if pending {
            return Err(err(
                "calendar_sync_pending",
                "Publish or discard the Calendar sync before acknowledging a write",
            ));
        }
        Ok(())
    }
}

fn validate_scope(scope: &SyncScope) -> Result<()> {
    crate::id(&scope.account)?;
    crate::live_calendar::provider_id(&scope.calendar)?;
    crate::id(&scope.query_fingerprint)?;
    Ok(())
}
fn check_batch_scope(checkpoint: &Value, revision: i64, scope: &SyncScope) -> Result<()> {
    if checkpoint["revision"] != revision
        || checkpoint["query_fingerprint"] != scope.query_fingerprint
    {
        return Err(err(
            "stale_revision",
            "Calendar query scope or revision changed",
        ));
    }
    Ok(())
}
pub(crate) fn parse_access(value: &str) -> Result<Access> {
    match value {
        "none" => Ok(Access::None),
        "free_busy" => Ok(Access::FreeBusy),
        "reader" => Ok(Access::Reader),
        "writer" => Ok(Access::Writer),
        "writer_without_private_access" => Ok(Access::WriterWithoutPrivateAccess),
        "owner" => Ok(Access::Owner),
        _ => Err(err("invalid_request", "Unknown calendar access")),
    }
}
pub(crate) fn access_name(access: Access) -> &'static str {
    match access {
        Access::None => "none",
        Access::FreeBusy => "free_busy",
        Access::Reader => "reader",
        Access::Writer => "writer",
        Access::WriterWithoutPrivateAccess => "writer_without_private_access",
        Access::Owner => "owner",
    }
}
pub(crate) fn calendar_error(error: fm_calendar::CalendarError) -> crate::CoreError {
    err(error.code, &error.message)
}
pub(crate) fn fingerprint(value: &Value) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
