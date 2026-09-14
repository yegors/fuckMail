use super::tests::{event, publish, setup, CAL};
use super::*;
use crate::live_calendar::{CalendarPage, CalendarSyncMode};
use crate::live_calendar_occurrences::{
    prepare_calendar_occurrence_instance, prepare_calendar_occurrence_master,
    CalendarOccurrenceMasterProof,
};

fn master() -> Value {
    let mut value = event();
    value["id"] = "series".into();
    value["etag"] = "master-v1".into();
    value["recurrence"] = json!(["RRULE:FREQ=DAILY;COUNT=3"]);
    value
}
fn instance() -> Value {
    let mut value = event();
    value["id"] = "instance".into();
    value["etag"] = "instance-v1".into();
    value["recurringEventId"] = "series".into();
    value["originalStartTime"] = json!({"dateTime":"2026-09-13T09:00:00Z"});
    value["start"] = json!({"dateTime":"2026-11-12T09:00:00Z"});
    value["end"] = json!({"dateTime":"2026-11-12T10:00:00Z"});
    value["eventLabelId"] = "retained-label".into();
    value["attendees"] = json!([
        {"email":CAL,"self":true,"organizer":true,"responseStatus":"accepted"},
        {"email":"guest@example.test","responseStatus":"tentative","displayName":"Guest","optional":true}
    ]);
    value
}
fn original() -> Value {
    json!({"kind":"zoned","instant":"2026-09-13T09:00:00Z","time_zone":"UTC"})
}
fn selector() -> CalendarOccurrenceSelector {
    CalendarOccurrenceSelector::parse(&json!({"calendar_id":CAL,"series_id":"series","original_start":original(),"event_id":"instance"})).unwrap()
}
fn seeded() -> (tempfile::TempDir, Store, String) {
    let (dir, mut store, account) = setup();
    store.preferences_save(&json!({"request_id":"display-zone","expected_revision":0,"preferences":{"calendar_display_time_zone":"UTC"}})).unwrap();
    publish(
        &mut store,
        &account,
        vec![master(), instance()],
        CalendarSyncMode::Full,
    );
    (dir, store, account)
}

#[test]
fn all_day_identity_and_effective_generation_survive_source_consolidation() {
    let (_dir, mut store, account) = seeded();
    let mut master = master();
    master["start"] = json!({"date":"2026-09-12"});
    master["end"] = json!({"date":"2026-09-13"});
    let mut instance = instance();
    instance["originalStartTime"] = json!({"date":"2026-09-13"});
    instance["start"] = json!({"date":"2026-11-12"});
    instance["end"] = json!({"date":"2026-11-13"});
    let run = store
        .calendar_sync_begin(&account, CAL, CalendarSyncMode::Full)
        .unwrap();
    let page = CalendarPage {
        input_cursor: None,
        items: vec![master.clone(), instance.clone()],
        next_page_token: None,
        next_sync_token: Some("all-day".into()),
    };
    store.calendar_stage_page_chunk(&run, &page, 0).unwrap();
    store.calendar_seal_page(&run, &page).unwrap();
    store.calendar_publish_sync(&run).unwrap();
    let selector=CalendarOccurrenceSelector::parse(&json!({"calendar_id":CAL,"series_id":"series","event_id":"instance","original_start":{"kind":"date","date":"2026-09-13"}})).unwrap();
    let scope = store.calendar_sync_scope(&account, CAL).unwrap();
    let plan = store
        .calendar_live_occurrence_begin(
            &account,
            selector,
            1,
            scope["permission_epoch"].as_i64().unwrap(),
        )
        .unwrap();
    let master_proof = prepare_calendar_occurrence_master(plan, master).unwrap();
    let held =
        prepare_calendar_occurrence_instance(master_proof.clone(), instance.clone()).unwrap();
    while store.calendar_finalize_chunk(&run).unwrap() > 0 {}
    let result = store.calendar_live_occurrence_finish(held).unwrap();
    assert_eq!(
        result["occurrence"]["original_start"],
        json!({"kind":"date","date":"2026-09-13"})
    );
    assert_eq!(result["event"]["all_day"], true);
    instance["originalStartTime"] = json!({"dateTime":"2026-09-13T00:00:00Z"});
    assert_eq!(
        prepare_calendar_occurrence_instance(master_proof, instance)
            .err()
            .unwrap()
            .code,
        "scope_mismatch"
    );
}

#[test]
fn current_detail_read_grants_can_reconcile_but_cannot_open_editable_or_dispatch() {
    let (_dir, mut store, account) = seeded();
    let lease = dispatch(&mut store, &account, "edit");
    store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Unknown)
        .unwrap();
    let scopes = std::collections::BTreeSet::from([
        "https://www.googleapis.com/auth/calendar.events.readonly".into(),
        "https://www.googleapis.com/auth/calendar.calendarlist.readonly".into(),
    ]);
    store
        .bind_google_account("calendar-write-subject", CAL, &scopes, Some(&account))
        .unwrap();
    let generation = store.calendar_occurrence_read_connection(&account).unwrap();
    let epoch = store.calendar_sync_scope(&account, CAL).unwrap()["permission_epoch"]
        .as_i64()
        .unwrap();
    store
        .calendar_observe_permission(&account, CAL, generation, epoch, Access::Owner, "UTC")
        .unwrap();
    let fence = store
        .calendar_live_reconcile_begin(&lease, generation)
        .unwrap();
    let mut actual = instance();
    actual["summary"] = "Authored occurrence".into();
    actual["etag"] = "observed-v2".into();
    let editable = proof(&store, &account, actual.clone());
    assert_eq!(
        store
            .calendar_live_occurrence_finish(editable)
            .err()
            .unwrap()
            .code,
        "permission_denied"
    );
    let observed = proof(&store, &account, actual.clone());
    assert_eq!(
        store
            .calendar_live_reconcile_occurrence_read(&lease, &fence, observed)
            .unwrap()["state"],
        "reconciled"
    );
    assert_eq!(
        store.calendar_live_publish(&lease).unwrap()["outcome"]["cache_published"],
        true
    );
    assert_eq!(source(&store, &account, "instance"), actual);
    let mut new_request = request("edit");
    new_request["request_id"] = "new-edit".into();
    assert_eq!(
        store
            .calendar_live_prepare(&account, &new_request)
            .err()
            .unwrap()
            .code,
        "permission_denied"
    );
}

#[test]
fn removed_restored_epoch_never_revives_old_proof_and_late_abandoned_ack_is_retained() {
    use crate::account_lifecycle::{AccountCleanupOutcome, AccountRemovalStep};
    let (_dir, mut store, account) = seeded();
    let lease = dispatch(&mut store, &account, "edit");
    let unknown = store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Unknown)
        .unwrap();
    store
        .calendar_live_cancel(&account, "edit", unknown["revision"].as_i64().unwrap())
        .unwrap();
    let held = proof(&store, &account, instance());
    let fence = store.calendar_live_reconcile_begin(&lease, 1).unwrap();
    let revision = store.account_lifecycle(&account).unwrap()["revision"].clone();
    let cleanup = match store
        .account_remove_begin(
            &account,
            &json!({"expected_revision":revision,"request_id":"remove","retention":"archive"}),
        )
        .unwrap()
    {
        AccountRemovalStep::Cleanup(l) => l,
        _ => panic!("fresh cleanup"),
    };
    let cleanup = store.account_cleanup_claim(&cleanup).unwrap();
    store
        .account_cleanup_finish(&cleanup, AccountCleanupOutcome::Removed)
        .unwrap();
    let revision = store.account_lifecycle(&account).unwrap()["revision"]
        .as_i64()
        .unwrap();
    let scopes = std::collections::BTreeSet::from([
        "https://www.googleapis.com/auth/calendar.events".into(),
        "https://www.googleapis.com/auth/calendar.calendarlist.readonly".into(),
    ]);
    let restore = store
        .google_restore_admit(
            &account,
            revision,
            "calendar-write-subject",
            CAL,
            &scopes,
            "desktop-client",
        )
        .unwrap();
    let path = store.profile.join("desktop-client.json");
    store
        .google_restore_bind(&restore, path.to_str().unwrap())
        .unwrap();
    assert_eq!(
        store.account_lifecycle(&account).unwrap()["state"],
        "active"
    );
    assert!(store
        .calendar_live_reconcile_occurrence_read(&lease, &fence, held)
        .is_err());
    let mut actual = instance();
    actual["summary"] = "Authored occurrence".into();
    actual["etag"] = "late-ack".into();
    let ack = store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Accepted(actual))
        .unwrap();
    assert_eq!(ack["state"], "accepted");
    assert_eq!(ack["outcome"]["notification_delivery"], "unverified");
    assert_eq!(
        store.calendar_live_publish(&lease).unwrap()["outcome"]["cache_published"],
        false
    );
    assert_eq!(source(&store, &account, "instance"), instance());
}
fn qualify(store: &Store, account: &str, source: Value) -> Result<CalendarOccurrenceMasterProof> {
    let epoch = store.calendar_sync_scope(account, CAL)?["permission_epoch"]
        .as_i64()
        .unwrap();
    let generation = store.calendar_occurrence_read_connection(account)?;
    let plan = store.calendar_live_occurrence_begin(account, selector(), generation, epoch)?;
    prepare_calendar_occurrence_master(plan, source)
}
fn proof(store: &Store, account: &str, source: Value) -> PreparedCalendarOccurrence {
    prepare_calendar_occurrence_instance(qualify(store, account, master()).unwrap(), source)
        .unwrap()
}
fn request(kind: &str) -> Value {
    let mut value = json!({"request_id":kind,"kind":kind,"calendar_id":CAL,"event_id":"instance","expected_etag":"instance-v1","occurrence":{"series_id":"series","original_start":original(),"expected_master_etag":"master-v1"}});
    if kind == "edit" {
        value["fields"] = json!({"title":"Authored occurrence"});
    }
    value
}
fn stage(store: &mut Store, account: &str, kind: &str) -> Value {
    store
        .calendar_live_prepare(account, &request(kind))
        .unwrap();
    let proof = proof(store, account, instance());
    store
        .calendar_live_stage_occurrence(account, kind, 0, proof)
        .unwrap()
}
fn dispatch(store: &mut Store, account: &str, kind: &str) -> CalendarWriteLease {
    stage(store, account, kind);
    store.calendar_live_start(account, kind, 1).unwrap()
}
fn source(store: &Store, account: &str, id: &str) -> Value {
    let raw: String = store
        .db
        .query_row(
            "SELECT json FROM calendar_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",
            params![account, CAL, id],
            |r| r.get(0),
        )
        .unwrap();
    serde_json::from_str(&raw).unwrap()
}
fn empty_catalog(store: &mut Store, account: &str) {
    let run = store
        .calendar_catalog_begin(account, CalendarSyncMode::Full)
        .unwrap();
    let page = CalendarPage {
        input_cursor: None,
        items: vec![],
        next_page_token: None,
        next_sync_token: Some("empty".into()),
    };
    store.calendar_stage_page_chunk(&run, &page, 0).unwrap();
    store.calendar_seal_page(&run, &page).unwrap();
    while store.calendar_catalog_finalize_chunk(&run).unwrap() > 0 {}
    store.calendar_publish_sync(&run).unwrap();
}
fn remove(store: &mut Store, account: &str) {
    let revision = store.account_lifecycle(account).unwrap()["revision"].clone();
    store
        .account_remove_begin(
            account,
            &json!({"expected_revision":revision,"request_id":"remove","retention":"archive"}),
        )
        .unwrap();
}

#[test]
fn opaque_qualification_rejects_wrong_identity_semantics_and_bounded_sources() {
    let (_dir, store, account) = seeded();
    for invalid in [
        json!({"calendar_id":CAL,"series_id":"series","original_start":original(),"scope":"series"}),
        json!({"calendar_id":CAL,"series_id":"series","original_start":{"kind":"date","date":"2026-09-13","time_zone":"UTC"}}),
        json!({"calendar_id":CAL,"series_id":"series","original_start":{"kind":"zoned","instant":"2026-09-13T09:00:00Z","time_zone":"invalid/zone"}}),
        json!({"calendar_id":CAL,"series_id":"series","original_start":original(),"event_id":null}),
    ] {
        assert!(CalendarOccurrenceSelector::parse(&invalid).is_err());
    }
    let mut changed = master();
    changed["summary"] = "Changed without an ETag change".into();
    assert_eq!(
        qualify(&store, &account, changed).err().unwrap().code,
        "stale_revision"
    );
    let mut huge = master();
    huge["description"] = "x".repeat(256 * 1024).into();
    assert!(qualify(&store, &account, huge).is_err());
    for field in ["id", "recurringEventId", "originalStartTime"] {
        let mut changed = instance();
        changed[field] = match field {
            "originalStartTime" => json!({"date":"2026-09-13"}),
            _ => "foreign".into(),
        };
        assert_eq!(
            prepare_calendar_occurrence_instance(
                qualify(&store, &account, master()).unwrap(),
                changed
            )
            .err()
            .unwrap()
            .code,
            "scope_mismatch"
        );
    }
    let mut equivalent = instance();
    equivalent["originalStartTime"] =
        json!({"dateTime":"2026-09-13T05:00:00-04:00","timeZone":"America/Toronto"});
    assert!(prepare_calendar_occurrence_instance(
        qualify(&store, &account, master()).unwrap(),
        equivalent
    )
    .is_ok());
    let sparse = json!({"id":"instance","status":"cancelled","recurringEventId":"series","originalStartTime":instance()["originalStartTime"]});
    let proof = proof(&store, &account, sparse);
    assert!(
        store.calendar_live_occurrence_finish(proof).is_err(),
        "sparse evidence is never an editable event"
    );
}

#[test]
fn resolved_moved_editor_and_flat_selectors_use_verified_instance_identity() {
    let (_dir, store, account) = seeded();
    let resolved = store
        .calendar_live_occurrence_finish(proof(&store, &account, instance()))
        .unwrap();
    assert_eq!(resolved["event_id"], "instance");
    assert_eq!(resolved["etag"], "instance-v1");
    assert_eq!(resolved["event"]["account"], account);
    assert_eq!(resolved["event"]["calendar_id"], CAL);
    assert_eq!(resolved["event"]["event_id"], resolved["event_id"]);
    assert_eq!(resolved["event"]["etag"], resolved["etag"]);
    assert_eq!(resolved["event"]["can_edit"], true);
    assert_eq!(resolved["event"]["can_edit_attendees"], true);
    assert_eq!(
        resolved["event"]["guest_addresses"],
        json!(["guest@example.test"])
    );
    let expanded = store
        .calendar_combined(&json!({"from":"2026-09-12","to":"2026-09-15"}))
        .unwrap();
    assert_eq!(expanded["total"], 2);
    for row in expanded["events"].as_array().unwrap() {
        assert_eq!(row["can_resolve_occurrence"], true);
        let selector = CalendarOccurrenceSelector::parse(&row["occurrence_selector"]).unwrap();
        assert_eq!(selector.series_id(), "series");
        assert_eq!(
            selector.event_id(),
            None,
            "an expanded master row is not an instance ID"
        );
    }
    let moved = store
        .calendar_combined(&json!({"from":"2026-11-12","to":"2026-11-13"}))
        .unwrap();
    assert_eq!(moved["total"], 1);
    assert_eq!(
        moved["events"][0]["occurrence_selector"]["event_id"],
        "instance"
    );
    assert_eq!(
        moved["events"][0]["occurrence_selector"]["original_start"],
        original()
    );
}

#[test]
fn v3_dispatch_preserves_exact_intent_instance_and_compact_master_across_restart() {
    let (dir, mut store, account) = seeded();
    let ready = stage(&mut store, &account, "edit");
    assert_eq!(ready["state"], "ready");
    assert_eq!(ready["intent_version"], 3);
    assert_eq!(ready["occurrence"], request("edit")["occurrence"]);
    let op = store.calendar_live_operation(&account, "edit").unwrap();
    let dispatch = op.lease.unwrap().dispatch;
    assert_eq!(op.intent["version"], 3);
    assert_eq!(op.intent["request"], request("edit"));
    assert_eq!(dispatch["version"], 3);
    assert_eq!(dispatch["baseline"], instance());
    assert_eq!(
        dispatch["body"],
        json!({"summary":"Authored occurrence","eventLabelId":"retained-label"})
    );
    assert_eq!(
        dispatch["original_start_provider"],
        instance()["originalStartTime"]
    );
    assert_eq!(
        dispatch["master_proof"]["source_sha256"],
        crate::calendar_store::fingerprint(&master()).unwrap()
    );
    assert_eq!(
        dispatch["master_proof"]["recurrence_sha256"],
        crate::calendar_store::fingerprint(&master()["recurrence"]).unwrap()
    );
    assert!(!dispatch["master_proof"]
        .to_string()
        .contains("Retained description"));
    assert!(dispatch["master_proof"].to_string().len() < 2048);
    assert_eq!(
        store
            .calendar_live_prepare(&account, &request("edit"))
            .unwrap(),
        ready
    );
    let mut changed = request("edit");
    changed["fields"]["title"] = "Other".into();
    assert!(store.calendar_live_prepare(&account, &changed).is_err());
    let epoch = store.calendar_sync_scope(&account, CAL).unwrap()["permission_epoch"]
        .as_i64()
        .unwrap();
    assert!(store
        .calendar_live_stage(&account, "edit", 1, 1, epoch, Some(instance()))
        .is_err());
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    assert_eq!(
        store.calendar_live_request(&account, "edit").unwrap(),
        request("edit")
    );
    let lease = store.calendar_live_start(&account, "edit", 1).unwrap();
    assert_eq!(lease.dispatch, dispatch);
    assert_eq!(lease.attempt, 1);
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    assert_eq!(
        store.calendar_live_status(&account, "edit").unwrap()["state"],
        "unknown"
    );
    assert_eq!(
        store
            .calendar_live_prepare(&account, &request("edit"))
            .unwrap()["state"],
        "unknown"
    );
    assert!(store.calendar_live_start(&account, "edit", 1).is_err());
    assert_eq!(
        store
            .calendar_live_operation(&account, "edit")
            .unwrap()
            .lease
            .unwrap()
            .attempt,
        1
    );
}

#[test]
fn held_occurrence_proofs_and_ready_dispatches_recheck_all_current_authority() {
    for change in [
        "master",
        "missing_etag",
        "cancelled_master",
        "permission",
        "membership",
        "connection",
        "removed",
    ] {
        let (_dir, mut store, account) = seeded();
        let held = proof(&store, &account, instance());
        stage(&mut store, &account, "edit");
        match change {
            "master" => {
                let mut updated = master();
                updated["etag"] = "master-v2".into();
                publish(&mut store, &account, vec![updated], CalendarSyncMode::Delta);
            }
            "missing_etag" => {
                let mut updated = master();
                updated.as_object_mut().unwrap().remove("etag");
                publish(&mut store, &account, vec![updated], CalendarSyncMode::Delta);
            }
            "cancelled_master" => publish(
                &mut store,
                &account,
                vec![json!({"id":"series","status":"cancelled"})],
                CalendarSyncMode::Delta,
            ),
            "permission" => {
                let epoch = store.calendar_sync_scope(&account, CAL).unwrap()["permission_epoch"]
                    .as_i64()
                    .unwrap();
                store
                    .calendar_observe_permission(&account, CAL, 1, epoch, Access::Reader, "UTC")
                    .unwrap();
            }
            "membership" => empty_catalog(&mut store, &account),
            "connection" => {
                store
                    .set_google_connection_state(
                        &account,
                        crate::live_profile::ConnectionState::Connected,
                    )
                    .unwrap();
            }
            _ => remove(&mut store, &account),
        }
        let failure = store.calendar_live_occurrence_finish(held).err().unwrap();
        assert_ne!(
            failure.code, "storage_error",
            "changed source must refuse as stale, {change}"
        );
        assert!(
            store.calendar_live_start(&account, "edit", 1).is_err(),
            "{change}"
        );
        let op = store.calendar_live_operation(&account, "edit").unwrap();
        assert_eq!(op.state, "ready");
        assert_eq!(op.lease.unwrap().attempt, 0);
        assert_eq!(op.intent["request"], request("edit"));
    }
}

#[test]
fn delete_ack_keeps_derived_provenance_and_sync_replaces_it_with_exact_observation() {
    let (dir, mut store, account) = seeded();
    let lease = dispatch(&mut store, &account, "delete");
    let ack = store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Deleted)
        .unwrap();
    assert_eq!(
        ack["outcome"]["source_origin"],
        "dispatched_occurrence_identity"
    );
    assert_eq!(ack["outcome"]["notification_delivery"], "unverified");
    assert_eq!(
        store.calendar_live_publish(&lease).unwrap()["outcome"]["cache_published"],
        true
    );
    let derived = json!({"id":"instance","status":"cancelled","recurringEventId":"series","originalStartTime":instance()["originalStartTime"]});
    assert_eq!(source(&store, &account, "instance"), derived);
    let observed = |store: &Store| -> Value {
        let raw:String=store.db.query_row("SELECT observed_json FROM calendar_event_state WHERE account=?1 AND calendar_id=?2 AND event_id='instance'",params![account,CAL],|r|r.get(0)).unwrap();
        serde_json::from_str(&raw).unwrap()
    };
    assert_eq!(
        observed(&store),
        json!({"kind":"acknowledged_occurrence_cancellation","request_id":"delete","source":derived})
    );
    publish(&mut store, &account, vec![master()], CalendarSyncMode::Full);
    assert_eq!(
        store
            .calendar_combined(&json!({"from":"2026-09-12","to":"2026-09-15"}))
            .unwrap()["total"],
        2
    );
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    assert_eq!(source(&store, &account, "instance"), derived);
    let mut fetched = derived.clone();
    fetched["updated"] = "2026-09-14T01:00:00Z".into();
    publish(
        &mut store,
        &account,
        vec![fetched.clone()],
        CalendarSyncMode::Delta,
    );
    assert_eq!(observed(&store), fetched);
    assert_eq!(source(&store, &account, "instance"), fetched);
    assert_eq!(
        store.calendar_live_status(&account, "delete").unwrap()["outcome"]["source_origin"],
        "dispatched_occurrence_identity",
        "later sync cannot rewrite the immutable response meaning"
    );
}

#[test]
fn sparse_read_reconciliation_stays_observed_and_exact_late_ack_strengthens_receipt() {
    let (_dir, mut store, account) = seeded();
    let lease = dispatch(&mut store, &account, "delete");
    store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Unknown)
        .unwrap();
    let fence = store.calendar_live_reconcile_begin(&lease, 1).unwrap();
    let active = proof(&store, &account, instance());
    assert_eq!(
        store
            .calendar_live_reconcile_occurrence_read(&lease, &fence, active)
            .unwrap()["state"],
        "unknown"
    );
    let sparse = json!({"id":"instance","status":"cancelled","recurringEventId":"series","originalStartTime":instance()["originalStartTime"]});
    assert!(store
        .calendar_live_reconcile_read(&lease, &fence, sparse.clone())
        .is_err());
    let held = proof(&store, &account, sparse.clone());
    let result = store
        .calendar_live_reconcile_occurrence_read(&lease, &fence, held)
        .unwrap();
    assert_eq!(result["state"], "reconciled");
    assert_eq!(
        result["outcome"],
        json!({"kind":"observed_state","matches_intent":true,"observed_etag":null,"event_cancelled":true,"notification_delivery":"unverified","source_origin":"fetched"})
    );
    assert_eq!(
        store.calendar_live_publish(&lease).unwrap()["outcome"]["cache_published"],
        true
    );
    assert_eq!(source(&store, &account, "instance"), sparse);
    let strengthened = store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Deleted)
        .unwrap();
    assert_eq!(strengthened["state"], "accepted");
    assert_eq!(
        strengthened["outcome"]["evidence_origin"],
        "provider_response"
    );
    assert_eq!(
        store.calendar_live_publish(&lease).unwrap()["outcome"]["cache_published"],
        false,
        "original dispatch authority cannot reverse the newer read publication"
    );
    assert_eq!(source(&store, &account, "instance"), sparse);
}

#[test]
fn held_reconciliation_and_late_provider_outcome_have_distinct_publication_fences() {
    for change in ["source", "membership", "removed"] {
        let (_dir, mut store, account) = seeded();
        let lease = dispatch(&mut store, &account, "edit");
        store
            .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Unknown)
            .unwrap();
        let fence = store.calendar_live_reconcile_begin(&lease, 1).unwrap();
        let mut actual = instance();
        actual["summary"] = "Authored occurrence".into();
        actual["etag"] = "instance-v2".into();
        let held = proof(&store, &account, actual.clone());
        let mut wanted = instance();
        match change {
            "source" => {
                wanted["summary"] = "Newer acquired text".into();
                wanted["etag"] = "instance-v3".into();
                publish(
                    &mut store,
                    &account,
                    vec![wanted.clone()],
                    CalendarSyncMode::Delta,
                );
            }
            "membership" => empty_catalog(&mut store, &account),
            _ => remove(&mut store, &account),
        }
        assert!(
            store
                .calendar_live_reconcile_occurrence_read(&lease, &fence, held)
                .is_err(),
            "{change}"
        );
        assert_eq!(
            store.calendar_live_status(&account, "edit").unwrap()["state"],
            "unknown"
        );
        let ack = store
            .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Accepted(actual))
            .unwrap();
        assert_eq!(
            ack["state"], "accepted",
            "exact late evidence must survive {change}"
        );
        let publication = store.calendar_live_publish(&lease);
        if change == "removed" {
            assert!(publication.is_err());
        } else {
            assert_eq!(publication.unwrap()["outcome"]["cache_published"], false);
        }
        assert_eq!(source(&store, &account, "instance"), wanted);
    }
}
