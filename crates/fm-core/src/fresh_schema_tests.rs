use crate::{Store, SCHEMA_VERSION};
use rusqlite::Connection;

#[test]
fn new_live_profile_has_direct_strict_schema_and_valid_references() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_live(temp.path()).unwrap();
    let version: u32 = store
        .db
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
    let strict_tables: i64 = store
        .db
        .query_row(
            "SELECT count(*) FROM pragma_table_list WHERE type='table' AND strict=1 AND name NOT LIKE 'message_fts_%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(strict_tables, 66);
    let fts_content_tables: i64 = store
        .db
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name='message_fts_content'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(fts_content_tables, 0);
    let category_plan: String = store
        .db
        .query_row(
            "EXPLAIN QUERY PLAN SELECT message_id FROM message_categories WHERE account='fixture' AND category_id='selected'",
            [],
            |row| row.get(3),
        )
        .unwrap();
    assert!(category_plan.contains("message_categories_by_category"));
    let foreign_key_errors: i64 = store
        .db
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(foreign_key_errors, 0);
    let accounts: i64 = store
        .db
        .query_row("SELECT count(*) FROM accounts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(accounts, 0);
    drop(store);
    Store::open_live(temp.path()).unwrap();
}

#[test]
fn new_synthetic_profile_reopens_with_valid_references() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_synthetic(temp.path()).unwrap();
    let version: u32 = store
        .db
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
    let foreign_key_errors: i64 = store
        .db
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(foreign_key_errors, 0);
    let accounts: i64 = store
        .db
        .query_row("SELECT count(*) FROM accounts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(accounts, 3);
    for retention in ["90_days", "2_years", "5_years"] {
        let changed = store
            .db
            .execute(
                "UPDATE account_metadata SET mail_retention=?1 WHERE account=(SELECT account FROM account_metadata LIMIT 1)",
                [retention],
            )
            .unwrap();
        assert_eq!(changed, 1);
    }
    let messages: i64 = store
        .db
        .query_row("SELECT count(*) FROM messages", [], |row| row.get(0))
        .unwrap();
    let indexed: i64 = store
        .db
        .query_row("SELECT count(*) FROM message_fts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(indexed, messages);
    let integrity: String = store
        .db
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
    store
        .db
        .execute(
            "INSERT INTO message_fts(message_fts) VALUES('integrity-check')",
            [],
        )
        .unwrap();
    drop(store);
    Store::open_synthetic(temp.path()).unwrap();
}

#[test]
fn interrupted_live_creation_cannot_seed_a_synthetic_profile() {
    let temp = tempfile::tempdir().unwrap();
    crate::live_profile::live_initialization_marker(temp.path(), true).unwrap();
    Connection::open(temp.path().join("store.sqlite3")).unwrap();
    let store = Store::open(temp.path()).unwrap();
    let kind: i64 = store
        .db
        .query_row(
            "SELECT value FROM meta WHERE key='profile_kind'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(kind, 1);
    let accounts: i64 = store
        .db
        .query_row("SELECT count(*) FROM accounts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(accounts, 0);
    drop(store);
    Store::open_synthetic(temp.path()).err().unwrap();
}

#[test]
fn old_profiles_are_refused_without_rewriting_their_schema() {
    for version in [2, 29, 30, 31] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("store.sqlite3");
        let db = Connection::open(&path).unwrap();
        db.pragma_update(None, "user_version", version).unwrap();
        drop(db);
        let error = Store::open(temp.path()).err().unwrap();
        assert_eq!(error.code, "schema_mismatch");
        assert!(error.message.contains("start fresh"));
        let db = Connection::open(&path).unwrap();
        let observed: u32 = db
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(observed, version);
        let tables: i64 = db
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name='meta'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tables, 0);
    }
}

#[test]
fn baseline_profiles_upgrade_to_the_current_schema_once() {
    let temp = tempfile::tempdir().unwrap();
    drop(Store::open_live(temp.path()).unwrap());
    let path = temp.path().join("store.sqlite3");
    let db = Connection::open(&path).unwrap();
    // A schema32 profile predates the calendar mail count date index and the link index.
    db.execute_batch("DROP TABLE local_model_failures; DROP INDEX google_mail_coverage_by_date; DROP TABLE message_people; DROP TABLE people; DROP TABLE message_link_hosts; DROP TABLE link_hosts; DROP TABLE link_index_state; PRAGMA user_version=32;")
        .unwrap();
    drop(db);
    let store = Store::open(temp.path()).unwrap();
    let version: u32 = store
        .db
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
    let plan: String = store
        .db
        .query_row(
            "EXPLAIN QUERY PLAN SELECT c.internal_date_ms FROM google_mail_coverage c INDEXED BY google_mail_coverage_by_date WHERE c.account='a' AND c.deleted=0 AND c.internal_date_ms>=0 AND c.internal_date_ms<1",
            [],
            |row| row.get(3),
        )
        .unwrap();
    assert!(plan.contains("google_mail_coverage_by_date") && plan.contains("internal_date_ms>?"));
    drop(store);
    Store::open(temp.path()).unwrap();
}

#[test]
fn schema33_profiles_gain_the_link_index() {
    let temp = tempfile::tempdir().unwrap();
    drop(Store::open_live(temp.path()).unwrap());
    let path = temp.path().join("store.sqlite3");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("DROP TABLE local_model_failures; DROP TABLE message_people; DROP TABLE people; DROP TABLE message_link_hosts; DROP TABLE link_hosts; DROP TABLE link_index_state; PRAGMA user_version=33;")
        .unwrap();
    drop(db);
    let store = Store::open(temp.path()).unwrap();
    let version: u32 = store
        .db
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
    let plan: String = store
        .db
        .query_row(
            "EXPLAIN QUERY PLAN SELECT count(*) FROM message_link_hosts WHERE host=1",
            [],
            |row| row.get(3),
        )
        .unwrap();
    assert!(
        plan.contains("PRIMARY KEY") || plan.contains("COVERING INDEX"),
        "{plan}"
    );
}

#[test]
fn schema34_profiles_gain_people_and_index_again() {
    let temp = tempfile::tempdir().unwrap();
    drop(Store::open_live(temp.path()).unwrap());
    let path = temp.path().join("store.sqlite3");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("DROP TABLE local_model_failures; DROP TABLE message_people; DROP TABLE people; INSERT INTO link_index_state VALUES(1,42); PRAGMA user_version=34;")
        .unwrap();
    drop(db);
    let store = Store::open(temp.path()).unwrap();
    let version: u32 = store
        .db
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
    let indexed: i64 = store
        .db
        .query_row("SELECT indexed_rowid FROM link_index_state", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(
        indexed, 0,
        "existing messages are indexed again for their people"
    );
    let plan: String = store
        .db
        .query_row(
            "EXPLAIN QUERY PLAN SELECT person FROM message_people WHERE message_rowid=1",
            [],
            |row| row.get(3),
        )
        .unwrap();
    assert!(
        plan.contains("PRIMARY KEY") || plan.contains("COVERING INDEX"),
        "{plan}"
    );
}

#[test]
fn schema35_profiles_keep_model_choices_and_use_the_built_in_prompts() {
    let temp = tempfile::tempdir().unwrap();
    drop(Store::open_live(temp.path()).unwrap());
    let path = temp.path().join("store.sqlite3");
    let db = Connection::open(&path).unwrap();
    let old = r#"{"endpoint":"http://127.0.0.1:11434","model":{"name":"m","digest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"prompt":"{\"model\":\"{{model}}\"}","accounts":{}}"#;
    db.execute_batch("DROP TABLE local_model_failures; INSERT INTO meta VALUES('local_model_watch_cursor',9),('local_model_watch_requeue',3); PRAGMA user_version=35;")
        .unwrap();
    db.execute("INSERT INTO local_model_settings VALUES(1,4,?1)", [old])
        .unwrap();
    drop(db);
    let store = Store::open(temp.path()).unwrap();
    let state = store.local_model_settings().unwrap();
    assert_eq!(state["revision"], 5);
    assert_eq!(state["settings"]["model"]["name"], "m");
    assert_eq!(state["settings"]["system"], fm_assist::ollama::DEFAULT_SYSTEM);
    let archived: String = store
        .db
        .query_row(
            "SELECT settings_json FROM local_model_settings_history WHERE revision=4",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(archived, old);
    let keys: i64 = store
        .db
        .query_row(
            "SELECT count(*) FROM meta WHERE key LIKE 'local_model_watch_%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(keys, 0);
}
