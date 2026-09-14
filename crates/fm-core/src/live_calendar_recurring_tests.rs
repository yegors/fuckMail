use super::tests::{event, publish, setup, CAL};
use super::*;
use crate::live_calendar::CalendarSyncMode;
use crate::live_calendar_occurrences::{
    prepare_calendar_occurrence_instance, prepare_calendar_occurrence_master,
};
fn sources() -> (Value, Value) {
    let mut master = event();
    master["id"] = "series".into();
    master["etag"] = "master-v1".into();
    master["recurrence"] = json!(["RRULE:FREQ=DAILY;COUNT=5"]);
    master.as_object_mut().unwrap().remove("extendedProperties");
    master["attendees"] = json!([{"email":CAL,"self":true,"organizer":true,"responseStatus":"accepted"},{"email":"guest@example.test","responseStatus":"tentative","optional":true,"displayName":"Guest"}]);
    let mut instance = master.clone();
    instance.as_object_mut().unwrap().remove("recurrence");
    instance["id"] = "instance".into();
    instance["etag"] = "instance-v1".into();
    instance["recurringEventId"] = "series".into();
    instance["originalStartTime"] = json!({"dateTime":"2026-09-13T09:00:00Z"});
    instance["start"] = json!({"dateTime":"2026-09-13T09:00:00Z"});
    instance["end"] = json!({"dateTime":"2026-09-13T10:00:00Z"});
    (master, instance)
}
fn seed() -> (tempfile::TempDir, Store, String) {
    let (dir, mut store, account) = setup();
    let (m, i) = sources();
    publish(&mut store, &account, vec![m, i], CalendarSyncMode::Full);
    (dir, store, account)
}
fn request(scope: &str, kind: &str, id: &str) -> Value {
    let series = scope == "series";
    let mut request = json!({"request_id":id,"kind":kind,"calendar_id":CAL,"event_id":if series{"series"}else{"instance"},"expected_etag":if series{"master-v1"}else{"instance-v1"},"recurring":{"scope":scope,"series_id":"series","expected_master_etag":"master-v1","original_start":{"kind":"zoned","instant":"2026-09-13T09:00:00Z","time_zone":"UTC"}}});
    if scope == "following" {
        request["recurring"]["reset_future_exceptions"] = true.into();
    }
    if kind == "edit" {
        request["fields"] = json!({"title":"New authored title"});
    }
    if kind == "rsvp" {
        request["response"] = "declined".into();
    }
    request
}
fn proof(store: &Store, account: &str, instance: Value) -> PreparedCalendarOccurrence {
    let epoch = store.calendar_sync_scope(account, CAL).unwrap()["permission_epoch"]
        .as_i64()
        .unwrap();
    let selector=CalendarOccurrenceSelector::parse(&json!({"calendar_id":CAL,"series_id":"series","event_id":"instance","original_start":{"kind":"zoned","instant":"2026-09-13T09:00:00Z","time_zone":"UTC"}})).unwrap();
    let raw:String=store.db.query_row("SELECT json FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id='series'",params![account,CAL],|r|r.get(0)).unwrap();
    let plan = store
        .calendar_live_occurrence_begin(account, selector, 1, epoch)
        .unwrap();
    prepare_calendar_occurrence_instance(
        prepare_calendar_occurrence_master(plan, serde_json::from_str(&raw).unwrap()).unwrap(),
        instance,
    )
    .unwrap()
}
fn stage(store: &mut Store, account: &str, request: &Value) -> CalendarWriteLease {
    let id = request["request_id"].as_str().unwrap();
    let status = store.calendar_live_prepare(account, request).unwrap();
    let epoch = store.calendar_sync_scope(account, CAL).unwrap()["permission_epoch"]
        .as_i64()
        .unwrap();
    if request["recurring"]["scope"] == "series" {
        store
            .calendar_live_stage(
                account,
                id,
                status["revision"].as_i64().unwrap(),
                1,
                epoch,
                Some(sources().0),
            )
            .unwrap();
    } else {
        let p = proof(store, account, sources().1);
        store
            .calendar_live_stage_recurring(account, id, status["revision"].as_i64().unwrap(), p)
            .unwrap();
    }
    store.calendar_live_start(account, id, 1).unwrap()
}
#[test]
fn recurring_create_and_series_rule_edit_preserve_ordinary_version_and_metadata() {
    let (_dir, mut store, account) = seed();
    let create = json!({"request_id":"new-series","kind":"create","calendar_id":CAL,"fields":{"title":"Weekly","start":"2026-09-14T09:00:00Z","end":"2026-09-14T10:00:00Z","all_day":false,"time_zone":"UTC","recurrence":["RRULE:FREQ=WEEKLY;COUNT=8"]}});
    let prepared = store.calendar_live_prepare(&account, &create).unwrap();
    assert_eq!(prepared["intent_version"], 4);
    let epoch = store.calendar_sync_scope(&account, CAL).unwrap()["permission_epoch"]
        .as_i64()
        .unwrap();
    store
        .calendar_live_stage(&account, "new-series", 0, 1, epoch, None)
        .unwrap();
    let lease = store
        .calendar_live_start(&account, "new-series", 1)
        .unwrap();
    assert_eq!(
        lease.dispatch["body"]["recurrence"],
        create["fields"]["recurrence"]
    );
    let mut ack = lease.dispatch["body"].clone();
    ack["id"] = lease.dispatch["target"]["event"].clone();
    ack["etag"] = "created".into();
    store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Accepted(ack))
        .unwrap();
    store.calendar_live_publish(&lease).unwrap();
    let mut edit = request("series", "edit", "edit-series");
    edit["fields"]["recurrence"] = json!(["RRULE:FREQ=WEEKLY;BYDAY=MO,WE;COUNT=10"]);
    let lease = stage(&mut store, &account, &edit);
    assert_eq!(lease.dispatch["version"], 4);
    assert_eq!(
        lease.dispatch["body"]["recurrence"],
        edit["fields"]["recurrence"]
    );
    assert!(lease.dispatch["body"].get("conferenceData").is_none());
    assert_eq!(lease.dispatch["baseline"], sources().0);
}
#[test]
fn attendee_can_resolve_and_respond_to_occurrence_and_series_without_organizer_edit() {
    for scope in ["occurrence", "series"] {
        let (_dir, mut store, account) = seed();
        let (mut master, mut instance) = sources();
        master["organizer"]["self"] = false.into();
        instance["organizer"]["self"] = false.into();
        publish(
            &mut store,
            &account,
            vec![master.clone(), instance.clone()],
            CalendarSyncMode::Full,
        );
        let p = proof(&store, &account, instance.clone());
        let resolved = store
            .calendar_live_recurring_read_finish(p, scope, "rsvp")
            .unwrap();
        assert_eq!(resolved["event"]["can_rsvp"], true);
        assert_eq!(resolved["event"]["can_edit"], false);
        let p = proof(&store, &account, instance.clone());
        assert!(store
            .calendar_live_recurring_read_finish(p, scope, "edit")
            .is_err());
        let req = request(scope, "rsvp", "respond");
        store.calendar_live_prepare(&account, &req).unwrap();
        let epoch = store.calendar_sync_scope(&account, CAL).unwrap()["permission_epoch"]
            .as_i64()
            .unwrap();
        if scope == "series" {
            store
                .calendar_live_stage(&account, "respond", 0, 1, epoch, Some(master.clone()))
                .unwrap();
        } else {
            let p = proof(&store, &account, instance.clone());
            store
                .calendar_live_stage_recurring(&account, "respond", 0, p)
                .unwrap();
        }
        let lease = store.calendar_live_start(&account, "respond", 1).unwrap();
        assert_eq!(
            lease.dispatch["body"]["attendees"],
            json!([{"email":CAL,"responseStatus":"declined"}])
        );
        assert_eq!(lease.dispatch["body"]["attendeesOmitted"], true);
        store
            .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Unknown)
            .unwrap();
        let mut observed = if scope == "series" { master } else { instance };
        observed["etag"] = "read-v2".into();
        observed["attendees"][0]["responseStatus"] = "declined".into();
        let fence = store.calendar_live_reconcile_begin(&lease, 1).unwrap();
        let status = if scope == "series" {
            store
                .calendar_live_reconcile_read(&lease, &fence, observed)
                .unwrap()
        } else {
            let p = proof(&store, &account, observed);
            store
                .calendar_live_reconcile_occurrence_read(&lease, &fence, p)
                .unwrap()
        };
        assert_eq!(status["state"], "reconciled");
        assert_eq!(status["outcome"]["notification_delivery"], "unverified");
    }
}
#[test]
fn following_lost_trim_reconciles_then_explicit_advance_preserves_late_step_evidence() {
    let (dir, mut store, account) = seed();
    let req = request("following", "edit", "split");
    let lease = stage(&mut store, &account, &req);
    assert_eq!(lease.dispatch["kind"], "edit");
    assert_eq!(lease.dispatch["target"]["event"], "series");
    assert_eq!(
        lease.dispatch["body"]["recurrence"],
        json!(["RRULE:COUNT=1;FREQ=DAILY"])
    );
    let replacement = lease.dispatch["following"]["replacement_body"].clone();
    assert_eq!(
        replacement["recurrence"],
        json!(["RRULE:COUNT=4;FREQ=DAILY"])
    );
    assert_eq!(replacement["summary"], "New authored title");
    assert_eq!(replacement["attachments"], sources().0["attachments"]);
    assert!(replacement["conferenceData"].get("entryPoints").is_none());
    assert!(replacement["conferenceData"]["createRequest"]["requestId"].is_string());
    store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Unknown)
        .unwrap();
    assert_eq!(
        store.calendar_live_prepare(&account, &req).unwrap()["state"],
        "unknown"
    );
    assert!(store.calendar_live_request(&account, "split").is_err());
    let mut trimmed = sources().0;
    trimmed["recurrence"] = lease.dispatch["body"]["recurrence"].clone();
    trimmed["etag"] = "trimmed-v2".into();
    let fence = store.calendar_live_reconcile_begin(&lease, 1).unwrap();
    assert_eq!(
        store
            .calendar_live_reconcile_read(&lease, &fence, trimmed.clone())
            .unwrap()["state"],
        "reconciled"
    );
    store.calendar_live_publish(&lease).unwrap();
    let partial = store.calendar_live_recurring_finish_step(&lease).unwrap();
    assert_eq!(partial["state"], "partial");
    assert_eq!(partial["steps"][0]["state"], "reconciled");
    assert_eq!(partial["can_resume"], true);
    assert_eq!(
        store.calendar_live_prepare(&account, &req).unwrap()["state"],
        "partial"
    );
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    assert_eq!(
        store.calendar_live_status(&account, "split").unwrap()["state"],
        "partial"
    );
    let epoch = store.calendar_sync_scope(&account, CAL).unwrap()["permission_epoch"]
        .as_i64()
        .unwrap();
    store
        .calendar_live_following_advance(&account, "split", 1, epoch, trimmed.clone())
        .unwrap();
    let next = store.calendar_live_start(&account, "split", 1).unwrap();
    assert_eq!(next.dispatch["kind"], "create");
    assert_eq!(next.dispatch["body"], replacement);
    let before = serde_json::to_value(&next).unwrap();
    store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Accepted(trimmed))
        .unwrap();
    assert_eq!(
        store.calendar_live_status(&account, "split").unwrap()["state"],
        "in_flight"
    );
    assert_eq!(
        serde_json::to_value(
            store
                .calendar_live_operation(&account, "split")
                .unwrap()
                .lease
                .unwrap()
        )
        .unwrap(),
        before
    );
    store
        .calendar_live_record_outcome(&next, CalendarWriteOutcome::Unknown)
        .unwrap();
    assert_eq!(
        store.calendar_live_status(&account, "split").unwrap()["can_resume"],
        false
    );
    assert!(store
        .calendar_live_following_advance(&account, "split", 1, epoch, sources().0)
        .is_err());
}
#[test]
fn unsupported_following_source_and_rule_refuse_before_claim_and_trim_only_cancel_has_no_insert() {
    let (_dir, mut store, account) = seed();
    let req = request("following", "delete", "trim");
    let mut bad = sources().0;
    bad["extendedProperties"] = json!({"private":{"unrepresentable":"kept"}});
    publish(
        &mut store,
        &account,
        vec![bad, sources().1],
        CalendarSyncMode::Full,
    );
    store.calendar_live_prepare(&account, &req).unwrap();
    let p = proof(&store, &account, sources().1);
    assert_eq!(
        store
            .calendar_live_stage_recurring(&account, "trim", 0, p)
            .unwrap_err()
            .code,
        "unsupported_operation"
    );
    assert!(store.calendar_live_start(&account, "trim", 1).is_err());
    publish(
        &mut store,
        &account,
        vec![sources().0, sources().1],
        CalendarSyncMode::Full,
    );
    let p = proof(&store, &account, sources().1);
    store
        .calendar_live_stage_recurring(&account, "trim", 0, p)
        .unwrap();
    let lease = store.calendar_live_start(&account, "trim", 1).unwrap();
    assert!(lease.dispatch["following"]["replacement_body"].is_null());
    let mut actual = sources().0;
    actual["recurrence"] = lease.dispatch["body"]["recurrence"].clone();
    actual["etag"] = "trimmed".into();
    store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Accepted(actual))
        .unwrap();
    store.calendar_live_publish(&lease).unwrap();
    let result = store.calendar_live_recurring_finish_step(&lease).unwrap();
    assert_eq!(result["following_complete"], true);
    assert_eq!(result["can_resume"], false);
    assert_eq!(result["steps"].as_array().unwrap().len(), 1);
}

#[test]
fn following_resume_refreshes_unpublished_trim_but_refuses_newer_or_absent_master() {
    for replacement in [None, Some(false), Some(true)] {
        let (_dir, mut store, account) = seed();
        let lease = stage(&mut store, &account, &request("following", "edit", "split"));
        let mut trimmed = sources().0;
        trimmed["recurrence"] = lease.dispatch["body"]["recurrence"].clone();
        trimmed["etag"] = "trimmed-v2".into();
        store
            .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Accepted(trimmed.clone()))
            .unwrap();
        // Simulate cancellation after durable ACK and before cache publication.
        store.calendar_live_recurring_finish_step(&lease).unwrap();
        if let Some(present) = replacement {
            let mut newer = trimmed.clone();
            newer["summary"] = "Newer provider edit".into();
            newer["etag"] = "newer-v3".into();
            publish(
                &mut store,
                &account,
                if present { vec![newer] } else { vec![] },
                CalendarSyncMode::Full,
            );
        }
        let epoch = store.calendar_sync_scope(&account, CAL).unwrap()["permission_epoch"]
            .as_i64()
            .unwrap();
        let result =
            store.calendar_live_following_advance(&account, "split", 1, epoch, trimmed.clone());
        if replacement.is_some() {
            assert_eq!(result.unwrap_err().code, "stale_revision");
            assert_eq!(
                store.calendar_live_status(&account, "split").unwrap()["state"],
                "partial"
            );
        } else {
            assert_eq!(result.unwrap()["state"], "ready");
            let raw:String=store.db.query_row("SELECT json FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id='series'",params![account,CAL],|r|r.get(0)).unwrap();
            assert_eq!(serde_json::from_str::<Value>(&raw).unwrap(), trimmed);
            assert_eq!(
                store.calendar_live_status(&account, "split").unwrap()["steps"][0]["state"],
                "acknowledged"
            );
        }
    }
}

#[test]
fn legacy_occurrence_cannot_bypass_pending_v4_series_but_exact_replay_survives() {
    let (_dir, mut store, account) = seed();
    let mut legacy = request("occurrence", "edit", "legacy");
    legacy["occurrence"] = legacy["recurring"].clone();
    legacy["occurrence"]
        .as_object_mut()
        .unwrap()
        .remove("scope");
    legacy.as_object_mut().unwrap().remove("recurring");
    let prepared = store.calendar_live_prepare(&account, &legacy).unwrap();
    let held = proof(&store, &account, sources().1);
    let pending = stage(
        &mut store,
        &account,
        &request("series", "edit", "series-pending"),
    );
    store
        .calendar_live_record_outcome(&pending, CalendarWriteOutcome::Unknown)
        .unwrap();
    store
        .calendar_live_stage_occurrence(
            &account,
            "legacy",
            prepared["revision"].as_i64().unwrap(),
            held,
        )
        .unwrap();
    assert_eq!(
        store
            .calendar_live_start(&account, "legacy", 1)
            .unwrap_err()
            .code,
        "calendar_operation_pending"
    );
    assert_eq!(
        store.calendar_live_prepare(&account, &legacy).unwrap()["state"],
        "ready"
    );
    legacy["request_id"] = "new-legacy".into();
    assert_eq!(
        store
            .calendar_live_prepare(&account, &legacy)
            .unwrap_err()
            .code,
        "calendar_operation_pending"
    );
}
#[test]
fn following_preserves_selected_exception_content_and_exact_source_across_reopen_and_steps() {
    let (dir, mut store, account) = seed();
    let mut selected = sources().1;
    selected["etag"] = "exception-v2".into();
    selected["summary"] = "Selected exception title".into();
    selected["location"] = "Selected exception room".into();
    selected["description"] = "Selected exception notes".into();
    selected["attachments"] =
        json!([{"fileUrl":"https://drive.example.test/exception","title":"Exact exception file"}]);
    selected["attendees"][1] = json!({"email":"exception-guest@example.test","responseStatus":"accepted","optional":true,"comment":"Keep this RSVP"});
    selected["conferenceData"]["entryPoints"][0]["uri"] =
        "https://meet.example.test/exception".into();
    let preview = store
        .calendar_live_recurring_read_finish(
            proof(&store, &account, selected.clone()),
            "following",
            "edit",
        )
        .unwrap();
    assert_eq!(preview["event"]["title"], selected["summary"]);
    let mut req = request("following", "edit", "exception-split");
    req["expected_etag"] = selected["etag"].clone();
    req["fields"] = json!({"description":"Only this field was authored"});
    let prepared = store.calendar_live_prepare(&account, &req).unwrap();
    let held = proof(&store, &account, selected.clone());
    store
        .calendar_live_stage_recurring(
            &account,
            "exception-split",
            prepared["revision"].as_i64().unwrap(),
            held,
        )
        .unwrap();
    let frozen = store
        .calendar_live_operation(&account, "exception-split")
        .unwrap()
        .lease
        .unwrap()
        .dispatch;
    assert_eq!(frozen["baseline"], sources().0);
    assert_eq!(frozen["selected_instance_baseline"], selected);
    let replacement = &frozen["following"]["replacement_body"];
    for name in ["summary", "location", "attachments", "start", "end"] {
        assert_eq!(replacement[name], selected[name], "{name}");
    }
    let people = replacement["attendees"].as_array().unwrap();
    assert_eq!(people.len(), 2);
    assert_eq!(
        people
            .iter()
            .find(|person| person["email"] == "exception-guest@example.test")
            .unwrap(),
        &selected["attendees"][1]
    );
    assert_eq!(
        people.iter().find(|person| person["email"] == CAL).unwrap(),
        &json!({"email":CAL,"responseStatus":"accepted"})
    );
    assert_eq!(replacement["description"], req["fields"]["description"]);
    assert_eq!(
        replacement["recurrence"],
        json!(["RRULE:COUNT=4;FREQ=DAILY"])
    );
    assert!(replacement.get("recurringEventId").is_none());
    assert!(replacement.get("originalStartTime").is_none());
    assert!(replacement["conferenceData"].get("entryPoints").is_none());
    assert!(replacement["conferenceData"]["createRequest"].is_object());
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    assert_eq!(
        store
            .calendar_live_operation(&account, "exception-split")
            .unwrap()
            .lease
            .unwrap()
            .dispatch,
        frozen
    );
    let trim = store
        .calendar_live_start(&account, "exception-split", 1)
        .unwrap();
    let mut ack = sources().0;
    ack["etag"] = "trim-v2".into();
    ack["recurrence"] = trim.dispatch["body"]["recurrence"].clone();
    store
        .calendar_live_record_outcome(&trim, CalendarWriteOutcome::Accepted(ack.clone()))
        .unwrap();
    store.calendar_live_publish(&trim).unwrap();
    store.calendar_live_recurring_finish_step(&trim).unwrap();
    let epoch = store.calendar_sync_scope(&account, CAL).unwrap()["permission_epoch"]
        .as_i64()
        .unwrap();
    store
        .calendar_live_following_advance(&account, "exception-split", 1, epoch, ack)
        .unwrap();
    let next = store
        .calendar_live_operation(&account, "exception-split")
        .unwrap()
        .lease
        .unwrap();
    assert_eq!(next.dispatch["body"], *replacement);
    assert_eq!(next.dispatch["selected_instance_baseline"], selected);
    let prior:String=store.db.query_row("SELECT recurring_steps_json FROM calendar_write_operations WHERE account=?1 AND request_id='exception-split'",[&account],|r|r.get(0)).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&prior).unwrap()[0]["lease"]["dispatch"]
            ["selected_instance_baseline"],
        selected
    );
}

#[test]
fn following_refuses_unsupported_selected_exception_before_any_trim() {
    let (_dir, mut store, account) = seed();
    let mut selected = sources().1;
    selected["extendedProperties"] = json!({"private":{"unsupported":"retain exact source"}});
    let error = store
        .calendar_live_recurring_read_finish(
            proof(&store, &account, selected.clone()),
            "following",
            "edit",
        )
        .unwrap_err();
    assert_eq!(error.code, "unsupported_operation");
    let prepared = store
        .calendar_live_prepare(&account, &request("following", "edit", "unsafe-exception"))
        .unwrap();
    let held = proof(&store, &account, selected);
    assert_eq!(
        store
            .calendar_live_stage_recurring(
                &account,
                "unsafe-exception",
                prepared["revision"].as_i64().unwrap(),
                held
            )
            .unwrap_err()
            .code,
        "unsupported_operation"
    );
    let op = store
        .calendar_live_operation(&account, "unsafe-exception")
        .unwrap();
    assert_eq!(op.state, "prepared");
    assert!(op.lease.is_none());
    assert!(store
        .calendar_live_start(&account, "unsafe-exception", 1)
        .is_err());
}

#[test]
fn recurring_projection_keeps_occurrence_rsvp_and_guests_after_master_response_and_reopen() {
    let (dir, mut store, account) = seed();
    store.preferences_save(&json!({"request_id":"projection-zone","expected_revision":0,"preferences":{"calendar_display_time_zone":"UTC"}})).unwrap();
    let lease = stage(
        &mut store,
        &account,
        &request("occurrence", "rsvp", "local-response"),
    );
    let mut selected = sources().1;
    selected["etag"] = "instance-response-v2".into();
    selected["attendees"][0]["responseStatus"] = "declined".into();
    store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Accepted(selected.clone()))
        .unwrap();
    assert_eq!(
        store.calendar_live_publish(&lease).unwrap()["outcome"]["cache_published"],
        true
    );
    let query = json!({"from":"2026-09-12","to":"2026-09-15"});
    let assert_rows = |store: &Store, master_response: &str, instance: &Value| {
        let page = store.calendar_combined(&query).unwrap();
        assert_eq!(page["total"], 3);
        for row in page["events"].as_array().unwrap() {
            let selected_row =
                row["occurrence_key"]["original_start"] == "2026-09-13T09:00:00+00:00";
            assert_eq!(
                row["self_response"],
                if selected_row {
                    "declined"
                } else {
                    master_response
                }
            );
            if selected_row {
                let guests = row["attendees"].as_array().unwrap();
                assert_eq!(guests.len(), 2);
                assert!(guests
                    .iter()
                    .any(|p| p["email"] == instance["attendees"][1]["email"]));
            }
            // Cached recurrence rows still require explicit resolution; an
            // instance receipt cannot grant direct editing or change authority.
            assert_eq!(row["can_edit"], false);
            assert_eq!(row["can_edit_attendees"], false);
            assert_eq!(row["busy_only"], false);
        }
    };
    assert_rows(&store, "accepted", &selected);
    let mut master = sources().0;
    master["etag"] = "master-response-v2".into();
    master["attendees"][0]["responseStatus"] = "tentative".into();
    selected["attendees"][1]["email"] = "exception-guest@example.test".into();
    publish(
        &mut store,
        &account,
        vec![master.clone(), selected.clone()],
        CalendarSyncMode::Full,
    );
    assert_rows(&store, "tentative", &selected);
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    assert_rows(&store, "tentative", &selected);
    master["visibility"] = "private".into();
    selected["visibility"] = "public".into();
    publish(
        &mut store,
        &account,
        vec![master, selected],
        CalendarSyncMode::Full,
    );
    let epoch = store.calendar_sync_scope(&account, CAL).unwrap()["permission_epoch"]
        .as_i64()
        .unwrap();
    store
        .calendar_observe_permission(
            &account,
            CAL,
            1,
            epoch,
            Access::WriterWithoutPrivateAccess,
            "UTC",
        )
        .unwrap();
    let page = store.calendar_combined(&query).unwrap();
    assert_eq!(page["total"], 3);
    for row in page["events"].as_array().unwrap() {
        assert_eq!(row["busy_only"], true);
        assert!(row.get("self_response").is_none());
        assert!(row.get("attendees").is_none());
        assert_eq!(row["can_resolve_occurrence"], false);
    }
}

#[test]
fn schema_nineteen_upgrade_preserves_legacy_journal_and_refuses_old_engine_version() {
    let (dir, mut store, account) = setup();
    let legacy = super::tests::prepare(
        &mut store,
        &account,
        super::tests::request("delete"),
        Some(event()),
    );
    store
        .calendar_live_record_outcome(&legacy, CalendarWriteOutcome::Deleted)
        .unwrap();
    let before:(String,String,String)=store.db.query_row("SELECT fingerprint,intent_json,outcome_json FROM calendar_write_operations WHERE account=?1",[&account],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    crate::test_profiles::restore_schema_nineteen(&store.db);
    drop(store);
    let store = Store::open_live(dir.path()).unwrap();
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        20
    );
    let after:(String,String,String)=store.db.query_row("SELECT fingerprint,intent_json,outcome_json FROM calendar_write_operations WHERE account=?1",[&account],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(before, after);
    assert_eq!(
        store.calendar_live_status(&account, "delete").unwrap()["intent_version"],
        2
    );
    store.db.pragma_update(None, "user_version", 21).unwrap();
    drop(store);
    assert_eq!(
        Store::open_live(dir.path()).err().unwrap().code,
        "schema_mismatch"
    );
}
