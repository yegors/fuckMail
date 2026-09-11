use super::*;
use fm_calendar::{EventKey, PageResult, SyncBatch};
use std::collections::BTreeMap;

fn make_scope(account: &str, calendar: &str) -> SyncScope {
    SyncScope {
        account: account.into(),
        calendar: calendar.into(),
        query_fingerprint: "events-showDeleted-no-expansion-v1".into(),
    }
}
fn event(id: &str, title: &str, etag: &str) -> Value {
    json!({"id":id,"etag":etag,"summary":title,"start":{"dateTime":"2026-09-11T09:00:00-04:00","timeZone":"America/Toronto"},"end":{"dateTime":"2026-09-11T10:00:00-04:00","timeZone":"America/Toronto"}})
}
fn completed(scope: &SyncScope, events: Vec<Value>, token: &str, full: bool) -> CompletedBatch {
    CompletedBatch {
        scope: scope.clone(),
        replacements: events
            .into_iter()
            .map(|v| (v["id"].as_str().unwrap().to_owned(), v))
            .collect::<BTreeMap<_, _>>(),
        next_sync_token: token.into(),
        full_replace: full,
    }
}
fn seed(store: &mut Store, scope: &SyncScope, events: Vec<Value>, access: Access) -> Value {
    let configured = store
        .calendar_configure(scope, "America/Toronto", access)
        .unwrap();
    store
        .calendar_commit_batch(
            configured["revision"].as_i64().unwrap(),
            &completed(scope, events, "token1", true),
        )
        .unwrap()
}
fn retained(store: &Store, scope: &SyncScope, id: &str) -> Value {
    let raw: String = store
        .db
        .query_row(
            "SELECT json FROM calendar_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",
            params![scope.account, scope.calendar, id],
            |r| r.get(0),
        )
        .unwrap();
    serde_json::from_str(&raw).unwrap()
}
fn rows(store: &mut Store, account: &str) -> Vec<Value> {
    store
        .handle(
            "calendar",
            Some(account),
            json!({"from":"2026-09-07","to":"2026-09-14"}),
        )
        .unwrap()["events"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn incomplete_expired_batch_retains_previous_view_until_complete_commit_and_replay() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let scope = make_scope("windscribe", "shared");
    seed(
        &mut store,
        &scope,
        vec![event("old", "Retained event", "v1")],
        Access::Reader,
    );
    let before = store
        .calendar_checkpoint(&scope.account, &scope.calendar)
        .unwrap();
    let mut batch = SyncBatch::new(scope.clone(), true).unwrap();
    assert!(matches!(
        batch
            .apply_page(
                &scope,
                None,
                vec![event("new", "Not published yet", "v1")],
                Some("p2".into()),
                None
            )
            .unwrap(),
        PageResult::NextPage(_)
    ));
    assert_eq!(
        store
            .calendar_checkpoint(&scope.account, &scope.calendar)
            .unwrap(),
        before
    );
    assert_eq!(retained(&store, &scope, "old")["summary"], "Retained event");
    let mut batch = batch.expired_token().unwrap();
    drop(store);
    let mut store = Store::open(temp.path()).unwrap();
    assert_eq!(
        store
            .calendar_checkpoint(&scope.account, &scope.calendar)
            .unwrap(),
        before
    );
    batch
        .apply_page(
            &scope,
            None,
            vec![event("first", "First new event", "v2")],
            Some("next".into()),
            None,
        )
        .unwrap();
    let PageResult::Complete(batch) = batch
        .apply_page(
            &scope,
            Some("next"),
            vec![event("second", "Second new event", "v2")],
            None,
            Some("token2".into()),
        )
        .unwrap()
    else {
        panic!("expected completed batch")
    };
    let revision = before["revision"].as_i64().unwrap();
    let result = store.calendar_commit_batch(revision, &batch).unwrap();
    assert_eq!(result["sync_token"], "token2");
    assert_eq!(result["applied_events"], 2);
    assert_eq!(retained(&store, &scope, "old")["summary"], "Retained event");
    let visible:bool=store.db.query_row("SELECT visible FROM calendar_event_state WHERE account=?1 AND calendar_id=?2 AND event_id='old'",params![scope.account,scope.calendar],|r|r.get(0)).unwrap();
    assert!(!visible);
    drop(store);
    let mut store = Store::open(temp.path()).unwrap();
    assert_eq!(
        store.calendar_commit_batch(revision, &batch).unwrap(),
        result
    );
    let mut changed = batch.clone();
    changed.next_sync_token = "changed-token".into();
    assert_eq!(
        store
            .calendar_commit_batch(revision, &changed)
            .unwrap_err()
            .code,
        "stale_revision"
    );
}

#[test]
fn failed_second_calendar_source_rolls_back_sources_visibility_cursor_and_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let scope = make_scope("windscribe", "atomic");
    seed(
        &mut store,
        &scope,
        vec![event("old", "Original event", "v1")],
        Access::Writer,
    );
    let before = store
        .calendar_checkpoint(&scope.account, &scope.calendar)
        .unwrap();
    let rev = before["revision"].as_i64().unwrap();
    let batch = completed(
        &scope,
        vec![
            event("a", "First insert", "v2"),
            event("b", "Fail insert", "v2"),
        ],
        "token2",
        true,
    );
    store.db.execute_batch("CREATE TEMP TRIGGER fail_calendar_second BEFORE INSERT ON calendar_sources WHEN NEW.calendar_id='atomic' AND NEW.event_id='b' BEGIN SELECT RAISE(ABORT,'controlled calendar insert failure'); END;").unwrap();
    assert_eq!(
        store.calendar_commit_batch(rev, &batch).unwrap_err().code,
        "storage_error"
    );
    assert_eq!(
        store
            .calendar_checkpoint(&scope.account, &scope.calendar)
            .unwrap(),
        before
    );
    let (sources,visible):(i64,i64)=store.db.query_row("SELECT COUNT(*),SUM(v.visible) FROM calendar_sources s JOIN calendar_event_state v USING(account,calendar_id,event_id) WHERE s.account=?1 AND s.calendar_id=?2",params![scope.account,scope.calendar],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    assert_eq!((sources, visible), (1, 1));
    drop(store);
    let mut store = Store::open(temp.path()).unwrap();
    store.calendar_commit_batch(rev, &batch).unwrap();
    assert_eq!(retained(&store, &scope, "a")["summary"], "First insert");
}

#[test]
fn sparse_cancelled_instance_keeps_identity_and_exact_observation() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let scope = make_scope("windscribe", "recurrence");
    let mut master = event("series", "Recurring meeting", "v1");
    master["recurrence"] = json!(["RRULE:FREQ=DAILY;COUNT=2"]);
    let mut moved = event("instance", "Moved instance", "v2");
    moved["recurringEventId"] = json!("series");
    moved["originalStartTime"] = moved["start"].clone();
    moved["start"]["dateTime"] = json!("2026-09-11T11:00:00-04:00");
    moved["end"]["dateTime"] = json!("2026-09-11T12:00:00-04:00");
    let original = moved["originalStartTime"].clone();
    seed(&mut store, &scope, vec![master, moved], Access::Writer);
    let rev = store
        .calendar_checkpoint(&scope.account, &scope.calendar)
        .unwrap()["revision"]
        .as_i64()
        .unwrap();
    let tombstone = json!({"id":"instance","status":"cancelled","etag":"v3"});
    store
        .calendar_commit_batch(
            rev,
            &completed(&scope, vec![tombstone.clone()], "token2", false),
        )
        .unwrap();
    let retained = retained(&store, &scope, "instance");
    assert_eq!(retained["originalStartTime"], original);
    assert_eq!(retained["recurringEventId"], "series");
    assert_eq!(retained["status"], "cancelled");
    let raw:String=store.db.query_row("SELECT observed_json FROM calendar_event_state WHERE account=?1 AND calendar_id=?2 AND event_id='instance'",params![scope.account,scope.calendar],|r|r.get(0)).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&raw).unwrap(), tombstone);
    let projected = rows(&mut store, "windscribe");
    let meetings: Vec<_> = projected
        .iter()
        .filter(|e| e["calendar_id"] == "recurrence")
        .collect();
    assert_eq!(meetings.len(), 1);
    assert!(meetings[0]["start"]
        .as_str()
        .unwrap()
        .starts_with("2026-09-12"));
}

#[test]
fn shared_calendar_projection_keeps_real_identity_and_filters_each_acl() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    for (calendar, access, private) in [
        ("owner-calendar", Access::Owner, false),
        ("reader-calendar", Access::Reader, true),
        ("busy-calendar", Access::FreeBusy, false),
        ("denied-calendar", Access::None, false),
    ] {
        let mut source = event("same-provider-id", "Confidential title", "v1");
        source["description"] = json!("Secret description");
        source["location"] = json!("Secret location");
        if private {
            source["visibility"] = json!("private");
        }
        seed(
            &mut store,
            &make_scope("windscribe", calendar),
            vec![source],
            access,
        );
    }
    seed(
        &mut store,
        &make_scope("personal", "owner-calendar"),
        vec![event("same-provider-id", "Personal secret", "v1")],
        Access::Owner,
    );
    let projected = rows(&mut store, "windscribe");
    assert!(!projected
        .iter()
        .any(|r| r["calendar_id"] == "denied-calendar" || r["title"] == "Personal secret"));
    let owner = projected
        .iter()
        .find(|r| r["calendar_id"] == "owner-calendar")
        .unwrap();
    assert_eq!(owner["title"], "Confidential title");
    assert_eq!(owner["read_only"], false);
    for calendar in ["reader-calendar", "busy-calendar"] {
        let row = projected
            .iter()
            .find(|r| r["calendar_id"] == calendar)
            .unwrap();
        assert_eq!(row["title"], "Busy");
        assert!(row.get("description").is_none());
        assert!(row.get("location").is_none());
        assert_ne!(owner["id"], row["id"]);
    }
    let before = store.revision().unwrap();
    let owner_scope = make_scope("windscribe", "owner-calendar");
    store
        .calendar_configure(&owner_scope, "America/Toronto", Access::Owner)
        .unwrap();
    assert_eq!(
        store.revision().unwrap(),
        before,
        "identical configuration is not a change"
    );
    store
        .calendar_configure(&owner_scope, "America/Toronto", Access::None)
        .unwrap();
    assert_eq!(
        store.revision().unwrap(),
        before + 1,
        "ACL revocation must wake client polling"
    );
    assert!(!rows(&mut store, "windscribe")
        .iter()
        .any(|row| row["calendar_id"] == "owner-calendar"));
    store
        .calendar_configure(&owner_scope, "UTC", Access::None)
        .unwrap();
    assert_eq!(
        store.revision().unwrap(),
        before + 2,
        "zone changes must wake client polling"
    );
}

#[test]
fn fake_edit_ack_is_durable_scoped_etag_checked_and_revoked_acl_cannot_commit() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let scope = make_scope("windscribe", "editable");
    let source = event("e", "Original title", "v1");
    seed(&mut store, &scope, vec![source.clone()], Access::Writer);
    let target = EventKey {
        account: scope.account.clone(),
        calendar: scope.calendar.clone(),
        event: "e".into(),
    };
    store
        .calendar_prepare_edit(
            &target,
            "edit-request",
            "v1",
            json!({"summary":"Human changed title"}),
        )
        .unwrap();
    assert_eq!(retained(&store, &scope, "e")["summary"], "Original title");
    let mut acknowledged = source.clone();
    acknowledged["summary"] = json!("Human changed title");
    acknowledged["etag"] = json!("v2");
    let mut wrong_occurrence = acknowledged.clone();
    wrong_occurrence["recurringEventId"] = json!("another-series");
    wrong_occurrence["originalStartTime"] = wrong_occurrence["start"].clone();
    assert_eq!(
        store
            .calendar_acknowledge(&target, "edit-request", wrong_occurrence)
            .unwrap_err()
            .code,
        "scope_mismatch"
    );
    let other = EventKey {
        calendar: "other".into(),
        ..target.clone()
    };
    assert_eq!(
        store
            .calendar_acknowledge(&other, "edit-request", acknowledged.clone())
            .unwrap_err()
            .code,
        "scope_mismatch"
    );
    drop(store);
    let mut store = Store::open(temp.path()).unwrap();
    let receipt = store
        .calendar_acknowledge(&target, "edit-request", acknowledged.clone())
        .unwrap();
    assert_eq!(
        retained(&store, &scope, "e")["summary"],
        "Human changed title"
    );
    assert_eq!(
        store
            .calendar_acknowledge(&target, "edit-request", acknowledged.clone())
            .unwrap(),
        receipt
    );
    assert_eq!(
        store
            .calendar_prepare_edit(&target, "stale-etag", "v1", json!({"summary":"Wrong"}))
            .unwrap_err()
            .code,
        "stale_revision"
    );
    store
        .calendar_prepare_edit(
            &target,
            "revoked-edit",
            "v2",
            json!({"summary":"Cannot commit after revoke"}),
        )
        .unwrap();
    store
        .calendar_configure(&scope, "America/Toronto", Access::Reader)
        .unwrap();
    acknowledged["summary"] = json!("Cannot commit after revoke");
    acknowledged["etag"] = json!("v3");
    assert_eq!(
        store
            .calendar_acknowledge(&target, "revoked-edit", acknowledged)
            .unwrap_err()
            .code,
        "permission_denied"
    );
    assert_eq!(
        retained(&store, &scope, "e")["summary"],
        "Human changed title"
    );
}

#[test]
fn fake_rsvp_uses_account_self_attendee_and_ack_transaction_is_replayable() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let scope = make_scope("windscribe", "invitations");
    let actor: String = store
        .db
        .query_row(
            "SELECT email FROM accounts WHERE id='windscribe'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut source = event("e", "Invitation", "v1");
    source["attendees"] = json!([{"email":actor,"self":true,"responseStatus":"needsAction"},{"email":"other@example.test","responseStatus":"accepted"}]);
    seed(&mut store, &scope, vec![source.clone()], Access::Reader);
    let target = EventKey {
        account: scope.account.clone(),
        calendar: scope.calendar.clone(),
        event: "e".into(),
    };
    let prepared = store
        .calendar_prepare_rsvp(&target, "rsvp-request", "v1", ResponseStatus::Accepted)
        .unwrap();
    let mut ack = source.clone();
    ack["attendees"] = prepared["intent"]["write"]["body"]["attendees"].clone();
    ack["etag"] = json!("v2");
    store.db.execute_batch("CREATE TEMP TRIGGER fail_calendar_ack BEFORE UPDATE ON calendar_sources WHEN NEW.calendar_id='invitations' BEGIN SELECT RAISE(ABORT,'controlled ack failure'); END;").unwrap();
    let before = store
        .calendar_checkpoint(&scope.account, &scope.calendar)
        .unwrap();
    assert_eq!(
        store
            .calendar_acknowledge(&target, "rsvp-request", ack.clone())
            .unwrap_err()
            .code,
        "storage_error"
    );
    assert_eq!(
        store
            .calendar_checkpoint(&scope.account, &scope.calendar)
            .unwrap(),
        before
    );
    assert_eq!(retained(&store, &scope, "e"), source);
    drop(store);
    let mut store = Store::open(temp.path()).unwrap();
    store
        .calendar_acknowledge(&target, "rsvp-request", ack)
        .unwrap();
    assert_eq!(
        retained(&store, &scope, "e")["attendees"][0]["responseStatus"],
        "accepted"
    );
    assert_eq!(
        retained(&store, &scope, "e")["attendees"][1],
        source["attendees"][1]
    );
    let foreign = make_scope("personal", "foreign-invite");
    let mut foreign_source = source;
    foreign_source["attendees"][0]["self"] = json!(true);
    seed(&mut store, &foreign, vec![foreign_source], Access::Reader);
    let foreign_target = EventKey {
        account: foreign.account,
        calendar: foreign.calendar,
        event: "e".into(),
    };
    assert_eq!(
        store
            .calendar_prepare_rsvp(
                &foreign_target,
                "foreign-response",
                "v1",
                ResponseStatus::Accepted
            )
            .unwrap_err()
            .code,
        "permission_denied"
    );
}

#[test]
fn remote_etag_change_preserves_pending_edit_and_query_scope_cannot_reuse_cursor() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let scope = make_scope("windscribe", "concurrent");
    seed(
        &mut store,
        &scope,
        vec![event("e", "Original", "v1")],
        Access::Owner,
    );
    let target = EventKey {
        account: scope.account.clone(),
        calendar: scope.calendar.clone(),
        event: "e".into(),
    };
    store
        .calendar_prepare_edit(
            &target,
            "pending-human-edit",
            "v1",
            json!({"summary":"Human change"}),
        )
        .unwrap();
    let before = store
        .calendar_checkpoint(&scope.account, &scope.calendar)
        .unwrap();
    let rev = before["revision"].as_i64().unwrap();
    store
        .calendar_commit_batch(
            rev,
            &completed(
                &scope,
                vec![event("e", "Organizer changed it", "v2")],
                "token2",
                false,
            ),
        )
        .unwrap();
    assert_eq!(
        store
            .calendar_acknowledge(
                &target,
                "pending-human-edit",
                event("e", "Human change", "v3")
            )
            .unwrap_err()
            .code,
        "stale_revision"
    );
    assert_eq!(
        retained(&store, &scope, "e")["summary"],
        "Organizer changed it"
    );
    let pending:bool=store.db.query_row("SELECT result_json IS NULL FROM calendar_write_operations WHERE account='windscribe' AND request_id='pending-human-edit'",[],|r|r.get(0)).unwrap();
    assert!(pending);
    let mut different = scope.clone();
    different.query_fingerprint = "different-calendar-query".into();
    assert_eq!(
        store
            .calendar_configure(&different, "America/Toronto", Access::Owner)
            .unwrap_err()
            .code,
        "scope_mismatch"
    );
    assert_eq!(
        store
            .calendar_checkpoint(&scope.account, &scope.calendar)
            .unwrap()["sync_token"],
        "token2"
    );
}

#[test]
fn mixed_calendar_midnights_use_display_day_and_rows_sort_by_actual_instant() {
    let temporary = tempfile::tempdir().unwrap();
    let mut store = Store::open(temporary.path()).unwrap();
    let timed = |id: &str, start: &str, end: &str, zone: &str| json!({"id":id,"etag":"v1","summary":id,"start":{"dateTime":start,"timeZone":zone},"end":{"dateTime":end,"timeZone":zone}});
    let utc = make_scope("windscribe", "utc-source");
    let configured = store
        .calendar_configure(&utc, "UTC", Access::Reader)
        .unwrap();
    let mut recurring = timed(
        "utc-recurring",
        "2026-09-11T02:00:00Z",
        "2026-09-11T03:00:00Z",
        "UTC",
    );
    recurring["recurrence"] = json!(["RRULE:FREQ=DAILY;COUNT=2"]);
    store
        .calendar_commit_batch(
            configured["revision"].as_i64().unwrap(),
            &completed(
                &utc,
                vec![
                    timed(
                        "utc-prior-display-day",
                        "2026-09-11T02:00:00Z",
                        "2026-09-11T03:00:00Z",
                        "UTC",
                    ),
                    timed(
                        "utc-late-display-day",
                        "2026-09-12T02:00:00Z",
                        "2026-09-12T03:00:00Z",
                        "UTC",
                    ),
                    timed(
                        "europe-early-instant",
                        "2026-09-11T09:00:00+02:00",
                        "2026-09-11T10:00:00+02:00",
                        "Europe/Paris",
                    ),
                    timed(
                        "toronto-later-instant",
                        "2026-09-11T08:00:00-04:00",
                        "2026-09-11T09:00:00-04:00",
                        "America/Toronto",
                    ),
                    recurring,
                ],
                "utc-token",
                true,
            ),
        )
        .unwrap();
    let tokyo = make_scope("windscribe", "tokyo-source");
    let configured = store
        .calendar_configure(&tokyo, "Asia/Tokyo", Access::Reader)
        .unwrap();
    store.calendar_commit_batch(configured["revision"].as_i64().unwrap(),&completed(&tokyo,vec![
        timed("tokyo-prior-display-day","2026-09-11T01:00:00+09:00","2026-09-11T02:00:00+09:00","Asia/Tokyo"),
        timed("tokyo-midday-display-day","2026-09-12T01:00:00+09:00","2026-09-12T02:00:00+09:00","Asia/Tokyo"),
        json!({"id":"all-day-today","etag":"v1","summary":"all-day-today","start":{"date":"2026-09-11"},"end":{"date":"2026-09-12"}}),
        json!({"id":"all-day-tomorrow","etag":"v1","summary":"all-day-tomorrow","start":{"date":"2026-09-12"},"end":{"date":"2026-09-13"}})],"tokyo-token",true)).unwrap();
    let day = store
        .handle(
            "calendar",
            Some("windscribe"),
            json!({"from":"2026-09-11","to":"2026-09-12"}),
        )
        .unwrap();
    assert_eq!(day["time_zone"], "America/Toronto");
    let rows: Vec<_> = day["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["calendar_id"] == "utc-source" || row["calendar_id"] == "tokyo-source")
        .collect();
    let titles: Vec<_> = rows
        .iter()
        .map(|row| row["title"].as_str().unwrap())
        .collect();
    assert!(
        titles.contains(&"utc-late-display-day"),
        "02:00Z next UTC date is 22:00 on displayed Toronto day"
    );
    assert!(titles.contains(&"tokyo-midday-display-day"));
    assert!(!titles.contains(&"utc-prior-display-day"));
    assert!(!titles.contains(&"tokyo-prior-display-day"));
    assert!(titles.contains(&"all-day-today"));
    assert!(!titles.contains(&"all-day-tomorrow"));
    assert_eq!(
        titles[0], "all-day-today",
        "all-day dates precede timed rows on the same displayed date"
    );
    assert!(
        titles
            .iter()
            .position(|title| *title == "europe-early-instant")
            .unwrap()
            < titles
                .iter()
                .position(|title| *title == "toronto-later-instant")
                .unwrap(),
        "07:00Z must sort before 12:00Z despite offset timestamp text"
    );
    let recurrence: Vec<_> = rows
        .iter()
        .filter(|row| row["title"] == "utc-recurring")
        .collect();
    assert_eq!(recurrence.len(), 1);
    assert_eq!(recurrence[0]["start"], "2026-09-12T02:00:00+00:00");
    assert_eq!(recurrence[0]["time_zone"], "UTC");
    let previous = store
        .handle(
            "calendar",
            Some("windscribe"),
            json!({"from":"2026-09-10","to":"2026-09-11"}),
        )
        .unwrap();
    let previous = previous["events"].as_array().unwrap();
    assert!(previous
        .iter()
        .any(|row| row["title"] == "utc-prior-display-day"));
    assert!(previous
        .iter()
        .any(|row| row["title"] == "tokyo-prior-display-day"));
    assert!(!previous
        .iter()
        .any(|row| row["title"] == "utc-late-display-day" || row["title"] == "all-day-today"));
}
