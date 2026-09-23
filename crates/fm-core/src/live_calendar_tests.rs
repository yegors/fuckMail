use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[path = "live_calendar_tombstone_tests.rs"]
mod tombstones;

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
    assert_eq!(store.db.query_row("SELECT COUNT(*) FROM calendar_sync_items WHERE run=?1 AND error_code='duplicate_identity' AND json IS NOT NULL",[lease.run],|r|r.get::<_,i64>(0)).unwrap(),1);
    assert_eq!(view(&store)["coverage"]["complete"], false);
    assert_eq!(view(&store)["total"], 1);
}

#[test]
fn published_runs_release_staging_json_and_replay_after_reopen() {
    let (dir, mut store, account) = setup();
    let lease = publish_events(
        &mut store,
        &account,
        vec![event("retained", "Source-owned content")],
        CalendarSyncMode::Full,
        "t1",
    );
    let staged: i64 = store
        .db
        .query_row(
            "SELECT count(*) FROM calendar_sync_items WHERE run=?1 AND json IS NOT NULL",
            [lease.run],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(staged, 0);
    let proofs: i64 = store
        .db
        .query_row(
            "SELECT count(*) FROM calendar_sync_items WHERE run=?1 AND length(fingerprint)>0",
            [lease.run],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(proofs, 1);
    assert_eq!(
        store.calendar_publish_sync(&lease).unwrap()["state"],
        "published"
    );
    drop(store);
    let store = Store::open_live(dir.path()).unwrap();
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

#[test]
fn prepared_page_digest_matches_existing_receipts_and_chunks_are_bound_to_the_profile() {
    let (_dir, mut store, account) = setup();
    let lease = store
        .calendar_sync_begin(&account, CAL, CalendarSyncMode::Full)
        .unwrap();
    let mut source = event("unicode", "Line\nquote\" and 東京");
    source["z-unknown"] = json!({"b":null,"a":[1,true,"\\"]});
    let original = page(vec![source], "token\"unicode東京");
    let old_manifest = fingerprint(&json!([
        original.items,
        original.next_page_token,
        original.next_sync_token
    ]))
    .unwrap();
    let cancel = fm_google::transport::CancellationToken::new();
    let prepared = prepare_calendar_page(lease.clone(), original, &cancel).unwrap();
    assert_eq!(prepared.manifest, old_manifest);
    let input = store.calendar_prepare_chunk(&lease, &prepared, 0).unwrap();
    let chunk = prepare_calendar_chunk(&prepared, input, &cancel).unwrap();
    let (_other_dir, mut other, other_account) = setup();
    let other_lease = other
        .calendar_sync_begin(&other_account, CAL, CalendarSyncMode::Full)
        .unwrap();
    assert_eq!(other_lease.run, lease.run);
    assert_eq!(
        other
            .calendar_stage_prepared_chunk(&other_lease, &prepared, &chunk)
            .unwrap_err()
            .code,
        "stale_preparation"
    );
    assert_eq!(
        store
            .calendar_stage_prepared_chunk(&lease, &prepared, &chunk)
            .unwrap(),
        1
    );
    store
        .calendar_seal_prepared_page(&lease, &prepared)
        .unwrap();
    store.calendar_publish_sync(&lease).unwrap();
    assert_eq!(view(&store)["total"], 1);
}

#[test]
fn asynchronous_preparation_rechecks_policy_and_keeps_sparse_cancellation_source() {
    let (_dir, mut store, account) = setup();
    let mut retained = event("cancelled", "Keep exact historical fields");
    retained["unknown"] = json!({"preserved":true});
    publish_events(
        &mut store,
        &account,
        vec![retained],
        CalendarSyncMode::Full,
        "t1",
    );
    let lease = store
        .calendar_sync_begin(&account, CAL, CalendarSyncMode::Delta)
        .unwrap();
    let cancel = fm_google::transport::CancellationToken::new();
    let prepared = prepare_calendar_page(
        lease.clone(),
        page(vec![json!({"id":"cancelled","status":"cancelled"})], "t2"),
        &cancel,
    )
    .unwrap();
    let input = store.calendar_prepare_chunk(&lease, &prepared, 0).unwrap();
    assert!(input.input_bytes() > prepared.serialized_bytes());
    let chunk = prepare_calendar_chunk(&prepared, input, &cancel).unwrap();
    let epoch = store.calendar_checkpoint(&account, CAL).unwrap()["permission_epoch"]
        .as_i64()
        .unwrap();
    store
        .calendar_observe_permission(
            &account,
            CAL,
            1,
            epoch,
            Access::Owner,
            "America/Los_Angeles",
        )
        .unwrap();
    assert_eq!(
        store
            .calendar_stage_prepared_chunk(&lease, &prepared, &chunk)
            .unwrap_err()
            .code,
        "stale_preparation"
    );
    let input = store.calendar_prepare_chunk(&lease, &prepared, 0).unwrap();
    let chunk = prepare_calendar_chunk(&prepared, input, &cancel).unwrap();
    store
        .calendar_stage_prepared_chunk(&lease, &prepared, &chunk)
        .unwrap();
    store
        .calendar_seal_prepared_page(&lease, &prepared)
        .unwrap();
    store.calendar_publish_sync(&lease).unwrap();
    let raw:String=store.db.query_row("SELECT json FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id='cancelled'",params![account,CAL],|r|r.get(0)).unwrap();
    let raw: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(raw["unknown"]["preserved"], true);
    assert_eq!(raw["status"], "cancelled");
    assert_eq!(raw["summary"], "Keep exact historical fields");
}

#[test]
fn prepared_chunks_enforce_input_bytes_and_cancel_without_receipts() {
    let (_dir, mut store, account) = setup();
    let lease = store
        .calendar_sync_begin(&account, CAL, CalendarSyncMode::Full)
        .unwrap();
    let cancel = fm_google::transport::CancellationToken::new();
    let sources = (0..40)
        .map(|i| {
            let mut row = event(&format!("item-{i}"), "Bounded");
            row["description"] = "x".repeat(40 * 1024).into();
            row
        })
        .collect();
    let prepared =
        prepare_calendar_page(lease.clone(), page(sources, "complete"), &cancel).unwrap();
    let input = store.calendar_prepare_chunk(&lease, &prepared, 0).unwrap();
    assert!(input.input_bytes() <= CHUNK_BYTES);
    assert!(input.item_count() < CALENDAR_STAGE_CHUNK);
    cancel.cancel();
    assert_eq!(
        prepare_calendar_chunk(&prepared, input, &cancel)
            .err()
            .unwrap()
            .code,
        "cancelled"
    );
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT COUNT(*) FROM calendar_sync_items WHERE run=?1",
                [lease.run],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        prepare_calendar_page(lease, page(vec![], "cancelled"), &cancel)
            .err()
            .unwrap()
            .code,
        "cancelled"
    );
}

#[test]
fn oversized_prior_and_merged_cancellations_preserve_source_and_checkpoint() {
    for (prior_mib, incoming_mib, refuses_before_copy) in [(33, 0, true), (20, 14, false)] {
        let (_dir, mut store, account) = setup();
        let previous = publish_events(
            &mut store,
            &account,
            vec![event("large-cancellation", "Retained historical event")],
            CalendarSyncMode::Full,
            "prior-token",
        );
        while store.calendar_finalize_chunk(&previous).unwrap() > 0 {}
        // Historical releases did not bound merged sparse sources. Seed that
        // valid retained shape directly, without admitting an oversized page.
        let mut historical = event("large-cancellation", "Retained historical event");
        historical["old_unknown_field"] = "x".repeat(prior_mib * 1024 * 1024).into();
        let historical = historical.to_string();
        store.db.execute("UPDATE calendar_sources SET json=?3 WHERE account=?1 AND calendar_id=?2 AND event_id='large-cancellation'",params![account,CAL,historical]).unwrap();
        let lease = store
            .calendar_sync_begin(&account, CAL, CalendarSyncMode::Delta)
            .unwrap();
        let mut cancellation = json!({"id":"large-cancellation","status":"cancelled"});
        cancellation["new_unknown_field"] = "y".repeat(incoming_mib * 1024 * 1024).into();
        let cancel = fm_google::transport::CancellationToken::new();
        let prepared = prepare_calendar_page(
            lease.clone(),
            page(vec![cancellation], "uncommitted-token"),
            &cancel,
        )
        .unwrap();
        let input = store.calendar_prepare_chunk(&lease, &prepared, 0);
        let error = if refuses_before_copy {
            input.err().expect("oversized prior source was copied")
        } else {
            let input = input.unwrap();
            assert_eq!(input.item_count(), 1);
            assert!(input.input_bytes() > CHUNK_BYTES);
            prepare_calendar_chunk(&prepared, input, &cancel)
                .err()
                .expect("oversized merged source was normalized")
        };
        assert_eq!(error.code, "calendar_preparation_limit");
        let retained: String = store.db.query_row("SELECT json FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id='large-cancellation'",params![account,CAL],|r|r.get(0)).unwrap();
        assert!(
            retained == historical,
            "retained source changed on resource refusal"
        );
        assert_eq!(
            store.calendar_checkpoint(&account, CAL).unwrap()["sync_token"],
            "prior-token"
        );
        assert_eq!(
            store
                .db
                .query_row(
                    "SELECT COUNT(*) FROM calendar_sync_items WHERE run=?1",
                    [lease.run],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        assert_eq!(
            store
                .db
                .query_row(
                    "SELECT COUNT(*) FROM calendar_sync_pages WHERE run=?1",
                    [lease.run],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
    }
}

#[test]
#[ignore = "Explicit large synthetic Calendar preparation/writer timing experiment"]
fn prepared_large_page_writer_timing() {
    use std::time::Instant;
    let (_dir, mut store, account) = setup();
    let cancel = fm_google::transport::CancellationToken::new();
    let mut page_prepare = Vec::new();
    let mut chunk_prepare = Vec::new();
    let mut writer = Vec::new();
    let mut input_reads = Vec::new();
    let mut seals = Vec::new();
    let mut publications = Vec::new();
    let mut consolidation = Vec::new();
    let mut max_bytes = 0;
    let mut max_items = 0;
    let mut source_bytes = 0;
    for round in 0..7 {
        let lease = store
            .calendar_sync_begin(
                &account,
                CAL,
                if round == 0 {
                    CalendarSyncMode::Full
                } else {
                    CalendarSyncMode::Delta
                },
            )
            .unwrap();
        let sources = (0..250)
            .map(|i| {
                let mut row = event(&format!("large-{i}"), "Synthetic timing event");
                row["description"] = format!("{round}{}", "x".repeat(64 * 1024)).into();
                row
            })
            .collect();
        let started = Instant::now();
        let prepared = prepare_calendar_page(
            lease.clone(),
            page(sources, &format!("round-{round}")),
            &cancel,
        )
        .unwrap();
        page_prepare.push(started.elapsed().as_secs_f64() * 1000.0);
        source_bytes = prepared.serialized_bytes();
        assert!(source_bytes <= preparation::CALENDAR_PAGE_BYTES);
        let mut next = 0;
        loop {
            let started = Instant::now();
            let input = store
                .calendar_prepare_chunk(&lease, &prepared, next)
                .unwrap();
            input_reads.push(started.elapsed().as_secs_f64() * 1000.0);
            max_bytes = max_bytes.max(input.input_bytes());
            max_items = max_items.max(input.item_count());
            assert!(input.input_bytes() <= CHUNK_BYTES);
            assert!(input.item_count() <= CALENDAR_STAGE_CHUNK);
            let started = Instant::now();
            let chunk = prepare_calendar_chunk(&prepared, input, &cancel).unwrap();
            chunk_prepare.push(started.elapsed().as_secs_f64() * 1000.0);
            let started = Instant::now();
            next = store
                .calendar_stage_prepared_chunk(&lease, &prepared, &chunk)
                .unwrap();
            writer.push(started.elapsed().as_secs_f64() * 1000.0);
            if next == prepared.len() {
                break;
            }
        }
        let started = Instant::now();
        store
            .calendar_seal_prepared_page(&lease, &prepared)
            .unwrap();
        seals.push(started.elapsed().as_secs_f64() * 1000.0);
        let started = Instant::now();
        store.calendar_publish_sync(&lease).unwrap();
        publications.push(started.elapsed().as_secs_f64() * 1000.0);
        loop {
            let started = Instant::now();
            let count = store.calendar_finalize_chunk(&lease).unwrap();
            consolidation.push(started.elapsed().as_secs_f64() * 1000.0);
            if count == 0 {
                break;
            }
        }
        assert_eq!(store.db.query_row("SELECT COUNT(*) FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2",params![account,CAL],|r|r.get::<_,i64>(0)).unwrap(),250);
        assert_eq!(store.db.query_row("SELECT COUNT(*) FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND substr(json_extract(json,'$.description'),1,1)=?3 AND length(json_extract(json,'$.description'))=65537",params![account,CAL,round.to_string()],|r|r.get::<_,i64>(0)).unwrap(),250);
    }
    fn metric(values: &[f64]) -> Value {
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        json!({"samples":sorted.len(),"p50_ms":sorted[sorted.len()/2],"p95_ms":sorted[((sorted.len()as f64*0.95).ceil()as usize-1).min(sorted.len()-1)],"max_ms":sorted.last().unwrap()})
    }
    assert!(writer.len() >= 100);
    println!(
        "CALENDAR_PREPARATION_TIMING {}",
        json!({"rounds":7,"events_per_page":250,"serialized_page_bytes":source_bytes,"maximum_chunk_input_bytes":max_bytes,"maximum_chunk_items":max_items,"page_preparation":metric(&page_prepare),"chunk_preparation":metric(&chunk_prepare),"writer_input_read":metric(&input_reads),"writer_stage_sql":metric(&writer),"writer_seal":metric(&seals),"writer_publication":metric(&publications),"writer_consolidation":metric(&consolidation),"native_ui_latency_verified":false})
    );
}
