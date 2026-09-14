use chrono::{DateTime, Utc};
use fm_calendar::*;
use serde_json::{json, Value};

fn window() -> Window {
    let utc = |s| DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
    Window::new(
        utc("2026-03-01T00:00:00Z"),
        utc("2026-03-20T00:00:00Z"),
        "America/Toronto".parse().unwrap(),
    )
    .unwrap()
}
fn source(id: &str, private: bool) -> Value {
    json!({"id":id,"etag":"v1","summary":"Protected meeting","description":"Protected description",
        "location":"Protected room","visibility":if private {"private"} else {"public"},
        "organizer":{"email":"organizer@example.test"},
        "attendees":[{"email":"me@example.test","self":true,"responseStatus":"needsAction"},
                     {"email":"other@example.test","responseStatus":"tentative"}],
        "conferenceData":{"entryPoints":[{"entryPointType":"video","uri":"https://example.test/protected"}]},
        "start":{"dateTime":"2026-03-01T09:00:00-05:00","timeZone":"America/Toronto"},
        "end":{"dateTime":"2026-03-01T10:00:00-05:00","timeZone":"America/Toronto"}})
}
fn event(source: &Value) -> Event {
    normalize("work", "primary", "America/Toronto", source).unwrap()
}
fn series(private: bool) -> Event {
    let mut value = source("series", private);
    value["recurrence"] = json!(["RRULE:FREQ=WEEKLY;COUNT=3"]);
    event(&value)
}
fn exception(private: bool) -> Event {
    let mut value = source("exception", private);
    value["recurringEventId"] = json!("series");
    value["originalStartTime"] = json!({"dateTime":"2026-03-08T09:00:00-04:00"});
    value["start"] = json!({"dateTime":"2026-03-08T11:00:00-04:00"});
    value["end"] = json!({"dateTime":"2026-03-08T12:00:00-04:00"});
    event(&value)
}
fn assert_busy(rows: &[Value]) {
    assert!(!rows.is_empty());
    for row in rows {
        assert_eq!(row["title"], "Busy");
        assert_eq!(row["busy_only"], true);
        assert_eq!(row["read_only"], true);
        for field in [
            "description",
            "location",
            "conference_url",
            "organizer",
            "attendees",
            "source",
        ] {
            assert!(row.get(field).is_none(), "leaked {field}");
        }
    }
    let wire = serde_json::to_string(rows).unwrap();
    for value in [
        "Protected",
        "organizer@example.test",
        "me@example.test",
        "other@example.test",
        "/protected",
    ] {
        assert!(!wire.contains(value), "leaked {value}");
    }
}
fn assert_immutable(event: &Event, access: Access) {
    assert_eq!(
        edit(event, access, "v1", json!({"summary":"Changed"}))
            .unwrap_err()
            .code,
        "permission_denied"
    );
    assert_eq!(
        rsvp(
            event,
            access,
            "me@example.test",
            "v1",
            ResponseStatus::Accepted
        )
        .unwrap_err()
        .code,
        "permission_denied"
    );
}

#[test]
fn limited_writer_edits_public_events_but_private_details_and_writes_are_denied() {
    let access = Access::WriterWithoutPrivateAccess;
    let public = event(&source("public", false));
    let rows = project_event_in_window(&public, &[], "America/Toronto", window(), access).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["title"], "Protected meeting");
    assert_eq!(rows[0]["read_only"], false);
    assert_eq!(rows[0]["busy_only"], false);
    assert_eq!(rows[0]["conference_url"], "https://example.test/protected");
    let write = edit(&public, access, "v1", json!({"summary":"Changed"})).unwrap();
    assert_eq!(write.target, public.key);
    assert_eq!(write.if_match, "v1");
    assert_eq!(
        edit(&public, access, "old", json!({})).unwrap_err().code,
        "stale_revision"
    );
    let response = rsvp(
        &public,
        access,
        "me@example.test",
        "v1",
        ResponseStatus::Accepted,
    )
    .unwrap();
    assert_eq!(response.body["attendees"][0]["responseStatus"], "accepted");
    assert_eq!(response.body["attendees"][1], public.source["attendees"][1]);
    assert_eq!(response.send_updates, "all");

    let private = event(&source("private", true));
    for access in [Access::Reader, Access::WriterWithoutPrivateAccess] {
        assert_busy(
            &project_event_in_window(&private, &[], "America/Toronto", window(), access).unwrap(),
        );
        assert_immutable(&private, access);
    }
    assert_eq!(private.source, source("private", true));
    for access in [Access::Writer, Access::Owner] {
        let rows =
            project_event_in_window(&private, &[], "America/Toronto", window(), access).unwrap();
        assert_eq!(rows[0]["title"], "Protected meeting");
        assert_eq!(rows[0]["read_only"], false);
        edit(&private, access, "v1", json!({"summary":"Permitted"})).unwrap();
    }
    for private in [false, true] {
        let result = following(
            &series(private),
            access,
            "v1",
            vec!["RRULE:FREQ=WEEKLY;COUNT=1".into()],
            json!({"recurrence":["RRULE:FREQ=WEEKLY;COUNT=2"]}),
        );
        if private {
            assert_eq!(result.unwrap_err().code, "permission_denied");
        } else {
            assert!(!result.unwrap().atomic);
        }
    }
}

#[test]
fn master_privacy_and_private_exception_both_survive_limited_writer_expansion() {
    let child = exception(false);
    let private = series(true);
    for access in [Access::Reader, Access::WriterWithoutPrivateAccess] {
        let rows = project_event_in_window(
            &private,
            &[child.clone()],
            "America/Toronto",
            window(),
            access,
        )
        .unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1]["start"], "2026-03-08T11:00:00-04:00");
        assert_busy(&rows);
        let resolved = child.with_recurring_master(&private).unwrap();
        assert_eq!(resolved.source, child.source);
        assert_eq!(resolved.series_private, Some(true));
        assert_immutable(&resolved, access);
    }
    let public = series(false);
    let child = exception(true);
    let rows = project_event_in_window(
        &public,
        &[child.clone()],
        "America/Toronto",
        window(),
        Access::WriterWithoutPrivateAccess,
    )
    .unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["busy_only"], false);
    assert_eq!(rows[0]["read_only"], false);
    assert_busy(&rows[1..2]);
    assert_eq!(rows[2]["read_only"], false);
    assert_immutable(
        &child.with_recurring_master(&public).unwrap(),
        Access::WriterWithoutPrivateAccess,
    );
}

#[test]
fn unresolved_instances_fail_closed_until_same_scope_public_master_is_bound() {
    let orphan = exception(false);
    assert_eq!(orphan.series_private, None);
    for access in [Access::Reader, Access::WriterWithoutPrivateAccess] {
        assert_busy(
            &project_event_in_window(&orphan, &[], "America/Toronto", window(), access).unwrap(),
        );
        assert_immutable(&orphan, access);
    }
    let master = series(false);
    let resolved = orphan.with_recurring_master(&master).unwrap();
    assert_eq!(resolved.series_private, Some(false));
    assert_eq!(resolved.source, orphan.source);
    assert_eq!(resolved.key, orphan.key);
    assert_eq!(resolved.original_start, orphan.original_start);
    edit(
        &resolved,
        Access::WriterWithoutPrivateAccess,
        "v1",
        json!({"summary":"Permitted"}),
    )
    .unwrap();
    rsvp(
        &resolved,
        Access::Reader,
        "me@example.test",
        "v1",
        ResponseStatus::Accepted,
    )
    .unwrap();
    for dimension in 0..4 {
        let mut wrong = master.clone();
        match dimension {
            0 => wrong.key.account = "personal".into(),
            1 => wrong.key.calendar = "other".into(),
            2 => wrong.key.event = "other-series".into(),
            _ => wrong.recurrence.clear(),
        }
        assert_eq!(
            orphan.with_recurring_master(&wrong).unwrap_err().code,
            "scope_mismatch"
        );
    }
}

#[test]
fn compact_master_context_preserves_private_cancelled_and_scope_rules_after_source_drop() {
    let child = exception(false);
    for (private, cancelled) in [(false, false), (true, false), (false, true)] {
        let mut master = series(private);
        master.cancelled = cancelled;
        let expected = child.with_recurring_master(&master).unwrap();
        let context = master.recurring_master_context().unwrap();
        assert_eq!(context.key(), &master.key);
        assert_eq!(context.private(), private);
        assert_eq!(context.cancelled(), cancelled);
        drop(master);
        let actual = child.with_recurring_context(&context).unwrap();
        assert_eq!(actual.source, child.source);
        assert_eq!(actual.series_private, expected.series_private);
        assert_eq!(actual.cancelled, expected.cancelled);
        assert_eq!(
            Access::WriterWithoutPrivateAccess.for_event(&actual),
            Access::WriterWithoutPrivateAccess.for_event(&expected)
        );
        for dimension in 0..3 {
            let mut wrong = child.clone();
            match dimension {
                0 => wrong.key.account = "different".into(),
                1 => wrong.key.calendar = "different".into(),
                _ => wrong.series_id = Some("different".into()),
            }
            assert_eq!(
                wrong.with_recurring_context(&context).unwrap_err().code,
                "scope_mismatch"
            );
        }
    }
    assert!(event(&source("ordinary", false))
        .recurring_master_context()
        .is_err());
    assert!(child.recurring_master_context().is_err());
}

#[test]
fn collection_projection_groups_instances_and_cancelled_parent_suppresses_children() {
    let master = series(true);
    let child = exception(false);
    let access = Access::WriterWithoutPrivateAccess;
    let expected = project_event_in_window(
        &master,
        &[child.clone()],
        "America/Toronto",
        window(),
        access,
    )
    .unwrap();
    let actual = project_google_events_in_window(
        "work",
        "primary",
        "America/Toronto",
        &[child.source.clone(), master.source.clone()],
        window(),
        access,
    )
    .unwrap();
    assert_eq!(actual, expected);
    assert_busy(&actual);
    let orphan = project_google_events_in_window(
        "work",
        "primary",
        "America/Toronto",
        &[child.source.clone()],
        window(),
        access,
    )
    .unwrap();
    assert_busy(&orphan);
    let tombstone = json!({"id":"series","status":"cancelled"});
    for access in [
        Access::Reader,
        Access::WriterWithoutPrivateAccess,
        Access::Owner,
    ] {
        assert!(project_google_events_in_window(
            "work",
            "primary",
            "America/Toronto",
            &[child.source.clone(), tombstone.clone()],
            window(),
            access
        )
        .unwrap()
        .is_empty());
    }
    let inherited = child.with_recurring_master(&event(&tombstone)).unwrap();
    assert!(inherited.cancelled);
    assert_eq!(inherited.source, child.source);
    assert_eq!(
        project_google_events_in_window(
            "work",
            "primary",
            "America/Toronto",
            &[child.source.clone(), child.source],
            window(),
            access
        )
        .unwrap_err()
        .code,
        "duplicate_event"
    );
}
