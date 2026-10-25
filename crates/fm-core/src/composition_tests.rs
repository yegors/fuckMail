use crate::*;
use fm_google::sync::{prepare_full_page, ListPage, MessageContent, MessageRef, PageStore};
use google_store::GooglePageStore;
use mail_parser::MimeHeaders;
use rusqlite::params;

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
        0
    );
    // Bare words search subject and text; from: names a draft's recipients.
    assert_eq!(
        call(
            &mut store,
            "drafts",
            json!({"query":"release from:private"})
        )["total"],
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
    // It reopens like a new draft: blank lines, then the current signature.
    assert_eq!(discarded["body"], "\n\nChanged default");
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
        "I will review the rollout window and confirm the timing.\n\n—\nWindscribe"
    );
}

#[test]
fn composition_reply_all_and_forward_freeze_retained_source_and_only_forward_carries_attachments() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let (cid, mid) = ingest(
        &mut store,
        "original",
        source_with_attachment("original", 0),
    );
    let attachment: String = store
        .db
        .query_row(
            "SELECT id FROM attachments WHERE account=?1 AND message_id=?2",
            params![ACCOUNT, mid],
            |r| r.get(0),
        )
        .unwrap();
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
    // A forward starts with the source's attachments; replies start with none.
    assert_eq!(reply["attachment_ids"], json!([]));
    assert_eq!(all["attachment_ids"], json!([]));
    assert_eq!(forward["attachment_ids"], json!([attachment]));
    assert_eq!(forward["attachments"][0]["name"], "retained.txt");
    assert_eq!(forward["attachments"][0]["state"], "available");
    assert_eq!(forward["references"], json!(["<ancestor@fixture.example>"]));
    assert_eq!(forward["reply_to_message_id"], "<original@fixture.example>");
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
    assert_eq!(
        parsed.in_reply_to().as_text(),
        Some("original@fixture.example")
    );
    let forwarded: Vec<_> = parsed.attachments().collect();
    assert_eq!(forwarded.len(), 1);
    assert_eq!(forwarded[0].attachment_name(), Some("retained.txt"));
    assert!(forwarded[0]
        .contents()
        .starts_with(b"Retained attachment bytes."));
    assert!(parsed.body_text(0).unwrap().contains("Human preface"));
    // The writer can drop forwarded attachments like any other.
    let trimmed = call(
        &mut store,
        "draft_create",
        json!({"mode":"forward","conversation_id":cid,"source_message_id":mid,"request_id":"forward-trimmed"}),
    );
    assert_eq!(trimmed["attachment_ids"], json!([attachment]));
    let saved = call(
        &mut store,
        "draft_save",
        json!({"draft_id":trimmed["draft_id"],"expected_revision":1,"request_id":"save-trimmed","body":"Without the file","attachment_ids":[],"to_recipients":[{"email":"forward@fixture.example"}]}),
    );
    assert_eq!(saved["attachments"], json!([]));
    let captured = call(
        &mut store,
        "draft_capture",
        json!({"draft_id":trimmed["draft_id"],"expected_revision":2,"request_id":"capture-trimmed"}),
    );
    let bytes = std::fs::read(captured["path"].as_str().unwrap()).unwrap();
    let parsed = mail_parser::MessageParser::default().parse(&bytes).unwrap();
    assert_eq!(parsed.attachments().count(), 0);
}

#[test]
fn composition_legacy_first_save_binds_loaded_member_across_actual_arrival() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let (cid, mid) = ingest(&mut store, "loaded", source("loaded", 0));
    let loaded = call(&mut store, "draft_get", json!({"conversation_id":cid}));
    assert_eq!(loaded["revision"], 0);
    assert_eq!(loaded["source_message_id"], mid);
    // The source's Reply-To, not its From (a no-reply sender bounces).
    assert_eq!(
        emails(&loaded["to_recipients"]),
        vec!["reply@fixture.example"]
    );
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
    assert_eq!(
        call(&mut store, "drafts", json!({"query":"paged uni"}))["total"],
        0
    );
    // Drafts match phrases as mail does.
    for (query, total) in [
        ("safe café", 3),
        ("punctuation-safe cafe", 3),
        ("punctuation safe", 0),
        ("draft 1", 1),
        ("afe", 0),
    ] {
        assert_eq!(
            call(&mut store, "drafts", json!({"query":query}))["total"],
            total,
            "{query}"
        );
    }
    let first = call(
        &mut store,
        "drafts",
        json!({"query":"paged from:uni","limit":2}),
    );
    assert_eq!(first["draft_count"], 4);
    assert_eq!(first["total"], 3);
    assert_eq!(first["next_offset"], 2);
    let last = call(
        &mut store,
        "drafts",
        json!({"query":"paged from:uni","limit":2,"offset":2}),
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
        json!({"query":"label:work"}),
        json!({"query":"is:done"}),
        json!({"query":"has:attachment"}),
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

#[test]
fn drafts_stored_with_bare_reply_ids_still_build() {
    // Earlier versions mirrored Gmail drafts with Message-IDs lacking their
    // angle brackets; sending such a draft must not fail.
    let draft = json!({"from":"me@example.test","from_name":"Me","subject":"Re: Invoice","body":"All paid.",
        "to_recipients":[{"email":"dave@example.test"}],"cc_recipients":[],"bcc_recipients":[],
        "reply_to_message_id":"MW4PR15MB4377@MW4PR15MB4377.namprd15.prod.outlook.com",
        "references":["root@example.test","<MW4PR15MB4377@MW4PR15MB4377.namprd15.prod.outlook.com>"]});
    let (request, identity) = crate::composition::compose_request(
        ACCOUNT,
        &draft,
        "<new@example.test>".into(),
        1_791_241_000,
    )
    .unwrap();
    assert_eq!(
        request.reply_to_message_id.as_deref(),
        Some("<MW4PR15MB4377@MW4PR15MB4377.namprd15.prod.outlook.com>")
    );
    assert_eq!(
        request.references,
        [
            "<root@example.test>",
            "<MW4PR15MB4377@MW4PR15MB4377.namprd15.prod.outlook.com>"
        ]
    );
    let built = fm_mail::build_mail(&request, &identity, &[]).unwrap();
    let parsed = mail_parser::MessageParser::default()
        .parse(&built.raw)
        .unwrap();
    assert_eq!(
        parsed.in_reply_to().as_text(),
        Some("MW4PR15MB4377@MW4PR15MB4377.namprd15.prod.outlook.com")
    );
}

#[test]
fn subjects_with_line_breaks_reply_and_save_on_one_line() {
    use crate::composition::one_line_subject;
    for (text, line) in [
        ("Order 4417 - 0042\r\n", "Order 4417 - 0042"),
        ("\tOrder\r\n 4417", "Order 4417"),
        ("Two  spaces stay ", "Two  spaces stay "),
        ("Line\u{2028}separator", "Line separator"),
    ] {
        assert_eq!(one_line_subject(text), line, "{text:?}");
    }
    // An encoded subject can decode to a line break and a tab. Replying to it
    // used to fail with "Subject must be one line".
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let bytes = String::from_utf8(source("broken-subject", 0))
        .unwrap()
        .replace(
            "Subject: Frozen source context",
            "Subject: =?UTF-8?Q?Order_4417_-=0A=090042?=",
        )
        .into_bytes();
    let (cid, mid) = ingest(&mut store, "broken-subject", bytes);
    for (mode, prefix) in [("reply", "Re:"), ("forward", "Fwd:")] {
        let draft = call(
            &mut store,
            "draft_create",
            json!({"mode":mode,"conversation_id":cid,"source_message_id":mid,"request_id":mode}),
        );
        assert_eq!(draft["subject"], format!("{prefix} Order 4417 - 0042"));
        let saved = call(
            &mut store,
            "draft_save",
            json!({"draft_id":draft["draft_id"],"expected_revision":1,"request_id":format!("{mode}-save"),"body":"Thanks","subject":draft["subject"]}),
        );
        assert_eq!(saved["revision"], 2);
    }
    let legacy = call(&mut store, "draft_get", json!({"conversation_id":cid}));
    assert!(!legacy["subject"]
        .as_str()
        .unwrap()
        .chars()
        .any(char::is_control));
    let saved = call(
        &mut store,
        "draft_save",
        json!({"conversation_id":cid,"expected_revision":0,"request_id":"legacy-save","body":"Thanks"}),
    );
    assert_eq!(saved["subject"], legacy["subject"]);
}

#[test]
fn forwards_open_with_two_empty_lines_above_the_quote() {
    use crate::composition::signed_body;
    assert_eq!(signed_body("", ""), "");
    assert_eq!(signed_body("Quote", ""), "\n\nQuote");
    assert_eq!(signed_body("", "Sig"), "\n\nSig");
    assert_eq!(signed_body("Quote", "Sig"), "\n\nSig\n\nQuote");
    // Without a signature the forwarded text used to be the first line, so
    // there was nowhere to type above it.
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    call(
        &mut store,
        "account_update",
        json!({"name":"Owner","signature_text":"","expected_revision":0,"request_id":"no-signature"}),
    );
    let (cid, mid) = ingest(
        &mut store,
        "unsigned-forward",
        source("unsigned-forward", 0),
    );
    let forward = call(
        &mut store,
        "draft_create",
        json!({"mode":"forward","conversation_id":cid,"source_message_id":mid,"request_id":"unsigned-forward"}),
    );
    assert!(forward["body"]
        .as_str()
        .unwrap()
        .starts_with("\n\n---------- Forwarded message ----------\n"));
    let reply = call(
        &mut store,
        "draft_create",
        json!({"mode":"reply","conversation_id":cid,"source_message_id":mid,"request_id":"unsigned-reply"}),
    );
    assert_eq!(reply["body"], "");
}

fn html_source(remote: &str) -> Vec<u8> {
    let plain = String::from_utf8(source(remote, 0)).unwrap();
    plain.replace(
        "Content-Type: text/plain; charset=utf-8\r\n\r\nOriginal retained text",
        "Content-Type: multipart/alternative; boundary=alt\r\n\r\n--alt\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nPlain words of the original\r\n--alt\r\nContent-Type: multipart/related; boundary=rel\r\n\r\n--rel\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<html><head><style>.brand{color:#0a6}</style></head><body><h1 class=\"brand\">Quarterly numbers</h1><img src=\"cid:Logo.Part@fixture.example\"></body></html>\r\n--rel\r\nContent-Type: image/png\r\nContent-ID: <Logo.Part@fixture.example>\r\nContent-Disposition: inline; filename=logo.png\r\nContent-Transfer-Encoding: base64\r\n\r\niVBORw0KGgo=\r\n--rel--\r\n--alt--\r\nTrailing",
    )
    .into_bytes()
}

#[test]
fn html_forwards_send_the_original_as_received() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    call(
        &mut store,
        "account_update",
        json!({"name":"Owner","signature_text":"","expected_revision":0,"request_id":"no-signature"}),
    );
    let (cid, mid) = ingest(&mut store, "html-original", html_source("html-original"));
    let create = |store: &mut Store, request: &str, format: Value| {
        let mut p = json!({"mode":"forward","conversation_id":cid,"source_message_id":mid,"request_id":request});
        if !format.is_null() {
            p["forward_format"] = format;
        }
        store.handle("draft_create", Some(ACCOUNT), p)
    };
    assert_eq!(
        create(&mut store, "bad-format", json!("rtf"))
            .unwrap_err()
            .code,
        "invalid_request"
    );
    // Text, the default, quotes the original in the editable body.
    let text = create(&mut store, "text-forward", Value::Null).unwrap();
    assert!(text["forward_html"].is_null());
    assert!(text["body"]
        .as_str()
        .unwrap()
        .contains("> Plain words of the original"));
    // HTML leaves the body to the author; the original follows on sending.
    let forward = create(&mut store, "html-forward", json!("html")).unwrap();
    assert_eq!(forward["forward_html"], true);
    assert_eq!(forward["body"], "");
    let saved = call(
        &mut store,
        "draft_save",
        json!({"draft_id":forward["draft_id"],"expected_revision":1,"request_id":"html-save","body":"See below.","to_recipients":[{"email":"reader@fixture.example"}]}),
    );
    assert_eq!(
        saved["forward_html"], true,
        "a save keeps the forward's format"
    );
    let mut parts = Vec::new();
    let (request, identity) = crate::composition::compose_prepared(
        &store.google_blob_store(),
        ACCOUNT,
        &saved,
        "<html-forward@fixture.example>".into(),
        1_791_241_000,
        &mut parts,
    )
    .unwrap();
    assert_eq!(parts.len(), 1, "the inline logo travels with the HTML");
    assert_eq!(
        parts[0].content_id.as_deref(),
        Some("Logo.Part@fixture.example")
    );
    let built = fm_mail::build_mail(&request, &identity, &parts).unwrap();
    let parsed = mail_parser::MessageParser::default()
        .parse(&built.raw)
        .unwrap();
    let html = parsed.body_html(0).unwrap();
    let author = html.find("See below.").unwrap();
    let header = html
        .find("---------- Forwarded message ----------")
        .unwrap();
    assert!(author < header);
    assert!(html
        .contains("<style>.brand{color:#0a6}</style><h1 class=\"brand\">Quarterly numbers</h1>"));
    assert!(html.contains("src=\"cid:Logo.Part@fixture.example\""));
    assert!(
        !html.contains("<body"),
        "the original's document wrapper is not nested"
    );
    let plain = parsed.body_text(0).unwrap();
    assert!(plain.starts_with("See below.\n\n---------- Forwarded message ----------"));
    assert!(plain.contains("> Plain words of the original"));
    let logo = parsed
        .parts
        .iter()
        .find(|part| {
            mail_parser::MimeHeaders::content_id(*part) == Some("Logo.Part@fixture.example")
        })
        .expect("inline part keeps its Content-ID");
    assert_eq!(logo.contents(), b"\x89PNG\r\n\x1a\n");
    // A message without HTML forwards as text even when HTML was asked for.
    let (plain_cid, plain_mid) = ingest(&mut store, "plain-original", source("plain-original", 0));
    let fallback = call(
        &mut store,
        "draft_create",
        json!({"mode":"forward","conversation_id":plain_cid,"source_message_id":plain_mid,"request_id":"plain-forward","forward_format":"html"}),
    );
    assert!(fallback["forward_html"].is_null());
    assert!(fallback["body"]
        .as_str()
        .unwrap()
        .contains("> Original retained text"));
}

#[test]
fn from_name_is_the_chosen_one_then_gmail_then_a_display_name_that_is_not_the_address() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let mut revision = 0;
    let mut update = |store: &mut Store, p: Value| -> Result<Value> {
        let mut p = p;
        p["signature_text"] = json!("");
        p["expected_revision"] = json!(revision);
        p["request_id"] = json!(format!("from-name-{revision}"));
        let row = store.handle("account_update", Some(ACCOUNT), p)?;
        revision += 1;
        Ok(row)
    };
    // Sends a new draft as a local capture and returns its From mailbox.
    let sent_from = |store: &mut Store, request: &str| -> (Option<String>, String) {
        let id = create(store, request)["draft_id"].clone();
        call(
            store,
            "draft_save",
            json!({"draft_id":id,"expected_revision":1,"request_id":format!("{request}-save"),
                "body":"Hello","subject":"From name","to_recipients":[{"email":"ada@fixture.example"}]}),
        );
        let captured = call(
            store,
            "draft_capture",
            json!({"draft_id":id,"expected_revision":2,"request_id":format!("{request}-capture"),"outcome":"unknown"}),
        );
        let bytes = std::fs::read(captured["path"].as_str().unwrap()).unwrap();
        let parsed = mail_parser::MessageParser::default().parse(&bytes).unwrap();
        let from = parsed.from().unwrap().first().unwrap();
        (
            from.name().map(str::to_owned),
            from.address().unwrap().to_owned(),
        )
    };

    // Without a choice or a Gmail name, a display name that is a real name is used...
    let row = update(&mut store, json!({"name":"Alex at work"})).unwrap();
    assert_eq!(row["from_name"], "");
    assert_eq!(row["default_from_name"], "Alex at work");
    assert_eq!(create(&mut store, "display")["from_name"], "Alex at work");
    // ...but one that is only the address leaves the name out.
    update(&mut store, json!({"name":"YOU@windscribe.example"})).unwrap();
    assert_eq!(
        sent_from(&mut store, "address-only"),
        (None, "you@windscribe.example".to_owned())
    );

    // Gmail's send-as name comes before the display name.
    assert!(store.google_send_as_name(ACCOUNT, " Alex Gmail ").unwrap());
    assert!(!store.google_send_as_name(ACCOUNT, "Alex Gmail").unwrap());
    let before_choice = create(&mut store, "gmail");
    assert_eq!(before_choice["from_name"], "Alex Gmail");

    // The person's choice comes first, and drafts already started keep theirs.
    let row = update(
        &mut store,
        json!({"name":"Work","from_name":"  Alex Example "}),
    )
    .unwrap();
    assert_eq!(row["from_name"], "Alex Example");
    assert_eq!(row["default_from_name"], "Alex Gmail");
    assert_eq!(
        sent_from(&mut store, "chosen"),
        (
            Some("Alex Example".to_owned()),
            "you@windscribe.example".to_owned()
        )
    );
    assert_eq!(
        call(
            &mut store,
            "draft_get",
            json!({"draft_id":before_choice["draft_id"]})
        )["from_name"],
        "Alex Gmail"
    );
    // Leaving `from_name` out keeps it; an empty one follows Gmail again.
    assert_eq!(
        update(&mut store, json!({"name":"Work"})).unwrap()["from_name"],
        "Alex Example"
    );
    update(&mut store, json!({"name":"Work","from_name":""})).unwrap();
    assert_eq!(create(&mut store, "cleared")["from_name"], "Alex Gmail");

    for bad in [json!("Tab\there"), json!("x".repeat(81)), json!(7)] {
        let error = update(&mut store, json!({"name":"Work","from_name":bad})).unwrap_err();
        assert_eq!(error.code, "invalid_request");
    }
}
