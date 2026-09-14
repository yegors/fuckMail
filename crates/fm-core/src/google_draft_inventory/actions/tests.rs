use super::*;

const PLAIN:&[u8]=b"From: Author <author@example.test>\r\nTo: Reader <reader@example.test>\r\nCc: Copy <copy@example.test>\r\nBcc: Hidden <hidden@example.test>\r\nSubject: Retained subject\r\nMessage-ID: <remote@example.test>\r\nReferences: <parent@example.test>\r\nIn-Reply-To: <parent@example.test>\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nFull original text.\r\n";
fn setup(path: &std::path::Path, raw: &[u8]) -> (Store, GoogleDraftScope, Value) {
    let mut store = Store::open_live(path).unwrap();
    let account = store
        .bind_google_account(
            "actions",
            "author@example.test",
            &BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]),
            None,
        )
        .unwrap();
    let generation = store.google_account_bindings().unwrap()[0].connection_generation;
    let read = GoogleDraftReadScope {
        account: account.clone(),
        connection_generation: generation,
    };
    store.google_draft_inventory_begin(&read).unwrap();
    let GoogleDraftInventoryStep::List(plan) = store.google_draft_inventory_next(&read).unwrap()
    else {
        panic!()
    };
    store
        .google_draft_inventory_list_finish(
            plan,
            GoogleDraftInventoryPage {
                drafts: vec![GoogleDraftListedId {
                    draft_id: "remote".into(),
                    message_id: "member".into(),
                }],
                next_page_token: None,
                result_size_estimate: Some(1),
            },
        )
        .unwrap();
    let GoogleDraftInventoryStep::Fetch(plan) = store.google_draft_inventory_next(&read).unwrap()
    else {
        panic!()
    };
    let proof = prepare_google_draft_import(
        plan,
        GoogleDraftImportOutcome::Available {
            draft_id: "remote".into(),
            message_id: "member".into(),
            thread_id: "thread".into(),
            raw: raw.into(),
        },
        &CancellationToken::default(),
    )
    .unwrap();
    store.google_draft_import_finish(proof).unwrap();
    let drafts = store.handle("drafts", Some(&account), json!({})).unwrap();
    let id = drafts["drafts"][0]["draft_id"].as_str().unwrap();
    let p = json!({"draft_id":id,"source_revision":1});
    (
        store,
        GoogleDraftScope {
            account,
            connection_generation: generation,
            primary_email: "author@example.test".into(),
        },
        p,
    )
}
fn remote(raw: &[u8]) -> GoogleDraftActionRemote {
    GoogleDraftActionRemote {
        draft_id: "remote".into(),
        message_id: "member".into(),
        thread_id: "thread".into(),
        raw: raw.into(),
    }
}
fn request(p: &Value, id: &str) -> Value {
    let mut p = p.clone();
    p["request_id"] = json!(id);
    p
}
fn adopt(store: &mut Store, scope: &GoogleDraftScope, p: &Value) -> Value {
    let GoogleDraftAuthoringStep::Prepare(plan) = store.google_draft_adopt_begin(scope, p).unwrap()
    else {
        panic!()
    };
    let proof =
        prepare_google_draft_authoring(plan, Some(remote(PLAIN)), &CancellationToken::default())
            .unwrap();
    store.google_draft_authoring_finish(proof).unwrap()
}
fn delete_scope(s: &GoogleDraftScope) -> GoogleDraftDeleteScope {
    GoogleDraftDeleteScope {
        account: s.account.clone(),
        connection_generation: s.connection_generation,
    }
}
fn prepare_delete(store: &mut Store, scope: &GoogleDraftScope, p: &Value) -> Value {
    let GoogleDraftDeleteStep::Prepare(plan) = store
        .google_draft_delete_begin(&delete_scope(scope), p)
        .unwrap()
    else {
        panic!()
    };
    let proof =
        prepare_google_draft_delete(plan, remote(PLAIN), &CancellationToken::default()).unwrap();
    store.google_draft_delete_finish(proof).unwrap()
}
fn error<T>(r: Result<T>) -> String {
    match r {
        Err(e) => e.code,
        Ok(_) => panic!("expected refusal"),
    }
}

#[test]
fn adoption_preserves_source_no_upload_and_uses_same_target_for_first_edit_and_send() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, s, p) = setup(dir.path(), PLAIN);
    let draft = adopt(&mut store, &s, &request(&p, "adopt"));
    assert_eq!(draft["google"]["state"], "adopted");
    assert!(draft["google"]["baseline"].is_null());
    assert_eq!(
        draft["google"]["effective_target"]["provider_draft_id"],
        "remote"
    );
    assert_eq!(draft["signature_text"], "");
    assert!(store
        .google_draft_due(&s.account, s.connection_generation, 10)
        .unwrap()
        .is_empty());
    let id = draft["draft_id"].as_str().unwrap();
    let send = json!({"draft_id":id,"expected_revision":1,"expected_google_revision":1,"request_id":"send"});
    assert!(
        matches!(store.google_send_intent(&s,&send).unwrap(),crate::google_send::GoogleSendIntent::Submit{provider_draft_id:Some(ref v)} if v=="remote")
    );
    let unchanged = store
        .handle(
            "draft_save",
            Some(&s.account),
            json!({"draft_id":id,"expected_revision":1,"request_id":"noop","body":draft["body"]}),
        )
        .unwrap();
    assert_eq!(unchanged["revision"], 2);
    assert!(store
        .google_draft_due(&s.account, s.connection_generation, 10)
        .unwrap()
        .is_empty());
    store.handle("draft_save",Some(&s.account),json!({"draft_id":id,"expected_revision":2,"request_id":"edit","body":"New authored words"})).unwrap();
    assert_eq!(
        store
            .google_draft_due(&s.account, s.connection_generation, 10)
            .unwrap()[0]
            .draft_id,
        id
    );
    assert!(
        matches!(store.google_draft_intent(&s,&json!({"draft_id":id,"expected_revision":3,"request_id":"upload"})).unwrap(),google_drafts::GoogleDraftIntent::Upload{provider_draft_id:Some(ref v)} if v=="remote")
    );
    let reopened = adopt(&mut store, &s, &request(&p, "reopen"));
    assert_eq!(reopened["draft_id"], id);
    assert_eq!(reopened["body"], "New authored words");
    let source = store
        .google_draft_source_history(&s.account, &json!({"draft_id":id}))
        .unwrap();
    assert_eq!(source["sources"][0]["sha256"], hash(PLAIN));
}

#[test]
fn delete_prepared_resume_claim_unknown_restart_and_source_evidence_never_infer_deletion() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, s, p) = setup(dir.path(), PLAIN);
    let req = request(&p, "delete");
    let first = prepare_delete(&mut store, &s, &req);
    let resumed = prepare_delete(&mut store, &s, &req);
    assert!(resumed["revision"].as_i64() > first["revision"].as_i64());
    let lease = store
        .google_draft_delete_claim(
            &delete_scope(&s),
            "delete",
            resumed["revision"].as_i64().unwrap(),
        )
        .unwrap();
    let result = store
        .google_draft_delete_outcome(
            &lease,
            GoogleDraftDeleteOutcome::Unknown {
                code: "transport_interrupted".into(),
            },
        )
        .unwrap();
    assert_eq!(result["state"], "acceptance_unknown");
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    let plan = store
        .google_draft_delete_reconcile_begin(&delete_scope(&s), "delete")
        .unwrap();
    let proof =
        prepare_google_draft_delete_observation(plan, None, &CancellationToken::default()).unwrap();
    let evidence = store.google_draft_delete_reconcile_finish(proof).unwrap();
    assert_eq!(evidence["state"], "acceptance_unknown");
    assert_eq!(evidence["read_evidence"]["draft_exists"], false);
    assert_eq!(evidence["read_evidence"]["deletion_proven"], false);
    let history = store
        .google_draft_source_history(&s.account, &p)
        .unwrap_err();
    assert_eq!(history.code, "invalid_request");
    let history = store
        .google_draft_source_history(&s.account, &json!({"draft_id":p["draft_id"]}))
        .unwrap();
    assert_eq!(history["total"], 2);
    assert_eq!(history["delete_operations"][0]["request_id"], "delete");
    assert_eq!(
        error(store.google_draft_delete_abandon(
            &s.account,
            &json!({"request_id":"delete","expected_revision":evidence["revision"]})
        )),
        "stale_google_work"
    );
}

#[test]
fn fresh_divergence_is_retained_and_cannot_adopt_or_delete() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, s, p) = setup(dir.path(), PLAIN);
    let GoogleDraftAuthoringStep::Prepare(plan) = store
        .google_draft_adopt_begin(&s, &request(&p, "adopt"))
        .unwrap()
    else {
        panic!()
    };
    let changed = [PLAIN, b"remote edit"].concat();
    let proof =
        prepare_google_draft_authoring(plan, Some(remote(&changed)), &CancellationToken::default())
            .unwrap();
    assert_eq!(
        error(store.google_draft_authoring_finish(proof)),
        "stale_source_revision"
    );
    let history = store
        .google_draft_source_history(&s.account, &json!({"draft_id":p["draft_id"]}))
        .unwrap();
    assert_eq!(history["total"], 2);
    assert_eq!(history["sources"][0]["sha256"], hash(&changed));
    assert_eq!(
        store
            .db
            .query_row("SELECT COUNT(*) FROM capture_operations", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        error(store.google_draft_delete_begin(&delete_scope(&s), &request(&p, "delete"))),
        "stale_source_revision"
    );
}

#[test]
fn copy_conversion_preserves_all_text_routes_and_files_and_refuses_unsupported_adoption() {
    let raw=b"From: Other <other@example.test>\r\nTo: Reader <reader@example.test>\r\nSubject: Html original\r\nMIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=x\r\n\r\n--x\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>Full text</p>\r\n--x\r\nContent-Type: image/png; name=photo.png\r\nContent-Disposition: inline; filename=photo.png\r\nContent-ID: <photo>\r\nContent-Transfer-Encoding: base64\r\n\r\nAAEC/w==\r\n--x--\r\n";
    let dir = tempfile::tempdir().unwrap();
    let (mut store, s, p) = setup(dir.path(), raw);
    let mut copy = request(&p, "copy");
    copy["from_policy"] = json!("primary");
    copy["content_policy"] = json!("plain_text");
    copy["inline_policy"] = json!("attachments");
    copy["thread_policy"] = json!("omit");
    let GoogleDraftAuthoringStep::Prepare(plan) =
        store.google_draft_copy_begin(&s.account, &copy).unwrap()
    else {
        panic!()
    };
    let proof = prepare_google_draft_authoring(plan, None, &CancellationToken::default()).unwrap();
    assert_eq!(proof.assessment["adopt"]["allowed"], false);
    assert_eq!(proof.assessment["copy"]["allowed"], true);
    let draft = store.google_draft_authoring_finish(proof).unwrap();
    assert_eq!(draft["from"], s.primary_email);
    assert_eq!(draft["subject"], "Html original");
    assert!(draft["body"].as_str().unwrap().contains("Full text"));
    assert!(draft["body_html"].is_null());
    assert_eq!(draft["attachments"].as_array().unwrap().len(), 1);
    assert_eq!(draft["attachments"][0]["size"], 4);
    assert_eq!(draft["google"]["derivation"]["source_sha256"], hash(raw));
    let GoogleDraftAuthoringStep::Reply(replay) =
        store.google_draft_copy_begin(&s.account, &copy).unwrap()
    else {
        panic!()
    };
    assert_eq!(replay, draft);
    assert_eq!(
        store
            .google_draft_due(&s.account, s.connection_generation, 10)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn deletion_blocks_provider_work_but_preserves_new_local_text_and_requires_fresh_claim_stamp() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, s, p) = setup(dir.path(), PLAIN);
    let draft = adopt(&mut store, &s, &request(&p, "adopt"));
    let id = draft["draft_id"].as_str().unwrap();
    let prepared = prepare_delete(&mut store, &s, &request(&p, "delete"));
    let saved=store.handle("draft_save",Some(&s.account),json!({"draft_id":id,"expected_revision":1,"request_id":"edit","body":"Preserve these unsent words"})).unwrap();
    assert!(store
        .google_draft_due(&s.account, s.connection_generation, 10)
        .unwrap()
        .is_empty());
    assert_eq!(
        error(store.google_draft_intent(
            &s,
            &json!({"draft_id":id,"expected_revision":2,"request_id":"upload"})
        )),
        "google_draft_delete_pending"
    );
    assert_eq!(error(store.google_send_intent(&s,&json!({"draft_id":id,"expected_revision":2,"expected_google_revision":saved["google"]["revision"],"request_id":"send"}))),"google_draft_delete_pending");
    assert_eq!(
        error(store.google_draft_delete_claim(
            &delete_scope(&s),
            "delete",
            prepared["revision"].as_i64().unwrap()
        )),
        "stale_source_revision"
    );
    store
        .google_draft_delete_abandon(
            &s.account,
            &json!({"request_id":"delete","expected_revision":prepared["revision"]}),
        )
        .unwrap();
    assert_eq!(
        store
            .google_draft_due(&s.account, s.connection_generation, 10)
            .unwrap()[0]
            .draft_id,
        id
    );
    let current = store
        .handle("draft_get", Some(&s.account), json!({"draft_id":id}))
        .unwrap();
    assert_eq!(current["body"], "Preserve these unsent words");
}

#[test]
fn delete_late_ack_preserves_current_edits_and_kind_collision_does_not_rewrite_receipts() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, s, p) = setup(dir.path(), PLAIN);
    let draft = adopt(&mut store, &s, &request(&p, "adopt"));
    let id = draft["draft_id"].as_str().unwrap();
    let prepared = prepare_delete(&mut store, &s, &request(&p, "delete"));
    let lease = store
        .google_draft_delete_claim(
            &delete_scope(&s),
            "delete",
            prepared["revision"].as_i64().unwrap(),
        )
        .unwrap();
    store.handle("draft_save",Some(&s.account),json!({"draft_id":id,"expected_revision":1,"request_id":"edit","body":"Kept after remote deletion"})).unwrap();
    store
        .set_google_connection_state(
            &s.account,
            crate::live_profile::ConnectionState::Disconnected,
        )
        .unwrap();
    let receipt = store
        .google_draft_delete_outcome(&lease, GoogleDraftDeleteOutcome::Deleted)
        .unwrap();
    assert_eq!(receipt["state"], "deleted");
    assert_eq!(
        store
            .google_draft_delete_outcome(&lease, GoogleDraftDeleteOutcome::Deleted)
            .unwrap(),
        receipt
    );
    let current = store
        .handle("draft_get", Some(&s.account), json!({"draft_id":id}))
        .unwrap();
    assert_eq!(current["body"], "Kept after remote deletion");
    assert_eq!(current["google_delete"]["state"], "deleted");
    assert_eq!(
        error(google_drafts::check_google_operation_kind(
            &store.db, &s.account, "delete", "send"
        )),
        "request_id_conflict"
    );
    assert_eq!(
        error(google_drafts::check_google_operation_kind(
            &store.db, &s.account, "delete", "draft"
        )),
        "request_id_conflict"
    );
    assert_eq!(
        store
            .google_draft_delete_status(&s.account, "delete")
            .unwrap(),
        receipt
    );
}

#[test]
fn prepared_proofs_are_cancelled_or_fenced_without_authored_commit() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, s, p) = setup(dir.path(), PLAIN);
    let cancel = CancellationToken::default();
    let GoogleDraftDeleteStep::Prepare(plan) = store
        .google_draft_delete_begin(&delete_scope(&s), &request(&p, "cancel"))
        .unwrap()
    else {
        panic!()
    };
    let proof = prepare_google_draft_delete(plan, remote(PLAIN), &cancel).unwrap();
    cancel.cancel();
    assert_eq!(error(store.google_draft_delete_finish(proof)), "cancelled");
    assert_eq!(
        error(store.google_draft_delete_status(&s.account, "cancel")),
        "not_found"
    );
    let GoogleDraftAuthoringStep::Prepare(plan) = store
        .google_draft_adopt_begin(&s, &request(&p, "adopt"))
        .unwrap()
    else {
        panic!()
    };
    let proof =
        prepare_google_draft_authoring(plan, Some(remote(PLAIN)), &CancellationToken::default())
            .unwrap();
    store
        .set_google_connection_state(
            &s.account,
            crate::live_profile::ConnectionState::Disconnected,
        )
        .unwrap();
    assert_eq!(
        error(store.google_draft_authoring_finish(proof)),
        "stale_google_work"
    );
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT COUNT(*) FROM drafts WHERE origin='local'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn read_only_reconcile_retains_unknown_outcome_after_write_permission_is_removed() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, s, p) = setup(dir.path(), PLAIN);
    let status = prepare_delete(&mut store, &s, &request(&p, "delete"));
    let lease = store
        .google_draft_delete_claim(
            &delete_scope(&s),
            "delete",
            status["revision"].as_i64().unwrap(),
        )
        .unwrap();
    store
        .google_draft_delete_outcome(
            &lease,
            GoogleDraftDeleteOutcome::Unknown {
                code: "transport_interrupted".into(),
            },
        )
        .unwrap();
    store
        .bind_google_account(
            "actions",
            "author@example.test",
            &BTreeSet::from(["https://www.googleapis.com/auth/gmail.readonly".into()]),
            Some(&s.account),
        )
        .unwrap();
    let scope = GoogleDraftDeleteScope {
        account: s.account.clone(),
        connection_generation: store.google_account_bindings().unwrap()[0].connection_generation,
    };
    let plan = store
        .google_draft_delete_reconcile_begin(&scope, "delete")
        .unwrap();
    let proof =
        prepare_google_draft_delete_observation(plan, None, &CancellationToken::default()).unwrap();
    let status = store.google_draft_delete_reconcile_finish(proof).unwrap();
    assert_eq!(status["state"], "acceptance_unknown");
    assert_eq!(status["read_evidence"]["draft_exists"], false);
    assert_eq!(
        error(store.google_draft_delete_claim(
            &scope,
            "delete",
            status["revision"].as_i64().unwrap()
        )),
        "stale_google_work"
    );
    let preserved = store.google_draft_source_begin(&s.account, &p).unwrap();
    let exported = export_google_draft_source(preserved, &CancellationToken::default()).unwrap();
    assert_eq!(
        std::fs::read(exported["path"].as_str().unwrap()).unwrap(),
        PLAIN
    );
}

#[test]
fn invocation_proof_distinguishes_early_refusal_from_an_attempted_authored_commit() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, s, p) = setup(dir.path(), PLAIN);
    let GoogleDraftAuthoringStep::Prepare(plan) = store
        .google_draft_adopt_begin(&s, &request(&p, "stale"))
        .unwrap()
    else {
        panic!()
    };
    let changed = [PLAIN, b"remote edit"].concat();
    let proof =
        prepare_google_draft_authoring(plan, Some(remote(&changed)), &CancellationToken::default())
            .unwrap();
    let failure = store.google_draft_authoring_finish(proof).unwrap_err();
    assert_eq!(failure.code, "stale_source_revision");
    assert_eq!(
        failure.details["draft_authoring_invocation_finished_without_commit"],
        true
    );
    let mut copy = request(&p, "copy");
    for (k, v) in [
        ("from_policy", "primary"),
        ("content_policy", "plain_text"),
        ("inline_policy", "attachments"),
        ("thread_policy", "omit"),
    ] {
        copy[k] = json!(v);
    }
    let GoogleDraftAuthoringStep::Prepare(plan) =
        store.google_draft_copy_begin(&s.account, &copy).unwrap()
    else {
        panic!()
    };
    let proof = prepare_google_draft_authoring(plan, None, &CancellationToken::default()).unwrap();
    // Reject the actual COMMIT, after authored rows were written. Its error
    // remains conservative even though SQLite rolls back this test transaction.
    store.db.commit_hook(Some(|| true));
    let failure = store.google_draft_authoring_finish(proof).unwrap_err();
    assert_ne!(
        failure.details["draft_authoring_invocation_finished_without_commit"],
        true
    );
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT COUNT(*) FROM drafts WHERE origin='local'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn full_mime_eligibility_refuses_special_routing_partial_content_and_honors_bounds() {
    let cases = [
        (
            "Reply-To: elsewhere@example.test\r\n",
            "unsupported_routing",
        ),
        (
            "Resent-From: elsewhere@example.test\r\n",
            "unsupported_routing",
        ),
        ("To: extra@example.test\r\n", "duplicate_routing"),
    ];
    for (header, reason) in cases {
        let dir = tempfile::tempdir().unwrap();
        let raw = [header.as_bytes(), PLAIN].concat();
        let (store, s, p) = setup(dir.path(), &raw);
        let GoogleDraftAuthoringStep::Prepare(plan) =
            store.google_draft_authoring_begin(&s.account, &p).unwrap()
        else {
            panic!()
        };
        let proof =
            prepare_google_draft_authoring(plan, None, &CancellationToken::default()).unwrap();
        assert_eq!(proof.assessment["copy"]["allowed"], false);
        assert!(proof.assessment["copy"]["reasons"]
            .as_array()
            .unwrap()
            .contains(&json!(reason)));
    }
    let dir = tempfile::tempdir().unwrap();
    let raw = [PLAIN, vec![b'x'; 256 * 1024].as_slice()].concat();
    let (store, s, p) = setup(dir.path(), &raw);
    let GoogleDraftAuthoringStep::Prepare(plan) =
        store.google_draft_authoring_begin(&s.account, &p).unwrap()
    else {
        panic!()
    };
    let proof = prepare_google_draft_authoring(plan, None, &CancellationToken::default()).unwrap();
    assert!(proof.assessment["copy"]["reasons"]
        .as_array()
        .unwrap()
        .contains(&json!("body_limit")));
    let mut invalid = p.clone();
    invalid["draft_id"] = json!("x".repeat(257));
    assert_eq!(
        error(store.google_draft_authoring_begin(&s.account, &invalid)),
        "invalid_request"
    );
}

#[test]
fn adopted_observation_retains_actual_thread_and_both_raw_versions_without_replacing_editor() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, s, p) = setup(dir.path(), PLAIN);
    let draft = adopt(&mut store, &s, &request(&p, "adopt"));
    let id = draft["draft_id"].as_str().unwrap();
    store.handle("draft_save",Some(&s.account),json!({"draft_id":id,"expected_revision":1,"request_id":"edit","body":"Local text before conflict"})).unwrap();
    let plan = store.google_draft_observe_begin(&s, id).unwrap();
    let proof = google_drafts::prepare_google_draft_observation(
        plan,
        Some(google_drafts::GoogleDraftRemote {
            draft_id: "remote".into(),
            message_id: "member".into(),
            thread_id: "changed-thread".into(),
            raw: PLAIN.into(),
        }),
        &CancellationToken::default(),
    )
    .unwrap();
    let conflict = store.google_draft_observe_finish(proof).unwrap();
    assert_eq!(conflict["state"], "conflict");
    assert!(conflict["baseline"].is_null());
    assert_eq!(
        conflict["conflict"]["observed"]["thread_id"],
        "changed-thread"
    );
    let changed = [PLAIN, b"Remote words"].concat();
    let plan = store.google_draft_observe_begin(&s, id).unwrap();
    let proof = google_drafts::prepare_google_draft_observation(
        plan,
        Some(google_drafts::GoogleDraftRemote {
            draft_id: "remote".into(),
            message_id: "new-member".into(),
            thread_id: "changed-thread".into(),
            raw: changed.clone(),
        }),
        &CancellationToken::default(),
    )
    .unwrap();
    store.google_draft_observe_finish(proof).unwrap();
    let current = store
        .handle("draft_get", Some(&s.account), json!({"draft_id":id}))
        .unwrap();
    assert_eq!(current["body"], "Local text before conflict");
    assert_eq!(current["revision"], 2);
    let history = store
        .google_draft_source_history(&s.account, &json!({"draft_id":id}))
        .unwrap();
    assert_eq!(history["total"], 3);
    assert_eq!(history["sources"][0]["sha256"], hash(&changed));
    assert_eq!(history["sources"][2]["sha256"], hash(PLAIN));
    for row in history["sources"].as_array().unwrap() {
        let bytes = local_content::read_blob(
            &store.google_blob_store(),
            row["sha256"].as_str().unwrap(),
            row["size"].as_u64().unwrap(),
        )
        .unwrap();
        assert_eq!(hash(&bytes), row["sha256"]);
    }
    let GoogleDraftAuthoringStep::Prepare(plan) = store
        .google_draft_adopt_begin(
            &s,
            &request(
                &json!({"draft_id":p["draft_id"],"source_revision":3}),
                "replace",
            ),
        )
        .unwrap()
    else {
        panic!()
    };
    let proof = prepare_google_draft_authoring(
        plan,
        Some(GoogleDraftActionRemote {
            draft_id: "remote".into(),
            message_id: "new-member".into(),
            thread_id: "changed-thread".into(),
            raw: changed,
        }),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(
        error(store.google_draft_authoring_finish(proof)),
        "google_draft_conflict"
    );
}
