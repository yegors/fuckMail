use super::*;
use crate::live_profile::ConnectionState;
use std::collections::BTreeMap;

fn labels(items: &[(&str, &str, &str)]) -> PreparedGoogleLabels {
    let rows: Vec<_> = items
        .iter()
        .map(|(id, name, kind)| json!({"id":id,"name":name,"type":kind}))
        .collect();
    prepare_google_labels(serde_json::from_value(json!({"labels":rows})).unwrap()).unwrap()
}
fn setup(path: &std::path::Path) -> (Store, String) {
    let mut store = Store::open_live(path).unwrap();
    let account = store
        .bind_google_account(
            "label-owner",
            "reader@example.test",
            &BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]),
            None,
        )
        .unwrap();
    (store, account)
}
fn publish(store: &Store, account: &str, value: &PreparedGoogleLabels) -> Value {
    let revision = store.google_labels(account, &json!({})).unwrap()["catalog_revision"]
        .as_i64()
        .unwrap();
    let generation = store
        .google_account_bindings()
        .unwrap()
        .into_iter()
        .find(|v| v.account == account)
        .unwrap()
        .connection_generation;
    let mut lease = loop {
        if let Some(lease) = store
            .google_labels_begin(account, generation, revision, &value.manifest())
            .unwrap()
        {
            break lease;
        }
    };
    while let Some(chunk) = value.chunk(lease.offset).unwrap() {
        lease = store.google_labels_stage(&lease, chunk).unwrap();
    }
    store.google_labels_publish(&lease, 1_000).unwrap()
}
fn create(request: &str, revision: i64, category: &str, label: &str) -> Value {
    json!({"request_id":request,"expected_catalog_revision":revision,"category_id":category,"provider_label_id":label,"create_name":"My category"})
}

#[test]
fn bounded_catalog_parts_are_invisible_and_resume_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let (store, account) = setup(directory.path());
    publish(&store, &account, &labels(&[("Old", "Old name", "user")]));
    let rows:Vec<_>=(0..145).map(|index|json!({"id":format!("L{index:03}"),"name":format!("Label {index}"),"type":"user"})).collect();
    let prepared =
        prepare_google_labels(serde_json::from_value(json!({"labels":rows})).unwrap()).unwrap();
    let first = store
        .google_labels_begin(&account, 1, 1, &prepared.manifest())
        .unwrap()
        .unwrap();
    let next = store
        .google_labels_stage(&first, prepared.chunk(0).unwrap().unwrap())
        .unwrap();
    assert_eq!(next.offset, 64);
    assert_eq!(
        store.google_labels(&account, &json!({})).unwrap()["labels"][0]["id"],
        "Old"
    );
    assert_eq!(
        store.google_labels_publish(&next, 1_000).unwrap_err().code,
        "stale_google_work"
    );
    drop(store);
    let store = Store::open_live(directory.path()).unwrap();
    let resumed = store
        .google_labels_begin(&account, 1, 1, &prepared.manifest())
        .unwrap()
        .unwrap();
    assert_eq!(resumed.offset, 64);
    let middle = store
        .google_labels_stage(&resumed, prepared.chunk(64).unwrap().unwrap())
        .unwrap();
    let final_part = store
        .google_labels_stage(&middle, prepared.chunk(middle.offset).unwrap().unwrap())
        .unwrap();
    store.google_labels_publish(&final_part, 2_000).unwrap();
    let first = store
        .google_labels(&account, &json!({"limit":100}))
        .unwrap();
    assert_eq!(first["catalog_revision"], 2);
    assert_eq!(first["labels"].as_array().unwrap().len(), 100);
    let second = store
        .google_labels(
            &account,
            &json!({"limit":100,"after":first["next_after"],"expected_catalog_revision":2}),
        )
        .unwrap();
    assert_eq!(second["labels"].as_array().unwrap().len(), 45);
    assert!(second["next_after"].is_null());
    assert_eq!(
        store
            .google_labels(
                &account,
                &json!({"after":"L099","expected_catalog_revision":1})
            )
            .unwrap_err()
            .code,
        "stale_revision"
    );
    publish(&store, &account, &labels(&[]));
    let before: i64 = store
        .db
        .query_row("SELECT count(*) FROM google_label_catalog_items", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(store.google_labels_cleanup(&account).unwrap());
    let after: i64 = store
        .db
        .query_row("SELECT count(*) FROM google_label_catalog_items", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(before - after, 64);
    assert!(store.google_labels(&account, &json!({})).unwrap()["labels"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn explicit_binding_is_replayable_offline_and_provider_names_do_not_rename_categories() {
    let directory = tempfile::tempdir().unwrap();
    let (mut store, account) = setup(directory.path());
    publish(
        &store,
        &account,
        &labels(&[("A", "Provider original", "user"), ("B", "Other", "user")]),
    );
    let request = create("bind", 1, "mine", "A");
    let receipt = store.google_category_bind(&account, &request).unwrap();
    assert_eq!(receipt["category"]["name"], "My category");
    assert_eq!(receipt["local_only"], true);
    publish(
        &store,
        &account,
        &labels(&[("A", "Provider renamed", "user"), ("B", "Other", "user")]),
    );
    assert_eq!(
        store.categories(&account).unwrap()["categories"][0]["name"],
        "My category"
    );
    assert_eq!(
        store.google_labels(&account, &json!({})).unwrap()["labels"][0]["name"],
        "Provider renamed"
    );
    store
        .set_google_connection_state(&account, ConnectionState::Disconnected)
        .unwrap();
    assert_eq!(
        store.google_category_bind(&account, &request).unwrap(),
        receipt
    );
    let other = create("offline", 2, "other", "B");
    assert!(store.google_category_bind(&account, &other).is_ok());
    let cached = store.google_labels(&account, &json!({})).unwrap();
    assert_eq!(cached["connection_state"], "disconnected");
    assert_eq!(cached["stale_connection"], true);
    assert_eq!(cached["available"], true);
    let mut different = request.clone();
    different["create_name"] = "Different".into();
    assert_eq!(
        store
            .google_category_bind(&account, &different)
            .unwrap_err()
            .code,
        "request_id_conflict"
    );
    store
        .set_google_connection_state(&account, ConnectionState::Connected)
        .unwrap();
    publish(&store, &account, &labels(&[]));
    let missing = store.google_labels(&account, &json!({})).unwrap();
    assert_eq!(missing["labels"].as_array().unwrap().len(), 2);
    assert!(missing["labels"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["present"] == false && row["name"].is_null() && row["selectable"] == false));
    assert_eq!(
        store
            .google_category_bind(&account, &create("deleted", 3, "deleted", "A"))
            .unwrap_err()
            .code,
        "label_unavailable"
    );
    drop(store);
    let mut reopened = Store::open_live(directory.path()).unwrap();
    assert_eq!(
        reopened.google_category_bind(&account, &request).unwrap(),
        receipt
    );
}

#[test]
fn cached_categories_follow_binding_remapping_and_rolled_back_receipts() {
    let directory = tempfile::tempdir().unwrap();
    let (mut store, account) = setup(directory.path());
    publish(
        &store,
        &account,
        &labels(&[("A", "First", "user"), ("B", "Second", "user")]),
    );
    // A retained local category can exist before its first provider mapping.
    store
        .db
        .execute(
            "INSERT INTO categories(account,id,name) VALUES(?1,'mine','My category')",
            [&account],
        )
        .unwrap();
    let unmapped = store
        .handle("categories", Some(&account), json!({}))
        .unwrap();
    assert_eq!(
        unmapped["categories"][0],
        json!({"id":"mine","name":"My category","conversation_count":0,"provider_label_id":null})
    );
    for (request, previous, next) in [("bind", Value::Null, "A"), ("remap", json!("A"), "B")] {
        store
            .handle(
                "google_category_bind",
                Some(&account),
                json!({"request_id":request,"expected_catalog_revision":1,"category_id":"mine","provider_label_id":next,"expected_provider_label_id":previous}),
            )
            .unwrap();
        let current = store
            .handle("categories", Some(&account), json!({}))
            .unwrap();
        assert_eq!(
            current["categories"][0],
            json!({"id":"mine","name":"My category","conversation_count":0,"provider_label_id":next})
        );
        assert_eq!(
            store
                .handle("categories", Some(&account), json!({}))
                .unwrap(),
            current
        );
    }
    store
        .db
        .execute_batch("CREATE TRIGGER reject_mapping_receipt BEFORE INSERT ON operations WHEN NEW.request_id='rejected-remap' BEGIN SELECT RAISE(ABORT,'fixture receipt failure'); END")
        .unwrap();
    assert!(store
        .handle(
            "google_category_bind",
            Some(&account),
            json!({"request_id":"rejected-remap","expected_catalog_revision":1,"category_id":"mine","provider_label_id":"A","expected_provider_label_id":"B"}),
        )
        .is_err());
    let retained = store
        .handle("categories", Some(&account), json!({}))
        .unwrap();
    assert_eq!(retained["categories"][0]["provider_label_id"], "B");
    drop(store);
    let mut reopened = Store::open_live(directory.path()).unwrap();
    assert_eq!(
        reopened
            .handle("categories", Some(&account), json!({}))
            .unwrap(),
        retained
    );

    let synthetic_directory = tempfile::tempdir().unwrap();
    let mut synthetic = Store::open(synthetic_directory.path()).unwrap();
    let rows = synthetic
        .handle("categories", Some("personal"), json!({}))
        .unwrap();
    assert!(!rows["categories"].as_array().unwrap().is_empty());
    assert!(rows["categories"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row.get("provider_label_id").is_none()));
}

#[test]
fn remapping_preserves_existing_assignments_pending_intent_and_flags() {
    let directory = tempfile::tempdir().unwrap();
    let (mut store, account) = setup(directory.path());
    publish(
        &store,
        &account,
        &labels(&[("A", "First", "user"), ("B", "Second", "user")]),
    );
    store
        .google_category_bind(&account, &create("bind", 1, "mine", "A"))
        .unwrap();
    let staged = crate::google_store::StagedMessage {
        remote_id: "remote".into(),
        remote_thread_id: "thread".into(),
        history_id: "100".into(),
        labels: vec![
            "INBOX".into(),
            "UNREAD".into(),
            "STARRED".into(),
            "A".into(),
            "Unrelated".into(),
        ],
        sender_name: "Human".into(),
        sender_email: "human@example.test".into(),
        recipient: "reader@example.test".into(),
        subject: "Category fixture".into(),
        body: "Retained body".into(),
        sent_at: "2026-09-13T10:00:00Z".into(),
        attachments: vec![],
        pgp: 0,
    };
    crate::google_store::publish_message(&store.db, &account, &staged, "source").unwrap();
    let (message, conversation): (String, String) = store
        .db
        .query_row(
            "SELECT id,conversation_id FROM messages WHERE account=?1",
            [&account],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let revision = store.conversation(&account, &conversation).unwrap()["revision"]
        .as_i64()
        .unwrap();
    store.mutate("set_category",&account,&json!({"id":conversation,"message_ids":[message],"category_id":"mine","present":true,"expected_revision":revision,"request_id":"assign"})).unwrap();
    let before = store.conversation(&account, &conversation).unwrap();
    let intent:Vec<(String,String,i64)>=store.db.prepare("SELECT field,category_id,revision FROM google_manual_intents WHERE account=?1 ORDER BY field,category_id").unwrap().query_map([&account],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().collect::<std::result::Result<_,_>>().unwrap();
    store.google_category_bind(&account,&json!({"request_id":"remap","expected_catalog_revision":1,"category_id":"mine","provider_label_id":"B","expected_provider_label_id":"A"})).unwrap();
    assert_eq!(store.conversation(&account, &conversation).unwrap(), before);
    let after:Vec<(String,String,i64)>=store.db.prepare("SELECT field,category_id,revision FROM google_manual_intents WHERE account=?1 ORDER BY field,category_id").unwrap().query_map([&account],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().collect::<std::result::Result<_,_>>().unwrap();
    assert_eq!(intent, after);
    assert_eq!(store.google_category_bind(&account,&json!({"request_id":"old-remap","expected_catalog_revision":1,"category_id":"mine","provider_label_id":"A","expected_provider_label_id":"A"})).unwrap_err().code,"stale_revision");
    let work = store
        .google_mail_mutation_next(&account, 1, 0)
        .unwrap()
        .unwrap();
    store
        .google_mail_mutation_observe(
            &work,
            Some(crate::google_live_mutations::MailLabelObservation {
                remote_id: "remote".into(),
                remote_thread_id: "thread".into(),
                history_id: "101".into(),
                labels: staged.labels.into_iter().collect(),
            }),
        )
        .unwrap();
    assert_eq!(
        store
            .google_mail_mutation_request_status(&account, "assign")
            .unwrap()["counts"]["failed"],
        1
    );
}

#[test]
fn stale_connection_and_invalid_binding_never_publish_or_create_categories() {
    let directory = tempfile::tempdir().unwrap();
    let (mut store, account) = setup(directory.path());
    let value = labels(&[
        ("INBOX", "Inbox", "system"),
        ("Pretend", "Pretend system", "system"),
        ("A", "Valid", "user"),
    ]);
    publish(&store, &account, &value);
    let previous = store.google_labels(&account, &json!({})).unwrap();
    for label in ["INBOX", "Pretend", "foreign"] {
        assert!(store
            .google_category_bind(&account, &create(label, 1, label, label))
            .is_err());
    }
    assert!(store.categories(&account).unwrap()["categories"]
        .as_array()
        .unwrap()
        .is_empty());
    let other = store
        .bind_google_account(
            "other-label-owner",
            "other@example.test",
            &BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]),
            None,
        )
        .unwrap();
    publish(
        &store,
        &other,
        &labels(&[("Foreign", "Foreign label", "user")]),
    );
    assert_eq!(
        store
            .google_category_bind(
                &account,
                &create("foreign-account", 1, "foreign", "Foreign")
            )
            .unwrap_err()
            .code,
        "label_unavailable"
    );
    let lease = loop {
        if let Some(l) = store
            .google_labels_begin(&account, 1, 1, &value.manifest())
            .unwrap()
        {
            break l;
        }
    };
    store
        .set_google_connection_state(&account, ConnectionState::Disconnected)
        .unwrap();
    assert_eq!(
        store
            .google_labels_stage(&lease, value.chunk(0).unwrap().unwrap())
            .err()
            .unwrap()
            .code,
        "stale_google_work"
    );
    let current = store.google_labels(&account, &json!({})).unwrap();
    assert_eq!(current["labels"], previous["labels"]);
    assert_eq!(current["catalog_revision"], previous["catalog_revision"]);
}

#[test]
fn duplicate_or_oversized_catalogs_fail_before_store_publication() {
    let duplicate=serde_json::from_value(json!({"labels":[{"id":"same","name":"A","type":"user"},{"id":"same","name":"B","type":"user"}]})).unwrap();
    assert!(prepare_google_labels(duplicate).is_err());
    let rows: Vec<_> = (0..1024)
        .map(|i| json!({"id":format!("id{i}"),"name":"x".repeat(4096),"type":"user"}))
        .collect();
    assert!(
        prepare_google_labels(serde_json::from_value(json!({"labels":rows})).unwrap()).is_err()
    );
    let rows: Vec<_> = (0..65)
        .map(|i| json!({"id":format!("id{i:03}"),"name":"x".repeat(4096),"type":"user"}))
        .collect();
    let value =
        prepare_google_labels(serde_json::from_value(json!({"labels":rows})).unwrap()).unwrap();
    let chunk = value.chunk(0).unwrap().unwrap();
    assert!(chunk.labels.len() < 64);
    assert!(
        chunk
            .labels
            .iter()
            .map(|v| v.id.len() + v.name.len() + 32)
            .sum::<usize>()
            <= CHUNK_BYTES
    );
}

#[test]
fn linking_after_import_projects_saved_labels_without_inbox_or_remote_writes() {
    let directory = tempfile::tempdir().unwrap();
    let (mut store, account) = setup(directory.path());
    publish(
        &store,
        &account,
        &labels(&[
            ("A", "Imported folder", "user"),
            ("B", "Other folder", "user"),
        ]),
    );
    let staged = crate::google_store::StagedMessage {
        remote_id: "already-imported".into(),
        remote_thread_id: "outside-inbox".into(),
        history_id: "100".into(),
        labels: vec!["A".into(), "UNREAD".into(), "STARRED".into()],
        sender_name: "Human".into(),
        sender_email: "human@example.test".into(),
        recipient: "reader@example.test".into(),
        subject: "Previously hidden".into(),
        body: "Preserved source".into(),
        sent_at: "2026-09-14T10:00:00Z".into(),
        attachments: vec![],
        pgp: 0,
    };
    crate::google_store::publish_message(&store.db, &account, &staged, "source").unwrap();
    let (mid, cid, before_labels): (String,String,String) = store.db.query_row(
        "SELECT m.id,m.conversation_id,s.provider_labels_json FROM messages m JOIN google_message_state s ON s.account=m.account AND s.message_id=m.id WHERE m.account=?1", [&account], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    let before = store.conversation(&account, &cid).unwrap();
    let request = create("link-after-import", 1, "imported", "A");
    let receipt = store.google_category_bind(&account, &request).unwrap();
    let after = store.conversation(&account, &cid).unwrap();
    assert_eq!(after["categories"][0]["id"], "imported");
    assert_eq!(after["messages"][0]["text"], before["messages"][0]["text"]);
    assert_eq!(after["messages"][0]["unread"], true);
    assert_eq!(after["messages"][0]["starred"], true);
    assert_eq!(
        after["revision"].as_i64().unwrap(),
        before["revision"].as_i64().unwrap() + 1
    );
    let all = store
        .handle(
            "conversations",
            Some(&account),
            json!({"folder":"all","category":"imported"}),
        )
        .unwrap();
    assert_eq!(all["filtered_conversation_count"], 1);
    let inbox = store
        .handle(
            "conversations",
            Some(&account),
            json!({"folder":"inbox","category":"imported"}),
        )
        .unwrap();
    assert_eq!(inbox["filtered_conversation_count"], 0);
    assert_eq!(
        store.google_category_bind(&account, &request).unwrap(),
        receipt
    );
    assert_eq!(store.conversation(&account, &cid).unwrap(), after);
    assert_eq!(
        store
            .db
            .query_row("SELECT COUNT(*) FROM google_manual_intents", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(store.db.query_row("SELECT provider_labels_json FROM google_message_state WHERE account=?1 AND message_id=?2",params![account,mid],|r|r.get::<_,String>(0)).unwrap(),before_labels);

    // Simulate a pre-fix live store: linked labels never populated membership.
    store
        .db
        .execute(
            "DELETE FROM message_categories WHERE account=?1",
            [&account],
        )
        .unwrap();
    store
        .db
        .execute(
            "DELETE FROM meta WHERE key='google_category_projection_v1'",
            [],
        )
        .unwrap();
    drop(store);
    let recovered = Store::open_live(directory.path()).unwrap();
    // Opening schedules the repair; the engine's label worker applies it.
    assert!(recovered.google_category_projection_step(&account).unwrap());
    assert!(!recovered.google_category_projection_step(&account).unwrap());
    let repaired = recovered.conversation(&account, &cid).unwrap();
    assert_eq!(repaired["categories"][0]["id"], "imported");
    assert_eq!(repaired["messages"][0]["unread"], true);
    assert_eq!(repaired["messages"][0]["starred"], true);
    assert_eq!(
        recovered
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        crate::SCHEMA_VERSION as i64
    );
    drop(recovered);
    let reopened = Store::open_live(directory.path()).unwrap();
    assert!(
        !reopened.google_category_projection_step(&account).unwrap(),
        "repair is scheduled once"
    );
    assert_eq!(
        reopened.conversation(&account, &cid).unwrap(),
        repaired,
        "repair is restart-idempotent"
    );
}

#[test]
fn saved_manual_category_choice_wins_during_projection_repair() {
    let directory = tempfile::tempdir().unwrap();
    let (mut store, account) = setup(directory.path());
    publish(
        &store,
        &account,
        &labels(&[("A", "Imported folder", "user")]),
    );
    store
        .google_category_bind(&account, &create("bind", 1, "mine", "A"))
        .unwrap();
    let staged = crate::google_store::StagedMessage {
        remote_id: "manual-source".into(),
        remote_thread_id: "manual-thread".into(),
        history_id: "100".into(),
        labels: vec!["A".into()],
        sender_name: "Human".into(),
        sender_email: "human@example.test".into(),
        recipient: "reader@example.test".into(),
        subject: "Manual choice".into(),
        body: "Retained".into(),
        sent_at: "2026-09-14T10:00:00Z".into(),
        attachments: vec![],
        pgp: 0,
    };
    crate::google_store::publish_message(&store.db, &account, &staged, "source").unwrap();
    let (mid, cid): (String, String) = store
        .db
        .query_row(
            "SELECT id,conversation_id FROM messages WHERE account=?1",
            [&account],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let before = store.conversation(&account, &cid).unwrap();
    store.handle("set_category",Some(&account),json!({"id":cid,"message_ids":[mid],"expected_revision":before["revision"],"request_id":"manual-clear","category_id":"mine","present":false})).unwrap();
    let saved = store.conversation(&account, &cid).unwrap();
    store
        .db
        .execute(
            "DELETE FROM meta WHERE key='google_category_projection_v1'",
            [],
        )
        .unwrap();
    drop(store);
    let recovered = Store::open_live(directory.path()).unwrap();
    while recovered.google_category_projection_step(&account).unwrap() {}
    assert_eq!(recovered.conversation(&account, &cid).unwrap(), saved);
    assert!(!recovered.db.query_row("SELECT value FROM google_manual_intents WHERE account=?1 AND message_id=?2 AND field='category' AND category_id='mine'",params![account,mid],|r|r.get::<_,bool>(0)).unwrap());
}

/// Publishes Gmail messages `from..to`, three to a conversation. Message `i`
/// carries label `A` when `i % 3 == 0` and `B` when `i % 5 == 0`, unless
/// `unlabelled`.
fn labelled_mail(store: &Store, account: &str, from: usize, to: usize, unlabelled: bool) {
    let tx = store.db.unchecked_transaction().unwrap();
    for index in from..to {
        let mut labels = vec!["INBOX".to_owned()];
        if index % 3 == 0 && !unlabelled {
            labels.push("A".into());
        }
        if index % 5 == 0 && !unlabelled {
            labels.push("B".into());
        }
        let staged = crate::google_store::StagedMessage {
            remote_id: format!("remote-{index:05}"),
            remote_thread_id: format!("thread-{:05}", index / 3),
            history_id: "100".into(),
            labels,
            sender_name: "Human".into(),
            sender_email: "human@example.test".into(),
            recipient: "reader@example.test".into(),
            subject: format!("Message {index}"),
            body: "Body".into(),
            sent_at: "2026-09-14T10:00:00Z".into(),
            attachments: vec![],
            pgp: 0,
        };
        crate::google_store::publish_message(&tx, account, &staged, "source").unwrap();
    }
    tx.commit().unwrap();
}
/// Pending projections as `(key, cursor)`.
fn projection_cursors(store: &Store) -> BTreeMap<String, i64> {
    store
        .db
        .prepare(
            "SELECT key,value FROM meta WHERE key LIKE 'google_category_projection'||char(10)||'%'",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap()
}
/// Rows of `google_message_state` the parts between two cursor snapshots
/// read: a missing cursor before is a projection just begun, a missing one
/// after a finished projection.
fn rows_read(store: &Store, before: &BTreeMap<String, i64>, after: &BTreeMap<String, i64>) -> i64 {
    let keys: BTreeSet<_> = before.keys().chain(after.keys()).collect();
    keys.into_iter()
        .map(|key| {
            let start = before.get(key).copied().unwrap_or(0);
            let end = after.get(key).copied().unwrap_or(i64::MAX);
            store
                .db
                .query_row(
                    "SELECT count(*) FROM google_message_state WHERE rowid>?1 AND rowid<=?2",
                    params![start, end],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap()
        })
        .sum()
}
fn memberships(store: &Store, account: &str) -> BTreeSet<(String, String)> {
    store
        .db
        .prepare("SELECT message_id,category_id FROM message_categories WHERE account=?1")
        .unwrap()
        .query_map([account], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap()
}
/// What the memberships of every linked category must be: exactly the
/// messages carrying its label.
fn labelled_memberships(store: &Store, account: &str) -> BTreeSet<(String, String)> {
    store
        .db
        .prepare("SELECT s.message_id,g.category_id FROM google_message_state s JOIN google_category_labels g ON g.account=s.account WHERE s.account=?1 AND EXISTS(SELECT 1 FROM json_each(s.provider_labels_json) j WHERE j.value=g.provider_label_id)")
        .unwrap()
        .query_map([account], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap()
}
fn conversation_revisions(store: &Store, account: &str) -> BTreeMap<String, i64> {
    store
        .db
        .prepare("SELECT id,revision FROM conversations WHERE account=?1")
        .unwrap()
        .query_map([account], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap()
}
fn linked_count(store: &Store, account: &str) -> i64 {
    store
        .db
        .query_row(
            "SELECT count(*) FROM google_category_labels WHERE account=?1",
            [account],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn merging_new_labels_links_one_per_call_and_projects_in_bounded_parts() {
    let directory = tempfile::tempdir().unwrap();
    let (mut store, account) = setup(directory.path());
    publish(
        &store,
        &account,
        &labels(&[("A", "Alpha", "user"), ("B", "Beta", "user")]),
    );
    labelled_mail(&store, &account, 0, 1_101, false);
    // Two more conversations without either label.
    labelled_mail(&store, &account, 1_101, 1_107, true);
    let revisions = conversation_revisions(&store, &account);
    let plan = store
        .db
        .prepare(&format!("EXPLAIN QUERY PLAN {PROJECTION_CHANGES}"))
        .unwrap()
        .query_map(params![account, "c", 0, 10, "A"], |r| r.get::<_, String>(3))
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert!(
        plan.iter()
            .any(|step| step.contains("SEARCH s USING INTEGER PRIMARY KEY (rowid>? AND rowid<?)")),
        "a part reads only its rowid range: {plan:?}"
    );

    let first = store.google_labels_merge(&account, "sync").unwrap();
    assert_eq!(first["more"], true);
    assert_eq!(first["pulled"], 1);
    assert_eq!(linked_count(&store, &account), 1, "one label per call");
    let mut cursors = projection_cursors(&store);
    assert_eq!(
        rows_read(&store, &BTreeMap::new(), &cursors),
        PROJECTION_ROWS
    );
    let mut calls = 1;
    let mut pulled = 1;
    let last = loop {
        let linked = linked_count(&store, &account);
        let before = memberships(&store, &account);
        let part = store.google_labels_merge(&account, "sync").unwrap();
        calls += 1;
        pulled += part["pulled"].as_u64().unwrap();
        let after = projection_cursors(&store);
        assert!(rows_read(&store, &cursors, &after) <= PROJECTION_ROWS);
        assert!(
            before
                .symmetric_difference(&memberships(&store, &account))
                .count()
                <= PROJECTION_ROWS as usize
        );
        assert!(linked_count(&store, &account) <= linked + 1);
        cursors = after;
        if part["more"] != true {
            break part;
        }
    };
    let parts = (1_107 + PROJECTION_ROWS - 1) / PROJECTION_ROWS;
    assert!(
        calls > 2 * parts,
        "{calls} calls for {parts} parts per label"
    );
    assert_eq!(pulled, 2);
    assert_eq!(last["pending"], json!([]));
    assert_eq!(last["skipped"], 0);
    assert!(cursors.is_empty());
    assert_eq!(linked_count(&store, &account), 2);
    let expected = labelled_memberships(&store, &account);
    assert_eq!(expected.len(), 367 + 221);
    assert_eq!(memberships(&store, &account), expected);
    let revised = conversation_revisions(&store, &account);
    let unchanged: Vec<_> = revised
        .iter()
        .filter(|(id, revision)| **revision == revisions[*id])
        .map(|(id, _)| id.clone())
        .collect();
    let unlabelled: Vec<_> = revisions
        .keys()
        .filter(|id| {
            !expected.iter().any(|(message, _)| {
                store
                    .db
                    .query_row(
                        "SELECT conversation_id=?2 FROM messages WHERE account=?1 AND id=?3",
                        params![account, id, message],
                        |r| r.get::<_, bool>(0),
                    )
                    .unwrap()
            })
        })
        .cloned()
        .collect();
    assert_eq!(unlabelled.len(), 2);
    assert_eq!(
        unchanged, unlabelled,
        "only conversations whose labels changed are revised"
    );
    // The import a new account was waiting for is complete.
    assert!(store
        .google_labels_initial_import(&account)
        .unwrap()
        .is_none());
    assert_eq!(
        store.google_labels_merge(&account, "sync").unwrap()["more"],
        false
    );
    assert_eq!(memberships(&store, &account), expected);
}

#[test]
fn a_projection_left_unfinished_by_a_closed_store_finishes_after_reopening() {
    let directory = tempfile::tempdir().unwrap();
    let (mut store, account) = setup(directory.path());
    publish(
        &store,
        &account,
        &labels(&[("A", "Alpha", "user"), ("B", "Beta", "user")]),
    );
    labelled_mail(&store, &account, 0, 700, false);
    for _ in 0..2 {
        assert_eq!(
            store.google_labels_merge(&account, "sync").unwrap()["more"],
            true
        );
    }
    let cursors = projection_cursors(&store);
    assert_eq!(cursors.len(), 1);
    assert_eq!(
        rows_read(&store, &BTreeMap::new(), &cursors),
        2 * PROJECTION_ROWS
    );
    let partial = memberships(&store, &account);
    assert!(!partial.is_empty());
    drop(store);

    let mut reopened = Store::open_live(directory.path()).unwrap();
    assert_eq!(
        projection_cursors(&reopened),
        cursors,
        "opening reads no mail"
    );
    assert_eq!(memberships(&reopened, &account), partial);
    let mut steps = 0;
    while reopened.google_category_projection_step(&account).unwrap() {
        steps += 1;
    }
    assert_eq!(steps, 1);
    assert!(projection_cursors(&reopened).is_empty());
    let expected = labelled_memberships(&reopened, &account);
    assert_eq!(expected.len(), 234, "only label A is linked so far");
    assert_eq!(memberships(&reopened, &account), expected);
    while reopened.google_labels_merge(&account, "sync").unwrap()["more"] == true {}
    assert_eq!(linked_count(&reopened, &account), 2);
    assert_eq!(
        memberships(&reopened, &account),
        labelled_memberships(&reopened, &account)
    );
}

#[test]
fn relinking_or_removing_a_category_mid_projection_settles_on_its_current_label() {
    let directory = tempfile::tempdir().unwrap();
    let (mut store, account) = setup(directory.path());
    publish(
        &store,
        &account,
        &labels(&[("A", "Alpha", "user"), ("B", "Beta", "user")]),
    );
    labelled_mail(&store, &account, 0, 700, false);
    store
        .google_category_bind(&account, &create("bind", 1, "mine", "A"))
        .unwrap();
    assert!(store.google_category_projection_step(&account).unwrap());
    // Relinking starts over, so rows already projected for A are corrected.
    store.google_category_bind(&account,&json!({"request_id":"remap","expected_catalog_revision":1,"category_id":"mine","provider_label_id":"B","expected_provider_label_id":"A"})).unwrap();
    while store.google_category_projection_step(&account).unwrap() {}
    let expected = labelled_memberships(&store, &account);
    assert_eq!(expected.len(), 140);
    assert_eq!(memberships(&store, &account), expected);

    let mut other = create("other", 1, "other", "A");
    other["create_name"] = json!("Other category");
    store.google_category_bind(&account, &other).unwrap();
    assert_eq!(projection_cursors(&store).len(), 1);
    store.label_edit(&account,"label_remove",&json!({"id":"other","expected_name":"Other category","expected_provider_label_id":"A","request_id":"remove-other"})).unwrap();
    assert!(store.google_category_projection_step(&account).unwrap());
    assert!(projection_cursors(&store).is_empty());
    assert!(!store.google_category_projection_step(&account).unwrap());
    assert_eq!(memberships(&store, &account), expected);
}
