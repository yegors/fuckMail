use super::*;
use crate::local_content::{prepare_local_content, LocalContentStep};
use fm_google::transport::CancellationToken;

fn call(store: &mut Store, method: &str, p: Value) -> Value {
    store.handle(method, Some("windscribe"), p).unwrap()
}
fn created(store: &mut Store, request: &str) -> Value {
    call(
        store,
        "draft_create",
        json!({"mode":"new","request_id":request}),
    )
}
fn discard(store: &mut Store, draft: &Value, request: &str) -> Value {
    call(
        store,
        "draft_discard",
        json!({"draft_id":draft["draft_id"],"expected_revision":draft["revision"],"request_id":request}),
    )
}
fn undo_params(draft: &Value, discard: &str, request: &str) -> Value {
    json!({"draft_id":draft["draft_id"],"expected_revision":draft["revision"],"discard_request_id":discard,"request_id":request})
}

#[test]
fn discard_undo_preserves_editor_content_identity_files_and_pending_recipients_across_restart() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open_synthetic(root.path()).unwrap();
    let (cid,mid):(String,String)=store.db.query_row("SELECT conversation_id,id FROM messages WHERE account='windscribe' ORDER BY id LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    let mut original = call(
        &mut store,
        "draft_create",
        json!({"mode":"reply","conversation_id":cid,"source_message_id":mid,"request_id":"undo-source"}),
    );
    original = call(
        &mut store,
        "draft_save",
        json!({"draft_id":original["draft_id"],"expected_revision":original["revision"],"body":"Previously saved words","body_html":"<p>Previously saved words</p>","signature_text":"Original signature","request_id":"undo-save"}),
    );
    let file = root.path().join("selected.bin");
    let bytes: Vec<u8> = (0..=255).collect();
    std::fs::write(&file, &bytes).unwrap();
    let LocalContentStep::Prepare(plan)=store.local_content_begin("draft_attachment_import","windscribe",json!({"draft_id":original["draft_id"],"expected_revision":original["revision"],"path":file,"request_id":"undo-file"})).unwrap() else {panic!("file plan")};
    let LocalContentStep::Reply(imported) = store
        .local_content_finish(prepare_local_content(plan, &CancellationToken::new()).unwrap())
        .unwrap()
    else {
        panic!("file reply")
    };
    original = imported["draft"].clone();
    let before = store
        .stored_composition("windscribe", &json!({"draft_id":original["draft_id"]}))
        .unwrap()
        .unwrap();
    let buffer = json!({"body":"Unsaved human words\nSecond line ✨","body_html":"<p>Unsaved <b>human words</b></p><p>Second line ✨</p>","signature_text":"Edited signature","subject":"Edited title","to_recipients":[{"name":"Zoë, Design","email":"zoe@example.test"}],"cc_recipients":[],"bcc_recipients":[{"email":"private@example.test"}],"attachment_ids":original["attachment_ids"],"recipient_inputs":{"bcc_recipients":"Unfinished <address@"}});
    let request = json!({"draft_id":original["draft_id"],"expected_revision":original["revision"],"request_id":"discard-editor","buffer":buffer});
    let discarded = call(&mut store, "draft_discard", request.clone());
    assert_eq!(discarded["body"], "\n\n—\nWindscribe");
    assert!(discarded["body_html"].is_null());
    assert_eq!(discarded["signature_text"], "");
    assert_eq!(discarded["attachment_ids"], json!([]));
    assert_eq!(discarded["recipient_inputs"], json!({}));
    assert!(!serde_json::to_string(&discarded)
        .unwrap()
        .contains("Unsaved human"));
    assert_eq!(
        call(&mut store, "draft_discard", request.clone()),
        discarded
    );
    let status = call(
        &mut store,
        "draft_discard_status",
        json!({"request_id":"discard-editor"}),
    );
    assert_eq!(status["discard"], discarded);
    assert_eq!(status["can_undo"], true);
    let mut conflicting = request;
    conflicting["buffer"]["body"] = json!("Different intent");
    assert_eq!(
        store
            .handle("draft_discard", Some("windscribe"), conflicting)
            .unwrap_err()
            .code,
        "request_id_conflict"
    );
    let prior_receipt:String=store.db.query_row("SELECT result_json FROM operations WHERE account='windscribe' AND request_id='discard-editor'",[],|r|r.get(0)).unwrap();
    drop(store);
    let mut store = Store::open_synthetic(root.path()).unwrap();
    let metadata: i64 = store
        .db
        .query_row(
            "SELECT revision FROM account_metadata WHERE account='windscribe'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    call(
        &mut store,
        "account_update",
        json!({"name":"Changed account name","signature_text":"Changed account default","expected_revision":metadata,"request_id":"changed-default"}),
    );
    let undo = undo_params(&discarded, "discard-editor", "restore-editor");
    let restored = call(&mut store, "draft_undo_discard", undo.clone());
    // The buffer's separate signature comes back as body text.
    assert_eq!(
        restored["body"],
        "Unsaved human words\nSecond line ✨\n\nEdited signature"
    );
    assert_eq!(
        restored["body_html"],
        "<p>Unsaved <b>human words</b></p><p>Second line ✨</p><br><br>Edited signature"
    );
    assert_eq!(restored["signature_text"], "");
    for field in [
        "subject",
        "to_recipients",
        "cc_recipients",
        "bcc_recipients",
        "attachment_ids",
        "recipient_inputs",
    ] {
        assert_eq!(restored[field], buffer[field], "{field}");
    }
    for field in [
        "from",
        "from_name",
        "identity_revision",
        "source_message_id",
        "source_hash",
        "reply_to_message_id",
        "references",
        "conversation_id",
        "mode",
        "created_at",
    ] {
        assert_eq!(restored[field], original[field], "{field}");
    }
    assert_eq!(
        call(
            &mut store,
            "draft_get",
            json!({"draft_id":restored["draft_id"]})
        )["recipient_inputs"],
        buffer["recipient_inputs"]
    );
    let retained = store
        .retained_draft(
            "windscribe",
            json!({"draft_id":restored["draft_id"],"expected_revision":restored["revision"]}),
        )
        .unwrap();
    assert_eq!(
        retained
            .prepare(&CancellationToken::new())
            .err()
            .unwrap()
            .code,
        "invalid_draft"
    );
    let clean = call(
        &mut store,
        "draft_save",
        json!({"draft_id":restored["draft_id"],"expected_revision":restored["revision"],"body":restored["body"],"bcc_recipients":restored["bcc_recipients"],"request_id":"resolve-pending"}),
    );
    assert_eq!(clean["recipient_inputs"], json!({}));
    let files = store
        .retained_draft(
            "windscribe",
            json!({"draft_id":clean["draft_id"],"expected_revision":clean["revision"]}),
        )
        .unwrap()
        .prepare(&CancellationToken::new())
        .unwrap();
    assert_eq!(files[0].bytes, bytes);
    assert_eq!(call(&mut store, "draft_undo_discard", undo), restored);
    assert_eq!(
        call(
            &mut store,
            "draft_get",
            json!({"draft_id":clean["draft_id"]})
        )["revision"],
        clean["revision"]
    );
    assert_eq!(store.db.query_row("SELECT result_json FROM operations WHERE account='windscribe' AND request_id='discard-editor'",[],|r|r.get::<_,String>(0)).unwrap(),prior_receipt);
    assert_eq!(
        store
            .stored_composition("windscribe", &json!({"draft_id":clean["draft_id"]}))
            .unwrap()
            .unwrap()["origin"],
        before["origin"]
    );
}

#[test]
fn discard_undo_refuses_newer_edits_other_accounts_and_changed_lifecycles() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open_synthetic(root.path()).unwrap();
    let original = created(&mut store, "stale-source");
    let other = created(&mut store, "different-draft");
    let removed = discard(&mut store, &original, "stale-discard");
    let undo = undo_params(&removed, "stale-discard", "stale-undo");
    let mut wrong = undo.clone();
    wrong["draft_id"] = other["draft_id"].clone();
    assert_eq!(
        store
            .handle("draft_undo_discard", Some("windscribe"), wrong)
            .unwrap_err()
            .code,
        "invalid_request"
    );
    assert!(store
        .handle("draft_undo_discard", Some("personal"), undo.clone())
        .is_err());
    let newer = call(
        &mut store,
        "draft_save",
        json!({"draft_id":removed["draft_id"],"expected_revision":removed["revision"],"body":"Newer human edit","request_id":"newer-edit"}),
    );
    assert_eq!(
        store
            .handle("draft_undo_discard", Some("windscribe"), undo)
            .unwrap_err()
            .code,
        "stale_revision"
    );
    assert_eq!(
        call(
            &mut store,
            "draft_get",
            json!({"draft_id":newer["draft_id"]})
        )["body"],
        "Newer human edit"
    );
    let removed = discard(&mut store, &newer, "lifecycle-discard");
    store
        .db
        .execute(
            "UPDATE accounts SET lifecycle_revision=lifecycle_revision+1 WHERE id='windscribe'",
            [],
        )
        .unwrap();
    assert_eq!(
        store
            .handle(
                "draft_undo_discard",
                Some("windscribe"),
                undo_params(&removed, "lifecycle-discard", "lifecycle-undo")
            )
            .unwrap_err()
            .code,
        "stale_revision"
    );
    assert_eq!(
        call(
            &mut store,
            "draft_discard_status",
            json!({"request_id":"lifecycle-discard"})
        )["can_undo"],
        false
    );
}

#[test]
fn discard_buffer_validation_is_atomic_and_unfinished_inputs_survive_normal_autosave() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open_synthetic(root.path()).unwrap();
    let mut draft = created(&mut store, "buffer-source");
    for buffer in [
        json!({"from":"another@example.test"}),
        json!({"attachment_ids":["outside-account"]}),
        json!({"recipient_inputs":{"from":"invalid field"}}),
    ] {
        assert!(store.handle("draft_discard",Some("windscribe"),json!({"draft_id":draft["draft_id"],"expected_revision":draft["revision"],"request_id":"invalid-buffer","buffer":buffer})).is_err());
        assert_eq!(
            call(
                &mut store,
                "draft_get",
                json!({"draft_id":draft["draft_id"]})
            )["discarded"],
            false
        );
    }
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM draft_discards", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    draft = call(
        &mut store,
        "draft_save",
        json!({"draft_id":draft["draft_id"],"expected_revision":draft["revision"],"body":"Words remain saved while typing an address","recipient_inputs":{"to_recipients":"Still typing <"},"request_id":"pending-autosave"}),
    );
    draft = call(
        &mut store,
        "draft_save",
        json!({"draft_id":draft["draft_id"],"expected_revision":draft["revision"],"body":"More words","request_id":"body-only-autosave"}),
    );
    assert_eq!(draft["recipient_inputs"]["to_recipients"], "Still typing <");
    drop(store);
    let mut store = Store::open_synthetic(root.path()).unwrap();
    let read = call(
        &mut store,
        "draft_get",
        json!({"draft_id":draft["draft_id"]}),
    );
    assert_eq!(read["recipient_inputs"], draft["recipient_inputs"]);
    assert_eq!(read["body"], "More words");
}
