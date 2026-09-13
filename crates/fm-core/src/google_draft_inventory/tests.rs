use super::*;
use crate::google_drafts::{self, GoogleDraftOutcome, GoogleDraftScope, GoogleDraftStep};
use crate::google_send::{self, GoogleSendOutcome, GoogleSendStep};

const RAW: &[u8] = b"From: Remote Author <outside@example.test>\r\nTo: Other <other@example.test>\r\nCc: Copy <copy@example.test>\r\nBcc: Secret <secret@example.test>\r\nReply-To: Route <route@example.test>\r\nSubject: Original subject\r\nMessage-ID: <remote-original@example.test>\r\nIn-Reply-To: <parent@example.test>\r\nReferences: <root@example.test> <parent@example.test>\r\nMIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=parts\r\n\r\n--parts\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>Original remote text</p><img src=\"cid:picture\">\r\n--parts\r\nContent-Type: image/png; name=picture.png\r\nContent-ID: <picture>\r\nContent-Disposition: inline; filename=picture.png\r\nContent-Transfer-Encoding: base64\r\n\r\nAAEC/w==\r\n--parts--\r\n";

fn setup(path: &std::path::Path) -> (Store, GoogleDraftReadScope, GoogleDraftReadScope) {
    let mut store = Store::open_live(path).unwrap();
    let scopes = BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]);
    let mut bind = |subject, email| {
        let account = store
            .bind_google_account(subject, email, &scopes, None)
            .unwrap();
        let generation = store
            .google_account_bindings()
            .unwrap()
            .into_iter()
            .find(|b| b.account == account)
            .unwrap()
            .connection_generation;
        GoogleDraftReadScope {
            account,
            connection_generation: generation,
        }
    };
    let a = bind("inventory-a", "author@example.test");
    let b = bind("inventory-b", "second@example.test");
    (store, a, b)
}
fn scope(s: &GoogleDraftReadScope) -> GoogleDraftScope {
    GoogleDraftScope {
        account: s.account.clone(),
        connection_generation: s.connection_generation,
        primary_email: "author@example.test".into(),
    }
}
fn code<T>(result: Result<T>) -> String {
    match result {
        Err(e) => e.code,
        Ok(_) => panic!("Expected refusal"),
    }
}
fn page(store: &mut Store, s: &GoogleDraftReadScope, ids: &[&str], next: Option<&str>) -> Value {
    let GoogleDraftInventoryStep::List(plan) = store.google_draft_inventory_next(s).unwrap() else {
        panic!("list expected")
    };
    store
        .google_draft_inventory_list_finish(
            plan,
            GoogleDraftInventoryPage {
                drafts: ids
                    .iter()
                    .map(|id| GoogleDraftListedId {
                        draft_id: (*id).into(),
                        message_id: format!("listed-{id}"),
                    })
                    .collect(),
                next_page_token: next.map(Into::into),
                result_size_estimate: Some(ids.len() as u64),
            },
        )
        .unwrap()
}
fn fetch(store: &Store, s: &GoogleDraftReadScope) -> GoogleDraftImportPlan {
    let GoogleDraftInventoryStep::Fetch(plan) = store.google_draft_inventory_next(s).unwrap()
    else {
        panic!("fetch expected")
    };
    plan
}
fn available(plan: GoogleDraftImportPlan, raw: &[u8], member: &str) -> PreparedGoogleDraftImport {
    let id = plan.provider_draft_id().to_owned();
    prepare_google_draft_import(
        plan,
        GoogleDraftImportOutcome::Available {
            draft_id: id,
            message_id: member.into(),
            thread_id: "provider-thread".into(),
            raw: raw.into(),
        },
        &CancellationToken::new(),
    )
    .unwrap()
}
fn import(
    store: &mut Store,
    s: &GoogleDraftReadScope,
    id: &str,
    raw: &[u8],
    member: &str,
) -> Value {
    store.google_draft_inventory_begin(s).unwrap();
    page(store, s, &[id], None);
    let proof = available(fetch(store, s), raw, member);
    store.google_draft_import_finish(proof).unwrap()
}
fn draft(store: &mut Store, s: &GoogleDraftReadScope, id: &str) -> Value {
    store
        .handle("draft_get", Some(&s.account), json!({"draft_id":id}))
        .unwrap()
}
fn draft_id(store: &Store, s: &GoogleDraftReadScope, remote: &str) -> String {
    store.db.query_row("SELECT draft_id FROM google_draft_sources WHERE account=?1 AND provider_draft_id=?2 AND is_current=1",params![s.account,remote],|r|r.get(0)).unwrap()
}
fn raw_export(store: &Store, s: &GoogleDraftReadScope, id: &str, revision: &Value) -> Vec<u8> {
    let plan = store
        .google_draft_source_begin(
            &s.account,
            &json!({"draft_id":id,"source_revision":revision}),
        )
        .unwrap();
    let result = export_google_draft_source(plan, &CancellationToken::new()).unwrap();
    let bytes = std::fs::read(result["path"].as_str().unwrap()).unwrap();
    assert_eq!(result["sha256"], hash(&bytes));
    assert_eq!(result["size"], bytes.len());
    bytes
}
fn count(store: &Store, table: &str) -> i64 {
    store
        .db
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn create(store: &mut Store, s: &GoogleDraftReadScope, request: &str) -> String {
    store
        .handle(
            "draft_create",
            Some(&s.account),
            json!({"mode":"new","request_id":request}),
        )
        .unwrap()["draft_id"]
        .as_str()
        .unwrap()
        .into()
}
fn freeze_local(
    store: &mut Store,
    s: &GoogleDraftReadScope,
    id: &str,
    request: &str,
) -> (google_drafts::GoogleDraftLease, Vec<u8>) {
    let p = json!({"draft_id":id,"expected_revision":draft(store,s,id)["revision"],"request_id":request});
    let GoogleDraftStep::Prepare(plan) = store.google_draft_begin(&scope(s), &p).unwrap() else {
        panic!()
    };
    let proof = google_drafts::prepare_google_draft(plan, &CancellationToken::new()).unwrap();
    let state = store.google_draft_finish(proof).unwrap();
    let lease = store
        .google_draft_claim(
            &scope(s),
            request,
            state["operation"]["revision"].as_i64().unwrap(),
        )
        .unwrap();
    let raw =
        google_drafts::load_google_draft_upload(lease.take_upload(), &CancellationToken::new())
            .unwrap();
    (lease, raw)
}

#[test]
fn exact_raw_routing_cid_parts_and_account_isolation_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, a, b) = setup(dir.path());
    assert_eq!(
        store.google_draft_inventory_status(&a.account).unwrap()["generation"],
        0
    );
    assert_eq!(count(&store, "google_draft_inventory"), 0);
    import(&mut store, &a, "same-container", RAW, "fetched-new-member");
    let id = draft_id(&store, &a, "same-container");
    let got = draft(&mut store, &a, &id);
    assert_eq!(got["origin"], "google_snapshot");
    assert_eq!(got["read_only"], true);
    assert_eq!(got["from"], "outside@example.test");
    assert_eq!(got["from_name"], "Remote Author");
    for (key, email) in [
        ("to_recipients", "other@example.test"),
        ("cc_recipients", "copy@example.test"),
        ("bcc_recipients", "secret@example.test"),
    ] {
        assert_eq!(got[key][0]["email"], email);
    }
    assert_eq!(got["subject"], "Original subject");
    assert_eq!(got["signature_text"], "");
    assert!(got["body"]
        .as_str()
        .unwrap()
        .contains("Original remote text"));
    assert!(got["body_html"].as_str().unwrap().contains("cid:picture"));
    assert!(got["google"].is_null());
    assert!(got["google_send"].is_null());
    assert_eq!(got["remote_source"]["message_id"], "fetched-new-member");
    assert_eq!(
        raw_export(&store, &a, &id, &got["remote_source"]["source_revision"]),
        RAW
    );
    let attachment = got["attachment_ids"][0].as_str().unwrap();
    let plan=store.google_draft_source_begin(&a.account,&json!({"draft_id":id,"source_revision":got["remote_source"]["source_revision"],"attachment_id":attachment})).unwrap();
    let exported = export_google_draft_source(plan, &CancellationToken::new()).unwrap();
    assert_eq!(
        std::fs::read(exported["path"].as_str().unwrap()).unwrap(),
        [0, 1, 2, 255]
    );
    assert_eq!(
        code(store.google_draft_source_begin(
            &b.account,
            &json!({"draft_id":id,"source_revision":got["remote_source"]["source_revision"]})
        )),
        "not_found"
    );
    import(
        &mut store,
        &b,
        "same-container",
        b"From: Second <second@example.test>\r\nSubject: Isolated\r\n\r\nSecond body",
        "other-member",
    );
    assert_ne!(draft_id(&store, &b, "same-container"), id);
    for table in [
        "messages",
        "conversations",
        "message_fts",
        "capture_operations",
        "operations",
    ] {
        assert_eq!(count(&store, table), 0, "{table}");
    }
    let stored = store
        .db
        .query_row(
            "SELECT metadata_json FROM google_draft_sources WHERE account=?1",
            [&a.account],
            |r| r.get::<_, String>(0),
        )
        .unwrap();
    let source: Source = serde_json::from_str(&stored).unwrap();
    let manifest = std::fs::read(
        store
            .profile
            .join("blobs")
            .join(source.part_manifest_sha256.unwrap()),
    )
    .unwrap();
    let parts: Vec<Part> = serde_json::from_slice(&manifest).unwrap();
    assert_eq!(parts[0].content_id.as_deref(), Some("picture"));
    assert_eq!(parts[0].disposition.as_deref(), Some("inline"));
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    assert_eq!(draft(&mut store, &a, &id), got);
}

#[test]
fn latest_preview_history_empty_scan_and_missing_observation_are_distinct() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, a, _) = setup(dir.path());
    import(&mut store, &a, "remote", RAW, "member1");
    let id = draft_id(&store, &a, "remote");
    let first = draft(&mut store, &a, &id);
    store
        .db
        .execute(
            "UPDATE google_draft_sources SET last_observed_at=1 WHERE account=?1",
            [&a.account],
        )
        .unwrap();
    import(&mut store, &a, "remote", RAW, "member1");
    let same = draft(&mut store, &a, &id);
    assert_eq!(same["revision"], first["revision"]);
    assert_eq!(
        same["remote_source"]["source_revision"],
        first["remote_source"]["source_revision"]
    );
    assert_eq!(
        same["remote_source"]["observed_at"],
        first["remote_source"]["observed_at"]
    );
    assert!(same["remote_source"]["last_observed_at"].as_i64().unwrap() > 1);
    assert_eq!(same["remote_source"]["seen_generation"], 2);
    let changed = String::from_utf8(RAW.to_vec())
        .unwrap()
        .replace("Original remote text", "New remote text")
        .into_bytes();
    import(&mut store, &a, "remote", &changed, "member2");
    let latest = draft(&mut store, &a, &id);
    assert!(latest["body"].as_str().unwrap().contains("New remote text"));
    assert!(
        latest["remote_source"]["source_revision"].as_i64().unwrap()
            > first["remote_source"]["source_revision"].as_i64().unwrap()
    );
    assert_eq!(
        raw_export(&store, &a, &id, &first["remote_source"]["source_revision"]),
        RAW
    );
    store.google_draft_inventory_begin(&a).unwrap();
    let status = page(&mut store, &a, &[], None);
    assert_eq!(status["enumeration_complete"], true);
    assert_eq!(status["observed_this_scan"], 0);
    assert_eq!(status["observed_items"], 1);
    assert_eq!(status["source_retained"], 1);
    let unseen = draft(&mut store, &a, &id);
    assert_eq!(unseen["remote_source"]["seen_in_latest_scan"], false);
    assert_eq!(unseen["body"], latest["body"]);
    assert_eq!(unseen["remote_source"]["absence_means_sent"], false);
    store.google_draft_inventory_begin(&a).unwrap();
    page(&mut store, &a, &["remote"], None);
    let proof = prepare_google_draft_import(
        fetch(&store, &a),
        GoogleDraftImportOutcome::Missing,
        &CancellationToken::new(),
    )
    .unwrap();
    let status = store.google_draft_import_finish(proof).unwrap();
    assert_eq!(status["observed_this_scan"], 1);
    assert_eq!(status["source_retained"], 0);
    assert_eq!(status["source_unavailable"], 1);
    let missing = draft(&mut store, &a, &id);
    assert_eq!(missing["body"], latest["body"]);
    assert_eq!(missing["remote_source"]["source_state"], "missing");
    assert_eq!(missing["remote_source"]["preview_state"], "unavailable");
    assert_eq!(
        code(store.google_draft_source_begin(
            &a.account,
            &json!({"draft_id":id,"source_revision":missing["remote_source"]["source_revision"]})
        )),
        "content_unavailable"
    );
    assert_eq!(code(store.google_draft_source_begin(&a.account,&json!({"draft_id":id,"source_revision":missing["remote_source"]["source_revision"],"attachment_id":latest["attachment_ids"][0]}))),"not_found");
    assert_eq!(
        raw_export(&store, &a, &id, &latest["remote_source"]["source_revision"]),
        changed
    );
    let history = store
        .google_draft_source_history(&a.account, &json!({"draft_id":id,"limit":1}))
        .unwrap();
    assert_eq!(history["total"], 3);
    assert_eq!(history["next_offset"], 1);
}

#[test]
fn capable_account_cannot_mutate_upload_or_send_read_only_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, a, _) = setup(dir.path());
    import(&mut store, &a, "remote", RAW, "member");
    let id = draft_id(&store, &a, "remote");
    let original = draft(&mut store, &a, &id);
    let revision = original["revision"].clone();
    for method in ["draft_save", "draft_discard", "draft_attachment_import"] {
        let mut p = json!({"draft_id":id,"expected_revision":revision,"request_id":method});
        if method == "draft_save" {
            p["body"] = json!("must not replace preview");
        }
        if method == "draft_attachment_import" {
            p["path"] = json!(dir.path().join("missing-never-opened"));
        }
        assert_eq!(
            code(store.handle(method, Some(&a.account), p)),
            "draft_read_only",
            "{method}"
        );
    }
    let request = json!({"draft_id":id,"expected_revision":revision,"request_id":"upload"});
    assert_eq!(
        code(store.google_draft_intent(&scope(&a), &request)),
        "draft_read_only"
    );
    assert_eq!(
        code(store.google_draft_begin(&scope(&a), &request)),
        "draft_read_only"
    );
    assert_eq!(
        code(store.google_draft_prepare_failed(&scope(&a), &request, 0, "invalid_draft")),
        "draft_read_only"
    );
    assert_eq!(
        code(store.google_draft_defer(&scope(&a), &request, 0, "content_unavailable", 5)),
        "draft_read_only"
    );
    assert_eq!(
        code(store.google_draft_observe_begin(&scope(&a), &id)),
        "draft_read_only"
    );
    let send = json!({"draft_id":id,"expected_revision":revision,"expected_google_revision":0,"request_id":"send"});
    assert_eq!(
        code(store.google_send_intent(&scope(&a), &send)),
        "draft_read_only"
    );
    assert_eq!(
        code(store.google_send_begin(&scope(&a), &send)),
        "draft_read_only"
    );
    assert_eq!(
        code(store.handle(
            "draft_capture",
            Some(&a.account),
            json!({"draft_id":id,"expected_revision":revision,"request_id":"capture"})
        )),
        "capability_unavailable"
    );
    assert!(store
        .google_draft_due(&a.account, a.connection_generation, 10)
        .unwrap()
        .is_empty());
    assert_eq!(count(&store, "operations"), 0);
    assert_eq!(count(&store, "capture_operations"), 0);
    assert_eq!(draft(&mut store, &a, &id), original);
}

#[test]
fn page_checkpoint_cancel_generation_and_expiry_are_bounded_and_restartable() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, a, _) = setup(dir.path());
    store.google_draft_inventory_begin(&a).unwrap();
    page(&mut store, &a, &["first", "second"], Some("next-page"));
    let cancel = CancellationToken::new();
    let plan = fetch(&store, &a);
    cancel.cancel();
    assert_eq!(
        code(prepare_google_draft_import(
            plan,
            GoogleDraftImportOutcome::Missing,
            &cancel
        )),
        "cancelled"
    );
    assert_eq!(count(&store, "drafts"), 0);
    let first = available(fetch(&store, &a), RAW, "one");
    store.google_draft_import_finish(first).unwrap();
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    store.google_draft_inventory_begin(&a).unwrap();
    assert_eq!(fetch(&store, &a).provider_draft_id(), "second");
    let late = available(fetch(&store, &a), RAW, "two");
    store
        .set_google_connection_state(
            &a.account,
            crate::live_profile::ConnectionState::Disconnected,
        )
        .unwrap();
    assert_eq!(
        code(store.google_draft_import_finish(late)),
        "stale_google_work"
    );
    assert_eq!(count(&store, "drafts"), 1);
    let grants = BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]);
    store
        .bind_google_account(
            "inventory-a",
            "author@example.test",
            &grants,
            Some(&a.account),
        )
        .unwrap();
    let mut current = a.clone();
    current.connection_generation = store
        .google_account_bindings()
        .unwrap()
        .into_iter()
        .find(|b| b.account == a.account)
        .unwrap()
        .connection_generation;
    store.google_draft_inventory_begin(&current).unwrap();
    let second = available(fetch(&store, &current), RAW, "two");
    store.google_draft_import_finish(second).unwrap();
    for attempt in 0..4 {
        let GoogleDraftInventoryStep::List(plan) =
            store.google_draft_inventory_next(&current).unwrap()
        else {
            panic!()
        };
        assert!(plan.page_token().is_some());
        let status = store.google_draft_inventory_list_expired(plan).unwrap();
        if attempt < 3 {
            assert!(status["paused"].is_null());
            page(&mut store, &current, &[], Some("next-page"));
        } else {
            assert_eq!(status["paused"], "expired_page_token");
        }
    }
    assert_eq!(count(&store, "drafts"), 2);
    assert_eq!(count(&store, "google_draft_sources"), 2);
    assert!(matches!(
        store.google_draft_inventory_next(&current).unwrap(),
        GoogleDraftInventoryStep::Complete(_)
    ));
    assert!(store.google_draft_inventory_begin(&current).unwrap()["paused"].is_null());
}

#[test]
fn unknown_create_similarity_is_evidence_only_and_late_exact_ack_links_without_retargeting() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, a, _) = setup(dir.path());
    let unknown_id = create(&mut store, &a, "unknown-local");
    let (unknown, raw) = freeze_local(&mut store, &a, &unknown_id, "unknown-create");
    let unknown_status = store
        .google_draft_outcome(
            &unknown,
            GoogleDraftOutcome::Unknown {
                code: "transport_interrupted".into(),
            },
        )
        .unwrap();
    import(&mut store, &a, "similar-container", &raw, "similar-member");
    let snapshot = draft_id(&store, &a, "similar-container");
    assert_ne!(snapshot, unknown_id);
    let got = draft(&mut store, &a, &snapshot);
    assert_eq!(got["remote_source"]["possible_unknown_create"], true);
    assert_eq!(
        got["remote_source"]["candidate_request_ids"],
        json!(["unknown-create"])
    );
    assert!(got["remote_source"]["linked_local_draft_id"].is_null());
    assert_eq!(
        store.google_draft_status(&a.account, &unknown_id).unwrap()["operation"],
        unknown_status["operation"]
    );
    let local = create(&mut store, &a, "late-local");
    let authored = draft(&mut store, &a, &local);
    let (lease, raw) = freeze_local(&mut store, &a, &local, "late-create");
    import(&mut store, &a, "late-container", &raw, "late-member");
    let duplicate = draft_id(&store, &a, "late-container");
    assert_ne!(duplicate, local);
    let old_source = draft(&mut store, &a, &duplicate)["remote_source"]["source_revision"].clone();
    store
        .google_draft_outcome(
            &lease,
            GoogleDraftOutcome::Acknowledged {
                draft_id: "late-container".into(),
                message_id: "late-member".into(),
            },
        )
        .unwrap();
    let receipt: String = store
        .db
        .query_row(
            "SELECT json FROM capture_operations WHERE account=?1 AND id='late-create'",
            [&a.account],
            |r| r.get(0),
        )
        .unwrap();
    import(&mut store, &a, "late-container", &raw, "late-member");
    let linked = draft(&mut store, &a, &duplicate);
    assert_eq!(linked["remote_source"]["linked_local_draft_id"], local);
    assert_eq!(linked["read_only"], true);
    let owner = draft(&mut store, &a, &local);
    assert_eq!(owner["body"], authored["body"]);
    assert_eq!(owner["revision"], authored["revision"]);
    assert_eq!(raw_export(&store, &a, &local, &old_source), raw);
    let list = store.handle("drafts", Some(&a.account), json!({})).unwrap();
    assert!(!list["drafts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["draft_id"] == duplicate));
    assert_eq!(list["total"], 3);
    assert!(!store
        .db
        .query_row(
            "SELECT discarded FROM drafts WHERE account=?1 AND draft_id=?2",
            params![a.account, duplicate],
            |r| r.get::<_, bool>(0)
        )
        .unwrap());
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT json FROM capture_operations WHERE account=?1 AND id='late-create'",
                [&a.account],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        receipt
    );
}

#[test]
fn publication_rechecks_new_exact_owner_and_preserves_pending_and_accepted_send() {
    for accepted in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let (mut store, a, _) = setup(dir.path());
        let local = create(&mut store, &a, "local");
        store.handle("draft_save",Some(&a.account),json!({"draft_id":local,"expected_revision":1,"request_id":"save","body":"Author text","to_recipients":[{"email":"reader@example.test"}]})).unwrap();
        let (lease, raw) = freeze_local(&mut store, &a, &local, "sync");
        store.google_draft_inventory_begin(&a).unwrap();
        page(&mut store, &a, &["exact-container"], None);
        let prepared = available(fetch(&store, &a), RAW, "external-version");
        store
            .google_draft_outcome(
                &lease,
                GoogleDraftOutcome::Acknowledged {
                    draft_id: "exact-container".into(),
                    message_id: "synced-member".into(),
                },
            )
            .unwrap();
        let observe = store
            .google_draft_observe_begin(&scope(&a), &local)
            .unwrap();
        let proof = google_drafts::prepare_google_draft_observation(
            observe,
            Some(google_drafts::GoogleDraftRemote {
                draft_id: "exact-container".into(),
                message_id: "synced-member".into(),
                raw: raw.clone(),
            }),
            &CancellationToken::new(),
        )
        .unwrap();
        store.google_draft_observe_finish(proof).unwrap();
        store.google_draft_import_finish(prepared).unwrap();
        assert_eq!(draft_id(&store, &a, "exact-container"), local);
        assert_eq!(count(&store, "drafts"), 1);
        let authored = draft(&mut store, &a, &local);
        assert_eq!(authored["body"], "Author text");
        assert_eq!(authored["origin"], "local");
        let p = json!({"draft_id":local,"expected_revision":authored["revision"],"expected_google_revision":authored["google"]["revision"],"request_id":"submit"});
        let GoogleSendStep::Prepare(plan) = store.google_send_begin(&scope(&a), &p).unwrap() else {
            panic!()
        };
        let proof = google_send::prepare_google_send(plan, &CancellationToken::new()).unwrap();
        let frozen = store.google_send_finish(proof).unwrap();
        let send = store
            .google_send_claim(&scope(&a), "submit", frozen["revision"].as_i64().unwrap())
            .unwrap();
        import(&mut store, &a, "exact-container", RAW, "external-version");
        assert_eq!(count(&store, "drafts"), 1);
        store
            .google_send_outcome(
                &send,
                if accepted {
                    GoogleSendOutcome::Accepted {
                        message_id: "accepted-message".into(),
                        thread_id: "accepted-thread".into(),
                    }
                } else {
                    GoogleSendOutcome::Unknown {
                        code: "transport_interrupted".into(),
                    }
                },
            )
            .unwrap();
        store.handle("draft_save",Some(&a.account),json!({"draft_id":local,"expected_revision":2,"request_id":"later-save","body":"Newer retained text"})).unwrap();
        let before = draft(&mut store, &a, &local);
        import(&mut store, &a, "exact-container", RAW, "still-listed");
        let after = draft(&mut store, &a, &local);
        assert_eq!(after["body"], "Newer retained text");
        assert_eq!(after["revision"], before["revision"]);
        assert_eq!(after["google_send"], before["google_send"]);
        assert!(store
            .google_draft_due(&a.account, a.connection_generation, 10)
            .unwrap()
            .is_empty());
    }
}

#[test]
fn truncation_issues_and_large_read_cap_preserve_original_raw() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, a, _) = setup(dir.path());
    let references = (0..101)
        .map(|n| format!("<{n}@x>"))
        .collect::<Vec<_>>()
        .join(" ");
    let raw=format!("From: \"{}\" <actual@example.test>\r\nReferences: {references}\r\nSubject: Long metadata\r\n\r\nbody","a".repeat(1100)).into_bytes();
    import(&mut store, &a, "long", &raw, "long-member");
    let id = draft_id(&store, &a, "long");
    let got = draft(&mut store, &a, &id);
    for issue in ["from_preview_limit", "references_preview_limit"] {
        assert!(got["remote_source"]["issues"]
            .as_array()
            .unwrap()
            .contains(&json!(issue)));
    }
    assert_eq!(got["remote_source"]["preview_state"], "partial");
    assert_eq!(got["references"].as_array().unwrap().len(), 100);
    assert_eq!(
        raw_export(&store, &a, &id, &got["remote_source"]["source_revision"]),
        raw
    );
    let metadata=format!("From: actual@example.test\r\nContent-Type: application/{}\r\nContent-Disposition: {}; filename=kept.bin\r\nContent-Transfer-Encoding: base64\r\n\r\nAAEC/w==\r\n","x".repeat(260),"y".repeat(140)).into_bytes();
    import(&mut store, &a, "metadata", &metadata, "metadata-member");
    let id = draft_id(&store, &a, "metadata");
    let got = draft(&mut store, &a, &id);
    assert_eq!(got["remote_source"]["preview_state"], "partial");
    assert!(got["remote_source"]["issues"]
        .as_array()
        .unwrap()
        .contains(&json!("attachment_metadata_preview_limit")));
    assert_eq!(
        raw_export(&store, &a, &id, &got["remote_source"]["source_revision"]),
        metadata
    );
    let mut large = b"From: large@example.test\r\nSubject: Above Send cap\r\n\r\n".to_vec();
    large.resize(35_000_001, b'x');
    import(&mut store, &a, "large", &large, "large-member");
    let id = draft_id(&store, &a, "large");
    let got = draft(&mut store, &a, &id);
    assert_eq!(got["remote_source"]["source_state"], "available");
    assert_eq!(got["remote_source"]["size"], 35_000_001);
    assert_eq!(got["remote_source"]["sha256"], hash(&large));
    store.google_draft_inventory_begin(&a).unwrap();
    page(&mut store, &a, &["too-big"], None);
    let plan = fetch(&store, &a);
    let proof = available(
        plan,
        &vec![b'x'; MAX_IMPORT_RAW_BYTES + 1],
        "too-big-member",
    );
    store.google_draft_import_finish(proof).unwrap();
    let id = draft_id(&store, &a, "too-big");
    let got = draft(&mut store, &a, &id);
    assert_eq!(got["remote_source"]["source_state"], "unavailable");
    assert!(got["remote_source"]["sha256"].is_null());
    assert!(got["remote_source"]["issues"]
        .as_array()
        .unwrap()
        .contains(&json!("source_too_large")));
}

#[test]
fn export_validates_ids_version_membership_cancellation_and_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, a, _) = setup(dir.path());
    import(&mut store, &a, "remote", RAW, "member");
    let id = draft_id(&store, &a, "remote");
    let got = draft(&mut store, &a, &id);
    let rev = got["remote_source"]["source_revision"].clone();
    for invalid in ["a".repeat(257), "bad\nidentifier".into()] {
        assert_eq!(
            code(store.google_draft_source_history(&a.account, &json!({"draft_id":invalid}))),
            "invalid_request"
        );
        assert_eq!(
            code(store.google_draft_source_begin(
                &a.account,
                &json!({"draft_id":id,"source_revision":rev,"attachment_id":invalid})
            )),
            "invalid_request"
        );
    }
    let cancel = CancellationToken::new();
    cancel.cancel();
    let p = store
        .google_draft_source_begin(&a.account, &json!({"draft_id":id,"source_revision":rev}))
        .unwrap();
    assert_eq!(code(export_google_draft_source(p, &cancel)), "cancelled");
    std::fs::write(
        store
            .profile
            .join("blobs")
            .join(got["remote_source"]["sha256"].as_str().unwrap()),
        b"corrupt",
    )
    .unwrap();
    let p = store
        .google_draft_source_begin(&a.account, &json!({"draft_id":id,"source_revision":rev}))
        .unwrap();
    assert!(export_google_draft_source(p, &CancellationToken::new()).is_err());
}

#[test]
fn cached_page_reads_only_requested_routing_and_caps_enriched_payload() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, a, _) = setup(dir.path());
    let id = create(&mut store, &a, "visible");
    let hidden = create(&mut store, &a, "outside-page");
    store
        .db
        .execute(
            "UPDATE drafts SET updated_at=1,routing_json='{}' WHERE account=?1 AND draft_id=?2",
            params![a.account, hidden],
        )
        .unwrap();
    // Valid JSON with the wrong projection type must not be deserialized on a
    // page that does not include it. This proves paging precedes row projection.
    store
        .db
        .execute(
            "UPDATE drafts SET body=X'FF' WHERE account=?1 AND draft_id=?2",
            params![a.account, hidden],
        )
        .unwrap();
    let page = store
        .handle("drafts", Some(&a.account), json!({"limit":1}))
        .unwrap();
    assert_eq!(page["drafts"][0]["draft_id"], id);
    assert_eq!(page["total"], 2);
    assert_eq!(page["next_offset"], 1);
    assert!(store
        .handle("drafts", Some(&a.account), json!({"limit":1,"offset":1}))
        .is_err());
    store
        .db
        .execute(
            "UPDATE drafts SET body='' WHERE account=?1 AND draft_id=?2",
            params![a.account, hidden],
        )
        .unwrap();
    for i in 0..20 {
        import(&mut store, &a, &format!("remote-{i}"), RAW, "member");
    }
    let plan: Vec<String> = store
        .db
        .prepare(&format!("EXPLAIN QUERY PLAN {SOURCE_PAGE_SQL}"))
        .unwrap()
        .query_map(params![a.account, json!([id]).to_string()], |r| r.get(3))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert!(
        plan.iter().any(|line| line.contains(
            "SEARCH s USING INDEX google_draft_current_owner (account=? AND draft_id=?)"
        )),
        "{plan:?}"
    );
    assert!(plan.iter().any(|line|line.contains("SEARCH s USING INDEX google_draft_current_link (account=? AND linked_local_draft_id=?)")),"{plan:?}");
    assert!(
        !plan.iter().any(|line| line.starts_with("SCAN s ")),
        "{plan:?}"
    );
    let ids: Vec<String> = store
        .db
        .prepare("SELECT draft_id FROM drafts WHERE origin='google_snapshot'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    // Add bounded metadata to the real source records, then measure only the
    // returned JSON byte bound, never wall-clock performance.
    for id in &ids {
        store.db.execute("UPDATE google_draft_sources SET metadata_json=json_set(metadata_json,'$.issues',json_array(?3)) WHERE account=?1 AND draft_id=?2",params![a.account,id,"x".repeat(30000)]).unwrap();
    }
    let page = store
        .handle("drafts", Some(&a.account), json!({"limit":100}))
        .unwrap();
    assert!(serde_json::to_vec(&page["drafts"]).unwrap().len() <= 512 * 1024);
    assert!(page["next_offset"].is_i64());
    assert_eq!(page["total"], 22);
}

#[test]
fn schema14_upgrade_preserves_live_draft_journal_and_refuses_future_schema() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, a, _) = setup(dir.path());
    let id = create(&mut store, &a, "old-local");
    let (lease, _) = freeze_local(&mut store, &a, &id, "old-operation");
    store
        .google_draft_outcome(
            &lease,
            GoogleDraftOutcome::Unknown {
                code: "transport_interrupted".into(),
            },
        )
        .unwrap();
    let before = draft(&mut store, &a, &id);
    let journal: String = store
        .db
        .query_row(
            "SELECT json FROM capture_operations WHERE account=?1",
            [&a.account],
            |r| r.get(0),
        )
        .unwrap();
    let receipt: String = store
        .db
        .query_row(
            "SELECT result_json FROM operations WHERE account=?1",
            [&a.account],
            |r| r.get(0),
        )
        .unwrap();
    crate::test_profiles::restore_schema_fourteen(&store.db);
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    assert_eq!(store.fixture_revision().unwrap(), 0);
    assert_eq!(draft(&mut store, &a, &id), before);
    assert_eq!(count(&store, "google_draft_sources"), 0);
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT json FROM capture_operations WHERE account=?1",
                [&a.account],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        journal
    );
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT result_json FROM operations WHERE account=?1",
                [&a.account],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        receipt
    );
    assert_eq!(
        store
            .handle(
                "draft_create",
                Some(&a.account),
                json!({"mode":"new","request_id":"old-local"})
            )
            .unwrap(),
        serde_json::from_str::<Value>(&receipt).unwrap()
    );
    store
        .db
        .pragma_update(None, "user_version", crate::SCHEMA_VERSION + 1)
        .unwrap();
    drop(store);
    assert_eq!(code(Store::open_live(dir.path())), "schema_mismatch");
}

#[test]
fn schema14_mixed_google_journal_preserves_accepted_submission_and_draft_receipts() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, a, _) = setup(dir.path());
    let local = create(&mut store, &a, "draft-local");
    let (lease, _) = freeze_local(&mut store, &a, &local, "draft-request");
    store
        .google_draft_outcome(
            &lease,
            GoogleDraftOutcome::Unknown {
                code: "transport_interrupted".into(),
            },
        )
        .unwrap();
    let sent = create(&mut store, &a, "send-local");
    store.handle("draft_save",Some(&a.account),json!({"draft_id":sent,"expected_revision":1,"request_id":"send-save","body":"Already accepted","to_recipients":[{"email":"reader@example.test"}]})).unwrap();
    let request = json!({"draft_id":sent,"expected_revision":2,"expected_google_revision":0,"request_id":"send-request"});
    let GoogleSendStep::Prepare(plan) = store.google_send_begin(&scope(&a), &request).unwrap()
    else {
        panic!()
    };
    let proof = google_send::prepare_google_send(plan, &CancellationToken::new()).unwrap();
    let frozen = store.google_send_finish(proof).unwrap();
    let send = store
        .google_send_claim(
            &scope(&a),
            "send-request",
            frozen["revision"].as_i64().unwrap(),
        )
        .unwrap();
    let accepted = store
        .google_send_outcome(
            &send,
            GoogleSendOutcome::Accepted {
                message_id: "accepted".into(),
                thread_id: "thread".into(),
            },
        )
        .unwrap();
    let journals = |s: &Store| {
        s.db.prepare("SELECT id,json,blob_hash FROM capture_operations ORDER BY id")
            .unwrap()
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    let receipts = |s: &Store| {
        s.db.prepare(
            "SELECT request_id,fingerprint,result_json FROM operations ORDER BY request_id",
        )
        .unwrap()
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
    };
    let before_journals = journals(&store);
    let before_receipts = receipts(&store);
    let before = draft(&mut store, &a, &sent);
    crate::test_profiles::restore_schema_fourteen(&store.db);
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    assert_eq!(journals(&store), before_journals);
    assert_eq!(receipts(&store), before_receipts);
    assert_eq!(draft(&mut store, &a, &sent), before);
    let google_send::GoogleSendIntent::Reply(replay) =
        store.google_send_intent(&scope(&a), &request).unwrap()
    else {
        panic!()
    };
    assert_eq!(replay, accepted);
    assert_eq!(
        store.google_draft_status(&a.account, &local).unwrap()["state"],
        "acceptance_unknown"
    );
}
