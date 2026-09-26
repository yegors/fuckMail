use fm_core::Store;
use serde_json::{json, Value};
use std::collections::HashSet;

fn search(store: &mut Store, account: &str, query: &str, field: &str) -> Value {
    store
        .handle(
            "conversations",
            Some(account),
            json!({"query":query,"field":field,"folder":"all","limit":100}),
        )
        .unwrap()
}

#[test]
fn punctuation_partial_sender_and_source_fields_are_scoped() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let sender = search(
        &mut store,
        "windscribe",
        "from:maya@windscribe.example",
        "sender",
    );
    assert_eq!(sender["conversations"].as_array().unwrap().len(), 2);
    assert_eq!(sender["conversations"][0]["id"], "windscribe-thread-000");
    assert_eq!(sender["conversations"][1]["id"], "windscribe-thread-004");
    // A bare address is the sender or a recipient: those two threads and the mail to her.
    let either = search(
        &mut store,
        "windscribe",
        "maya@windscribe.example",
        "sender",
    );
    let either: HashSet<_> = either["conversations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_owned())
        .collect();
    assert!(
        either.len() > 2
            && either.contains("windscribe-thread-000")
            && either.contains("windscribe-thread-004")
    );
    assert_eq!(
        search(
            &mut store,
            "personal",
            "from:maya@windscribe.example",
            "sender"
        )["conversations"],
        json!([])
    );
    assert_eq!(
        search(&mut store, "windscribe", "may linq", "sender")["conversations"],
        json!([])
    );
    // Words typed together are one phrase; only its last word may be partial.
    assert_eq!(
        search(&mut store, "windscribe", "maya lind", "sender")["conversations"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        search(&mut store, "windscribe", "may lind", "sender")["conversations"],
        json!([])
    );
    assert_eq!(
        search(&mut store, "windscribe", "Frankfurt", "subject")["conversations"],
        json!([])
    );
    assert_eq!(
        search(&mut store, "windscribe", "Frankfurt", "body")["conversations"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let body = search(&mut store, "windscribe", "bare-metal", "body");
    assert_eq!(body["conversations"].as_array().unwrap().len(), 1);
    assert_eq!(body["conversations"][0]["id"], "windscribe-thread-000");
    assert!(body["conversations"][0]["snippet"]
        .as_str()
        .unwrap()
        .contains("bare-metal"));
}

#[test]
fn long_thread_search_groups_once_and_returns_actual_matching_member() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let result = search(&mut store, "windscribe", "longthread17", "body");
    let rows = result["conversations"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "windscribe-thread-004");
    assert_eq!(rows[0]["message_count"], 32);
    assert_eq!(
        rows[0]["matching_message_ids"],
        json!(["windscribe-message-004-017"])
    );
    assert_eq!(rows[0]["message_ids"].as_array().unwrap().len(), 32);
    assert!(rows[0]["snippet"]
        .as_str()
        .unwrap()
        .contains("longthread17"));
    let filtered = store
        .handle(
            "conversations",
            Some("windscribe"),
            json!({"query":"longthread17","category":"finance","folder":"all"}),
        )
        .unwrap();
    assert_eq!(filtered["conversations"], json!([]));
}

#[test]
fn pagination_is_stable_distinct_and_does_not_change_account_totals() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let before = store.handle("accounts", None, json!({})).unwrap();
    let mut offset = 0;
    let mut ids = Vec::new();
    loop {
        let request = json!({"folder":"all","limit":17,"offset":offset});
        let page = store
            .handle("conversations", Some("windscribe"), request.clone())
            .unwrap();
        assert_eq!(
            page,
            store
                .handle("conversations", Some("windscribe"), request)
                .unwrap()
        );
        for row in page["conversations"].as_array().unwrap() {
            ids.push(row["id"].as_str().unwrap().to_owned());
        }
        match page["next_offset"].as_i64() {
            Some(next) => offset = next,
            None => break,
        }
    }
    assert_eq!(ids.len(), 110);
    assert_eq!(ids.iter().collect::<HashSet<_>>().len(), 110);
    assert_eq!(ids[0], "windscribe-thread-000");
    assert_eq!(store.handle("accounts", None, json!({})).unwrap(), before);
}

#[test]
fn unsent_drafts_never_replace_or_enter_source_search() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    store.handle("draft_save",Some("windscribe"),json!({"conversation_id":"windscribe-thread-000","body":"Unpublisheddraftuniquetoken","request_id":"draft-source-separation","expected_revision":0})).unwrap();
    assert_eq!(
        search(
            &mut store,
            "windscribe",
            "Unpublisheddraftuniquetoken",
            "all"
        )["conversations"],
        json!([])
    );
    assert!(
        !search(&mut store, "windscribe", "Frankfurt", "body")["conversations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn query_bounds_and_unknown_fields_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    for request in [
        json!({"limit":101}),
        json!({"offset":-1}),
        json!({"field":"draft"}),
        json!({"query":true}),
        json!({"query":"***"}),
        json!({"query":"x".repeat(513)}),
        json!({"raw_sql":"SELECT"}),
    ] {
        assert_eq!(
            store
                .handle("conversations", Some("windscribe"), request)
                .unwrap_err()
                .code,
            "invalid_request"
        );
    }
}
