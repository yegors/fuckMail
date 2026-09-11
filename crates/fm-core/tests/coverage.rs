use fm_core::Store;
use rusqlite::{params, Connection};
use serde_json::{json, Value};

fn coverage(store: &mut Store, account: &str) -> Value {
    store
        .handle(
            "conversations",
            Some(account),
            json!({"limit":1,"folder":"all"}),
        )
        .unwrap()["coverage"]
        .clone()
}

#[test]
fn coverage_counts_real_index_rows_and_marks_file_verification_unknown() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let before = coverage(&mut store, "windscribe");
    let other = coverage(&mut store, "personal");
    assert_eq!(before["messages"], before["indexed_messages"]);
    assert_eq!(before["text_index_complete"], true);
    assert_eq!(before["retained_source_records"], before["messages"]);
    assert!(before["retained_sources"].is_null());
    assert_eq!(before["source_verification"], "not_run");
    assert_eq!(before["recorded_available_attachments"], 1);
    assert!(before["complete_attachments"].is_null());
    assert!(before["verified_attachments"].is_null());
    assert_eq!(before["attachment_verification"], "not_run");
    assert_eq!(before["offline_complete"], false);
    drop(store);

    // Controlled corruption of this disposable fixture, while its Store is closed.
    let db = Connection::open(temp.path().join("store.sqlite3")).unwrap();
    let (rowid, hash): (i64, String) = db
        .query_row(
            "SELECT rowid,source_hash FROM messages WHERE account=?1 AND id=?2",
            params!["windscribe", "windscribe-message-004-017"],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    db.execute("DELETE FROM message_fts WHERE rowid=?1", [rowid])
        .unwrap();
    drop(db);
    std::fs::remove_file(temp.path().join("blobs").join(hash)).unwrap();

    let mut store = Store::open(temp.path()).unwrap();
    let after = coverage(&mut store, "windscribe");
    assert_eq!(after["messages"], before["messages"]);
    assert_eq!(
        after["indexed_messages"].as_i64().unwrap(),
        before["indexed_messages"].as_i64().unwrap() - 1
    );
    assert_eq!(after["text_index_complete"], false);
    assert_eq!(
        after["retained_source_records"],
        before["retained_source_records"]
    );
    assert!(after["retained_sources"].is_null());
    assert_eq!(after["offline_complete"], false);
    assert_eq!(coverage(&mut store, "personal"), other);
    let results = store
        .handle(
            "conversations",
            Some("windscribe"),
            json!({"query":"longthread17","folder":"all"}),
        )
        .unwrap();
    assert_eq!(results["conversations"], json!([]));
    assert_eq!(results["coverage"], after);
    let detail = store
        .handle(
            "conversation",
            Some("windscribe"),
            json!({"id":"windscribe-thread-004"}),
        )
        .unwrap();
    let message = detail["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == "windscribe-message-004-017")
        .unwrap();
    assert_eq!(message["source_available"], false);
    assert!(message["text"].as_str().unwrap().contains("longthread17"));
}

#[test]
fn recorded_available_attachment_never_implies_a_verified_offline_copy() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    let attachment = store
        .handle(
            "attachment",
            Some("windscribe"),
            json!({"id":"windscribe-attachment-0"}),
        )
        .unwrap();
    let path = attachment["path"].as_str().unwrap().to_owned();
    let before = coverage(&mut store, "windscribe");
    std::fs::write(path, b"corrupted synthetic attachment").unwrap();
    let after = coverage(&mut store, "windscribe");
    assert_eq!(after, before); // Metadata is still metadata, explicitly unverified.
    assert!(after["complete_attachments"].is_null());
    assert_eq!(after["offline_complete"], false);
    assert_eq!(
        store
            .handle(
                "attachment",
                Some("windscribe"),
                json!({"id":"windscribe-attachment-0"})
            )
            .unwrap_err()
            .code,
        "content_unavailable"
    );
}
