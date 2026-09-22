use super::tests::{event, prepare, publish, request, setup, CAL};
use super::*;
use crate::live_calendar::{CalendarPage, CalendarSyncMode};

fn publish_empty_catalog(store: &mut Store, account: &str) {
    let run = store
        .calendar_catalog_begin(account, CalendarSyncMode::Full)
        .unwrap();
    let page = CalendarPage {
        input_cursor: None,
        items: vec![],
        next_page_token: None,
        next_sync_token: Some("empty-catalog".into()),
    };
    store.calendar_stage_page_chunk(&run, &page, 0).unwrap();
    store.calendar_seal_page(&run, &page).unwrap();
    while store.calendar_catalog_finalize_chunk(&run).unwrap() > 0 {}
    store.calendar_publish_sync(&run).unwrap();
}

fn invited() -> Value {
    let mut source = event();
    source["eventLabelId"] = "provider-label-17".into();
    source["colorId"] = "3".into();
    source["attendees"] = json!([
        {"email":CAL,"organizer":true,"self":true,"responseStatus":"accepted"},
        {"email":"guest@example.test","displayName":"A guest","responseStatus":"accepted","optional":true,"comment":"Retained","additionalGuests":1,"id":"person-id"},
        {"email":"room@example.test","resource":true,"responseStatus":"tentative"}
    ]);
    source
}
fn patch(source: &Value, fields: Value) -> Result<Value> {
    let event = fm_calendar::normalize("account", CAL, "UTC", source).unwrap();
    field_patch("test-options", &fields, Some(&event), "UTC", CAL, 2)
}

fn writable(person: &Value) -> Value {
    let mut result = person.clone();
    for key in ["id", "self", "organizer", "asyncOperation"] {
        result.as_object_mut().unwrap().remove(key);
    }
    result
}

#[test]
fn invitation_read_publication_uses_current_fence_and_preserves_dispatch() {
    for change in [
        "none",
        "permission",
        "source",
        "connection",
        "membership",
        "limited_public",
    ] {
        let (dir, mut store, account) = setup();
        let mut req = request("edit");
        req["fields"] = json!({"title":"Observed"});
        let lease = prepare(&mut store, &account, req, Some(event()));
        let dispatch_before: String = store.db.query_row("SELECT dispatch_json FROM calendar_write_operations WHERE account=?1 AND request_id='edit'", [&account], |row| row.get(0)).unwrap();
        store
            .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Unknown)
            .unwrap();
        let role = if change == "limited_public" {
            Access::WriterWithoutPrivateAccess
        } else {
            Access::Owner
        };
        let permission = store.calendar_sync_scope(&account, CAL).unwrap()["permission_epoch"]
            .as_i64()
            .unwrap();
        store
            .calendar_observe_permission(&account, CAL, 1, permission, role, "UTC")
            .unwrap();
        let fence = store.calendar_live_reconcile_begin(&lease, 1).unwrap();
        assert_ne!(fence.permission_epoch, lease.permission_epoch);
        let mut observed = event();
        observed["summary"] = "Observed".into();
        observed["etag"] = "observed-v2".into();
        let mut expected = observed.clone();
        match change {
            "permission" => {
                store
                    .calendar_observe_permission(
                        &account,
                        CAL,
                        1,
                        fence.permission_epoch,
                        Access::Owner,
                        "UTC",
                    )
                    .unwrap();
                expected = event();
            }
            "source" => {
                expected = event();
                expected["summary"] = "Newer source".into();
                expected["etag"] = "newer-v3".into();
                publish(
                    &mut store,
                    &account,
                    vec![expected.clone()],
                    CalendarSyncMode::Full,
                );
            }
            "connection" => {
                store
                    .set_google_connection_state(
                        &account,
                        crate::live_profile::ConnectionState::Connected,
                    )
                    .unwrap();
                expected = event();
            }
            "membership" => {
                publish_empty_catalog(&mut store, &account);
                let scope = store.calendar_sync_scope(&account, CAL).unwrap();
                assert_eq!(scope["membership_denied"], true);
                assert_eq!(scope["permission_epoch"], fence.permission_epoch);
                assert_eq!(scope["revision"], fence.source_revision);
                expected = event();
            }
            _ => {}
        }
        let receipt = store
            .calendar_live_reconcile_read(&lease, &fence, observed)
            .unwrap();
        assert_eq!(receipt["state"], "reconciled");
        // The separate read authority survives restart without mutating the
        // frozen provider request or allowing an old cache/permission epoch.
        drop(store);
        let mut store = Store::open_live(dir.path()).unwrap();
        let receipt = store.calendar_live_publish(&lease).unwrap();
        assert_eq!(
            receipt["outcome"]["cache_published"],
            matches!(change, "none" | "limited_public")
        );
        let raw: String = store.db.query_row("SELECT json FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id='event1'", [&account, CAL], |row| row.get(0)).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&raw).unwrap(),
            expected,
            "{change}"
        );
        let dispatch_after: String = store.db.query_row("SELECT dispatch_json FROM calendar_write_operations WHERE account=?1 AND request_id='edit'", [&account], |row| row.get(0)).unwrap();
        assert_eq!(dispatch_before, dispatch_after);
        assert_eq!(store.calendar_live_publish(&lease).unwrap(), receipt);
    }
}

#[test]
fn catalog_membership_loss_blocks_ack_publication_and_staging_expiry() {
    for reconcile in [false, true] {
        let (_dir, mut store, account) = setup();
        let mut req = request("edit");
        req["fields"] = json!({"title":"After"});
        let lease = prepare(&mut store, &account, req, Some(event()));
        let fence = store.calendar_live_reconcile_begin(&lease, 1).unwrap();
        let pending = store
            .calendar_sync_begin(&account, CAL, CalendarSyncMode::Full)
            .unwrap();
        let mut observed = event();
        observed["summary"] = "After".into();
        observed["etag"] = "observed-v2".into();
        if reconcile {
            store
                .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Unknown)
                .unwrap();
            store
                .calendar_live_reconcile_read(&lease, &fence, observed)
                .unwrap();
        } else {
            store
                .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Accepted(observed))
                .unwrap();
        }
        publish_empty_catalog(&mut store, &account);
        let scope = store.calendar_sync_scope(&account, CAL).unwrap();
        assert_eq!(scope["membership_denied"], true);
        assert_eq!(scope["permission_epoch"], fence.permission_epoch);
        assert_eq!(scope["revision"], fence.source_revision);
        assert!(store
            .calendar_live_expire_staging(&lease)
            .unwrap()
            .is_empty());
        let pending_state: String = store
            .db
            .query_row(
                "SELECT state FROM calendar_sync_runs WHERE id=?1",
                [pending.run],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pending_state, "staging");
        let receipt = store.calendar_live_publish(&lease).unwrap();
        assert_eq!(
            receipt["state"],
            if reconcile {
                "reconciled"
            } else {
                "acknowledged"
            }
        );
        assert_eq!(receipt["outcome"]["cache_published"], false);
        let raw: String = store.db.query_row("SELECT json FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id='event1'", [&account, CAL], |row| row.get(0)).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&raw).unwrap(), event());
    }
}

#[test]
fn invitation_abandoned_v2_attempt_retains_only_its_exact_late_acceptance() {
    for legacy in [false, true] {
        for kind in ["edit", "delete"] {
            let (_dir, mut store, account) = setup();
            let mut req = request(kind);
            if kind == "edit" {
                req["fields"] = json!({"title":"Wanted"});
            }
            store.calendar_live_prepare(&account, &req).unwrap();
            if legacy {
                store.db.execute("UPDATE calendar_write_operations SET intent_json=json_remove(intent_json,'$.version') WHERE account=?1",[&account]).unwrap();
            }
            let epoch = store.calendar_checkpoint(&account, CAL).unwrap()["permission_epoch"]
                .as_i64()
                .unwrap();
            let lease = store
                .calendar_live_claim(&account, kind, 0, 1, epoch, Some(event()))
                .unwrap();
            // A check can finish before the original HTTP request: no matching
            // change is visible yet, while the exact attempt still owns a lease.
            let unknown = store
                .calendar_live_reconcile_source(&lease, event())
                .unwrap();
            assert_eq!(unknown["state"], "unknown");
            let abandoned = store
                .calendar_live_cancel(&account, kind, unknown["revision"].as_i64().unwrap())
                .unwrap();
            assert_eq!(abandoned["state"], "abandoned");
            assert_eq!(
                store
                    .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Unknown)
                    .unwrap(),
                abandoned
            );
            assert!(store.calendar_live_start(&account, kind, 1).is_err());
            let mut wrong = lease.clone();
            wrong.attempt += 1;
            assert_eq!(
                store
                    .calendar_live_record_outcome(&wrong, CalendarWriteOutcome::Unknown)
                    .unwrap_err()
                    .code,
                "stale_revision"
            );
            let mut accepted = event();
            accepted["summary"] = "Wanted".into();
            accepted["etag"] = "v2".into();
            let outcome = if kind == "delete" {
                CalendarWriteOutcome::Deleted
            } else {
                CalendarWriteOutcome::Accepted(accepted)
            };
            let result = store.calendar_live_record_outcome(&lease, outcome).unwrap();
            if legacy {
                assert_eq!(result, abandoned);
                continue;
            }
            assert_eq!(result["state"], "accepted");
            assert_eq!(result["outcome"]["evidence_origin"], "provider_response");
            assert_eq!(result["outcome"]["notification_delivery"], "unverified");
            // Strengthening evidence cannot overwrite a newer acquired source.
            let mut newer = event();
            newer["etag"] = "v10".into();
            newer["summary"] = "Newer event".into();
            publish(
                &mut store,
                &account,
                vec![newer.clone()],
                CalendarSyncMode::Full,
            );
            let result = store.calendar_live_publish(&lease).unwrap();
            assert_eq!(result["state"], "acknowledged");
            assert_eq!(result["outcome"]["cache_published"], false);
            assert_eq!(
                store
                    .stored_calendar_event(&account, CAL, "event1")
                    .unwrap()
                    .0
                    .source,
                newer
            );
        }
    }
    let (_dir, mut store, account) = setup();
    let mut req = request("edit");
    req["fields"] = json!({"title":"Never sent"});
    store.calendar_live_prepare(&account, &req).unwrap();
    let epoch = store.calendar_checkpoint(&account, CAL).unwrap()["permission_epoch"]
        .as_i64()
        .unwrap();
    let ready = store
        .calendar_live_stage(&account, "edit", 0, 1, epoch, Some(event()))
        .unwrap();
    let lease = store
        .calendar_live_operation(&account, "edit")
        .unwrap()
        .lease
        .unwrap();
    assert_eq!(lease.attempt, 0);
    let abandoned = store
        .calendar_live_cancel(&account, "edit", ready["revision"].as_i64().unwrap())
        .unwrap();
    assert_eq!(
        store
            .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Accepted(event()))
            .unwrap(),
        abandoned
    );
}

#[test]
fn invitation_create_reconciliation_accepts_optional_organizer_but_no_extra_guests() {
    for guests in [
        json!([]),
        json!(["guest@example.test", "room@example.test"]),
    ] {
        for organizer_row in [false, true] {
            let (_dir, mut store, account) = setup();
            let req = json!({"request_id":"create","kind":"create","calendar_id":CAL,"fields":{"title":"Created","start":"2026-09-12","end":"2026-09-13","all_day":true,"time_zone":"UTC","attendees":guests}});
            let lease = prepare(&mut store, &account, req, None);
            if guests.as_array().unwrap().is_empty() {
                assert!(lease.dispatch["body"].get("attendees").is_none());
            }
            let mut observed = lease.dispatch["body"].clone();
            observed["id"] = lease.dispatch["target"]["event"].clone();
            observed["etag"] = "created-v2".into();
            observed["organizer"] = json!({"email":CAL,"self":true});
            if observed.get("attendees").is_none() {
                observed["attendees"] = json!([]);
            }
            if organizer_row {
                observed["attendees"].as_array_mut().unwrap().push(
                    json!({"email":CAL,"organizer":true,"self":true,"responseStatus":"accepted"}),
                );
            }
            for extra in [
                json!({"email":"extra@example.test"}),
                json!({"email":"extra@example.test","self":true}),
                json!({"email":"extra@example.test","organizer":true}),
            ] {
                let mut bad = observed.clone();
                bad["attendees"].as_array_mut().unwrap().push(extra);
                assert_eq!(
                    store.calendar_live_reconcile_source(&lease, bad).unwrap()["state"],
                    "unknown"
                );
            }
            let mut incomplete = observed.clone();
            incomplete["attendeesOmitted"] = true.into();
            assert_eq!(
                store
                    .calendar_live_reconcile_source(&lease, incomplete)
                    .unwrap()["state"],
                "unknown"
            );
            if organizer_row {
                let mut wrong_copy = observed.clone();
                wrong_copy["organizer"]["self"] = false.into();
                assert_eq!(
                    store
                        .calendar_live_reconcile_source(&lease, wrong_copy)
                        .unwrap()["state"],
                    "unknown"
                );
            }
            let result = store
                .calendar_live_reconcile_source(&lease, observed)
                .unwrap();
            assert_eq!(result["state"], "reconciled");
            assert_eq!(result["outcome"]["kind"], "observed_state");
            assert!(store.calendar_live_start(&account, "create", 1).is_err());
        }
    }
}

#[test]
fn invitation_unchanged_explicit_fields_still_constrain_observed_state() {
    let (_dir, mut store, account) = setup();
    let mut original = invited();
    original["start"]["timeZone"] = "UTC".into();
    original["end"]["timeZone"] = "UTC".into();
    let mut req = request("edit");
    req["fields"] = json!({"title":"Before","description":"Retained description","location":"","start":"2026-09-12T09:00:00Z","end":"2026-09-12T10:00:00Z","all_day":false,"time_zone":"UTC","attendees":["guest@example.test","room@example.test"]});
    let lease = prepare(&mut store, &account, req, Some(original.clone()));
    assert_eq!(
        lease.dispatch["body"],
        json!({"eventLabelId":"provider-label-17"})
    );
    for (key, value) in [
        ("summary", json!("Changed elsewhere")),
        ("description", json!("Changed description")),
        ("location", json!("Changed room")),
        (
            "start",
            json!({"dateTime":"2026-09-12T09:30:00Z","timeZone":"UTC"}),
        ),
        ("attendeesOmitted", json!(true)),
    ] {
        let mut bad = original.clone();
        bad[key] = value;
        assert_eq!(
            store.calendar_live_reconcile_source(&lease, bad).unwrap()["state"],
            "unknown",
            "{key}"
        );
    }
    let mut changed = original.clone();
    changed["attendees"][1]["email"] = "different@example.test".into();
    assert_eq!(
        store
            .calendar_live_reconcile_source(&lease, changed)
            .unwrap()["state"],
        "unknown"
    );
    let result = store
        .calendar_live_reconcile_source(&lease, original)
        .unwrap();
    assert_eq!(result["state"], "reconciled");
    assert_eq!(result["outcome"]["notification_delivery"], "unverified");
}

#[test]
fn invitation_roster_replaces_only_membership_and_preserves_provider_objects() {
    let original = invited();
    let body = patch(
        &original,
        json!({"title":"Changed","attendees":["GUEST@example.test","new@example.test"]}),
    )
    .unwrap();
    assert_eq!(body["eventLabelId"], original["eventLabelId"]);
    assert!(body.get("colorId").is_none());
    assert!(body.get("conferenceData").is_none());
    assert!(body.get("attachments").is_none());
    let people = body["attendees"].as_array().unwrap();
    assert!(people.contains(&writable(&original["attendees"][0])));
    assert!(people.contains(&writable(&original["attendees"][1])));
    assert!(people
        .iter()
        .all(|p| ["id", "self", "organizer", "asyncOperation"]
            .iter()
            .all(|key| p.get(key).is_none())));
    assert!(people.contains(&json!({"email":"new@example.test","responseStatus":"needsAction"})));
    assert_eq!(people.len(), 3);
    let unchanged = patch(
        &original,
        json!({"attendees":["ROOM@example.test","GUEST@example.test"]}),
    )
    .unwrap();
    assert!(unchanged.get("attendees").is_none());
    let cleared = patch(&original, json!({"attendees":[]})).unwrap();
    assert_eq!(
        cleared["attendees"],
        json!([writable(&original["attendees"][0])])
    );
    let omitted = patch(&original, json!({"location":"Elsewhere"})).unwrap();
    assert!(omitted.get("attendees").is_none());
    for bad in [
        json!([CAL]),
        json!(["guest@example.test", "GUEST@example.test"]),
        json!(["invalid"]),
        json!([{"email":"x@example.test","responseStatus":"accepted"}]),
    ] {
        assert_eq!(
            patch(&original, json!({"attendees":bad})).unwrap_err().code,
            "invalid_request"
        );
    }
}

#[test]
fn invitation_incomplete_or_unsupported_rosters_block_only_roster_replacement() {
    for (field, value, reason) in [
        ("attendeesOmitted", json!(true), "incomplete_attendees"),
        (
            "attendees",
            json!([{"email":"group@example.test","asyncOperation":"inProgress"}]),
            "incomplete_attendees",
        ),
        (
            "attendees",
            json!([{"displayName":"No address"}]),
            "unsupported_attendees",
        ),
        (
            "attendees",
            json!([{"email":"guest@example.test"},{"email":"GUEST@example.test"}]),
            "unsupported_attendees",
        ),
        (
            "attendees",
            json!([{"email":"guest@example.test","providerExtension":"unrepresentable"}]),
            "unsupported_attendees",
        ),
    ] {
        let mut source = invited();
        source[field] = value;
        assert_eq!(
            patch(&source, json!({"attendees":[]})).unwrap_err().code,
            reason
        );
        assert_eq!(
            patch(&source, json!({"title":"A title edit"})).unwrap()["summary"],
            "A title edit"
        );
        let event = fm_calendar::normalize("account", CAL, "UTC", &source).unwrap();
        assert!(initial_event(&event, Access::Owner, "edit", 2).is_ok());
        assert_eq!(
            live_guest_actions(&event, Access::Owner).unwrap_err().code,
            reason
        );
    }
}

#[test]
fn invitation_fifty_participants_includes_protected_and_implicit_organizer() {
    let addresses = |n| {
        Value::Array(
            (0..n)
                .map(|i| json!(format!("guest{i}@example.test")))
                .collect(),
        )
    };
    let create = |people: Value| {
        field_patch(
            "test-options",
            &json!({"title":"Invitation","start":"2026-09-12","end":"2026-09-13","all_day":true,"attendees":people}),
            None,
            "UTC",
            CAL,
            2,
        )
    };
    assert_eq!(
        create(addresses(49)).unwrap()["attendees"]
            .as_array()
            .unwrap()
            .len(),
        49
    );
    assert_eq!(
        create(addresses(50)).unwrap_err().code,
        "too_many_attendees"
    );
    let mut source = invited();
    source["attendees"] = json!([
        {"email":CAL,"organizer":true},
        {"email":"self@example.test","self":true}
    ]);
    assert_eq!(
        patch(&source, json!({"attendees":addresses(48)})).unwrap()["attendees"]
            .as_array()
            .unwrap()
            .len(),
        50
    );
    assert_eq!(
        patch(&source, json!({"attendees":addresses(49)}))
            .unwrap_err()
            .code,
        "too_many_attendees"
    );
    source["attendees"] = json!([{ "email":"self@example.test","self":true }]);
    assert_eq!(
        patch(&source, json!({"attendees":addresses(48)})).unwrap()["attendees"]
            .as_array()
            .unwrap()
            .len(),
        49
    );
    assert_eq!(
        patch(&source, json!({"attendees":addresses(49)}))
            .unwrap_err()
            .code,
        "too_many_attendees"
    );
}

#[test]
fn invitation_fifty_writable_rows_reconcile_without_double_counting_organizer() {
    let mut original = invited();
    original["attendees"] = json!([{ "email":CAL,"organizer":true,"self":true,"responseStatus":"accepted","id":"readonly-organizer-id" }]);
    let guests: Vec<_> = (0..49).map(|i| format!("guest{i}@example.test")).collect();
    let body = patch(&original, json!({"attendees":guests})).unwrap();
    let people = body["attendees"].as_array().unwrap();
    assert_eq!(people.len(), 50);
    assert!(people
        .iter()
        .all(|p| ["id", "self", "organizer", "asyncOperation"]
            .iter()
            .all(|key| p.get(key).is_none())));
    let mut observed = original.clone();
    observed["attendees"] = body["attendees"].clone();
    observed["attendees"][0] = original["attendees"][0].clone();
    assert!(matches_patch_v2(&observed, &body));
    observed["attendees"].as_array_mut().unwrap().pop();
    assert!(!matches_patch_v2(&observed, &body));
    assert_eq!(original["attendees"][0]["id"], "readonly-organizer-id");
}

#[test]
fn invitation_labels_meetings_and_ordinary_attachments_survive_dispatch() {
    let (_dir, mut store, account) = setup();
    let original = invited();
    let mut completed_conference = original.clone();
    completed_conference["conferenceData"]["createRequest"] =
        json!({"requestId":"old-conference","status":{"statusCode":"success"}});
    assert!(fm_calendar::ordinary_conference_supported(
        &completed_conference
    ));
    completed_conference["conferenceData"]["createRequest"]["status"]["statusCode"] =
        "pending".into();
    assert!(!fm_calendar::ordinary_conference_supported(
        &completed_conference
    ));
    let mut req = request("edit");
    req["fields"] = json!({"title":"New title","attendees":["guest@example.test"]});
    let lease = prepare(&mut store, &account, req, Some(original.clone()));
    assert_eq!(lease.dispatch["version"], 2);
    assert_eq!(lease.dispatch["event_label_version"], 1);
    assert_eq!(lease.dispatch["baseline"], original);
    assert_eq!(lease.dispatch["send_updates"], "all");
    let mut accepted = original.clone();
    for (key, value) in lease.dispatch["body"].as_object().unwrap() {
        accepted[key] = value.clone();
    }
    accepted["etag"] = "v2".into();
    store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Accepted(accepted.clone()))
        .unwrap();
    let result = store.calendar_live_publish(&lease).unwrap();
    assert_eq!(result["outcome"]["notification_delivery"], "unverified");
    assert_eq!(
        store
            .stored_calendar_event(&account, CAL, "event1")
            .unwrap()
            .0
            .source,
        accepted
    );
    let mut legacy_color = original.clone();
    legacy_color.as_object_mut().unwrap().remove("eventLabelId");
    assert_eq!(
        patch(&legacy_color, json!({"title":"New"})).unwrap()["colorId"],
        "3"
    );
    for solution in [json!("unknown"), json!("")] {
        let mut source = original.clone();
        source["conferenceData"]["conferenceSolution"]["key"]["type"] = solution;
        let event = fm_calendar::normalize(&account, CAL, "UTC", &source).unwrap();
        assert_eq!(
            initial_event(&event, Access::Owner, "edit", 2)
                .unwrap_err()
                .code,
            "unsupported_operation"
        );
    }
}

#[test]
fn invitation_reconcile_requires_exact_complete_guest_membership_including_removals() {
    let source = invited();
    let body = patch(&source, json!({"attendees":["new@example.test"]})).unwrap();
    assert!(!matches_patch_v2(&source, &body));
    let mut observed = source.clone();
    observed["attendees"] = body["attendees"].clone();
    assert!(matches_patch_v2(&observed, &body));
    // A recipient can respond between the mutation and readback; this isn't an
    // organizer attempt to set somebody else's RSVP.
    observed["attendees"][1]["responseStatus"] = "accepted".into();
    assert!(matches_patch_v2(&observed, &body));
    observed["attendeesOmitted"] = true.into();
    assert!(!matches_patch_v2(&observed, &body));
    observed.as_object_mut().unwrap().remove("attendeesOmitted");
    observed["attendees"]
        .as_array_mut()
        .unwrap()
        .push(json!({"email":"extra@example.test"}));
    assert!(!matches_patch_v2(&observed, &body));
    let empty = json!({"attendees":[]});
    assert!(!matches_patch_v2(&source, &empty));
    assert!(matches_patch_v2(&json!({"attendees":[]}), &empty));
    let rsvp =
        json!({"attendeesOmitted":true,"attendees":[{"email":CAL,"responseStatus":"accepted"}]});
    assert!(matches_patch_v2(&source, &rsvp));
    let mut duplicate = source;
    duplicate["attendees"]
        .as_array_mut()
        .unwrap()
        .push(json!({"email":CAL,"self":true,"responseStatus":"accepted"}));
    assert!(!matches_patch_v2(&duplicate, &rsvp));
}

#[test]
fn invitation_unknown_restart_observed_state_is_terminal_and_exact_late_ack_strengthens() {
    let (dir, mut store, account) = setup();
    let original = invited();
    let mut req = request("edit");
    req["fields"] = json!({"attendees":["guest@example.test"]});
    let lease = prepare(&mut store, &account, req.clone(), Some(original.clone()));
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    assert_eq!(
        store.calendar_live_status(&account, "edit").unwrap()["state"],
        "unknown"
    );
    let mut observed = original;
    observed["attendees"] = lease.dispatch["body"]["attendees"].clone();
    observed["etag"] = "observed-v2".into();
    let mut incomplete = observed.clone();
    incomplete["start"] = Value::Null;
    assert_eq!(
        store
            .calendar_live_reconcile_source(&lease, incomplete)
            .unwrap()["state"],
        "unknown"
    );
    let status = store
        .calendar_live_reconcile_source(&lease, observed.clone())
        .unwrap();
    assert_eq!(status["state"], "reconciled");
    assert_eq!(status["outcome"]["kind"], "observed_state");
    assert_eq!(status["outcome"]["observed_etag"], "observed-v2");
    assert_eq!(status["outcome"]["notification_delivery"], "unverified");
    assert_eq!(store.calendar_live_prepare(&account, &req).unwrap(), status);
    assert!(store.calendar_live_start(&account, "edit", 1).is_err());
    let published = store.calendar_live_publish(&lease).unwrap();
    assert_eq!(published["state"], "reconciled");
    assert_eq!(published["outcome"]["cache_published"], true);
    assert_eq!(store.calendar_live_publish(&lease).unwrap(), published);
    assert_eq!(
        store
            .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Unknown)
            .unwrap(),
        published
    );
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    assert_eq!(
        store.calendar_live_status(&account, "edit").unwrap(),
        published
    );
    let accepted = store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Accepted(observed))
        .unwrap();
    assert_eq!(accepted["state"], "accepted");
    assert_eq!(accepted["outcome"]["evidence_origin"], "provider_response");
    assert_eq!(
        store.calendar_live_publish(&lease).unwrap()["state"],
        "acknowledged"
    );
}

#[test]
fn invitation_reconciled_delete_and_newer_cache_remain_distinct_from_acknowledgement() {
    let (_dir, mut store, account) = setup();
    let lease = prepare(&mut store, &account, request("delete"), Some(invited()));
    assert_eq!(
        store
            .calendar_live_reconcile_source(&lease, event())
            .unwrap()["state"],
        "unknown"
    );
    assert_eq!(
        store
            .calendar_live_reconcile_source(&lease, json!({"id":"other","status":"cancelled"}))
            .unwrap_err()
            .code,
        "scope_mismatch"
    );
    let result = store
        .calendar_live_reconcile_source(&lease, json!({"id":"event1","status":"cancelled"}))
        .unwrap();
    assert_eq!(result["outcome"]["event_cancelled"], true);
    assert_eq!(result["outcome"]["observed_etag"], Value::Null);
    let mut newer = event();
    newer["summary"] = "A later remote version".into();
    newer["etag"] = "v10".into();
    publish(
        &mut store,
        &account,
        vec![newer.clone()],
        CalendarSyncMode::Full,
    );
    let result = store.calendar_live_publish(&lease).unwrap();
    assert_eq!(result["state"], "reconciled");
    assert_eq!(result["outcome"]["cache_published"], false);
    assert_eq!(
        store
            .stored_calendar_event(&account, CAL, "event1")
            .unwrap()
            .0
            .source,
        newer
    );
    assert_eq!(
        store
            .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Deleted)
            .unwrap()["outcome"]["kind"],
        "deleted"
    );
}

#[test]
fn invitation_schema_seventeen_preserves_legacy_journal_bytes_and_interpretation() {
    let (dir, mut store, account) = setup();
    let mut req = request("edit");
    req["fields"] = json!({"title":"Legacy wanted"});
    store.calendar_live_prepare(&account, &req).unwrap();
    store.db.execute("UPDATE calendar_write_operations SET intent_json=json_remove(intent_json,'$.version') WHERE account=?1",[&account]).unwrap();
    let epoch = store.calendar_checkpoint(&account, CAL).unwrap()["permission_epoch"]
        .as_i64()
        .unwrap();
    let lease = store
        .calendar_live_claim(&account, "edit", 0, 1, epoch, Some(event()))
        .unwrap();
    assert!(lease.dispatch.get("version").is_none());
    store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Unknown)
        .unwrap();
    let receipt = "{\"legacy exact bytes\": true }";
    store.db.execute("INSERT INTO calendar_write_operations(account,request_id,fingerprint,intent_json,result_json,execution_mode,state) VALUES(?1,'old-completed','old','{\"target\":{\"calendar\":\"calendar\",\"event\":\"event\"},\"request\":{\"kind\":\"edit\"}}',?2,'live','acknowledged')",params![account,receipt]).unwrap();
    type PreservedRow = (
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let rows = |store: &Store| -> Vec<PreservedRow> {
        store.db.prepare("SELECT request_id,intent_json,dispatch_json,outcome_json,result_json FROM calendar_write_operations ORDER BY request_id").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap().map(|r|r.unwrap()).collect()
    };
    let before = rows(&store);
    crate::test_profiles::restore_schema_sixteen(&store.db);
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        crate::SCHEMA_VERSION
    );
    assert_eq!(rows(&store), before);
    assert_eq!(
        store.calendar_live_status(&account, "edit").unwrap()["intent_version"],
        1
    );
    let mut observed = event();
    observed["summary"] = "Legacy wanted".into();
    observed["etag"] = "v2".into();
    let result = store
        .calendar_live_reconcile_source(&lease, observed)
        .unwrap();
    assert_eq!(result["state"], "accepted");
    assert_eq!(result["outcome"], json!({"kind":"accepted"}));
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
