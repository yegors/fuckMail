//! P1-04/P1-06: one permission-filtered combined calendar and durable local writes.
use crate::calendar_store::{calendar_error, fingerprint, parse_access};
use crate::{err, fields, id, string, Result, Store};
use fm_calendar::{Access, EventKey};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::collections::BTreeSet;

impl Store {
    pub(crate) fn calendar_sources(&self) -> Result<Value> {
        self.calendar_sources_for(None)
    }
    pub(crate) fn calendar_sources_for(&self, archive_account: Option<&str>) -> Result<Value> {
        let live = self.profile_kind()? == crate::live_profile::ProfileKind::Live;
        let mut statement = self.db.prepare("SELECT c.account,c.calendar_id,a.name,a.email,c.time_zone,c.access,c.revision,c.sync_token,k.json,COALESCE(k.deleted,0),COALESCE(k.observed_epoch,0),COALESCE(cs.full_generation,0),COALESCE(cs.complete,0),c.published_generation,c.retained_errors,c.full_generation,a.lifecycle_state,g.scopes_json,g.connection_state FROM calendar_sync_state c JOIN accounts a ON a.id=c.account LEFT JOIN calendar_catalog k USING(account,calendar_id) LEFT JOIN calendar_catalog_state cs ON cs.account=c.account LEFT JOIN google_accounts g ON g.account=c.account WHERE (a.lifecycle_state='active' OR a.id=?1) ORDER BY a.name,c.calendar_id LIMIT 101")?;
        let mut calendars = Vec::new();
        let mut rows = statement.query([archive_account])?;
        while let Some(row) = rows.next()? {
            let mut permission: String = row.get(5)?;
            let calendar: String = row.get(1)?;
            let metadata: Option<String> = row.get(8)?;
            let metadata: Value = metadata
                .map(|v| serde_json::from_str(&v))
                .transpose()?
                .unwrap_or(Value::Null);
            if live && (row.get::<_, bool>(9)? || row.get::<_, i64>(10)? < row.get::<_, i64>(11)?) {
                permission = "none".into();
            }
            let active = row.get::<_, String>(16)? == "active";
            let grants: BTreeSet<String> = row
                .get::<_, Option<String>>(17)?
                .map(|value| serde_json::from_str(&value))
                .transpose()?
                .unwrap_or_default();
            let grants = fm_google::auth::ScopeSet::new(&grants);
            // Keep the provider ACL intact in storage. OAuth grants can limit
            // this active view further; an explicitly opened archive stays readable.
            if live && active && permission != "none" && !grants.calendar_events_read() {
                permission = if grants.calendar_free_busy() {
                    "free_busy"
                } else {
                    "none"
                }
                .into();
            }
            let generation: i64 = row.get(13)?;
            let complete = if live {
                row.get::<_,bool>(12)? && row.get::<_,i64>(14)?==0 && row.get::<_,i64>(15)?>0 && generation>0 && self.db.query_row("SELECT errors=0 AND identity_complete=1 FROM calendar_sync_runs WHERE id=?1",[generation],|r|r.get::<_,bool>(0)).optional()?.unwrap_or(false)
            } else {
                row.get::<_, Option<String>>(7)?.is_some()
            };
            let name = metadata["summaryOverride"]
                .as_str()
                .or(metadata["summary"].as_str())
                .unwrap_or(match calendar.as_str() {
                    "primary" => "Primary",
                    "availability" => "Availability",
                    _ => calendar.as_str(),
                });
            let writable =
                crate::live_calendar_writes::live_source_writable(parse_access(&permission)?)
                    && active
                    && (!live
                        || (row.get::<_, Option<String>>(18)?.as_deref() == Some("connected")
                            && grants.calendar_events_write()));
            calendars.push(json!({"account":row.get::<_,String>(0)?,"calendar_id":calendar,"name":name,"account_name":row.get::<_,String>(2)?,"account_email":row.get::<_,String>(3)?,"time_zone":row.get::<_,String>(4)?,"permission":permission,"writable":writable,"can_create":writable && (!live || complete),"revision":row.get::<_,i64>(6)?,"primary":metadata["primary"]==true,"hidden":metadata["hidden"]==true,"provider_selected":metadata["selected"],"color":metadata["backgroundColor"],"coverage":if permission=="none"{"denied"}else if complete{"cached"}else{"incomplete"}}));
        }
        let truncated = calendars.len() > 100;
        calendars.truncate(100);
        Ok(json!({"calendars":calendars,"revision":self.revision()?,"truncated":truncated}))
    }

    pub(crate) fn calendar_combined(&self, p: &Value) -> Result<Value> {
        self.calendar_combined_for(p, None)
    }
    pub(crate) fn calendar_combined_for(
        &self,
        p: &Value,
        archive_account: Option<&str>,
    ) -> Result<Value> {
        fields(p, &["from", "to", "calendars"])?;
        let from = parse_date(string(p, "from")?)?;
        let to = parse_date(string(p, "to")?)?;
        if to <= from || (to - from).num_days() > 42 {
            return Err(err("invalid_request", "Calendar range must be 1–42 days"));
        }
        let preferences = self.preferences()?;
        let selection = p
            .get("calendars")
            .unwrap_or(&preferences["preferences"]["selected_calendars"]);
        let selected = if selection.is_null() {
            None
        } else {
            let entries = selection
                .as_array()
                .filter(|v| v.len() <= 100)
                .ok_or_else(|| {
                    err(
                        "invalid_request",
                        "Calendar selection must be null or at most 100 identities",
                    )
                })?;
            let mut selected = BTreeSet::new();
            for entry in entries {
                fields(entry, &["account", "calendar_id"])?;
                selected.insert((
                    id(string(entry, "account")?)?.to_owned(),
                    crate::live_calendar::provider_id(
                        entry["calendar_id"]
                            .as_str()
                            .ok_or_else(|| err("invalid_request", "Calendar ID must be text"))?,
                    )?
                    .to_owned(),
                ));
            }
            Some(selected)
        };
        let catalog = self.calendar_sources_for(archive_account)?;
        let sources = catalog["calendars"].as_array().unwrap();
        if let Some(selected) = &selected {
            for (account, calendar) in selected {
                if self.account_lifecycle(account)?["state"] == "removed" {
                    continue;
                }
                if !sources
                    .iter()
                    .any(|v| v["account"] == *account && v["calendar_id"] == *calendar)
                {
                    return Err(err("not_found", "A selected calendar no longer exists"));
                }
            }
        }
        let live = self.profile_kind()? == crate::live_profile::ProfileKind::Live;
        let Some(display_zone) = self.calendar_display_zone()? else {
            return Ok(
                json!({"total":0,"events":[],"from":from.to_string(),"to":to.to_string(),"time_zone":null,"revision":self.revision()?,"coverage":{"state":"display_zone_required","complete":false,"live":false,"selected_calendars":0,"denied_calendars":0,"incomplete_calendars":sources.len()},"continuation":null}),
            );
        };
        let window =
            fm_calendar::Window::from_dates(from, to, &display_zone).map_err(calendar_error)?;
        let mut events = Vec::new();
        let mut selected_count = 0;
        let mut denied = 0;
        let mut incomplete = usize::from(catalog["truncated"] == true);
        let mut candidates = 0;
        let mut budgets = 0;
        let mut details = Vec::new();
        if live && selected.is_none() {
            let pending:i64=self.db.query_row("SELECT COUNT(*) FROM google_accounts g JOIN accounts a ON a.id=g.account LEFT JOIN calendar_catalog_state c ON c.account=g.account WHERE a.lifecycle_state='active' AND COALESCE(c.complete,0)=0",[],|r|r.get(0))?;
            incomplete += pending as usize;
            if sources.is_empty() && pending == 0 {
                incomplete += 1;
            }
        }
        for source in sources {
            let account = source["account"].as_str().unwrap();
            let calendar = source["calendar_id"].as_str().unwrap();
            if selected
                .as_ref()
                .is_some_and(|s| !s.contains(&(account.into(), calendar.into())))
            {
                continue;
            }
            selected_count += 1;
            let permission = parse_access(source["permission"].as_str().unwrap())?;
            if permission == Access::None {
                denied += 1;
                continue;
            }
            if candidates >= 4096 {
                incomplete += 1;
                budgets += 1;
                break;
            }
            let (mut projected, limited, covered) = if live && permission == Access::FreeBusy {
                let (rows, covered) = self.calendar_busy_projection(account, calendar, window)?;
                (rows, false, covered)
            } else {
                let projection = self.calendar_indexed_projection(
                    account,
                    calendar,
                    source["time_zone"].as_str().unwrap(),
                    permission,
                    window,
                )?;
                candidates += projection.candidates;
                (
                    projection.events,
                    projection.incomplete,
                    source["coverage"] == "cached",
                )
            };
            if !covered || limited {
                incomplete += 1;
            }
            if limited {
                budgets += 1;
            }
            details.push(json!({"account":account,"calendar_id":calendar,"covered":covered,"projection_limited":limited}));
            for row in &mut projected {
                row["account"] = account.into();
                row["account_name"] = source["account_name"].clone();
                row["calendar_name"] = source["name"].clone();
                let recurring = !row["occurrence_key"]["original_start"].is_null();
                row["recurring"] = recurring.into();
                if live {
                    continue;
                }
                row["can_rsvp"] = false.into();
                if row["busy_only"] != true && !recurring {
                    if let Some(event_id) = row["event_id"].as_str() {
                        let (event, access) =
                            self.stored_calendar_event(account, calendar, event_id)?;
                        if let Some(actor) = event.attendees.iter().find(|v| {
                            v["self"] == true
                                && v["email"].as_str().is_some_and(|email| {
                                    email.eq_ignore_ascii_case(
                                        source["account_email"].as_str().unwrap(),
                                    )
                                })
                        }) {
                            row["can_rsvp"] = (!event.etag.is_empty()
                                && matches!(
                                    access.for_event(&event),
                                    Access::Reader | Access::Writer | Access::Owner
                                ))
                            .into();
                            row["self_response"] = actor["responseStatus"].clone();
                        }
                    }
                }
            }
            events.extend(projected);
            if events.len() > 1000 {
                if !live {
                    return Err(err("limit_exceeded","Combined calendar exceeds 1,000 occurrences; narrow the date range or calendars"));
                }
                events.truncate(1000);
                incomplete += 1;
                budgets += 1;
                break;
            }
        }
        fm_calendar::sort_projected_events(&mut events, window).map_err(calendar_error)?;
        Ok(
            json!({"total":events.len(),"events":events,"from":from.to_string(),"to":to.to_string(),"time_zone":display_zone,"revision":self.revision()?,"coverage":{"state":if denied>0{"restricted"}else if incomplete>0{"incomplete"}else{"cached"},"complete":denied==0&&incomplete==0,"live":false,"selected_calendars":selected_count,"denied_calendars":denied,"incomplete_calendars":incomplete,"projection_limited":budgets>0,"candidates":candidates,"calendars":details},"continuation":null}),
        )
    }
    pub(crate) fn calendar_write(&mut self, account: &str, p: &Value) -> Result<Value> {
        fields(
            p,
            &[
                "request_id",
                "kind",
                "calendar_id",
                "event_id",
                "expected_etag",
                "fields",
                "response",
                "prepare_only",
            ],
        )?;
        let request_id = id(string(p, "request_id")?)?;
        let kind = string(p, "kind")?;
        let calendar = crate::live_calendar::provider_id(
            p["calendar_id"]
                .as_str()
                .ok_or_else(|| err("invalid_request", "Calendar ID must be text"))?,
        )?;
        let prepare_only = p
            .get("prepare_only")
            .map(|v| {
                v.as_bool()
                    .ok_or_else(|| err("invalid_request", "prepare_only must be boolean"))
            })
            .transpose()?
            .unwrap_or(false);
        let target = EventKey {
            account: account.into(),
            calendar: calendar.into(),
            event: if kind == "create" {
                format!(
                    "local-{}",
                    &fingerprint(&json!([account, calendar, request_id]))?[..40]
                )
            } else {
                crate::live_calendar::provider_id(
                    p["event_id"]
                        .as_str()
                        .ok_or_else(|| err("invalid_request", "Event ID must be text"))?,
                )?
                .into()
            },
        };
        let mut request = p.clone();
        request.as_object_mut().unwrap().remove("prepare_only");
        let hash = fingerprint(&request)?;
        let prior:Option<(String,Option<String>)>=self.db.query_row("SELECT intent_json,result_json FROM calendar_write_operations WHERE account=?1 AND request_id=?2 AND execution_mode='local'",params![account,request_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((prior, receipt)) = prior {
            let prior: Value = serde_json::from_str(&prior)?;
            if prior["public_fingerprint"] != hash {
                return Err(err(
                    "request_id_conflict",
                    "Request ID belongs to another calendar operation",
                ));
            }
            if receipt.is_some() {
                return self.calendar_write_ack(account, &json!({"request_id":request_id}));
            }
            self.require_active_account(account)?;
        } else {
            self.require_active_account(account)?;
            let checkpoint = self.calendar_checkpoint(account, calendar)?;
            let access = parse_access(checkpoint["access"].as_str().unwrap())?;
            if kind == "create" {
                if p.get("event_id").is_some()
                    || p.get("expected_etag").is_some()
                    || p.get("response").is_some()
                {
                    return Err(err(
                        "invalid_request",
                        "Create does not accept event identity, ETag or RSVP",
                    ));
                }
                if !matches!(access, Access::Owner | Access::Writer) {
                    return Err(err("permission_denied", "Choose a writable calendar"));
                }
                let mut body = calendar_fields(
                    p.get("fields")
                        .ok_or_else(|| err("invalid_request", "Create needs event fields"))?,
                )?;
                let actor: String = self.db.query_row(
                    "SELECT email FROM accounts WHERE id=?1",
                    [account],
                    |r| r.get(0),
                )?;
                mark_actor(&mut body, &actor);
                let mut source = body.clone();
                source["id"] = target.event.clone().into();
                source["etag"] = "pending-create".into();
                source["status"] = "confirmed".into();
                fm_calendar::normalize(
                    account,
                    calendar,
                    checkpoint["time_zone"].as_str().unwrap(),
                    &source,
                )
                .map_err(calendar_error)?;
                let intent = json!({"request":{"kind":"create","body":body},"write":{"target":target,"body":body},"public_fingerprint":hash});
                self.db.execute("INSERT INTO calendar_write_operations(account,request_id,fingerprint,intent_json,result_json) VALUES(?1,?2,?3,?4,NULL)",params![account,request_id,hash,serde_json::to_string(&intent)?])?;
            } else if kind == "edit" || kind == "rsvp" {
                let (event, _) = self.stored_calendar_event(account, calendar, &target.event)?;
                if !event.recurrence.is_empty() || event.series_id.is_some() {
                    return Err(err(
                        "unsupported_operation",
                        "Recurring event changes are not available in the local phase",
                    ));
                }
                if event.private && !matches!(access, Access::Owner | Access::Writer) {
                    return Err(err(
                        "permission_denied",
                        "This calendar does not permit private event details",
                    ));
                }
                let etag = string(p, "expected_etag")?;
                if kind == "edit" {
                    if p.get("response").is_some() {
                        return Err(err("invalid_request", "Edit does not accept RSVP"));
                    }
                    let mut body = calendar_fields(
                        p.get("fields")
                            .ok_or_else(|| err("invalid_request", "Edit needs event fields"))?,
                    )?;
                    if let Some(attendees) = body.get_mut("attendees").and_then(Value::as_array_mut)
                    {
                        for attendee in attendees {
                            if let Some(existing) = event.attendees.iter().find(|a| {
                                a["email"]
                                    .as_str()
                                    .zip(attendee["email"].as_str())
                                    .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
                            }) {
                                *attendee = existing.clone();
                            }
                        }
                    }
                    let mut source = event.source.clone();
                    for (field, value) in body.as_object().unwrap() {
                        source[field] = value.clone();
                    }
                    fm_calendar::normalize(
                        account,
                        calendar,
                        checkpoint["time_zone"].as_str().unwrap(),
                        &source,
                    )
                    .map_err(calendar_error)?;
                    let request = json!({"kind":"edit","target":target,"expected_etag":etag,"body":body,"public_fingerprint":hash});
                    self.calendar_prepare(&target, request_id, request, |event, access, _| {
                        fm_calendar::edit(event, access, etag, body).map_err(calendar_error)
                    })?;
                } else {
                    if p.get("fields").is_some() {
                        return Err(err("invalid_request", "RSVP does not accept event fields"));
                    }
                    let response = match string(p, "response")? {
                        "accepted" => fm_calendar::ResponseStatus::Accepted,
                        "tentative" => fm_calendar::ResponseStatus::Tentative,
                        "declined" => fm_calendar::ResponseStatus::Declined,
                        _ => {
                            return Err(err(
                                "invalid_request",
                                "Choose accepted, tentative or declined",
                            ))
                        }
                    };
                    let request = json!({"kind":"rsvp","target":target,"expected_etag":etag,"response":response,"public_fingerprint":hash});
                    self.calendar_prepare(&target, request_id, request, |event, access, actor| {
                        fm_calendar::rsvp(event, access, actor, etag, response)
                            .map_err(calendar_error)
                    })?;
                }
            } else {
                return Err(err("unsupported_operation", "Choose create, edit or rsvp"));
            }
        }
        if prepare_only {
            return Ok(
                json!({"request_id":request_id,"target":target,"state":"pending","local_only":true}),
            );
        }
        self.calendar_write_ack(account, &json!({"request_id":request_id}))
    }

    pub(crate) fn calendar_write_ack(&mut self, account: &str, p: &Value) -> Result<Value> {
        fields(p, &["request_id"])?;
        let request_id = id(string(p, "request_id")?)?;
        let (raw,prior):(String,Option<String>)=self.db.query_row("SELECT intent_json,result_json FROM calendar_write_operations WHERE account=?1 AND request_id=?2 AND execution_mode='local'",params![account,request_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?.ok_or_else(||err("not_found","Calendar operation does not exist"))?;
        let intent: Value = serde_json::from_str(&raw)?;
        if intent.get("public_fingerprint").is_none() {
            return Err(err(
                "unsupported_operation",
                "This is not a local calendar operation",
            ));
        }
        if let Some(prior) = prior {
            let mut result: Value = serde_json::from_str(&prior)?;
            result["state"] = "fake_acknowledged".into();
            result["local_only"] = true.into();
            return Ok(result);
        }
        self.require_active_account(account)?;
        let target: EventKey = serde_json::from_value(intent["write"]["target"].clone())?;
        let result = self.commit_local_calendar_write(&target, request_id, &intent);
        if let Err(error) = &result {
            let mut failed = intent.clone();
            failed["last_error"] = json!({"code":error.code,"message":error.message});
            self.db.execute("UPDATE calendar_write_operations SET intent_json=?1 WHERE account=?2 AND request_id=?3",params![serde_json::to_string(&failed)?,account,request_id])?;
        }
        result
    }

    fn commit_local_calendar_write(
        &mut self,
        target: &EventKey,
        request_id: &str,
        intent: &Value,
    ) -> Result<Value> {
        self.calendar_assert_no_pending(&target.account, &target.calendar)?;
        let checkpoint = self.calendar_checkpoint(&target.account, &target.calendar)?;
        let access = parse_access(checkpoint["access"].as_str().unwrap())?;
        let creating = intent["request"]["kind"] == "create";
        let mut source = if creating {
            if !matches!(access, Access::Owner | Access::Writer) {
                return Err(err(
                    "permission_denied",
                    "Calendar permission changed before creation",
                ));
            }
            if self.db.query_row("SELECT EXISTS(SELECT 1 FROM calendar_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3)",params![target.account,target.calendar,target.event],|r|r.get::<_,bool>(0))?{return Err(err("request_id_conflict","Client-created event identity already exists"));}
            let mut value = intent["write"]["body"].clone();
            value["id"] = target.event.clone().into();
            value["status"] = "confirmed".into();
            value
        } else {
            let (event, _) =
                self.stored_calendar_event(&target.account, &target.calendar, &target.event)?;
            if event.private && !matches!(access, Access::Owner | Access::Writer) {
                return Err(err(
                    "permission_denied",
                    "Private event details are no longer permitted",
                ));
            }
            let mut source = event.source;
            for (field, value) in intent["write"]["body"].as_object().unwrap() {
                source[field] = value.clone();
            }
            source
        };
        source["etag"] = format!(
            "\"local-{}\"",
            &fingerprint(&json!([target, request_id, intent["public_fingerprint"]]))?[..40]
        )
        .into();
        if !creating {
            let mut result = self.calendar_acknowledge(target, request_id, source)?;
            result["state"] = "fake_acknowledged".into();
            result["local_only"] = true.into();
            return Ok(result);
        }
        fm_calendar::normalize(
            &target.account,
            &target.calendar,
            checkpoint["time_zone"].as_str().unwrap(),
            &source,
        )
        .map_err(calendar_error)?;
        let revision = checkpoint["revision"].as_i64().unwrap();
        let result = json!({"request_id":request_id,"target":target,"etag":source["etag"],"revision":revision+1,"state":"fake_acknowledged","local_only":true,"source_sha256":fingerprint(&source)?});
        let tx = self.db.unchecked_transaction()?;
        if tx.execute("UPDATE calendar_sync_state SET revision=revision+1 WHERE account=?1 AND calendar_id=?2 AND revision=?3",params![target.account,target.calendar,revision])?!=1{return Err(err("stale_revision","Calendar changed before create acknowledgement"));}
        let raw = serde_json::to_string(&source)?;
        tx.execute(
            "INSERT INTO calendar_sources(account,calendar_id,event_id,json) VALUES(?1,?2,?3,?4)",
            params![target.account, target.calendar, target.event, raw],
        )?;
        tx.execute("INSERT INTO calendar_event_state(account,calendar_id,event_id,visible,observed_json) VALUES(?1,?2,?3,1,?4)",params![target.account,target.calendar,target.event,raw])?;
        tx.execute("UPDATE calendar_write_operations SET result_json=?1 WHERE account=?2 AND request_id=?3",params![serde_json::to_string(&result)?,target.account,request_id])?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(result)
    }

    pub(crate) fn calendar_operations(&self, account: &str, p: &Value) -> Result<Value> {
        fields(p, &["limit", "cursor"])?;
        let limit = p
            .get("limit")
            .map(|v| {
                v.as_u64()
                    .filter(|v| *v > 0 && *v <= 100)
                    .ok_or_else(|| err("invalid_request", "Operation limit is 1–100"))
            })
            .transpose()?
            .unwrap_or(50);
        let (after_acknowledged, before_id) = match p.get("cursor") {
            None | Some(Value::Null) => (-1_i64, i64::MAX),
            Some(cursor) => {
                fields(cursor, &["acknowledged", "before_id"])?;
                let acknowledged = cursor["acknowledged"]
                    .as_bool()
                    .ok_or_else(|| err("invalid_request", "Invalid operation cursor"))?;
                let before = cursor["before_id"]
                    .as_i64()
                    .filter(|id| *id > 0)
                    .ok_or_else(|| err("invalid_request", "Invalid operation cursor"))?;
                (i64::from(acknowledged), before)
            }
        };
        if self.profile_kind()? == crate::live_profile::ProfileKind::Live {
            return self.calendar_live_operations(account, limit, after_acknowledged, before_id);
        }
        let mut statement=self.db.prepare("SELECT request_id,intent_json,result_json,rowid FROM calendar_write_operations WHERE account=?1 AND json_extract(intent_json,'$.public_fingerprint') IS NOT NULL AND ((result_json IS NOT NULL)>?3 OR ((result_json IS NOT NULL)=?3 AND rowid<?4)) ORDER BY (result_json IS NOT NULL),rowid DESC LIMIT ?2")?;
        let records = statement
            .query_map(
                params![account, limit + 1, after_acknowledged, before_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let has_more = records.len() > limit as usize;
        let mut continuation = Value::Null;
        let mut operations = Vec::new();
        for (request_id, raw, result, row_id) in records.into_iter().take(limit as usize) {
            let intent: Value = serde_json::from_str(&raw)?;
            if has_more {
                continuation = json!({"acknowledged":result.is_some(),"before_id":row_id});
            }
            operations.push(json!({"request_id":request_id,"target":intent["write"]["target"],"kind":intent["request"]["kind"],"state":if result.is_some(){"fake_acknowledged"}else if intent.get("last_error").is_some(){"failed"}else{"pending"},"error":intent["last_error"],"local_only":true}));
        }
        Ok(json!({"operations":operations,"continuation":continuation}))
    }
}

fn parse_date(value: &str) -> Result<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| err("invalid_request", "Calendar needs YYYY-MM-DD dates"))
}
fn mark_actor(body: &mut Value, actor: &str) {
    if let Some(attendees) = body.get_mut("attendees").and_then(Value::as_array_mut) {
        for attendee in attendees {
            if attendee["email"]
                .as_str()
                .is_some_and(|email| email.eq_ignore_ascii_case(actor))
            {
                attendee["email"] = actor.into();
                attendee["self"] = true.into();
            }
        }
    }
}
fn calendar_fields(value: &Value) -> Result<Value> {
    fields(
        value,
        &[
            "title",
            "start",
            "end",
            "all_day",
            "time_zone",
            "location",
            "description",
            "conference_url",
            "reminder_minutes",
            "availability",
            "attendees",
        ],
    )?;
    let title = value["title"]
        .as_str()
        .ok_or_else(|| err("invalid_request", "Event title must be text"))?;
    if title.trim().is_empty() || title.len() > 512 || title.chars().any(char::is_control) {
        return Err(err(
            "invalid_request",
            "Event title must contain 1–512 bytes",
        ));
    }
    let all_day = value["all_day"]
        .as_bool()
        .ok_or_else(|| err("invalid_request", "Choose timed or all-day"))?;
    let zone = string(value, "time_zone")?;
    let start = string(value, "start")?;
    let end = string(value, "end")?;
    let times = |v: &str| {
        if all_day {
            json!({"date":v})
        } else {
            json!({"dateTime":v,"timeZone":zone})
        }
    };
    let mut body = json!({"summary":title,"start":times(start),"end":times(end)});
    for (key, value) in crate::calendar_editor_options::patch(value, "synthetic-editor")?
        .as_object()
        .unwrap()
    {
        body[key] = value.clone();
    }
    for (field, bound) in [("location", 2048), ("description", 32768)] {
        let text = value
            .get(field)
            .map(|v| {
                v.as_str()
                    .ok_or_else(|| err("invalid_request", "Event text must be a string"))
            })
            .transpose()?
            .unwrap_or("");
        if text.len() > bound {
            return Err(err("invalid_request", "Event text exceeds its field limit"));
        }
        body[field] = text.into();
    }
    let conference = value
        .get("conference_url")
        .map(|v| {
            v.as_str()
                .ok_or_else(|| err("invalid_request", "Conference URL must be text"))
        })
        .transpose()?
        .unwrap_or("");
    if !conference.is_empty()
        && (!conference.starts_with("https://")
            || conference.len() > 2048
            || conference.chars().any(char::is_whitespace))
    {
        return Err(err(
            "invalid_request",
            "Conference URL must be an HTTPS URL up to 2,048 bytes",
        ));
    }
    body["conferenceData"] = if conference.is_empty() {
        json!({"entryPoints":[]})
    } else {
        json!({"entryPoints":[{"entryPointType":"video","uri":conference}]})
    };
    if let Some(attendees) = value.get("attendees") {
        let attendees = attendees
            .as_array()
            .filter(|a| a.len() <= 50)
            .ok_or_else(|| {
                err(
                    "invalid_request",
                    "At most 50 attendee email addresses are supported",
                )
            })?;
        let mut seen = BTreeSet::new();
        let mut output = Vec::new();
        for address in attendees {
            let address = address
                .as_str()
                .ok_or_else(|| err("invalid_request", "Attendees must be email addresses"))?;
            if address.len() > 254
                || address.chars().any(char::is_whitespace)
                || !address.contains('@')
                || address.starts_with('@')
                || address.ends_with('@')
            {
                return Err(err("invalid_request", "Invalid attendee email address"));
            }
            if seen.insert(address.to_lowercase()) {
                output.push(json!({"email":address,"responseStatus":"needsAction"}));
            }
        }
        body["attendees"] = output.into();
    }
    // Validating even all-day zones prevents silent timezone typo acceptance.
    fm_calendar::normalize(
        "validate",
        "validate",
        zone,
        &json!({"id":"validate","summary":title,"start":times(start),"end":times(end)}),
    )
    .map_err(calendar_error)?;
    Ok(body)
}

pub(crate) fn seed_phase1_calendars(tx: &rusqlite::Transaction<'_>) -> Result<()> {
    for account in ["windscribe", "personal", "sideproject"] {
        tx.execute("UPDATE calendar_sync_state SET access='owner',sync_token='fixture-complete' WHERE account=?1 AND calendar_id='primary'",[account])?;
        tx.execute("INSERT OR IGNORE INTO calendar_sync_state(account,calendar_id,query_fingerprint,time_zone,access,sync_token,revision) VALUES(?1,'primary','fixture-calendar-v2','America/Toronto','owner','fixture-complete',1)",[account])?;
        let mut events = vec![
            json!({"id":"cross-account","etag":"v1","summary":format!("{} planning",account),"description":"Synthetic planning across separate accounts.","location":"Toronto office","hangoutLink":"https://example.com/meeting","start":{"dateTime":"2026-09-11T10:00:00-04:00","timeZone":"America/Toronto"},"end":{"dateTime":"2026-09-11T11:00:00-04:00","timeZone":"America/Toronto"},"attendees":[{"email":format!("you@{account}.example"),"self":true,"responseStatus":"needsAction"},{"email":"colleague@example.com","responseStatus":"accepted"}]}),
            json!({"id":"early","etag":"v1","summary":format!("{} early call",account),"start":{"dateTime":"2026-09-11T05:30:00-04:00"},"end":{"dateTime":"2026-09-11T06:15:00-04:00"}}),
        ];
        if account == "personal" {
            for n in 0..4 {
                events.push(json!({"id":format!("all-day-{n}"),"etag":"v1","summary":format!("Personal all-day {}",n+1),"start":{"date":"2026-09-11"},"end":{"date":"2026-09-12"}}));
            }
        }
        for mut source in events {
            if let Some(link) = source["hangoutLink"].as_str() {
                source["conferenceData"] =
                    json!({"entryPoints":[{"entryPointType":"video","uri":link}]});
            }
            let raw = serde_json::to_string(&source)?;
            tx.execute("INSERT OR IGNORE INTO calendar_sources(account,calendar_id,event_id,json) VALUES(?1,'primary',?2,?3)",params![account,source["id"].as_str().unwrap(),raw])?;
        }
    }
    tx.execute("INSERT OR IGNORE INTO calendar_sync_state(account,calendar_id,query_fingerprint,time_zone,access,sync_token,revision) VALUES('sideproject','availability','fixture-calendar-v2','America/Toronto','free_busy','fixture-complete',1)",[])?;
    let source = json!({"id":"restricted","etag":"v1","summary":"RESTRICTED PRIVATE TITLE","description":"PRIVATE DESCRIPTION","location":"PRIVATE LOCATION","start":{"dateTime":"2026-09-11T10:30:00-04:00"},"end":{"dateTime":"2026-09-11T11:30:00-04:00"}});
    tx.execute("INSERT OR IGNORE INTO calendar_sources(account,calendar_id,event_id,json) VALUES('sideproject','availability','restricted',?1)",[serde_json::to_string(&source)?])?;
    Ok(())
}

#[cfg(test)]
#[path = "phase1_calendar_tests.rs"]
mod tests;
