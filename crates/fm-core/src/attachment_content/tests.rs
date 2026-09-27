//! Offline byte preservation, conversion, draft retention and storage measurements.
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use fm_google::sync::{prepare_full_page, ListPage, MessageContent, MessageRef, PageStore};
use serde_json::{json, Value};
use std::fs;

fn raw(bytes: &[u8], name: &str) -> Vec<u8> {
    format!("From: Sender <sender@example.test>\r\nTo: Reader <reader@example.test>\r\nMessage-ID: <offline@example.test>\r\nSubject: Offline attachment\r\nDate: Fri, 11 Sep 2026 10:00:00 +0000\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=fixture-boundary\r\n\r\n--fixture-boundary\r\nContent-Type: text/plain\r\n\r\nOffline body\r\n--fixture-boundary\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=\"{name}\"\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n--fixture-boundary--\r\n", STANDARD.encode(bytes)).into_bytes()
}

fn import(store: &mut Store, bytes: &[u8], name: &str) -> (String, String, String) {
    let cp = store.google_begin_full("windscribe", "100").unwrap();
    let content = MessageContent {
        id: "offline-file".into(),
        thread_id: "offline-thread".into(),
        label_ids: vec!["INBOX".into()],
        history_id: "110".into(),
        source_mime: raw(bytes, name),
    };
    crate::google_store::GooglePageStore::new(store)
        .stage_message(&cp, content)
        .unwrap();
    let page = prepare_full_page(
        &cp,
        ListPage {
            messages: vec![MessageRef {
                id: "offline-file".into(),
                thread_id: "offline-thread".into(),
            }],
            next_page_token: None,
            result_size_estimate: Some(1),
        },
    )
    .unwrap();
    crate::google_store::GooglePageStore::new(store)
        .commit_page(&page)
        .unwrap();
    store.db.query_row("SELECT a.id,a.blob_hash,m.source_hash FROM attachments a JOIN messages m ON m.account=a.account AND m.id=a.message_id WHERE m.account='windscribe' AND m.remote_id='offline-file'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap()
}

fn export(store: &Store, id: &str) -> Value {
    store
        .attachment_begin("windscribe", &json!({"id":id}))
        .unwrap()
        .export()
        .unwrap()
}

#[test]
fn synced_attachment_has_one_durable_source_and_regenerates_offline() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let bytes = b"Binary PDF-like data\0\xff\x01";
    let before = fs::read_dir(tmp.path().join("blobs")).unwrap().count();
    let (id, hash, source) = import(&mut store, bytes, "report.pdf");
    assert_eq!(
        fs::read_dir(tmp.path().join("blobs")).unwrap().count(),
        before + 1
    );
    assert!(!tmp.path().join("blobs").join(&hash).exists());
    assert_eq!(
        fs::read(tmp.path().join("blobs").join(source)).unwrap(),
        raw(bytes, "report.pdf")
    );
    let opened = export(&store, &id);
    assert_eq!(fs::read(opened["path"].as_str().unwrap()).unwrap(), bytes);
    assert_eq!(opened["sha256"], hash);
    fs::remove_file(opened["path"].as_str().unwrap()).unwrap();
    drop(store);
    let store = Store::open(tmp.path()).unwrap();
    assert_eq!(
        fs::read(export(&store, &id)["path"].as_str().unwrap()).unwrap(),
        bytes
    );
    assert!(!tmp.path().join("blobs").join(hash).exists());
    assert!(store
        .attachment_begin("personal", &json!({"id":id}))
        .is_err());
}

#[test]
fn corrupt_source_and_wrong_part_are_refused_without_writing_a_copy() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let (id, _, source) = import(&mut store, b"protected", "file.bin");
    store
        .db
        .execute(
            "UPDATE attachment_sources SET part_index=99 WHERE id=?1",
            [&id],
        )
        .unwrap();
    assert_eq!(
        store
            .attachment_begin("windscribe", &json!({"id":id}))
            .unwrap()
            .export()
            .unwrap_err()
            .code,
        "content_unavailable"
    );
    store
        .db
        .execute(
            "UPDATE attachment_sources SET part_index=0 WHERE id=?1",
            [&id],
        )
        .unwrap();
    fs::write(tmp.path().join("blobs").join(source), b"corrupt").unwrap();
    assert_eq!(
        store
            .attachment_begin("windscribe", &json!({"id":id}))
            .unwrap()
            .export()
            .unwrap_err()
            .code,
        "content_unavailable"
    );
}

#[test]
fn schema_upgrade_converts_old_copies_and_keeps_shared_authored_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let bytes = b"shared authored bytes";
    let (id, hash, _) = import(&mut store, bytes, "file.bin");
    store.stage_blob(bytes).unwrap();
    store
        .db
        .execute_batch("DROP TABLE attachment_sources; PRAGMA user_version=37;")
        .unwrap();
    store.db.execute("INSERT INTO attachments(account,id,name,mime_type,size,state,blob_hash) VALUES('windscribe','authored','file.bin','application/octet-stream',?1,'available',?2)",params![bytes.len(),hash]).unwrap();
    drop(store);
    let store = Store::open(tmp.path()).unwrap();
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        crate::SCHEMA_VERSION
    );
    while let Some(plan) = store.attachment_migration_begin().unwrap() {
        store
            .attachment_migration_finish(
                prepare_migration(plan, &CancellationToken::new()).unwrap(),
            )
            .unwrap();
    }
    assert!(store
        .attachment_source("windscribe", &id)
        .unwrap()
        .is_some());
    store.cleanup_deleted_archive_blobs().unwrap();
    assert!(tmp.path().join("blobs").join(&hash).exists());
    assert_eq!(
        fs::read(export(&store, "authored")["path"].as_str().unwrap()).unwrap(),
        bytes
    );
    store
        .db
        .execute("DELETE FROM attachments WHERE id='authored'", [])
        .unwrap();
    store
        .db
        .execute(
            "INSERT OR IGNORE INTO archive_blob_cleanup VALUES(?1)",
            [&hash],
        )
        .unwrap();
    store.cleanup_deleted_archive_blobs().unwrap();
    // Old staged import JSON may conservatively retain the hash until removed.
    store
        .db
        .execute("DELETE FROM google_staged_messages", [])
        .unwrap();
    store
        .db
        .execute(
            "INSERT OR IGNORE INTO archive_blob_cleanup VALUES(?1)",
            [&hash],
        )
        .unwrap();
    store.cleanup_deleted_archive_blobs().unwrap();
    assert!(!tmp.path().join("blobs").join(&hash).exists());
    assert_eq!(
        fs::read(export(&store, &id)["path"].as_str().unwrap()).unwrap(),
        bytes
    );
}

#[test]
fn conversion_preserves_a_copy_when_its_original_is_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let (id, hash, source) = import(&mut store, b"only remaining copy", "file.bin");
    store.stage_blob(b"only remaining copy").unwrap();
    store
        .db
        .execute("DELETE FROM attachment_sources WHERE id=?1", [&id])
        .unwrap();
    fs::remove_file(tmp.path().join("blobs").join(source)).unwrap();
    while let Some(plan) = store.attachment_migration_begin().unwrap() {
        store
            .attachment_migration_finish(
                prepare_migration(plan, &CancellationToken::new()).unwrap(),
            )
            .unwrap();
    }
    assert!(store
        .attachment_source("windscribe", &id)
        .unwrap()
        .is_none());
    assert!(tmp.path().join("blobs").join(hash).exists());
    assert_eq!(
        fs::read(export(&store, &id)["path"].as_str().unwrap()).unwrap(),
        b"only remaining copy"
    );
}

#[test]
fn draft_account_transfer_retains_received_bytes_independently() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let bytes = b"transfer received attachment";
    let (id, hash, source) = import(&mut store, bytes, "file.bin");
    let draft = store
        .handle(
            "draft_create",
            Some("windscribe"),
            json!({"mode":"new","request_id":"transfer-received"}),
        )
        .unwrap();
    let draft=store.handle("draft_save",Some("windscribe"),json!({"draft_id":draft["draft_id"],"expected_revision":draft["revision"],"body":"keep my file","attachment_ids":[id],"request_id":"transfer-received-save"})).unwrap();
    let params = json!({"draft_id":draft["draft_id"],"expected_revision":draft["revision"],"expected_google_revision":draft["google"]["revision"],"target_account":"personal","request_id":"transfer-received-move"});
    let crate::draft_transfer::TransferStep::Prepare(plan) =
        store.draft_transfer_begin("windscribe", &params).unwrap()
    else {
        panic!("prepare transfer")
    };
    let prepared = crate::draft_transfer::prepare(plan, CancellationToken::new()).unwrap();
    let result = store.draft_transfer_finish(prepared).unwrap();
    fs::remove_file(tmp.path().join("blobs").join(source)).unwrap();
    store.cleanup_deleted_archive_blobs().unwrap();
    assert!(tmp.path().join("blobs").join(hash).exists());
    let target = &result["draft"]["attachment_ids"][0];
    let exported = store
        .attachment_begin("personal", &json!({"id":target}))
        .unwrap()
        .export()
        .unwrap();
    assert_eq!(fs::read(exported["path"].as_str().unwrap()).unwrap(), bytes);
}

#[test]
fn received_attachment_in_a_draft_survives_message_purge_and_cache_loss() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let bytes = b"forward this file";
    let (id, _, source) = import(&mut store, bytes, "forward.bin");
    let draft = store
        .handle(
            "draft_create",
            Some("windscribe"),
            json!({"mode":"new","request_id":"new-offline"}),
        )
        .unwrap();
    let saved = store.handle("draft_save",Some("windscribe"),json!({"draft_id":draft["draft_id"],"expected_revision":draft["revision"],"body":"Attachment remains local","to_recipients":[{"email":"reader@example.test"}],"attachment_ids":[id],"request_id":"save-offline"})).unwrap();
    store.db.execute_batch("CREATE TEMP TABLE retention_messages AS SELECT id,conversation_id,remote_id,source_hash,rowid AS source_rowid FROM messages WHERE remote_id='offline-file';").unwrap();
    crate::account_retention::purge_messages(&store.db, "windscribe").unwrap();
    store.cleanup_deleted_archive_blobs().unwrap();
    assert!(tmp.path().join("blobs").join(source).exists());
    let cached = export(&store, &id)["path"].as_str().unwrap().to_owned();
    fs::remove_file(&cached).unwrap();
    assert_eq!(
        fs::read(export(&store, &id)["path"].as_str().unwrap()).unwrap(),
        bytes
    );
    fs::remove_file(&cached).unwrap();
    let captured = store.handle("draft_capture",Some("windscribe"),json!({"draft_id":saved["draft_id"],"expected_revision":saved["revision"],"request_id":"capture-offline"})).unwrap();
    let mime = fs::read(captured["path"].as_str().unwrap()).unwrap();
    let parsed = mail_parser::MessageParser::default().parse(&mime).unwrap();
    assert_eq!(parsed.attachments().next().unwrap().contents(), bytes);
}

#[test]
#[ignore = "release-mode storage and attachment latency measurement"]
fn measure_attachment_storage_and_open_latency() {
    for (name, size) in [
        ("generated-1MiB.bin", 1024 * 1024),
        ("generated-5MiB.bin", 5 * 1024 * 1024),
        ("generated-20MiB.bin", 20 * 1024 * 1024),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = Store::open(tmp.path()).unwrap();
        let bytes: Vec<u8> = (0..size)
            .map(|i| ((i * 31 + i / 251) % 256) as u8)
            .collect();
        let stored_bytes = || {
            fs::read_dir(tmp.path().join("blobs"))
                .unwrap()
                .map(|entry| entry.unwrap().metadata().unwrap().len())
                .sum::<u64>()
        };
        let before = stored_bytes();
        let (id, _, _) = import(&mut store, &bytes, name);
        let retained = stored_bytes() - before;
        assert_eq!(retained, raw(&bytes, name).len() as u64);
        let start = std::time::Instant::now();
        let first = export(&store, &id);
        let first_elapsed = start.elapsed();
        let start = std::time::Instant::now();
        for _ in 0..10 {
            export(&store, &id);
        }
        let warm = start.elapsed() / 10;
        assert_eq!(fs::read(first["path"].as_str().unwrap()).unwrap(), bytes);
        assert_eq!(stored_bytes() - before, retained);
        println!("{name}: attachment={size} retained={retained} saved_durable={size} first_ms={:.3} repeat_ms={:.3}",first_elapsed.as_secs_f64()*1000.0,warm.as_secs_f64()*1000.0);
    }
}
