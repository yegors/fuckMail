use super::*;

fn master() -> Value {
    let mut source = event("series", "Retained master");
    source["recurrence"] = json!(["RRULE:FREQ=DAILY;COUNT=3"]);
    source
}
fn cancelled(id: &str, original: Value) -> Value {
    json!({"id":id,"status":"cancelled","recurringEventId":"series","originalStartTime":original})
}
fn range(store: &Store, from: &str, to: &str) -> Value {
    store
        .calendar_combined(&json!({"from":from,"to":to}))
        .unwrap()
}
fn three_days(store: &Store) -> Value {
    range(store, "2026-09-12", "2026-09-15")
}
fn finalize(store: &mut Store, lease: &CalendarSyncLease) {
    while store.calendar_finalize_chunk(lease).unwrap() > 0 {}
}
fn source_stamp(store: &Store, account: &str, id: &str) -> (String, i64, String, i64) {
    store.db.query_row("SELECT s.json,s.source_generation,e.observed_json,e.seen_full_generation FROM calendar_sources s JOIN calendar_event_state e USING(account,calendar_id,event_id) WHERE s.account=?1 AND s.calendar_id=?2 AND s.event_id=?3",params![account,CAL,id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap()
}

#[test]
fn minimal_timed_dst_and_date_cancellations_survive_full_refresh_and_reopen() {
    for mode in ["timed", "dst", "date"] {
        let (dir, mut store, account) = setup();
        let mut source = master();
        let (original, from, to) = match mode {
            "dst" => {
                source["start"] = json!({"dateTime":"2026-10-31T09:00:00-07:00","timeZone":"America/Los_Angeles"});
                source["end"] = json!({"dateTime":"2026-10-31T10:00:00-07:00","timeZone":"America/Los_Angeles"});
                (
                    json!({"dateTime":"2026-11-01T09:00:00-08:00","timeZone":"America/Los_Angeles"}),
                    "2026-10-31",
                    "2026-11-03",
                )
            }
            "date" => {
                source["start"] = json!({"date":"2026-09-12"});
                source["end"] = json!({"date":"2026-09-13"});
                (json!({"date":"2026-09-13"}), "2026-09-12", "2026-09-15")
            }
            _ => (
                json!({"dateTime":"2026-09-13T09:00:00Z"}),
                "2026-09-12",
                "2026-09-15",
            ),
        };
        let tombstone = cancelled("cancelled", original);
        let lease = publish_events(
            &mut store,
            &account,
            vec![source.clone(), tombstone.clone()],
            CalendarSyncMode::Full,
            "first",
        );
        finalize(&mut store, &lease);
        let before = source_stamp(&store, &account, "cancelled");
        assert_eq!(serde_json::from_str::<Value>(&before.2).unwrap(), tombstone);
        assert_eq!(range(&store, from, to)["total"], 2, "{mode}");
        let lease = publish_events(
            &mut store,
            &account,
            vec![source],
            CalendarSyncMode::Full,
            "replacement",
        );
        assert_eq!(range(&store, from, to)["total"], 2, "published {mode}");
        finalize(&mut store, &lease);
        assert_eq!(source_stamp(&store, &account, "cancelled"), before);
        let visible: bool = store.db.query_row("SELECT EXISTS(SELECT 1 FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id='cancelled')",params![account,CAL],|r|r.get(0)).unwrap();
        assert!(!visible, "retention must not relax the current source view");
        assert_eq!(
            store
                .stored_calendar_event(&account, CAL, "cancelled")
                .unwrap_err()
                .code,
            "not_found"
        );
        drop(store);
        let store = Store::open_live(dir.path()).unwrap();
        let result = range(&store, from, to);
        assert_eq!(result["total"], 2, "reopen {mode}");
        assert_eq!(result["coverage"]["complete"], true);
        assert_eq!(source_stamp(&store, &account, "cancelled"), before);
    }
}

#[test]
fn moved_cancellation_and_hidden_ack_identity_are_superseded_only_after_publication() {
    let (dir, mut store, account) = setup();
    let original = json!({"dateTime":"2026-09-13T09:00:00Z"});
    let mut moved = event("instance", "Moved outside the selected days");
    moved["recurringEventId"] = "series".into();
    moved["originalStartTime"] = original.clone();
    moved["start"] = json!({"dateTime":"2026-09-20T09:00:00Z"});
    moved["end"] = json!({"dateTime":"2026-09-20T10:00:00Z"});
    let lease = publish_events(
        &mut store,
        &account,
        vec![master(), moved.clone()],
        CalendarSyncMode::Full,
        "initial",
    );
    finalize(&mut store, &lease);
    let lease = publish_events(
        &mut store,
        &account,
        vec![cancelled("instance", original)],
        CalendarSyncMode::Delta,
        "cancel",
    );
    finalize(&mut store, &lease);
    // Match the hidden source state used by acknowledged deletion; identity is
    // sufficient for suppression but does not make it an ordinary visible row.
    store.db.execute("UPDATE calendar_event_state SET visible=0 WHERE account=?1 AND calendar_id=?2 AND event_id='instance'",params![account,CAL]).unwrap();
    assert_eq!(three_days(&store)["total"], 2);
    assert_eq!(range(&store, "2026-09-20", "2026-09-21")["total"], 0);
    moved["start"] = json!({"dateTime":"2026-09-13T11:00:00Z"});
    moved["end"] = json!({"dateTime":"2026-09-13T12:00:00Z"});
    moved["etag"] = "new-active".into();
    let staged = store
        .calendar_sync_begin(&account, CAL, CalendarSyncMode::Delta)
        .unwrap();
    stage(
        &mut store,
        &staged,
        &page(vec![moved.clone()], "unpublished"),
    );
    assert_eq!(three_days(&store)["total"], 2);
    store.calendar_expire_sync(&staged).unwrap();
    finalize(&mut store, &staged);
    assert_eq!(three_days(&store)["total"], 2);
    let staged = store
        .calendar_sync_begin(&account, CAL, CalendarSyncMode::Delta)
        .unwrap();
    stage(&mut store, &staged, &page(vec![moved], "published"));
    assert_eq!(three_days(&store)["total"], 2);
    store.calendar_publish_sync(&staged).unwrap();
    assert_eq!(three_days(&store)["total"], 3);
    finalize(&mut store, &staged);
    drop(store);
    assert_eq!(
        three_days(&Store::open_live(dir.path()).unwrap())["total"],
        3
    );
}

#[test]
fn retained_cancellations_do_not_restore_absent_parents_or_bypass_catalog_privacy_and_archive() {
    let (_dir, mut store, account) = setup();
    let tombstone = cancelled("cancelled", json!({"dateTime":"2026-09-13T09:00:00Z"}));
    let lease = publish_events(
        &mut store,
        &account,
        vec![master(), tombstone],
        CalendarSyncMode::Full,
        "initial",
    );
    finalize(&mut store, &lease);
    let lease = publish_events(
        &mut store,
        &account,
        vec![],
        CalendarSyncMode::Full,
        "empty",
    );
    finalize(&mut store, &lease);
    assert_eq!(three_days(&store)["total"], 0);
    let mut private = master();
    private["visibility"] = "private".into();
    publish_catalog(
        &mut store,
        &account,
        vec![catalog(CAL, "writerWithoutPrivateAccess")],
        CalendarSyncMode::Full,
    );
    let lease = publish_events(
        &mut store,
        &account,
        vec![private],
        CalendarSyncMode::Full,
        "private",
    );
    finalize(&mut store, &lease);
    let result = three_days(&store);
    assert_eq!(result["total"], 2);
    for row in result["events"].as_array().unwrap() {
        assert_eq!(row["busy_only"], true);
        assert_eq!(row["title"], "Busy");
        assert!(row.get("event_id").is_none());
    }
    publish_catalog(&mut store, &account, vec![], CalendarSyncMode::Full);
    assert_eq!(three_days(&store)["total"], 0);
    publish_catalog(
        &mut store,
        &account,
        vec![catalog(CAL, "owner")],
        CalendarSyncMode::Full,
    );
    assert_eq!(three_days(&store)["total"], 2);
    let revision = store.account_lifecycle(&account).unwrap()["revision"].clone();
    store
        .account_remove_begin(
            &account,
            &json!({"request_id":"remove","expected_revision":revision,"retention":"archive"}),
        )
        .unwrap();
    assert_eq!(three_days(&store)["total"], 0);
    let archived = store
        .calendar_combined_for(
            &json!({"from":"2026-09-12","to":"2026-09-15"}),
            Some(&account),
        )
        .unwrap();
    assert_eq!(archived["total"], 2);
    assert!(archived["events"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["account_archived"] == true && v["can_edit"] == false));
}

#[test]
fn equal_provider_ids_in_other_accounts_and_calendars_do_not_cross_suppress() {
    let (_dir, mut store, account) = setup();
    let second = store
        .bind_google_account("second-subject", "second@example.test", &scopes(), None)
        .unwrap();
    publish_catalog(
        &mut store,
        &second,
        vec![catalog(CAL, "owner")],
        CalendarSyncMode::Full,
    );
    let lease = publish_events(
        &mut store,
        &account,
        vec![
            master(),
            cancelled("cancelled", json!({"dateTime":"2026-09-13T09:00:00Z"})),
        ],
        CalendarSyncMode::Full,
        "first",
    );
    finalize(&mut store, &lease);
    publish_events(
        &mut store,
        &second,
        vec![master()],
        CalendarSyncMode::Full,
        "second",
    );
    publish_catalog(
        &mut store,
        &account,
        vec![catalog(CAL, "owner"), catalog("other-calendar", "owner")],
        CalendarSyncMode::Full,
    );
    let other = store
        .calendar_sync_begin(&account, "other-calendar", CalendarSyncMode::Full)
        .unwrap();
    stage(&mut store, &other, &page(vec![master()], "other"));
    store.calendar_publish_sync(&other).unwrap();
    let result = three_days(&store);
    let rows = result["events"].as_array().unwrap();
    for (owner, calendar, count) in [
        (&account, CAL, 2),
        (&second, CAL, 3),
        (&account, "other-calendar", 3),
    ] {
        assert_eq!(
            rows.iter()
                .filter(|v| v["account"] == owner.as_str() && v["calendar_id"] == calendar)
                .count(),
            count
        );
    }
}

#[test]
fn malformed_duplicate_and_byte_limited_children_block_their_series() {
    for mode in ["malformed", "duplicate", "bytes", "count"] {
        let (_dir, mut store, account) = setup();
        let mut items = vec![master()];
        let count = match mode {
            "bytes" => 40,
            "count" => 1001,
            "duplicate" => 2,
            _ => 1,
        };
        for n in 0..count {
            let original = if matches!(mode, "bytes" | "count") {
                (chrono::DateTime::parse_from_rfc3339("2026-09-13T09:00:00Z").unwrap()
                    + chrono::Duration::minutes(n as i64))
                .to_rfc3339()
            } else {
                "2026-09-13T09:00:00Z".into()
            };
            let mut item = cancelled(&format!("child-{n:04}"), json!({"dateTime":original}));
            if mode == "bytes" {
                item["retained_provider_field"] = "x".repeat(230_000).into();
            }
            items.push(item);
        }
        let lease = store
            .calendar_sync_begin(&account, CAL, CalendarSyncMode::Full)
            .unwrap();
        let pages = items.chunks(250).collect::<Vec<_>>();
        for (index, items) in pages.iter().enumerate() {
            let last = index + 1 == pages.len();
            let page = CalendarPage {
                input_cursor: (index > 0).then(|| format!("page-{index}")),
                items: items.to_vec(),
                next_page_token: (!last).then(|| format!("page-{}", index + 1)),
                next_sync_token: last.then(|| "complete".into()),
            };
            stage(&mut store, &lease, &page);
        }
        store.calendar_publish_sync(&lease).unwrap();
        finalize(&mut store, &lease);
        if mode == "malformed" {
            // Simulate an unreadable retained cancellation without destroying
            // its indexed series association. This is a fail-closed read test.
            store.db.execute("UPDATE calendar_sources SET json=json_set(json,'$.originalStartTime',json('{\"date\":\"invalid\"}')) WHERE account=?1 AND calendar_id=?2 AND event_id='child-0000'",params![account,CAL]).unwrap();
        }
        let result = three_days(&store);
        assert_eq!(result["total"], 0, "{mode}");
        assert_eq!(result["coverage"]["complete"], false, "{mode}");
        assert_eq!(result["coverage"]["projection_limited"], true, "{mode}");
    }
}

#[test]
fn hidden_published_cancelled_master_blocks_a_still_current_moved_child() {
    let (dir, mut store, account) = setup();
    let mut child = event("moved", "Must not survive its cancelled master");
    child["recurringEventId"] = "series".into();
    child["originalStartTime"] = json!({"dateTime":"2026-09-13T09:00:00Z"});
    child["start"] = json!({"dateTime":"2026-09-20T09:00:00Z"});
    child["end"] = json!({"dateTime":"2026-09-20T10:00:00Z"});
    let lease = publish_events(
        &mut store,
        &account,
        vec![master(), child],
        CalendarSyncMode::Full,
        "initial",
    );
    finalize(&mut store, &lease);
    assert_eq!(range(&store, "2026-09-20", "2026-09-21")["total"], 1);
    let lease = publish_events(
        &mut store,
        &account,
        vec![json!({"id":"series","status":"cancelled"})],
        CalendarSyncMode::Delta,
        "cancel-master",
    );
    finalize(&mut store, &lease);
    store.db.execute("UPDATE calendar_event_state SET visible=0 WHERE account=?1 AND calendar_id=?2 AND event_id='series'",params![account,CAL]).unwrap();
    assert_eq!(range(&store, "2026-09-20", "2026-09-21")["total"], 0);
    let current: bool = store.db.query_row("SELECT EXISTS(SELECT 1 FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id='moved')",params![account,CAL],|r|r.get(0)).unwrap();
    assert!(
        current,
        "child remains current; only parent cancellation suppresses it"
    );
    drop(store);
    let store = Store::open_live(dir.path()).unwrap();
    assert_eq!(range(&store, "2026-09-20", "2026-09-21")["total"], 0);
    assert_eq!(
        store
            .stored_calendar_event(&account, CAL, "series")
            .unwrap_err()
            .code,
        "not_found"
    );
}

#[test]
fn schema_eighteen_upgrade_changes_only_views_and_preserves_sources_and_legacy_intent() {
    let (dir, mut store, account) = setup();
    let lease = publish_events(
        &mut store,
        &account,
        vec![
            master(),
            cancelled("cancelled", json!({"dateTime":"2026-09-13T09:00:00Z"})),
        ],
        CalendarSyncMode::Full,
        "initial",
    );
    finalize(&mut store, &lease);
    store.calendar_live_prepare(&account, &json!({"request_id":"legacy-v2","kind":"edit","calendar_id":CAL,"event_id":"ordinary","expected_etag":"v1","fields":{"title":"Retained authored change"}})).unwrap();
    let snapshot = |store: &Store| -> BTreeMap<String, Vec<Vec<rusqlite::types::Value>>> {
        [
            "accounts",
            "calendar_sources",
            "calendar_event_state",
            "calendar_event_index",
            "calendar_sync_state",
            "calendar_write_operations",
        ]
        .into_iter()
        .map(|table| {
            let retained_columns = if table == "calendar_write_operations" { "account,request_id,fingerprint,intent_json,result_json,execution_mode,state,operation_revision,attempt,connection_generation,permission_epoch,dispatch_json,outcome_json" } else { "*" };
            let mut query = store
                .db
                .prepare(&format!("SELECT {retained_columns} FROM {table} ORDER BY rowid"))
                .unwrap();
            let columns = query.column_count();
            let rows = query
                .query_map([], |r| (0..columns).map(|i| r.get(i)).collect())
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            (table.into(), rows)
        })
        .collect()
    };
    crate::test_profiles::restore_schema_eighteen(&store.db);
    let before = snapshot(&store);
    let visible_before: Vec<(String, String)> = store
        .db
        .prepare("SELECT event_id,json FROM calendar_visible_sources ORDER BY event_id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    drop(store);
    let store = Store::open_live(dir.path()).unwrap();
    assert_eq!(snapshot(&store), before);
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        crate::SCHEMA_VERSION
    );
    let visible_after: Vec<(String, String)> = store
        .db
        .prepare("SELECT event_id,json FROM calendar_visible_sources ORDER BY event_id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(visible_before, visible_after);
    assert_eq!(three_days(&store)["total"], 2);
    let plan: Vec<String> = store.db.prepare("EXPLAIN QUERY PLAN SELECT i.event_id FROM calendar_event_index i INDEXED BY calendar_index_series JOIN calendar_published_sources p ON p.account=i.account AND p.calendar_id=i.calendar_id AND p.event_id=i.event_id AND p.slot=i.slot AND p.generation=i.generation WHERE i.account=?1 AND i.calendar_id=?2 AND i.series_id=?3 AND ((p.visible=1 AND p.seen_full_generation>=p.full_generation) OR i.cancelled=1) ORDER BY i.event_id LIMIT 1001").unwrap().query_map(params![account,CAL,"series"],|r|r.get(3)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    assert!(
        plan.iter()
            .any(|v| v.contains("SEARCH i USING INDEX calendar_index_series")),
        "{plan:?}"
    );
    store
        .db
        .pragma_update(None, "user_version", crate::SCHEMA_VERSION + 1)
        .unwrap();
    drop(store);
    assert_eq!(
        Store::open_live(dir.path()).err().unwrap().code,
        "schema_mismatch"
    );
}
