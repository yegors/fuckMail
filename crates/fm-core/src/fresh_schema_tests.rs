//! Fresh profile schema, upgrade boundaries and integrity checks.
use crate::{Store, SCHEMA_VERSION};
use rusqlite::Connection;

/// Removes what version 41 added, so a fresh profile looks like one made
/// before it.
pub(crate) fn undo_openpgp_schema(db: &Connection) {
    db.execute_batch(
        "DROP TABLE pgp_accounts; DROP TABLE pgp_key_addresses; DROP TABLE pgp_keys; ALTER TABLE messages DROP COLUMN pgp;",
    )
    .unwrap();
}

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
    assert_eq!(strict_tables, 71);
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
    for version in [2, 29, 31, 32, 36] {
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
fn fresh_profiles_carry_the_calendar_mail_count_date_index() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open_live(temp.path()).unwrap();
    let plan: String = store
        .db
        .query_row(
            "EXPLAIN QUERY PLAN SELECT c.internal_date_ms FROM google_mail_coverage c INDEXED BY google_mail_coverage_by_date WHERE c.account='a' AND c.deleted=0 AND c.internal_date_ms>=0 AND c.internal_date_ms<1",
            [],
            |row| row.get(3),
        )
        .unwrap();
    assert!(plan.contains("google_mail_coverage_by_date") && plan.contains("internal_date_ms>?"));
}

#[test]
fn draft_sources_upgrade_drops_the_key_to_their_own_draft() {
    let temp = tempfile::tempdir().unwrap();
    drop(Store::open_live(temp.path()).unwrap());
    // Rebuild the table as profiles made before the change carry it.
    let db = Connection::open(temp.path().join("store.sqlite3")).unwrap();
    let fresh: String = db
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE name='google_draft_sources'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let old = fresh
        .replace("google_draft_sources(", "old_sources(")
        .replace(
            "PRIMARY KEY(account,source_revision),",
            "PRIMARY KEY(account,source_revision),\n FOREIGN KEY(account,draft_id) REFERENCES drafts(account,draft_id),",
        );
    // A source whose draft is missing, as the old key never allowed.
    db.execute_batch(&format!("PRAGMA foreign_keys=OFF; {old}; DROP TABLE google_draft_sources; ALTER TABLE old_sources RENAME TO google_draft_sources;"))
        .unwrap();
    db.execute(
        "INSERT INTO google_draft_sources VALUES('a',1,'r1','google-draft-1',NULL,1,1,1,1,'{}')",
        [],
    )
    .unwrap();
    // The version before this change.
    undo_openpgp_schema(&db);
    db.pragma_update(None, "user_version", 38).unwrap();
    drop(db);

    let store = Store::open_live(temp.path()).unwrap();
    let version: u32 = store
        .db
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
    let keys: Vec<String> = store
        .db
        .prepare("SELECT \"from\" FROM pragma_foreign_key_list('google_draft_sources')")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(keys, ["account", "linked_local_draft_id"]);
    let indexes: i64 = store
        .db
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='index' AND tbl_name='google_draft_sources' AND sql IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(indexes, 6);
    let kept: String = store
        .db
        .query_row(
            "SELECT draft_id FROM google_draft_sources WHERE provider_draft_id='r1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(kept, "google-draft-1");
    // A Gmail draft's source goes in before the draft that mirrors it.
    store
        .db
        .execute("INSERT INTO google_draft_sources VALUES('a',2,'r2','google-draft-2',NULL,1,1,1,1,'{}')", [])
        .unwrap();
}

#[test]
fn messages_once_too_large_are_fetched_again_after_upgrade() {
    let temp = tempfile::tempdir().unwrap();
    drop(Store::open_live(temp.path()).unwrap());
    let db = Connection::open(temp.path().join("store.sqlite3")).unwrap();
    db.execute_batch("INSERT INTO accounts(id,name,email,provider) VALUES('a','A','a@example.test','google');
        INSERT INTO google_accounts(account,google_subject,scopes_json,connection_state,mail_generation) VALUES('a','subject','[]','connected',3);")
        .unwrap();
    for (id, issue, deleted) in [
        ("large", Some("item_too_large"), 0),
        ("many", Some("attachment_limit"), 0),
        ("broken", Some("malformed_response"), 0),
        ("gone", Some("item_too_large"), 1),
        ("whole", None, 0),
    ] {
        db.execute("INSERT INTO google_mail_coverage(account,remote_id,observed_generation,source_state,index_state,attachment_state,issue,deleted) VALUES('a',?1,3,'unavailable','unindexed','incomplete',?2,?3)", rusqlite::params![id, issue, deleted])
            .unwrap();
    }
    // The version before the coverage reset.
    undo_openpgp_schema(&db);
    db.pragma_update(None, "user_version", 39).unwrap();
    drop(db);

    let store = Store::open_live(temp.path()).unwrap();
    let seen: Vec<(String, i64)> = store
        .db
        .prepare(
            "SELECT remote_id,observed_generation FROM google_mail_coverage ORDER BY remote_id",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let expected = [
        ("broken", 3),
        ("gone", 3),
        ("large", 0),
        ("many", 0),
        ("whole", 3),
    ];
    assert_eq!(
        seen,
        expected.map(|(id, generation)| (id.to_owned(), generation))
    );
}
