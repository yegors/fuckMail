use super::*;
use std::collections::BTreeSet;

fn setup(path: &std::path::Path) -> (Store, GoogleDraftScope, String) {
    let mut store = Store::open_live(path).unwrap();
    let account = store
        .bind_google_account(
            "draft-owner",
            "author@example.test",
            &BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]),
            None,
        )
        .unwrap();
    let generation = store.google_account_bindings().unwrap()[0].connection_generation;
    let draft = store
        .handle(
            "draft_create",
            Some(&account),
            json!({"mode":"new","request_id":"create"}),
        )
        .unwrap();
    let id = draft["draft_id"].as_str().unwrap().to_owned();
    (
        store,
        GoogleDraftScope {
            account,
            connection_generation: generation,
            primary_email: "author@example.test".into(),
        },
        id,
    )
}
fn request(id: &str, revision: i64, request: &str) -> Value {
    json!({"draft_id":id,"expected_revision":revision,"request_id":request})
}
fn plan(store: &Store, scope: &GoogleDraftScope, p: &Value) -> GoogleDraftPlan {
    match store.google_draft_begin(scope, p).unwrap() {
        GoogleDraftStep::Prepare(plan) => plan,
        GoogleDraftStep::Reply(_) => panic!("expected preparation"),
    }
}
fn freeze(store: &mut Store, scope: &GoogleDraftScope, p: &Value) -> Value {
    let plan = plan(store, scope, p);
    let proof = prepare_google_draft(plan, &CancellationToken::default()).unwrap();
    store.google_draft_finish(proof).unwrap()
}
fn claim(store: &mut Store, scope: &GoogleDraftScope, status: &Value) -> GoogleDraftLease {
    store
        .google_draft_claim(
            scope,
            status["operation"]["request_id"].as_str().unwrap(),
            status["operation"]["revision"].as_i64().unwrap(),
        )
        .unwrap()
}
fn ack(store: &mut Store, lease: &GoogleDraftLease, draft: &str, message: &str) -> Value {
    store
        .google_draft_outcome(
            lease,
            GoogleDraftOutcome::Acknowledged {
                draft_id: draft.into(),
                message_id: message.into(),
            },
        )
        .unwrap()
}
fn observe(
    store: &mut Store,
    scope: &GoogleDraftScope,
    id: &str,
    remote: Option<(&str, &str, &[u8])>,
) -> Value {
    let plan = store.google_draft_observe_begin(scope, id).unwrap();
    let raw = remote.map(|(draft, message, raw)| GoogleDraftRemote {
        thread_id: "provider-thread".into(),
        draft_id: draft.into(),
        message_id: message.into(),
        raw: raw.into(),
    });
    let proof = prepare_google_draft_observation(plan, raw, &CancellationToken::default()).unwrap();
    store.google_draft_observe_finish(proof).unwrap()
}
fn save(store: &mut Store, scope: &GoogleDraftScope, id: &str, revision: i64, body: &str) -> Value {
    store.handle("draft_save",Some(&scope.account),json!({"draft_id":id,"expected_revision":revision,"request_id":format!("save-{revision}"),"body":body})).unwrap()
}
fn raw(store: &Store, lease: &GoogleDraftLease) -> Vec<u8> {
    let bytes =
        load_google_draft_upload(lease.take_upload(), &CancellationToken::default()).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        lease.upload_sha256()
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
    bytes
}

#[test]
fn recipient_free_create_dual_hash_baseline_newer_authored_text_and_noop() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let p = request(&id, 1, "upload-one");
    let prepared = freeze(&mut store, &scope, &p);
    let lease = claim(&mut store, &scope, &prepared);
    let bytes = raw(&store, &lease);
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.contains("author@example.test"));
    assert!(!text.contains("\r\nTo:"));
    let saved = save(&mut store, &scope, &id, 1, "Newer local writing remains");
    ack(&mut store, &lease, "draft-remote", "message-remote");
    let mut normalized = bytes.clone();
    normalized.extend_from_slice(b"\r\n");
    let status = observe(
        &mut store,
        &scope,
        &id,
        Some(("draft-remote", "message-remote", &normalized)),
    );
    assert_eq!(status["state"], "synced");
    assert_eq!(status["local_revision"], 2);
    assert_eq!(status["baseline"]["draft_revision"], 1);
    assert_eq!(status["baseline"]["upload_sha256"], lease.upload_sha256());
    assert_eq!(
        status["baseline"]["observed_sha256"],
        format!("{:x}", Sha256::digest(&normalized))
    );
    assert_eq!(status["fidelity_warning"], true);
    let draft = store
        .handle("draft_get", Some(&scope.account), json!({"draft_id":id}))
        .unwrap();
    assert_eq!(draft["body"], saved["body"]);
    assert_eq!(draft["revision"], saved["revision"]);
    assert_eq!(draft["google"], status);
    assert_eq!(
        observe(
            &mut store,
            &scope,
            &id,
            Some(("draft-remote", "message-remote", &normalized))
        ),
        status
    );
    assert_eq!(
        store
            .google_draft_due(&scope.account, scope.connection_generation, 10)
            .unwrap()[0]
            .revision,
        2
    );
    let update = freeze(&mut store, &scope, &request(&id, 2, "upload-two"));
    let lease2 = claim(&mut store, &scope, &update);
    assert_eq!(lease2.provider_draft_id(), Some("draft-remote"));
    let raw2 = raw(&store, &lease2);
    ack(&mut store, &lease2, "draft-remote", "message-two");
    observe(
        &mut store,
        &scope,
        &id,
        Some(("draft-remote", "message-two", &raw2)),
    );
    assert!(store
        .google_draft_due(&scope.account, scope.connection_generation, 10)
        .unwrap()
        .is_empty());
    assert!(matches!(
        store
            .google_draft_begin(&scope, &request(&id, 2, "no-new-upload"))
            .unwrap(),
        GoogleDraftStep::Reply(_)
    ));
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM capture_operations", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn draft_gate_uses_shared_granted_mail_write_capability() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, mut scope, id) = setup(dir.path());
    for (grant, allowed) in [
        ("https://mail.google.com/", true),
        ("https://www.googleapis.com/auth/gmail.readonly", false),
    ] {
        store
            .bind_google_account(
                "draft-owner",
                "author@example.test",
                &BTreeSet::from([grant.into()]),
                Some(&scope.account),
            )
            .unwrap();
        scope.connection_generation =
            store.google_account_bindings().unwrap()[0].connection_generation;
        assert_eq!(
            store
                .google_draft_intent(&scope, &request(&id, 1, "scope-check"))
                .is_ok(),
            allowed,
            "{grant}"
        );
    }
}

#[test]
fn historical_replay_is_not_current_twin_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let original_request = request(&id, 1, "original");
    let prepared = freeze(&mut store, &scope, &original_request);
    let lease = claim(&mut store, &scope, &prepared);
    let bytes = raw(&store, &lease);
    ack(&mut store, &lease, "remote", "first-member");
    let baseline = observe(
        &mut store,
        &scope,
        &id,
        Some(("remote", "first-member", &bytes)),
    );
    save(&mut store, &scope, &id, 1, "New work");
    let newer = freeze(&mut store, &scope, &request(&id, 2, "newer"));
    let GoogleDraftStep::Reply(replay) =
        store.google_draft_begin(&scope, &original_request).unwrap()
    else {
        panic!("historical replay must not prepare")
    };
    assert_eq!(replay["operation_is_current"], false);
    assert_eq!(replay["state"], "synced");
    assert_eq!(replay["operation"], baseline["operation"]);
    assert_eq!(replay["local_revision"], 2);
    let current = store.google_draft_status(&scope.account, &id).unwrap();
    assert_eq!(current["operation_is_current"], true);
    assert_eq!(current["operation"], newer["operation"]);
    assert_eq!(current["state"], "prepared");
    assert_eq!(current["operation"]["provider_draft_id"], "remote");
    assert_eq!(
        store
            .handle("draft_get", Some(&scope.account), json!({"draft_id":id}))
            .unwrap()["google"],
        current
    );
}

#[test]
fn deterministic_failure_is_revision_bound_and_does_not_starve_later_drafts() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let other = store
        .handle(
            "draft_create",
            Some(&scope.account),
            json!({"mode":"new","request_id":"second"}),
        )
        .unwrap();
    store
        .db
        .execute("UPDATE drafts SET updated_at=0 WHERE draft_id=?1", [&id])
        .unwrap();
    let request = request(&id, 1, "first-attempt");
    assert_eq!(
        store
            .google_draft_due(&scope.account, scope.connection_generation, 1)
            .unwrap()[0]
            .draft_id,
        id
    );
    for code in [
        "cancelled",
        "storage_error",
        "content_unavailable",
        "google_permission_denied",
    ] {
        assert!(store
            .google_draft_prepare_failed(&scope, &request, 0, code)
            .is_err());
    }
    let failed = store
        .google_draft_prepare_failed(&scope, &request, 0, "limit_exceeded")
        .unwrap();
    assert_eq!(failed["state"], "failed");
    assert_eq!(failed["preparation_failure"]["draft_revision"], 1);
    assert!(failed["operation"].is_null());
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
            .google_draft_due(&scope.account, scope.connection_generation, 1)
            .unwrap()[0]
            .draft_id,
        other["draft_id"]
    );
    assert!(
        matches!(
            store.google_draft_intent(&scope, &request).unwrap(),
            GoogleDraftIntent::Upload { .. }
        ),
        "explicit same-revision retry remains possible"
    );
    save(&mut store, &scope, &id, 1, "Revised content");
    assert!(store
        .google_draft_prepare_failed(
            &scope,
            &request,
            failed["revision"].as_i64().unwrap(),
            "limit_exceeded"
        )
        .is_err());
    assert!(store
        .google_draft_due(&scope.account, scope.connection_generation, 10)
        .unwrap()
        .iter()
        .any(|row| row.draft_id == id && row.revision == 2));
    drop(store);
    let store = Store::open_live(dir.path()).unwrap();
    assert_eq!(
        store.google_draft_status(&scope.account, &id).unwrap()["preparation_failure"],
        failed["preparation_failure"]
    );
}

#[test]
fn transient_deferral_is_bounded_retryable_and_skips_only_the_same_authored_revision() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let other = store
        .handle(
            "draft_create",
            Some(&scope.account),
            json!({"mode":"new","request_id":"other"}),
        )
        .unwrap();
    store
        .db
        .execute("UPDATE drafts SET updated_at=0 WHERE draft_id=?1", [&id])
        .unwrap();
    let p = request(&id, 1, "retry");
    assert!(store
        .google_draft_defer(&scope, &p, 0, "storage_error", 301)
        .is_err());
    assert!(store
        .google_draft_defer(&scope, &p, 0, "cancelled", 30)
        .is_err());
    let deferred = store
        .google_draft_defer(&scope, &p, 0, "content_unavailable", 30)
        .unwrap();
    assert_eq!(deferred["state"], "local");
    assert!(deferred["preparation_failure"].is_null());
    let until = deferred["retry"]["until_ms"].as_i64().unwrap();
    assert_eq!(
        store
            .google_draft_due_at(&scope.account, scope.connection_generation, 1, until - 1)
            .unwrap()[0]
            .draft_id,
        other["draft_id"]
    );
    assert_eq!(
        store
            .google_draft_due_at(&scope.account, scope.connection_generation, 1, until)
            .unwrap()[0]
            .draft_id,
        id
    );
    assert!(store
        .google_draft_defer(&scope, &p, 0, "storage_error", 30)
        .is_err());
    save(
        &mut store,
        &scope,
        &id,
        1,
        "Edited while prior bytes await retry",
    );
    assert!(store
        .google_draft_due_at(&scope.account, scope.connection_generation, 10, until - 1)
        .unwrap()
        .iter()
        .any(|row| row.draft_id == id && row.revision == 2));
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
fn failed_replacement_keeps_conflict_and_definitive_rejection_requires_an_edit() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let prepared = freeze(&mut store, &scope, &request(&id, 1, "create"));
    let lease = claim(&mut store, &scope, &prepared);
    let bytes = raw(&store, &lease);
    ack(&mut store, &lease, "remote", "member");
    observe(&mut store, &scope, &id, Some(("remote", "member", &bytes)));
    let conflict = observe(
        &mut store,
        &scope,
        &id,
        Some(("remote", "new-member", b"Remote change")),
    );
    let replace = json!({"draft_id":id,"expected_revision":1,"request_id":"replace-fails","expected_google_revision":conflict["revision"],"action":"replace_local"});
    let failed = store
        .google_draft_prepare_failed(
            &scope,
            &replace,
            conflict["revision"].as_i64().unwrap(),
            "invalid_draft",
        )
        .unwrap();
    assert_eq!(failed["state"], "conflict");
    assert_eq!(failed["conflict"], conflict["conflict"]);
    assert_eq!(failed["operation"], conflict["operation"]);
    save(
        &mut store,
        &scope,
        &id,
        1,
        "Later edit still needs explicit replacement",
    );
    assert!(store
        .google_draft_due(&scope.account, scope.connection_generation, 10)
        .unwrap()
        .is_empty());
    assert!(store
        .google_draft_begin(&scope, &request(&id, 2, "automatic"))
        .is_err());
    let replacement = json!({"draft_id":id,"expected_revision":2,"request_id":"explicit","expected_google_revision":failed["revision"],"action":"replace_local"});
    let prepared = freeze(&mut store, &scope, &replacement);
    let lease = claim(&mut store, &scope, &prepared);
    let rejected = store
        .google_draft_outcome(
            &lease,
            GoogleDraftOutcome::Rejected {
                code: "google_permission_denied".into(),
            },
        )
        .unwrap();
    assert_eq!(rejected["state"], "failed");
    assert!(store
        .google_draft_due(&scope.account, scope.connection_generation, 10)
        .unwrap()
        .is_empty());
    save(
        &mut store,
        &scope,
        &id,
        2,
        "A genuinely new authored revision",
    );
    assert_eq!(
        store
            .google_draft_due(&scope.account, scope.connection_generation, 10)
            .unwrap()[0]
            .revision,
        3
    );
}

#[test]
fn prepared_resume_uses_original_params_and_checks_observed_remote_before_claim() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let first = freeze(&mut store, &scope, &request(&id, 1, "first"));
    let lease = claim(&mut store, &scope, &first);
    let raw = raw(&store, &lease);
    ack(&mut store, &lease, "remote", "member-one");
    observe(
        &mut store,
        &scope,
        &id,
        Some(("remote", "member-one", &raw)),
    );
    let changed = observe(
        &mut store,
        &scope,
        &id,
        Some(("remote", "member-two", b"Remote edited MIME")),
    );
    assert_eq!(changed["state"], "conflict");
    let resolve = json!({"draft_id":id,"expected_revision":1,"expected_google_revision":changed["revision"],"request_id":"replace","action":"replace_local"});
    let prepared = freeze(&mut store, &scope, &resolve);
    save(
        &mut store,
        &scope,
        &id,
        1,
        "Local edit while immutable replacement waits",
    );
    let due = store
        .google_draft_due(&scope.account, scope.connection_generation, 10)
        .unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].revision, 1);
    assert_eq!(due[0].request, Some(resolve.clone()));
    assert!(matches!(
        store.google_draft_intent(&scope, &resolve).unwrap(),
        GoogleDraftIntent::Upload {
            provider_draft_id: Some(_)
        }
    ));
    let unchanged = observe(
        &mut store,
        &scope,
        &id,
        Some(("remote", "member-two", b"Remote edited MIME")),
    );
    assert_eq!(unchanged["revision"], prepared["revision"]);
    assert_eq!(
        unchanged["operation"]["revision"],
        prepared["operation"]["revision"]
    );
    let conflict = observe(
        &mut store,
        &scope,
        &id,
        Some(("remote", "member-three", b"Changed again")),
    );
    assert_eq!(conflict["state"], "conflict");
    assert!(store.google_draft_claim(&scope, "replace", 1).is_err());
    assert_eq!(
        store
            .handle("draft_get", Some(&scope.account), json!({"draft_id":id}))
            .unwrap()["body"],
        "Local edit while immutable replacement waits"
    );
}

#[test]
fn changed_ack_identity_same_id_raw_change_and_missing_preserve_conflicts() {
    for (draft, message) in [
        ("other-draft", "acked-member"),
        ("acked-draft", "other-member"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (mut store, scope, id) = setup(dir.path());
        let prepared = freeze(&mut store, &scope, &request(&id, 1, "upload"));
        let lease = claim(&mut store, &scope, &prepared);
        let bytes = raw(&store, &lease);
        ack(&mut store, &lease, "acked-draft", "acked-member");
        let conflict = observe(&mut store, &scope, &id, Some((draft, message, &bytes)));
        assert_eq!(conflict["conflict"]["reason"], "ack_identity_changed");
        assert!(conflict["baseline"].is_null());
        assert_eq!(conflict["operation"]["ack"]["draft_id"], "acked-draft");
        assert_eq!(
            observe(&mut store, &scope, &id, Some((draft, message, &bytes))),
            conflict
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let prepared = freeze(&mut store, &scope, &request(&id, 1, "upload"));
    let lease = claim(&mut store, &scope, &prepared);
    let bytes = raw(&store, &lease);
    ack(&mut store, &lease, "draft", "member");
    let baseline = observe(&mut store, &scope, &id, Some(("draft", "member", &bytes)));
    let conflict = observe(
        &mut store,
        &scope,
        &id,
        Some(("draft", "member", b"Same identity changed bytes")),
    );
    assert_eq!(conflict["conflict"]["reason"], "raw_changed");
    assert_eq!(conflict["baseline"], baseline["baseline"]);
    assert!(store
        .google_draft_begin(&scope, &request(&id, 1, "silent-overwrite"))
        .is_err());
    assert!(store
        .google_draft_due(&scope.account, scope.connection_generation, 10)
        .unwrap()
        .is_empty());
    let absent = observe(&mut store, &scope, &id, None);
    assert_eq!(absent["state"], "conflict");
    assert!(absent["conflict"]["observed"].is_null());
    let stale = json!({"draft_id":id,"expected_revision":1,"expected_google_revision":conflict["revision"],"request_id":"stale-approval","action":"replace_local"});
    assert!(store.google_draft_begin(&scope, &stale).is_err());
    let approved = json!({"draft_id":id,"expected_revision":1,"expected_google_revision":absent["revision"],"request_id":"replace-missing","action":"replace_local"});
    assert!(matches!(
        store.google_draft_intent(&scope, &approved).unwrap(),
        GoogleDraftIntent::Upload {
            provider_draft_id: None
        }
    ));
}

#[test]
fn generation_and_revision_fences_do_not_publish_stale_preparation_or_raw() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, mut scope, id) = setup(dir.path());
    let p = request(&id, 1, "old-freeze");
    let proof =
        prepare_google_draft(plan(&store, &scope, &p), &CancellationToken::default()).unwrap();
    save(&mut store, &scope, &id, 1, "Kept text");
    assert_eq!(
        store.google_draft_finish(proof).unwrap_err().code,
        "stale_revision"
    );
    let p = request(&id, 2, "live-freeze");
    let prepared = freeze(&mut store, &scope, &p);
    let lease = claim(&mut store, &scope, &prepared);
    let bytes = raw(&store, &lease);
    let oldscope = scope.clone();
    store
        .set_google_connection_state(
            &scope.account,
            crate::live_profile::ConnectionState::Disconnected,
        )
        .unwrap();
    let late = ack(&mut store, &lease, "draft", "member");
    assert_eq!(late["state"], "acknowledged");
    assert!(store.google_draft_observe_begin(&oldscope, &id).is_err());
    store
        .bind_google_account(
            "draft-owner",
            "author@example.test",
            &BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]),
            Some(&scope.account),
        )
        .unwrap();
    scope.connection_generation = store.google_account_bindings().unwrap()[0].connection_generation;
    let plan = store.google_draft_observe_begin(&scope, &id).unwrap();
    let proof = prepare_google_draft_observation(
        plan,
        Some(GoogleDraftRemote {
            thread_id: "provider-thread".into(),
            draft_id: "draft".into(),
            message_id: "member".into(),
            raw: bytes.clone(),
        }),
        &CancellationToken::default(),
    )
    .unwrap();
    store
        .set_google_connection_state(
            &scope.account,
            crate::live_profile::ConnectionState::Disconnected,
        )
        .unwrap();
    assert_eq!(
        store.google_draft_observe_finish(proof).unwrap_err().code,
        "stale_google_work"
    );
    store
        .bind_google_account(
            "draft-owner",
            "author@example.test",
            &BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]),
            Some(&scope.account),
        )
        .unwrap();
    scope.connection_generation = store.google_account_bindings().unwrap()[0].connection_generation;
    let conflict = observe(&mut store, &scope, &id, Some(("draft", "member", &bytes)));
    assert_eq!(conflict["conflict"]["reason"], "connection_changed");
    assert!(conflict["baseline"].is_null());
}

#[test]
fn in_flight_restart_is_unknown_and_never_due_or_recreated() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let p = request(&id, 1, "one-attempt");
    let prepared = freeze(&mut store, &scope, &p);
    let lease = claim(&mut store, &scope, &prepared);
    let bytes = raw(&store, &lease);
    drop(store);
    let mut store = Store::open_live(dir.path()).unwrap();
    let status = store.google_draft_status(&scope.account, &id).unwrap();
    assert_eq!(status["state"], "acceptance_unknown");
    assert!(store
        .google_draft_due(&scope.account, scope.connection_generation, 10)
        .unwrap()
        .is_empty());
    assert!(matches!(
        store.google_draft_begin(&scope, &p).unwrap(),
        GoogleDraftStep::Reply(_)
    ));
    assert!(store
        .google_draft_begin(&scope, &request(&id, 1, "new-attempt"))
        .is_err());
    assert!(store
        .google_draft_claim(
            &scope,
            "one-attempt",
            status["operation"]["revision"].as_i64().unwrap()
        )
        .is_err());
    assert!(store.google_draft_observe_begin(&scope, &id).is_err());
    assert_eq!(
        load_google_draft_upload(lease.take_upload(), &CancellationToken::default()).unwrap(),
        bytes
    );
}

#[test]
fn unknown_update_replacement_requires_explicit_revision_bound_existing_target() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let p = freeze(&mut store, &scope, &request(&id, 1, "first"));
    let lease = claim(&mut store, &scope, &p);
    let bytes = raw(&store, &lease);
    ack(&mut store, &lease, "draft", "member");
    observe(&mut store, &scope, &id, Some(("draft", "member", &bytes)));
    save(&mut store, &scope, &id, 1, "Second content");
    let p = freeze(&mut store, &scope, &request(&id, 2, "unknown"));
    let lease = claim(&mut store, &scope, &p);
    store
        .google_draft_outcome(
            &lease,
            GoogleDraftOutcome::Unknown {
                code: "transport_interrupted".into(),
            },
        )
        .unwrap();
    observe(
        &mut store,
        &scope,
        &id,
        Some(("draft", "member-new", b"Uncertain remote")),
    );
    let changed = observe(
        &mut store,
        &scope,
        &id,
        Some(("draft", "member-newer", b"Changed remote again")),
    );
    let p = json!({"draft_id":id,"expected_revision":2,"request_id":"replace","expected_google_revision":changed["revision"],"action":"replace_local"});
    let original = store
        .db
        .query_row(
            "SELECT json FROM capture_operations WHERE account=?1 AND id='unknown'",
            [&scope.account],
            |r| r.get::<_, String>(0),
        )
        .unwrap();
    assert!(store
        .google_draft_begin(&scope, &request(&id, 2, "automatic-retry"))
        .is_err());
    let missing = observe(&mut store, &scope, &id, None);
    let missing_request = json!({"draft_id":id,"expected_revision":2,"request_id":"missing-replacement","expected_google_revision":missing["revision"],"action":"replace_local"});
    assert_eq!(
        store
            .google_draft_begin(&scope, &missing_request)
            .err()
            .unwrap()
            .code,
        "reconciliation_unavailable"
    );
    let present = observe(
        &mut store,
        &scope,
        &id,
        Some(("draft", "member-newer", b"Changed remote again")),
    );
    assert!(
        store.google_draft_begin(&scope, &p).is_err(),
        "previous approval must be stale"
    );
    let explicit = json!({"draft_id":id,"expected_revision":2,"request_id":"explicit-replace","expected_google_revision":present["revision"],"action":"replace_local"});
    let prepared = freeze(&mut store, &scope, &explicit);
    let replacement = claim(&mut store, &scope, &prepared);
    assert_eq!(replacement.provider_draft_id(), Some("draft"));
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT json FROM capture_operations WHERE account=?1 AND id='unknown'",
                [&scope.account],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        original
    );
}

#[test]
fn import_attachment_freeze_verifies_bytes_and_scope_without_writer_io() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let path = dir.path().join("retained.txt");
    std::fs::write(&path, b"Exact attachment content").unwrap();
    store
        .handle(
            "draft_attachment_import",
            Some(&scope.account),
            json!({"draft_id":id,"expected_revision":1,"request_id":"import","path":path}),
        )
        .unwrap();
    let p = request(&id, 2, "upload");
    let mut wrong = scope.clone();
    wrong.primary_email = "other@example.test".into();
    assert_eq!(
        store.google_draft_intent(&wrong, &p).err().unwrap().code,
        "identity_mismatch"
    );
    let plan = plan(&store, &scope, &p);
    std::fs::remove_file(path).unwrap();
    let proof = prepare_google_draft(plan, &CancellationToken::default()).unwrap();
    let prepared = store.google_draft_finish(proof).unwrap();
    assert_eq!(
        store
            .google_draft_claim(
                &wrong,
                "upload",
                prepared["operation"]["revision"].as_i64().unwrap(),
            )
            .err()
            .unwrap()
            .code,
        "identity_mismatch"
    );
    assert_eq!(
        store.google_draft_status(&scope.account, &id).unwrap(),
        prepared
    );
    let lease = claim(&mut store, &scope, &prepared);
    let bytes = raw(&store, &lease);
    let parsed = mail_parser::MessageParser::default().parse(&bytes).unwrap();
    let parts: Vec<_> = parsed.attachments().collect();
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].contents(), b"Exact attachment content");
    // Exact dispatched replay exposes the retained receipt before current
    // admission checks. It must neither create another upload nor change bytes.
    let journal = |store: &Store| {
        store
            .db
            .query_row(
                "SELECT json FROM capture_operations WHERE account=?1 AND id='upload'",
                [&scope.account],
                |r| r.get::<_, String>(0),
            )
            .unwrap()
    };
    let before_replay = journal(&store);
    match store.google_draft_intent(&wrong, &p).unwrap() {
        GoogleDraftIntent::Reply(reply) => {
            assert_eq!(
                reply,
                store.google_draft_status(&scope.account, &id).unwrap()
            );
            assert_eq!(reply["operation"]["state"], "in_flight");
        }
        GoogleDraftIntent::Upload { .. } => panic!("dispatched replay must be status-only"),
    }
    assert_eq!(journal(&store), before_replay);
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert_eq!(
        load_google_draft_upload(lease.take_upload(), &cancel)
            .unwrap_err()
            .code,
        "cancelled"
    );
    std::fs::write(
        store.profile.join("blobs").join(lease.upload_sha256()),
        b"corrupt",
    )
    .unwrap();
    assert!(load_google_draft_upload(lease.take_upload(), &CancellationToken::default()).is_err());
}

#[test]
fn schema12_migration_and_execution_discriminator_preserve_synthetic_receipts() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let account = "windscribe";
    let draft=store.handle("draft_save",Some(account),json!({"conversation_id":"windscribe-thread-000","expected_revision":0,"request_id":"save","body":"Unchanged old words"})).unwrap();
    let p = json!({"conversation_id":"windscribe-thread-000","expected_revision":1,"request_id":"old-capture"});
    let capture = store
        .handle("draft_capture", Some(account), p.clone())
        .unwrap();
    let old: String = store
        .db
        .query_row(
            "SELECT json FROM capture_operations WHERE id='old-capture'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let receipts: String = store
        .db
        .query_row(
            "SELECT result_json FROM operations WHERE request_id='save'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let fixture = store.fixture_revision().unwrap();
    crate::test_profiles::restore_schema_twelve(&store.db);
    drop(store);
    let mut store = Store::open(dir.path()).unwrap();
    assert_eq!(store.fixture_revision().unwrap(), fixture);
    assert_eq!(
        store.handle("status", None, json!({})).unwrap()["schema_version"],
        crate::SCHEMA_VERSION
    );
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT json FROM capture_operations WHERE id='old-capture'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        old
    );
    assert_eq!(
        store
            .db
            .query_row(
                "SELECT result_json FROM operations WHERE request_id='save'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        receipts
    );
    assert_eq!(
        store
            .handle("draft_capture", Some(account), p.clone())
            .unwrap(),
        capture
    );
    assert_eq!(
        store
            .handle(
                "draft_get",
                Some(account),
                json!({"draft_id":draft["draft_id"]})
            )
            .unwrap()["body"],
        draft["body"]
    );
    store.db.execute("INSERT INTO capture_operations(account,id,json,blob_hash,execution_kind) VALUES(?1,'google-collision','{}','unused','google')",[account]).unwrap();
    let mut collision = p;
    collision["request_id"] = json!("google-collision");
    assert_eq!(
        store
            .handle("draft_capture", Some(account), collision)
            .unwrap_err()
            .code,
        "request_id_conflict"
    );
    assert_eq!(
        check_execution_kind(&store.db, account, "old-capture", "google")
            .unwrap_err()
            .code,
        "request_id_conflict"
    );
    let history = store
        .handle("capture_history", Some(account), json!({}))
        .unwrap();
    assert_eq!(history["total"], 1);
    assert!(!history.to_string().contains("google-collision"));
    store.recover_captures().unwrap();
}

#[test]
fn prepared_due_partial_index_skips_synced_inventory_and_keeps_exact_resume_revision() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, scope, id) = setup(dir.path());
    let p = request(&id, 1, "prepared");
    freeze(&mut store, &scope, &p);
    save(&mut store, &scope, &id, 1, "Newer authored revision");
    // Independent historical inventory with no eligible work; all rows retain
    // ordinary drafts schema/routing and bounded twin metadata.
    let twin=json!({"version":1,"revision":1,"state":"synced","operation_id":null,"baseline":{"draft_id":"remote","message_id":"member","observed_sha256":"hash","upload_sha256":"hash","draft_revision":1,"size":1},"conflict":null}).to_string();
    for n in 0..1000 {
        store.db.execute("INSERT INTO drafts(account,draft_id,mode,routing_json,body,revision,signature_text,from_name,identity_revision,created_at,updated_at,google_json) SELECT account,?2,mode,routing_json,'',1,signature_text,from_name,identity_revision,created_at,updated_at,?3 FROM drafts WHERE account=?1 AND draft_id=?4",params![scope.account,format!("inventory-{n}"),twin,id]).unwrap();
    }
    let due = store
        .google_draft_due(&scope.account, scope.connection_generation, 10)
        .unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].draft_id, id);
    assert_eq!(due[0].revision, 1);
    assert_eq!(due[0].request, Some(p));
    let plans: Vec<String> = store
        .db
        .prepare(&format!("EXPLAIN QUERY PLAN {DUE_SQL}"))
        .unwrap()
        .query_map(params![scope.account, 10, unix_ms().unwrap()], |r| r.get(3))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert!(
        plans.iter().any(|p| p.contains("google_drafts_due")),
        "{plans:?}"
    );
}
