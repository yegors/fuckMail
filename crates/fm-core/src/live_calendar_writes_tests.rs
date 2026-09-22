use super::*;
use crate::live_calendar::{CalendarPage, CalendarSyncMode};
use std::collections::BTreeSet;

#[test]
fn editor_options_preserve_omitted_fields_and_freeze_provider_intent() {
    let (dir, mut store, account) = setup();
    let mut original = event();
    original["reminders"] = json!({"useDefault":false,"overrides":[{"method":"email","minutes":60},{"method":"popup","minutes":15}]});
    let normalized = fm_calendar::normalize(&account, CAL, "UTC", &original).unwrap();
    let untouched = field_patch(
        "title-only",
        &json!({"title":"Changed"}),
        Some(&normalized),
        "UTC",
        CAL,
        2,
    )
    .unwrap();
    for key in [
        "reminders",
        "transparency",
        "conferenceData",
        "start",
        "end",
    ] {
        assert!(untouched.get(key).is_none());
    }
    let mut req = request("edit");
    req["fields"] = json!({"reminder_minutes":30,"availability":"free","conference":"none"});
    let lease = prepare(&mut store, &account, req.clone(), Some(original.clone()));
    assert_eq!(
        lease.dispatch["body"],
        json!({"reminders":{"useDefault":false,"overrides":[{"method":"popup","minutes":30}]},"transparency":"transparent","conferenceData":null})
    );
    let document = store
        .calendar_live_operation(&account, "edit")
        .unwrap()
        .intent;
    let mut observed = original.clone();
    observed["reminders"] = lease.dispatch["body"]["reminders"].clone();
    observed["transparency"] = "transparent".into();
    observed.as_object_mut().unwrap().remove("conferenceData");
    let actual = fm_calendar::normalize(&account, CAL, "UTC", &observed).unwrap();
    assert!(matches_intent_v2(&actual, &document, &lease.dispatch));
    observed["reminders"]["overrides"][0]["minutes"] = 15.into();
    assert!(!matches_intent_v2(
        &fm_calendar::normalize(&account, CAL, "UTC", &observed).unwrap(),
        &document,
        &lease.dispatch
    ));
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    store.calendar_live_prepare(&account, &req).unwrap();
    assert_eq!(
        store
            .calendar_live_operation(&account, "edit")
            .unwrap()
            .lease
            .unwrap()
            .dispatch,
        lease.dispatch
    );
    for bad in [
        json!({"reminder_minutes":-1}),
        json!({"reminder_minutes":40321}),
        json!({"reminder_minutes":1.5}),
        json!({"availability":"maybe"}),
        json!({"conference":"https://example.test"}),
    ] {
        assert!(field_patch("invalid", &bad, Some(&normalized), "UTC", CAL, 2).is_err());
    }
}

#[test]
fn meeting_creation_matches_only_the_same_successful_request() {
    let (_dir, mut store, account) = setup();
    let mut req = request("edit");
    req["request_id"] = "new-meet".into();
    req["fields"] = json!({"conference":"google_meet"});
    let lease = prepare(&mut store, &account, req, Some(event()));
    let document = store
        .calendar_live_operation(&account, "new-meet")
        .unwrap()
        .intent;
    let conference = &lease.dispatch["body"]["conferenceData"];
    assert_eq!(conference["createRequest"]["requestId"], "new-meet");
    assert!(conference.get("entryPoints").is_none());
    let mut source = event();
    source["conferenceData"] = conference.clone();
    for status in ["pending", "failure", "success"] {
        source["conferenceData"]["createRequest"]["status"]["statusCode"] = status.into();
        let actual = fm_calendar::normalize(&account, CAL, "UTC", &source).unwrap();
        assert_eq!(
            matches_intent_v2(&actual, &document, &lease.dispatch),
            status == "success"
        );
    }
    source["conferenceData"]["createRequest"]["requestId"] = "another-request".into();
    assert!(!matches_intent_v2(
        &fm_calendar::normalize(&account, CAL, "UTC", &source).unwrap(),
        &document,
        &lease.dispatch
    ));
}

#[test]
fn explicit_time_conversion_clears_the_other_kind_and_preserves_multiday_dates() {
    let mut source = event();
    for (all_day, start, end) in [
        (true, "2026-09-12", "2026-09-15"),
        (false, "2026-09-12T09:00:00Z", "2026-09-14T10:00:00Z"),
    ] {
        let event = fm_calendar::normalize("a", CAL, "UTC", &source).unwrap();
        let mut fields = json!({"all_day":all_day,"start":start,"end":end,"time_zone":"UTC"});
        assert!(field_patch("conversion", &fields, Some(&event), "UTC", CAL, 2).is_err());
        fields["convert_time_kind"] = true.into();
        let patch = field_patch("conversion", &fields, Some(&event), "UTC", CAL, 2).unwrap();
        assert!(patch["start"]
            .get(if all_day { "dateTime" } else { "date" })
            .is_some_and(Value::is_null));
        for (key, value) in patch.as_object().unwrap() {
            source[key] = value.clone();
        }
        crate::calendar_editor_options::clear_endpoint_nulls(&mut source);
        assert!(fm_calendar::normalize("a", CAL, "UTC", &source).is_ok());
        assert_eq!(
            source["end"][if all_day { "date" } else { "dateTime" }],
            end
        );
    }
}
pub(super) const CAL: &str = "human@example.test";
pub(super) fn setup() -> (tempfile::TempDir, Store, String) {
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
pub(super) fn event() -> Value {
    json!({"id":"event1","etag":"v1","summary":"Before","description":"Retained description","organizer":{"email":CAL,"self":true},"start":{"dateTime":"2026-09-12T09:00:00Z"},"end":{"dateTime":"2026-09-12T10:00:00Z"},"conferenceData":{"conferenceSolution":{"key":{"type":"hangoutsMeet"}},"signature":"retained","entryPoints":[{"entryPointType":"video","uri":"https://meet.example.test/room"}]},"attachments":[{"fileUrl":"https://drive.example.test/item"}],"extendedProperties":{"private":{"untouched":"yes"}}})
}
pub(super) fn publish(store: &mut Store, account: &str, items: Vec<Value>, mode: CalendarSyncMode) {
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
pub(super) fn request(kind: &str) -> Value {
    json!({"request_id":kind,"kind":kind,"calendar_id":CAL,"event_id":"event1","expected_etag":"v1"})
}
pub(super) fn prepare(
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
fn status_projects_small_fields_without_parsing_retained_dispatch_or_accepted_source() {
    let (_dir, mut store, account) = setup();
    let mut req = request("edit");
    req["fields"] = json!({"title":"PRIVATE AUTHORED TITLE"});
    let lease = prepare(&mut store, &account, req, Some(event()));
    store.db.execute("UPDATE calendar_write_operations SET state='accepted',dispatch_json='deliberately-not-json',outcome_json='deliberately-not-json' WHERE account=?1 AND request_id='edit'",[&account]).unwrap();
    let result = store.calendar_live_status(&account, "edit").unwrap();
    assert_eq!(result["target"], lease.dispatch["target"]);
    assert_eq!(
        result["outcome"],
        json!({"kind":"accepted","evidence_origin":"provider_response","notification_delivery":"unverified"})
    );
    assert_eq!(result["kind"], "edit");
    assert!(result.to_string().len() < 512);
    assert!(!result.to_string().contains("PRIVATE"));
    assert!(
        store.calendar_live_operation(&account, "edit").is_err(),
        "the retained worker source really is unreadable in this projection fixture"
    );
    assert_eq!(
        store
            .calendar_live_status("foreign", "edit")
            .unwrap_err()
            .code,
        "not_found"
    );
}

#[test]
fn live_operation_history_pages_all_states_and_sanitizes_outcomes_without_sources() {
    let (_dir, mut store, account) = setup();
    let states = [
        "prepared",
        "ready",
        "in_flight",
        "unknown",
        "conflict",
        "rejected",
        "abandoned",
        "accepted",
        "ready",
        "acknowledged",
        "acknowledged",
    ];
    for (index, state) in states.iter().enumerate() {
        let id = format!("op-{index}");
        let request = json!({"request_id":id,"kind":"create","calendar_id":CAL,"fields":{"title":"PRIVATE TITLE","start":"2026-09-12","end":"2026-09-13","all_day":true,"time_zone":"UTC"}});
        store.calendar_live_prepare(&account, &request).unwrap();
        let outcome = match *state {
            "ready" => {
                json!({"kind":"undispatched_cooldown","retry_after_secs":60,"source":"PRIVATE BODY"})
            }
            "rejected" => {
                json!({"kind":"rejected","code":"permission_denied","message":"PRIVATE PROVIDER ERROR"})
            }
            "abandoned" => json!({"kind":"rejected","code":"PRIVATE PROVIDER ERROR"}),
            _ => json!({"kind":"accepted","source":{"description":"PRIVATE BODY"}}),
        };
        let receipt =
            (*state == "acknowledged").then(|| json!({"cache_published":false}).to_string());
        store.db.execute("UPDATE calendar_write_operations SET state=?3,operation_revision=?4,dispatch_json='deliberately-not-json',outcome_json=?5,result_json=?6 WHERE account=?1 AND request_id=?2",params![account,id,state,index as i64,serde_json::to_string(&outcome).unwrap(),receipt]).unwrap();
    }
    store.db.execute("INSERT INTO calendar_write_operations(account,request_id,fingerprint,intent_json) VALUES(?1,'local-only','fixture','{\"public_fingerprint\":\"fixture\"}')",[&account]).unwrap();
    let mut cursor = Value::Null;
    let mut all = Vec::new();
    loop {
        let page = store
            .calendar_operations(&account, &json!({"limit":3,"cursor":cursor}))
            .unwrap();
        assert!(page["operations"].as_array().unwrap().len() <= 3);
        assert!(page.to_string().len() < 4096);
        assert!(!page.to_string().contains("PRIVATE"));
        all.extend(page["operations"].as_array().unwrap().iter().cloned());
        cursor = page["continuation"].clone();
        if cursor.is_null() {
            break;
        }
    }
    assert_eq!(
        all.iter()
            .map(|row| row["request_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "op-8", "op-7", "op-6", "op-5", "op-4", "op-3", "op-2", "op-1", "op-0", "op-10", "op-9"
        ]
    );
    assert!(all
        .iter()
        .all(|row| row["local_only"] == false && row["target"]["account"] == account));
    assert_eq!(
        all[0]["outcome"],
        json!({"kind":"undispatched_cooldown","retry_after_secs":60})
    );
    assert_eq!(all[2]["outcome"], json!({"kind":"rejected"}));
    assert_eq!(
        all[3]["outcome"],
        json!({"kind":"rejected","code":"permission_denied"})
    );
    assert_eq!(
        all[9]["outcome"],
        json!({"kind":"accepted","cache_published":false,"evidence_origin":"provider_response","notification_delivery":"unverified"})
    );
    assert_eq!(
        store.calendar_operations("foreign", &json!({})).unwrap()["operations"],
        json!([])
    );
    for invalid in [
        json!({"limit":0}),
        json!({"limit":101}),
        json!({"cursor":{"acknowledged":false,"before_id":0}}),
    ] {
        assert_eq!(
            store
                .calendar_operations(&account, &invalid)
                .unwrap_err()
                .code,
            "invalid_request"
        );
    }
}

#[test]
fn resume_loads_only_bounded_immutable_unsent_request_and_never_changes_state() {
    let (dir, mut store, account) = setup();
    let mut request = request("edit");
    request["fields"] = json!({"title":"Retained edit"});
    store.calendar_live_prepare(&account, &request).unwrap();
    assert_eq!(
        store.calendar_live_request(&account, "edit").unwrap(),
        request
    );
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    assert_eq!(
        store.calendar_live_request(&account, "edit").unwrap(),
        request
    );
    assert_eq!(
        store.calendar_live_status(&account, "edit").unwrap()["state"],
        "prepared"
    );
    let epoch = store.calendar_checkpoint(&account, CAL).unwrap()["permission_epoch"]
        .as_i64()
        .unwrap();
    let before = store
        .calendar_live_stage(&account, "edit", 0, 1, epoch, Some(event()))
        .unwrap();
    store.db.execute("UPDATE calendar_write_operations SET dispatch_json='deliberately-not-json' WHERE account=?1 AND request_id='edit'",[&account]).unwrap();
    assert_eq!(
        store.calendar_live_request(&account, "edit").unwrap(),
        request
    );
    assert_eq!(
        store.calendar_live_status(&account, "edit").unwrap(),
        before
    );
    drop(store);
    let store = Store::open_live(dir.path()).unwrap();
    assert_eq!(
        store.calendar_live_request(&account, "edit").unwrap(),
        request
    );
    for state in [
        "in_flight",
        "unknown",
        "accepted",
        "acknowledged",
        "conflict",
        "rejected",
        "abandoned",
    ] {
        store.db.execute("UPDATE calendar_write_operations SET state=?2 WHERE account=?1 AND request_id='edit'",params![account,state]).unwrap();
        assert_eq!(
            store
                .calendar_live_request(&account, "edit")
                .unwrap_err()
                .code,
            "calendar_operation_pending"
        );
    }
    let oversized =
        json!({"request":{"request_id":"edit","fields":{"description":"x".repeat(262144)}}});
    store.db.execute("UPDATE calendar_write_operations SET state='ready',intent_json=?2 WHERE account=?1 AND request_id='edit'",params![account,oversized.to_string()]).unwrap();
    assert_eq!(
        store
            .calendar_live_request(&account, "edit")
            .unwrap_err()
            .code,
        "invalid_request"
    );
    assert_eq!(
        store
            .calendar_live_request("foreign", "edit")
            .unwrap_err()
            .code,
        "not_found"
    );
}

#[test]
fn live_action_projection_matches_supported_writes_roles_and_private_sources() {
    for role in [
        Access::Owner,
        Access::Writer,
        Access::Reader,
        Access::WriterWithoutPrivateAccess,
    ] {
        let (_dir, mut store, account) = setup();
        store.preferences_save(&json!({"request_id":"display-zone","expected_revision":0,"preferences":{"calendar_display_time_zone":"UTC"}})).unwrap();
        let epoch = store.calendar_checkpoint(&account, CAL).unwrap()["permission_epoch"]
            .as_i64()
            .unwrap();
        store
            .calendar_observe_permission(&account, CAL, 1, epoch, role, "UTC")
            .unwrap();
        let mut items = Vec::new();
        for (id, extra) in [
            ("ordinary", json!({})),
            (
                "invitation",
                json!({"attendees":[{"self":true,"email":CAL,"responseStatus":"needsAction"}]}),
            ),
            (
                "partial-self",
                json!({"attendeesOmitted":true,"attendees":[{"self":true,"email":CAL.to_uppercase(),"responseStatus":"tentative"}]}),
            ),
            ("partial-missing", json!({"attendeesOmitted":true})),
            (
                "other-self",
                json!({"attendees":[{"self":true,"email":"other@example.test"}]}),
            ),
            (
                "duplicate-self",
                json!({"attendees":[{"self":true,"email":CAL},{"self":true,"email":CAL}]}),
            ),
            ("locked", json!({"locked":true})),
            ("special", json!({"eventType":"focusTime"})),
            (
                "recurring",
                json!({"recurrence":["RRULE:FREQ=DAILY;COUNT=2"]}),
            ),
            ("no-etag", json!({"etag":""})),
            (
                "foreign-organizer",
                json!({"organizer":{"email":"other@example.test","self":false}}),
            ),
            (
                "private",
                json!({"visibility":"private","summary":"PRIVATE CAPABILITY TITLE"}),
            ),
        ] {
            let mut source = event();
            source["id"] = id.into();
            source["summary"] = id.into();
            for (key, value) in extra.as_object().unwrap() {
                source[key] = value.clone();
            }
            items.push(source);
        }
        publish(&mut store, &account, items, CalendarSyncMode::Full);
        let writable = role != Access::Reader;
        let sources = store.calendar_sources().unwrap();
        assert_eq!(sources["calendars"][0]["can_create"], writable);
        let result = store
            .calendar_combined(&json!({"from":"2026-09-12","to":"2026-09-14"}))
            .unwrap();
        let rows = result["events"].as_array().unwrap();
        for row in rows {
            if row["busy_only"] == true {
                assert_eq!(row["can_edit"], false);
                assert_eq!(row["can_delete"], false);
                assert_eq!(row["can_rsvp"], false);
                assert!(row.get("self_response").is_none());
                continue;
            }
            let id = row["event_id"].as_str().unwrap();
            let edit = writable
                && matches!(
                    id,
                    "ordinary"
                        | "private"
                        | "invitation"
                        | "partial-self"
                        | "partial-missing"
                        | "other-self"
                        | "duplicate-self"
                );
            let rsvp = writable && matches!(id, "invitation" | "partial-self");
            assert_eq!(row["can_edit"], edit, "edit role={role:?} id={id}");
            assert_eq!(row["can_delete"], edit, "delete role={role:?} id={id}");
            assert_eq!(row["can_rsvp"], rsvp, "RSVP role={role:?} id={id}");
        }
        if matches!(role, Access::Reader | Access::WriterWithoutPrivateAccess) {
            assert!(!result.to_string().contains("PRIVATE CAPABILITY TITLE"));
            assert!(rows.iter().any(|row| row["busy_only"] == true));
        }
        assert!(rows
            .iter()
            .any(|row| row["event_id"] == "partial-self" && row["self_response"] == "tentative"));
        store.db.execute("UPDATE calendar_sync_state SET retained_errors=1 WHERE account=?1 AND calendar_id=?2",params![account,CAL]).unwrap();
        assert_eq!(
            store.calendar_sources().unwrap()["calendars"][0]["can_create"],
            false
        );
        let incomplete = store
            .calendar_combined(&json!({"from":"2026-09-12","to":"2026-09-14"}))
            .unwrap();
        assert!(incomplete["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["can_edit"] == false
                && row["can_delete"] == false
                && row["can_rsvp"] == false));
    }
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
    assert!(initial_event(&event, Access::Reader, "rsvp", 2).is_err());
    assert!(initial_event(&event, Access::Owner, "edit", 2).is_ok());
    assert!(live_guest_actions(&event, Access::Owner).is_err());
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
        "reconciled"
    );
    store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Unknown)
        .unwrap();
    assert_eq!(
        store.calendar_live_status(&account, "edit").unwrap()["state"],
        "reconciled"
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
    assert!(field_patch(
        "test-options",
        &json!({"conference_url":""}),
        Some(&normalized),
        "UTC",
        CAL,
        2
    )
    .is_err());
    assert_eq!(
        field_patch(
            "test-options",
            &json!({"title":"Before","conference_url":"https://meet.example.test/room"}),
            Some(&normalized),
            "UTC",
            CAL,
            2
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
    assert!(initial_event(&recurring, Access::Owner, "edit", 2).is_err());
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
