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
fn live_capabilities_distinguish_cached_work_grants_and_active_delivery() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open_live(dir.path()).unwrap();
    let account = store
        .bind_google_account("capability-owner", "reader@example.test", &scopes(), None)
        .unwrap();
    store
        .handle(
            "draft_create",
            Some(&account),
            json!({"mode":"new","request_id":"retained-local-draft"}),
        )
        .unwrap();
    let draft = store.handle("drafts", Some(&account), json!({})).unwrap()["drafts"].clone();
    for (grants, mail_write, calendar_write) in [
        (vec!["openid", "email"], false, false),
        (
            vec![
                "https://www.googleapis.com/auth/gmail.readonly",
                "https://www.googleapis.com/auth/calendar.readonly",
            ],
            false,
            false,
        ),
        (
            vec!["https://www.googleapis.com/auth/gmail.modify"],
            true,
            false,
        ),
        (
            vec![
                "https://mail.google.com/",
                "https://www.googleapis.com/auth/calendar.events",
            ],
            true,
            true,
        ),
    ] {
        let grants = grants.into_iter().map(str::to_owned).collect();
        store
            .bind_google_account(
                "capability-owner",
                "reader@example.test",
                &grants,
                Some(&account),
            )
            .unwrap();
        for state in [
            ConnectionState::Connected,
            ConnectionState::Disconnected,
            ConnectionState::ReauthorizationRequired,
        ] {
            store.set_google_connection_state(&account, state).unwrap();
            let status = store.handle("status", None, json!({})).unwrap();
            for key in ["mail", "drafts", "calendar_read"] {
                assert_eq!(status["capabilities"][key], true, "{key}");
            }
            let accounts = store.handle("accounts", None, json!({})).unwrap();
            let caps = &accounts["accounts"][0]["google"]["capabilities"];
            assert_eq!(caps["mail_read"], true);
            assert_eq!(caps["calendar_read"], true);
            assert_eq!(caps["drafts"], true);
            assert_eq!(caps["mail_write"], mail_write);
            assert_eq!(
                caps["mail_delivery"],
                mail_write && state == ConnectionState::Connected
            );
            assert_eq!(
                caps["calendar_write"],
                calendar_write && state == ConnectionState::Connected
            );
            assert_eq!(
                caps["send"],
                mail_write && state == ConnectionState::Connected
            );
            assert_eq!(
                store.handle("drafts", Some(&account), json!({})).unwrap()["drafts"],
                draft
            );
            assert!(store
                .handle("conversations", Some(&account), json!({}))
                .is_ok());
            assert!(store
                .handle(
                    "calendar_combined",
                    None,
                    json!({"from":"2026-09-12","to":"2026-09-13"})
                )
                .is_ok());
        }
    }
}

#[test]
fn cached_labels_and_binding_dispatch_share_account_validation_and_exact_receipts() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open_live(dir.path()).unwrap();
    let account = store
        .bind_google_account(
            "label-dispatch-owner",
            "reader@example.test",
            &scopes(),
            None,
        )
        .unwrap();
    assert_eq!(
        store
            .handle("google_labels", Some(&account), json!({}))
            .unwrap()["labels"],
        json!([])
    );
    let prepared = crate::google_labels::prepare_google_labels(
        serde_json::from_value(
            json!({"labels":[{"id":"Label_owned","name":"Provider name","type":"user"}]}),
        )
        .unwrap(),
    )
    .unwrap();
    let lease = store
        .google_labels_begin(&account, 1, 0, &prepared.manifest())
        .unwrap()
        .unwrap();
    let lease = store
        .google_labels_stage(&lease, prepared.chunk(0).unwrap().unwrap())
        .unwrap();
    store.google_labels_publish(&lease, 1000).unwrap();
    assert_eq!(
        store
            .handle("google_labels", Some(&account), json!({"limit":1}))
            .unwrap()["labels"][0]["id"],
        "Label_owned"
    );
    let request = json!({"request_id":"bind-owned","expected_catalog_revision":1,"category_id":"owned","provider_label_id":"Label_owned","create_name":"My category"});
    let receipt = store
        .handle("google_category_bind", Some(&account), request.clone())
        .unwrap();
    assert_eq!(receipt["local_only"], true);
    assert_eq!(
        store
            .handle("google_category_bind", Some(&account), request)
            .unwrap(),
        receipt
    );
    assert_eq!(
        store
            .handle(
                "google_mail_mutation_status",
                Some(&account),
                json!({"request_id":"bind-owned"})
            )
            .unwrap_err()
            .code,
        "not_found"
    );
    for method in [
        "google_labels",
        "google_category_bind",
        "google_mail_mutation_status",
    ] {
        assert_eq!(
            store.handle(method, None, json!({})).unwrap_err().code,
            "account_required"
        );
        assert_eq!(
            store
                .handle(method, Some("foreign"), json!({}))
                .unwrap_err()
                .code,
            "not_found"
        );
        assert_eq!(
            store
                .handle(method, Some(&account), json!({"unexpected":true}))
                .unwrap_err()
                .code,
            "invalid_request"
        );
    }
    assert_eq!(
        store
            .handle(
                "google_mail_mutation_status",
                Some(&account),
                json!({"request_id":null})
            )
            .unwrap_err()
            .code,
        "invalid_request"
    );
    assert_eq!(
        store
            .handle("google_mail_mutation_status", Some(&account), json!({}))
            .unwrap(),
        json!({"counts":{},"failures":[],"pending":[]})
    );
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
    for key in ["mail", "drafts", "calendar_read"] {
        assert_eq!(status["capabilities"][key], true, "{key}");
    }
    assert_eq!(status["capabilities"]["send"], true);
    for key in ["phase1", "capture_history", "calendar_write_local"] {
        assert_eq!(status["capabilities"][key], false, "{key}");
    }
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
    for method in ["draft_capture", "calendar_write_ack", "calendar_write"] {
        assert_eq!(
            store
                .handle(method, Some("unbound"), json!({}))
                .unwrap_err()
                .code,
            "capability_unavailable"
        );
    }
    // Live state commands now exist, but an empty profile cannot author work
    // against an invented account or accidentally create a fixture account.
    for method in ["flag", "set_category", "mail_filing"] {
        assert_eq!(
            store
                .handle(method, Some("unbound"), json!({}))
                .unwrap_err()
                .code,
            "not_found"
        );
    }
    for table in ["accounts", "operations", "google_mail_mutations"] {
        assert_eq!(
            store
                .db
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
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
fn only_a_newly_added_account_waits_for_its_first_label_import() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open_live(dir.path()).unwrap();
    let account = store
        .bind_google_account("label-import-owner", "reader@example.test", &scopes(), None)
        .unwrap();
    let key = crate::label_sync::initial_import_key(&account);
    let pending = |store: &Store| -> bool {
        store
            .db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM meta WHERE key=?1)",
                [&key],
                |r| r.get(0),
            )
            .unwrap()
    };
    assert!(pending(&store));
    store
        .db
        .execute("DELETE FROM meta WHERE key=?1", [&key])
        .unwrap();
    store
        .bind_google_account(
            "label-import-owner",
            "reader@example.test",
            &scopes(),
            Some(&account),
        )
        .unwrap();
    assert!(!pending(&store), "reconnecting never repeats the import");
}

#[test]
fn a_new_account_leaves_its_existing_mail_to_the_backlog_unless_chosen() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open_live(dir.path()).unwrap();
    let account = store
        .bind_google_account("history-owner", "history@example.test", &scopes(), None)
        .unwrap();
    let key = crate::local_model::history_cutoff_key(&account);
    let cutoff = |store: &Store| -> Option<i64> {
        store
            .db
            .query_row("SELECT value FROM meta WHERE key=?1", [&key], |r| r.get(0))
            .optional()
            .unwrap()
    };
    let added = cutoff(&store).expect("a new account records when it was added");
    assert!((chrono::Utc::now().timestamp_millis() - added).abs() < 60_000);
    let row = store.accounts().unwrap()["accounts"][0].clone();
    store
        .handle(
            "account_update",
            Some(&account),
            json!({"name":"History","signature_text":"","analyze_history":true,
                "expected_revision":row["metadata_revision"],"request_id":"analyze-history"}),
        )
        .unwrap();
    assert_eq!(
        cutoff(&store),
        None,
        "opting in analyzes the synced history"
    );
    store
        .bind_google_account(
            "history-owner",
            "history@example.test",
            &scopes(),
            Some(&account),
        )
        .unwrap();
    assert_eq!(
        cutoff(&store),
        None,
        "reconnecting keeps the person's choice"
    );
}

#[test]
fn new_accounts_take_distinct_palette_colours_that_can_be_changed() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open_live(dir.path()).unwrap();
    let first = store
        .bind_google_account("colour-one", "one@example.test", &scopes(), None)
        .unwrap();
    let revision = store.preferences().unwrap()["revision"].clone();
    let second = store
        .bind_google_account("colour-two", "two@example.test", &scopes(), None)
        .unwrap();
    assert_eq!(
        store.preferences().unwrap()["revision"],
        revision,
        "adding an account leaves shared preference saves valid"
    );
    let color = |store: &Store, id: &str| -> Value {
        store.accounts().unwrap()["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == id)
            .unwrap()["color"]
            .clone()
    };
    assert_eq!(color(&store, &first), crate::preferences::ACCOUNT_COLORS[0]);
    assert_eq!(
        color(&store, &second),
        crate::preferences::ACCOUNT_COLORS[1]
    );
    let row = |store: &Store| -> Value {
        store.accounts().unwrap()["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == first)
            .unwrap()
            .clone()
    };
    let before = row(&store);
    store
        .handle(
            "account_update",
            Some(&first),
            json!({"name":before["name"],"signature_text":"","color":"#6fb5b2","expected_revision":before["metadata_revision"],"request_id":"colour"}),
        )
        .unwrap();
    assert_eq!(color(&store, &first), "#6FB5B2");
    let after = row(&store);
    assert_eq!(
        store
            .handle(
                "account_update",
                Some(&first),
                json!({"name":after["name"],"signature_text":"","color":"#123456","expected_revision":after["metadata_revision"],"request_id":"off-palette"}),
            )
            .unwrap_err()
            .code,
        "invalid_request",
        "only palette colours are accepted"
    );
}
