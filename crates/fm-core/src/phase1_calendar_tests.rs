use super::*;

fn fields_value() -> Value {
    json!({"title":"Design review — 日本語","start":"2026-09-12T09:00:00-04:00","end":"2026-09-12T10:00:00-04:00","all_day":false,"time_zone":"America/Toronto","location":"Room 2","description":"Human event notes","conference_url":"https://example.com/join","attendees":["you@personal.example"]})
}
fn create(id: &str) -> Value {
    json!({"request_id":id,"kind":"create","calendar_id":"primary","fields":fields_value()})
}
fn combined(store: &Store) -> Value {
    store
        .calendar_combined(&json!({"from":"2026-09-07","to":"2026-09-14","calendars":null}))
        .unwrap()
}

#[test]
fn combined_scope_is_independent_with_restricted_fields_and_empty_selection() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path()).unwrap();
    store.db.execute("UPDATE calendar_sync_state SET access='reader' WHERE account='personal' AND calendar_id='primary'", []).unwrap();
    let private = json!({"id":"private-only","etag":"private-etag","summary":"PRIVATE APPOINTMENT","description":"PRIVATE NOTES","visibility":"private","start":{"dateTime":"2026-09-11T08:00:00-04:00"},"end":{"dateTime":"2026-09-11T09:00:00-04:00"}});
    store
        .db
        .execute(
            "INSERT INTO calendar_sources(account,calendar_id,event_id,json) VALUES('personal','primary','private-only',?1)",
            [private.to_string()],
        )
        .unwrap();
    let data = combined(&store);
    let rows = data["events"].as_array().unwrap();
    let origins = rows
        .iter()
        .map(|r| r["account"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        origins,
        BTreeSet::from(["personal", "sideproject", "windscribe"])
    );
    assert!(!data.to_string().contains("RESTRICTED PRIVATE"));
    assert!(!data.to_string().contains("PRIVATE LOCATION"));
    assert!(!data.to_string().contains("PRIVATE APPOINTMENT"));
    assert!(!data.to_string().contains("PRIVATE NOTES"));
    assert!(!data.to_string().contains("private-etag"));
    let busy = rows
        .iter()
        .find(|r| r["calendar_id"] == "availability")
        .unwrap();
    assert_eq!(busy["busy_only"], true);
    assert_eq!(busy["can_edit"], false);
    assert!(busy.get("etag").is_none());
    let empty = store
        .calendar_combined(&json!({"from":"2026-09-07","to":"2026-09-14","calendars":[]}))
        .unwrap();
    assert_eq!(empty["total"], 0);
    assert_eq!(
        store
            .calendar_combined(
                &json!({"from":"2026-09-07","to":"2026-09-14","query":"PRIVATE DESCRIPTION"})
            )
            .unwrap_err()
            .code,
        "invalid_request"
    );
    let identities = rows
        .iter()
        .filter(|r| r["event_id"] == "cross-account")
        .map(|r| r["id"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(identities.len(), 3);
}

#[test]
fn creation_replay_restart_edit_and_stale_revision_preserve_one_source() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let request = create("create-once");
    let created = store.calendar_write("personal", &request).unwrap();
    assert_eq!(created["state"], "fake_acknowledged");
    assert_eq!(store.calendar_write("personal", &request).unwrap(), created);
    let mut prepare_replay = request.clone();
    prepare_replay["prepare_only"] = true.into();
    assert_eq!(
        store.calendar_write("personal", &prepare_replay).unwrap(),
        created,
        "An acknowledged operation cannot become pending when replayed through prepare_only"
    );
    let target = created["target"]["event"].as_str().unwrap().to_owned();
    drop(store);
    let mut store = Store::open(temp.path()).unwrap();
    assert_eq!(store.calendar_write("personal", &request).unwrap(), created);
    let mut edit = json!({"request_id":"edit-once","kind":"edit","calendar_id":"primary","event_id":target,"expected_etag":created["etag"],"fields":fields_value()});
    edit["fields"]["title"] = "Edited event".into();
    let edited = store.calendar_write("personal", &edit).unwrap();
    assert_ne!(edited["etag"], created["etag"]);
    edit["request_id"] = "stale-edit".into();
    assert_eq!(
        store.calendar_write("personal", &edit).unwrap_err().code,
        "stale_revision"
    );
    let data = combined(&store);
    let events = data["events"].as_array().unwrap();
    assert_eq!(events.iter().filter(|e| e["event_id"] == target).count(), 1);
    assert!(events.iter().any(|e| e["title"] == "Edited event"));
    let event = events.iter().find(|e| e["event_id"] == target).unwrap();
    assert_eq!(event["conference_url"], "https://example.com/join");
    assert_eq!(event["location"], "Room 2");
    assert_eq!(event["description"], "Human event notes");
    assert_eq!(event["can_rsvp"], true);
    let mut different = request.clone();
    different["fields"]["title"] = "Another".into();
    assert_eq!(
        store
            .calendar_write("personal", &different)
            .unwrap_err()
            .code,
        "request_id_conflict"
    );
}

#[test]
fn durable_prepare_rechecks_revoked_permissions_before_publishing() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let mut request = create("prepared");
    request["prepare_only"] = true.into();
    assert_eq!(
        store.calendar_write("personal", &request).unwrap()["state"],
        "pending"
    );
    drop(store);
    let mut store = Store::open(temp.path()).unwrap();
    store.db.execute("UPDATE calendar_sync_state SET access='reader' WHERE account='personal' AND calendar_id='primary'",[]).unwrap();
    assert_eq!(
        store
            .calendar_write_ack("personal", &json!({"request_id":"prepared"}))
            .unwrap_err()
            .code,
        "permission_denied"
    );
    assert!(!combined(&store).to_string().contains("Design review"));
    assert_eq!(
        store.calendar_operations("personal", &json!({})).unwrap()["operations"][0]["state"],
        "failed"
    );
}

#[test]
fn self_rsvp_preserves_other_attendees_and_replays() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let request = json!({"request_id":"rsvp","kind":"rsvp","calendar_id":"primary","event_id":"cross-account","expected_etag":"v1","response":"accepted"});
    let result = store.calendar_write("personal", &request).unwrap();
    assert_eq!(store.calendar_write("personal", &request).unwrap(), result);
    let data = combined(&store);
    let row = data["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["event_id"] == "cross-account" && r["account"] == "personal")
        .unwrap();
    assert_eq!(row["self_response"], "accepted");
    assert_eq!(row["attendees"][1]["responseStatus"], "accepted");
    let mut denied = request.clone();
    denied["request_id"] = "busy-rsvp".into();
    denied["calendar_id"] = "availability".into();
    denied["event_id"] = "restricted".into();
    assert_eq!(
        store
            .calendar_write("sideproject", &denied)
            .unwrap_err()
            .code,
        "permission_denied"
    );
}

#[test]
fn invalid_fields_and_recurring_changes_are_explicitly_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let mut request = create("bad");
    request["fields"]["end"] = "2026-09-12T08:00:00-04:00".into();
    assert!(store.calendar_write("personal", &request).is_err());
    request["fields"] = fields_value();
    request["fields"]["recurrence"] = json!(["RRULE:FREQ=DAILY"]);
    assert!(store.calendar_write("personal", &request).is_err());
    request["fields"] = fields_value();
    request["fields"]["time_zone"] = "Not/AZone".into();
    assert!(store.calendar_write("personal", &request).is_err());
    request["fields"] = fields_value();
    request["fields"]["title"] = "".into();
    assert!(store
        .calendar_write("personal", &request)
        .unwrap_err()
        .message
        .contains("title"));
    let mut bounded = create("bounded-title");
    bounded["fields"]["title"] = "é".repeat(256).into();
    assert_eq!(
        store.calendar_write("personal", &bounded).unwrap()["state"],
        "fake_acknowledged",
        "The documented 512-byte Unicode title limit must be accepted"
    );
    bounded["request_id"] = "oversized-title".into();
    bounded["fields"]["title"] = format!("{}a", "é".repeat(256)).into();
    assert_eq!(
        store.calendar_write("personal", &bounded).unwrap_err().code,
        "invalid_request"
    );
    let raw = json!({"id":"series-test","etag":"v1","summary":"Series","start":{"dateTime":"2026-09-11T08:00:00-04:00"},"end":{"dateTime":"2026-09-11T09:00:00-04:00"},"recurrence":["RRULE:FREQ=DAILY;COUNT=3"]});
    store
        .db
        .execute(
            "INSERT INTO calendar_sources(account,calendar_id,event_id,json) VALUES('personal','primary','series-test',?1)",
            [raw.to_string()],
        )
        .unwrap();
    let request = json!({"request_id":"series-edit","kind":"edit","calendar_id":"primary","event_id":"series-test","expected_etag":"v1","fields":fields_value()});
    assert_eq!(
        store.calendar_write("personal", &request).unwrap_err().code,
        "unsupported_operation"
    );
}

#[test]
fn six_week_month_across_fall_dst_is_a_valid_bounded_calendar_window() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path()).unwrap();
    let result = store
        .calendar_combined(&json!({"from":"2026-10-12","to":"2026-11-23","calendars":null}))
        .unwrap();
    assert_eq!(result["total"], 0);
    assert!(store
        .calendar_combined(&json!({"from":"2026-10-12","to":"2026-11-24","calendars":null}))
        .is_err());
}

#[test]
fn aggregate_occurrence_limit_never_silently_truncates_multiple_accounts() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path()).unwrap();
    let tx = store.db.unchecked_transaction().unwrap();
    for (account, count) in [("personal", 500), ("windscribe", 501)] {
        for index in 0..count {
            let source = json!({"id":format!("limit-{index}"),"etag":"v1","summary":"Bound check","start":{"date":"2026-12-10"},"end":{"date":"2026-12-11"}});
            tx.execute(
                "INSERT INTO calendar_sources(account,calendar_id,event_id,json) VALUES(?1,'primary',?2,?3)",
                params![account, source["id"].as_str().unwrap(), source.to_string()],
            )
            .unwrap();
        }
    }
    tx.commit().unwrap();
    let request = json!({"from":"2026-12-10","to":"2026-12-11","calendars":null});
    assert_eq!(
        store.calendar_combined(&request).unwrap_err().code,
        "limit_exceeded"
    );
    let mut narrower = request;
    narrower["calendars"] = json!([{"account":"personal","calendar_id":"primary"}]);
    assert_eq!(store.calendar_combined(&narrower).unwrap()["total"], 500);
}

#[test]
fn title_edit_preserves_self_and_existing_attendee_response_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let mut event_fields = fields_value();
    event_fields["attendees"] = json!(["you@personal.example", "colleague@example.com"]);
    let edited=store.calendar_write("personal",&json!({"request_id":"retain-attendees","kind":"edit","calendar_id":"primary","event_id":"cross-account","expected_etag":"v1","fields":event_fields})).unwrap();
    store.calendar_write("personal",&json!({"request_id":"after-edit-rsvp","kind":"rsvp","calendar_id":"primary","event_id":"cross-account","expected_etag":edited["etag"],"response":"tentative"})).unwrap();
    let data = combined(&store);
    let row = data["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["event_id"] == "cross-account" && r["account"] == "personal")
        .unwrap();
    assert_eq!(row["self_response"], "tentative");
    assert_eq!(row["attendees"][1]["responseStatus"], "accepted");
}

#[test]
fn unloaded_or_revoked_selected_calendars_are_not_reported_as_complete_empty_days() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let scope = fm_calendar::SyncScope {
        account: "personal".into(),
        calendar: "unloaded".into(),
        query_fingerprint: "phase1-test".into(),
    };
    store
        .calendar_configure(&scope, "America/Toronto", Access::Reader)
        .unwrap();
    let request = json!({"from":"2026-09-11","to":"2026-09-12","calendars":[{"account":"personal","calendar_id":"unloaded"}]});
    let unloaded = store.calendar_combined(&request).unwrap();
    assert_eq!(unloaded["total"], 0);
    assert_eq!(unloaded["coverage"]["complete"], false);
    assert_eq!(unloaded["coverage"]["state"], "incomplete");
    store
        .calendar_configure(&scope, "America/Toronto", Access::None)
        .unwrap();
    let denied = store.calendar_combined(&request).unwrap();
    assert_eq!(denied["total"], 0);
    assert_eq!(denied["coverage"]["complete"], false);
    assert_eq!(denied["coverage"]["state"], "restricted");
}

#[test]
fn public_operation_history_prioritizes_pending_and_pages_through_all_records() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let mut request = create("oldest-pending");
    request["prepare_only"] = true.into();
    store.calendar_write("personal", &request).unwrap();
    let raw: String = store.db.query_row("SELECT intent_json FROM calendar_write_operations WHERE account='personal' AND request_id='oldest-pending'", [], |row| row.get(0)).unwrap();
    let tx = store.db.unchecked_transaction().unwrap();
    for index in 0..65 {
        let mut intent: Value = serde_json::from_str(&raw).unwrap();
        if index == 0 {
            intent["last_error"] =
                json!({"code":"permission_denied","message":"Calendar access changed"});
        }
        tx.execute(
            "INSERT INTO calendar_write_operations(account,request_id,fingerprint,intent_json,result_json) VALUES('personal',?1,?1,?2,NULL)",
            params![format!("pending-{index}"), intent.to_string()],
        )
        .unwrap();
    }
    for index in 0..65 {
        tx.execute(
            "INSERT INTO calendar_write_operations(account,request_id,fingerprint,intent_json,result_json) VALUES('personal',?1,?1,?2,'{}')",
            params![format!("saved-{index}"), raw],
        )
        .unwrap();
    }
    tx.execute(
        "INSERT INTO calendar_write_operations(account,request_id,fingerprint,intent_json,result_json) VALUES('personal','internal','internal','{}',NULL)",
        [],
    )
    .unwrap();
    tx.commit().unwrap();
    drop(store);
    let store = Store::open(temp.path()).unwrap();
    let mut params = json!({"limit":50});
    let mut ids = BTreeSet::new();
    let mut states = Vec::new();
    loop {
        let page = store.calendar_operations("personal", &params).unwrap();
        assert!(page["operations"].as_array().unwrap().len() <= 50);
        for operation in page["operations"].as_array().unwrap() {
            assert!(
                ids.insert(operation["request_id"].as_str().unwrap().to_owned()),
                "No operation should be repeated between unchanged pages"
            );
            states.push(operation["state"].as_str().unwrap().to_owned());
            if operation["request_id"] == "pending-0" {
                assert_eq!(operation["state"], "failed");
                assert_eq!(operation["error"]["message"], "Calendar access changed");
            }
        }
        if page["continuation"].is_null() {
            break;
        }
        params["cursor"] = page["continuation"].clone();
    }
    assert_eq!(ids.len(), 131);
    assert!(ids.contains("oldest-pending"));
    assert!(!ids.contains("internal"));
    assert!(states[..66]
        .iter()
        .all(|state| state != "fake_acknowledged"));
    assert!(states[66..]
        .iter()
        .all(|state| state == "fake_acknowledged"));
    assert_eq!(
        store
            .calendar_operations(
                "personal",
                &json!({"cursor":{"acknowledged":false,"before_id":0}})
            )
            .unwrap_err()
            .code,
        "invalid_request"
    );
}
