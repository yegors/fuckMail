use super::*;
use std::collections::{BTreeMap, BTreeSet};

const CAL: &str = "human@example.test";
fn scopes() -> BTreeSet<String> {
    [
        "openid",
        "email",
        "https://www.googleapis.com/auth/calendar.events",
        "https://www.googleapis.com/auth/calendar.calendarlist.readonly",
        "https://www.googleapis.com/auth/calendar.events.freebusy",
    ]
    .map(String::from)
    .into()
}
fn setup() -> (tempfile::TempDir, Store, String) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open_live(dir.path()).unwrap();
    let account = store
        .bind_google_account(
            "synthetic-calendar-subject",
            "human@example.test",
            &scopes(),
            None,
        )
        .unwrap();
    store.preferences_save(&json!({"request_id":"zone","expected_revision":0,"preferences":{"calendar_display_time_zone":"Europe/London"}})).unwrap();
    publish_catalog(
        &mut store,
        &account,
        vec![catalog(CAL, "owner")],
        CalendarSyncMode::Full,
    );
    (dir, store, account)
}
fn catalog(id: &str, role: &str) -> Value {
    json!({"id":id,"summary":"Real catalog name","summaryOverride":"Chosen name","timeZone":"America/Los_Angeles","accessRole":role,"hidden":true,"selected":false,"primary":true,"backgroundColor":"#112233"})
}
fn event(id: &str, title: &str) -> Value {
    json!({"id":id,"etag":"v1","summary":title,"start":{"dateTime":"2026-09-12T09:00:00Z"},"end":{"dateTime":"2026-09-12T10:00:00Z"}})
}
fn page(items: Vec<Value>, token: &str) -> CalendarPage {
    CalendarPage {
        input_cursor: None,
        items,
        next_page_token: None,
        next_sync_token: Some(token.into()),
    }
}
fn stage(store: &mut Store, lease: &CalendarSyncLease, page: &CalendarPage) {
    let mut next = store.calendar_stage_page_chunk(lease, page, 0).unwrap();
    while next < page.items.len() {
        let last = next;
        next = store.calendar_stage_page_chunk(lease, page, next).unwrap();
        assert!(next > last);
    }
    store.calendar_seal_page(lease, page).unwrap();
}
fn publish_catalog(store: &mut Store, account: &str, items: Vec<Value>, mode: CalendarSyncMode) {
    let lease = store.calendar_catalog_begin(account, mode).unwrap();
    stage(store, &lease, &page(items, "catalog-token"));
    while store.calendar_catalog_finalize_chunk(&lease).unwrap() > 0 {}
    store.calendar_publish_sync(&lease).unwrap();
}
fn publish_events(
    store: &mut Store,
    account: &str,
    items: Vec<Value>,
    mode: CalendarSyncMode,
    token: &str,
) -> CalendarSyncLease {
    let lease = store.calendar_sync_begin(account, CAL, mode).unwrap();
    stage(store, &lease, &page(items, token));
    store.calendar_publish_sync(&lease).unwrap();
    lease
}
fn view(store: &Store) -> Value {
    store
        .calendar_combined(&json!({"from":"2026-09-12","to":"2026-09-13"}))
        .unwrap()
}
fn stamp(store: &Store) -> i64 {
    store
        .db
        .query_row("SELECT revision FROM calendar_publication", [], |r| {
            r.get(0)
        })
        .unwrap()
}

#[test]
fn provider_identity_and_cursor_bounds_are_separate() {
    assert!(provider_id(&"c".repeat(1024)).is_ok());
    assert!(provider_id(&"c".repeat(1025)).is_err());
    assert!(provider_id("private\ncalendar").is_err());
    assert!(token(&"t".repeat(8192)).is_ok());
    assert!(token(&"t".repeat(8193)).is_err());
}

#[test]
fn limited_writer_resolves_parent_privacy_and_fails_closed_for_orphans() {
    let (_dir, mut store, account) = setup();
    let mut master = event("series", "PRIVATE MASTER");
    master["visibility"] = "private".into();
    master["recurrence"] = json!(["RRULE:FREQ=DAILY;COUNT=3"]);
    let mut exception = event("exception", "PRIVATE CHILD");
    exception["recurringEventId"] = "series".into();
    exception["originalStartTime"] = json!({"dateTime":"2026-09-12T09:00:00Z"});
    let mut orphan = event("orphan", "UNRESOLVED CHILD");
    orphan["recurringEventId"] = "missing-master".into();
    orphan["originalStartTime"] = json!({"dateTime":"2026-09-12T09:00:00Z"});
    publish_events(
        &mut store,
        &account,
        vec![master, exception, orphan],
        CalendarSyncMode::Full,
        "t1",
    );
    store
        .calendar_observe_permission(
            &account,
            CAL,
            1,
            store.calendar_checkpoint(&account, CAL).unwrap()["permission_epoch"]
                .as_i64()
                .unwrap(),
            Access::WriterWithoutPrivateAccess,
            "America/Los_Angeles",
        )
        .unwrap();
    let v = view(&store);
    assert_eq!(v["total"], 2);
    assert!(!v.to_string().contains("PRIVATE"));
    assert!(!v.to_string().contains("UNRESOLVED"));
    for row in v["events"].as_array().unwrap() {
        assert_eq!(row["busy_only"], true);
        assert_eq!(row["can_edit"], false);
        assert_eq!(row["can_rsvp"], false);
        assert!(row.get("etag").is_none());
    }
    let (child, access) = store
        .stored_calendar_event(&account, CAL, "exception")
        .unwrap();
    assert_eq!(access.for_event(&child), Access::FreeBusy);
    assert!(fm_calendar::edit(&child, access, "v1", json!({"summary":"forbidden"})).is_err());
    store
        .calendar_observe_permission(
            &account,
            CAL,
            1,
            store.calendar_checkpoint(&account, CAL).unwrap()["permission_epoch"]
                .as_i64()
                .unwrap(),
            Access::Writer,
            "America/Los_Angeles",
        )
        .unwrap();
    assert!(view(&store).to_string().contains("PRIVATE CHILD"));
}

#[test]
fn duplicate_provider_identity_is_retained_as_explicit_incomplete_data() {
    let (_dir, mut store, account) = setup();
    let lease = publish_events(
        &mut store,
        &account,
        vec![event("same", "First"), event("same", "Ambiguous")],
        CalendarSyncMode::Full,
        "t1",
    );
    assert_eq!(store.db.query_row("SELECT COUNT(*) FROM calendar_sync_items WHERE run=?1 AND error_code='duplicate_identity'",[lease.run],|r|r.get::<_,i64>(0)).unwrap(),1);
    assert_eq!(view(&store)["coverage"]["complete"], false);
    assert_eq!(view(&store)["total"], 1);
}

#[test]
fn old_permission_response_cannot_restore_grants_after_a_newer_observation() {
    let (_dir, mut store, account) = setup();
    let epoch = store.calendar_checkpoint(&account, CAL).unwrap()["permission_epoch"]
        .as_i64()
        .unwrap();
    store
        .calendar_observe_permission(
            &account,
            CAL,
            1,
            epoch,
            Access::FreeBusy,
            "America/Los_Angeles",
        )
        .unwrap();
    assert_eq!(
        store
            .calendar_observe_permission(
                &account,
                CAL,
                1,
                epoch,
                Access::Owner,
                "America/Los_Angeles"
            )
            .unwrap_err()
            .code,
        "stale_revision"
    );
    assert_eq!(
        store.calendar_checkpoint(&account, CAL).unwrap()["access"],
        "free_busy"
    );
}

#[test]
fn exception_budget_reports_incomplete_without_publishing_an_unresolved_master() {
    let (_dir, mut store, account) = setup();
    let mut master = event("series", "Must not project without exceptions");
    master["recurrence"] = json!(["RRULE:FREQ=DAILY;COUNT=3"]);
    let lease = publish_events(
        &mut store,
        &account,
        vec![master],
        CalendarSyncMode::Full,
        "t1",
    );
    let tx = store.db.unchecked_transaction().unwrap();
    for n in 0..1001 {
        let child = json!({"id":format!("child-{n}"),"recurringEventId":"series","originalStartTime":{"dateTime":"2010-01-01T09:00:00Z"},"summary":"Old exception","start":{"dateTime":"2010-01-01T09:00:00Z"},"end":{"dateTime":"2010-01-01T10:00:00Z"}});
        tx.execute("INSERT INTO calendar_sources(account,calendar_id,event_id,json,source_generation) VALUES(?1,?2,?3,?4,?5)",params![account,CAL,child["id"].as_str().unwrap(),child.to_string(),lease.run]).unwrap();
        tx.execute("INSERT INTO calendar_event_state(account,calendar_id,event_id,observed_json,seen_full_generation) VALUES(?1,?2,?3,?4,?5)",params![account,CAL,child["id"].as_str().unwrap(),child.to_string(),lease.run]).unwrap();
    }
    tx.commit().unwrap();
    let v = view(&store);
    assert_eq!(v["total"], 0);
    assert_eq!(v["coverage"]["complete"], false);
    assert_eq!(v["coverage"]["projection_limited"], true);
}

#[test]
fn durable_chunks_resume_then_publish_without_early_cursor_or_projection_change() {
    let (dir, mut store, account) = setup();
    publish_events(
        &mut store,
        &account,
        vec![event("old", "Old committed")],
        CalendarSyncMode::Full,
        "t1",
    );
    let before = stamp(&store);
    let lease = store
        .calendar_sync_begin(&account, CAL, CalendarSyncMode::Delta)
        .unwrap();
    let mut items = (0..40)
        .map(|n| event(&format!("event-{n}"), "New staged"))
        .collect::<Vec<_>>();
    items.push(event("old", "Changed"));
    let p = page(items, "t2");
    assert_eq!(store.calendar_stage_page_chunk(&lease, &p, 0).unwrap(), 32);
    assert_eq!(
        store.calendar_seal_page(&lease, &p).unwrap_err().code,
        "page_incomplete"
    );
    assert_eq!(
        store.calendar_checkpoint(&account, CAL).unwrap()["sync_token"],
        "t1"
    );
    assert_eq!(stamp(&store), before);
    assert_eq!(view(&store)["total"], 1);
    drop(store);
    let mut store = Store::open(dir.path()).unwrap();
    let resumed = store.calendar_sync_resume(&account, CAL).unwrap().unwrap();
    assert_eq!(resumed.run, lease.run);
    assert_eq!(
        store.calendar_stage_page_chunk(&resumed, &p, 0).unwrap(),
        32
    );
    assert_eq!(
        store.calendar_stage_page_chunk(&resumed, &p, 32).unwrap(),
        41
    );
    assert_eq!(view(&store)["events"][0]["title"], "Old committed");
    store.calendar_seal_page(&resumed, &p).unwrap();
    assert_eq!(
        store.calendar_checkpoint(&account, CAL).unwrap()["sync_token"],
        "t1"
    );
    let receipt = store.calendar_publish_sync(&resumed).unwrap();
    assert_eq!(store.calendar_publish_sync(&resumed).unwrap(), receipt);
    assert_eq!(view(&store)["total"], 41);
    assert_eq!(view(&store)["coverage"]["complete"], true);
    let published = stamp(&store);
    assert!(published > before);
    assert_eq!(store.calendar_finalize_chunk(&resumed).unwrap(), 1);
    assert_eq!(store.calendar_finalize_chunk(&resumed).unwrap(), 0);
    assert_eq!(stamp(&store), published);
    assert_eq!(view(&store)["total"], 41);
}

#[test]
fn expiry_discards_partial_sources_preserves_previous_view_and_requires_cleanup() {
    let (_dir, mut store, account) = setup();
    publish_events(
        &mut store,
        &account,
        vec![event("old", "Old")],
        CalendarSyncMode::Full,
        "t1",
    );
    let lease = store
        .calendar_sync_begin(&account, CAL, CalendarSyncMode::Full)
        .unwrap();
    stage(
        &mut store,
        &lease,
        &page(vec![event("new", "New"), event("old", "Changed")], "unused"),
    );
    store.calendar_expire_sync(&lease).unwrap();
    assert_eq!(
        store
            .calendar_sync_begin(&account, CAL, CalendarSyncMode::Full)
            .err()
            .unwrap()
            .code,
        "calendar_finalize_pending"
    );
    assert_eq!(view(&store)["events"][0]["title"], "Old");
    let before = stamp(&store);
    assert_eq!(store.calendar_finalize_chunk(&lease).unwrap(), 2);
    assert_eq!(stamp(&store), before);
    assert_eq!(
        store.calendar_checkpoint(&account, CAL).unwrap()["sync_token"],
        "t1"
    );
    publish_events(
        &mut store,
        &account,
        vec![event("replacement", "Replacement")],
        CalendarSyncMode::Full,
        "t3",
    );
    assert_eq!(view(&store)["total"], 1);
    assert_eq!(view(&store)["events"][0]["title"], "Replacement");
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT COUNT(*) FROM calendar_sources WHERE event_id='old'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
}

#[test]
fn permission_and_connection_epochs_fence_old_pages_and_stale_catalog_grants() {
    let (_dir, mut store, account) = setup();
    publish_events(
        &mut store,
        &account,
        vec![event("private", "Secret")],
        CalendarSyncMode::Full,
        "t1",
    );
    let run = store
        .calendar_sync_begin(&account, CAL, CalendarSyncMode::Delta)
        .unwrap();
    let catalog_run = store
        .calendar_catalog_begin(&account, CalendarSyncMode::Delta)
        .unwrap();
    stage(
        &mut store,
        &catalog_run,
        &page(vec![catalog(CAL, "owner")], "c2"),
    );
    store
        .calendar_observe_permission(
            &account,
            CAL,
            run.connection_generation,
            store.calendar_checkpoint(&account, CAL).unwrap()["permission_epoch"]
                .as_i64()
                .unwrap(),
            Access::FreeBusy,
            "America/Los_Angeles",
        )
        .unwrap();
    assert!(!view(&store).to_string().contains("Secret"));
    assert_eq!(
        store
            .calendar_stage_page_chunk(&run, &page(vec![], "t2"), 0)
            .unwrap_err()
            .code,
        "stale_revision"
    );
    while store.calendar_catalog_finalize_chunk(&catalog_run).unwrap() > 0 {}
    store.calendar_publish_sync(&catalog_run).unwrap();
    assert_eq!(
        store.calendar_checkpoint(&account, CAL).unwrap()["access"],
        "free_busy"
    );
    store.calendar_expire_sync(&run).unwrap();
    store
        .calendar_observe_permission(
            &account,
            CAL,
            run.connection_generation,
            store.calendar_checkpoint(&account, CAL).unwrap()["permission_epoch"]
                .as_i64()
                .unwrap(),
            Access::Owner,
            "America/Los_Angeles",
        )
        .unwrap();
    assert!(store.calendar_checkpoint(&account, CAL).unwrap()["sync_token"].is_null());
    let old = store
        .calendar_sync_begin(&account, CAL, CalendarSyncMode::Full)
        .unwrap();
    store
        .bind_google_account(
            "synthetic-calendar-subject",
            "human@example.test",
            &scopes(),
            Some(&account),
        )
        .unwrap();
    assert_eq!(
        store
            .calendar_stage_page_chunk(&old, &page(vec![], "t3"), 0)
            .unwrap_err()
            .code,
        "stale_revision"
    );
}

#[test]
fn unchanged_permission_refresh_preserves_staging_and_reconnect_rebind_fences_old_response() {
    let (_dir, mut store, account) = setup();
    let lease = store
        .calendar_sync_begin(&account, CAL, CalendarSyncMode::Full)
        .unwrap();
    let first = CalendarPage {
        input_cursor: None,
        items: vec![event("first", "First")],
        next_page_token: Some("second-page".into()),
        next_sync_token: None,
    };
    stage(&mut store, &lease, &first);
    let epoch = store.calendar_checkpoint(&account, CAL).unwrap()["permission_epoch"]
        .as_i64()
        .unwrap();
    let newer = store
        .calendar_observe_permission(
            &account,
            CAL,
            1,
            epoch,
            Access::Owner,
            "America/Los_Angeles",
        )
        .unwrap();
    assert_eq!(newer, epoch + 1);
    assert_eq!(
        store
            .calendar_observe_permission(
                &account,
                CAL,
                1,
                epoch,
                Access::Reader,
                "America/Los_Angeles"
            )
            .unwrap_err()
            .code,
        "stale_revision"
    );
    store
        .bind_google_account(
            "synthetic-calendar-subject",
            "human@example.test",
            &scopes(),
            Some(&account),
        )
        .unwrap();
    let current = store.calendar_sync_rebind(&lease).unwrap();
    assert_eq!(current.run, lease.run);
    assert_eq!(current.connection_generation, 2);
    let last = CalendarPage {
        input_cursor: Some("second-page".into()),
        items: vec![event("last", "Last")],
        next_page_token: None,
        next_sync_token: Some("final-token".into()),
    };
    assert_eq!(
        store
            .calendar_stage_page_chunk(&lease, &last, 0)
            .unwrap_err()
            .code,
        "stale_revision"
    );
    stage(&mut store, &current, &last);
    store.calendar_publish_sync(&current).unwrap();
    assert_eq!(view(&store)["total"], 2);
}

#[test]
fn permission_upgrade_requires_fresh_inventory_and_preserves_retained_cache() {
    let (_dir, mut store, account) = setup();
    publish_events(
        &mut store,
        &account,
        vec![event("retained", "Retained")],
        CalendarSyncMode::Full,
        "first-token",
    );
    let initial = store.calendar_checkpoint(&account, CAL).unwrap();
    store
        .calendar_observe_permission(
            &account,
            CAL,
            1,
            initial["permission_epoch"].as_i64().unwrap(),
            Access::Reader,
            "America/Los_Angeles",
        )
        .unwrap();
    let reduced = store.calendar_checkpoint(&account, CAL).unwrap();
    assert!(reduced["sync_token"].is_null());
    assert_eq!(view(&store)["total"], 1);
    assert_eq!(view(&store)["coverage"]["complete"], false);
    assert_eq!(
        store
            .calendar_sync_begin(&account, CAL, CalendarSyncMode::Delta)
            .err()
            .unwrap()
            .code,
        "invalid_request"
    );
    store
        .calendar_observe_permission(
            &account,
            CAL,
            1,
            reduced["permission_epoch"].as_i64().unwrap(),
            Access::Owner,
            "America/Los_Angeles",
        )
        .unwrap();
    publish_events(
        &mut store,
        &account,
        vec![event("retained", "Updated complete source")],
        CalendarSyncMode::Full,
        "fresh-token",
    );
    assert_eq!(
        store.calendar_checkpoint(&account, CAL).unwrap()["retained_errors"],
        0
    );
    assert!(view(&store).to_string().contains("Updated complete source"));
}

#[test]
fn restart_discovers_published_and_expired_consolidation_without_losing_committed_rows() {
    let (dir, mut store, account) = setup();
    publish_events(
        &mut store,
        &account,
        vec![event("retained", "Before")],
        CalendarSyncMode::Full,
        "t1",
    );
    let published = publish_events(
        &mut store,
        &account,
        vec![event("retained", "After")],
        CalendarSyncMode::Delta,
        "t2",
    );
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    let cleanup = store.calendar_sync_cleanup(&account, CAL).unwrap().unwrap();
    assert_eq!(cleanup.run, published.run);
    assert_eq!(store.calendar_finalize_chunk(&cleanup).unwrap(), 1);
    assert!(store
        .calendar_sync_cleanup(&account, CAL)
        .unwrap()
        .is_none());
    let expired = store
        .calendar_sync_begin(&account, CAL, CalendarSyncMode::Delta)
        .unwrap();
    stage(
        &mut store,
        &expired,
        &page(
            vec![
                event("retained", "Never published"),
                event("new", "Never visible"),
            ],
            "t3",
        ),
    );
    store.calendar_expire_sync(&expired).unwrap();
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    let cleanup = store.calendar_sync_cleanup(&account, CAL).unwrap().unwrap();
    assert_eq!(cleanup.run, expired.run);
    assert_eq!(store.calendar_finalize_chunk(&cleanup).unwrap(), 2);
    assert!(store
        .calendar_sync_cleanup(&account, CAL)
        .unwrap()
        .is_none());
    let visible = view(&store).to_string();
    assert!(visible.contains("After"));
    assert!(!visible.contains("Never"));
}

#[test]
fn catalog_only_permission_refresh_does_not_require_ungranted_event_details() {
    let (_dir, mut store, account) = setup();
    let mut granted = scopes();
    granted.remove("https://www.googleapis.com/auth/calendar.events");
    store
        .bind_google_account(
            "synthetic-calendar-subject",
            "human@example.test",
            &granted,
            Some(&account),
        )
        .unwrap();
    let epoch = store.calendar_checkpoint(&account, CAL).unwrap()["permission_epoch"]
        .as_i64()
        .unwrap();
    store
        .calendar_observe_catalog_entry(&account, &catalog(CAL, "freeBusyReader"), 2, epoch)
        .unwrap();
    assert_eq!(
        store.calendar_sync_scope(&account, CAL).unwrap()["access"],
        "free_busy"
    );
    assert_eq!(
        store
            .calendar_sync_begin(&account, CAL, CalendarSyncMode::Full)
            .err()
            .unwrap()
            .code,
        "permission_denied"
    );
}

#[test]
fn catalog_restrictions_apply_during_staging_and_full_absence_hides_retained_details() {
    let (_dir, mut store, account) = setup();
    publish_events(
        &mut store,
        &account,
        vec![event("old", "Private detail")],
        CalendarSyncMode::Full,
        "t1",
    );
    let c = store
        .calendar_catalog_begin(&account, CalendarSyncMode::Delta)
        .unwrap();
    let p = page(vec![catalog(CAL, "freeBusyReader")], "c2");
    store.calendar_stage_page_chunk(&c, &p, 0).unwrap();
    assert_eq!(
        store.calendar_checkpoint(&account, CAL).unwrap()["access"],
        "free_busy"
    );
    assert!(!view(&store).to_string().contains("Private detail"));
    store.calendar_expire_sync(&c).unwrap();
    publish_catalog(&mut store, &account, vec![], CalendarSyncMode::Full);
    assert_eq!(
        store.calendar_sources().unwrap()["calendars"][0]["permission"],
        "none"
    );
    assert!(store
        .calendar_sync_begin(&account, CAL, CalendarSyncMode::Full)
        .is_err());
    assert_eq!(
        store
            .db
            .query_row("SELECT COUNT(*) FROM calendar_sources", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn malformed_raw_items_remain_durable_and_prevent_false_complete_inventory() {
    let (_dir, mut store, account) = setup();
    publish_events(
        &mut store,
        &account,
        vec![event("old", "Keep old")],
        CalendarSyncMode::Full,
        "t1",
    );
    let lease = publish_events(
        &mut store,
        &account,
        vec![
            json!({"id":"bad","summary":"Raw unsupported time"}),
            json!({"summary":"Unknown identity"}),
        ],
        CalendarSyncMode::Full,
        "t2",
    );
    let v = view(&store);
    assert_eq!(v["total"], 1);
    assert_eq!(v["coverage"]["complete"], false);
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT COUNT(*) FROM calendar_sync_items WHERE run=?1 AND error_code IS NOT NULL",
                [lease.run],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        2
    );
    publish_events(&mut store, &account, vec![], CalendarSyncMode::Delta, "t3");
    assert_eq!(view(&store)["coverage"]["complete"], false);
    publish_events(
        &mut store,
        &account,
        vec![event("new", "Recovered")],
        CalendarSyncMode::Full,
        "t4",
    );
    assert_eq!(view(&store)["coverage"]["complete"], true);
    assert_eq!(view(&store)["total"], 1);
}

#[test]
fn replay_manifest_and_cursor_cycles_cannot_advance_checkpoint() {
    let (_dir, mut store, account) = setup();
    let lease = store
        .calendar_sync_begin(&account, CAL, CalendarSyncMode::Full)
        .unwrap();
    let p = CalendarPage {
        input_cursor: None,
        items: vec![event("e", "original")],
        next_page_token: Some("next".into()),
        next_sync_token: None,
    };
    stage(&mut store, &lease, &p);
    let changed = CalendarPage {
        items: vec![event("e", "changed")],
        ..p
    };
    assert_eq!(
        store
            .calendar_stage_page_chunk(&lease, &changed, 0)
            .unwrap_err()
            .code,
        "stale_page"
    );
    let cycle = CalendarPage {
        input_cursor: Some("next".into()),
        items: vec![],
        next_page_token: Some("next".into()),
        next_sync_token: None,
    };
    store.calendar_stage_page_chunk(&lease, &cycle, 0).unwrap();
    assert_eq!(
        store.calendar_seal_page(&lease, &cycle).unwrap_err().code,
        "stale_page"
    );
    assert!(store.calendar_checkpoint(&account, CAL).unwrap()["sync_token"].is_null());
}

#[test]
fn free_busy_has_distinct_display_identity_no_event_or_edit_identity_and_explicit_errors() {
    use fm_calendar::google_api::{BusyError, BusyInterval};
    let (_dir, mut store, account) = setup();
    let generation = store.calendar_connection(&account, "busy").unwrap();
    let epoch = store
        .calendar_observe_permission(
            &account,
            CAL,
            generation,
            store.calendar_checkpoint(&account, CAL).unwrap()["permission_epoch"]
                .as_i64()
                .unwrap(),
            Access::FreeBusy,
            "America/Los_Angeles",
        )
        .unwrap();
    let dt = |s: &str| {
        chrono::DateTime::parse_from_rfc3339(s)
            .unwrap()
            .with_timezone(&chrono::Utc)
    };
    let mut snapshot = BusySnapshot {
        account: fm_google::AccountId::new(&account).unwrap(),
        from: dt("2026-09-11T23:00:00Z"),
        to: dt("2026-09-12T23:00:00Z"),
        calendars: BTreeMap::from([(
            CAL.into(),
            CalendarBusy::Available(vec![BusyInterval {
                start: dt("2026-09-12T09:00:00Z"),
                end: dt("2026-09-12T10:00:00Z"),
            }]),
        )]),
    };
    store
        .calendar_busy_commit(
            &snapshot,
            1,
            generation,
            &BTreeMap::from([(CAL.into(), epoch)]),
        )
        .unwrap();
    let v = view(&store);
    assert_eq!(v["time_zone"], "Europe/London");
    assert_eq!(v["events"][0]["kind"], "free_busy");
    assert!(v["events"][0]["id"].as_str().unwrap().starts_with("busy:"));
    for key in ["event_id", "occurrence_key", "etag"] {
        assert!(v["events"][0].get(key).is_none());
    }
    assert_eq!(v["events"][0]["can_edit"], false);
    snapshot.calendars.insert(
        CAL.into(),
        CalendarBusy::Unavailable(vec![BusyError::PermissionDenied]),
    );
    store
        .calendar_busy_commit(
            &snapshot,
            2,
            generation,
            &BTreeMap::from([(CAL.into(), epoch)]),
        )
        .unwrap();
    assert_eq!(view(&store)["total"], 0);
    assert_eq!(view(&store)["coverage"]["complete"], false);
    store
        .calendar_observe_permission(
            &account,
            CAL,
            generation,
            store.calendar_checkpoint(&account, CAL).unwrap()["permission_epoch"]
                .as_i64()
                .unwrap(),
            Access::None,
            "America/Los_Angeles",
        )
        .unwrap();
    assert!(store
        .calendar_busy_commit(
            &snapshot,
            3,
            generation,
            &BTreeMap::from([(CAL.into(), epoch)])
        )
        .is_err());
}

#[test]
fn large_retained_history_uses_window_candidates_and_live_zone_is_explicit() {
    let (_dir, mut store, account) = setup();
    publish_events(
        &mut store,
        &account,
        vec![event("visible", "In window")],
        CalendarSyncMode::Full,
        "t1",
    );
    let (generation,full):(i64,i64)=store.db.query_row("SELECT published_generation,full_generation FROM calendar_sync_state WHERE account=?1 AND calendar_id=?2",params![account,CAL],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    let tx = store.db.unchecked_transaction().unwrap();
    for n in 0..10_001 {
        let source = json!({"id":format!("history-{n}"),"summary":"History","start":{"dateTime":"2010-01-01T09:00:00Z"},"end":{"dateTime":"2010-01-01T10:00:00Z"}});
        tx.execute("INSERT INTO calendar_sources(account,calendar_id,event_id,json,source_generation) VALUES(?1,?2,?3,?4,?5)",params![account,CAL,source["id"].as_str().unwrap(),source.to_string(),generation]).unwrap();
        tx.execute("INSERT INTO calendar_event_state(account,calendar_id,event_id,observed_json,seen_full_generation) VALUES(?1,?2,?3,?4,?5)",params![account,CAL,source["id"].as_str().unwrap(),source.to_string(),full]).unwrap();
    }
    tx.commit().unwrap();
    let v = view(&store);
    assert_eq!(v["total"], 1);
    assert_eq!(v["coverage"]["candidates"], 1);
    assert_eq!(v["coverage"]["complete"], true);
    let catalog = store.calendar_sources().unwrap();
    assert_eq!(catalog["calendars"][0]["name"], "Chosen name");
    assert_eq!(catalog["calendars"][0]["hidden"], true);
    let p = store.preferences().unwrap();
    assert!(store.preferences_save(&json!({"request_id":"invalid-zone","expected_revision":p["revision"],"preferences":{"calendar_display_time_zone":"Toronto"}})).is_err());
    store.preferences_save(&json!({"request_id":"clear-zone","expected_revision":p["revision"],"preferences":{"calendar_display_time_zone":null}})).unwrap();
    assert_eq!(view(&store)["coverage"]["state"], "display_zone_required");
}
