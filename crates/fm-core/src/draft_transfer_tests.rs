use super::*;
use crate::local_content::{prepare_local_content, LocalContentStep};

fn move_draft(store: &mut Store, p: &Value) -> Result<Value> {
    match store.draft_transfer_begin("windscribe", p)? {
        TransferStep::Reply(reply) => Ok(reply),
        TransferStep::Prepare(plan) => {
            store.draft_transfer_finish(prepare(plan, CancellationToken::new())?)
        }
    }
}
fn created(store: &mut Store, request: &str) -> Value {
    store
        .handle(
            "draft_create",
            Some("windscribe"),
            json!({"mode":"new","request_id":request}),
        )
        .unwrap()
}
fn parameters(draft: &Value, request: &str) -> Value {
    json!({"draft_id":draft["draft_id"],"expected_revision":draft["revision"],"expected_google_revision":draft["google"]["revision"],"target_account":"personal","request_id":request})
}

#[test]
fn transfer_preserves_authored_fields_and_file_ownership_across_replay_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open_synthetic(dir.path()).unwrap();
    let mut draft = created(&mut store, "transfer-source");
    draft=store.handle("draft_save",Some("windscribe"),json!({"draft_id":draft["draft_id"],"expected_revision":draft["revision"],"request_id":"source-fields","body":"Human words\nSecond line","body_html":"<p>Human <b>words</b></p>","signature_text":"My custom signature","subject":"Transfer subject","to_recipients":[{"name":"Zoë, Design","email":"zoe@example.test"}],"cc_recipients":[],"bcc_recipients":[{"email":"hidden@example.test"}]})).unwrap();
    let file = dir.path().join("source-file.txt");
    std::fs::write(&file, b"Exact file bytes\0\xff").unwrap();
    let import = json!({"draft_id":draft["draft_id"],"expected_revision":draft["revision"],"request_id":"source-file","path":file.to_str().unwrap()});
    let LocalContentStep::Prepare(plan) = store
        .local_content_begin("draft_attachment_import", "windscribe", import)
        .unwrap()
    else {
        panic!("file plan")
    };
    let proof = prepare_local_content(plan, &CancellationToken::new()).unwrap();
    let LocalContentStep::Reply(imported) = store.local_content_finish(proof).unwrap() else {
        panic!("file receipt")
    };
    draft = imported["draft"].clone();
    let original = store
        .stored_composition("windscribe", &json!({"draft_id":draft["draft_id"]}))
        .unwrap()
        .unwrap();
    let p = parameters(&draft, "move-account");
    let result = move_draft(&mut store, &p).unwrap();
    assert_eq!(result["state"], "complete");
    let target = &result["draft"];
    for field in [
        "subject",
        "to_recipients",
        "cc_recipients",
        "bcc_recipients",
        "body",
        "body_html",
        "signature_text",
    ] {
        assert_eq!(target[field], draft[field], "{field}");
    }
    assert_ne!(target["from"], draft["from"]);
    assert_eq!(target["account"], "personal");
    assert!(target["conversation_id"].is_null() && target["source_message_id"].is_null());
    assert_ne!(target["attachment_ids"], draft["attachment_ids"]);
    let hash: String = store
        .db
        .query_row(
            "SELECT blob_hash FROM attachments WHERE account='personal' AND id=?1",
            [target["attachment_ids"][0].as_str().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hash, imported["attachment"]["sha256"]);
    let retained = store
        .stored_composition("windscribe", &json!({"draft_id":draft["draft_id"]}))
        .unwrap()
        .unwrap();
    for field in [
        "body",
        "body_html",
        "signature_text",
        "routing_json",
        "attachments_json",
        "from_name",
    ] {
        assert_eq!(retained[field], original[field]);
    }
    assert_eq!(retained["discarded"], true);
    assert_eq!(store.handle("draft_save",Some("windscribe"),json!({"draft_id":draft["draft_id"],"expected_revision":retained["revision"],"request_id":"cannot-resurrect","body":"Wrong account"})).unwrap_err().code,"draft_read_only");
    assert_eq!(move_draft(&mut store, &p).unwrap(), result);
    let mut wrong = p.clone();
    wrong["target_account"] = json!("windscribe");
    assert_eq!(
        move_draft(&mut store, &wrong).unwrap_err().code,
        "request_id_conflict"
    );
    drop(store);
    let mut store = Store::open_synthetic(dir.path()).unwrap();
    assert_eq!(move_draft(&mut store, &p).unwrap(), result);
    assert_eq!(
        store
            .draft_transfer_status("windscribe", "move-account")
            .unwrap()["state"],
        "complete"
    );
    assert_eq!(
        store
            .draft_transfer_status("personal", "move-account")
            .unwrap_err()
            .code,
        "not_found"
    );
    let retained = store
        .retained_draft(
            "personal",
            json!({"draft_id":target["draft_id"],"expected_revision":target["revision"]}),
        )
        .unwrap();
    let files = retained.prepare(&CancellationToken::new()).unwrap();
    assert_eq!(files[0].account_id, "personal");
    assert_eq!(files[0].bytes, b"Exact file bytes\0\xff");
}

#[test]
fn preparation_rechecks_source_destination_and_cancellation_without_retiring_original() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open_synthetic(dir.path()).unwrap();
    let draft = created(&mut store, "preparation-source");
    let p = parameters(&draft, "prepare-account");
    let TransferStep::Prepare(plan) = store.draft_transfer_begin("windscribe", &p).unwrap() else {
        panic!("plan")
    };
    let proof = prepare(plan, CancellationToken::new()).unwrap();
    store
        .db
        .execute(
            "UPDATE account_metadata SET revision=revision+1 WHERE account='personal'",
            [],
        )
        .unwrap();
    assert_eq!(
        store.draft_transfer_finish(proof).unwrap_err().code,
        "stale_revision"
    );
    let TransferStep::Prepare(plan) = store.draft_transfer_begin("windscribe", &p).unwrap() else {
        panic!("plan")
    };
    let cancel = CancellationToken::new();
    let proof = prepare(plan, cancel.clone()).unwrap();
    cancel.cancel();
    assert_eq!(
        store.draft_transfer_finish(proof).unwrap_err().code,
        "cancelled"
    );
    assert!(!store
        .composition("windscribe", &json!({"draft_id":draft["draft_id"]}))
        .unwrap()["discarded"]
        .as_bool()
        .unwrap());
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM draft_transfers", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let mut copy = p.clone();
    copy["keep_original"] = json!(true);
    let result = move_draft(&mut store, &copy).unwrap();
    assert_eq!(result["keep_original"], true);
    assert!(!store
        .composition("windscribe", &json!({"draft_id":draft["draft_id"]}))
        .unwrap()["read_only"]
        .as_bool()
        .unwrap());
}

#[test]
fn transferred_reply_keeps_rfc_context_and_rejects_changes_during_preparation() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open_synthetic(dir.path()).unwrap();
    let (conversation, message) = crate::composition_tests::ingest(
        &mut store,
        "move-reply",
        crate::composition_tests::source("move-reply", 0),
    );
    let reply=store.handle("draft_create",Some("windscribe"),json!({"mode":"reply_all","conversation_id":conversation,"source_message_id":message,"request_id":"source-reply"})).unwrap();
    let p = parameters(&reply, "move-reply-account");
    let TransferStep::Prepare(plan) = store.draft_transfer_begin("windscribe", &p).unwrap() else {
        panic!("plan")
    };
    let proof = prepare(plan, CancellationToken::new()).unwrap();
    store.db.execute("UPDATE drafts SET revision=revision+1,body='Newer authored text' WHERE account='windscribe' AND draft_id=?1",[reply["draft_id"].as_str().unwrap()]).unwrap();
    assert_eq!(
        store.draft_transfer_finish(proof).unwrap_err().code,
        "stale_revision"
    );
    let updated = store
        .composition("windscribe", &json!({"draft_id":reply["draft_id"]}))
        .unwrap();
    let moved = move_draft(&mut store, &parameters(&updated, "move-updated-reply")).unwrap();
    let target = &moved["draft"];
    for key in [
        "reply_to_message_id",
        "references",
        "source_hash",
        "subject",
        "to_recipients",
        "cc_recipients",
        "body",
    ] {
        assert_eq!(target[key], updated[key], "{key}");
    }
    assert_eq!(
        target["reply_to_message_id"],
        "<move-reply@fixture.example>"
    );
    assert!(target["conversation_id"].is_null() && target["source_message_id"].is_null());
    let retained = store
        .retained_draft(
            "personal",
            json!({"draft_id":target["draft_id"],"expected_revision":1}),
        )
        .unwrap();
    retained.prepare(&CancellationToken::new()).unwrap();
    assert_eq!(
        target["account_transfer"]["target_draft_id"],
        target["draft_id"]
    );
}
