use super::*;
use crate::live_calendar::{CalendarPage, CalendarSyncMode};
use std::collections::BTreeSet;
const CAL: &str = "human@example.test";
fn setup() -> (tempfile::TempDir, Store, String) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open_live(dir.path()).unwrap();
    let scopes: BTreeSet<String> = [
        "openid",
        "email",
        "https://www.googleapis.com/auth/calendar.events",
        "https://www.googleapis.com/auth/calendar.calendarlist.readonly",
    ]
    .map(String::from)
    .into();
    let account = store
        .bind_google_account("calendar-write-subject", CAL, &scopes, None)
        .unwrap();
    let lease = store
        .calendar_catalog_begin(&account, CalendarSyncMode::Full)
        .unwrap();
    let page = CalendarPage {
        input_cursor: None,
        items: vec![json!({"id":CAL,"accessRole":"owner","timeZone":"UTC"})],
        next_page_token: None,
        next_sync_token: Some("catalog".into()),
    };
    store.calendar_stage_page_chunk(&lease, &page, 0).unwrap();
    store.calendar_seal_page(&lease, &page).unwrap();
    while store.calendar_catalog_finalize_chunk(&lease).unwrap() > 0 {}
    store.calendar_publish_sync(&lease).unwrap();
    publish(&mut store, &account, vec![event()], CalendarSyncMode::Full);
    (dir, store, account)
}
fn event() -> Value {
    json!({"id":"event1","etag":"v1","summary":"Before","description":"Retained description","organizer":{"email":CAL,"self":true},"start":{"dateTime":"2026-09-12T09:00:00Z"},"end":{"dateTime":"2026-09-12T10:00:00Z"},"conferenceData":{"signature":"retained","entryPoints":[{"entryPointType":"video","uri":"https://meet.example.test/room"}]},"attachments":[{"fileUrl":"https://drive.example.test/item"}],"extendedProperties":{"private":{"untouched":"yes"}}})
}
fn publish(store: &mut Store, account: &str, items: Vec<Value>, mode: CalendarSyncMode) {
    let lease = store.calendar_sync_begin(account, CAL, mode).unwrap();
    let page = CalendarPage {
        input_cursor: None,
        items,
        next_page_token: None,
        next_sync_token: Some("events".into()),
    };
    store.calendar_stage_page_chunk(&lease, &page, 0).unwrap();
    store.calendar_seal_page(&lease, &page).unwrap();
    store.calendar_publish_sync(&lease).unwrap();
    while store.calendar_finalize_chunk(&lease).unwrap() > 0 {}
}
fn request(kind: &str) -> Value {
    json!({"request_id":kind,"kind":kind,"calendar_id":CAL,"event_id":"event1","expected_etag":"v1"})
}
fn prepare(
    store: &mut Store,
    account: &str,
    request: Value,
    source: Option<Value>,
) -> CalendarWriteLease {
    let id = request["request_id"].as_str().unwrap();
    store.calendar_live_prepare(account, &request).unwrap();
    let epoch = store.calendar_checkpoint(account, CAL).unwrap()["permission_epoch"]
        .as_i64()
        .unwrap();
    store
        .calendar_live_claim(account, id, 0, 1, epoch, source)
        .unwrap()
}
#[test]
fn changed_fields_preserve_unedited_provider_data_and_replay_does_not_dispatch() {
    let (_dir, mut store, account) = setup();
    let mut req = request("edit");
    req["fields"] = json!({"title":"After"});
    let lease = prepare(&mut store, &account, req.clone(), Some(event()));
    assert_eq!(lease.dispatch["body"], json!({"summary":"After"}));
    assert_eq!(
        store.calendar_live_prepare(&account, &req).unwrap()["state"],
        "in_flight"
    );
    let mut source = event();
    source["summary"] = "After".into();
    source["etag"] = "v2".into();
    store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Accepted(source.clone()))
        .unwrap();
    assert_eq!(
        store.calendar_live_publish(&lease).unwrap()["state"],
        "acknowledged"
    );
    assert_eq!(
        store
            .stored_calendar_event(&account, CAL, "event1")
            .unwrap()
            .0
            .source,
        source
    );
    req["fields"]["title"] = "Another".into();
    assert_eq!(
        store
            .calendar_live_prepare(&account, &req)
            .unwrap_err()
            .code,
        "request_id_conflict"
    );
}
#[test]
fn proven_undispatched_cooldown_preserves_ready_intent_and_rejects_an_old_attempt() {
    let (dir, mut store, account) = setup();
    let mut request = request("edit");
    request["fields"] = json!({"title":"After"});
    let first = prepare(&mut store, &account, request.clone(), Some(event()));
    let returned = store
        .calendar_live_record_outcome(
            &first,
            CalendarWriteOutcome::UndispatchedCooldown {
                retry_after_secs: 60,
            },
        )
        .unwrap();
    assert_eq!(returned["state"], "ready");
    let retained = store.calendar_live_operation(&account, "edit").unwrap();
    assert_eq!(retained.intent["request"], request);
    assert_eq!(retained.lease.unwrap().dispatch, first.dispatch);
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    assert_eq!(
        store.calendar_live_status(&account, "edit").unwrap(),
        returned
    );
    // Only another explicit invocation may start the retained unsent plan.
    let second = store.calendar_live_start(&account, "edit", 1).unwrap();
    assert_eq!(second.attempt, first.attempt + 1);
    assert_eq!(second.dispatch, first.dispatch);
    assert_eq!(
        store
            .calendar_live_record_outcome(
                &first,
                CalendarWriteOutcome::UndispatchedCooldown {
                    retry_after_secs: 60
                },
            )
            .unwrap_err()
            .code,
        "stale_revision"
    );
    let mut accepted = event();
    accepted["summary"] = "After".into();
    accepted["etag"] = "v2".into();
    store
        .calendar_live_record_outcome(&second, CalendarWriteOutcome::Accepted(accepted))
        .unwrap();
    assert_eq!(
        store
            .calendar_live_record_outcome(
                &second,
                CalendarWriteOutcome::UndispatchedCooldown {
                    retry_after_secs: 60
                },
            )
            .unwrap()["state"],
        "accepted",
        "a completed provider receipt cannot be made retryable"
    );
}
#[test]
fn stable_create_id_and_delete_are_journaled_without_fake_acknowledgement() {
    let (_dir, mut store, account) = setup();
    let req = json!({"request_id":"new","kind":"create","calendar_id":CAL,"fields":{"title":"Created","start":"2026-09-12","end":"2026-09-13","all_day":true,"time_zone":"UTC","attendees":[],"conference_url":""}});
    let lease = prepare(&mut store, &account, req, None);
    let id = lease.dispatch["target"]["event"].as_str().unwrap();
    assert!(id
        .bytes()
        .all(|v| v.is_ascii_digit() || (b'a'..=b'v').contains(&v)));
    let mut created = lease.dispatch["body"].clone();
    created["id"] = id.into();
    created["etag"] = "created-v1".into();
    store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Accepted(created))
        .unwrap();
    store.calendar_live_publish(&lease).unwrap();
    assert_eq!(
        store
            .calendar_write_ack(&account, &json!({"request_id":"new"}))
            .unwrap_err()
            .code,
        "not_found"
    );
    let lease = prepare(&mut store, &account, request("delete"), Some(event()));
    store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Deleted)
        .unwrap();
    store.calendar_live_publish(&lease).unwrap();
    assert!(store
        .stored_calendar_event(&account, CAL, "event1")
        .is_err());
}
#[test]
fn self_rsvp_is_a_minimal_patch_and_reader_cannot_submit() {
    let (_dir, mut store, account) = setup();
    let mut source = event();
    source["attendeesOmitted"] = true.into();
    source["attendees"] = json!([{"email":CAL.to_uppercase(),"self":true,"responseStatus":"needsAction","comment":"retained"}]);
    let mut req = request("rsvp");
    req["response"] = "accepted".into();
    let lease = prepare(&mut store, &account, req, Some(source.clone()));
    assert_eq!(
        lease.dispatch["body"],
        json!({"attendeesOmitted":true,"attendees":[{"email":CAL.to_uppercase(),"responseStatus":"accepted"}]})
    );
    let event = fm_calendar::normalize(&account, CAL, "UTC", &source).unwrap();
    assert!(initial_event(&event, Access::Reader, "rsvp").is_err());
    assert!(initial_event(&event, Access::Owner, "edit").is_err());
}
#[test]
fn conflict_and_crash_preserve_intent_and_reconcile_without_retry() {
    let (dir, mut store, account) = setup();
    let mut req = request("edit");
    req["fields"] = json!({"title":"Wanted"});
    let lease = prepare(&mut store, &account, req, Some(event()));
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    store.calendar_live_recover().unwrap();
    assert_eq!(
        store.calendar_live_status(&account, "edit").unwrap()["state"],
        "unknown"
    );
    let mut source = event();
    source["summary"] = "Wanted".into();
    source["etag"] = "v2".into();
    assert_eq!(
        store
            .calendar_live_reconcile_source(&lease, source)
            .unwrap()["state"],
        "accepted"
    );
    store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Unknown)
        .unwrap();
    assert_eq!(
        store.calendar_live_status(&account, "edit").unwrap()["state"],
        "accepted"
    );
    let mut req = request("edit");
    req["request_id"] = "conflict".into();
    req["fields"] = json!({"title":"Retain this"});
    let lease = prepare(&mut store, &account, req, Some(event()));
    store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Conflict)
        .unwrap();
    let op = store.calendar_live_operation(&account, "conflict").unwrap();
    assert_eq!(op.state, "conflict");
    assert_eq!(op.intent["request"]["fields"]["title"], "Retain this");
}
#[test]
fn successful_receipt_survives_newer_sync_and_permission_observation() {
    let (_dir, mut store, account) = setup();
    let mut req = request("edit");
    req["fields"] = json!({"title":"Mine"});
    let lease = prepare(&mut store, &account, req, Some(event()));
    let mut newer = event();
    newer["summary"] = "Newer remote edit".into();
    newer["etag"] = "v3".into();
    publish(
        &mut store,
        &account,
        vec![newer.clone()],
        CalendarSyncMode::Delta,
    );
    let mut accepted = event();
    accepted["summary"] = "Mine".into();
    accepted["etag"] = "v2".into();
    store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Accepted(accepted))
        .unwrap();
    store.calendar_live_publish(&lease).unwrap();
    assert_eq!(
        store
            .stored_calendar_event(&account, CAL, "event1")
            .unwrap()
            .0
            .source,
        newer
    );
    let mut req = request("delete");
    req["expected_etag"] = "v3".into();
    let lease = prepare(&mut store, &account, req, Some(newer));
    let epoch = store.calendar_checkpoint(&account, CAL).unwrap()["permission_epoch"]
        .as_i64()
        .unwrap();
    store
        .calendar_observe_permission(&account, CAL, 1, epoch, Access::Reader, "UTC")
        .unwrap();
    store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Deleted)
        .unwrap();
    assert_eq!(
        store.calendar_live_publish(&lease).unwrap()["state"],
        "acknowledged"
    );
}
#[test]
fn no_conference_changes_no_recurring_writes_and_no_stale_claims() {
    let (_dir, mut store, account) = setup();
    let original = event();
    let normalized = fm_calendar::normalize(&account, CAL, "UTC", &original).unwrap();
    assert!(field_patch(&json!({"conference_url":""}), Some(&normalized), "UTC").is_err());
    assert_eq!(
        field_patch(
            &json!({"title":"Before","conference_url":"https://meet.example.test/room"}),
            Some(&normalized),
            "UTC"
        )
        .unwrap(),
        json!({})
    );
    let mut req = request("edit");
    req["fields"] = json!({"title":"Wanted"});
    store.calendar_live_prepare(&account, &req).unwrap();
    let mut newer = event();
    newer["etag"] = "v2".into();
    let epoch = store.calendar_checkpoint(&account, CAL).unwrap()["permission_epoch"]
        .as_i64()
        .unwrap();
    assert_eq!(
        store
            .calendar_live_claim(&account, "edit", 0, 1, epoch, Some(newer))
            .unwrap_err()
            .code,
        "stale_revision"
    );
    assert_eq!(
        store
            .calendar_live_operation(&account, "edit")
            .unwrap()
            .state,
        "prepared"
    );
    let mut recurring = event();
    recurring["recurrence"] = json!(["RRULE:FREQ=DAILY"]);
    let recurring = fm_calendar::normalize(&account, CAL, "UTC", &recurring).unwrap();
    assert!(initial_event(&recurring, Access::Owner, "edit").is_err());
}

#[test]
fn accepted_write_expires_older_staging_without_losing_source_or_receipt() {
    let (_dir, mut store, account) = setup();
    let mut req = request("edit");
    req["fields"] = json!({"title":"After"});
    let lease = prepare(&mut store, &account, req, Some(event()));
    let sync = store
        .calendar_sync_begin(&account, CAL, CalendarSyncMode::Delta)
        .unwrap();
    let page = CalendarPage {
        input_cursor: None,
        items: vec![event()],
        next_page_token: None,
        next_sync_token: Some("before-write".into()),
    };
    store.calendar_stage_page_chunk(&sync, &page, 0).unwrap();
    let mut source = event();
    source["summary"] = "After".into();
    source["etag"] = "v2".into();
    store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Accepted(source.clone()))
        .unwrap();
    let runs = store.calendar_live_expire_staging(&lease).unwrap();
    assert_eq!(runs.len(), 1);
    for run in runs {
        while store.calendar_finalize_chunk(&run).unwrap() > 0 {}
    }
    assert!(store.calendar_seal_page(&sync, &page).is_err());
    store.calendar_live_publish(&lease).unwrap();
    assert_eq!(
        store
            .stored_calendar_event(&account, CAL, "event1")
            .unwrap()
            .0
            .source,
        source
    );
    let revision = store.calendar_live_status(&account, "edit").unwrap()["revision"].clone();
    assert_eq!(store.calendar_live_recover().unwrap(), 0);
    assert_eq!(
        store.calendar_live_status(&account, "edit").unwrap()["revision"],
        revision
    );
    assert!(same_time(
        &json!({"dateTime":"2026-09-12T10:00:00+01:00"}),
        &json!({"dateTime":"2026-09-12T09:00:00Z"})
    ));
}
