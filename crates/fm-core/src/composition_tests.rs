use crate::*;
use fm_google::sync::{prepare_full_page, ListPage, MessageContent, MessageRef, PageStore};
use google_store::GooglePageStore;
use rusqlite::params;
use sha2::{Digest, Sha256};

const ACCOUNT: &str = "windscribe";
const THREAD: &str = "windscribe-thread-000";

fn call(store: &mut Store, method: &str, p: Value) -> Value {
    store.handle(method, Some(ACCOUNT), p).unwrap()
}

fn create(store: &mut Store, request: &str) -> Value {
    call(
        store,
        "draft_create",
        json!({"mode":"new","request_id":request}),
    )
}

fn emails(rows: &Value) -> Vec<&str> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|r| r["email"].as_str().unwrap())
        .collect()
}

pub(crate) fn source(remote: &str, minute: u8) -> Vec<u8> {
    format!("From: Original Author <author@fixture.example>\r\nReply-To: Reply Desk <reply@fixture.example>\r\nTo: Owner <YOU@WINDSCRIBE.EXAMPLE>, Ada <ada@fixture.example>\r\nCc: Duplicate <ADA@fixture.example>, Bob <bob@fixture.example>\r\nBcc: Private <secret@fixture.example>\r\nSubject: Frozen source context\r\nDate: Fri, 11 Sep 2026 15:{minute:02}:00 +0000\r\nMessage-ID: <{remote}@fixture.example>\r\nReferences: <ancestor@fixture.example>\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nOriginal retained text {remote}.\r\nSecond line.\r\n").into_bytes()
}

fn source_with_attachment(remote: &str, minute: u8) -> Vec<u8> {
    let plain = String::from_utf8(source(remote, minute)).unwrap();
    let multipart=plain.replace("Content-Type: text/plain; charset=utf-8\r\n\r\n", "Content-Type: multipart/mixed; boundary=composition-boundary\r\n\r\n--composition-boundary\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n");
    format!("{multipart}--composition-boundary\r\nContent-Type: text/plain; name=retained.txt\r\nContent-Disposition: attachment; filename=retained.txt\r\n\r\nRetained attachment bytes.\r\n--composition-boundary--\r\n").into_bytes()
}

// Use the same source staging and transactional ingestion as the engine. The
// test reads local IDs back independently instead of constructing Core IDs.
pub(crate) fn ingest(store: &mut Store, remote: &str, bytes: Vec<u8>) -> (String, String) {
    ingest_for_account(store, ACCOUNT, remote, bytes)
}

pub(crate) fn ingest_for_account(
    store: &mut Store,
    account: &str,
    remote: &str,
    bytes: Vec<u8>,
) -> (String, String) {
    let checkpoint = store
        .google_checkpoint(account)
        .unwrap()
        .unwrap_or_else(|| store.google_begin_full(account, "100").unwrap());
    let plan = prepare_full_page(
        &checkpoint,
        ListPage {
            messages: vec![MessageRef {
                id: remote.into(),
                thread_id: "composition-source-thread".into(),
            }],
            next_page_token: Some(format!("page-{remote}")),
            result_size_estimate: None,
        },
    )
    .unwrap();
    let mut adapter = GooglePageStore::new(store);
    adapter
        .stage_message(
            &checkpoint,
            MessageContent {
                id: remote.into(),
                thread_id: "composition-source-thread".into(),
                label_ids: vec!["INBOX".into(), "UNREAD".into()],
                history_id: "110".into(),
                source_mime: bytes,
            },
        )
        .unwrap();
    adapter.commit_page(&plan).unwrap();
    store
        .db
        .query_row(
            "SELECT conversation_id,id FROM messages WHERE account=?1 AND remote_id=?2",
            params![account, remote],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
}

#[test]
fn composition_new_draft_capture_and_restart_never_manufacture_received_mail() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let before = call(
        &mut store,
        "conversations",
        json!({"folder":"all","grouped":true}),
    );
    let original = create(&mut store, "new-message");
    assert_eq!(original["mode"], "new");
    assert!(original["conversation_id"].is_null());
    assert!(original["source_message_id"].is_null());
    assert_eq!(original["revision"], 1);
    assert_eq!(original["from"], "you@windscribe.example");
    assert_eq!(create(&mut store, "new-message"), original);
    let id = original["draft_id"].clone();
    let saved = call(
        &mut store,
        "draft_save",
        json!({"draft_id":id,"expected_revision":1,"request_id":"author-new",
        "body":"Uniquecomposition body — 日本語","subject":"Release coordination","signature_text":"Personal signature",
        "to_recipients":[{"name":"Ada","email":"ada@fixture.example"}],"cc_recipients":[{"email":"bob@fixture.example"}],
        "bcc_recipients":[{"email":"private@fixture.example"}]}),
    );
    call(
        &mut store,
        "account_update",
        json!({"name":"Changed identity","signature_text":"Changed default","expected_revision":0,"request_id":"changed-default"}),
    );
    let capture_request =
        json!({"draft_id":id,"expected_revision":2,"request_id":"unknown-new","outcome":"unknown"});
    let captured = call(&mut store, "draft_capture", capture_request.clone());
    assert_eq!(captured["state"], "acceptance_unknown");
    assert_eq!(captured["attempts_started"], 1);
    assert_eq!(captured["draft_id"], id);
    assert!(captured["conversation_id"].is_null());
    let bytes = std::fs::read(captured["path"].as_str().unwrap()).unwrap();
    let parsed = mail_parser::MessageParser::default().parse(&bytes).unwrap();
    assert_eq!(parsed.subject(), Some("Release coordination"));
    assert_eq!(
        parsed.to().unwrap().first().unwrap().address(),
        Some("ada@fixture.example")
    );
    assert_eq!(
        parsed.cc().unwrap().first().unwrap().address(),
        Some("bob@fixture.example")
    );
    // The existing Gmail submission format retains authored Bcc in raw MIME;
    // there is no separate SMTP envelope. This is the explicit local capture.
    assert_eq!(
        parsed.bcc().unwrap().first().unwrap().address(),
        Some("private@fixture.example")
    );
    assert!(parsed.in_reply_to().as_text().is_none());
    let body = parsed.body_text(0).unwrap();
    assert_eq!(body.matches("Personal signature").count(), 1);
    assert!(!body.contains("Changed default"));
    let newer = call(
        &mut store,
        "draft_save",
        json!({"draft_id":id,"expected_revision":2,"request_id":"later-edit","body":"Later authored revision"}),
    );
    assert_eq!(newer["signature_text"], saved["signature_text"]);
    assert_eq!(newer["from_name"], saved["from_name"]);
    assert_eq!(newer["bcc_recipients"], saved["bcc_recipients"]);
    assert_eq!(
        call(&mut store, "draft_capture", capture_request.clone()),
        captured
    );
    let history = call(
        &mut store,
        "capture_history",
        json!({"draft_id":id,"unresolved_only":true}),
    );
    assert_eq!(history["captures"].as_array().unwrap().len(), 1);
    assert_eq!(history["captures"][0]["subject"], "Release coordination");
    assert_eq!(history["captures"][0]["synthetic_outcome"], "unknown");
    let after = call(
        &mut store,
        "conversations",
        json!({"folder":"all","grouped":true}),
    );
    assert_eq!(after["account_counts"], before["account_counts"]);
    assert_eq!(after["conversations"], before["conversations"]);
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE account='windscribe'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        143
    );
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT COUNT(*) FROM conversations WHERE account='windscribe'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        110
    );
    assert_eq!(
        call(
            &mut store,
            "conversations",
            json!({"query":"Uniquecomposition"})
        )["filtered_conversation_count"],
        0
    );
    assert_eq!(
        call(&mut store, "drafts", json!({"query":"release private"}))["total"],
        1
    );
    drop(store);
    let mut store = Store::open(dir.path()).unwrap();
    assert_eq!(call(&mut store, "draft_get", json!({"draft_id":id})), newer);
    assert_eq!(call(&mut store, "draft_capture", capture_request), captured);
    assert_eq!(
        std::fs::read(captured["path"].as_str().unwrap()).unwrap(),
        bytes
    );
    let discarded = call(
        &mut store,
        "draft_discard",
        json!({"draft_id":id,"expected_revision":3,"request_id":"discard-new"}),
    );
    assert_eq!(discarded["body"], "");
    assert_eq!(discarded["discarded"], true);
    assert_eq!(
        call(&mut store, "drafts", json!({"query":"release"}))["total"],
        0
    );
    assert_eq!(
        store
            .handle(
                "draft_capture",
                Some(ACCOUNT),
                json!({"draft_id":id,"expected_revision":4,"request_id":"discarded-capture"})
            )
            .unwrap_err()
            .code,
        "invalid_draft"
    );
    assert_eq!(
        call(
            &mut store,
            "draft_get",
            json!({"conversation_id":"windscribe-thread-003"})
        )["body"],
        "I will review the rollout window and confirm the timing."
    );
}

#[test]
fn composition_reply_all_and_forward_freeze_retained_source_without_bcc_or_attachments() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let (cid, mid) = ingest(
        &mut store,
        "original",
        source_with_attachment("original", 0),
    );
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT COUNT(*) FROM attachments WHERE account=?1 AND message_id=?2",
                params![ACCOUNT, mid],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    let reply = call(
        &mut store,
        "draft_create",
        json!({"mode":"reply","conversation_id":cid,"source_message_id":mid,"request_id":"reply"}),
    );
    let all = call(
        &mut store,
        "draft_create",
        json!({"mode":"reply_all","conversation_id":cid,"source_message_id":mid,"request_id":"reply-all"}),
    );
    let forward = call(
        &mut store,
        "draft_create",
        json!({"mode":"forward","conversation_id":cid,"source_message_id":mid,"request_id":"forward"}),
    );
    assert_ne!(reply["draft_id"], all["draft_id"]);
    assert_ne!(all["draft_id"], forward["draft_id"]);
    assert_eq!(emails(&reply["to_recipients"]), ["reply@fixture.example"]);
    assert_eq!(reply["reply_all_available"], true);
    assert_eq!(
        emails(&all["to_recipients"]),
        ["reply@fixture.example", "ada@fixture.example"]
    );
    assert_eq!(emails(&all["cc_recipients"]), ["bob@fixture.example"]);
    assert_eq!(all["bcc_recipients"], json!([]));
    assert_eq!(all["subject"], "Re: Frozen source context");
    assert_eq!(all["references"], json!(["<ancestor@fixture.example>"]));
    assert_eq!(all["reply_to_message_id"], "<original@fixture.example>");
    assert_eq!(forward["subject"], "Fwd: Frozen source context");
    assert_eq!(forward["to_recipients"], json!([]));
    assert_eq!(forward["attachment_ids"], json!([]));
    assert_eq!(forward["references"], json!([]));
    assert!(forward["reply_to_message_id"].is_null());
    assert!(forward["body"]
        .as_str()
        .unwrap()
        .contains("> Original retained text original.\n> Second line."));
    let (_, arrived) = ingest(&mut store, "arrived", source("arrived", 1));
    assert_ne!(arrived, mid);
    assert_eq!(
        call(&mut store, "draft_get", json!({"draft_id":all["draft_id"]})),
        all
    );
    let saved = call(
        &mut store,
        "draft_save",
        json!({"draft_id":all["draft_id"],"expected_revision":1,"request_id":"save-all","body":"Authored reply all"}),
    );
    let captured = call(
        &mut store,
        "draft_capture",
        json!({"draft_id":all["draft_id"],"expected_revision":2,"request_id":"capture-all"}),
    );
    let bytes = std::fs::read(captured["path"].as_str().unwrap()).unwrap();
    let mime = String::from_utf8_lossy(&bytes);
    assert!(mime.contains("In-Reply-To: <original@fixture.example>"));
    assert!(!mime.contains("<arrived@fixture.example>"));
    assert!(!mime.contains("secret@fixture.example"));
    assert_eq!(saved["source_hash"], all["source_hash"]);
    assert_eq!(store.handle("draft_save",Some(ACCOUNT),json!({"draft_id":all["draft_id"],"source_message_id":arrived,"expected_revision":2,"request_id":"retarget","body":"Must remain unchanged"})).unwrap_err().code,"invalid_request");
    assert_eq!(
        call(&mut store, "draft_get", json!({"draft_id":all["draft_id"]})),
        saved
    );
    let authored_forward = format!("Human preface\n{}", forward["body"].as_str().unwrap());
    call(
        &mut store,
        "draft_save",
        json!({"draft_id":forward["draft_id"],"expected_revision":1,"request_id":"save-forward","body":authored_forward,"to_recipients":[{"email":"forward@fixture.example"}]}),
    );
    let captured = call(
        &mut store,
        "draft_capture",
        json!({"draft_id":forward["draft_id"],"expected_revision":2,"request_id":"capture-forward"}),
    );
    let bytes = std::fs::read(captured["path"].as_str().unwrap()).unwrap();
    let parsed = mail_parser::MessageParser::default().parse(&bytes).unwrap();
    assert_eq!(parsed.subject(), Some("Fwd: Frozen source context"));
    assert!(parsed.in_reply_to().as_text().is_none());
    assert_eq!(parsed.attachments().count(), 0);
    assert!(parsed.body_text(0).unwrap().contains("Human preface"));
}

#[test]
fn composition_legacy_first_save_binds_loaded_member_across_actual_arrival() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let (cid, mid) = ingest(&mut store, "loaded", source("loaded", 0));
    let loaded = call(&mut store, "draft_get", json!({"conversation_id":cid}));
    assert_eq!(loaded["revision"], 0);
    assert_eq!(loaded["source_message_id"], mid);
    let (_, arrived) = ingest(&mut store, "after-load", source("after-load", 1));
    assert_eq!(
        call(&mut store, "draft_get", json!({"conversation_id":cid}))["source_message_id"],
        arrived
    );
    let saved = call(
        &mut store,
        "draft_save",
        json!({"conversation_id":cid,"source_message_id":mid,"expected_revision":0,"request_id":"save-loaded","body":"Reply to the loaded source"}),
    );
    assert_eq!(saved["source_message_id"], mid);
    assert_eq!(saved["to_recipients"], loaded["to_recipients"]);
    assert_eq!(saved["subject"], loaded["subject"]);
    assert_eq!(saved["reply_to_message_id"], "<loaded@fixture.example>");
    assert_eq!(store.handle("draft_save",Some(ACCOUNT),json!({"conversation_id":cid,"source_message_id":arrived,"expected_revision":0,"request_id":"save-loaded","body":"Reply to the loaded source"})).unwrap_err().code,"request_id_conflict");
    assert_eq!(
        call(
            &mut store,
            "draft_save",
            json!({"conversation_id":cid,"source_message_id":mid,"expected_revision":0,"request_id":"save-loaded","body":"Reply to the loaded source"})
        ),
        saved
    );
    let request =
        json!({"conversation_id":cid,"expected_revision":1,"request_id":"capture-loaded"});
    let capture = call(&mut store, "draft_capture", request.clone());
    let mime = std::fs::read(capture["path"].as_str().unwrap()).unwrap();
    assert!(String::from_utf8_lossy(&mime).contains("In-Reply-To: <loaded@fixture.example>"));
    assert!(!String::from_utf8_lossy(&mime).contains("<after-load@fixture.example>"));
    assert!(
        capture.get("draft_id").is_none(),
        "legacy receipt shape stays exact"
    );
    let journal: String = store
        .db
        .query_row(
            "SELECT json FROM capture_operations WHERE account=?1 AND id='capture-loaded'",
            [ACCOUNT],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        call(
            &mut store,
            "draft_capture",
            json!({"draft_id":cid,"expected_revision":1,"request_id":"capture-loaded"})
        ),
        capture
    );
    drop(store);
    let mut store = Store::open(dir.path()).unwrap();
    assert_eq!(call(&mut store, "draft_capture", request), capture);
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT json FROM capture_operations WHERE account=?1 AND id='capture-loaded'",
                [ACCOUNT],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        journal
    );
    assert_eq!(
        call(&mut store, "draft_get", json!({"draft_id":cid})),
        saved
    );
}

#[test]
fn composition_drafts_page_search_scope_and_invalid_edits_are_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let mut ids = vec![];
    for index in 0..3 {
        let draft = create(&mut store, &format!("paged-{index}"));
        ids.push(draft["draft_id"].clone());
        call(
            &mut store,
            "draft_save",
            json!({"draft_id":draft["draft_id"],"expected_revision":1,"request_id":format!("page-save-{index}"),"body":format!("Punctuation-safe Café draft {index}"),"subject":"Paged subject","to_recipients":[{"email":"unique@fixture.example"}]}),
        );
    }
    let first = call(&mut store, "drafts", json!({"query":"paged uni","limit":2}));
    assert_eq!(first["draft_count"], 4);
    assert_eq!(first["total"], 3);
    assert_eq!(first["next_offset"], 2);
    let last = call(
        &mut store,
        "drafts",
        json!({"query":"paged uni","limit":2,"offset":2}),
    );
    assert!(last["next_offset"].is_null());
    let mut listed = first["drafts"]
        .as_array()
        .unwrap()
        .iter()
        .chain(last["drafts"].as_array().unwrap())
        .map(|r| r["draft_id"].clone())
        .collect::<Vec<_>>();
    ids.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
    listed.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
    assert_eq!(listed, ids);
    assert_eq!(
        store
            .handle("drafts", Some("personal"), json!({"query":"paged"}))
            .unwrap()["total"],
        0
    );
    assert_eq!(
        store
            .handle("draft_get", Some("personal"), json!({"draft_id":ids[0]}))
            .unwrap_err()
            .code,
        "not_found"
    );
    let before = call(&mut store, "draft_get", json!({"draft_id":ids[0]}));
    for patch in [
        json!({"subject":"Injected\r\nBcc: bad@example.test"}),
        json!({"to_recipients":[{"email":"bad\r\n@example.test"}]}),
        json!({"to_recipients":[{"email":"good@example.test","extra":true}]}),
        json!({"bcc_recipients":vec![json!({"email":"a@example.test"});100],"to_recipients":[{"email":"b@example.test"}]}),
        json!({"from":"foreign@example.test"}),
        json!({"mode":"forward"}),
    ] {
        let mut p = json!({"draft_id":ids[0],"body":"Must not replace valid text","expected_revision":2,"request_id":"invalid-edit"});
        p.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert_eq!(
            store
                .handle("draft_save", Some(ACCOUNT), p)
                .unwrap_err()
                .code,
            "invalid_request"
        );
        assert_eq!(
            call(&mut store, "draft_get", json!({"draft_id":ids[0]})),
            before
        );
    }
    assert_eq!(store.handle("draft_save",Some(ACCOUNT),json!({"draft_id":ids[0],"body":"Stale text","expected_revision":1,"request_id":"stale"})).unwrap_err().code,"stale_revision");
    for p in [
        json!({"sender":"a"}),
        json!({"status":"review"}),
        json!({"limit":101}),
        json!({"offset":-1}),
        json!({"query":"from:ada"}),
        json!({"query":"x ".repeat(33)}),
    ] {
        assert_eq!(
            store.handle("drafts", Some(ACCOUNT), p).unwrap_err().code,
            "invalid_request"
        );
    }
    for p in [
        json!({}),
        json!({"draft_id":ids[0],"conversation_id":THREAD}),
    ] {
        assert_eq!(
            store
                .handle("draft_get", Some(ACCOUNT), p)
                .unwrap_err()
                .code,
            "invalid_request"
        );
    }
    call(
        &mut store,
        "view_save",
        json!({"view":{"folder":"drafts","selectedDraft":ids[0]},"expected_revision":0,"request_id":"draft-view"}),
    );
    assert_eq!(
        store.view(ACCOUNT).unwrap()["view"]["selectedDraft"],
        ids[0]
    );
    let references = (0..101)
        .map(|i| format!("<ref-{i}@fixture.example>"))
        .collect::<Vec<_>>()
        .join(" ");
    let oversized = String::from_utf8(source("many-references", 0))
        .unwrap()
        .replace("<ancestor@fixture.example>", &references)
        .into_bytes();
    let (cid, mid) = ingest(&mut store, "many-references", oversized);
    assert_eq!(store.handle("draft_create",Some(ACCOUNT),json!({"mode":"reply","conversation_id":cid,"source_message_id":mid,"request_id":"too-many-references"})).unwrap_err().code,"limit_exceeded");
}

#[test]
fn composition_schema5_upgrade_preserves_authored_columns_and_exact_old_journals() {
    let dir = tempfile::tempdir().unwrap();
    let db = Connection::open(dir.path().join("store.sqlite3")).unwrap();
    db.execute_batch(include_str!("schema.sql")).unwrap();
    for sql in [
        include_str!("migration-003-drafts.sql"),
        include_str!("migration-003-google.sql"),
        include_str!("migration-004-calendar.sql"),
        include_str!("migration-004-assist.sql"),
        include_str!("migration-005.sql"),
    ] {
        db.execute_batch(sql).unwrap();
    }
    db.execute_batch("PRAGMA user_version=5").unwrap();
    let mut old = Store {
        mail_changes: queries::MailChanges::install(&db),
        db,
        profile: dir.path().into(),
        _lock: File::create(dir.path().join("store.lock")).unwrap(),
        projection_cache: Default::default(),
        coverage_cache: Default::default(),
        conversation_cache: Default::default(),
    };
    old.seed().unwrap();
    old.db
        .execute("UPDATE meta SET value=2 WHERE key='fixture_revision'", [])
        .unwrap();
    let aid: String = old
        .db
        .query_row(
            "SELECT id FROM attachments WHERE account=?1 AND state='available' LIMIT 1",
            [ACCOUNT],
            |r| r.get(0),
        )
        .unwrap();
    let old_request = json!({"conversation_id":THREAD,"body":"Old authored words — 日本語","body_html":"<p>Old authored words</p>","attachment_ids":[aid],"signature_text":"Frozen old signature","expected_revision":8,"request_id":"old-save"});
    let old_reply = json!({"conversation_id":THREAD,"body":"Old authored words — 日本語","revision":9,"body_html":"<p>Old authored words</p>","attachment_ids":[aid],"signature_text":"Frozen old signature","from_name":"Old display name","identity_revision":7,"discarded":false});
    old.db.execute("INSERT INTO drafts(account,conversation_id,body,revision,body_html,attachments_json,signature_text,from_name,identity_revision) VALUES(?1,?2,?3,9,?4,?5,'Frozen old signature','Old display name',7)",params![ACCOUNT,THREAD,old_reply["body"].as_str(),old_reply["body_html"].as_str(),serde_json::to_string(&json!([aid])).unwrap()]).unwrap();
    let result_json = serde_json::to_string(&old_reply).unwrap();
    let fingerprint = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&json!(["draft_save", old_request])).unwrap())
    );
    old.db.execute("INSERT INTO operations(account,request_id,fingerprint,result_json) VALUES(?1,'old-save',?2,?3)",params![ACCOUNT,fingerprint,result_json]).unwrap();
    let first_request = json!({"conversation_id":THREAD,"body":"Historical first save","expected_revision":0,"request_id":"old-first-save"});
    let first_receipt =
        json!({"conversation_id":THREAD,"body":"Historical first save","revision":1});
    let first_fingerprint = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&json!(["draft_save", first_request])).unwrap())
    );
    let first_json = serde_json::to_string(&first_receipt).unwrap();
    old.db.execute("INSERT INTO operations(account,request_id,fingerprint,result_json) VALUES(?1,'old-first-save',?2,?3)",params![ACCOUNT,first_fingerprint,first_json]).unwrap();
    let mime=b"From: you@windscribe.example\r\nTo: maya@windscribe.example\r\nSubject: Old exact capture\r\nMessage-ID: <old@fixture.example>\r\n\r\nImmutable old captured body";
    let hash = old.stage_blob(mime).unwrap();
    let mut operation = fm_google::send::SendOperation::prepared(
        fm_google::AccountId::new(ACCOUNT).unwrap(),
        "old-capture".into(),
        mime,
    )
    .unwrap();
    operation.state = fm_google::send::SendState::Accepted {
        message_id: "old-receipt".into(),
        thread_id: THREAD.into(),
    };
    let record=serde_json::to_string(&json!({"conversation_id":THREAD,"draft_revision":9,"outcome":"accepted","size":mime.len(),"created_at":1788800000,"attempts_started":1,"operation":operation})).unwrap();
    old.db.execute("INSERT INTO capture_operations(account,id,json,blob_hash) VALUES(?1,'old-capture',?2,?3)",params![ACCOUNT,record,hash]).unwrap();
    let authored = |store: &Store| -> Vec<Vec<Value>> {
        let mut s=store.db.prepare("SELECT account,conversation_id,body,revision,body_html,attachments_json,discarded,signature_text,from_name,identity_revision FROM drafts ORDER BY account,conversation_id").unwrap();
        s.query_map([], |r| {
            Ok(vec![
                json!(r.get::<_, String>(0)?),
                json!(r.get::<_, String>(1)?),
                json!(r.get::<_, String>(2)?),
                json!(r.get::<_, i64>(3)?),
                json!(r.get::<_, Option<String>>(4)?),
                json!(r.get::<_, String>(5)?),
                json!(r.get::<_, bool>(6)?),
                json!(r.get::<_, String>(7)?),
                json!(r.get::<_, String>(8)?),
                json!(r.get::<_, i64>(9)?),
            ])
        })
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap()
    };
    let before = authored(&old);
    drop(old);
    let mut store = Store::open(dir.path()).unwrap();
    assert_eq!(authored(&store), before);
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        SCHEMA_VERSION as i64
    );
    assert_eq!(store.fixture_revision().unwrap(), 2);
    assert_eq!(call(&mut store, "draft_save", old_request), old_reply);
    let draft = call(&mut store, "draft_get", json!({"conversation_id":THREAD}));
    assert_eq!(draft["draft_id"], THREAD);
    assert_eq!(draft["legacy_reply"], true);
    assert_eq!(draft["mode"], "reply");
    assert!(draft["source_message_id"].is_string());
    assert_eq!(draft["created_at"], 0);
    let mut updated_first = first_request.clone();
    updated_first["source_message_id"] = draft["source_message_id"].clone();
    assert_eq!(
        call(&mut store, "draft_save", updated_first.clone()),
        first_receipt
    );
    updated_first["body"] = json!("Different human content");
    assert_eq!(
        store
            .handle("draft_save", Some(ACCOUNT), updated_first)
            .unwrap_err()
            .code,
        "request_id_conflict"
    );
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT fingerprint,result_json FROM operations WHERE request_id='old-first-save'",
                [],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            )
            .unwrap(),
        (first_fingerprint, first_json)
    );
    let capture = call(
        &mut store,
        "draft_capture",
        json!({"draft_id":THREAD,"expected_revision":9,"request_id":"old-capture"}),
    );
    assert_eq!(capture["state"], "accepted");
    assert_eq!(capture["attempts_started"], 1);
    assert_eq!(
        std::fs::read(capture["path"].as_str().unwrap()).unwrap(),
        mime
    );
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT json FROM capture_operations WHERE id='old-capture'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        record
    );
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT result_json FROM operations WHERE request_id='old-save'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        result_json
    );
    assert_eq!(
        store
            .db
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
        0
    );
}

#[test]
fn composition_maximum_escaped_subject_capture_remains_replayable() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let draft = create(&mut store, "large-subject");
    let subject = "\\\"".repeat(4096);
    assert_eq!(subject.len(), 8192);
    call(
        &mut store,
        "draft_save",
        json!({"draft_id":draft["draft_id"],"body":"Bounded body","subject":subject,"to_recipients":[{"email":"recipient@fixture.example"}],"expected_revision":1,"request_id":"save-large-subject"}),
    );
    let request = json!({"draft_id":draft["draft_id"],"expected_revision":2,"request_id":"capture-large-subject"});
    let captured = call(&mut store, "draft_capture", request.clone());
    assert_eq!(captured["state"], "accepted");
    let size=store.db.query_row("SELECT length(CAST(json AS BLOB)) FROM capture_operations WHERE id='capture-large-subject'",[],|r|r.get::<_,i64>(0)).unwrap();
    assert!(size > 16 * 1024 && size <= 32 * 1024);
    drop(store);
    let mut store = Store::open(dir.path()).unwrap();
    assert_eq!(call(&mut store, "draft_capture", request), captured);
    assert_eq!(
        call(
            &mut store,
            "capture_history",
            json!({"draft_id":draft["draft_id"]})
        )["captures"][0]["subject"],
        subject
    );
}
