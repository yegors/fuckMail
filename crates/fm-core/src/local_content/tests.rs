use super::*;
use crate::composition_tests::{ingest, source};

const ACCOUNT: &str = "windscribe";
fn call(store: &mut Store, method: &str, p: Value) -> Value {
    store.handle(method, Some(ACCOUNT), p).unwrap()
}
fn plan(store: &Store, method: &str, p: Value) -> LocalContentPlan {
    match store.local_content_begin(method, ACCOUNT, p).unwrap() {
        LocalContentStep::Prepare(plan) => plan,
        _ => panic!("expected preparation"),
    }
}
fn prepare(plan: LocalContentPlan) -> PreparedLocalContent {
    prepare_local_content(plan, &CancellationToken::new()).unwrap()
}
fn finish(store: &mut Store, proof: PreparedLocalContent) -> Value {
    match store.local_content_finish(proof).unwrap() {
        LocalContentStep::Reply(reply) => reply,
        _ => panic!("unexpected preparation redirect"),
    }
}
fn error<T>(result: Result<T>) -> String {
    match result {
        Err(error) => error.code,
        Ok(_) => panic!("expected failure"),
    }
}
fn new_draft(store: &mut Store) -> Value {
    call(
        store,
        "draft_create",
        json!({"mode":"new","request_id":"new-local-content"}),
    )
}
fn import_request(draft: &Value, path: &Path, id: &str) -> Value {
    json!({"draft_id":draft["draft_id"],"expected_revision":draft["revision"],"request_id":id,"path":path.to_str().unwrap()})
}
fn table(store: &Store, name: &str) -> Vec<Vec<rusqlite::types::Value>> {
    let mut stmt = store
        .db
        .prepare(&format!("SELECT * FROM {name} ORDER BY rowid"))
        .unwrap();
    let columns = stmt.column_count();
    stmt.query_map([], |row| (0..columns).map(|i| row.get(i)).collect())
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap()
}

#[test]
fn local_import_capture_restart_and_replay_preserve_bytes_and_received_queries() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let original = new_draft(&mut store);
    let draft = call(
        &mut store,
        "draft_save",
        json!({"draft_id":original["draft_id"],"expected_revision":1,"request_id":"author-file-draft","body":"Human authored text","body_html":"<p>Human authored text</p>","subject":"Attachment snapshot","to_recipients":[{"email":"reader@example.test"}],"bcc_recipients":[{"email":"private@example.test"}],"signature_text":"Frozen signature"}),
    );
    let baseline = [false, true].map(|has| {
        call(
            &mut store,
            "conversations",
            json!({"folder":"all","has_attachment":has,"limit":50}),
        )
    });
    let messages = table(&store, "messages");
    let fts = table(&store, "message_fts");
    let path = dir.path().join("budget – final.bin");
    let bytes = b"\x00Frozen local file\r\n\xff";
    std::fs::write(&path, bytes).unwrap();
    let request = import_request(&draft, &path, "import-once");
    let pending = plan(&store, "draft_attachment_import", request.clone());
    let reply = finish(&mut store, prepare(pending));
    assert_eq!(reply["draft"]["revision"], 3);
    for field in [
        "body",
        "body_html",
        "subject",
        "from",
        "signature_text",
        "to_recipients",
        "bcc_recipients",
        "source_message_id",
    ] {
        assert_eq!(reply["draft"][field], draft[field]);
    }
    assert_eq!(reply["attachment"]["message_id"], Value::Null);
    assert_eq!(reply["attachment"]["mime_type"], "application/octet-stream");
    assert_eq!(
        reply["attachment"]["sha256"],
        format!("{:x}", Sha256::digest(bytes))
    );
    assert_eq!(table(&store, "messages"), messages);
    assert_eq!(table(&store, "message_fts"), fts);
    for (has, old) in [false, true].into_iter().zip(baseline) {
        let new = call(
            &mut store,
            "conversations",
            json!({"folder":"all","has_attachment":has,"limit":50}),
        );
        for field in [
            "conversations",
            "filtered_conversation_count",
            "coverage",
            "account_counts",
        ] {
            assert_eq!(new[field], old[field], "{field} {has}");
        }
    }
    std::fs::remove_file(&path).unwrap();
    assert!(
        matches!(store.local_content_begin("draft_attachment_import",ACCOUNT,request.clone()).unwrap(),LocalContentStep::Reply(ref value) if *value==reply)
    );
    let mut conflicting = request.clone();
    conflicting["path"] = json!(dir.path().join("different-missing.bin"));
    assert_eq!(
        error(store.local_content_begin("draft_attachment_import", ACCOUNT, conflicting)),
        "request_id_conflict"
    );
    let saved = call(
        &mut store,
        "draft_save",
        json!({"draft_id":draft["draft_id"],"expected_revision":3,"request_id":"after-import","body":"Later local text\n\nFrozen signature"}),
    );
    assert_eq!(saved["attachment_ids"], reply["draft"]["attachment_ids"]);
    let capture_request =
        json!({"draft_id":draft["draft_id"],"expected_revision":4,"request_id":"capture-imported"});
    let captured = call(&mut store, "draft_capture", capture_request.clone());
    let raw = std::fs::read(captured["path"].as_str().unwrap()).unwrap();
    let parsed = mail_parser::MessageParser::default().parse(&raw).unwrap();
    let parts: Vec<_> = parsed.attachments().collect();
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].contents(), bytes);
    assert!(parsed.body_text(0).unwrap().contains("Later local text"));
    assert_eq!(
        parsed
            .body_text(0)
            .unwrap()
            .matches("Frozen signature")
            .count(),
        1
    );
    let rows = table(&store, "operations");
    let capture_rows = table(&store, "capture_operations");
    drop(store);
    let mut store = Store::open(dir.path()).unwrap();
    assert_eq!(call(&mut store, "draft_attachment_import", request), reply);
    assert_eq!(call(&mut store, "draft_capture", capture_request), captured);
    assert_eq!(table(&store, "operations"), rows);
    assert_eq!(table(&store, "capture_operations"), capture_rows);
}

#[test]
fn local_import_rejects_changed_open_file_before_publishing_content() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let draft = new_draft(&mut store);
    let path = dir.path().join("changing.bin");
    let attachments = table(&store, "attachments");
    let operations = table(&store, "operations");
    let blobs_before = std::fs::read_dir(dir.path().join("blobs")).unwrap().count();
    for (case, replacement) in [("truncated", b"x".as_slice()), ("same-length", b"changed!")] {
        std::fs::write(&path, b"original").unwrap();
        let proof = prepare_after_import_metadata(
            plan(
                &store,
                "draft_attachment_import",
                import_request(&draft, &path, case),
            ),
            &CancellationToken::new(),
            || {
                std::fs::write(&path, replacement)?;
                // Make the metadata difference deterministic even on a coarse
                // timestamp filesystem; the truncation also fails length.
                std::fs::OpenOptions::new()
                    .write(true)
                    .open(&path)?
                    .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(1_000_000))?;
                Ok(())
            },
        );
        assert_eq!(error(proof), "content_unavailable", "{case}");
        assert_eq!(table(&store, "attachments"), attachments);
        assert_eq!(table(&store, "operations"), operations);
        assert_eq!(
            call(
                &mut store,
                "draft_get",
                json!({"draft_id":draft["draft_id"]})
            ),
            draft
        );
        assert_eq!(
            std::fs::read_dir(dir.path().join("blobs")).unwrap().count(),
            blobs_before
        );
    }
}

#[test]
fn local_import_cancellation_stale_revision_and_failed_commit_publish_no_attachment() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let draft = new_draft(&mut store);
    let path = dir.path().join("small.bin");
    std::fs::write(&path, b"content").unwrap();
    let before = table(&store, "attachments");
    let request = import_request(&draft, &path, "cancelled-import");
    let token = CancellationToken::new();
    let proof = prepare_local_content(
        plan(&store, "draft_attachment_import", request.clone()),
        &token,
    )
    .unwrap();
    token.cancel();
    assert_eq!(error(store.local_content_finish(proof)), "cancelled");
    assert_eq!(table(&store, "attachments"), before);
    assert_eq!(
        error(prepare_local_content(
            plan(&store, "draft_attachment_import", request.clone()),
            &token
        )),
        "cancelled"
    );
    let stale = prepare(plan(&store, "draft_attachment_import", request));
    let edited = call(
        &mut store,
        "draft_save",
        json!({"draft_id":draft["draft_id"],"expected_revision":1,"request_id":"edit-during-copy","body":"Retain this concurrent edit"}),
    );
    assert_eq!(error(store.local_content_finish(stale)), "stale_revision");
    assert_eq!(table(&store, "attachments"), before);
    assert_eq!(
        call(
            &mut store,
            "draft_get",
            json!({"draft_id":draft["draft_id"]})
        ),
        edited
    );
    let request = import_request(&edited, &path, "failed-import-receipt");
    let proof = prepare(plan(&store, "draft_attachment_import", request));
    store.db.execute_batch("CREATE TRIGGER fail_import_receipt BEFORE INSERT ON operations WHEN NEW.request_id='failed-import-receipt' BEGIN SELECT RAISE(ABORT,'fixture receipt failure'); END").unwrap();
    assert_eq!(error(store.local_content_finish(proof)), "storage_error");
    assert_eq!(table(&store, "attachments"), before);
    assert_eq!(
        call(
            &mut store,
            "draft_get",
            json!({"draft_id":draft["draft_id"]})
        ),
        edited
    );
}

#[test]
fn local_save_replay_precedes_corrupt_blob_access_and_new_saves_verify_content() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let draft = new_draft(&mut store);
    let path = dir.path().join("retained.dat");
    std::fs::write(&path, b"original data").unwrap();
    let imported = call(
        &mut store,
        "draft_attachment_import",
        import_request(&draft, &path, "import-corruption"),
    );
    let request = json!({"draft_id":draft["draft_id"],"expected_revision":2,"request_id":"save-before-corrupt","body":"Kept body"});
    let saved = call(&mut store, "draft_save", request.clone());
    let blob = dir
        .path()
        .join("blobs")
        .join(imported["attachment"]["sha256"].as_str().unwrap());
    std::fs::write(&blob, b"corrupted data").unwrap();
    assert!(
        matches!(store.local_content_begin("draft_save",ACCOUNT,request).unwrap(),LocalContentStep::Reply(ref value) if *value==saved)
    );
    let next = json!({"draft_id":draft["draft_id"],"expected_revision":3,"request_id":"reject-corrupt","body":"Must not replace saved body"});
    let pending = plan(&store, "draft_save", next); // SQL snapshot never opens the blob.
    assert_eq!(
        error(prepare_local_content(pending, &CancellationToken::new())),
        "content_unavailable"
    );
    assert_eq!(
        call(
            &mut store,
            "draft_get",
            json!({"draft_id":draft["draft_id"]})
        ),
        saved
    );
    let invalid = json!({"draft_id":draft["draft_id"],"expected_revision":3,"request_id":"cross-attachment","body":"Must not change","attachment_ids":["personal-attachment-0"]});
    assert_eq!(
        error(store.local_content_begin("draft_save", ACCOUNT, invalid)),
        "not_found"
    );
}

#[test]
fn local_first_save_prepares_frozen_legacy_routing_and_rechecks_source_and_identity() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let (cid, mid) = ingest(&mut store, "first-source", source("first-source", 0));
    let original = call(&mut store, "draft_get", json!({"conversation_id":cid}));
    let explicit = json!({"conversation_id":cid,"source_message_id":mid,"expected_revision":0,"request_id":"bound-first-save","body":"Authored against first source"});
    let bound = prepare(plan(&store, "draft_save", explicit.clone()));
    let omitted = json!({"conversation_id":cid,"expected_revision":0,"request_id":"unbound-first-save","body":"Do not retarget"});
    let unbound = prepare(plan(&store, "draft_save", omitted));
    ingest(&mut store, "later-source", source("later-source", 1));
    assert_eq!(error(store.local_content_finish(unbound)), "stale_revision");
    let saved = finish(&mut store, bound);
    for key in [
        "source_message_id",
        "source_hash",
        "subject",
        "to_recipients",
        "reply_to_message_id",
        "references",
    ] {
        assert_eq!(saved[key], original[key], "{key}");
    }
    let request = json!({"draft_id":saved["draft_id"],"expected_revision":1,"request_id":"identity-race","body":"Uncommitted edit"});
    let proof = prepare(plan(&store, "draft_save", request));
    let revision: i64 = store
        .db
        .query_row(
            "SELECT revision FROM account_metadata WHERE account=?1",
            [ACCOUNT],
            |r| r.get(0),
        )
        .unwrap();
    call(
        &mut store,
        "account_update",
        json!({"expected_revision":revision,"request_id":"changed-identity","name":"Changed name","signature_text":"Changed signature"}),
    );
    assert_eq!(error(store.local_content_finish(proof)), "stale_revision");
    assert_eq!(
        call(
            &mut store,
            "draft_get",
            json!({"draft_id":saved["draft_id"]})
        ),
        saved
    );
    assert_eq!(call(&mut store, "draft_save", explicit), saved);
}

#[test]
fn local_legacy_source_io_is_deferred_and_unavailable_fallback_remains_capture_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let (cid, mid) = ingest(
        &mut store,
        "unavailable-source",
        source("unavailable-source", 0),
    );
    let hash: String = store
        .db
        .query_row(
            "SELECT source_hash FROM messages WHERE account=?1 AND id=?2",
            params![ACCOUNT, mid],
            |r| r.get(0),
        )
        .unwrap();
    std::fs::remove_file(dir.path().join("blobs").join(hash)).unwrap();
    let request = json!({"conversation_id":cid,"source_message_id":mid,"expected_revision":0,"request_id":"save-unavailable-source","body":"Keep human-authored text"});
    let pending = plan(&store, "draft_save", request);
    let saved = finish(&mut store, prepare(pending));
    assert_eq!(saved["source_unavailable"], true);
    assert_eq!(saved["body"], "Keep human-authored text");
    let capture = plan(
        &store,
        "draft_capture",
        json!({"draft_id":saved["draft_id"],"expected_revision":1,"request_id":"refuse-source"}),
    );
    assert_eq!(
        error(prepare_local_content(capture, &CancellationToken::new())),
        "content_unavailable"
    );
}

#[test]
fn local_capture_race_replays_winning_bytes_after_later_draft_edit() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let draft = new_draft(&mut store);
    let saved = call(
        &mut store,
        "draft_save",
        json!({"draft_id":draft["draft_id"],"expected_revision":1,"request_id":"capture-race-body","body":"Original frozen content","to_recipients":[{"email":"recipient@example.test"}]}),
    );
    let request = json!({"draft_id":saved["draft_id"],"expected_revision":2,"request_id":"one-frozen-capture"});
    let first = plan(&store, "draft_capture", request.clone());
    let second = plan(&store, "draft_capture", request.clone());
    let winner = finish(&mut store, prepare(first));
    // Two real preparations on either side of a Date boundary must converge
    // on the first persisted MIME, even though the second proof hashes differently.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        <= winner["created_at"].as_u64().unwrap()
    {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    call(
        &mut store,
        "draft_save",
        json!({"draft_id":saved["draft_id"],"expected_revision":2,"request_id":"after-frozen-winner","body":"Later authored content"}),
    );
    let step = store.local_content_finish(prepare(second)).unwrap();
    let replay = match step {
        LocalContentStep::Reply(_) => panic!("different dated proof must redirect to the winner"),
        LocalContentStep::Prepare(plan) => finish(&mut store, prepare(plan)),
    };
    assert_eq!(replay, winner);
    assert_eq!(winner["attempts_started"], 1);
    assert_eq!(table(&store, "capture_operations").len(), 1);
    assert_eq!(
        call(
            &mut store,
            "draft_get",
            json!({"draft_id":saved["draft_id"]})
        )["body"],
        "Later authored content"
    );
}

#[test]
fn local_import_zero_byte_count_and_combined_byte_bounds_and_live_capture_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let draft = new_draft(&mut store);
    let path = dir.path().join("empty.bin");
    std::fs::write(&path, []).unwrap();
    let imported = call(
        &mut store,
        "draft_attachment_import",
        import_request(&draft, &path, "empty-import"),
    );
    assert_eq!(imported["attachment"]["size"], 0);
    let many = vec![json!(imported["attachment"]["id"]); 51];
    assert_eq!(error(store.local_content_begin("draft_save",ACCOUNT,json!({"draft_id":draft["draft_id"],"expected_revision":2,"request_id":"many-files","body":"","attachment_ids":many}))),"invalid_request");
    let large = std::fs::File::create(&path).unwrap();
    large
        .set_len(fm_mail::MAX_ATTACHMENT_BYTES as u64 + 1)
        .unwrap();
    drop(large);
    assert_eq!(
        error(prepare_local_content(
            plan(
                &store,
                "draft_attachment_import",
                import_request(&imported["draft"], &path, "oversized-import")
            ),
            &CancellationToken::new()
        )),
        "invalid_request"
    );
    // The same per-file maximum is too large once a retained received file is
    // selected; this tests the combined budget before allocating the input.
    let selected = call(
        &mut store,
        "draft_save",
        json!({"draft_id":draft["draft_id"],"expected_revision":2,"request_id":"combined-limit","body":"","attachment_ids":[imported["attachment"]["id"],"windscribe-attachment-0"]}),
    );
    let large = std::fs::File::create(&path).unwrap();
    large.set_len(fm_mail::MAX_ATTACHMENT_BYTES as u64).unwrap();
    drop(large);
    assert_eq!(
        error(prepare_local_content(
            plan(
                &store,
                "draft_attachment_import",
                import_request(&selected, &path, "combined-overflow")
            ),
            &CancellationToken::new()
        )),
        "invalid_request"
    );
    let live_dir = tempfile::tempdir().unwrap();
    let mut live = Store::open_live(live_dir.path()).unwrap();
    let account = live
        .bind_google_account(
            "local-attachment-owner",
            "owner@example.test",
            &[
                "openid",
                "email",
                "https://www.googleapis.com/auth/gmail.modify",
            ]
            .map(String::from)
            .into(),
            None,
        )
        .unwrap();
    let draft = live
        .handle(
            "draft_create",
            Some(&account),
            json!({"mode":"new","request_id":"live-new"}),
        )
        .unwrap();
    std::fs::write(&path, b"offline live attachment").unwrap();
    let reply = live
        .handle(
            "draft_attachment_import",
            Some(&account),
            import_request(&draft, &path, "live-import"),
        )
        .unwrap();
    assert_eq!(reply["attachment"]["account"], account);
    assert_eq!(error(live.local_content_begin("draft_capture",&account,json!({"draft_id":draft["draft_id"],"expected_revision":2,"request_id":"never-live-capture"}))),"capability_unavailable");
}

#[test]
fn local_preparation_rechecks_selected_attachment_stamps_and_account_scope() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let draft = new_draft(&mut store);
    let path = dir.path().join("input.bin");
    std::fs::write(&path, b"input").unwrap();
    let imported = call(
        &mut store,
        "draft_attachment_import",
        import_request(&draft, &path, "stamp-import"),
    );
    let saved = &imported["draft"];
    let save = json!({"draft_id":saved["draft_id"],"expected_revision":2,"request_id":"stale-attachment","body":"Do not publish"});
    let proof = prepare(plan(&store, "draft_save", save));
    let replacement = store.stage_blob(b"provider replacement").unwrap();
    store
        .db
        .execute(
            "UPDATE attachments SET blob_hash=?3,size=?4 WHERE account=?1 AND id=?2",
            params![
                ACCOUNT,
                imported["attachment"]["id"].as_str(),
                replacement,
                b"provider replacement".len()
            ],
        )
        .unwrap();
    assert_eq!(error(store.local_content_finish(proof)), "stale_revision");
    assert_eq!(
        call(
            &mut store,
            "draft_get",
            json!({"draft_id":saved["draft_id"]})
        )["body"],
        saved["body"]
    );
    assert_eq!(
        error(store.local_content_begin(
            "draft_attachment_import",
            "personal",
            import_request(saved, &path, "wrong-account")
        )),
        "not_found"
    );
    let proof = prepare(plan(
        &store,
        "draft_attachment_import",
        import_request(saved, &path, "wrong-profile"),
    ));
    let other_dir = tempfile::tempdir().unwrap();
    let mut other = Store::open(other_dir.path()).unwrap();
    assert_eq!(error(other.local_content_finish(proof)), "scope_mismatch");
}

#[test]
fn local_concurrent_capture_exports_publish_one_verified_copy() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let draft = new_draft(&mut store);
    let saved = call(
        &mut store,
        "draft_save",
        json!({"draft_id":draft["draft_id"],"expected_revision":1,"request_id":"export-body","body":"Concurrent immutable export","to_recipients":[{"email":"reader@example.test"}]}),
    );
    let request =
        json!({"draft_id":saved["draft_id"],"expected_revision":2,"request_id":"same-export"});
    let captured = call(&mut store, "draft_capture", request.clone());
    std::fs::remove_file(captured["path"].as_str().unwrap()).unwrap();
    let first = plan(&store, "draft_capture", request.clone());
    let second = plan(&store, "draft_capture", request);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let other = barrier.clone();
    let left = std::thread::spawn(move || {
        other.wait();
        prepare(first)
    });
    let right = std::thread::spawn(move || {
        barrier.wait();
        prepare(second)
    });
    assert_eq!(finish(&mut store, left.join().unwrap()), captured);
    assert_eq!(finish(&mut store, right.join().unwrap()), captured);
    let bytes = std::fs::read(captured["path"].as_str().unwrap()).unwrap();
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), captured["sha256"]);
    assert_eq!(table(&store, "capture_operations").len(), 1);
}
