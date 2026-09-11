use fm_core::Store;
use serde_json::{json, Value};

fn call(store: &mut Store, method: &str, params: Value) -> Value {
    store.handle(method, Some("windscribe"), params).unwrap()
}
fn thread(store: &mut Store) -> Value {
    let list = call(store, "conversations", json!({"folder":"all","limit":100}));
    let row = list["conversations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["message_count"].as_u64().unwrap() >= 2)
        .unwrap();
    call(store, "conversation", json!({"id":row["id"]}))
}
fn flag_request(detail: &Value, request: &str, indices: &[usize]) -> Value {
    json!({"id":detail["id"],"expected_revision":detail["revision"],"request_id":request,
        "message_ids":indices.iter().map(|i|detail["messages"][i]["id"].clone()).collect::<Vec<_>>()})
}

#[test]
fn explicit_flags_target_only_snapshot_and_remain_independent() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let initial = thread(&mut store);
    let untouched = initial["messages"][1].clone();
    let mut p = flag_request(&initial, "flag-unread-star", &[0]);
    p["read"] = json!(false);
    p["starred"] = json!(true);
    let starred = call(&mut store, "flag", p);
    assert_eq!(starred["messages"][0]["unread"], true);
    assert_eq!(starred["messages"][0]["starred"], true);
    let mut p = flag_request(&starred, "mark-only-read", &[0]);
    p["read"] = json!(true);
    let read = call(&mut store, "flag", p);
    assert_eq!(read["messages"][0]["unread"], false);
    assert_eq!(read["messages"][0]["starred"], true);
    assert_eq!(read["messages"][1], untouched);
    let mut p = flag_request(&read, "clear-only-star", &[0]);
    p["starred"] = json!(false);
    let unstarred = call(&mut store, "flag", p);
    assert_eq!(unstarred["messages"][0]["unread"], false);
    assert_eq!(unstarred["messages"][0]["starred"], false);
}

#[test]
fn journal_replay_returns_original_response_across_restart_and_later_edits() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let initial = thread(&mut store);
    let mut request = flag_request(&initial, "retry-after-restart", &[0]);
    request["read"] = json!(true);
    let original = call(&mut store, "flag", request.clone());
    let mut later = flag_request(&original, "later-change", &[0]);
    later["starred"] = json!(true);
    let current = call(&mut store, "flag", later);
    drop(store);
    let mut store = Store::open(temp.path()).unwrap();
    assert_eq!(call(&mut store, "flag", request.clone()), original);
    assert_eq!(
        call(&mut store, "conversation", json!({"id":initial["id"]})),
        current
    );
    request["read"] = json!(false);
    assert_eq!(
        store
            .handle("flag", Some("windscribe"), request)
            .unwrap_err()
            .code,
        "request_id_conflict"
    );
}

#[test]
fn stale_and_cross_account_snapshot_rejections_have_no_partial_effect() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let initial = thread(&mut store);
    let other = store
        .handle(
            "conversations",
            Some("personal"),
            json!({"limit":1,"folder":"all"}),
        )
        .unwrap();
    let other = store
        .handle(
            "conversation",
            Some("personal"),
            json!({"id":other["conversations"][0]["id"]}),
        )
        .unwrap();
    let mut request = flag_request(&initial, "invalid-second-target", &[0]);
    request["read"] = json!(true);
    request["message_ids"]
        .as_array_mut()
        .unwrap()
        .push(other["messages"][0]["id"].clone());
    assert_eq!(
        store
            .handle("flag", Some("windscribe"), request)
            .unwrap_err()
            .code,
        "not_found"
    );
    assert_eq!(
        call(&mut store, "conversation", json!({"id":initial["id"]})),
        initial
    );
    let mut request = flag_request(&initial, "stale-revision", &[0]);
    request["read"] = json!(true);
    request["expected_revision"] = json!(0);
    let error = store
        .handle("flag", Some("windscribe"), request)
        .unwrap_err();
    assert_eq!(error.code, "stale_revision");
    assert_eq!(error.details["current_revision"], initial["revision"]);
    assert_eq!(
        call(&mut store, "conversation", json!({"id":initial["id"]})),
        initial
    );
}

#[test]
fn draft_unicode_and_empty_discard_survive_restart_without_changing_flags() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let initial = thread(&mut store);
    let body = "I wrote this myself. Café €42\n日本語 — спасибо\n> Earlier reply";
    let request = json!({"conversation_id":initial["id"],"expected_revision":0,"request_id":"human-draft","body":body});
    let saved = call(&mut store, "draft_save", request.clone());
    assert_eq!(saved["revision"], 1);
    assert_eq!(saved["body"], body);
    assert_eq!(
        call(&mut store, "conversation", json!({"id":initial["id"]})),
        initial
    );
    drop(store);
    let mut store = Store::open(temp.path()).unwrap();
    assert_eq!(
        call(
            &mut store,
            "draft_get",
            json!({"conversation_id":initial["id"]})
        ),
        saved
    );
    assert_eq!(call(&mut store, "draft_save", request), saved);
    let cleared = call(
        &mut store,
        "draft_save",
        json!({"conversation_id":initial["id"],"expected_revision":1,"request_id":"explicit-clear","body":""}),
    );
    assert_eq!(cleared["body"], "");
    assert_eq!(cleared["revision"], 2);
    drop(store);
    let mut store = Store::open(temp.path()).unwrap();
    assert_eq!(
        call(
            &mut store,
            "draft_get",
            json!({"conversation_id":initial["id"]})
        ),
        cleared
    );
}

#[test]
fn manual_categories_preserve_all_flags_and_other_members() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let initial = thread(&mut store);
    let category = call(&mut store, "categories", json!({}))["categories"][0]["id"].clone();
    let mut request = flag_request(&initial, "manual-category", &[0]);
    request["category_id"] = category;
    request["present"] = json!(true);
    let result = call(&mut store, "set_category", request);
    for (before, after) in initial["messages"]
        .as_array()
        .unwrap()
        .iter()
        .zip(result["messages"].as_array().unwrap())
    {
        assert_eq!(before["unread"], after["unread"]);
        assert_eq!(before["starred"], after["starred"]);
    }
    assert_eq!(initial["messages"][1], result["messages"][1]);
}

#[test]
fn account_view_is_durable_and_rejects_foreign_selection() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let initial = thread(&mut store);
    let view = json!({"selected":initial["id"],"query":"Frankfurt","readerY":123.5,"weekStart":"2026-09-07","screen":"settings"});
    let saved = call(
        &mut store,
        "view_save",
        json!({"expected_revision":0,"request_id":"save-view","view":view}),
    );
    assert_eq!(saved["view"], view);
    assert_eq!(
        store
            .handle("view_get", Some("personal"), json!({}))
            .unwrap()["revision"],
        0
    );
    drop(store);
    let mut store = Store::open(temp.path()).unwrap();
    assert_eq!(call(&mut store, "view_get", json!({})), saved);
    let bad = store.handle("view_save", Some("personal"), json!({"expected_revision":0,"request_id":"foreign-selection","view":{"selected":initial["id"]}})).unwrap_err();
    assert_eq!(bad.code, "not_found");
}

#[test]
fn invalid_mutations_reject_duplicates_unknown_fields_and_size_overflow() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let initial = thread(&mut store);
    let mut duplicate = flag_request(&initial, "duplicate", &[0, 0]);
    duplicate["read"] = json!(true);
    assert_eq!(
        store
            .handle("flag", Some("windscribe"), duplicate)
            .unwrap_err()
            .code,
        "invalid_request"
    );
    let mut malformed = flag_request(&initial, "bad-bool", &[0]);
    malformed["read"] = json!("true");
    assert_eq!(
        store
            .handle("flag", Some("windscribe"), malformed)
            .unwrap_err()
            .code,
        "invalid_request"
    );
    let mut unknown = flag_request(&initial, "unknown", &[0]);
    unknown["read"] = json!(true);
    unknown["done"] = json!(true);
    assert_eq!(
        store
            .handle("flag", Some("windscribe"), unknown)
            .unwrap_err()
            .code,
        "invalid_request"
    );
    let oversized = json!({"conversation_id":initial["id"],"request_id":"oversized","expected_revision":0,"body":"x".repeat(256*1024+1)});
    assert_eq!(
        store
            .handle("draft_save", Some("windscribe"), oversized)
            .unwrap_err()
            .code,
        "invalid_request"
    );
    let oversized = json!({"request_id":"oversized-view","expected_revision":0,"view":{"query":"x".repeat(16*1024)}});
    assert_eq!(
        store
            .handle("view_save", Some("windscribe"), oversized)
            .unwrap_err()
            .code,
        "invalid_request"
    );
    assert_eq!(
        call(&mut store, "conversation", json!({"id":initial["id"]})),
        initial
    );
}
