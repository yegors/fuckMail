use chrono::{DateTime, Utc};
use fm_calendar::*;
use serde_json::{json, Value};

fn utc(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}
fn window(from: &str, to: &str) -> Window {
    Window::new(utc(from), utc(to), "America/Toronto".parse().unwrap()).unwrap()
}
fn event(value: Value) -> Event {
    normalize("work", "primary", "America/Toronto", &value).unwrap()
}
fn weekly(start: &str, end: &str) -> Event {
    event(
        json!({"id":"series","etag":"v1","summary":"Human review","start":{"dateTime":start,"timeZone":"America/Toronto"},"end":{"dateTime":end,"timeZone":"America/Toronto"},"recurrence":["RRULE:FREQ=WEEKLY;COUNT=3"]}),
    )
}
fn times(items: &[Occurrence]) -> Vec<String> {
    items.iter().map(|v| v.start.identity()).collect()
}

#[test]
fn a_timed_event_may_end_when_it_starts_but_an_all_day_one_may_not() {
    let point = event(
        json!({"id":"point","etag":"v1","status":"confirmed","start":{"dateTime":"2023-03-30T14:00:00-04:00","timeZone":"America/New_York"},"end":{"dateTime":"2023-03-30T14:00:00-04:00","timeZone":"America/New_York"}}),
    );
    assert_eq!(
        expand(
            &point,
            &[],
            window("2023-03-30T00:00:00Z", "2023-03-31T00:00:00Z"),
            Access::Owner
        )
        .unwrap()
        .len(),
        1
    );
    assert!(normalize("work","primary","America/Toronto",&json!({"id":"back","start":{"dateTime":"2023-03-30T14:00:00-04:00"},"end":{"dateTime":"2023-03-30T13:59:00-04:00"}})).is_err());
    assert!(normalize("work","primary","America/Toronto",&json!({"id":"day","start":{"date":"2023-03-30"},"end":{"date":"2023-03-30"}})).is_err());
}

#[test]
fn weekly_wall_clock_keeps_nine_across_spring_dst() {
    let series = weekly("2026-03-01T09:00:00-05:00", "2026-03-01T10:00:00-05:00");
    let items = expand(
        &series,
        &[],
        window("2026-03-01T00:00:00Z", "2026-03-20T00:00:00Z"),
        Access::Owner,
    )
    .unwrap();
    // Independently specified civil dates and UTC offsets, not library-generated expectations.
    assert_eq!(
        times(&items),
        [
            "2026-03-01T14:00:00+00:00",
            "2026-03-08T13:00:00+00:00",
            "2026-03-15T13:00:00+00:00"
        ]
    );
    assert_eq!(items[1].end.identity(), "2026-03-08T14:00:00+00:00");
}

#[test]
fn weekly_wall_clock_keeps_nine_across_fall_dst() {
    let series = weekly("2026-10-25T09:00:00-04:00", "2026-10-25T10:00:00-04:00");
    let items = expand(
        &series,
        &[],
        window("2026-10-24T00:00:00Z", "2026-11-10T00:00:00Z"),
        Access::Owner,
    )
    .unwrap();
    assert_eq!(
        times(&items),
        [
            "2026-10-25T13:00:00+00:00",
            "2026-11-01T14:00:00+00:00",
            "2026-11-08T14:00:00+00:00"
        ]
    );
}

#[test]
fn repeated_dst_wall_time_uses_first_occurrence() {
    let series = weekly("2026-10-25T01:30:00-04:00", "2026-10-25T02:00:00-04:00");
    let items = expand(
        &series,
        &[],
        window("2026-10-24T00:00:00Z", "2026-11-10T00:00:00Z"),
        Access::Owner,
    )
    .unwrap();
    // November 1 has two 01:30 times; RFC 5545 selects the first, still EDT.
    assert_eq!(
        times(&items),
        [
            "2026-10-25T05:30:00+00:00",
            "2026-11-01T05:30:00+00:00",
            "2026-11-08T06:30:00+00:00",
        ]
    );
}

#[test]
fn moved_and_cancelled_instances_keep_original_identity() {
    let series = weekly("2026-03-01T09:00:00-05:00", "2026-03-01T10:00:00-05:00");
    let moved = event(
        json!({"id":"moved","etag":"v2","summary":"Moved review","recurringEventId":"series","originalStartTime":{"dateTime":"2026-03-08T09:00:00-04:00","timeZone":"America/Toronto"},"start":{"dateTime":"2026-03-09T15:00:00-04:00","timeZone":"America/Toronto"},"end":{"dateTime":"2026-03-09T16:00:00-04:00","timeZone":"America/Toronto"}}),
    );
    let cancelled = event(
        json!({"id":"cancelled","status":"cancelled","recurringEventId":"series","originalStartTime":{"dateTime":"2026-03-15T09:00:00-04:00","timeZone":"America/Toronto"}}),
    );
    let items = expand(
        &series,
        &[moved, cancelled],
        window("2026-03-01T00:00:00Z", "2026-03-20T00:00:00Z"),
        Access::Owner,
    )
    .unwrap();
    assert_eq!(
        times(&items),
        ["2026-03-01T14:00:00+00:00", "2026-03-09T19:00:00+00:00"]
    );
    assert_eq!(
        items[1].key.original_start.as_deref(),
        Some("2026-03-08T13:00:00+00:00")
    );
    assert_eq!(items[1].key.series, "series");
}

#[test]
fn moved_occurrence_enters_window_from_outside_original_range() {
    let series = weekly("2026-03-01T09:00:00-05:00", "2026-03-01T10:00:00-05:00");
    let moved = event(
        json!({"id":"moved","summary":"Moved in","recurringEventId":"series","originalStartTime":{"dateTime":"2026-03-01T09:00:00-05:00"},"start":{"dateTime":"2026-03-19T09:00:00-04:00"},"end":{"dateTime":"2026-03-19T10:00:00-04:00"}}),
    );
    let items = expand(
        &series,
        &[moved],
        window("2026-03-19T00:00:00Z", "2026-03-20T00:00:00Z"),
        Access::Owner,
    )
    .unwrap();
    assert_eq!(times(&items), ["2026-03-19T13:00:00+00:00"]);
    assert_eq!(
        items[0].key.original_start.as_deref(),
        Some("2026-03-01T14:00:00+00:00")
    );
}

#[test]
fn all_day_dates_have_exclusive_end_and_survive_dst() {
    let all_day = event(
        json!({"id":"days","summary":"Conference","start":{"date":"2026-03-07"},"end":{"date":"2026-03-09"}}),
    );
    let items = expand(
        &all_day,
        &[],
        window("2026-03-08T05:00:00Z", "2026-03-09T04:00:00Z"),
        Access::Reader,
    )
    .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].start.identity(), "2026-03-07");
    assert_eq!(items[0].end.identity(), "2026-03-09");
    assert!(expand(
        &all_day,
        &[],
        window("2026-03-09T04:00:00Z", "2026-03-10T04:00:00Z"),
        Access::Reader
    )
    .unwrap()
    .is_empty());
}

#[test]
fn non_existent_dst_wall_time_is_skipped_without_consuming_count() {
    let mut series = weekly("2026-03-07T02:30:00-05:00", "2026-03-07T03:00:00-05:00");
    series.recurrence = vec!["RRULE:FREQ=DAILY;COUNT=3".into()];
    let rows = expand(
        &series,
        &[],
        window("2026-03-07T00:00:00Z", "2026-03-12T00:00:00Z"),
        Access::Owner,
    )
    .unwrap();
    assert_eq!(
        times(&rows),
        [
            "2026-03-07T07:30:00+00:00",
            "2026-03-09T06:30:00+00:00",
            "2026-03-10T06:30:00+00:00"
        ]
    );
}

#[test]
fn collection_adapter_uses_one_identity_for_week_footer_and_cli() {
    let series = weekly("2026-03-01T09:00:00-05:00", "2026-03-01T10:00:00-05:00");
    let day = |s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    let week = project_google_events(
        "work",
        "primary",
        "America/Toronto",
        &[series.source.clone()],
        day("2026-03-01"),
        day("2026-03-08"),
        Access::Reader,
    )
    .unwrap();
    let footer = project_google_events(
        "work",
        "primary",
        "America/Toronto",
        &[series.source],
        day("2026-03-01"),
        day("2026-03-02"),
        Access::Reader,
    )
    .unwrap();
    assert_eq!(week, footer);
    assert_eq!(week[0]["start"], "2026-03-01T09:00:00-05:00");
    assert_eq!(week[0]["title"], "Human review");
    assert_eq!(week[0]["calendar_id"], "primary");
}

#[test]
fn rdate_exdate_and_monthly_rule_keep_expected_occurrences() {
    let series = event(
        json!({"id":"monthly","summary":"Last Monday","start":{"dateTime":"2026-09-28T09:00:00-04:00","timeZone":"America/Toronto"},"end":{"dateTime":"2026-09-28T10:00:00-04:00","timeZone":"America/Toronto"},"recurrence":["RRULE:FREQ=MONTHLY;BYDAY=-1MO;COUNT=3","RDATE;TZID=America/Toronto:20261001T090000","EXDATE;TZID=America/Toronto:20260928T090000"]}),
    );
    let items = expand(
        &series,
        &[],
        window("2026-09-27T00:00:00Z", "2026-11-01T00:00:00Z"),
        Access::Owner,
    )
    .unwrap();
    assert_eq!(
        times(&items),
        ["2026-10-01T13:00:00+00:00", "2026-10-26T13:00:00+00:00"]
    );
}

#[test]
fn all_day_recurrence_keeps_dates() {
    let series = event(
        json!({"id":"days","start":{"date":"2026-09-07"},"end":{"date":"2026-09-08"},"recurrence":["RRULE:FREQ=DAILY;COUNT=3"]}),
    );
    let items = expand(
        &series,
        &[],
        window("2026-09-07T04:00:00Z", "2026-09-10T04:00:00Z"),
        Access::Reader,
    )
    .unwrap();
    assert_eq!(times(&items), ["2026-09-07", "2026-09-08", "2026-09-09"]);
}

#[test]
fn all_day_exdate_and_rdate_are_preserved() {
    let series = event(
        json!({"id":"days","start":{"date":"2026-09-07"},"end":{"date":"2026-09-08"},"recurrence":["RRULE:FREQ=DAILY;COUNT=3","EXDATE;VALUE=DATE:20260908","RDATE;VALUE=DATE:20260911"]}),
    );
    let items = expand(
        &series,
        &[],
        window("2026-09-07T04:00:00Z", "2026-09-12T04:00:00Z"),
        Access::Reader,
    )
    .unwrap();
    assert_eq!(times(&items), ["2026-09-07", "2026-09-09", "2026-09-11"]);
}

#[test]
fn private_master_cannot_leak_through_public_looking_exception() {
    let mut series = weekly("2026-03-01T09:00:00-05:00", "2026-03-01T10:00:00-05:00");
    series.private = true;
    let exception = event(
        json!({"id":"override","summary":"Secret exception title","recurringEventId":"series","originalStartTime":{"dateTime":"2026-03-08T09:00:00-04:00"},"start":{"dateTime":"2026-03-08T11:00:00-04:00"},"end":{"dateTime":"2026-03-08T12:00:00-04:00"}}),
    );
    let rows = expand(
        &series,
        &[exception],
        window("2026-03-08T00:00:00Z", "2026-03-09T00:00:00Z"),
        Access::Reader,
    )
    .unwrap();
    assert_eq!(rows[0].title, "Busy");
}

#[test]
fn source_and_private_fields_do_not_leak_into_busy_projection() {
    let secret = event(
        json!({"id":"secret-id","summary":"Secret launch","description":"confidential plan","location":"Private room","visibility":"private","organizer":{"email":"private@example.test"},"attendees":[{"email":"hidden@example.test"}],"conferenceData":{"entryPoints":[{"entryPointType":"video","uri":"https://example.test/secret"}]},"start":{"dateTime":"2026-09-11T09:00:00-04:00"},"end":{"dateTime":"2026-09-11T10:00:00-04:00"}}),
    );
    for access in [Access::FreeBusy, Access::Reader] {
        let view = expand(
            &secret,
            &[],
            window("2026-09-11T00:00:00Z", "2026-09-12T00:00:00Z"),
            access,
        )
        .unwrap();
        assert_eq!(view[0].title, "Busy");
        let output = serde_json::to_string(&view).unwrap();
        for protected in [
            "Secret launch",
            "confidential plan",
            "Private room",
            "private@example.test",
            "hidden@example.test",
            "https://example.test/secret",
            "conference_url",
            "attendees",
        ] {
            assert!(!output.contains(protected), "leaked {protected}");
        }
    }
    assert!(expand(
        &secret,
        &[],
        window("2026-09-11T00:00:00Z", "2026-09-12T00:00:00Z"),
        Access::None
    )
    .unwrap()
    .is_empty());
    assert_eq!(secret.source["summary"], "Secret launch");
}

#[test]
fn overlapping_independent_events_are_both_preserved_and_scope_is_strict() {
    let a = weekly("2026-03-01T09:00:00-05:00", "2026-03-01T10:00:00-05:00");
    let b = event(
        json!({"id":"parallel","start":{"dateTime":"2026-03-01T09:30:00-05:00"},"end":{"dateTime":"2026-03-01T10:30:00-05:00"}}),
    );
    let range = window("2026-03-01T00:00:00Z", "2026-03-02T00:00:00Z");
    let mut rows = expand(&a, &[], range, Access::Reader).unwrap();
    rows.extend(expand(&b, &[], range, Access::Reader).unwrap());
    assert_eq!(rows.len(), 2);
    let foreign=normalize("personal","primary","America/Toronto",&json!({"id":"exception","status":"cancelled","recurringEventId":"series","originalStartTime":{"dateTime":"2026-03-01T09:00:00-05:00"}})).unwrap();
    assert_eq!(
        expand(&a, &[foreign], range, Access::Owner)
            .unwrap_err()
            .code,
        "scope_mismatch"
    );
}

#[test]
fn unsupported_rules_zones_and_unbounded_ranges_are_explicit_errors() {
    let mut event = weekly("2026-03-01T09:00:00-05:00", "2026-03-01T10:00:00-05:00");
    event.recurrence = vec!["RRULE:FREQ=SECONDLY".into()];
    assert_eq!(
        expand(
            &event,
            &[],
            window("2026-03-01T00:00:00Z", "2026-03-02T00:00:00Z"),
            Access::Owner
        )
        .unwrap_err()
        .code,
        "unsupported_rule"
    );
    assert!(Window::new(
        utc("2026-01-01T00:00:00Z"),
        utc("2026-12-31T00:00:00Z"),
        "UTC".parse().unwrap()
    )
    .is_err());
    assert!(normalize("a", "b", "Unknown/Zone", &event.source).is_err());
    assert_eq!(event.recurrence, ["RRULE:FREQ=SECONDLY"]);
}

#[test]
fn edits_rsvp_and_following_have_one_notification_owner_and_revision_guards() {
    let mut source = weekly("2026-03-01T09:00:00-05:00", "2026-03-01T10:00:00-05:00").source;
    source["attendees"] = json!([{"email":"me@example.test","self":true,"responseStatus":"needsAction"},{"email":"other@example.test","responseStatus":"tentative"}]);
    let meeting = event(source);
    assert_eq!(
        edit(&meeting, Access::Reader, "v1", json!({"summary":"change"}))
            .unwrap_err()
            .code,
        "permission_denied"
    );
    assert_eq!(
        edit(&meeting, Access::Owner, "old", json!({"summary":"change"}))
            .unwrap_err()
            .code,
        "stale_revision"
    );
    let response = rsvp(
        &meeting,
        Access::Reader,
        "me@example.test",
        "v1",
        ResponseStatus::Accepted,
    )
    .unwrap();
    assert_eq!(response.body["attendees"][0]["responseStatus"], "accepted");
    assert_eq!(response.body["attendees"][1]["responseStatus"], "tentative");
    assert_eq!(response.notification_owner, "google_calendar_api");
    assert_eq!(response.send_updates, "all");
    assert!(rsvp(
        &meeting,
        Access::Reader,
        "other@example.test",
        "v1",
        ResponseStatus::Accepted
    )
    .is_err());
    assert!(rsvp(
        &meeting,
        Access::FreeBusy,
        "me@example.test",
        "v1",
        ResponseStatus::Accepted
    )
    .is_err());
    let following=following(&meeting,Access::Owner,"v1",vec!["RRULE:FREQ=WEEKLY;COUNT=1".into()],json!({"recurrence":["RRULE:FREQ=WEEKLY;COUNT=2"],"start":{"dateTime":"2026-03-08T10:00:00-04:00"}})).unwrap();
    assert!(!following.atomic);
    assert!(following.reconcile_exceptions);
    assert_eq!(following.trim_existing.target.event, "series");
}

#[test]
fn sync_pages_commit_cursor_only_at_end_and_410_restarts_full_snapshot() {
    let scope = SyncScope {
        account: "work".into(),
        calendar: "primary".into(),
        query_fingerprint: "expanded=false;showDeleted=true".into(),
    };
    let mut batch = SyncBatch::new(scope.clone(), true).unwrap();
    assert!(matches!(
        batch
            .apply_page(
                &scope,
                None,
                vec![json!({"id":"one","summary":"old"})],
                Some("p2".into()),
                None
            )
            .unwrap(),
        PageResult::NextPage(_)
    ));
    assert!(batch
        .apply_page(&scope, None, vec![], None, Some("bad-order".into()))
        .is_err());
    let mut reset = batch.expired_token().unwrap();
    let PageResult::Complete(result) = reset
        .apply_page(
            &scope,
            None,
            vec![
                json!({"id":"one","summary":"fresh"}),
                json!({"id":"cancelled","status":"cancelled"}),
            ],
            None,
            Some("new-token".into()),
        )
        .unwrap()
    else {
        panic!("expected final snapshot")
    };
    assert!(result.full_replace);
    assert_eq!(result.next_sync_token, "new-token");
    assert_eq!(result.replacements["one"]["summary"], "fresh");
    assert_eq!(result.replacements["cancelled"]["status"], "cancelled");
    assert!(reset
        .apply_page(&scope, None, vec![], None, Some("again".into()))
        .is_err());
}

#[test]
fn meeting_links_come_from_the_location_or_notes() {
    let teams = "________________\nMicrosoft Teams meeting\nJoin: https://teams.microsoft.com/meet/341468704584292?p=ifEabn9\nMeeting ID: 341\nNeed help?<https://aka.ms/JoinTeamsMeeting?omkt=en-US> | System reference<https://teams.microsoft.com/l/meetup-join/19%3ameeting_X%40thread.v2/0>";
    assert_eq!(
        fm_calendar::meeting_link(teams).as_deref(),
        Some("https://teams.microsoft.com/meet/341468704584292?p=ifEabn9")
    );
    assert_eq!(
        fm_calendar::meeting_link("Room 4 / https://acme.zoom.us/j/123456789?pwd=x.").as_deref(),
        Some("https://acme.zoom.us/j/123456789?pwd=x")
    );
    assert_eq!(
        fm_calendar::meeting_link(r#"<a href="https://meet.google.com/abc-defg-hij?authuser=0&amp;hs=1">Join</a>"#).as_deref(),
        Some("https://meet.google.com/abc-defg-hij?authuser=0&hs=1")
    );
    for none in ["Need help? https://aka.ms/JoinTeamsMeeting", "http://teams.microsoft.com/meet/1", "https://zoom.us/pricing", "Microsoft Teams Meeting", ""] {
        assert_eq!(fm_calendar::meeting_link(none), None, "{none}");
    }
}
