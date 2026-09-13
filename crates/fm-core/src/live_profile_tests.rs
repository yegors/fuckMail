use super::*;
use crate::live_profile::ProfileKind;

fn scopes() -> BTreeSet<String> {
    [
        "openid",
        "email",
        "https://www.googleapis.com/auth/gmail.modify",
    ]
    .map(String::from)
    .into()
}

#[test]
fn desktop_client_reference_and_connection_generation_publish_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open_live(dir.path()).unwrap();
    let path = dir
        .path()
        .join("desktop-client.json")
        .to_str()
        .unwrap()
        .to_owned();
    let account = store
        .bind_google_account_with_client(
            "subject-one",
            "one@example.test",
            &scopes(),
            None,
            "test-client",
            &path,
        )
        .unwrap();
    let first = store.google_account_bindings().unwrap().remove(0);
    assert_eq!(first.connection_generation, 1);
    assert_eq!(
        store
            .google_account_client(&account)
            .unwrap()
            .unwrap()
            .config_path,
        path
    );
    assert_eq!(
        store
            .bind_google_account_with_client(
                "subject-two",
                "two@example.test",
                &scopes(),
                None,
                "another-client",
                &path
            )
            .unwrap_err()
            .code,
        "client_mismatch"
    );
    assert_eq!(store.google_account_bindings().unwrap().len(), 1);
    store.db.execute_batch("CREATE TRIGGER reject_client_update BEFORE UPDATE ON google_account_clients BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    assert!(store
        .bind_google_account_with_client(
            "subject-one",
            "changed@example.test",
            &scopes(),
            Some(&account),
            "test-client",
            &path
        )
        .is_err());
    assert_eq!(
        store.google_account_bindings().unwrap()[0].connection_generation,
        first.connection_generation
    );
    assert_eq!(
        store.handle("accounts", None, json!({})).unwrap()["accounts"][0]["email"],
        "one@example.test"
    );
    store
        .db
        .execute_batch("DROP TRIGGER reject_client_update")
        .unwrap();
    store
        .bind_google_account_with_client(
            "subject-one",
            "changed@example.test",
            &scopes(),
            Some(&account),
            "test-client",
            &path,
        )
        .unwrap();
    assert_eq!(
        store.google_account_bindings().unwrap()[0].connection_generation,
        2
    );
    store
        .set_google_connection_state(&account, crate::live_profile::ConnectionState::Disconnected)
        .unwrap();
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        store.google_account_bindings().unwrap()[0].connection_generation,
        3
    );
    assert_eq!(
        store
            .google_account_client(&account)
            .unwrap()
            .unwrap()
            .client_id,
        "test-client"
    );
}

#[test]
fn empty_live_profile_reopens_without_seed_or_simulated_operations() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open_live(dir.path()).unwrap();
    let namespace = store.vault_namespace().unwrap();
    assert_eq!(store.profile_kind().unwrap(), ProfileKind::Live);
    let status = store.handle("status", None, json!({})).unwrap();
    assert_eq!(status["fixture_revision"], 0);
    assert_eq!(status["synthetic"], false);
    assert_eq!(status["fixture_today"], Value::Null);
    assert_eq!(status["capabilities"]["send"], false);
    let calendar = store
        .handle(
            "calendar_combined",
            None,
            json!({"from":"2026-09-12","to":"2026-09-13"}),
        )
        .unwrap();
    assert_eq!(calendar["events"], json!([]));
    assert_eq!(calendar["coverage"]["complete"], false);
    assert_eq!(
        store.handle("accounts", None, json!({})).unwrap()["accounts"],
        json!([])
    );
    for table in [
        "accounts",
        "messages",
        "calendar_sources",
        "drafts",
        "operations",
        "capture_operations",
        "google_checkpoints",
    ] {
        assert_eq!(
            store
                .db
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    assert!(!dir.path().join("blobs").exists());
    for method in [
        "draft_capture",
        "calendar_write_ack",
        "calendar_write",
        "flag",
        "set_category",
    ] {
        assert_eq!(
            store
                .handle(method, Some("unbound"), json!({}))
                .unwrap_err()
                .code,
            "capability_unavailable"
        );
    }
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.profile_kind().unwrap(), ProfileKind::Live);
    assert_eq!(store.vault_namespace().unwrap(), namespace);
    assert_eq!(store.fixture_revision().unwrap(), 0);
    let other = tempfile::tempdir().unwrap();
    assert_ne!(
        Store::open_live(other.path())
            .unwrap()
            .vault_namespace()
            .unwrap(),
        namespace
    );
}

#[test]
fn interrupted_live_schema_migration_keeps_empty_identity() {
    let dir = tempfile::tempdir().unwrap();
    let db = rusqlite::Connection::open(dir.path().join("store.sqlite3")).unwrap();
    db.execute_batch(&include_str!("schema.sql").replace("PRAGMA user_version=2;", "INSERT INTO meta VALUES('profile_kind',1); INSERT INTO meta VALUES('fixture_revision',0); PRAGMA user_version=2;")).unwrap();
    drop(db);
    let mut store = Store::open(dir.path()).unwrap();
    assert_eq!(store.profile_kind().unwrap(), ProfileKind::Live);
    assert_eq!(
        store.handle("accounts", None, json!({})).unwrap()["accounts"],
        json!([])
    );
    store
        .db
        .execute("DELETE FROM meta WHERE key='fixture_revision'", [])
        .unwrap();
    drop(store);
    assert_eq!(
        Store::open(dir.path()).err().unwrap().code,
        "profile_invalid"
    );
    let db = rusqlite::Connection::open(dir.path().join("store.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn interrupted_first_live_transaction_or_marker_never_seeds_demo() {
    let dir = tempfile::tempdir().unwrap();
    live_initialization_marker(dir.path(), true).unwrap();
    // Simulate an initial transaction which rolled back to schema version zero.
    drop(rusqlite::Connection::open(dir.path().join("store.sqlite3")).unwrap());
    let mut store = Store::open(dir.path()).unwrap();
    assert_eq!(store.fixture_revision().unwrap(), 0);
    assert_eq!(
        store.handle("accounts", None, json!({})).unwrap()["accounts"],
        json!([])
    );
    drop(store);
    let invalid = tempfile::tempdir().unwrap();
    std::fs::write(invalid.path().join("live-profile"), b"").unwrap();
    assert_eq!(
        Store::open(invalid.path()).err().unwrap().code,
        "profile_invalid"
    );
    assert!(!invalid.path().join("store.sqlite3").exists());
}

#[test]
fn subject_binding_deduplicates_preserves_drafts_and_rejects_wrong_reconnect() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open_live(dir.path()).unwrap();
    let account = store
        .bind_google_account(
            "opaque-provider-subject",
            "first@example.test",
            &scopes(),
            None,
        )
        .unwrap();
    let draft = store
        .handle(
            "draft_create",
            Some(&account),
            json!({"mode":"new","request_id":"live-draft"}),
        )
        .unwrap();
    let before = store.handle("drafts", Some(&account), json!({})).unwrap();
    assert!(!draft.is_null());
    assert_eq!(
        store
            .bind_google_account(
                "different-subject",
                "first@example.test",
                &scopes(),
                Some(&account)
            )
            .unwrap_err()
            .code,
        "identity_mismatch"
    );
    assert_eq!(
        store
            .bind_google_account(
                "opaque-provider-subject",
                "renamed@example.test",
                &BTreeSet::from(["openid".into(), "email".into()]),
                Some(&account)
            )
            .unwrap(),
        account
    );
    store
        .set_google_connection_state(&account, ConnectionState::Disconnected)
        .unwrap();
    let accounts = store.handle("accounts", None, json!({})).unwrap();
    assert_eq!(accounts["accounts"].as_array().unwrap().len(), 1);
    assert_eq!(accounts["accounts"][0]["email"], "renamed@example.test");
    assert_eq!(accounts["accounts"][0]["coverage"]["synthetic"], false);
    assert_eq!(
        accounts["accounts"][0]["coverage"]["inventory_complete"],
        false
    );
    assert_eq!(
        accounts["accounts"][0]["google"]["connection_state"],
        "disconnected"
    );
    assert_eq!(
        accounts["accounts"][0]["google"]["capabilities"]["send"],
        false
    );
    assert_eq!(
        store.handle("drafts", Some(&account), json!({})).unwrap()["drafts"],
        before["drafts"]
    );
    drop(store);
    let mut reopened = Store::open(dir.path()).unwrap();
    let bindings = reopened.google_account_bindings().unwrap();
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].subject, "opaque-provider-subject");
    assert_eq!(bindings[0].state, ConnectionState::Disconnected);
    assert_eq!(
        reopened
            .handle("drafts", Some(&account), json!({}))
            .unwrap()["drafts"],
        before["drafts"]
    );
    assert_eq!(
        reopened
            .handle("bind_google_account", None, json!({"sub":"fake"}))
            .unwrap_err()
            .code,
        "unsupported_method"
    );
}

#[test]
fn schema_seven_upgrade_preserves_demo_and_refuses_live_conversion() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let before = store.handle("accounts", None, json!({})).unwrap();
    let draft = store
        .handle(
            "draft_create",
            Some("windscribe"),
            json!({"mode":"new","request_id":"preserved-draft"}),
        )
        .unwrap();
    let drafts = store
        .handle("drafts", Some("windscribe"), json!({}))
        .unwrap();
    let receipts: String = store
        .db
        .query_row(
            "SELECT result_json FROM operations WHERE request_id='preserved-draft'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    crate::test_profiles::restore_schema_eight(&store.db);
    store.db.execute_batch("DROP TABLE google_accounts; DROP TABLE profile_identity; DELETE FROM meta WHERE key='profile_kind'; PRAGMA user_version=7;").unwrap();
    drop(store);
    assert_eq!(
        Store::open_live(dir.path()).err().unwrap().code,
        "profile_kind_mismatch"
    );
    let mut store = Store::open(dir.path()).unwrap();
    assert_eq!(store.fixture_revision().unwrap(), 3);
    assert_eq!(
        store.handle("accounts", None, json!({})).unwrap()["accounts"][0]["message_count"],
        before["accounts"][0]["message_count"]
    );
    assert_eq!(
        store
            .handle("drafts", Some("windscribe"), json!({}))
            .unwrap()["drafts"],
        drafts["drafts"]
    );
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT result_json FROM operations WHERE request_id='preserved-draft'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        receipts
    );
    assert_eq!(
        store
            .handle(
                "draft_create",
                Some("windscribe"),
                json!({"mode":"new","request_id":"preserved-draft"})
            )
            .unwrap(),
        draft
    );
    assert_eq!(
        store
            .bind_google_account("opaque", "x@example.test", &scopes(), None)
            .unwrap_err()
            .code,
        "profile_kind_mismatch"
    );
}
