use super::*;
use crate::live_calendar::{CalendarPage, CalendarSyncMode};
use base64::Engine as _;

fn wrapped_base64(text: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    encoded
        .as_bytes()
        .chunks(76)
        .map(|line| std::str::from_utf8(line).unwrap())
        .collect::<Vec<_>>()
        .join("\r\n")
}

/// Google's shape: text/calendar inside multipart/alternative plus invite.ics.
fn google_mime(calendar: &str, attachment: &str) -> Vec<u8> {
    format!("From: Org Person <org@example.test>\r\nTo: me@example.test\r\nSubject: Invitation: Design review\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"mixed\"\r\n\r\n--mixed\r\nContent-Type: multipart/alternative; boundary=\"alt\"\r\n\r\n--alt\r\nContent-Type: text/plain; charset=\"UTF-8\"\r\n\r\nYou have been invited.\r\n--alt\r\nContent-Type: text/html; charset=\"UTF-8\"\r\n\r\n<p>You have been invited.</p>\r\n--alt\r\nContent-Type: text/calendar; charset=\"UTF-8\"; method=REQUEST\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n--alt--\r\n--mixed\r\nContent-Type: application/ics; name=\"invite.ics\"\r\nContent-Disposition: attachment; filename=\"invite.ics\"\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n--mixed--\r\n",
        wrapped_base64(calendar), wrapped_base64(attachment)).into_bytes()
}

/// Outlook's shape: text/calendar as the last multipart/alternative part.
fn outlook_mime(calendar: &str) -> Vec<u8> {
    format!("From: Pat Organizer <pat@example.test>\r\nTo: me@example.test\r\nSubject: Budget sync\r\nMIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary=\"outlook\"\r\n\r\n--outlook\r\nContent-Type: text/plain; charset=\"utf-8\"\r\n\r\nAgenda\r\n--outlook\r\nContent-Type: text/calendar; charset=\"utf-8\"; method=REQUEST\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n--outlook--\r\n",
        wrapped_base64(calendar)).into_bytes()
}

fn calendar(method: &str, uid: &str, sequence: u32, times: &str) -> String {
    format!("BEGIN:VCALENDAR\r\nPRODID:-//Google Inc//Google Calendar 70.9054//EN\r\nVERSION:2.0\r\nCALSCALE:GREGORIAN\r\nMETHOD:{method}\r\nBEGIN:VEVENT\r\n{times}\r\nDTSTAMP:20260920T100000Z\r\nORGANIZER;CN=Org Person:mailto:org@example.test\r\nUID:{uid}\r\nSEQUENCE:{sequence}\r\nSTATUS:CONFIRMED\r\nSUMMARY:Design review\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n")
}

/// Conversation, message ID, sender and sent time.
struct Mail<'a>(&'a str, &'a str, &'a str, &'a str);

/// A received message with, as Gmail sync stores it, its calendar file.
fn add_mail(store: &Store, account: &str, mail: Mail<'_>, source: &[u8], ics: Option<&str>) {
    let Mail(conversation, id, sender, sent_at) = mail;
    let hash = store.stage_blob(source).unwrap();
    store
        .db
        .execute(
            "INSERT OR IGNORE INTO conversations(account,id,subject) VALUES(?1,?2,'Invitation')",
            params![account, conversation],
        )
        .unwrap();
    store.db.execute("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash) VALUES(?1,?2,?3,?2,'Organizer',?4,'me@example.test','Invitation','',?5,1,0,1,0,?6)",
        params![account, id, conversation, sender, sent_at, hash]).unwrap();
    if let Some(ics) = ics {
        let part = store.stage_blob(ics.as_bytes()).unwrap();
        store.db.execute("INSERT INTO attachments(account,id,message_id,name,mime_type,size,state,blob_hash) VALUES(?1,?2,?3,'invite.ics','application/ics',?4,'available',?5)",
            params![account, format!("{id}-ics"), id, ics.len() as i64, part]).unwrap();
    }
}

fn version(store: &Store, account: &str, id: &str) -> String {
    store
        .db
        .query_row(
            "SELECT source_hash FROM messages WHERE account=?1 AND id=?2",
            params![account, id],
            |r| r.get(0),
        )
        .unwrap()
}

fn invite(store: &Store, account: &str, id: &str) -> Value {
    let request = json!({"id":id,"source_version":version(store, account, id)});
    let plan = store.message_invite_begin(account, &request).unwrap();
    store
        .message_invite_finish(prepare(plan, &CancellationToken::new()).unwrap())
        .unwrap()
}

fn source(store: &Store, account: &str, id: &str) -> Value {
    let request = json!({"id":id,"source_version":version(store, account, id)});
    crate::message_source::read(
        store.message_source_begin(account, &request).unwrap(),
        &CancellationToken::new(),
    )
    .unwrap()
}

fn add_calendar(store: &Store, account: &str, calendar: &str) {
    store.db.execute("INSERT INTO calendar_sync_state(account,calendar_id,query_fingerprint,time_zone,access,sync_token,revision) VALUES(?1,?2,'fixture-calendar-v1','America/Toronto','reader',NULL,0)",
        params![account, calendar]).unwrap();
}

fn add_event(store: &Store, account: &str, calendar: &str, source: &Value) {
    store
        .db
        .execute(
            "INSERT INTO calendar_sources(account,calendar_id,event_id,json) VALUES(?1,?2,?3,?4)",
            params![
                account,
                calendar,
                source["id"].as_str().unwrap(),
                source.to_string()
            ],
        )
        .unwrap();
}

fn account_email(store: &Store, account: &str) -> String {
    store
        .db
        .query_row("SELECT email FROM accounts WHERE id=?1", [account], |r| {
            r.get(0)
        })
        .unwrap()
}

/// The combined Calendar's row for one occurrence of the personal primary calendar.
fn calendar_row(
    store: &Store,
    from: &str,
    to: &str,
    series: &str,
    original: Option<&str>,
) -> Value {
    let combined = store
        .calendar_combined(&json!({"from":from,"to":to}))
        .unwrap();
    combined["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| {
            row["account"] == "personal"
                && row["calendar_id"] == "primary"
                && row["occurrence_key"]["series"] == series
                && row["occurrence_key"]["original_start"].as_str() == original
        })
        .unwrap_or_else(|| panic!("no row for {series} {original:?}: {combined}"))
        .clone()
}

#[test]
fn google_invitation_links_its_event_as_the_calendar_projects_it_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let email = account_email(&store, "personal");
    let event = json!({"id":"inv1","iCalUID":"abc123@google.com","etag":"\"v1\"","summary":"Design review",
        "organizer":{"email":"org@example.test","displayName":"Org Person"},
        "attendees":[{"email":email,"self":true,"responseStatus":"needsAction"},{"email":"org@example.test","organizer":true,"responseStatus":"accepted"}],
        "start":{"dateTime":"2026-09-29T15:00:00-04:00","timeZone":"America/Toronto"},
        "end":{"dateTime":"2026-09-29T16:00:00-04:00","timeZone":"America/Toronto"}});
    // A shared calendar's copy where the account is not a guest loses to primary.
    add_calendar(&store, "personal", "team@group.calendar.google.com");
    let mut shared = event.clone();
    shared["attendees"] = json!([{"email":"org@example.test","organizer":true}]);
    add_event(
        &store,
        "personal",
        "team@group.calendar.google.com",
        &shared,
    );
    add_event(&store, "personal", "primary", &event);
    let text = calendar(
        "REQUEST",
        "abc123@google.com",
        0,
        "DTSTART;TZID=America/Toronto:20260929T150000\r\nDTEND;TZID=America/Toronto:20260929T160000",
    );
    add_mail(
        &store,
        "personal",
        Mail(
            "inv-thread",
            "inv-message",
            "org@example.test",
            "2026-09-20T10:00:00Z",
        ),
        &google_mime(&text, &text),
        Some(&text),
    );
    let before = store.db.total_changes();
    let result = invite(&store, "personal", "inv-message");
    assert_eq!(store.db.total_changes(), before);
    let invite = &result["invite"];
    assert_eq!(invite["uid"], "abc123@google.com");
    assert_eq!(invite["method"], "REQUEST");
    assert_eq!(invite["sequence"], 0);
    assert_eq!(invite["summary"], "Design review");
    assert_eq!(invite["recurrence_id"], Value::Null);
    assert_eq!(invite["start"], "2026-09-29T19:00:00Z");
    assert_eq!(invite["end"], "2026-09-29T20:00:00Z");
    assert_eq!(invite["all_day"], false);
    assert_eq!(invite["was_start"], Value::Null);
    assert_eq!(invite["was_end"], Value::Null);
    assert_eq!(invite["was_all_day"], Value::Null);
    let search = store
        .calendar_search(&json!({"query":"Design review","from":"2026-09-28","to":"2026-10-01"}))
        .unwrap();
    let row = search["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["calendar_id"] == "primary")
        .unwrap();
    assert_eq!(invite["event"], *row, "the GUI receives the same event row");
    for (key, value) in [
        ("account", json!("personal")),
        ("calendar_id", json!("primary")),
        ("event_id", json!("inv1")),
        ("etag", json!("\"v1\"")),
        ("can_rsvp", json!(true)),
        ("self_response", json!("needsAction")),
        ("recurring", json!(false)),
    ] {
        assert_eq!(invite["event"][key], value, "{key}");
    }
    assert_eq!(
        source(&store, "personal", "inv-message")["has_invite"],
        true
    );
}

#[test]
fn updated_invitation_reports_the_latest_earlier_different_time() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let times = |start: &str, end: &str| format!("DTSTART:{start}\r\nDTEND:{end}");
    let first = calendar(
        "REQUEST",
        "moved@google.com",
        0,
        &times("20260929T190000Z", "20260929T200000Z"),
    );
    let other = calendar(
        "REQUEST",
        "other@google.com",
        0,
        &times("20260101T100000Z", "20260101T110000Z"),
    );
    let moved = calendar(
        "REQUEST",
        "moved@google.com",
        1,
        &times("20260929T210000Z", "20260929T220000Z"),
    );
    let retitled = calendar(
        "REQUEST",
        "moved@google.com",
        2,
        &times("20260929T210000Z", "20260929T220000Z"),
    );
    let cancelled = calendar(
        "CANCEL",
        "moved@google.com",
        3,
        &times("20260929T210000Z", "20260929T220000Z"),
    );
    let later = calendar(
        "REQUEST",
        "moved@google.com",
        4,
        &times("20260929T230000Z", "20260929T233000Z"),
    );
    for (conversation, id, sender, sent_at, text) in [
        // Found through the conversation: another sender.
        (
            "update-thread",
            "first",
            "calendar@example.test",
            "2026-09-01T10:00:00Z",
            &first,
        ),
        (
            "other-thread",
            "other",
            "org@example.test",
            "2026-09-02T10:00:00Z",
            &other,
        ),
        (
            "update-thread",
            "moved",
            "org@example.test",
            "2026-09-03T10:00:00Z",
            &moved,
        ),
        (
            "update-thread",
            "retitled",
            "ORG@example.test",
            "2026-09-04T10:00:00Z",
            &retitled,
        ),
        (
            "update-thread",
            "cancelled",
            "org@example.test",
            "2026-09-05T10:00:00Z",
            &cancelled,
        ),
        // Found through the sender: another conversation.
        (
            "later-thread",
            "later",
            "org@example.test",
            "2026-09-06T10:00:00Z",
            &later,
        ),
    ] {
        add_mail(
            &store,
            "personal",
            Mail(conversation, id, sender, sent_at),
            &google_mime(text, text),
            Some(text),
        );
    }
    let was = |id: &str| {
        let value = invite(&store, "personal", id)["invite"].clone();
        (
            value["was_start"].as_str().map(str::to_owned),
            value["was_end"].as_str().map(str::to_owned),
        )
    };
    assert_eq!(was("first"), (None, None));
    let earlier = (
        Some("2026-09-29T19:00:00Z".to_owned()),
        Some("2026-09-29T20:00:00Z".to_owned()),
    );
    assert_eq!(was("moved"), earlier);
    // The unchanged previous request is skipped for the last different time.
    assert_eq!(was("retitled"), earlier);
    let value = invite(&store, "personal", "cancelled")["invite"].clone();
    assert_eq!(value["method"], "CANCEL");
    assert_eq!(value["was_start"], Value::Null);
    let value = invite(&store, "personal", "later")["invite"].clone();
    assert_eq!(value["start"], "2026-09-29T23:00:00Z");
    assert_eq!(value["was_start"], "2026-09-29T21:00:00Z");
    assert_eq!(value["was_end"], "2026-09-29T22:00:00Z");
    assert_eq!(value["was_all_day"], false);
    assert_eq!(value["event"], Value::Null);
}

#[test]
fn ordinary_malformed_and_changed_messages_have_no_invitation() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    add_mail(
        &store,
        "personal",
        Mail("plain", "plain", "a@example.test", "2026-09-01T10:00:00Z"),
        b"From: A <a@example.test>\r\nSubject: Hello\r\nContent-Type: text/plain\r\n\r\nNo calendar here.",
        None,
    );
    assert_eq!(invite(&store, "personal", "plain"), json!({"invite":null}));
    assert_eq!(source(&store, "personal", "plain")["has_invite"], false);
    let broken = "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nDTSTART:20260929T190000Z\r\nEND:VTODO\r\nEND:VCALENDAR\r\n";
    add_mail(
        &store,
        "personal",
        Mail("broken", "broken", "a@example.test", "2026-09-02T10:00:00Z"),
        &google_mime(broken, broken),
        Some(broken),
    );
    assert_eq!(invite(&store, "personal", "broken"), json!({"invite":null}));
    assert_eq!(source(&store, "personal", "broken")["has_invite"], false);
    // A broken inline part falls back to a usable invite.ics.
    let all_day = calendar(
        "REQUEST",
        "day@google.com",
        0,
        "DTSTART;VALUE=DATE:20261010\r\nDTEND;VALUE=DATE:20261011",
    );
    add_mail(
        &store,
        "personal",
        Mail("day", "day", "a@example.test", "2026-09-03T10:00:00Z"),
        &google_mime(broken, &all_day),
        Some(&all_day),
    );
    let value = invite(&store, "personal", "day")["invite"].clone();
    assert_eq!(
        (
            &value["uid"],
            &value["start"],
            &value["end"],
            &value["all_day"],
            &value["event"]
        ),
        (
            &json!("day@google.com"),
            &json!("2026-10-10"),
            &json!("2026-10-11"),
            &json!(true),
            &Value::Null
        )
    );
    assert_eq!(source(&store, "personal", "day")["has_invite"], true);
    let request = json!({"id":"day","source_version":version(&store, "personal", "day")});
    assert_eq!(
        store
            .message_invite_begin("windscribe", &request)
            .err()
            .unwrap()
            .code,
        "not_found"
    );
    assert_eq!(
        store
            .message_invite_begin("personal", &json!({"id":"day","source_version":"old"}))
            .err()
            .unwrap()
            .code,
        "stale_source"
    );
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(prepare(
        store.message_invite_begin("personal", &request).unwrap(),
        &cancel
    )
    .is_err());
    let prepared = prepare(
        store.message_invite_begin("personal", &request).unwrap(),
        &CancellationToken::new(),
    )
    .unwrap();
    store
        .db
        .execute(
            "UPDATE messages SET source_hash=?1 WHERE account='personal' AND id='day'",
            [version(&store, "personal", "plain")],
        )
        .unwrap();
    assert_eq!(
        store.message_invite_finish(prepared).err().unwrap().code,
        "stale_source"
    );
}

#[test]
fn outlook_windows_zone_names_resolve_and_unknown_zones_leave_no_time() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let text = "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nPRODID:Microsoft Exchange Server 2010\r\nVERSION:2.0\r\nBEGIN:VTIMEZONE\r\nTZID:Eastern Standard Time\r\nBEGIN:STANDARD\r\nDTSTART:16010101T020000\r\nTZOFFSETFROM:-0400\r\nTZOFFSETTO:-0500\r\nRRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=1SU;BYMONTH=11\r\nEND:STANDARD\r\nBEGIN:DAYLIGHT\r\nDTSTART:16010101T020000\r\nTZOFFSETFROM:-0500\r\nTZOFFSETTO:-0400\r\nRRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=2SU;BYMONTH=3\r\nEND:DAYLIGHT\r\nEND:VTIMEZONE\r\nBEGIN:VEVENT\r\nORGANIZER;CN=Pat Organizer:mailto:pat@example.test\r\nATTENDEE;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION;RSVP=TRUE;CN=Me:mailto:me@example.test\r\nDESCRIPTION;LANGUAGE=en-US:Agenda\\nItems\r\nUID:040000008200E00074C5B7101A82E00800000000A0B1C2D3E4F5A6B7\r\nSUMMARY;LANGUAGE=en-US:Budget sync\r\nDTSTART;TZID=Eastern Standard Time:20261105T090000\r\nDTEND;TZID=Eastern Standard Time:20261105T093000\r\nSEQUENCE:0\r\nBEGIN:VALARM\r\nACTION:DISPLAY\r\nTRIGGER:-PT15M\r\nEND:VALARM\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let summer = text.replace("20261105", "20261015");
    let custom = text.replace("Eastern Standard Time", "Customized Time Zone");
    for (id, text) in [
        ("winter", text),
        ("summer", summer.as_str()),
        ("custom", custom.as_str()),
    ] {
        add_mail(
            &store,
            "personal",
            Mail(id, id, "pat@example.test", "2026-09-01T10:00:00Z"),
            &outlook_mime(text),
            None,
        );
        assert_eq!(source(&store, "personal", id)["has_invite"], true);
    }
    let value = invite(&store, "personal", "winter")["invite"].clone();
    assert_eq!(
        value["uid"],
        "040000008200E00074C5B7101A82E00800000000A0B1C2D3E4F5A6B7"
    );
    assert_eq!(value["summary"], "Budget sync");
    assert_eq!(value["method"], "REQUEST");
    assert_eq!(value["start"], "2026-11-05T14:00:00Z");
    assert_eq!(value["end"], "2026-11-05T14:30:00Z");
    assert_eq!(value["event"], Value::Null);
    let value = invite(&store, "personal", "summer")["invite"].clone();
    assert_eq!(value["start"], "2026-10-15T13:00:00Z");
    // A custom zone with daylight rules is not guessed at.
    let value = invite(&store, "personal", "custom")["invite"].clone();
    assert_eq!(value["summary"], "Budget sync");
    assert_eq!(
        (&value["start"], &value["end"]),
        (&Value::Null, &Value::Null)
    );
}

#[test]
fn recurring_invitations_link_the_named_or_next_occurrence() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let email = account_email(&store, "personal");
    let zoned = |at: &str| json!({"dateTime":at,"timeZone":"America/Toronto"});
    add_event(
        &store,
        "personal",
        "primary",
        &json!({"id":"weekly","iCalUID":"series@google.com","etag":"\"v1\"","summary":"Weekly sync",
        "recurrence":["RRULE:FREQ=WEEKLY;COUNT=10"],"attendees":[{"email":email,"self":true,"responseStatus":"accepted"}],
        "start":zoned("2026-09-14T10:00:00-04:00"),"end":zoned("2026-09-14T10:30:00-04:00")}),
    );
    add_event(
        &store,
        "personal",
        "primary",
        &json!({"id":"weekly_20260928T140000Z","iCalUID":"series@google.com","etag":"\"v2\"","summary":"Weekly sync (moved)",
        "recurringEventId":"weekly","originalStartTime":zoned("2026-09-28T10:00:00-04:00"),
        "start":zoned("2026-09-28T13:00:00-04:00"),"end":zoned("2026-09-28T13:30:00-04:00")}),
    );
    add_event(
        &store,
        "personal",
        "primary",
        &json!({"id":"weekly_20261012T140000Z","iCalUID":"series@google.com","status":"cancelled",
        "recurringEventId":"weekly","originalStartTime":zoned("2026-10-12T10:00:00-04:00")}),
    );
    let series = "DTSTART;TZID=America/Toronto:20260914T100000\r\nDTEND;TZID=America/Toronto:20260914T103000\r\nRRULE:FREQ=WEEKLY;COUNT=10";
    let instance = |original: &str, start: &str| {
        format!("RECURRENCE-ID;TZID=America/Toronto:{original}\r\nDTSTART;TZID=America/Toronto:{start}\r\nDURATION:PT30M")
    };
    for (id, times) in [
        ("series", series.to_owned()),
        ("moved", instance("20260928T100000", "20260928T130000")),
        ("plain", instance("20261005T100000", "20261005T100000")),
        ("cancelled", instance("20261012T100000", "20261012T100000")),
        ("outside", instance("20270104T100000", "20270104T100000")),
    ] {
        let text = calendar("REQUEST", "series@google.com", 1, &times);
        add_mail(
            &store,
            "personal",
            Mail(id, id, "org@example.test", "2026-09-01T10:00:00Z"),
            &google_mime(&text, &text),
            Some(&text),
        );
    }
    // The demo profile's today is 2026-09-11: the first occurrence is next.
    let value = invite(&store, "personal", "series")["invite"].clone();
    assert_eq!(value["recurrence_id"], Value::Null);
    assert_eq!(value["start"], "2026-09-14T14:00:00Z");
    assert_eq!(
        value["event"],
        calendar_row(
            &store,
            "2026-09-13",
            "2026-09-16",
            "weekly",
            Some("2026-09-14T14:00:00+00:00")
        )
    );
    assert_eq!(value["event"]["recurring"], true);
    let value = invite(&store, "personal", "moved")["invite"].clone();
    assert_eq!(value["recurrence_id"], "2026-09-28T14:00:00Z");
    assert_eq!(value["start"], "2026-09-28T17:00:00Z");
    assert_eq!(value["event"]["title"], "Weekly sync (moved)");
    assert_eq!(
        value["event"],
        calendar_row(
            &store,
            "2026-09-27",
            "2026-09-30",
            "weekly",
            Some("2026-09-28T14:00:00+00:00")
        )
    );
    let value = invite(&store, "personal", "plain")["invite"].clone();
    assert_eq!(value["start"], "2026-10-05T14:00:00Z");
    assert_eq!(
        value["event"],
        calendar_row(
            &store,
            "2026-10-04",
            "2026-10-07",
            "weekly",
            Some("2026-10-05T14:00:00+00:00")
        )
    );
    // No event for a cancelled or non-existent occurrence; the email's time stays.
    for id in ["cancelled", "outside"] {
        let value = invite(&store, "personal", id)["invite"].clone();
        assert_eq!(value["event"], Value::Null, "{id}");
        assert!(value["start"].is_string(), "{id}");
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
fn live_invitation_prefers_the_published_event_and_its_time() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open_live(dir.path()).unwrap();
    let account = store
        .bind_google_account(
            "invite-subject",
            "me@example.test",
            &BTreeSet::from(["https://www.googleapis.com/auth/calendar.readonly".into()]),
            None,
        )
        .unwrap();
    store.preferences_save(&json!({"request_id":"invite-zone","expected_revision":0,"preferences":{"calendar_display_time_zone":"UTC"}})).unwrap();
    let catalog = store
        .calendar_catalog_begin(&account, CalendarSyncMode::Full)
        .unwrap();
    publish(
        &mut store,
        &catalog,
        vec![
            json!({"id":"me@example.test","summary":"me@example.test","timeZone":"UTC","accessRole":"owner","primary":true}),
        ],
    );
    while store.calendar_catalog_finalize_chunk(&catalog).unwrap() > 0 {}
    store.calendar_publish_sync(&catalog).unwrap();
    let lease = store
        .calendar_sync_begin(&account, "me@example.test", CalendarSyncMode::Full)
        .unwrap();
    publish(
        &mut store,
        &lease,
        vec![
            json!({"id":"live1","iCalUID":"live@google.com","etag":"\"1\"","status":"confirmed","summary":"Live review",
            "organizer":{"email":"org@example.test"},
            "attendees":[{"email":"me@example.test","self":true,"responseStatus":"needsAction"}],
            "start":{"dateTime":"2026-10-01T09:00:00Z","timeZone":"UTC"},"end":{"dateTime":"2026-10-01T10:00:00Z","timeZone":"UTC"}}),
        ],
    );
    store.calendar_publish_sync(&lease).unwrap();
    while store.calendar_finalize_chunk(&lease).unwrap() > 0 {}
    // The email predates a move that Google already has.
    let text = calendar(
        "REQUEST",
        "live@google.com",
        0,
        "DTSTART:20261001T080000Z\r\nDTEND:20261001T090000Z",
    );
    add_mail(
        &store,
        &account,
        Mail(
            "live-thread",
            "live-message",
            "org@example.test",
            "2026-09-20T10:00:00Z",
        ),
        &google_mime(&text, &text),
        Some(&text),
    );
    let before = store.db.total_changes();
    let value = invite(&store, &account, "live-message")["invite"].clone();
    assert_eq!(store.db.total_changes(), before);
    let search = store
        .calendar_search(&json!({"query":"Live review","from":"2026-09-30","to":"2026-10-03"}))
        .unwrap();
    assert_eq!(value["event"], search["events"][0]);
    assert_eq!(value["event"]["calendar_id"], "me@example.test");
    assert_eq!(value["event"]["event_id"], "live1");
    assert_eq!(value["start"], "2026-10-01T09:00:00Z");
    assert_eq!(value["end"], "2026-10-01T10:00:00Z");
}
