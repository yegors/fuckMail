use super::*;
use crate::live_calendar::{CalendarPage, CalendarSyncMode};
use std::collections::BTreeSet;

#[test]
fn selected_reminder_time_none_and_default_control_the_due_window() {
    let window = fm_calendar::Window::from_dates(
        NaiveDate::from_ymd_opt(2026, 9, 15).unwrap(),
        NaiveDate::from_ymd_opt(2026, 9, 16).unwrap(),
        "UTC",
    )
    .unwrap();
    let now = DateTime::parse_from_rfc3339("2026-09-15T09:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let mut event = json!({"start":"2026-09-15T10:00:00Z","all_day":false,"reminder_minutes":60});
    assert!(due_now(&event, now, &window, 15).unwrap().is_some());
    assert!(due_now(&event, now - Duration::seconds(1), &window, 15)
        .unwrap()
        .is_none());
    // Explicit None (Google useDefault:false without overrides) never alerts.
    event["reminder_minutes"] = Value::Null;
    for offset in [Duration::minutes(45), Duration::hours(1)] {
        assert!(due_now(&event, now + offset, &window, 15)
            .unwrap()
            .is_none());
    }
    // An event that uses the calendar default follows the saved default lead.
    event.as_object_mut().unwrap().remove("reminder_minutes");
    assert!(due_now(&event, now + Duration::minutes(44), &window, 15)
        .unwrap()
        .is_none());
    assert!(due_now(&event, now + Duration::minutes(45), &window, 15)
        .unwrap()
        .is_some());
    assert!(due_now(&event, now + Duration::hours(1), &window, 15)
        .unwrap()
        .is_some());
    event["reminder_minutes"] = 0.into();
    assert!(due_now(&event, now + Duration::minutes(59), &window, 15)
        .unwrap()
        .is_none());
    assert!(due_now(&event, now + Duration::hours(1), &window, 15)
        .unwrap()
        .is_some());
    assert!(due_now(
        &event,
        now + Duration::hours(1) + Duration::seconds(61),
        &window,
        15
    )
    .unwrap()
    .is_none());
}

fn setup() -> (tempfile::TempDir, Store, String) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open_live(dir.path()).unwrap();
    let account = store
        .bind_google_account(
            "reminder-subject",
            "person@example.test",
            &BTreeSet::from(["https://www.googleapis.com/auth/calendar.readonly".into()]),
            None,
        )
        .unwrap();
    preferences(
        &mut store,
        json!({"calendar_display_time_zone":"America/New_York","notifications":"all"}),
    );
    catalog(
        &mut store,
        &account,
        vec![entry("a", "owner"), entry("b", "reader")],
    );
    (dir, store, account)
}
fn preferences(store: &mut Store, preferences: Value) {
    let revision = store.preferences().unwrap()["revision"].as_i64().unwrap();
    store.preferences_save(&json!({"request_id":format!("prefs-{revision}"),"expected_revision":revision,"preferences":preferences})).unwrap();
}
fn entry(id: &str, role: &str) -> Value {
    json!({"id":id,"summary":id,"timeZone":"UTC","accessRole":role})
}
fn stage(store: &mut Store, lease: &crate::live_calendar::CalendarSyncLease, items: Vec<Value>) {
    let page = CalendarPage {
        input_cursor: None,
        items,
        next_page_token: None,
        next_sync_token: Some("next".into()),
    };
    let mut offset = store.calendar_stage_page_chunk(lease, &page, 0).unwrap();
    while offset < page.items.len() {
        offset = store
            .calendar_stage_page_chunk(lease, &page, offset)
            .unwrap();
    }
    store.calendar_seal_page(lease, &page).unwrap();
}
fn catalog(store: &mut Store, account: &str, items: Vec<Value>) {
    let lease = store
        .calendar_catalog_begin(account, CalendarSyncMode::Full)
        .unwrap();
    stage(store, &lease, items);
    while store.calendar_catalog_finalize_chunk(&lease).unwrap() > 0 {}
    store.calendar_publish_sync(&lease).unwrap();
}
fn publish(
    store: &mut Store,
    account: &str,
    calendar: &str,
    items: Vec<Value>,
    mode: CalendarSyncMode,
) {
    let lease = store.calendar_sync_begin(account, calendar, mode).unwrap();
    stage(store, &lease, items);
    store.calendar_publish_sync(&lease).unwrap();
    while store.calendar_finalize_chunk(&lease).unwrap() > 0 {}
}
fn event(id: &str, start: &str) -> Value {
    let at = DateTime::parse_from_rfc3339(start).unwrap();
    json!({"id":id,"etag":format!("etag-{start}"),"summary":id,
        "start":{"dateTime":start},"end":{"dateTime":(at+Duration::hours(1)).to_rfc3339()},
        "organizer":{"email":"person@example.test","self":true},
        "attendees":[{"email":"person@example.test","self":true,"responseStatus":"accepted"}],
        "conferenceData":{"entryPoints":[{"entryPointType":"video","uri":"https://meet.google.com/aaa-bbbb-ccc"}]}})
}
fn reminders(store: &Store, now: &str) -> Value {
    store.calendar_reminders(&json!({"now":now})).unwrap()
}
fn ack(store: &Store, row: &Value, state: &str, now: &str) -> Result<Value> {
    store.calendar_reminder_ack(row["account"].as_str(), &json!({"calendar_id":row["calendar_id"],"occurrence_id":row["id"],"start":row["start"],"state":state,"now":now}))
}

#[test]
fn long_provider_and_occurrence_ids_survive_poll_show_snooze_and_dismiss() {
    let (_dir, mut store, account) = setup();
    // Google calendar identifiers and the serialized occurrence key are not
    // generic 256-byte local IDs. Exercise the projection and receipt together.
    let calendar = format!("{}@group.calendar.google.com", "a".repeat(280));
    catalog(&mut store, &account, vec![entry(&calendar, "owner")]);
    publish(
        &mut store,
        &account,
        &calendar,
        vec![event(&"e".repeat(200), "2026-09-14T13:00:00Z")],
        CalendarSyncMode::Full,
    );
    let due = reminders(&store, "2026-09-14T12:55:00Z");
    assert_eq!(due["reminders"].as_array().unwrap().len(), 1);
    let row = &due["reminders"][0];
    assert!(row["id"].as_str().unwrap().len() > 256);
    ack(&store, row, "shown", "2026-09-14T12:55:00Z").unwrap();
    ack(&store, row, "snoozed", "2026-09-14T12:55:00Z").unwrap();
    assert!(reminders(&store, "2026-09-14T12:56:00Z")["reminders"]
        .as_array()
        .unwrap()
        .is_empty());
    let resumed = reminders(&store, "2026-09-14T13:00:00Z");
    assert_eq!(resumed["reminders"][0]["id"], row["id"]);
    ack(&store, row, "shown", "2026-09-14T13:00:00Z").unwrap();
    ack(&store, row, "dismissed", "2026-09-14T13:00:00Z").unwrap();
    assert!(reminders(&store, "2026-09-14T13:00:00Z")["reminders"]
        .as_array()
        .unwrap()
        .is_empty());
    let mut invalid = row.clone();
    for identity in ["x".repeat(8193), "invalid\nidentity".into(), String::new()] {
        invalid["id"] = identity.into();
        assert_eq!(
            ack(&store, &invalid, "shown", "2026-09-14T13:00:00Z")
                .unwrap_err()
                .code,
            "invalid_request"
        );
    }
}

#[test]
fn exact_due_window_all_day_display_zone_and_no_historical_burst() {
    let (_dir, mut store, account) = setup();
    let all_day = json!({"id":"all-day","etag":"all-day-v1","summary":"All day","start":{"date":"2026-09-14"},"end":{"date":"2026-09-15"}});
    publish(
        &mut store,
        &account,
        "a",
        vec![
            event("timed", "2026-09-14T13:00:00Z"),
            all_day,
            event("old", "2026-09-14T10:00:00Z"),
        ],
        CalendarSyncMode::Full,
    );
    assert_eq!(
        reminders(&store, "2026-09-14T12:54:59.999Z")["reminders"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "events without their own reminder use the 15 minute default"
    );
    let soon = reminders(&store, "2026-09-14T12:55:00Z");
    assert_eq!(soon["reminders"].as_array().unwrap().len(), 2);
    assert!(soon["reminders"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["timing"] == "Soon" && v["due_at"] == "2026-09-14T13:00:00+00:00"));
    assert!(reminders(&store, "2026-09-14T13:00:00Z")["reminders"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["timing"] == "Now"));
    assert_eq!(
        reminders(&store, "2026-09-14T13:01:00Z")["reminders"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(reminders(&store, "2026-09-14T13:01:00.001Z")["reminders"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(reminders(&store, "2026-09-15T09:00:00Z")["reminders"]
        .as_array()
        .unwrap()
        .is_empty());
    let count: i64 = store
        .db
        .query_row("SELECT COUNT(*) FROM calendar_reminder_receipts", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 0, "read polling must not acknowledge or mark events");
}

#[test]
fn reminder_receipts_survive_reopen_and_reschedule_has_a_new_start_identity() {
    let (dir, mut store, account) = setup();
    publish(
        &mut store,
        &account,
        "a",
        vec![event("one", "2026-09-14T13:00:00Z")],
        CalendarSyncMode::Full,
    );
    let row = reminders(&store, "2026-09-14T12:59:00Z")["reminders"][0].clone();
    assert_eq!(
        ack(&store, &row, "shown", "2026-09-14T12:59:00Z").unwrap()["state"],
        "shown"
    );
    assert_eq!(
        reminders(&store, "2026-09-14T12:59:01Z")["reminders"][0]["reminder_state"],
        "shown"
    );
    assert_eq!(
        ack(&store, &row, "dismissed", "2026-09-14T12:59:01Z").unwrap()["state"],
        "dismissed"
    );
    assert_eq!(
        ack(&store, &row, "shown", "2026-09-14T12:59:02Z").unwrap()["state"],
        "dismissed"
    );
    assert!(reminders(&store, "2026-09-14T12:59:03Z")["reminders"]
        .as_array()
        .unwrap()
        .is_empty());
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    assert_eq!(
        ack(&store, &row, "dismissed", "2026-09-15T10:00:00Z").unwrap()["state"],
        "dismissed"
    );
    publish(
        &mut store,
        &account,
        "a",
        vec![event("one", "2026-09-14T13:30:00Z")],
        CalendarSyncMode::Delta,
    );
    assert_eq!(
        ack(&store, &row, "dismissed", "2026-09-14T13:26:00Z")
            .unwrap_err()
            .code,
        "stale_reminder"
    );
    let next = reminders(&store, "2026-09-14T13:26:00Z")["reminders"][0].clone();
    assert!(next["reminder_state"].is_null());
    assert_eq!(next["start"], "2026-09-14T13:30:00+00:00");
    assert_eq!(
        ack(&store, &next, "shown", "2026-09-14T13:26:00Z").unwrap()["state"],
        "shown"
    );
}

#[test]
fn google_events_with_reminders_turned_off_never_alert() {
    let (_dir, mut store, account) = setup();
    let mut off = event("off", "2026-09-14T13:00:00Z");
    off["reminders"] = json!({"useDefault":false,"overrides":[]});
    publish(
        &mut store,
        &account,
        "a",
        vec![off, event("default", "2026-09-14T13:00:00Z")],
        CalendarSyncMode::Full,
    );
    let due = reminders(&store, "2026-09-14T12:59:00Z");
    let titles: Vec<_> = due["reminders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["title"].clone())
        .collect();
    assert_eq!(titles, vec![json!("default")]);
}

#[test]
fn snoozed_reminder_returns_after_saved_delay_even_past_start() {
    let (dir, mut store, account) = setup();
    publish(
        &mut store,
        &account,
        "a",
        vec![event("one", "2026-09-14T13:00:00Z")],
        CalendarSyncMode::Full,
    );
    let row = reminders(&store, "2026-09-14T12:59:00Z")["reminders"][0].clone();
    ack(&store, &row, "shown", "2026-09-14T12:59:00Z").unwrap();
    assert_eq!(
        ack(&store, &row, "snoozed", "2026-09-14T12:59:02Z").unwrap()["state"],
        "snoozed"
    );
    drop(store);
    let store = Store::open_live(dir.path()).unwrap();
    assert!(reminders(&store, "2026-09-14T13:04:01Z")["reminders"]
        .as_array()
        .unwrap()
        .is_empty());
    let ready = reminders(&store, "2026-09-14T13:04:02Z")["reminders"][0].clone();
    assert!(ready["reminder_state"].is_null());
    assert_eq!(
        ack(&store, &ready, "shown", "2026-09-14T13:04:02Z").unwrap()["state"],
        "shown"
    );
    assert_eq!(
        ack(&store, &ready, "dismissed", "2026-09-14T13:04:03Z").unwrap()["state"],
        "dismissed"
    );
}

#[test]
fn late_snooze_returns_after_the_original_post_start_window() {
    let (_dir, mut store, account) = setup();
    publish(
        &mut store,
        &account,
        "a",
        vec![event("late", "2026-09-14T13:00:00Z")],
        CalendarSyncMode::Full,
    );
    let row = reminders(&store, "2026-09-14T12:59:00Z")["reminders"][0].clone();
    ack(&store, &row, "shown", "2026-09-14T12:59:00Z").unwrap();
    ack(&store, &row, "snoozed", "2026-09-14T13:59:00Z").unwrap();
    assert!(reminders(&store, "2026-09-14T14:03:59Z")["reminders"]
        .as_array()
        .unwrap()
        .is_empty());
    let resumed = reminders(&store, "2026-09-14T14:04:00Z")["reminders"][0].clone();
    assert!(resumed["reminder_state"].is_null());
    assert_eq!(
        ack(&store, &resumed, "shown", "2026-09-14T14:04:00Z").unwrap()["state"],
        "shown"
    );
}

#[test]
fn current_permissions_declines_calendar_selection_and_removal_filter_reminders() {
    let (_dir, mut store, account) = setup();
    let mut declined = event("declined", "2026-09-14T13:00:00Z");
    declined["attendees"][0]["responseStatus"] = "declined".into();
    let mut private = event("private", "2026-09-14T13:00:00Z");
    private["visibility"] = "private".into();
    publish(
        &mut store,
        &account,
        "a",
        vec![event("allowed", "2026-09-14T13:00:00Z"), declined],
        CalendarSyncMode::Full,
    );
    publish(
        &mut store,
        &account,
        "b",
        vec![private, event("other", "2026-09-14T13:00:00Z")],
        CalendarSyncMode::Full,
    );
    let rows = reminders(&store, "2026-09-14T12:59:00Z")["reminders"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(rows.len(), 2);
    assert!(!rows
        .iter()
        .any(|v| v["title"] == "declined" || v["title"] == "private" || v["busy_only"] == true));
    let other = rows
        .iter()
        .find(|v| v["calendar_id"] == "b")
        .unwrap()
        .clone();
    preferences(
        &mut store,
        json!({"selected_calendars":[{"account":account,"calendar_id":"a"}]}),
    );
    assert_eq!(
        reminders(&store, "2026-09-14T12:59:00Z")["reminders"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        ack(&store, &other, "dismissed", "2026-09-14T12:59:00Z")
            .unwrap_err()
            .code,
        "stale_reminder"
    );
    let allowed = reminders(&store, "2026-09-14T12:59:00Z")["reminders"][0].clone();
    catalog(
        &mut store,
        &account,
        vec![entry("a", "freeBusyReader"), entry("b", "reader")],
    );
    assert!(reminders(&store, "2026-09-14T12:59:00Z")["reminders"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        ack(&store, &allowed, "dismissed", "2026-09-14T12:59:00Z")
            .unwrap_err()
            .code,
        "stale_reminder"
    );
    store
        .account_remove_begin(
            &account,
            &json!({"expected_revision":1,"request_id":"remove","retention":"archive"}),
        )
        .unwrap();
    assert_eq!(
        ack(&store, &allowed, "dismissed", "2026-09-14T12:59:00Z")
            .unwrap_err()
            .code,
        "account_removed"
    );
}

#[test]
fn shown_requires_due_enabled_alert_but_explicit_join_dismiss_uses_current_event() {
    let (_dir, mut store, account) = setup();
    publish(
        &mut store,
        &account,
        "a",
        vec![event("join", "2026-09-14T13:00:00Z")],
        CalendarSyncMode::Full,
    );
    let row = reminders(&store, "2026-09-14T12:59:00Z")["reminders"][0].clone();
    assert_eq!(
        ack(&store, &row, "shown", "2026-09-14T10:00:00Z")
            .unwrap_err()
            .code,
        "stale_reminder"
    );
    preferences(&mut store, json!({"notifications":"off"}));
    assert!(reminders(&store, "2026-09-14T12:59:00Z")["reminders"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        ack(&store, &row, "shown", "2026-09-14T12:59:00Z")
            .unwrap_err()
            .code,
        "stale_reminder"
    );
    let mut changed = event("join", "2026-09-14T13:00:00Z");
    changed["etag"] = "new-conference".into();
    changed["conferenceData"]["entryPoints"][0]["uri"] =
        "https://meet.google.com/ddd-eeee-fff".into();
    publish(
        &mut store,
        &account,
        "a",
        vec![changed],
        CalendarSyncMode::Delta,
    );
    let current = ack(&store, &row, "dismissed", "2026-09-14T10:00:00Z").unwrap();
    assert_eq!(
        current["event"]["conference_url"],
        "https://meet.google.com/ddd-eeee-fff"
    );
    assert_eq!(current["remote_data_changed"], false);
    assert_eq!(
        ack(&store, &row, "dismissed", "2026-09-15T10:00:00Z").unwrap()["state"],
        "dismissed"
    );
}

fn message(store: &Store, account: &str, id: &str, at: &str, flags: (bool, bool, bool, bool)) {
    store
        .db
        .execute(
            "INSERT OR IGNORE INTO conversations(account,id,subject) VALUES(?1,'mail','Inbox')",
            [account],
        )
        .unwrap();
    store.db.execute("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash,trashed) VALUES(?1,?2,'mail',?2,'Sender','sender@example.test','person@example.test',?2,'retained content',?3,?4,0,?5,?6,'test-source',?7)",params![account,id,at,flags.0,flags.1,flags.2,flags.3]).unwrap();
}
fn notifications(store: &Store, after: i64, since: &str) -> Value {
    store
        .mail_notifications(&json!({"after":after,"since":since,"now":"2026-09-14T13:00:00Z"}))
        .unwrap()
}

#[test]
fn mail_cursor_starts_at_current_max_and_pages_history_without_historical_bursts() {
    let (_dir, store, account) = setup();
    message(
        &store,
        &account,
        "already-here",
        "2026-09-14T12:59:00Z",
        (true, true, false, false),
    );
    let initial = store.mail_notifications(&json!({})).unwrap();
    assert!(initial["messages"].as_array().unwrap().is_empty());
    let after = initial["after"].as_i64().unwrap();
    store.db.execute_batch("BEGIN IMMEDIATE").unwrap();
    for index in 0..1001 {
        message(
            &store,
            &account,
            &format!("history-{index}"),
            "2025-09-14T12:59:00Z",
            (true, true, false, false),
        );
    }
    message(
        &store,
        &account,
        "fresh",
        "2026-09-14T12:59:59Z",
        (true, true, false, false),
    );
    store.db.execute_batch("COMMIT").unwrap();
    let first = notifications(&store, after, "2026-09-14T12:59:30Z");
    assert_eq!(first["after"], after + 1000);
    assert_eq!(first["more"], true);
    assert!(first["messages"].as_array().unwrap().is_empty());
    let second = notifications(
        &store,
        first["after"].as_i64().unwrap(),
        "2026-09-14T12:59:30Z",
    );
    assert_eq!(second["messages"].as_array().unwrap().len(), 1);
    assert_eq!(second["messages"][0]["id"], "fresh");
    assert_eq!(second["more"], false);
    assert!(notifications(
        &store,
        second["after"].as_i64().unwrap(),
        "2026-09-14T12:59:30Z"
    )["messages"]
        .as_array()
        .unwrap()
        .is_empty());
    let body: String = store
        .db
        .query_row(
            "SELECT body FROM messages WHERE account=?1 AND id='fresh'",
            [account],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(body, "retained content");
}

#[test]
fn mail_alerts_skip_nonincoming_and_archived_accounts_and_advance_when_disabled() {
    let (_dir, mut store, account) = setup();
    let other = store
        .bind_google_account(
            "other-reminder-subject",
            "other@example.test",
            &BTreeSet::new(),
            None,
        )
        .unwrap();
    let initial = store.mail_notifications(&json!({})).unwrap()["after"]
        .as_i64()
        .unwrap();
    for (id, flags) in [
        ("read", (false, true, false, false)),
        ("sent", (true, true, true, false)),
        ("filed", (true, false, false, false)),
        ("trash", (true, true, false, true)),
        ("valid", (true, true, false, false)),
    ] {
        message(&store, &account, id, "2026-09-14T12:59:59Z", flags);
    }
    message(
        &store,
        &other,
        "valid",
        "2026-09-14T12:59:59Z",
        (true, true, false, false),
    );
    message(
        &store,
        &account,
        "future",
        "2026-09-14T13:02:00Z",
        (true, true, false, false),
    );
    message(
        &store,
        &account,
        "recent-before-session",
        "2026-09-14T12:59:00Z",
        (true, true, false, false),
    );
    store
        .account_remove_begin(
            &other,
            &json!({"expected_revision":1,"request_id":"remove","retention":"archive"}),
        )
        .unwrap();
    let result = notifications(&store, initial, "2026-09-14T12:59:30Z");
    assert_eq!(result["messages"].as_array().unwrap().len(), 1);
    assert_eq!(result["messages"][0]["account"], account);
    assert_eq!(result["messages"][0]["id"], "valid");
    let mut after = result["after"].as_i64().unwrap();
    for setting in ["events", "off"] {
        preferences(&mut store, json!({"notifications":setting}));
        message(
            &store,
            &account,
            setting,
            "2026-09-14T12:59:59Z",
            (true, true, false, false),
        );
        let result = notifications(&store, after, "2026-09-14T12:59:30Z");
        assert!(result["messages"].as_array().unwrap().is_empty());
        assert!(result["after"].as_i64().unwrap() > after);
        after = result["after"].as_i64().unwrap();
    }
    preferences(&mut store, json!({"notifications":"all"}));
    assert!(
        notifications(&store, after, "2026-09-14T12:59:30Z")["messages"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
