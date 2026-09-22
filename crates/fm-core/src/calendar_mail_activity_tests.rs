use super::*;

fn insert(store: &Store, account: &str, id: &str, stamp: &str, sent: bool) {
    store
        .db
        .execute(
            "INSERT INTO conversations(account,id,subject) VALUES(?1,?2,'Count fixture')",
            params![account, id],
        )
        .unwrap();
    store.db.execute("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash,revision) VALUES(?1,?2,?2,?2,'Private','private@example.test','recipient@example.test','Private subject','Private body',?3,1,1,1,?4,'',1)",params![account,id,stamp,sent]).unwrap();
}
fn query(store: &mut Store, account: Option<&str>, from: &str, to: &str) -> Value {
    store
        .handle(
            "calendar_mail_activity",
            account,
            json!({"from":from,"to":to}),
        )
        .unwrap()
}
#[test]
fn calendar_mail_activity_counts_messages_in_local_days_without_changes() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    // Toronto midnight, a 23-hour spring day, and both copies of fall's 01:30.
    for (id, stamp, sent) in [
        ("before", "2026-03-08T04:59:59Z", false),
        ("start", "2026-03-08T05:00:00Z", false),
        ("last", "2026-03-09T03:59:59Z", true),
        ("next", "2026-03-09T04:00:00Z", false),
        ("fall-a", "2026-11-01T01:30:00-04:00", false),
        ("fall-b", "2026-11-01T01:30:00-05:00", true),
    ] {
        insert(&store, "personal", id, stamp, sent);
    }
    insert(
        &store,
        "windscribe",
        "other-account",
        "2026-03-08T18:00:00Z",
        true,
    );
    let changes = store.db.total_changes();
    let revision = store.revision().unwrap();
    let scoped = query(&mut store, Some("personal"), "2026-03-08", "2026-03-10");
    assert_eq!(
        scoped["days"],
        json!([{"date":"2026-03-08","received":1,"sent":1},{"date":"2026-03-09","received":1,"sent":0}])
    );
    assert_eq!(
        query(&mut store, None, "2026-03-08", "2026-03-09")["days"][0]["sent"],
        2
    );
    assert_eq!(
        query(&mut store, Some("personal"), "2026-11-01", "2026-11-02")["days"][0],
        json!({"date":"2026-11-01","received":1,"sent":1})
    );
    assert_eq!(store.db.total_changes(), changes);
    assert_eq!(store.revision().unwrap(), revision);
    assert!(!scoped.to_string().contains("Private"));
    assert_eq!(scoped["cached_only"], true);
}
#[test]
fn calendar_mail_activity_prefers_provider_date_and_excludes_removed_accounts() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    insert(&store, "personal", "dated", "2026-01-01T00:00:00Z", false);
    store.db.execute("INSERT INTO google_accounts(account,google_subject,scopes_json,connection_state) VALUES('personal','fixture-subject','[]','connected')",[]).unwrap();
    let stamp = DateTime::parse_from_rfc3339("2026-03-09T03:59:59Z")
        .unwrap()
        .timestamp_millis();
    store.db.execute("INSERT INTO google_mail_coverage(account,remote_id,observed_generation,source_state,index_state,attachment_state,internal_date_ms) VALUES('personal','dated',1,'retained','indexed','complete',?1)",[stamp]).unwrap();
    let row = query(&mut store, Some("personal"), "2026-03-08", "2026-03-09");
    assert_eq!(row["days"][0]["received"], 1);
    assert_eq!(row["header_date_count"], 0);
    assert_eq!(
        query(&mut store, Some("personal"), "2026-01-01", "2026-01-02")["days"][0]["received"],
        0
    );
    store
        .db
        .execute(
            "UPDATE accounts SET lifecycle_state='removed' WHERE id='personal'",
            [],
        )
        .unwrap();
    assert_eq!(
        query(&mut store, None, "2026-03-08", "2026-03-09")["days"][0]["received"],
        0
    );
    assert_eq!(
        store
            .handle(
                "calendar_mail_activity",
                Some("personal"),
                json!({"from":"2026-03-08","to":"2026-03-09"})
            )
            .unwrap_err()
            .code,
        "account_removed"
    );
}
#[test]
fn calendar_mail_activity_bounds_empty_days_and_refresh() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    assert_eq!(
        query(&mut store, None, "2026-01-01", "2026-02-12")["days"]
            .as_array()
            .unwrap()
            .len(),
        42
    );
    assert_eq!(
        query(&mut store, Some("personal"), "2026-03-08", "2026-03-09")["days"][0]["received"],
        0
    );
    insert(&store, "personal", "new", "2026-03-08T12:00:00Z", false);
    assert_eq!(
        query(&mut store, Some("personal"), "2026-03-08", "2026-03-09")["days"][0]["received"],
        1
    );
    for (from, to) in [
        ("2026-01-01", "2026-02-13"),
        ("2026-03-09", "2026-03-08"),
        ("2026-03-08", "2026-03-08"),
        ("2026-2-1", "2026-02-02"),
        ("no", "2026-02-02"),
    ] {
        assert!(store
            .handle("calendar_mail_activity", None, json!({"from":from,"to":to}))
            .is_err());
    }
}
