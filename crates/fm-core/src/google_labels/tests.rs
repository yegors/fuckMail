use super::*;
use crate::live_profile::ConnectionState;

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
