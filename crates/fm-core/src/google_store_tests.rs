use super::*;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use fm_google::sync::{
    advance, prepare_full_page, prepare_history_page, GmailTransport, HistoryPage, ListPage,
    MessageRef, Phase, RawMessage,
};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};

#[derive(Default)]
struct Fake {
    lists: VecDeque<std::result::Result<ListPage, GoogleError>>,
    histories: VecDeque<std::result::Result<HistoryPage, GoogleError>>,
    messages: BTreeMap<String, RawMessage>,
    fail_get: Option<String>,
    list_tokens: Vec<Option<String>>,
}
impl GmailTransport for Fake {
    fn list(
        &mut self,
        _: &AccountId,
        token: Option<&str>,
        include_spam_trash: bool,
    ) -> std::result::Result<ListPage, GoogleError> {
        assert!(include_spam_trash);
        self.list_tokens.push(token.map(str::to_owned));
        self.lists.pop_front().expect("unexpected list")
    }
    fn history(
        &mut self,
        _: &AccountId,
        _: &str,
        _: Option<&str>,
    ) -> std::result::Result<HistoryPage, GoogleError> {
        self.histories.pop_front().expect("unexpected history")
    }
    fn get_raw(&mut self, _: &AccountId, id: &str) -> std::result::Result<RawMessage, GoogleError> {
        if self.fail_get.as_deref() == Some(id) {
            return Err(GoogleError::TransportInterrupted);
        }
        self.messages.get(id).cloned().ok_or(GoogleError::NotFound)
    }
}
fn source(word: &str) -> Vec<u8> {
    format!("From: Synthetic Author <author@example.test>\r\nTo: Alex <you@example.test>\r\nSubject: Provider bridge {word}\r\nDate: Fri, 11 Sep 2026 15:00:00 +0000\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{word} source body, Café.\r\n").into_bytes()
}
fn raw(id: &str, word: &str, labels: &[&str]) -> RawMessage {
    RawMessage {
        id: id.into(),
        thread_id: format!("thread-{id}"),
        label_ids: labels.iter().map(|s| (*s).into()).collect(),
        history_id: "110".into(),
        raw: URL_SAFE_NO_PAD.encode(source(word)),
    }
}
fn content(message: RawMessage) -> MessageContent {
    let id = message.id.clone();
    fm_google::sync::decode_raw_message(message, &id).unwrap()
}
fn list(ids: &[&str], next: Option<&str>) -> ListPage {
    ListPage {
        messages: ids
            .iter()
            .map(|id| MessageRef {
                id: (*id).into(),
                thread_id: format!("thread-{id}"),
            })
            .collect(),
        next_page_token: next.map(str::to_owned),
        result_size_estimate: Some(999),
    }
}
fn history(ids: &[&str], deleted: &[&str], cursor: &str) -> HistoryPage {
    serde_json::from_value(json!({"historyId":cursor,"history":[{"id":cursor,
        "messagesAdded":ids.iter().map(|id|json!({"message":{"id":id,"threadId":format!("thread-{id}")}})).collect::<Vec<_>>(),
        "messagesDeleted":deleted.iter().map(|id|json!({"message":{"id":id,"threadId":format!("thread-{id}")}})).collect::<Vec<_>>() }]})).unwrap()
}
fn search(store: &mut Store, account: &str, word: &str) -> Vec<Value> {
    store
        .handle(
            "conversations",
            Some(account),
            json!({"folder":"all","query":word,"limit":50}),
        )
        .unwrap()["conversations"]
        .as_array()
        .unwrap()
        .clone()
}
fn detail(store: &mut Store, account: &str, remote: &str) -> Value {
    let cid: String = store
        .db
        .query_row(
            "SELECT conversation_id FROM messages WHERE account=?1 AND remote_id=?2",
            params![account, remote],
            |row| row.get(0),
        )
        .unwrap();
    store
        .handle("conversation", Some(account), json!({"id":cid}))
        .unwrap()
}
fn count(store: &Store, table: &str) -> i64 {
    store
        .db
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}
fn commit(store: &mut Store, checkpoint: &mut Checkpoint, plan: PagePlan) {
    GooglePageStore::new(store).commit_page(&plan).unwrap();
    *checkpoint = plan.next;
}

#[test]
fn two_page_import_catchup_reopen_replay_and_colliding_accounts() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let initial = count(&store, "messages");
    let mut cp = store.google_begin_full("windscribe", "100").unwrap();
    let mut fake = Fake::default();
    fake.lists
        .extend([Ok(list(&["a"], Some("page2"))), Ok(list(&["a", "b"], None))]);
    fake.histories.push_back(Ok(history(&["c"], &[], "130")));
    for id in ["a", "b", "c"] {
        fake.messages.insert(
            id.into(),
            raw(id, &format!("bridgeword{id}"), &["INBOX", "UNREAD"]),
        );
    }
    let first = advance(&mut cp, &mut fake, &mut GooglePageStore::new(&mut store)).unwrap();
    assert_eq!(count(&store, "messages"), initial + 1);
    assert_eq!(search(&mut store, "windscribe", "bridgeworda").len(), 1);
    assert!(search(&mut store, "personal", "bridgeworda").is_empty());
    assert_eq!(cp.committed_history_id, None);
    drop(store);
    let mut store = Store::open(tmp.path()).unwrap();
    assert_eq!(
        store.google_checkpoint("windscribe").unwrap(),
        Some(cp.clone())
    );
    GooglePageStore::new(&mut store)
        .commit_page(&first)
        .unwrap();
    advance(&mut cp, &mut fake, &mut GooglePageStore::new(&mut store)).unwrap();
    assert_eq!(cp.committed_history_id, None);
    advance(&mut cp, &mut fake, &mut GooglePageStore::new(&mut store)).unwrap();
    assert_eq!(cp.phase, Phase::Idle);
    assert_eq!(cp.committed_history_id.as_deref(), Some("130"));
    assert_eq!(count(&store, "messages"), initial + 3);
    assert_eq!(fake.list_tokens, vec![None, Some("page2".into())]);
    assert_eq!(search(&mut store, "windscribe", "bridgewordc").len(), 1);
    GooglePageStore::new(&mut store)
        .commit_page(&first)
        .unwrap();
    let mut conflicting = first.clone();
    conflicting.caught_up = true;
    assert!(matches!(
        GooglePageStore::new(&mut store).commit_page(&conflicting),
        Err(GoogleError::StaleCheckpoint)
    ));
    let personal = store.google_begin_full("personal", "100").unwrap();
    GooglePageStore::new(&mut store)
        .stage_message(&personal, content(raw("a", "personalbridge", &["INBOX"])))
        .unwrap();
    GooglePageStore::new(&mut store)
        .commit_page(&prepare_full_page(&personal, list(&["a"], None)).unwrap())
        .unwrap();
    let work = detail(&mut store, "windscribe", "a");
    let home = detail(&mut store, "personal", "a");
    assert_ne!(work["id"], home["id"]);
    assert_ne!(work["messages"][0]["id"], home["messages"][0]["id"]);
    assert!(search(&mut store, "windscribe", "personalbridge").is_empty());
    assert_eq!(
        store
            .handle("conversation", Some("personal"), json!({"id":work["id"]}))
            .unwrap_err()
            .code,
        "not_found"
    );
}

#[test]
fn partial_transport_staging_is_invisible_and_retry_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let mut cp = store.google_begin_full("windscribe", "100").unwrap();
    let original = cp.clone();
    let initial = count(&store, "messages");
    let mut fake = Fake::default();
    fake.lists.push_back(Ok(list(&["a", "b"], None)));
    fake.messages
        .insert("a".into(), raw("a", "partialalpha", &["INBOX"]));
    fake.messages
        .insert("b".into(), raw("b", "partialbravo", &["INBOX"]));
    fake.fail_get = Some("b".into());
    assert!(matches!(
        advance(&mut cp, &mut fake, &mut GooglePageStore::new(&mut store)),
        Err(GoogleError::TransportInterrupted)
    ));
    assert_eq!(cp, original);
    assert_eq!(
        store.google_checkpoint("windscribe").unwrap(),
        Some(original.clone())
    );
    assert_eq!(count(&store, "messages"), initial);
    assert_eq!(count(&store, "google_staged_messages"), 1);
    assert!(search(&mut store, "windscribe", "partialalpha").is_empty());
    drop(store);
    let mut store = Store::open(tmp.path()).unwrap();
    assert_eq!(count(&store, "google_staged_messages"), 1);
    fake.fail_get = None;
    fake.lists.push_back(Ok(list(&["a", "b"], None)));
    advance(&mut cp, &mut fake, &mut GooglePageStore::new(&mut store)).unwrap();
    assert_eq!(count(&store, "messages"), initial + 2);
    assert_eq!(search(&mut store, "windscribe", "partialalpha").len(), 1);
    assert_eq!(search(&mut store, "windscribe", "partialbravo").len(), 1);
}

#[test]
fn sql_failure_rolls_back_messages_fts_checkpoint_and_commit_journal() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let cp = store.google_begin_full("windscribe", "100").unwrap();
    let initial = count(&store, "messages");
    let initial_fts = count(&store, "message_fts");
    let plan = prepare_full_page(&cp, list(&["a", "b"], None)).unwrap();
    for (id, word) in [("a", "rollbackalpha"), ("b", "rollbackbravo")] {
        GooglePageStore::new(&mut store)
            .stage_message(&cp, content(raw(id, word, &["INBOX"])))
            .unwrap();
    }
    store.db.execute_batch("CREATE TEMP TRIGGER simulate_second_row_failure BEFORE INSERT ON messages WHEN NEW.remote_id='b' BEGIN SELECT RAISE(ABORT,'controlled insert failure'); END;").unwrap();
    assert!(matches!(
        GooglePageStore::new(&mut store).commit_page(&plan),
        Err(GoogleError::StoreUnavailable)
    ));
    assert_eq!(count(&store, "messages"), initial);
    assert_eq!(count(&store, "message_fts"), initial_fts);
    assert_eq!(count(&store, "google_page_commits"), 0);
    assert_eq!(count(&store, "google_message_state"), 0);
    assert_eq!(
        store.google_checkpoint("windscribe").unwrap(),
        Some(cp.clone())
    );
    assert!(search(&mut store, "windscribe", "rollbackalpha").is_empty());
    drop(store);
    let mut store = Store::open(tmp.path()).unwrap();
    GooglePageStore::new(&mut store).commit_page(&plan).unwrap();
    assert_eq!(count(&store, "messages"), initial + 2);
    assert_eq!(count(&store, "message_fts"), initial_fts + 2);
    assert_eq!(count(&store, "google_page_commits"), 1);
    assert_eq!(
        store.google_checkpoint("windscribe").unwrap(),
        Some(plan.next)
    );
}

#[test]
fn manual_intents_and_draft_survive_newer_provider_snapshot_and_explicit_delete() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    store
        .google_map_category("windscribe", "finance", "Label_Finance")
        .unwrap();
    let mut cp = store.google_begin_full("windscribe", "100").unwrap();
    GooglePageStore::new(&mut store)
        .stage_message(
            &cp,
            content(raw(
                "a",
                "manualoverlay",
                &["INBOX", "UNREAD", "Label_Finance"],
            )),
        )
        .unwrap();
    let plan = prepare_full_page(&cp, list(&["a"], None)).unwrap();
    commit(&mut store, &mut cp, plan);
    let initial = detail(&mut store, "windscribe", "a");
    // A provider response is staged before the human edits. Commit must consult
    // the latest intent inside its own transaction, not a staging-time snapshot.
    GooglePageStore::new(&mut store)
        .stage_message(
            &cp,
            content(raw(
                "a",
                "manualoverlay",
                &["INBOX", "UNREAD", "Label_Finance"],
            )),
        )
        .unwrap();
    let changed=store.handle("flag",Some("windscribe"),json!({"id":initial["id"],"expected_revision":initial["revision"],"request_id":"user-read-star","message_ids":[initial["messages"][0]["id"]],"read":true,"starred":true})).unwrap();
    store.handle("set_category",Some("windscribe"),json!({"id":changed["id"],"expected_revision":changed["revision"],"request_id":"user-remove-category","message_ids":[changed["messages"][0]["id"]],"category_id":"finance","present":false})).unwrap();
    let draft=store.handle("draft_save",Some("windscribe"),json!({"conversation_id":initial["id"],"expected_revision":0,"request_id":"user-authored-draft","body":"I wrote this draft myself, draftnotsearchable."})).unwrap();
    let plan = prepare_history_page(&cp, history(&["a"], &[], "130")).unwrap();
    commit(&mut store, &mut cp, plan);
    let current = detail(&mut store, "windscribe", "a");
    assert_eq!(current["messages"][0]["unread"], false);
    assert_eq!(current["messages"][0]["starred"], true);
    assert!(current["messages"][0]["categories"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        store
            .handle(
                "draft_get",
                Some("windscribe"),
                json!({"conversation_id":initial["id"]})
            )
            .unwrap(),
        draft
    );
    assert!(search(&mut store, "windscribe", "draftnotsearchable").is_empty());
    let provider: String = store
        .db
        .query_row(
            "SELECT provider_labels_json FROM google_message_state WHERE account='windscribe'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&provider).unwrap(),
        vec!["INBOX", "UNREAD", "Label_Finance"]
    );
    assert_eq!(count(&store, "google_manual_intents"), 3);
    let plan = cp.begin_delta().unwrap();
    commit(&mut store, &mut cp, plan);
    // Even equal remote flags are observations, not acknowledgement of our edit.
    GooglePageStore::new(&mut store)
        .stage_message(
            &cp,
            content(raw("a", "manualoverlay", &["INBOX", "STARRED"])),
        )
        .unwrap();
    let plan = prepare_history_page(&cp, history(&["a"], &[], "140")).unwrap();
    commit(&mut store, &mut cp, plan);
    assert_eq!(count(&store, "google_manual_intents"), 3);
    let plan = cp.begin_delta().unwrap();
    commit(&mut store, &mut cp, plan);
    let plan = prepare_history_page(&cp, history(&[], &["a"], "150")).unwrap();
    commit(&mut store, &mut cp, plan);
    let (inbox,sent,deleted):(bool,bool,bool)=store.db.query_row("SELECT m.inbox,m.sent,g.deleted FROM messages m JOIN google_message_state g ON g.account=m.account AND g.message_id=m.id WHERE m.account='windscribe' AND m.remote_id='a'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert!(!inbox && !sent && deleted);
    assert_eq!(search(&mut store, "windscribe", "manualoverlay").len(), 1);
    let deleted_detail = detail(&mut store, "windscribe", "a");
    assert_eq!(deleted_detail["messages"][0]["source_available"], true);
    assert_eq!(deleted_detail["messages"][0]["starred"], true);
    drop(store);
    let mut store = Store::open(tmp.path()).unwrap();
    assert_eq!(
        store
            .handle(
                "draft_get",
                Some("windscribe"),
                json!({"conversation_id":initial["id"]})
            )
            .unwrap(),
        draft
    );
    assert_eq!(count(&store, "google_manual_intents"), 3);
}

#[test]
fn expired_cursor_restart_preserves_unseen_messages_and_rate_limit_does_not_advance() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let mut cp = store.google_begin_full("windscribe", "100").unwrap();
    GooglePageStore::new(&mut store)
        .stage_message(&cp, content(raw("a", "retainedabsence", &["INBOX"])))
        .unwrap();
    let plan = prepare_full_page(&cp, list(&["a"], None)).unwrap();
    commit(&mut store, &mut cp, plan);
    let plan = prepare_history_page(&cp, history(&[], &[], "120")).unwrap();
    commit(&mut store, &mut cp, plan);
    let plan = cp.begin_delta().unwrap();
    commit(&mut store, &mut cp, plan);
    let before = cp.clone();
    let mut fake = Fake::default();
    fake.histories.push_back(Err(GoogleError::RateLimited {
        retry_after_secs: Some(60),
    }));
    assert!(matches!(
        advance(&mut cp, &mut fake, &mut GooglePageStore::new(&mut store)),
        Err(GoogleError::RateLimited { .. })
    ));
    assert_eq!(cp, before);
    assert_eq!(store.google_checkpoint("windscribe").unwrap(), Some(before));
    fake.histories.push_back(Err(GoogleError::HistoryExpired));
    advance(&mut cp, &mut fake, &mut GooglePageStore::new(&mut store)).unwrap();
    assert_eq!(cp.phase, Phase::NeedsFullSync);
    drop(store);
    let mut store = Store::open(tmp.path()).unwrap();
    assert_eq!(
        store.google_checkpoint("windscribe").unwrap(),
        Some(cp.clone())
    );
    let plan = cp.restart_full("200".into()).unwrap();
    commit(&mut store, &mut cp, plan);
    assert_eq!(cp.generation, 2);
    fake.lists.push_back(Ok(list(&[], None)));
    fake.histories.push_back(Ok(history(&[], &[], "220")));
    advance(&mut cp, &mut fake, &mut GooglePageStore::new(&mut store)).unwrap();
    advance(&mut cp, &mut fake, &mut GooglePageStore::new(&mut store)).unwrap();
    assert_eq!(cp.committed_history_id.as_deref(), Some("220"));
    assert_eq!(search(&mut store, "windscribe", "retainedabsence").len(), 1);
    let inbox: bool = store
        .db
        .query_row(
            "SELECT inbox FROM messages WHERE account='windscribe' AND remote_id='a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(inbox, "full scan absence cannot infer deletion");
}

#[test]
fn missing_or_corrupt_staged_content_never_advances_checkpoint() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let cp = store.google_begin_full("windscribe", "100").unwrap();
    let plan = prepare_full_page(&cp, list(&["a"], None)).unwrap();
    assert!(matches!(
        GooglePageStore::new(&mut store).commit_page(&plan),
        Err(GoogleError::StoreUnavailable)
    ));
    let initial = count(&store, "messages");
    GooglePageStore::new(&mut store)
        .stage_message(&cp, content(raw("a", "unpublishedsource", &["INBOX"])))
        .unwrap();
    let hash:String=store.db.query_row("SELECT source_hash FROM google_staged_messages WHERE account='windscribe' AND remote_id='a'",[],|r|r.get(0)).unwrap();
    let path = tmp.path().join("blobs").join(&hash);
    std::fs::remove_file(&path).unwrap();
    assert!(matches!(
        GooglePageStore::new(&mut store).commit_page(&plan),
        Err(GoogleError::StoreUnavailable)
    ));
    std::fs::write(&path, b"corrupted bytes").unwrap();
    assert!(matches!(
        GooglePageStore::new(&mut store).commit_page(&plan),
        Err(GoogleError::StoreUnavailable)
    ));
    assert_eq!(
        store.google_checkpoint("windscribe").unwrap(),
        Some(cp.clone())
    );
    assert_eq!(count(&store, "messages"), initial);
    assert!(search(&mut store, "windscribe", "unpublishedsource").is_empty());
    // A missing blob may be refetched; a corrupt existing blob is not silently trusted.
    assert!(GooglePageStore::new(&mut store)
        .stage_message(&cp, content(raw("a", "unpublishedsource", &["INBOX"])))
        .is_err());
    std::fs::remove_file(path).unwrap();
    GooglePageStore::new(&mut store)
        .stage_message(&cp, content(raw("a", "unpublishedsource", &["INBOX"])))
        .unwrap();
    GooglePageStore::new(&mut store).commit_page(&plan).unwrap();
    assert_eq!(
        search(&mut store, "windscribe", "unpublishedsource").len(),
        1
    );
}

#[test]
fn mime_corpus_import_preserves_bytes_visible_search_and_attachment_checksums() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let cp = store.google_begin_full("windscribe", "100").unwrap();
    let manifest: Value =
        serde_json::from_str(include_str!("../../../fixtures/mime-manifest.json")).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let mut ids = Vec::new();
    for (index, entry) in manifest["messages"].as_array().unwrap().iter().enumerate() {
        let id = format!("corpus-{index}");
        let bytes = std::fs::read(root.join(entry["path"].as_str().unwrap())).unwrap();
        GooglePageStore::new(&mut store)
            .stage_message(
                &cp,
                MessageContent {
                    id: id.clone(),
                    thread_id: format!("thread-{id}"),
                    label_ids: vec!["INBOX".into()],
                    history_id: "110".into(),
                    source_mime: bytes,
                },
            )
            .unwrap();
        ids.push(id);
    }
    let borrowed: Vec<&str> = ids.iter().map(String::as_str).collect();
    GooglePageStore::new(&mut store)
        .commit_page(&prepare_full_page(&cp, list(&borrowed, None)).unwrap())
        .unwrap();
    for (index, entry) in manifest["messages"].as_array().unwrap().iter().enumerate() {
        let remote = &ids[index];
        let message = detail(&mut store, "windscribe", remote)["messages"][0].clone();
        let text = message["text"].as_str().unwrap();
        for expected in entry["expected_text_contains"].as_array().unwrap() {
            assert!(
                text.contains(expected.as_str().unwrap()),
                "missing {expected} in {text}"
            );
        }
        if let Some(forbidden) = entry["expected_text_not_contains"].as_array() {
            for needle in forbidden {
                assert!(
                    !text.contains(needle.as_str().unwrap()),
                    "unsafe text leaked: {needle}"
                );
            }
        }
        let hash: String = store
            .db
            .query_row(
                "SELECT source_hash FROM messages WHERE account='windscribe' AND remote_id=?1",
                [remote],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(hash, entry["source_sha256"]);
        let retained = store
            .verified_blob(&hash, entry["source_size"].as_u64().unwrap())
            .unwrap();
        assert_eq!(
            std::fs::read(retained).unwrap(),
            std::fs::read(root.join(entry["path"].as_str().unwrap())).unwrap()
        );
        if let Some(attachments) = entry["attachments"].as_array() {
            assert_eq!(
                message["attachments"].as_array().unwrap().len(),
                attachments.len()
            );
            for expected in attachments {
                let found = message["attachments"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|a| a["name"] == expected["name"])
                    .unwrap();
                let opened = store
                    .handle("attachment", Some("windscribe"), json!({"id":found["id"]}))
                    .unwrap();
                assert_eq!(opened["sha256"], expected["sha256"]);
            }
        }
    }
    let matches = store
        .handle(
            "conversations",
            Some("windscribe"),
            json!({"folder":"all","query":"availability","field":"body","limit":50}),
        )
        .unwrap();
    assert!(matches["conversations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["id"] == local_id("thread", "windscribe", "thread-corpus-2")));
    assert!(search(&mut store, "windscribe", "SCRIPT_MUST_NOT_APPEAR").is_empty());
}

#[test]
fn accepted_assistance_category_survives_provider_observation_but_manual_value_wins() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    store
        .google_map_category("windscribe", "finance", "Label_finance")
        .unwrap();
    let mut cp = store.google_begin_full("windscribe", "100").unwrap();
    GooglePageStore::new(&mut store)
        .stage_message(
            &cp,
            content(raw("a", "assistedcategory", &["INBOX", "UNREAD"])),
        )
        .unwrap();
    let plan = prepare_full_page(&cp, list(&["a"], None)).unwrap();
    commit(&mut store, &mut cp, plan);
    let original = detail(&mut store, "windscribe", "a");
    let mid = original["messages"][0]["id"].as_str().unwrap();
    // The assistance module separately verifies source/policy stamps before this
    // accepted-intent record exists. This test isolates provider reconciliation.
    store.db.execute("INSERT INTO assist_category_intents(account,message_id,category_id,value,revision,request_id) VALUES('windscribe',?1,'finance',1,1,'accepted-fake-result')",[mid]).unwrap();
    GooglePageStore::new(&mut store)
        .stage_message(
            &cp,
            content(raw("a", "assistedcategory", &["INBOX", "UNREAD"])),
        )
        .unwrap();
    let plan = prepare_history_page(&cp, history(&["a"], &[], "130")).unwrap();
    commit(&mut store, &mut cp, plan);
    let applied = detail(&mut store, "windscribe", "a");
    assert!(applied["messages"][0]["categories"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c == "finance"));
    assert_eq!(applied["messages"][0]["unread"], true);
    assert_eq!(applied["messages"][0]["starred"], false);
    store.handle("set_category",Some("windscribe"),json!({"id":applied["id"],"expected_revision":applied["revision"],"request_id":"human-rejects-tag","message_ids":[applied["messages"][0]["id"]],"category_id":"finance","present":false})).unwrap();
    let plan = cp.begin_delta().unwrap();
    commit(&mut store, &mut cp, plan);
    GooglePageStore::new(&mut store)
        .stage_message(
            &cp,
            content(raw(
                "a",
                "assistedcategory",
                &["INBOX", "UNREAD", "Label_finance"],
            )),
        )
        .unwrap();
    let plan = prepare_history_page(&cp, history(&["a"], &[], "140")).unwrap();
    commit(&mut store, &mut cp, plan);
    assert!(
        detail(&mut store, "windscribe", "a")["messages"][0]["categories"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn category_mappings_reject_all_system_ids_and_existing_invalid_mapping_blocks_import() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    for label in [
        "INBOX",
        "SENT",
        "UNREAD",
        "STARRED",
        "SPAM",
        "TRASH",
        "DRAFT",
        "IMPORTANT",
        "CHAT",
        "CATEGORY_PERSONAL",
        "CATEGORY_FUTURE",
        "category_social",
    ] {
        assert_eq!(
            store
                .google_map_category("windscribe", "finance", label)
                .unwrap_err()
                .code,
            "invalid_request"
        );
    }
    store.db.execute("INSERT INTO google_category_labels(account,category_id,provider_label_id) VALUES('windscribe','finance','DRAFT')",[]).unwrap();
    let cp = store.google_begin_full("windscribe", "100").unwrap();
    let plan = prepare_full_page(&cp, list(&["a"], None)).unwrap();
    GooglePageStore::new(&mut store)
        .stage_message(
            &cp,
            content(raw("a", "invalidmapping", &["INBOX", "DRAFT"])),
        )
        .unwrap();
    assert!(matches!(
        GooglePageStore::new(&mut store).commit_page(&plan),
        Err(GoogleError::InvalidResponse(_))
    ));
    assert_eq!(store.google_checkpoint("windscribe").unwrap(), Some(cp));
    assert!(search(&mut store, "windscribe", "invalidmapping").is_empty());
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE account='windscribe' AND remote_id='a'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap(),
        0,
    );
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT COUNT(*) FROM google_page_commits WHERE account='windscribe'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap(),
        0,
    );
    assert_eq!(
        store.db.query_row(
            "SELECT COUNT(*) FROM google_staged_messages WHERE account='windscribe' AND remote_id='a'",
            [],
            |r| r.get::<_, i64>(0),
        ).unwrap(),
        1,
    );
}
