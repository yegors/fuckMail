use super::*;
use crate::{google_drafts::*, google_send::*, local_content::*};
use fm_google::transport::CancellationToken;

fn code<T>(result: Result<T>) -> String {
    match result {
        Ok(_) => panic!("expected refusal"),
        Err(e) => e.code,
    }
}
fn request(store: &Store, account: &str, request: &str) -> Value {
    json!({"expected_revision":store.account_lifecycle(account).unwrap()["revision"],"request_id":request,"retention":"archive"})
}
fn remove(store: &mut Store, account: &str, request_id: &str) -> AccountCleanupLease {
    let p = request(store, account, request_id);
    match store.account_remove_begin(account, &p).unwrap() {
        AccountRemovalStep::Cleanup(l) => l,
        _ => panic!("new removal"),
    }
}
fn clean(store: &mut Store, lease: &AccountCleanupLease) -> Value {
    let claim = store.account_cleanup_claim(lease).unwrap();
    store
        .account_cleanup_finish(&claim, AccountCleanupOutcome::Removed)
        .unwrap()
}
fn setup(path: &std::path::Path) -> (Store, GoogleDraftScope) {
    let mut store = Store::open_live(path).unwrap();
    let scopes = BTreeSet::from([
        "https://www.googleapis.com/auth/gmail.modify".into(),
        "https://www.googleapis.com/auth/calendar.events".into(),
        "https://www.googleapis.com/auth/calendar.calendarlist.readonly".into(),
    ]);
    let account = store
        .bind_google_account("lifecycle-owner", "author@example.test", &scopes, None)
        .unwrap();
    store
        .bind_google_account("other-owner", "other@example.test", &scopes, None)
        .unwrap();
    (
        store,
        GoogleDraftScope {
            account,
            connection_generation: 1,
            primary_email: "author@example.test".into(),
        },
    )
}
fn draft(store: &mut Store, scope: &GoogleDraftScope, request_id: &str) -> Value {
    let created = store
        .handle(
            "draft_create",
            Some(&scope.account),
            json!({"mode":"new","request_id":request_id}),
        )
        .unwrap();
    store.handle("draft_save",Some(&scope.account),json!({"draft_id":created["draft_id"],"expected_revision":1,"request_id":format!("save-{request_id}"),"body":"Preserved unsent human text","subject":"Human subject","to_recipients":[{"email":"recipient@example.test"}]})).unwrap()
}
fn restore(store: &mut Store, account: &str) -> String {
    let rev = store.account_lifecycle(account).unwrap()["revision"]
        .as_i64()
        .unwrap();
    let scopes = BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]);
    let lease = store
        .google_restore_admit(
            account,
            rev,
            "lifecycle-owner",
            "author@example.test",
            &scopes,
            "desktop-client",
        )
        .unwrap();
    let path = store.profile.join("desktop-client.json");
    store
        .google_restore_bind(&lease, path.to_str().unwrap())
        .unwrap()
}

#[test]
fn removal_cleanup_claims_are_serial_and_old_receipts_cannot_delete_restored_grants() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(dir.path());
    let saved = draft(&mut store, &scope, "authored");
    let params = request(&store, &scope.account, "remove-1");
    let lease = remove(&mut store, &scope.account, "remove-1");
    assert_eq!(
        store.account_lifecycle(&scope.account).unwrap()["state"],
        "removed"
    );
    assert_eq!(
        store
            .google_account_bindings()
            .unwrap()
            .into_iter()
            .find(|v| v.account == scope.account)
            .unwrap()
            .connection_generation,
        2
    );
    assert_eq!(
        store.handle("accounts", None, json!({})).unwrap()["accounts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let archived = store
        .handle("accounts", None, json!({"include_removed":true}))
        .unwrap();
    let archived = archived["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == scope.account)
        .unwrap();
    assert_eq!(archived["read_only"], true);
    assert_eq!(archived["google"]["capabilities"]["send"], false);
    let current = store
        .handle(
            "draft_get",
            Some(&scope.account),
            json!({"draft_id":saved["draft_id"]}),
        )
        .unwrap();
    assert_eq!(current["body"], saved["body"]);
    assert_eq!(current["read_only"], true);
    assert_eq!(current["origin"], "local");
    let claim = store.account_cleanup_claim(&lease).unwrap();
    assert_eq!(
        code(store.account_cleanup_claim(&lease)),
        "stale_account_revision"
    );
    let retry = request(&store, &scope.account, "remove-racing");
    assert_eq!(
        code(store.account_remove_begin(&scope.account, &retry)),
        "account_cleanup_busy"
    );
    let done = store
        .account_cleanup_finish(&claim, AccountCleanupOutcome::Removed)
        .unwrap();
    let immutable = done["operation"].clone();
    assert_eq!(done["lifecycle"]["cleanup"]["state"], "complete");
    let scopes = BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]);
    assert_eq!(
        code(store.check_google_connection(
            "lifecycle-owner",
            "author@example.test",
            &scopes,
            None,
            "desktop-client"
        )),
        "account_removed"
    );
    assert_eq!(restore(&mut store, &scope.account), scope.account);
    match store.account_remove_begin(&scope.account, &params).unwrap() {
        AccountRemovalStep::Reply(v) => {
            assert_eq!(v["operation"], immutable);
            assert_eq!(v["lifecycle"]["state"], "active");
            assert_eq!(v["operation_is_current"], false);
        }
        _ => panic!("historical replay must never authorize cleanup"),
    }
    assert_eq!(
        code(store.account_cleanup_claim(&lease)),
        "stale_account_revision"
    );
    let changed = json!({"retention":"archive","expected_revision":1,"request_id":"authored"});
    assert_eq!(
        code(store.account_remove_begin(&scope.account, &changed)),
        "request_id_conflict"
    );
}

#[test]
fn failed_cleanup_and_restore_restart_remain_truthfully_retryable() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(dir.path());
    let first = request(&store, &scope.account, "remove-1");
    let lease = remove(&mut store, &scope.account, "remove-1");
    let claim = store.account_cleanup_claim(&lease).unwrap();
    let failed = store
        .account_cleanup_finish(
            &claim,
            AccountCleanupOutcome::Failed {
                code: "private native exception must never be stored".into(),
            },
        )
        .unwrap();
    assert_eq!(
        failed["lifecycle"]["cleanup"]["code"],
        "credential_cleanup_failed"
    );
    assert_eq!(
        code(store.google_restore_check(
            &scope.account,
            failed["lifecycle"]["revision"].as_i64().unwrap()
        )),
        "account_cleanup_required"
    );
    assert!(matches!(
        store.account_remove_begin(&scope.account, &first).unwrap(),
        AccountRemovalStep::Reply(_)
    ));
    let second = request(&store, &scope.account, "remove-2");
    let lease = remove(&mut store, &scope.account, "remove-2");
    let _inflight = store.account_cleanup_claim(&lease).unwrap();
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    let life = store.account_lifecycle(&scope.account).unwrap();
    assert_eq!(life["cleanup"]["state"], "pending");
    assert_eq!(life["cleanup"]["code"], "interrupted");
    let resumed = match store.account_remove_begin(&scope.account, &second).unwrap() {
        AccountRemovalStep::Cleanup(l) => l,
        _ => panic!(),
    };
    clean(&mut store, &resumed);
    let rev = store.account_lifecycle(&scope.account).unwrap()["revision"]
        .as_i64()
        .unwrap();
    let scopes = BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]);
    assert_eq!(
        code(store.google_restore_admit(
            &scope.account,
            rev,
            "different-subject",
            "author@example.test",
            &scopes,
            "desktop-client"
        )),
        "identity_mismatch"
    );
    let restore = store
        .google_restore_admit(
            &scope.account,
            rev,
            "lifecycle-owner",
            "author@example.test",
            &scopes,
            "desktop-client",
        )
        .unwrap();
    let request = request(&store, &scope.account, "cleanup-during-persistence");
    assert_eq!(
        code(store.account_remove_begin(&scope.account, &request)),
        "account_cleanup_busy"
    );
    assert_eq!(
        code(store.account_cleanup_claim(&resumed)),
        "stale_account_revision"
    );
    store.google_restore_failed(&restore, "cancelled").unwrap();
    assert_eq!(
        store.account_lifecycle(&scope.account).unwrap()["cleanup"]["state"],
        "pending"
    );
    assert!(
        matches!(
            store.account_remove_begin(&scope.account, &second).unwrap(),
            AccountRemovalStep::Reply(_)
        ),
        "old completed cleanup cannot remove a later partial restore grant"
    );
    let cleanup = remove(&mut store, &scope.account, "remove-after-restore-failure");
    clean(&mut store, &cleanup);
    let rev = store.account_lifecycle(&scope.account).unwrap()["revision"]
        .as_i64()
        .unwrap();
    let _restore = store
        .google_restore_admit(
            &scope.account,
            rev,
            "lifecycle-owner",
            "author@example.test",
            &scopes,
            "desktop-client",
        )
        .unwrap();
    drop(store);
    let store = Store::open_live(dir.path()).unwrap();
    let life = store.account_lifecycle(&scope.account).unwrap();
    assert_eq!(life["state"], "removed");
    assert_eq!(life["cleanup"]["state"], "pending");
    assert_eq!(
        code(store.google_restore_check(&scope.account, life["revision"].as_i64().unwrap())),
        "account_cleanup_required"
    );
}

#[test]
fn held_local_and_provider_preparations_cannot_revive_after_remove_restore() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(dir.path());
    let saved = draft(&mut store, &scope, "authored");
    let save = json!({"draft_id":saved["draft_id"],"expected_revision":2,"request_id":"held-save","body":"stale replacement"});
    let local = match store
        .local_content_begin("draft_save", &scope.account, save.clone())
        .unwrap()
    {
        LocalContentStep::Prepare(p) => {
            prepare_local_content(p, &CancellationToken::new()).unwrap()
        }
        _ => panic!(),
    };
    let upload =
        json!({"draft_id":saved["draft_id"],"expected_revision":2,"request_id":"held-upload"});
    let upload = match store.google_draft_begin(&scope, &upload).unwrap() {
        GoogleDraftStep::Prepare(p) => prepare_google_draft(p, &CancellationToken::new()).unwrap(),
        _ => panic!(),
    };
    let send = json!({"draft_id":saved["draft_id"],"expected_revision":2,"expected_google_revision":0,"request_id":"held-send"});
    let send = match store.google_send_begin(&scope, &send).unwrap() {
        GoogleSendStep::Prepare(p) => prepare_google_send(p, &CancellationToken::new()).unwrap(),
        _ => panic!(),
    };
    let lease = remove(&mut store, &scope.account, "remove");
    clean(&mut store, &lease);
    restore(&mut store, &scope.account);
    assert_eq!(
        code(store.local_content_finish(local)),
        "stale_account_revision"
    );
    assert!(store.google_draft_finish(upload).is_err());
    assert!(store.google_send_finish(send).is_err());
    assert_eq!(
        store
            .db
            .query_row("SELECT COUNT(*) FROM capture_operations", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT COUNT(*) FROM operations WHERE request_id='held-save'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    let current = store
        .handle(
            "draft_get",
            Some(&scope.account),
            json!({"draft_id":saved["draft_id"]}),
        )
        .unwrap();
    assert_eq!(current["body"], saved["body"]);
    assert_eq!(current["revision"], 2);
    store
        .handle("draft_save", Some(&scope.account), save)
        .unwrap();
}

#[test]
fn archived_prepared_replay_is_status_only_and_exact_late_send_ack_survives() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(dir.path());
    let saved = draft(&mut store, &scope, "pending");
    let p = json!({"draft_id":saved["draft_id"],"expected_revision":2,"expected_google_revision":0,"request_id":"send"});
    let proof = match store.google_send_begin(&scope, &p).unwrap() {
        GoogleSendStep::Prepare(p) => prepare_google_send(p, &CancellationToken::new()).unwrap(),
        _ => panic!(),
    };
    let status = store.google_send_finish(proof).unwrap();
    let other = draft(&mut store, &scope, "prepared");
    let upload = json!({"draft_id":other["draft_id"],"expected_revision":2,"request_id":"upload"});
    let proof = match store.google_draft_begin(&scope, &upload).unwrap() {
        GoogleDraftStep::Prepare(p) => prepare_google_draft(p, &CancellationToken::new()).unwrap(),
        _ => panic!(),
    };
    let prepared = store.google_draft_finish(proof).unwrap();
    let lease = store
        .google_send_claim(&scope, "send", status["revision"].as_i64().unwrap())
        .unwrap();
    let removal = remove(&mut store, &scope.account, "remove");
    assert!(matches!(
        store.google_draft_intent(&scope, &upload).unwrap(),
        GoogleDraftIntent::Reply(_)
    ));
    assert_eq!(
        code(store.google_draft_claim(
            &scope,
            "upload",
            prepared["operation"]["revision"].as_i64().unwrap()
        )),
        "account_removed"
    );
    let accepted = store
        .google_send_outcome(
            &lease,
            GoogleSendOutcome::Accepted {
                message_id: "exact-ack".into(),
                thread_id: "exact-thread".into(),
            },
        )
        .unwrap();
    assert_eq!(accepted["state"], "accepted");
    assert_eq!(accepted["ack"]["message_id"], "exact-ack");
    assert!(matches!(
        store.google_send_intent(&scope, &p).unwrap(),
        GoogleSendIntent::Reply(_)
    ));
    let mut conflict = p.clone();
    conflict["expected_revision"] = json!(99);
    assert_eq!(
        code(store.google_send_intent(&scope, &conflict)),
        "request_id_conflict"
    );
    assert_eq!(
        code(store.google_send_reconcile_begin(&scope, "send")),
        "account_removed"
    );
    clean(&mut store, &removal);
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    assert_eq!(
        store.google_send_status(&scope.account, "send").unwrap()["ack"],
        accepted["ack"]
    );
    let current = store
        .handle(
            "draft_get",
            Some(&scope.account),
            json!({"draft_id":saved["draft_id"]}),
        )
        .unwrap();
    assert_eq!(current["body"], saved["body"]);
    assert_eq!(current["google_send"]["state"], "accepted");
}

#[test]
fn archive_keeps_cached_calendar_and_capture_but_blocks_new_authorship() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let created = store
        .handle(
            "draft_create",
            Some("personal"),
            json!({"mode":"new","request_id":"draft"}),
        )
        .unwrap();
    let saved=store.handle("draft_save",Some("personal"),json!({"draft_id":created["draft_id"],"expected_revision":1,"request_id":"save","body":"Original","to_recipients":[{"email":"reader@example.test"}]})).unwrap();
    let capture =
        json!({"draft_id":saved["draft_id"],"expected_revision":2,"request_id":"capture"});
    let receipt = store
        .handle("draft_capture", Some("personal"), capture.clone())
        .unwrap();
    let before = store
        .handle("calendar", Some("personal"), json!({}))
        .unwrap();
    let lease = remove(&mut store, "personal", "remove");
    clean(&mut store, &lease);
    assert_eq!(
        store
            .handle(
                "draft_create",
                Some("personal"),
                json!({"mode":"new","request_id":"draft"})
            )
            .unwrap(),
        created
    );
    assert_eq!(
        store
            .handle("draft_capture", Some("personal"), capture)
            .unwrap(),
        receipt
    );
    for (method, p) in [
        ("draft_create", json!({"mode":"new","request_id":"new"})),
        (
            "draft_discard",
            json!({"draft_id":saved["draft_id"],"expected_revision":2,"request_id":"discard"}),
        ),
        (
            "draft_save",
            json!({"draft_id":saved["draft_id"],"expected_revision":2,"request_id":"edit","body":"lost"}),
        ),
        (
            "draft_attachment_import",
            json!({"draft_id":saved["draft_id"],"expected_revision":2,"request_id":"file","path":dir.path().join("absent")}),
        ),
        (
            "draft_get",
            json!({"conversation_id":"personal-thread-000"}),
        ),
        (
            "view_save",
            json!({"view":{},"expected_revision":0,"request_id":"view"}),
        ),
        (
            "account_update",
            json!({"name":"Changed","signature_text":"Changed","expected_revision":1,"request_id":"metadata"}),
        ),
    ] {
        assert_eq!(
            code(store.handle(method, Some("personal"), p)),
            "account_removed",
            "{method}"
        );
    }
    assert!(
        store.handle("calendar_sources", None, json!({})).unwrap()["calendars"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["account"] != "personal")
    );
    let after = store
        .handle("calendar", Some("personal"), json!({}))
        .unwrap();
    assert_eq!(
        after["events"].as_array().unwrap().len(),
        before["events"].as_array().unwrap().len()
    );
    assert!(after["events"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["read_only"] == true && v["can_edit"] == false));
    assert!(!store
        .handle("conversations", Some("personal"), json!({}))
        .unwrap()["conversations"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn schema_seventeen_upgrade_preserves_old_columns_receipts_and_future_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let draft = store
        .handle(
            "draft_create",
            Some("personal"),
            json!({"mode":"new","request_id":"historical"}),
        )
        .unwrap();
    crate::test_profiles::restore_schema_seventeen(&store.db);
    let names = [
        "accounts",
        "drafts",
        "operations",
        "capture_operations",
        "calendar_write_operations",
        "calendar_sources",
        "messages",
        "attachments",
    ];
    let snapshot = |db: &Connection| {
        names
            .iter()
            .map(|name| {
                let columns = db
                    .prepare(&format!("SELECT name FROM pragma_table_info('{name}')"))
                    .unwrap()
                    .query_map([], |r| r.get::<_, String>(0))
                    .unwrap()
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .unwrap();
                let mut stmt = db
                    .prepare(&format!(
                        "SELECT {} FROM {name} ORDER BY rowid",
                        columns.join(",")
                    ))
                    .unwrap();
                let rows = stmt
                    .query_map([], |r| {
                        (0..columns.len())
                            .map(|n| r.get::<_, rusqlite::types::Value>(n))
                            .collect::<rusqlite::Result<Vec<_>>>()
                    })
                    .unwrap()
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .unwrap();
                (name.to_string(), columns, rows)
            })
            .collect::<Vec<_>>()
    };
    let before = snapshot(&store.db);
    drop(store);
    let mut store = Store::open(dir.path()).unwrap();
    assert_eq!(
        store
            .db
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        crate::SCHEMA_VERSION as i64
    );
    for (name, columns, rows) in before {
        let mut stmt = store
            .db
            .prepare(&format!(
                "SELECT {} FROM {name} ORDER BY rowid",
                columns.join(",")
            ))
            .unwrap();
        let after = stmt
            .query_map([], |r| {
                (0..columns.len())
                    .map(|n| r.get::<_, rusqlite::types::Value>(n))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(rows, after, "{name}");
    }
    assert_eq!(
        store.account_lifecycle("personal").unwrap(),
        json!({"state":"active","revision":1,"cleanup":null})
    );
    assert_eq!(
        store
            .handle(
                "draft_create",
                Some("personal"),
                json!({"mode":"new","request_id":"historical"})
            )
            .unwrap(),
        draft
    );
    store
        .db
        .pragma_update(None, "user_version", crate::SCHEMA_VERSION + 1)
        .unwrap();
    drop(store);
    assert_eq!(code(Store::open(dir.path())), "schema_mismatch");
}

#[test]
fn archived_prepared_capture_exports_without_attempt_or_journal_transition() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let created = store
        .handle(
            "draft_create",
            Some("personal"),
            json!({"mode":"new","request_id":"draft"}),
        )
        .unwrap();
    let saved=store.handle("draft_save",Some("personal"),json!({"draft_id":created["draft_id"],"expected_revision":1,"request_id":"save","body":"Retained","to_recipients":[{"email":"reader@example.test"}]})).unwrap();
    let p = json!({"draft_id":saved["draft_id"],"expected_revision":2,"request_id":"capture"});
    let original = store
        .handle("draft_capture", Some("personal"), p.clone())
        .unwrap();
    // Represent a crash after the durable Prepared receipt, before its claim.
    store.db.execute("UPDATE capture_operations SET json=json_set(json,'$.operation.state','Prepared','$.attempts_started',0) WHERE account='personal' AND id='capture'",[]).unwrap();
    let before: String = store
        .db
        .query_row(
            "SELECT json FROM capture_operations WHERE id='capture'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let removal = remove(&mut store, "personal", "remove");
    clean(&mut store, &removal);
    let reply = store.handle("draft_capture", Some("personal"), p).unwrap();
    assert_eq!(reply["state"], "prepared");
    assert_eq!(reply["attempts_started"], 0);
    assert_eq!(reply["sha256"], original["sha256"]);
    assert_eq!(
        std::fs::read(reply["path"].as_str().unwrap()).unwrap(),
        std::fs::read(original["path"].as_str().unwrap()).unwrap()
    );
    let after: String = store
        .db
        .query_row(
            "SELECT json FROM capture_operations WHERE id='capture'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(before, after);
}

#[test]
fn retained_source_and_attachment_exports_survive_archive_but_held_import_cannot_publish() {
    use crate::google_draft_inventory::*;
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(dir.path());
    let read = GoogleDraftReadScope {
        account: scope.account.clone(),
        connection_generation: 1,
    };
    store.google_draft_inventory_begin(&read).unwrap();
    let GoogleDraftInventoryStep::List(list) = store.google_draft_inventory_next(&read).unwrap()
    else {
        panic!()
    };
    store
        .google_draft_inventory_list_finish(
            list,
            GoogleDraftInventoryPage {
                drafts: vec![
                    GoogleDraftListedId {
                        draft_id: "retained".into(),
                        message_id: "retained-message".into(),
                    },
                    GoogleDraftListedId {
                        draft_id: "held".into(),
                        message_id: "held-message".into(),
                    },
                ],
                next_page_token: None,
                result_size_estimate: Some(2),
            },
        )
        .unwrap();
    let raw=b"From: Outside <outside@example.test>\r\nTo: Recipient <reader@example.test>\r\nSubject: Retained source\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=p\r\n\r\n--p\r\nContent-Type: text/plain\r\n\r\nExact text\r\n--p\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=retained.bin\r\nContent-Transfer-Encoding: base64\r\n\r\nAAEC/w==\r\n--p--\r\n";
    let prepare = |store: &Store| {
        let GoogleDraftInventoryStep::Fetch(plan) =
            store.google_draft_inventory_next(&read).unwrap()
        else {
            panic!()
        };
        let id = plan.provider_draft_id().to_owned();
        prepare_google_draft_import(
            plan,
            GoogleDraftImportOutcome::Available {
                draft_id: id.clone(),
                message_id: format!("{id}-message"),
                thread_id: format!("{id}-thread"),
                raw: raw.to_vec(),
            },
            &CancellationToken::new(),
        )
        .unwrap()
    };
    let first = prepare(&store);
    store.google_draft_import_finish(first).unwrap();
    let held = prepare(&store);
    let id: String = store
        .db
        .query_row(
            "SELECT draft_id FROM google_draft_sources WHERE account=?1 AND is_current=1",
            [&scope.account],
            |r| r.get(0),
        )
        .unwrap();
    let saved = store
        .handle("draft_get", Some(&scope.account), json!({"draft_id":id}))
        .unwrap();
    let revision = saved["remote_source"]["source_revision"].clone();
    let removal = remove(&mut store, &scope.account, "remove");
    clean(&mut store, &removal);
    assert_eq!(
        code(store.google_draft_import_finish(held)),
        "account_removed"
    );
    let history = store
        .google_draft_source_history(&scope.account, &json!({"draft_id":id}))
        .unwrap();
    assert_eq!(history["sources"].as_array().unwrap().len(), 1);
    let source = store
        .google_draft_source_begin(
            &scope.account,
            &json!({"draft_id":id,"source_revision":revision}),
        )
        .unwrap();
    let exported = export_google_draft_source(source, &CancellationToken::new()).unwrap();
    assert_eq!(
        std::fs::read(exported["path"].as_str().unwrap()).unwrap(),
        raw
    );
    let attachment=store.google_draft_source_begin(&scope.account,&json!({"draft_id":id,"source_revision":revision,"attachment_id":saved["attachments"][0]["id"]})).unwrap();
    let exported = export_google_draft_source(attachment, &CancellationToken::new()).unwrap();
    assert_eq!(
        std::fs::read(exported["path"].as_str().unwrap()).unwrap(),
        [0, 1, 2, 255]
    );
    let ordinary = store
        .handle(
            "attachment",
            Some(&scope.account),
            json!({"id":saved["attachments"][0]["id"]}),
        )
        .unwrap();
    assert_eq!(
        std::fs::read(ordinary["path"].as_str().unwrap()).unwrap(),
        [0, 1, 2, 255]
    );
    assert_eq!(
        code(store.google_draft_authoring_begin(
            &scope.account,
            &json!({"draft_id":id,"source_revision":revision})
        )),
        "account_removed"
    );
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT COUNT(*) FROM google_draft_sources WHERE account=?1",
                [&scope.account],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
}

#[test]
fn live_calendar_late_ack_is_retained_while_archive_blocks_cache_and_pending_replay_dispatch() {
    use crate::live_calendar::{CalendarPage, CalendarSyncMode};
    use crate::live_calendar_writes::CalendarWriteOutcome;
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(dir.path());
    let calendar = "author@example.test";
    let catalog = store
        .calendar_catalog_begin(&scope.account, CalendarSyncMode::Full)
        .unwrap();
    let page = CalendarPage {
        input_cursor: None,
        items: vec![json!({"id":calendar,"accessRole":"owner","timeZone":"UTC"})],
        next_page_token: None,
        next_sync_token: Some("catalog".into()),
    };
    store.calendar_stage_page_chunk(&catalog, &page, 0).unwrap();
    store.calendar_seal_page(&catalog, &page).unwrap();
    while store.calendar_catalog_finalize_chunk(&catalog).unwrap() > 0 {}
    store.calendar_publish_sync(&catalog).unwrap();
    let events = store
        .calendar_sync_begin(&scope.account, calendar, CalendarSyncMode::Full)
        .unwrap();
    let empty = CalendarPage {
        input_cursor: None,
        items: vec![],
        next_page_token: None,
        next_sync_token: Some("events".into()),
    };
    store.calendar_stage_page_chunk(&events, &empty, 0).unwrap();
    store.calendar_seal_page(&events, &empty).unwrap();
    store.calendar_publish_sync(&events).unwrap();
    while store.calendar_finalize_chunk(&events).unwrap() > 0 {}
    let request = json!({"request_id":"create-calendar","kind":"create","calendar_id":calendar,"fields":{"title":"Original","start":"2026-09-14","end":"2026-09-15","all_day":true,"time_zone":"UTC"}});
    let prepared = store
        .calendar_live_prepare(&scope.account, &request)
        .unwrap();
    let epoch = store.calendar_sync_scope(&scope.account, calendar).unwrap()["permission_epoch"]
        .as_i64()
        .unwrap();
    let lease = store
        .calendar_live_claim(
            &scope.account,
            "create-calendar",
            prepared["revision"].as_i64().unwrap(),
            1,
            epoch,
            None,
        )
        .unwrap();
    let read = store.calendar_live_reconcile_begin(&lease, 1).unwrap();
    let mut pending = request.clone();
    pending["request_id"] = json!("known-unsent");
    let pending_status = store
        .calendar_live_prepare(&scope.account, &pending)
        .unwrap();
    let removal = remove(&mut store, &scope.account, "remove");
    let mut acknowledged = lease.dispatch["body"].clone();
    acknowledged["id"] = lease.dispatch["target"]["event"].clone();
    acknowledged["etag"] = json!("provider-ack");
    acknowledged["status"] = json!("confirmed");
    assert_eq!(
        code(store.calendar_live_reconcile_read(&lease, &read, acknowledged.clone())),
        "account_removed"
    );
    store
        .calendar_live_record_outcome(&lease, CalendarWriteOutcome::Accepted(acknowledged.clone()))
        .unwrap();
    assert_eq!(
        store
            .calendar_live_status(&scope.account, "create-calendar")
            .unwrap()["state"],
        "accepted"
    );
    assert_eq!(
        store
            .calendar_live_prepare(&scope.account, &pending)
            .unwrap(),
        pending_status
    );
    assert_eq!(
        code(store.calendar_live_start(&scope.account, "known-unsent", 1)),
        "account_removed"
    );
    let _ = store.calendar_live_publish(&lease); // May retain a cache_published:false final receipt.
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT COUNT(*) FROM calendar_sources WHERE account=?1",
                [&scope.account],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    clean(&mut store, &removal);
    restore(&mut store, &scope.account);
    assert_eq!(
        code(store.calendar_live_reconcile_read(&lease, &read, acknowledged)),
        "stale_account_revision"
    );
    drop(store);
    let store = Store::open_live(dir.path()).unwrap();
    assert!(matches!(
        store
            .calendar_live_status(&scope.account, "create-calendar")
            .unwrap()["state"]
            .as_str(),
        Some("accepted" | "acknowledged")
    ));
    assert_eq!(
        store
            .calendar_live_status(&scope.account, "known-unsent")
            .unwrap()["state"],
        "prepared"
    );
}

#[test]
fn live_schema_seventeen_preserves_unknown_upload_and_accepted_send_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope) = setup(dir.path());
    let a = draft(&mut store, &scope, "upload-draft");
    let p = json!({"draft_id":a["draft_id"],"expected_revision":2,"request_id":"unknown-upload"});
    let proof = match store.google_draft_begin(&scope, &p).unwrap() {
        GoogleDraftStep::Prepare(p) => prepare_google_draft(p, &CancellationToken::new()).unwrap(),
        _ => panic!(),
    };
    let status = store.google_draft_finish(proof).unwrap();
    let upload = store
        .google_draft_claim(
            &scope,
            "unknown-upload",
            status["operation"]["revision"].as_i64().unwrap(),
        )
        .unwrap();
    store
        .google_draft_outcome(
            &upload,
            GoogleDraftOutcome::Unknown {
                code: "transport_interrupted".into(),
            },
        )
        .unwrap();
    let b = draft(&mut store, &scope, "send-draft");
    let p = json!({"draft_id":b["draft_id"],"expected_revision":2,"expected_google_revision":0,"request_id":"accepted-send"});
    let proof = match store.google_send_begin(&scope, &p).unwrap() {
        GoogleSendStep::Prepare(p) => prepare_google_send(p, &CancellationToken::new()).unwrap(),
        _ => panic!(),
    };
    let status = store.google_send_finish(proof).unwrap();
    let send = store
        .google_send_claim(
            &scope,
            "accepted-send",
            status["revision"].as_i64().unwrap(),
        )
        .unwrap();
    store
        .google_send_outcome(
            &send,
            GoogleSendOutcome::Accepted {
                message_id: "immutable-ack".into(),
                thread_id: "immutable-thread".into(),
            },
        )
        .unwrap();
    let journal = |store: &Store| {
        store.db.prepare("SELECT account,id,json,blob_hash,execution_kind FROM capture_operations ORDER BY account,id").unwrap().query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
    };
    let before = journal(&store);
    let raw = load_google_draft_upload(upload.take_upload(), &CancellationToken::new()).unwrap();
    let send_raw = load_google_send_upload(send.take_upload(), &CancellationToken::new()).unwrap();
    crate::test_profiles::restore_schema_seventeen(&store.db);
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    assert_eq!(journal(&store), before);
    assert_eq!(
        load_google_draft_upload(upload.take_upload(), &CancellationToken::new()).unwrap(),
        raw
    );
    assert_eq!(
        load_google_send_upload(send.take_upload(), &CancellationToken::new()).unwrap(),
        send_raw
    );
    assert_eq!(
        store
            .google_draft_status(&scope.account, a["draft_id"].as_str().unwrap())
            .unwrap()["state"],
        "acceptance_unknown"
    );
    assert_eq!(
        store
            .google_send_status(&scope.account, "accepted-send")
            .unwrap()["ack"]["message_id"],
        "immutable-ack"
    );
    let removal = remove(&mut store, &scope.account, "remove");
    clean(&mut store, &removal);
    assert_eq!(journal(&store), before);
    assert_eq!(
        store
            .handle(
                "draft_get",
                Some(&scope.account),
                json!({"draft_id":a["draft_id"]})
            )
            .unwrap()["body"],
        a["body"]
    );
}
