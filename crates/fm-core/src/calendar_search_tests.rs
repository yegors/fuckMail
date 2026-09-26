use super::*;
use crate::live_calendar::{CalendarPage, CalendarSyncMode};

fn event(id: &str, title: &str) -> Value {
    json!({"id":id,"etag":"v1","summary":title,"description":"Searchable agenda","location":"Blue Room",
        "organizer":{"email":"organizer@example.test","displayName":"Élodie Organizer"},
        "attendees":[{"email":"guest@example.test","displayName":"Guest Person"}],
        "start":{"dateTime":"2026-09-12T09:00:00Z","timeZone":"UTC"},
        "end":{"dateTime":"2026-09-12T10:00:00Z","timeZone":"UTC"}})
}
fn insert(store: &Store, account: &str, source: &Value) {
    store.db.execute("INSERT INTO calendar_sources(account,calendar_id,event_id,json) VALUES(?1,'primary',?2,?3)", rusqlite::params![account,source["id"].as_str().unwrap(),source.to_string()]).unwrap();
}
fn params(query: &str) -> Value {
    json!({"query":query,"from":"2026-09-12","to":"2026-09-15"})
}
fn search(store: &Store, query: &str) -> Value {
    store.calendar_search(&params(query)).unwrap()
}

#[test]
fn calendar_search_matches_only_projected_fields_and_preserves_identity_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let source = event("search-one", "Distinctive Needle");
    insert(&store, "personal", &source);
    insert(&store, "windscribe", &source);
    let before = store.db.total_changes();
    let revision = store.revision().unwrap();
    let result = store
        .handle(
            "calendar_search",
            None,
            params("nEEdle with:ÉLODIE with:guest@ title:distinctive"),
        )
        .unwrap();
    assert_eq!(result["total"], 2);
    assert_eq!(result["has_more"], false);
    assert_eq!(result["revision"], revision);
    assert_eq!(result["from"], "2026-09-12");
    assert_eq!(result["to"], "2026-09-15");
    let identities: BTreeSet<_> = result["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        identities.len(),
        2,
        "Equal provider IDs in distinct accounts stay distinct"
    );
    assert!(result["events"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["organizer_name"] == "Élodie Organizer"));
    assert_eq!(search(&store, "Needle impossible")["total"], 0);
    // Plain words are one phrase in the title or notes, from a word start.
    for (query, total) in [
        ("distinctive needle", 2),
        ("DISTINCTIVE NEE", 2),
        ("agenda", 2),
        ("needle distinctive", 0),
        ("eedle", 0),
        ("Blue Room", 0),
        ("guest person", 0),
        ("with:guest", 2),
        ("with:\"Guest Person\"", 2),
        ("guest@example.test", 2),
        ("with:nobody", 0),
        ("title:agenda", 0),
        ("title:needle agenda", 0),
    ] {
        assert_eq!(search(&store, query)["total"], total, "{query}");
    }
    assert_eq!(store.db.total_changes(), before);
    assert_eq!(store.revision().unwrap(), revision);
    assert_eq!(store.db.query_row("SELECT json FROM calendar_sources WHERE account='personal' AND event_id='search-one'", [], |r| r.get::<_,String>(0)).unwrap(), source.to_string());
}

#[test]
fn calendar_search_selection_limits_and_multiday_dedup_are_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let mut multi = event("multi", "Unique search token");
    multi["start"] = json!({"date":"2026-09-12"});
    multi["end"] = json!({"date":"2026-09-15"});
    insert(&store, "personal", &multi);
    insert(&store, "personal", &event("second", "Unique search token"));
    insert(&store, "windscribe", &event("third", "Unique search token"));
    let all = search(&store, "Unique search token");
    assert_eq!(
        all["total"], 3,
        "Multiple daily paint segments are one search result"
    );
    let mut limited = params("Unique search token");
    limited["limit"] = 1.into();
    let limited = store.calendar_search(&limited).unwrap();
    assert_eq!(limited["total"], 1);
    assert_eq!(limited["has_more"], true);
    assert_eq!(
        limited["coverage"], all["coverage"],
        "Search never upgrades inherited coverage"
    );
    let revision = store.preferences().unwrap()["revision"].clone();
    store.preferences_save(&json!({"request_id":"search-selection","expected_revision":revision,"preferences":{"selected_calendars":[{"account":"personal","calendar_id":"primary"}]}})).unwrap();
    assert_eq!(search(&store, "Unique search token")["total"], 2);
    let mut explicit = params("Unique search token");
    explicit["calendars"] = json!([]);
    assert_eq!(store.calendar_search(&explicit).unwrap()["total"], 0);
    explicit["calendars"] = Value::Null;
    assert_eq!(store.calendar_search(&explicit).unwrap()["total"], 3);
    store
        .db
        .execute(
            "UPDATE accounts SET lifecycle_state='removed' WHERE id='personal'",
            [],
        )
        .unwrap();
    assert_eq!(
        store.calendar_search(&explicit).unwrap()["total"],
        1,
        "Archives are not searched by this global active query"
    );
}

#[test]
fn calendar_search_uses_exception_fields_and_never_exposes_private_master_details() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let mut master = event("search-series", "Master Zebra");
    master["recurrence"] = json!(["RRULE:FREQ=DAILY;COUNT=2"]);
    let mut child = event("search-exception", "Exception Heron");
    child["recurringEventId"] = "search-series".into();
    child["originalStartTime"] = json!({"dateTime":"2026-09-13T09:00:00Z","timeZone":"UTC"});
    child["start"]["dateTime"] = "2026-09-13T11:00:00Z".into();
    child["end"]["dateTime"] = "2026-09-13T12:00:00Z".into();
    child["description"] = "Changed Quetzal".into();
    child["organizer"]["displayName"] = "Exception Name".into();
    child["attendees"][0]["displayName"] = "Exception Guest".into();
    insert(&store, "personal", &master);
    insert(&store, "personal", &child);
    assert_eq!(
        search(&store, "quetzal title:heron with:\"Exception Guest\"")["total"],
        1
    );
    assert_eq!(search(&store, "heron with:élodie")["total"], 0);
    assert_eq!(search(&store, "Zebra")["total"], 1);
    master["visibility"] = "private".into();
    store.db.execute("UPDATE calendar_sources SET json=?1 WHERE account='personal' AND event_id='search-series'", [master.to_string()]).unwrap();
    for access in ["reader", "writer_without_private_access"] {
        store.db.execute("UPDATE calendar_sync_state SET access=?1 WHERE account='personal' AND calendar_id='primary'", [access]).unwrap();
        for query in [
            "Zebra",
            "Heron",
            "Quetzal",
            "Exception",
            "organizer@example.test",
        ] {
            assert_eq!(
                search(&store, query)["total"],
                0,
                "Private master restrictions apply to its exception ({access}, {query})"
            );
        }
        let projection = store
            .calendar_combined(&json!({"from":"2026-09-12","to":"2026-09-15"}))
            .unwrap();
        assert!(!projection.to_string().contains("Exception Name"));
    }
}

fn publish(store: &mut Store, lease: &crate::live_calendar::CalendarSyncLease, items: Vec<Value>) {
    let page = CalendarPage {
        input_cursor: None,
        items,
        next_page_token: None,
        next_sync_token: Some("next".into()),
    };
    let mut next = store.calendar_stage_page_chunk(lease, &page, 0).unwrap();
    while next < page.items.len() {
        next = store.calendar_stage_page_chunk(lease, &page, next).unwrap();
    }
    store.calendar_seal_page(lease, &page).unwrap();
}

#[test]
fn calendar_search_live_readonly_grants_allow_details_but_catalog_and_busy_grants_do_not() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open_live(dir.path()).unwrap();
    let account = store
        .bind_google_account(
            "search-subject",
            "reader@example.test",
            &BTreeSet::from(["https://www.googleapis.com/auth/calendar.readonly".into()]),
            None,
        )
        .unwrap();
    store.preferences_save(&json!({"request_id":"search-zone","expected_revision":0,"preferences":{"calendar_display_time_zone":"UTC"}})).unwrap();
    let catalog = store
        .calendar_catalog_begin(&account, CalendarSyncMode::Full)
        .unwrap();
    publish(
        &mut store,
        &catalog,
        vec![json!({"id":"primary","summary":"Calendar","timeZone":"UTC","accessRole":"owner"})],
    );
    while store.calendar_catalog_finalize_chunk(&catalog).unwrap() > 0 {}
    store.calendar_publish_sync(&catalog).unwrap();
    let lease = store
        .calendar_sync_begin(&account, "primary", CalendarSyncMode::Full)
        .unwrap();
    publish(
        &mut store,
        &lease,
        vec![event("live-search", "Permission Needle")],
    );
    store.calendar_publish_sync(&lease).unwrap();
    while store.calendar_finalize_chunk(&lease).unwrap() > 0 {}
    let value = search(&store, "needle with:Élodie");
    assert_eq!(value["total"], 1);
    assert_eq!(value["events"][0]["can_edit"], false);
    for scope in ["calendar.calendarlist.readonly", "calendar.freebusy"] {
        store
            .db
            .execute(
                "UPDATE google_accounts SET scopes_json=?1 WHERE account=?2",
                rusqlite::params![
                    json!([format!("https://www.googleapis.com/auth/{scope}")]).to_string(),
                    account
                ],
            )
            .unwrap();
        let value = search(&store, "Needle");
        assert_eq!(value["total"], 0);
        assert!(!value.to_string().contains("Permission Needle"));
    }
}

#[test]
fn calendar_search_refuses_malformed_and_unbounded_requests_without_changes() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let before = store.db.total_changes();
    for query in [
        "".into(),
        " \t\n".into(),
        "é".repeat(257),
        "word ".repeat(33),
    ] {
        assert_eq!(
            store.calendar_search(&params(&query)).unwrap_err().code,
            "invalid_request"
        );
    }
    for (key, value) in [
        ("query", json!(12)),
        ("limit", json!(0)),
        ("limit", json!(101)),
        ("limit", json!(1.5)),
        ("limit", Value::Null),
        ("from", json!("invalid")),
        ("to", json!("2027-01-01")),
        ("to", json!("2026-09-12")),
        ("extra", json!(true)),
        ("calendars", json!(false)),
        ("around", json!("2026-09-12")),
    ] {
        let mut p = params("Needle");
        p[key] = value;
        assert_eq!(
            store.calendar_search(&p).unwrap_err().code,
            "invalid_request",
            "{p}"
        );
    }
    assert_eq!(store.db.total_changes(), before);
}

#[test]
fn calendar_search_reads_title_with_date_and_bare_addresses() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let text = "project review maya@example.test title:\"Board deck\" with:\"Maya Chen\" date:tomorrow notes";
    let value = search(&store, text);
    assert_eq!(
        value["terms"],
        json!({"text":text,"words":["project review","notes"],"title":["Board deck"],
            "with":["maya@example.test","Maya Chen"],"date":"tomorrow"})
    );
    // date: alone moves the Calendar; it matches nothing and is not an error.
    insert(&store, "personal", &event("dated", "Dated"));
    let dated = search(&store, "date:2026-10-02");
    assert_eq!(dated["total"], 0);
    assert_eq!(dated["terms"]["date"], "2026-10-02");
    assert_eq!(search(&store, "title: with:")["total"], 0);
}

#[test]
fn calendar_search_around_lists_each_event_once_upcoming_then_recent_past() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let at = |id: &str, title: &str, day: &str| {
        let mut source = event(id, title);
        source["start"]["dateTime"] = format!("{day}T15:00:00Z").into();
        source["end"]["dateTime"] = format!("{day}T16:00:00Z").into();
        source
    };
    let mut weekly = at("weekly", "Weekly sync", "2026-08-01");
    weekly["recurrence"] = json!(["RRULE:FREQ=WEEKLY"]);
    let mut kickoff = at("kickoff", "Sync kickoff", "2026-08-01");
    kickoff["recurrence"] = json!(["RRULE:FREQ=DAILY;COUNT=10"]);
    for source in [
        weekly,
        kickoff,
        at("retro", "Sync retro", "2026-10-20"),
        at("ancient", "Sync ancient", "2026-06-01"),
        at("far", "Sync far", "2027-12-01"),
        at("other", "Unrelated", "2026-09-13"),
    ] {
        insert(&store, "personal", &source);
    }
    let around = |query: &str, limit: u64| {
        store
            .calendar_search(&json!({"query":query,"around":"2026-09-12","limit":limit}))
            .unwrap()
    };
    let value = around("sync", 7);
    let rows = value["events"].as_array().unwrap();
    let seen: Vec<_> = rows
        .iter()
        .map(|row| {
            (
                row["title"].as_str().unwrap(),
                row["past"].as_bool().unwrap(),
                row["start"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            ("Weekly sync", false, "2026-09-12T15:00:00+00:00"),
            ("Sync retro", false, "2026-10-20T15:00:00+00:00"),
            ("Sync kickoff", true, "2026-08-10T15:00:00+00:00"),
        ],
        "the next time of each event, then the latest past time within 60 days"
    );
    assert_eq!(value["has_more"], false);
    assert_eq!(value["around"], "2026-09-12");
    assert_eq!(value["from"], "2026-07-14");
    assert_eq!(value["to"], "2027-09-13");
    assert_eq!(value["terms"]["words"], json!(["sync"]));
    let limited = around("sync", 2);
    assert_eq!(limited["total"], 2);
    assert_eq!(limited["has_more"], true);
    assert_eq!(around("date:today", 7)["total"], 0);
    let before = store.db.total_changes();
    assert_eq!(around("nothing matches", 7)["total"], 0);
    assert_eq!(store.db.total_changes(), before);
    for bad in [json!("2026-9-12"), json!(null), json!(12)] {
        assert_eq!(
            store
                .calendar_search(&json!({"query":"sync","around":bad}))
                .unwrap_err()
                .code,
            "invalid_request"
        );
    }
}
