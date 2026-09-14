use super::*;
use google_drafts::{
    prepare_google_draft, prepare_google_draft_observation, GoogleDraftOutcome, GoogleDraftRemote,
    GoogleDraftStep,
};
use std::collections::BTreeSet;

fn setup(path: &std::path::Path) -> (Store, GoogleSendScope, String) {
    let mut store = Store::open_live(path).unwrap();
    let account = store
        .bind_google_account(
            "send-owner",
            "author@example.test",
            &BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]),
            None,
        )
        .unwrap();
    let scope = GoogleSendScope {
        account: account.clone(),
        connection_generation: store.google_account_bindings().unwrap()[0].connection_generation,
        primary_email: "author@example.test".into(),
    };
    let draft = store
        .handle(
            "draft_create",
            Some(&account),
            json!({"mode":"new","request_id":"create"}),
        )
        .unwrap();
    let id = draft["draft_id"].as_str().unwrap().to_owned();
    save(&mut store, &scope, &id, 1, "original authored text");
    (store, scope, id)
}
fn save(store: &mut Store, scope: &GoogleSendScope, id: &str, revision: i64, body: &str) -> Value {
    store.handle("draft_save",Some(&scope.account),json!({"draft_id":id,"expected_revision":revision,"request_id":format!("save-{id}-{revision}"),"body":body,"subject":"Frozen submission","to_recipients":[{"name":"Reader","email":"reader@example.test"}],"cc_recipients":[{"email":"copy@example.test"}],"bcc_recipients":[{"email":"private@example.test"}],"signature_text":"Exact signature once"})).unwrap()
}
fn request(store: &Store, scope: &GoogleSendScope, id: &str, req: &str) -> Value {
    let twin = store.google_draft_status(&scope.account, id).unwrap();
    json!({"draft_id":id,"expected_revision":twin["local_revision"],"expected_google_revision":twin["revision"],"request_id":req})
}
fn plan(store: &Store, scope: &GoogleSendScope, p: &Value) -> GoogleSendPlan {
    match store.google_send_begin(scope, p).unwrap() {
        GoogleSendStep::Prepare(plan) => plan,
        _ => panic!("expected new plan"),
    }
}
fn proof(store: &Store, scope: &GoogleSendScope, p: &Value) -> PreparedGoogleSend {
    prepare_google_send(plan(store, scope, p), &CancellationToken::new()).unwrap()
}
fn freeze(store: &mut Store, scope: &GoogleSendScope, p: &Value) -> Value {
    let proof = proof(store, scope, p);
    store.google_send_finish(proof).unwrap()
}
fn claim(store: &mut Store, scope: &GoogleSendScope, status: &Value) -> GoogleSendLease {
    store
        .google_send_claim(
            scope,
            status["request_id"].as_str().unwrap(),
            status["revision"].as_i64().unwrap(),
        )
        .unwrap()
}
fn error<T>(r: Result<T>) -> String {
    match r {
        Err(e) => e.code,
        Ok(_) => panic!("expected error"),
    }
}
fn accepted(store: &mut Store, lease: &GoogleSendLease, id: &str) -> Value {
    store
        .google_send_outcome(
            lease,
            GoogleSendOutcome::Accepted {
                message_id: id.into(),
                thread_id: format!("thread-{id}"),
            },
        )
        .unwrap()
}
fn body(store: &mut Store, scope: &GoogleSendScope, id: &str) -> Value {
    store
        .handle("draft_get", Some(&scope.account), json!({"draft_id":id}))
        .unwrap()
}
fn empty_evidence() -> GoogleSendEvidence {
    GoogleSendEvidence {
        query: "sent_message_id".into(),
        draft_observation: "not_checked".into(),
        truncated: false,
        matches: vec![],
        acknowledged_message: None,
        missing_ids: vec![],
    }
}
fn sync(store: &mut Store, scope: &GoogleSendScope, id: &str) -> Vec<u8> {
    let rev = store.google_draft_status(&scope.account, id).unwrap()["local_revision"].clone();
    let p = json!({"draft_id":id,"expected_revision":rev,"request_id":format!("sync-{id}")});
    let plan = match store.google_draft_begin(scope, &p).unwrap() {
        GoogleDraftStep::Prepare(p) => p,
        _ => panic!(),
    };
    let proof = prepare_google_draft(plan, &CancellationToken::new()).unwrap();
    let prepared = store.google_draft_finish(proof).unwrap();
    let lease = store
        .google_draft_claim(
            scope,
            p["request_id"].as_str().unwrap(),
            prepared["operation"]["revision"].as_i64().unwrap(),
        )
        .unwrap();
    let raw =
        google_drafts::load_google_draft_upload(lease.take_upload(), &CancellationToken::new())
            .unwrap();
    store
        .google_draft_outcome(
            &lease,
            GoogleDraftOutcome::Acknowledged {
                draft_id: "remote-draft".into(),
                message_id: "remote-member".into(),
            },
        )
        .unwrap();
    observe(store, scope, id, &raw, "remote-member");
    raw
}
fn observe(
    store: &mut Store,
    scope: &GoogleSendScope,
    id: &str,
    raw: &[u8],
    member: &str,
) -> Value {
    let p = store.google_draft_observe_begin(scope, id).unwrap();
    let proof = prepare_google_draft_observation(
        p,
        Some(GoogleDraftRemote {
            thread_id: "provider-thread".into(),
            draft_id: "remote-draft".into(),
            message_id: member.into(),
            raw: raw.to_vec(),
        }),
        &CancellationToken::new(),
    )
    .unwrap();
    store.google_draft_observe_finish(proof).unwrap()
}

#[test]
fn strict_frozen_mime_attachment_acceptance_and_newer_explicit_send_preserve_work() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let bytes = b"\x00exact local attachment\r\n\xff";
    let path = dir.path().join("receipt.bin");
    std::fs::write(&path, bytes).unwrap();
    store
        .handle(
            "draft_attachment_import",
            Some(&scope.account),
            json!({"draft_id":id,"expected_revision":2,"request_id":"import","path":path}),
        )
        .unwrap();
    let p = request(&store, &scope, &id, "send-one");
    let prepared = freeze(&mut store, &scope, &p);
    assert_eq!(prepared["state"], "prepared");
    assert_eq!(prepared["provider_draft_id"], Value::Null);
    assert!(store
        .google_draft_due(&scope.account, scope.connection_generation, 10)
        .unwrap()
        .is_empty());
    let lease = claim(&mut store, &scope, &prepared);
    let raw = load_google_send_upload(lease.take_upload(), &CancellationToken::new()).unwrap();
    assert_eq!(format!("{:x}", Sha256::digest(&raw)), lease.upload_sha256());
    let message = mail_parser::MessageParser::default().parse(&raw).unwrap();
    assert_eq!(
        message.message_id().unwrap(),
        lease.message_id().trim_matches(['<', '>'])
    );
    assert_eq!(message.subject(), Some("Frozen submission"));
    let text = message.body_text(0).unwrap();
    assert_eq!(text.matches("Exact signature once").count(), 1);
    assert!(text.contains("original authored text"));
    assert_eq!(message.attachments().count(), 1);
    assert_eq!(message.attachment(0).unwrap().contents(), bytes);
    for address in [
        "reader@example.test",
        "copy@example.test",
        "private@example.test",
    ] {
        assert!(String::from_utf8_lossy(&raw).contains(address));
    }
    let newer = save(
        &mut store,
        &scope,
        &id,
        3,
        "newer authored text stays local",
    );
    let ack = accepted(&mut store, &lease, "accepted-original");
    assert_eq!(ack["last_accepted_draft_revision"], 3);
    assert_eq!(ack["ack"]["message_id"], "accepted-original");
    assert_eq!(body(&mut store, &scope, &id)["body"], newer["body"]);
    assert_eq!(body(&mut store, &scope, &id)["google_send"], ack);
    assert!(store
        .google_draft_due(&scope.account, scope.connection_generation, 10)
        .unwrap()
        .is_empty());
    let p2 = request(&store, &scope, &id, "send-newer");
    let prepared2 = freeze(&mut store, &scope, &p2);
    assert_ne!(prepared2["message_id"], prepared["message_id"]);
    assert_eq!(prepared2["provider_draft_id"], Value::Null);
    let lease2 = claim(&mut store, &scope, &prepared2);
    let rejected = store
        .google_send_outcome(
            &lease2,
            GoogleSendOutcome::Rejected {
                code: "google_permission_denied".into(),
            },
        )
        .unwrap();
    assert_eq!(rejected["state"], "rejected");
    assert_eq!(rejected["last_accepted_draft_revision"], 3);
    assert!(store
        .google_draft_due(&scope.account, scope.connection_generation, 10)
        .unwrap()
        .is_empty());
    let old = match store.google_send_intent(&scope, &p).unwrap() {
        GoogleSendIntent::Reply(v) => v,
        _ => panic!(),
    };
    assert_eq!(old["state"], "accepted");
    assert_eq!(old["operation_is_current"], false);
    assert_eq!(old["ack"], ack["ack"]);
    assert_eq!(
        body(&mut store, &scope, &id)["google_send"]["request_id"],
        "send-newer"
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT COUNT(*) FROM message_fts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    assert_eq!(body(&mut store, &scope, &id)["body"], newer["body"]);
    assert_eq!(
        store
            .google_send_status(&scope.account, "send-one")
            .unwrap()["ack"],
        ack["ack"]
    );
}

#[test]
fn competing_same_request_preparations_and_claims_keep_one_winner() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let p = request(&store, &scope, &id, "same-request");
    let first = proof(&store, &scope, &p);
    let second = proof(&store, &scope, &p);
    let first_hash = first.hash.clone();
    let winner = store.google_send_finish(first).unwrap();
    assert_eq!(store.google_send_finish(second).unwrap(), winner);
    assert_eq!(winner["upload_sha256"], first_hash);
    let other = request(&store, &scope, &id, "other-request");
    assert_eq!(
        error(store.google_send_begin(&scope, &other)),
        "google_send_pending"
    );
    let lease = claim(&mut store, &scope, &winner);
    assert_eq!(
        error(store.google_send_claim(&scope, "same-request", 1)),
        "stale_google_work"
    );
    assert!(
        matches!(store.google_send_begin(&scope,&p).unwrap(),GoogleSendStep::Reply(ref v) if v["state"]=="in_flight")
    );
    let ack = accepted(&mut store, &lease, "accepted-once");
    assert_eq!(accepted(&mut store, &lease, "accepted-once"), ack);
    let repeat = request(&store, &scope, &id, "new-id-same-revision");
    assert_eq!(
        error(store.google_send_begin(&scope, &repeat)),
        "google_send_consumed"
    );
    let mut changed = p;
    changed["expected_revision"] = json!(99);
    assert_eq!(
        error(store.google_send_begin(&scope, &changed)),
        "request_id_conflict"
    );
}

#[test]
fn local_and_connection_races_refuse_preparation_but_keep_exact_late_acceptance() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let p = request(&store, &scope, &id, "stale-local");
    let prepared = proof(&store, &scope, &p);
    save(
        &mut store,
        &scope,
        &id,
        2,
        "newer before freeze publication",
    );
    assert_eq!(error(store.google_send_finish(prepared)), "stale_revision");
    let p = request(&store, &scope, &id, "stale-generation");
    let prepared = proof(&store, &scope, &p);
    store
        .set_google_connection_state(
            &scope.account,
            crate::live_profile::ConnectionState::Disconnected,
        )
        .unwrap();
    assert_eq!(
        error(store.google_send_finish(prepared)),
        "stale_google_work"
    );
    store
        .bind_google_account(
            "send-owner",
            "author@example.test",
            &BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]),
            Some(&scope.account),
        )
        .unwrap();
    let fresh = GoogleSendScope {
        connection_generation: store.google_account_bindings().unwrap()[0].connection_generation,
        ..scope
    };
    let p = request(&store, &fresh, &id, "late-result");
    let ready = freeze(&mut store, &fresh, &p);
    let lease = claim(&mut store, &fresh, &ready);
    store
        .set_google_connection_state(
            &fresh.account,
            crate::live_profile::ConnectionState::Disconnected,
        )
        .unwrap();
    let before = body(&mut store, &fresh, &id);
    let ack = accepted(&mut store, &lease, "late-accepted");
    assert_eq!(ack["state"], "accepted");
    assert_eq!(body(&mut store, &fresh, &id)["body"], before["body"]);
    assert_eq!(
        body(&mut store, &fresh, &id)["revision"],
        before["revision"]
    );
}

#[test]
fn obsolete_unpublished_requests_fence_every_prepared_proof_after_save_or_observation() {
    for changed_google in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (mut store, scope, id) = setup(dir.path());
        if changed_google {
            sync(&mut store, &scope, &id);
        }
        let original = request(&store, &scope, &id, "lost-before-journal");
        let first = proof(&store, &scope, &original);
        let second = proof(&store, &scope, &original);
        let expected_code = if changed_google {
            let observed = observe(
                &mut store,
                &scope,
                &id,
                b"Changed RAW retained by a later observation",
                "remote-member",
            );
            assert_eq!(observed["state"], "conflict");
            assert!(observed["revision"].as_i64() > original["expected_google_revision"].as_i64());
            assert_eq!(observed["local_revision"], original["expected_revision"]);
            "stale_google_revision"
        } else {
            let saved = save(&mut store, &scope, &id, 2, "Newer work after a lost reply");
            assert_eq!(saved["revision"], 3);
            "stale_revision"
        };
        let after_change = body(&mut store, &scope, &id);
        let expected_details = json!({"send_request_obsolete_without_dispatch":true});
        let failure = store.google_send_intent(&scope, &original).err().unwrap();
        assert_eq!(failure.code, expected_code);
        assert_eq!(failure.details, expected_details);
        for prepared in [first, second] {
            let failure = store.google_send_finish(prepared).unwrap_err();
            assert_eq!(failure.code, expected_code);
            assert_eq!(failure.details, expected_details);
        }
        assert_eq!(body(&mut store, &scope, &id), after_change);
        assert_eq!(
            store.db.query_row(
                "SELECT COUNT(*) FROM capture_operations WHERE account=?1 AND id='lost-before-journal'",
                [&scope.account], |r| r.get::<_, i64>(0),
            ).unwrap(),
            0
        );
        let absent = store
            .google_send_status(&scope.account, "lost-before-journal")
            .unwrap_err();
        assert_eq!(absent.code, "not_found");
        assert_eq!(absent.details, Value::Null);
        let unclaimed = store
            .google_send_claim(&scope, "lost-before-journal", 1)
            .err()
            .unwrap();
        assert_eq!(unclaimed.code, "not_found");
        assert_eq!(unclaimed.details, Value::Null);

        drop(store);
        let mut store = Store::open_live(dir.path()).unwrap();
        let failure = store.google_send_begin(&scope, &original).err().unwrap();
        assert_eq!(failure.code, expected_code);
        assert_eq!(failure.details, expected_details);
        assert_eq!(body(&mut store, &scope, &id), after_change);
        assert_eq!(
            error(store.google_send_claim(&scope, "lost-before-journal", 1)),
            "not_found"
        );
    }
}

#[test]
fn absence_future_revisions_and_existing_journals_never_prove_obsolete() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let original = request(&store, &scope, &id, "original-send");
    let absent = store
        .google_send_status(&scope.account, "original-send")
        .unwrap_err();
    assert_eq!(absent.code, "not_found");
    assert_eq!(absent.details, Value::Null);
    for (field, code) in [
        ("expected_revision", "stale_revision"),
        ("expected_google_revision", "stale_google_revision"),
    ] {
        let mut future = original.clone();
        future[field] = json!(original[field].as_i64().unwrap() + 1);
        let failure = store.google_send_intent(&scope, &future).err().unwrap();
        assert_eq!(failure.code, code);
        assert_eq!(failure.details, Value::Null);
    }

    let ready = freeze(&mut store, &scope, &original);
    save(
        &mut store,
        &scope,
        &id,
        2,
        "Newer authored work beside an existing journal",
    );
    assert!(matches!(
        store.google_send_intent(&scope, &original).unwrap(),
        GoogleSendIntent::Submit { .. }
    ));
    assert!(
        matches!(store.google_send_begin(&scope, &original).unwrap(), GoogleSendStep::Reply(v) if v["state"] == "prepared")
    );
    let lease = claim(&mut store, &scope, &ready);
    assert!(
        matches!(store.google_send_intent(&scope, &original).unwrap(), GoogleSendIntent::Reply(v) if v["state"] == "in_flight")
    );
    let unknown = store
        .google_send_outcome(
            &lease,
            GoogleSendOutcome::Unknown {
                code: "transport_interrupted".into(),
            },
        )
        .unwrap();
    assert!(
        matches!(store.google_send_intent(&scope, &original).unwrap(), GoogleSendIntent::Reply(v) if v == unknown)
    );
    let mut changed = original.clone();
    changed["expected_revision"] = json!(1);
    let conflict = store.google_send_intent(&scope, &changed).err().unwrap();
    assert_eq!(conflict.code, "request_id_conflict");
    assert_eq!(conflict.details, Value::Null);

    drop(store);
    let store = Store::open_live(dir.path()).unwrap();
    assert!(
        matches!(store.google_send_intent(&scope, &original).unwrap(), GoogleSendIntent::Reply(v) if v == unknown)
    );
}

#[test]
fn prepared_resume_and_abandon_are_exact_and_never_apply_to_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let p = request(&store, &scope, &id, "resume");
    let ready = freeze(&mut store, &scope, &p);
    let original = ready["upload_sha256"].clone();
    save(
        &mut store,
        &scope,
        &id,
        2,
        "later version while known unsent",
    );
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    let resumed = match store.google_send_begin(&scope, &p).unwrap() {
        GoogleSendStep::Reply(v) => v,
        _ => panic!(),
    };
    assert_eq!(resumed["request"], p);
    assert_eq!(resumed["upload_sha256"], original);
    let lease = claim(&mut store, &scope, &resumed);
    let unsent = store
        .google_send_outcome(
            &lease,
            GoogleSendOutcome::Undispatched {
                code: "cancelled".into(),
            },
        )
        .unwrap();
    assert_eq!(unsent["state"], "prepared");
    assert_eq!(
        error(store.google_send_abandon(
            &scope.account,
            &json!({"request_id":"resume","expected_revision":1})
        )),
        "stale_google_work"
    );
    let abandoned = store
        .google_send_abandon(
            &scope.account,
            &json!({"request_id":"resume","expected_revision":unsent["revision"]}),
        )
        .unwrap();
    assert_eq!(abandoned["state"], "abandoned");
    assert_eq!(
        store
            .google_draft_due(&scope.account, scope.connection_generation, 10)
            .unwrap()
            .len(),
        1
    );
    let p2 = request(&store, &scope, &id, "unknown");
    let ready = freeze(&mut store, &scope, &p2);
    let lease = claim(&mut store, &scope, &ready);
    let unknown = store
        .google_send_outcome(
            &lease,
            GoogleSendOutcome::Unknown {
                code: "transport_interrupted".into(),
            },
        )
        .unwrap();
    assert_eq!(
        error(store.google_send_abandon(
            &scope.account,
            &json!({"request_id":"unknown","expected_revision":unknown["revision"]})
        )),
        "stale_google_work"
    );
    assert!(
        matches!(store.google_send_intent(&scope,&p2).unwrap(),GoogleSendIntent::Reply(ref v) if v["state"]=="acceptance_unknown")
    );
}

#[test]
fn synced_target_uses_frozen_mime_and_remote_divergence_prevents_claim() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let remote = sync(&mut store, &scope, &id);
    save(
        &mut store,
        &scope,
        &id,
        2,
        "local edited after twin baseline",
    );
    let p = request(&store, &scope, &id, "send-twin");
    let ready = freeze(&mut store, &scope, &p);
    assert_eq!(ready["provider_draft_id"], "remote-draft");
    assert_ne!(
        ready["upload_sha256"],
        format!("{:x}", Sha256::digest(&remote))
    );
    let before = store.google_draft_status(&scope.account, &id).unwrap();
    assert_eq!(
        observe(&mut store, &scope, &id, &remote, "remote-member"),
        before
    );
    let conflict = observe(
        &mut store,
        &scope,
        &id,
        b"Other retained remote version",
        "changed-member",
    );
    assert_eq!(conflict["state"], "conflict");
    assert_eq!(
        error(store.google_send_claim(&scope, "send-twin", 1)),
        "google_draft_conflict"
    );
    assert_eq!(
        body(&mut store, &scope, &id)["body"],
        "local edited after twin baseline"
    );
    assert_eq!(
        store
            .google_send_status(&scope.account, "send-twin")
            .unwrap()["state"],
        "prepared"
    );
}

#[test]
fn b1_and_send_preparation_race_cannot_bypass_each_other_or_unknown_draft() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let sp = request(&store, &scope, &id, "send-race");
    let sendproof = proof(&store, &scope, &sp);
    let dp = json!({"draft_id":id,"expected_revision":2,"request_id":"draft-race"});
    let dp = match store.google_draft_begin(&scope, &dp).unwrap() {
        GoogleDraftStep::Prepare(p) => p,
        _ => panic!(),
    };
    let dp = prepare_google_draft(dp, &CancellationToken::new()).unwrap();
    let draftready = store.google_draft_finish(dp).unwrap();
    assert_eq!(
        error(store.google_send_finish(sendproof)),
        "stale_google_revision"
    );
    let sp = request(&store, &scope, &id, "cannot-bypass");
    assert_eq!(
        error(store.google_send_begin(&scope, &sp)),
        "google_draft_pending"
    );
    let lease = store
        .google_draft_claim(
            &scope,
            "draft-race",
            draftready["operation"]["revision"].as_i64().unwrap(),
        )
        .unwrap();
    store
        .google_draft_outcome(
            &lease,
            GoogleDraftOutcome::Unknown {
                code: "transport_interrupted".into(),
            },
        )
        .unwrap();
    let sp = request(&store, &scope, &id, "cannot-bypass-unknown");
    assert_eq!(
        error(store.google_send_begin(&scope, &sp)),
        "google_draft_pending"
    );
    let second = store
        .handle(
            "draft_create",
            Some(&scope.account),
            json!({"mode":"new","request_id":"second"}),
        )
        .unwrap();
    let second = second["draft_id"].as_str().unwrap();
    save(&mut store, &scope, second, 1, "Other draft");
    let dp = json!({"draft_id":second,"expected_revision":2,"request_id":"losing-draft"});
    let dp = match store.google_draft_begin(&scope, &dp).unwrap() {
        GoogleDraftStep::Prepare(p) => p,
        _ => panic!(),
    };
    let dp = prepare_google_draft(dp, &CancellationToken::new()).unwrap();
    let sp = request(&store, &scope, second, "winning-send");
    freeze(&mut store, &scope, &sp);
    assert_eq!(error(store.google_draft_finish(dp)), "google_send_pending");
}

#[test]
fn restart_mixed_journal_and_execution_collisions_remain_separate() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let p = request(&store, &scope, &id, "send");
    let ready = freeze(&mut store, &scope, &p);
    claim(&mut store, &scope, &ready);
    let dp = json!({"draft_id":id,"expected_revision":2,"request_id":"send"});
    assert_eq!(
        error(store.google_draft_begin(&scope, &dp)),
        "request_id_conflict"
    );
    assert_eq!(
        error(google_drafts::check_execution_kind(
            &store.db,
            &scope.account,
            "send",
            "synthetic"
        )),
        "request_id_conflict"
    );
    let other = store
        .handle(
            "draft_create",
            Some(&scope.account),
            json!({"mode":"new","request_id":"other"}),
        )
        .unwrap();
    let other = other["draft_id"].as_str().unwrap();
    let dp = json!({"draft_id":other,"expected_revision":1,"request_id":"draft"});
    let plan = match store.google_draft_begin(&scope, &dp).unwrap() {
        GoogleDraftStep::Prepare(p) => p,
        _ => panic!(),
    };
    let proof = prepare_google_draft(plan, &CancellationToken::new()).unwrap();
    let ready = store.google_draft_finish(proof).unwrap();
    store
        .google_draft_claim(
            &scope,
            "draft",
            ready["operation"]["revision"].as_i64().unwrap(),
        )
        .unwrap();
    assert_eq!(
        error(store.google_send_status(&scope.account, "draft")),
        "request_id_conflict"
    );
    drop(store);
    let store = Store::open_live(dir.path()).unwrap();
    assert_eq!(
        store.google_send_status(&scope.account, "send").unwrap()["state"],
        "acceptance_unknown"
    );
    assert_eq!(
        store.google_draft_status(&scope.account, other).unwrap()["state"],
        "acceptance_unknown"
    );
    assert!(store
        .google_draft_due(&scope.account, scope.connection_generation, 10)
        .unwrap()
        .is_empty());
    let p2 = request(&store, &scope, &id, "new-send");
    assert_eq!(
        error(store.google_send_begin(&scope, &p2)),
        "google_send_pending"
    );
    assert_eq!(
        error(store.google_send_status("another-account", "send")),
        "not_found"
    );
}

#[test]
fn read_evidence_never_promotes_unknown_and_is_bounded_revision_and_generation_fenced() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let p = request(&store, &scope, &id, "unknown");
    let ready = freeze(&mut store, &scope, &p);
    let lease = claim(&mut store, &scope, &ready);
    assert_eq!(
        error(store.google_send_reconcile_begin(&scope, "unknown")),
        "google_send_pending"
    );
    let unknown = store
        .google_send_outcome(
            &lease,
            GoogleSendOutcome::Unknown {
                code: "lost_ack".into(),
            },
        )
        .unwrap();
    let mut evidence = empty_evidence();
    evidence.matches.push(GoogleSendObservedMessage {
        message_id: "candidate".into(),
        thread_id: "candidate-thread".into(),
        history_id: Some("110".into()),
        labels: vec!["SENT".into()],
    });
    let first = store
        .google_send_reconcile_begin(&scope, "unknown")
        .unwrap();
    let stale = store
        .google_send_reconcile_begin(&scope, "unknown")
        .unwrap();
    assert_eq!(first.message_id(), lease.message_id());
    let found = store
        .google_send_reconcile_finish(first, evidence.clone())
        .unwrap();
    assert_eq!(found["state"], "acceptance_unknown");
    assert_eq!(found["ack"], Value::Null);
    assert_eq!(found["error"], unknown["error"]);
    assert_eq!(
        error(store.google_send_reconcile_finish(stale, empty_evidence())),
        "stale_google_work"
    );
    let next = store
        .google_send_reconcile_begin(&scope, "unknown")
        .unwrap();
    let absent = store
        .google_send_reconcile_finish(next, empty_evidence())
        .unwrap();
    assert_eq!(absent["state"], "acceptance_unknown");
    let next = store
        .google_send_reconcile_begin(&scope, "unknown")
        .unwrap();
    evidence.matches = vec![evidence.matches[0].clone(); 11];
    assert_eq!(
        error(store.google_send_reconcile_finish(next, evidence)),
        "invalid_google_response"
    );
    let next = store
        .google_send_reconcile_begin(&scope, "unknown")
        .unwrap();
    store
        .set_google_connection_state(
            &scope.account,
            crate::live_profile::ConnectionState::Disconnected,
        )
        .unwrap();
    assert_eq!(
        error(store.google_send_reconcile_finish(next, empty_evidence())),
        "stale_google_work"
    );
}

#[test]
fn schema13_migration_preserves_absent_draft_kind_json_and_old_engine_refuses14() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    sync(&mut store, &scope, &id);
    let op_id = format!("sync-{id}");
    store.db.execute("UPDATE capture_operations SET json=json_remove(json,'$.kind') WHERE account=?1 AND id=?2",params![scope.account,op_id]).unwrap();
    let original: String = store
        .db
        .query_row(
            "SELECT json FROM capture_operations WHERE account=?1 AND id=?2",
            params![scope.account, op_id],
            |r| r.get(0),
        )
        .unwrap();
    let draft = body(&mut store, &scope, &id);
    let fixture = store.fixture_revision().unwrap();
    crate::test_profiles::restore_schema_fourteen(&store.db);
    store.db.execute_batch("DROP INDEX google_send_recovery; DROP INDEX google_drafts_due; PRAGMA user_version=13;").unwrap();
    let old_index = include_str!("../migration-013-google-drafts.sql")
        .split_once("CREATE INDEX google_drafts_due")
        .unwrap()
        .1;
    store
        .db
        .execute_batch(&format!("CREATE INDEX google_drafts_due{old_index}"))
        .unwrap();
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    assert_eq!(store.fixture_revision().unwrap(), fixture);
    assert_eq!(body(&mut store, &scope, &id), draft);
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT json FROM capture_operations WHERE account=?1 AND id=?2",
                params![scope.account, op_id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        original
    );
    assert_eq!(
        store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        crate::SCHEMA_VERSION
    );
    // The same admission check used by historical engines refuses a future
    // schema before migrations, identity checks or journal recovery can run.
    store
        .db
        .pragma_update(None, "user_version", crate::SCHEMA_VERSION + 1)
        .unwrap();
    drop(store);
    assert_eq!(error(Store::open_live(dir.path())), "schema_mismatch");
    let db = rusqlite::Connection::open(dir.path().join("store.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        crate::SCHEMA_VERSION + 1
    );
}

#[test]
fn strict_missing_recipients_bad_primary_and_cancel_never_publish_dispatchable_work() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    store.handle("draft_save",Some(&scope.account),json!({"draft_id":id,"expected_revision":2,"request_id":"empty-recipients","body":"No recipients","to_recipients":[],"cc_recipients":[],"bcc_recipients":[]})).unwrap();
    let p = request(&store, &scope, &id, "strict");
    let pending = plan(&store, &scope, &p);
    assert_eq!(
        error(prepare_google_send(pending, &CancellationToken::new())),
        "invalid_draft"
    );
    let bad = GoogleSendScope {
        primary_email: "other@example.test".into(),
        ..scope.clone()
    };
    assert_eq!(
        error(store.google_send_begin(&bad, &p)),
        "identity_mismatch"
    );
    save(&mut store, &scope, &id, 3, "Recipients restored");
    let p = request(&store, &scope, &id, "cancel");
    let pending = plan(&store, &scope, &p);
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert_eq!(error(prepare_google_send(pending, &cancelled)), "cancelled");
    assert_eq!(
        store
            .db
            .query_row("SELECT COUNT(*) FROM capture_operations", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn reply_reply_all_and_forward_reuse_exact_retained_source_and_threading() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, _) = setup(dir.path());
    let raw = String::from_utf8(crate::composition_tests::source("frozen-source", 0))
        .unwrap()
        .replace("YOU@WINDSCRIBE.EXAMPLE", "AUTHOR@EXAMPLE.TEST")
        .into_bytes();
    let (cid, mid) = crate::composition_tests::ingest_for_account(
        &mut store,
        &scope.account,
        "frozen-source",
        raw,
    );
    for mode in ["reply", "reply_all", "forward"] {
        let draft=store.handle("draft_create",Some(&scope.account),json!({"mode":mode,"conversation_id":cid,"source_message_id":mid,"request_id":format!("create-{mode}")})).unwrap();
        let id = draft["draft_id"].as_str().unwrap();
        let body = format!("Authored {mode}\n{}", draft["body"].as_str().unwrap());
        let mut save = json!({"draft_id":id,"expected_revision":1,"request_id":format!("save-{mode}"),"body":body,"signature_text":"Frozen source signature"});
        if mode == "forward" {
            save["to_recipients"] = json!([{"email":"forward-reader@example.test"}]);
        }
        store
            .handle("draft_save", Some(&scope.account), save)
            .unwrap();
        let p = request(&store, &scope, id, &format!("send-{mode}"));
        let ready = freeze(&mut store, &scope, &p);
        let lease = claim(&mut store, &scope, &ready);
        let bytes =
            load_google_send_upload(lease.take_upload(), &CancellationToken::new()).unwrap();
        let message = mail_parser::MessageParser::default().parse(&bytes).unwrap();
        let wire = String::from_utf8_lossy(&bytes);
        assert_eq!(
            message
                .body_text(0)
                .unwrap()
                .matches("Frozen source signature")
                .count(),
            1
        );
        assert!(!wire.contains("secret@fixture.example"));
        if mode == "forward" {
            assert_eq!(lease.thread_id(), None);
            assert!(message
                .body_text(0)
                .unwrap()
                .contains("Original retained text frozen-source"));
            assert!(!wire.contains("In-Reply-To:"));
            assert!(wire.contains("forward-reader@example.test"));
        } else {
            assert_eq!(lease.thread_id(), Some("composition-source-thread"));
            assert!(wire.contains("In-Reply-To: <frozen-source@fixture.example>"));
            assert!(wire.contains("<ancestor@fixture.example>"));
            assert!(wire.contains("reply@fixture.example"));
            if mode == "reply_all" {
                assert!(wire.contains("ada@fixture.example"));
                assert!(wire.contains("bob@fixture.example"));
            } else {
                assert!(!wire.contains("ada@fixture.example"));
            }
        }
        accepted(&mut store, &lease, &format!("accepted-{mode}"));
    }
    assert_eq!(
        store
            .db
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn provider_byte_limit_is_checked_after_real_mime_encoding_before_journal_publication() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let path = dir.path().join("large.bin");
    let bytes: Vec<u8> = (0..25 * 1024 * 1024).map(|i| (i % 256) as u8).collect();
    std::fs::write(&path, &bytes).unwrap();
    store
        .handle(
            "draft_attachment_import",
            Some(&scope.account),
            json!({"draft_id":id,"expected_revision":2,"request_id":"large-import","path":path}),
        )
        .unwrap();
    let p = request(&store, &scope, &id, "over-provider-limit");
    let plan = plan(&store, &scope, &p);
    assert_eq!(
        error(prepare_google_send(plan, &CancellationToken::new())),
        "limit_exceeded"
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT COUNT(*) FROM capture_operations", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(body(&mut store, &scope, &id)["revision"], 3);
}
