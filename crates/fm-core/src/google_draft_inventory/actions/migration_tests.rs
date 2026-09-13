use super::*;
use crate::google_draft_inventory::tests::{create, freeze_local, import, scope, setup};
use crate::google_drafts::GoogleDraftOutcome;
use crate::google_send::{prepare_google_send, GoogleSendOutcome, GoogleSendStep};
use std::collections::BTreeMap;

fn preserved_rows(store: &Store) -> BTreeMap<&'static str, Vec<Vec<rusqlite::types::Value>>> {
    [
        ("drafts", "account,draft_id"),
        ("capture_operations", "account,id"),
        ("operations", "account,request_id"),
        ("google_draft_sources", "account,source_revision"),
        ("google_draft_inventory", "account"),
        ("attachments", "account,id"),
    ]
    .into_iter()
    .map(|(table, order)| {
        let rows = store
            .db
            .prepare(&format!("SELECT * FROM {table} ORDER BY {order}"))
            .unwrap()
            .query_map([], |row| {
                (0..row.as_ref().column_count())
                    .map(|i| row.get(i))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        (table, rows)
    })
    .collect()
}

fn retained_blobs(store: &Store) -> BTreeMap<String, Vec<u8>> {
    std::fs::read_dir(store.profile.join("blobs"))
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            (
                entry.file_name().into_string().unwrap(),
                std::fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
}

#[test]
fn schema15_upgrade_keeps_source_versions_authored_bytes_and_mixed_receipts() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, a, b) = setup(dir.path());
    let first = b"From: Outside <outside@example.test>\r\nTo: Reader <reader@example.test>\r\nSubject: Original\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nRetained original text.";
    let second = b"From: Outside <outside@example.test>\r\nBcc: Private <blind@example.test>\r\nSubject: Changed\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=parts\r\n\r\n--parts\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>Changed text</p>\r\n--parts\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=proof.bin\r\nContent-Transfer-Encoding: base64\r\n\r\nAAEC/w==\r\n--parts--\r\n";
    import(&mut store, &a, "remote", first, "first");
    import(&mut store, &a, "remote", second, "second");
    import(&mut store, &b, "remote", first, "first");

    let pending = create(&mut store, &a, "pending-create");
    let (lease, _) = freeze_local(&mut store, &a, &pending, "unknown-upload");
    store
        .google_draft_outcome(
            &lease,
            GoogleDraftOutcome::Unknown {
                code: "transport_interrupted".into(),
            },
        )
        .unwrap();
    let sent = create(&mut store, &a, "send-create");
    store.handle("draft_save", Some(&a.account), json!({"draft_id":sent,"expected_revision":1,
        "request_id":"authored-save","body":"Human text 日本語, café","to_recipients":[{"email":"reader@example.test"}]})).unwrap();
    let request = json!({"draft_id":sent,"expected_revision":2,"expected_google_revision":0,"request_id":"accepted-send"});
    let GoogleSendStep::Prepare(plan) = store.google_send_begin(&scope(&a), &request).unwrap()
    else {
        panic!("Expected Send preparation")
    };
    let prepared = prepare_google_send(plan, &CancellationToken::new()).unwrap();
    let frozen = store.google_send_finish(prepared).unwrap();
    let lease = store
        .google_send_claim(
            &scope(&a),
            "accepted-send",
            frozen["revision"].as_i64().unwrap(),
        )
        .unwrap();
    let accepted = store
        .google_send_outcome(
            &lease,
            GoogleSendOutcome::Accepted {
                message_id: "accepted-message".into(),
                thread_id: "accepted-thread".into(),
            },
        )
        .unwrap();

    crate::test_profiles::restore_schema_fifteen(&store.db);
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        15
    );
    assert_eq!(store.db.query_row("SELECT count(*) FROM sqlite_schema WHERE name IN ('google_draft_adoption_target','google_delete_target','google_delete_recovery','google_delete_history')", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
    let rows = preserved_rows(&store);
    let blobs = retained_blobs(&store);
    assert_eq!(rows["google_draft_sources"].len(), 3);
    assert_eq!(rows["capture_operations"].len(), 2);
    assert_eq!(rows["attachments"].len(), 1);
    drop(store);

    let reopened = Store::open_live(dir.path()).unwrap();
    assert_eq!(reopened.fixture_revision().unwrap(), 0);
    assert_eq!(
        reopened
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        16
    );
    assert_eq!(preserved_rows(&reopened), rows);
    assert_eq!(retained_blobs(&reopened), blobs);
    assert_eq!(
        reopened
            .google_send_status(&a.account, "accepted-send")
            .unwrap(),
        accepted
    );
    assert_eq!(
        reopened.google_draft_status(&a.account, &pending).unwrap()["state"],
        "acceptance_unknown"
    );
    assert!(reopened
        .google_draft_due(&a.account, a.connection_generation, 10)
        .unwrap()
        .is_empty());
    let mut sources = reopened.db.prepare("SELECT draft_id,source_revision,metadata_json FROM google_draft_sources WHERE account=?1 ORDER BY source_revision").unwrap();
    let retained = sources
        .query_map([&a.account], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    drop(sources);
    for ((id, revision, metadata), expected) in
        retained.iter().zip([first.as_slice(), second.as_slice()])
    {
        let exported = export_google_draft_source(
            reopened
                .google_draft_source_begin(
                    &a.account,
                    &json!({"draft_id":id,"source_revision":revision}),
                )
                .unwrap(),
            &CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(
            std::fs::read(exported["path"].as_str().unwrap()).unwrap(),
            expected
        );
        assert_eq!(
            serde_json::from_str::<Value>(metadata).unwrap()["sha256"],
            exported["sha256"]
        );
    }
    assert_eq!(preserved_rows(&reopened), rows);
    reopened
        .db
        .pragma_update(None, "user_version", crate::SCHEMA_VERSION + 1)
        .unwrap();
    drop(reopened);
    let Err(refusal) = Store::open_live(dir.path()) else {
        panic!("Future schema must be refused")
    };
    assert_eq!(refusal.code, "schema_mismatch");
    let db = rusqlite::Connection::open(dir.path().join("store.sqlite3")).unwrap();
    db.pragma_update(None, "user_version", 16).unwrap();
    drop(db);
    let reopened = Store::open_live(dir.path()).unwrap();
    assert_eq!(preserved_rows(&reopened), rows);
    assert_eq!(retained_blobs(&reopened), blobs);
}
