use super::*;
use crate::google_store::{publish_message, StagedMessage};

struct Fixture {
    directory: tempfile::TempDir,
    store: Store,
    account: String,
    conversation: String,
    message: String,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open_live(directory.path()).unwrap();
        let account = store
            .bind_google_account(
                "mail-mutation-fixture",
                "reader@example.test",
                &BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]),
                None,
            )
            .unwrap();
        publish(
            &store,
            &account,
            "first",
            "100",
            &["INBOX", "UNREAD", "STARRED", "Label_unrelated"],
        );
        let (message, conversation) = store
            .db
            .query_row(
                "SELECT id,conversation_id FROM messages WHERE account=?1",
                [&account],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        Self {
            directory,
            store,
            account,
            conversation,
            message,
        }
    }
    fn command(&mut self, method: &str, request: &str, extra: Value) -> Result<Value> {
        let revision: i64 = self.store.db.query_row(
            "SELECT revision FROM conversations WHERE account=?1 AND id=?2",
            params![self.account, self.conversation],
            |r| r.get(0),
        )?;
        let mut p = json!({"id":self.conversation,"message_ids":[self.message],"request_id":request,"expected_revision":revision});
        for (key, value) in extra.as_object().unwrap() {
            p[key] = value.clone();
        }
        self.store.mutate(method, &self.account, &p)
    }
    fn next(&self, now: u64) -> MailMutationWork {
        self.store
            .google_mail_mutation_next(&self.account, 1, now)
            .unwrap()
            .unwrap()
    }
    fn state(&self, request: &str) -> String {
        self.store
            .db
            .query_row(
                "SELECT state FROM google_mail_mutations WHERE account=?1 AND request_id=?2",
                params![self.account, request],
                |r| r.get(0),
            )
            .unwrap()
    }
    fn flags(&self) -> (bool, bool, bool, bool) {
        self.store
            .db
            .query_row(
                "SELECT unread,starred,inbox,trashed FROM messages WHERE account=?1 AND id=?2",
                params![self.account, self.message],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap()
    }
    fn count(&self, table: &str) -> u64 {
        self.store
            .db
            .query_row(
                &format!("SELECT count(*) FROM {table} WHERE account=?1"),
                [&self.account],
                |r| r.get(0),
            )
            .unwrap()
    }
}
fn publish(store: &Store, account: &str, remote: &str, history: &str, labels: &[&str]) {
    let staged = StagedMessage {
        remote_id: remote.into(),
        remote_thread_id: "thread".into(),
        history_id: history.into(),
        labels: labels.iter().map(|v| (*v).into()).collect(),
        sender_name: "Human".into(),
        sender_email: "sender@example.test".into(),
        recipient: "reader@example.test".into(),
        subject: "Fixture conversation".into(),
        body: "Retained synthetic source".into(),
        sent_at: "2026-09-12T12:00:00Z".into(),
        attachments: vec![],
    };
    publish_message(&store.db, account, &staged, "synthetic-source-hash").unwrap();
}
fn observation(labels: &[&str], history: &str) -> Option<MailLabelObservation> {
    Some(MailLabelObservation {
        remote_id: "first".into(),
        remote_thread_id: "thread".into(),
        history_id: history.into(),
        labels: labels.iter().map(|v| (*v).into()).collect(),
    })
}
fn submit(decision: MailMutationDecision) -> MailMutationAttempt {
    match decision {
        MailMutationDecision::Submit(attempt) => attempt,
        _ => panic!("expected claimed attempt"),
    }
}

#[test]
fn exact_delivery_status_counts_every_member_outside_bounded_account_previews() {
    let mut f = Fixture::new();
    for index in 0..21 {
        f.command(
            "flag",
            &format!("older-{index}"),
            json!({"read":index%2==0}),
        )
        .unwrap();
    }
    f.command("flag", "target", json!({"read":true})).unwrap();
    // Delivery evidence outlives source rows. Populate a larger frozen snapshot
    // to require exact grouping beyond either account preview's twenty rows.
    for index in 0..64 {
        f.store.db.execute("INSERT INTO google_mail_mutations(account,request_id,message_id,remote_id,action,desired_json,state) SELECT account,request_id,?3,remote_id,action,desired_json,'confirmed' FROM google_mail_mutations WHERE account=?1 AND request_id=?2 AND message_id=?4",params![f.account,"target",format!("retained-{index}"),f.message]).unwrap();
    }
    let aggregate = f.store.google_mail_mutation_status(&f.account).unwrap();
    assert_eq!(aggregate.as_object().unwrap().len(), 3);
    assert_eq!(aggregate["pending"].as_array().unwrap().len(), 20);
    assert!(!aggregate["pending"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["request_id"] == "target"));
    let exact = f
        .store
        .google_mail_mutation_request_status(&f.account, "target")
        .unwrap();
    assert_eq!(
        exact,
        json!({"request_id":"target","state":"pending","counts":{"confirmed":64,"pending":1},"total":65})
    );
    assert_eq!(
        f.store
            .handle(
                "google_mail_mutation_status",
                Some(&f.account),
                json!({"request_id":"target"})
            )
            .unwrap(),
        exact
    );
    for (stored, reported) in [
        ("in_flight", "unknown"),
        ("reconcile", "unknown"),
        ("failed", "failed"),
        ("superseded", "superseded"),
        ("confirmed", "confirmed"),
    ] {
        f.store.db.execute("UPDATE google_mail_mutations SET state=?3 WHERE account=?1 AND request_id='target' AND message_id=?2",params![f.account,f.message,stored]).unwrap();
        let exact = f
            .store
            .google_mail_mutation_request_status(&f.account, "target")
            .unwrap();
        assert_eq!(exact["state"], reported);
        assert_eq!(exact["total"], 65);
        assert!(!exact.to_string().contains("retained-"));
    }
    let path = f.directory.path().to_owned();
    drop(f.store);
    let store = Store::open_live(&path).unwrap();
    assert_eq!(
        store
            .google_mail_mutation_request_status(&f.account, "target")
            .unwrap()["state"],
        "confirmed"
    );
}

#[test]
fn exact_delivery_status_refuses_missing_non_mail_and_foreign_requests() {
    let mut f = Fixture::new();
    f.command("flag", "mail", json!({"read":true})).unwrap();
    f.store.db.execute("INSERT INTO operations(account,request_id,fingerprint,result_json) VALUES(?1,'not-mail','synthetic','{}')",[&f.account]).unwrap();
    for (account, request) in [
        (&f.account[..], "missing"),
        (&f.account[..], "not-mail"),
        ("foreign", "mail"),
    ] {
        assert_eq!(
            f.store
                .google_mail_mutation_request_status(account, request)
                .unwrap_err()
                .code,
            "not_found"
        );
    }
    assert_eq!(
        f.store
            .google_mail_mutation_request_status(&f.account, "")
            .unwrap_err()
            .code,
        "invalid_request"
    );
}

#[test]
fn offline_snapshot_and_exact_receipt_survive_restart_and_new_arrival() {
    let mut f = Fixture::new();
    f.store
        .set_google_connection_state(&f.account, ConnectionState::Disconnected)
        .unwrap();
    let revision: i64 = f
        .store
        .db
        .query_row(
            "SELECT revision FROM conversations WHERE account=?1",
            [&f.account],
            |r| r.get(0),
        )
        .unwrap();
    let command = json!({"id":f.conversation,"message_ids":[f.message],"expected_revision":revision,"request_id":"done","read":true,"starred":false});
    let receipt = f.store.mutate("flag", &f.account, &command).unwrap();
    assert_eq!(
        receipt["delivery"],
        json!({"state":"pending","request_id":"done"})
    );
    assert_eq!(f.flags(), (false, false, true, false));
    assert_eq!(f.count("google_mail_mutations"), 1);
    publish(
        &f.store,
        &f.account,
        "later",
        "101",
        &["INBOX", "UNREAD", "STARRED"],
    );
    assert_eq!(
        f.store.mutate("flag", &f.account, &command).unwrap(),
        receipt
    );
    let mut different = command.clone();
    different["read"] = json!(false);
    assert_eq!(
        f.store
            .mutate("flag", &f.account, &different)
            .unwrap_err()
            .code,
        "request_id_conflict"
    );
    let directory = f.directory.path().to_owned();
    drop(f.store);
    f.store = Store::open(&directory).unwrap();
    assert_eq!(
        f.store.mutate("flag", &f.account, &command).unwrap(),
        receipt
    );
    assert_eq!(f.count("google_mail_mutations"), 1);
    let later: (bool, bool) = f
        .store
        .db
        .query_row(
            "SELECT unread,starred FROM messages WHERE account=?1 AND remote_id='later'",
            [&f.account],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(later, (true, true));
    assert_eq!(
        f.store
            .google_mail_mutation_next(&f.account, 1, 0)
            .unwrap_err()
            .code,
        "stale_google_work"
    );
}

#[test]
fn ambiguous_applied_post_confirms_by_readback_without_another_claim() {
    let mut f = Fixture::new();
    f.command("flag", "done", json!({"read":true,"starred":false}))
        .unwrap();
    let work = f.next(0);
    let attempt = submit(
        f.store
            .google_mail_mutation_observe(
                &work,
                observation(&["INBOX", "UNREAD", "STARRED", "Label_unrelated"], "100"),
            )
            .unwrap(),
    );
    assert!(attempt.add_labels.is_empty());
    assert_eq!(attempt.remove_labels, vec!["UNREAD", "STARRED"]);
    assert_eq!(f.state("done"), "in_flight");
    let directory = f.directory.path().to_owned();
    drop(f.store);
    f.store = Store::open(&directory).unwrap();
    let work = f.next(0);
    assert_eq!(work.attempt, 1);
    assert!(matches!(
        f.store
            .google_mail_mutation_observe(&work, observation(&["INBOX", "Label_unrelated"], "102"))
            .unwrap(),
        MailMutationDecision::Finished
    ));
    assert_eq!(f.state("done"), "confirmed");
    assert_eq!(f.count("google_manual_intents"), 0);
    assert!(f
        .store
        .google_mail_mutation_next(&f.account, 1, 0)
        .unwrap()
        .is_none());
    assert_eq!(
        f.store
            .google_mail_mutation_attempt_result(&attempt, MailMutationAttemptOutcome::Submitted)
            .unwrap_err()
            .code,
        "stale_google_work"
    );
    let labels: String = f
        .store
        .db
        .query_row(
            "SELECT provider_labels_json FROM google_message_state WHERE account=?1",
            [&f.account],
            |r| r.get(0),
        )
        .unwrap();
    assert!(labels.contains("Label_unrelated"));
}

#[test]
fn newer_intent_supersedes_old_fields_without_late_ack_overwrite() {
    let mut f = Fixture::new();
    f.command("flag", "old-done", json!({"read":true,"starred":false}))
        .unwrap();
    f.command("flag", "new-delay", json!({"starred":true}))
        .unwrap();
    let work = f.next(0);
    let attempt = submit(
        f.store
            .google_mail_mutation_observe(
                &work,
                observation(&["INBOX", "UNREAD", "STARRED"], "100"),
            )
            .unwrap(),
    );
    assert_eq!(attempt.remove_labels, vec!["UNREAD"]);
    f.store
        .google_mail_mutation_attempt_result(&attempt, MailMutationAttemptOutcome::Submitted)
        .unwrap();
    let work = f.next(0);
    f.store
        .google_mail_mutation_observe(&work, observation(&["INBOX", "STARRED"], "101"))
        .unwrap();
    assert_eq!(f.state("old-done"), "confirmed");
    assert_eq!(f.flags(), (false, true, true, false));
    assert_eq!(f.count("google_manual_intents"), 1);
    let work = f.next(0);
    assert_eq!(work.request_id, "new-delay");
    assert!(matches!(
        f.store
            .google_mail_mutation_observe(&work, observation(&["INBOX", "STARRED"], "101"))
            .unwrap(),
        MailMutationDecision::Finished
    ));
    assert_eq!(f.state("new-delay"), "confirmed");
    assert_eq!(f.count("google_manual_intents"), 0);
}

#[test]
fn newer_reverse_action_after_claim_stays_overlaid_until_its_own_confirmation() {
    let mut f = Fixture::new();
    f.command("flag", "read", json!({"read":true})).unwrap();
    let work = f.next(0);
    let attempt = submit(
        f.store
            .google_mail_mutation_observe(&work, observation(&["INBOX", "UNREAD"], "100"))
            .unwrap(),
    );
    f.command("flag", "unread", json!({"read":false})).unwrap();
    f.store
        .google_mail_mutation_attempt_result(&attempt, MailMutationAttemptOutcome::Submitted)
        .unwrap();
    let work = f.next(0);
    f.store
        .google_mail_mutation_observe(&work, observation(&["INBOX"], "101"))
        .unwrap();
    assert_eq!(f.state("read"), "superseded");
    assert!(f.flags().0);
    let work = f.next(0);
    let attempt = submit(
        f.store
            .google_mail_mutation_observe(&work, observation(&["INBOX"], "101"))
            .unwrap(),
    );
    assert_eq!(attempt.add_labels, vec!["UNREAD"]);
}

#[test]
fn archive_trash_restore_preserve_review_flags_and_other_labels() {
    let mut f = Fixture::new();
    f.command("mail_filing", "archive", json!({"action":"archive"}))
        .unwrap();
    assert_eq!(f.flags(), (true, true, false, false));
    publish(
        &f.store,
        &f.account,
        "first",
        "101",
        &["INBOX", "UNREAD", "STARRED", "Label_unrelated"],
    );
    assert!(
        !f.flags().2,
        "provider ingestion must preserve pending archive"
    );
    let work = f.next(0);
    let attempt = submit(
        f.store
            .google_mail_mutation_observe(
                &work,
                observation(&["INBOX", "UNREAD", "STARRED", "Label_unrelated"], "101"),
            )
            .unwrap(),
    );
    assert_eq!(attempt.remove_labels, vec!["INBOX"]);
    assert!(attempt.add_labels.is_empty());
    f.store
        .google_mail_mutation_attempt_result(&attempt, MailMutationAttemptOutcome::Submitted)
        .unwrap();
    let work = f.next(0);
    f.store
        .google_mail_mutation_observe(
            &work,
            observation(&["UNREAD", "STARRED", "Label_unrelated"], "102"),
        )
        .unwrap();
    f.command("mail_filing", "trash", json!({"action":"trash"}))
        .unwrap();
    publish(
        &f.store,
        &f.account,
        "first",
        "103",
        &["UNREAD", "STARRED", "Label_unrelated"],
    );
    assert_eq!(f.flags(), (true, true, false, true));
    let work = f.next(0);
    let attempt = submit(
        f.store
            .google_mail_mutation_observe(
                &work,
                observation(&["UNREAD", "STARRED", "Label_unrelated"], "103"),
            )
            .unwrap(),
    );
    assert_eq!(attempt.work.action, MailMutationAction::Trash);
    f.store
        .google_mail_mutation_attempt_result(&attempt, MailMutationAttemptOutcome::Submitted)
        .unwrap();
    let work = f.next(0);
    f.store
        .google_mail_mutation_observe(
            &work,
            observation(&["TRASH", "UNREAD", "STARRED", "Label_unrelated"], "104"),
        )
        .unwrap();
    f.command("mail_filing", "restore", json!({"action":"restore"}))
        .unwrap();
    assert_eq!(f.flags(), (true, true, false, false));
    let work = f.next(0);
    let attempt = submit(
        f.store
            .google_mail_mutation_observe(
                &work,
                observation(&["TRASH", "UNREAD", "STARRED", "Label_unrelated"], "104"),
            )
            .unwrap(),
    );
    assert_eq!(attempt.work.action, MailMutationAction::Restore);
    assert!(!attempt.add_labels.contains(&"INBOX".into()));
}

#[test]
fn category_uses_frozen_mapping_and_rejects_a_changed_mapping() {
    let mut f = Fixture::new();
    f.store
        .db
        .execute(
            "INSERT INTO categories(account,id,name) VALUES(?1,'category','Category')",
            [&f.account],
        )
        .unwrap();
    f.store
        .google_map_category(&f.account, "category", "Label_one")
        .unwrap();
    f.command(
        "set_category",
        "category",
        json!({"category_id":"category","present":true}),
    )
    .unwrap();
    let work = f.next(0);
    let attempt = submit(
        f.store
            .google_mail_mutation_observe(
                &work,
                observation(&["INBOX", "UNREAD", "STARRED", "Label_unrelated"], "100"),
            )
            .unwrap(),
    );
    assert_eq!(attempt.add_labels, vec!["Label_one"]);
    assert!(attempt.remove_labels.is_empty());
    f.store
        .google_mail_mutation_attempt_result(&attempt, MailMutationAttemptOutcome::Submitted)
        .unwrap();
    f.store
        .google_map_category(&f.account, "category", "Label_two")
        .unwrap();
    let work = f.next(0);
    assert!(matches!(
        f.store
            .google_mail_mutation_observe(
                &work,
                observation(&["INBOX", "Label_one", "Label_unrelated"], "101")
            )
            .unwrap(),
        MailMutationDecision::Finished
    ));
    assert_eq!(f.state("category"), "failed");
    assert_eq!(f.count("google_manual_intents"), 1);
    assert_eq!(
        f.store.google_mail_mutation_status(&f.account).unwrap()["failures"][0]["error_code"],
        "label_mapping_changed"
    );
}

#[test]
fn epochs_stale_readbacks_and_missing_targets_never_allow_blind_retry() {
    let mut f = Fixture::new();
    f.command("flag", "read", json!({"read":true})).unwrap();
    let work = f.next(0);
    assert!(matches!(
        f.store
            .google_mail_mutation_observe(&work, observation(&["UNREAD"], "99"))
            .unwrap(),
        MailMutationDecision::RetryRead
    ));
    assert!(f
        .store
        .google_mail_mutation_next(&f.account, 1, 999)
        .unwrap()
        .is_none());
    let work = f.next(1000);
    let attempt = submit(
        f.store
            .google_mail_mutation_observe(&work, observation(&["UNREAD"], "100"))
            .unwrap(),
    );
    f.store
        .set_google_connection_state(&f.account, ConnectionState::Disconnected)
        .unwrap();
    f.store
        .google_mail_mutation_attempt_result(&attempt, MailMutationAttemptOutcome::Submitted)
        .unwrap();
    assert_eq!(f.state("read"), "reconcile");
    f.store
        .bind_google_account(
            "mail-mutation-fixture",
            "reader@example.test",
            &BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]),
            Some(&f.account),
        )
        .unwrap();
    let work = f
        .store
        .google_mail_mutation_next(&f.account, 3, 1000)
        .unwrap()
        .unwrap();
    assert!(matches!(
        f.store.google_mail_mutation_observe(&work, None).unwrap(),
        MailMutationDecision::Finished
    ));
    assert_eq!(f.state("read"), "failed");
    assert_eq!(
        f.count("messages"),
        1,
        "remote absence never deletes retained source rows"
    );
}

#[test]
fn missing_scope_blocks_provider_flags_and_local_labels_queue_only_after_linking() {
    let mut f = Fixture::new();
    f.store
        .db
        .execute(
            "UPDATE google_accounts SET scopes_json='[]' WHERE account=?1",
            [&f.account],
        )
        .unwrap();
    assert_eq!(
        f.command("flag", "read", json!({"read":true}))
            .unwrap_err()
            .code,
        "capability_unavailable"
    );
    assert!(f.flags().0);
    assert_eq!(f.count("operations"), 0);
    assert_eq!(f.count("google_manual_intents"), 0);
    f.store
        .db
        .execute(
            "UPDATE google_accounts SET scopes_json=?2 WHERE account=?1",
            params![
                f.account,
                json!(["https://www.googleapis.com/auth/gmail.modify"]).to_string()
            ],
        )
        .unwrap();
    f.store
        .db
        .execute(
            "INSERT INTO categories(account,id,name) VALUES(?1,'category','Category')",
            [&f.account],
        )
        .unwrap();
    f.command(
        "set_category",
        "category",
        json!({"category_id":"category","present":true}),
    )
    .unwrap();
    assert_eq!(f.count("operations"), 1);
    assert_eq!(f.count("google_mail_mutations"), 0);
    f.store
        .google_map_category(&f.account, "category", "Label_local")
        .unwrap();
    assert_eq!(
        f.store.google_label_memberships_queue(&f.account).unwrap(),
        1
    );
    assert_eq!(
        f.store.google_label_memberships_queue(&f.account).unwrap(),
        0
    );
    assert_eq!(f.count("google_mail_mutations"), 1);
}

#[test]
fn deleted_local_target_keeps_receipt_and_finishes_without_delivery() {
    let mut f = Fixture::new();
    f.command("flag", "read", json!({"read":true})).unwrap();
    f.store
        .db
        .execute(
            "DELETE FROM google_manual_intents WHERE account=?1",
            [&f.account],
        )
        .unwrap();
    f.store
        .db
        .execute(
            "DELETE FROM google_message_state WHERE account=?1",
            [&f.account],
        )
        .unwrap();
    f.store
        .db
        .execute("DELETE FROM messages WHERE account=?1", [&f.account])
        .unwrap();
    assert_eq!(f.count("operations"), 1);
    assert_eq!(f.count("google_mail_mutations"), 1);
    let work = f.next(0);
    assert!(matches!(
        f.store
            .google_mail_mutation_observe(&work, observation(&["UNREAD"], "101"))
            .unwrap(),
        MailMutationDecision::Finished
    ));
    assert_eq!(f.state("read"), "failed");
    assert_eq!(
        f.store.google_mail_mutation_status(&f.account).unwrap()["failures"][0]["error_code"],
        "local_message_missing"
    );
}

#[test]
fn confirming_identical_visible_state_does_not_invalidate_navigation_or_fts() {
    let mut f = Fixture::new();
    f.command("flag", "read", json!({"read":true})).unwrap();
    let before:(i64,i64,String)=f.store.db.query_row("SELECT m.revision,c.revision,m.source_hash FROM messages m JOIN conversations c ON c.account=m.account AND c.id=m.conversation_id WHERE m.account=?1",[&f.account],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    let work = f.next(0);
    f.store
        .google_mail_mutation_observe(
            &work,
            observation(&["INBOX", "STARRED", "Label_unrelated"], "101"),
        )
        .unwrap();
    let after:(i64,i64,String)=f.store.db.query_row("SELECT m.revision,c.revision,m.source_hash FROM messages m JOIN conversations c ON c.account=m.account AND c.id=m.conversation_id WHERE m.account=?1",[&f.account],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(before, after);
    let fts: u64 = f
        .store
        .db
        .query_row(
            "SELECT count(*) FROM message_fts WHERE message_fts MATCH 'retained'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(fts, 1);
}

#[test]
fn assisted_single_label_delivery_acknowledgement_and_manual_override() {
    let mut f = Fixture::new();
    f.store
        .db
        .execute(
            "INSERT INTO categories(account,id,name) VALUES(?1,'category','Category')",
            [&f.account],
        )
        .unwrap();
    f.store
        .google_map_category(&f.account, "category", "Label_one")
        .unwrap();
    let flags = f.flags();
    {
        let tx = f.store.db.unchecked_transaction().unwrap();
        assert_eq!(
            enqueue_assisted_category(&tx, &f.account, &f.message, "category").unwrap(),
            "pending"
        );
        tx.commit().unwrap();
    }
    let work = f.next(0);
    assert!(work.desired[0].assisted);
    let attempt = submit(
        f.store
            .google_mail_mutation_observe(
                &work,
                observation(&["INBOX", "UNREAD", "STARRED", "Label_unrelated"], "100"),
            )
            .unwrap(),
    );
    assert_eq!(attempt.add_labels, vec!["Label_one"]);
    assert!(attempt.remove_labels.is_empty());
    f.store
        .google_mail_mutation_attempt_result(&attempt, MailMutationAttemptOutcome::Submitted)
        .unwrap();
    let work = f.next(0);
    assert!(matches!(
        f.store
            .google_mail_mutation_observe(
                &work,
                observation(
                    &["INBOX", "UNREAD", "STARRED", "Label_unrelated", "Label_one"],
                    "101"
                )
            )
            .unwrap(),
        MailMutationDecision::Finished
    ));
    assert_eq!(f.count("assist_category_intents"), 0);
    assert_eq!(f.flags(), flags);
    f.command(
        "set_category",
        "human-removal",
        json!({"category_id":"category","present":false}),
    )
    .unwrap();
    {
        let tx = f.store.db.unchecked_transaction().unwrap();
        assert_eq!(
            enqueue_assisted_category(&tx, &f.account, &f.message, "category").unwrap(),
            "manual_override"
        );
    }
}

#[test]
fn assisted_arrival_read_is_durable_and_human_read_choice_wins() {
    let f = Fixture::new();
    {
        let tx = f.store.db.unchecked_transaction().unwrap();
        assert!(enqueue_assisted_read(&tx, &f.account, &f.message).unwrap());
        tx.commit().unwrap();
    }
    assert!(!f.flags().0, "eligible incoming mail becomes locally read");
    let work = f.next(1000);
    assert_eq!(work.desired.len(), 1);
    assert_eq!(work.desired[0].field, "read");
    assert_eq!(work.desired[0].label, "UNREAD");
    assert!(!work.desired[0].present);
    project_labels(
        &f.store.db,
        &f.account,
        &f.message,
        &BTreeSet::from(["INBOX".into(), "UNREAD".into()]),
    )
    .unwrap();
    assert!(
        !f.flags().0,
        "provider snapshots preserve the pending read overlay"
    );
    let mut other = Fixture::new();
    other
        .command("flag", "human-unread", json!({"read":false}))
        .unwrap();
    let tx = other.store.db.unchecked_transaction().unwrap();
    assert!(
        !enqueue_assisted_read(&tx, &other.account, &other.message).unwrap(),
        "pending human intent has precedence"
    );
}
#[test]
fn assisted_label_yields_to_new_provider_category_before_submission() {
    let mut f = Fixture::new();
    for (category, label) in [("one", "Label_one"), ("two", "Label_two")] {
        f.store
            .db
            .execute(
                "INSERT INTO categories(account,id,name) VALUES(?1,?2,?2)",
                params![f.account, category],
            )
            .unwrap();
        f.store
            .google_map_category(&f.account, category, label)
            .unwrap();
    }
    {
        let tx = f.store.db.unchecked_transaction().unwrap();
        enqueue_assisted_category(&tx, &f.account, &f.message, "one").unwrap();
        tx.commit().unwrap();
    }
    let work = f.next(0);
    assert!(matches!(
        f.store
            .google_mail_mutation_observe(
                &work,
                observation(&["INBOX", "UNREAD", "STARRED", "Label_two"], "101")
            )
            .unwrap(),
        MailMutationDecision::Finished
    ));
    assert_eq!(f.count("assist_category_intents"), 0);
}

#[test]
fn assisted_local_label_waits_for_explicit_link_and_keeps_manual_precedence() {
    let mut f = Fixture::new();
    f.store
        .db
        .execute(
            "INSERT INTO categories(account,id,name) VALUES(?1,'local','Local label')",
            [&f.account],
        )
        .unwrap();
    let flags = f.flags();
    {
        let tx = f.store.db.unchecked_transaction().unwrap();
        assert_eq!(
            enqueue_assisted_category(&tx, &f.account, &f.message, "local").unwrap(),
            "local_only"
        );
        tx.commit().unwrap();
    }
    assert_eq!(f.count("google_mail_mutations"), 0);
    f.store
        .google_map_category(&f.account, "local", "Label_local")
        .unwrap();
    assert_eq!(
        f.store.google_label_memberships_queue(&f.account).unwrap(),
        1
    );
    assert_eq!(
        f.store.google_label_memberships_queue(&f.account).unwrap(),
        0
    );
    let work = f.next(0);
    assert!(work.desired[0].assisted);
    f.command(
        "set_category",
        "manual-before-linked-delivery",
        json!({"category_id":"local","present":false}),
    )
    .unwrap();
    assert!(matches!(
        f.store
            .google_mail_mutation_observe(
                &work,
                observation(&["INBOX", "UNREAD", "STARRED"], "102")
            )
            .unwrap(),
        MailMutationDecision::Finished
    ));
    assert_eq!(f.flags(), flags);
}

#[test]
fn mail_undo_restores_exact_flags_replays_and_rejects_newer_changes() {
    let mut f = Fixture::new();
    let before = f.flags();
    let done = f
        .command("flag", "done", json!({"read":true,"starred":false}))
        .unwrap();
    assert_eq!(f.flags(), (false, false, true, false));
    let undo = json!({"id":f.conversation,"expected_revision":done["revision"],"undo_request_id":"done","request_id":"undo-done"});
    let restored = f.store.mutate("mail_undo", &f.account, &undo).unwrap();
    assert_eq!(f.flags(), before);
    assert_eq!(restored["undone_request_id"], "done");
    assert_eq!(
        f.store.mutate("mail_undo", &f.account, &undo).unwrap(),
        restored
    );
    assert_eq!(f.state("undo-done"), "pending");
    let done = f
        .command("flag", "done-again", json!({"read":true,"starred":false}))
        .unwrap();
    f.command("flag", "newer", json!({"starred":true})).unwrap();
    assert_eq!(f.store.mutate("mail_undo",&f.account,&json!({"id":f.conversation,"expected_revision":done["revision"],"undo_request_id":"done-again","request_id":"stale-undo"})).unwrap_err().code,"stale_revision");
    assert_eq!(f.flags(), (false, true, true, false));
}

#[test]
fn mail_undo_trash_restores_provider_trash_before_other_labels() {
    let mut f = Fixture::new();
    let deleted = f
        .command("mail_filing", "trash", json!({"action":"trash"}))
        .unwrap();
    f.store.mutate("mail_undo",&f.account,&json!({"id":f.conversation,"expected_revision":deleted["revision"],"undo_request_id":"trash","request_id":"undo-trash"})).unwrap();
    assert_eq!(f.flags(), (true, true, true, false));
    // The earlier unsent trash intent is superseded before Undo is sent.
    let first = f.next(10);
    f.store
        .google_mail_mutation_observe(&first, observation(&["TRASH", "UNREAD", "STARRED"], "101"))
        .unwrap();
    let next = f.next(11);
    let attempt = submit(
        f.store
            .google_mail_mutation_observe(
                &next,
                observation(&["TRASH", "UNREAD", "STARRED"], "102"),
            )
            .unwrap(),
    );
    assert_eq!(attempt.work.action, MailMutationAction::Restore);
}

#[test]
fn mail_undo_records_and_delivers_only_the_fields_it_restores() {
    let mut f = Fixture::new();
    for category in ["linked", "local"] {
        f.store
            .db
            .execute(
                "INSERT INTO categories(account,id,name) VALUES(?1,?2,?2)",
                params![f.account, category],
            )
            .unwrap();
    }
    f.store
        .google_map_category(&f.account, "linked", "Label_linked")
        .unwrap();
    let done = f
        .command("flag", "read-only", json!({"read":true}))
        .unwrap();
    f.store.mutate("mail_undo",&f.account,&json!({"id":f.conversation,"expected_revision":done["revision"],"undo_request_id":"read-only","request_id":"undo-read"})).unwrap();
    let intents: Vec<(String, String)> = f
        .store
        .db
        .prepare("SELECT field,category_id FROM google_manual_intents WHERE account=?1 ORDER BY field,category_id")
        .unwrap()
        .query_map([&f.account], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(intents, vec![("read".into(), String::new())]);
    let desired: String = f
        .store
        .db
        .query_row(
            "SELECT desired_json FROM google_mail_mutations WHERE account=?1 AND request_id='undo-read'",
            [&f.account],
            |r| r.get(0),
        )
        .unwrap();
    let desired: Value = serde_json::from_str(&desired).unwrap();
    assert_eq!(desired.as_array().unwrap().len(), 1, "{desired}");
    assert_eq!(desired[0]["field"], "read");
    // Undoing a flag change leaves labels to the model and never queues removals.
    {
        let tx = f.store.db.unchecked_transaction().unwrap();
        assert_ne!(
            enqueue_assisted_category(&tx, &f.account, &f.message, "linked").unwrap(),
            "manual_override"
        );
    }
    f.store
        .google_map_category(&f.account, "local", "Label_local")
        .unwrap();
    assert_eq!(
        f.store.google_label_memberships_queue(&f.account).unwrap(),
        0
    );
}

#[test]
fn retention_uses_gmail_received_date_and_removes_emptied_conversations() {
    let mut f = Fixture::new();
    // An old header date alone does not prune mail Gmail received recently.
    f.store
        .db
        .execute(
            "UPDATE messages SET sent_at='2020-01-01T00:00:00Z' WHERE account=?1",
            [&f.account],
        )
        .unwrap();
    let received = chrono::Utc::now().timestamp_millis();
    f.store.db.execute("INSERT INTO google_mail_coverage(account,remote_id,observed_generation,source_state,index_state,attachment_state,internal_date_ms) VALUES(?1,'first',1,'retained','indexed','complete',?2)",params![f.account,received]).unwrap();
    let before = f.store.accounts().unwrap()["accounts"][0].clone();
    let saved=f.store.handle("account_update",Some(&f.account),json!({"name":"Personal","signature_text":"","mail_retention":"90_days","expected_revision":before["metadata_revision"],"request_id":"received-window"})).unwrap();
    assert_eq!(f.count("messages"), 1);
    f.store
        .db
        .execute(
            "UPDATE google_mail_coverage SET internal_date_ms=?2 WHERE account=?1",
            params![f.account, received - 400 * 86_400_000],
        )
        .unwrap();
    f.store.handle("account_update",Some(&f.account),json!({"name":"Personal","signature_text":"","mail_retention":"1_year","expected_revision":saved["metadata_revision"],"request_id":"received-prune"})).unwrap();
    assert_eq!(f.count("messages"), 0);
    assert_eq!(f.count("conversations"), 0);
    assert_eq!(
        f.store.accounts().unwrap()["accounts"][0]["conversation_count"],
        0
    );
}

#[test]
fn retention_edit_prunes_local_rows_and_extending_resets_acquisition() {
    let mut f = Fixture::new();
    f.store
        .db
        .execute(
            "UPDATE messages SET sent_at='2020-01-01T00:00:00Z' WHERE account=?1",
            [&f.account],
        )
        .unwrap();
    let before = f.store.accounts().unwrap()["accounts"][0].clone();
    let saved=f.store.handle("account_update",Some(&f.account),json!({"name":"Personal","signature_text":"","mail_retention":"90_days","expected_revision":before["metadata_revision"],"request_id":"shorten"})).unwrap();
    assert_eq!(f.count("messages"), 0);
    assert_eq!(f.count("google_mail_mutations"), 0);
    assert_eq!(
        f.store
            .db
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
        0
    );
    let generation: i64 = f
        .store
        .db
        .query_row(
            "SELECT mail_generation FROM google_accounts WHERE account=?1",
            [&f.account],
            |r| r.get(0),
        )
        .unwrap();
    let extended=f.store.handle("account_update",Some(&f.account),json!({"name":"Personal","signature_text":"","mail_retention":"all","expected_revision":saved["metadata_revision"],"request_id":"extend"})).unwrap();
    assert!(extended["mail_since"].is_null());
    assert_eq!(
        f.store
            .db
            .query_row(
                "SELECT mail_generation FROM google_accounts WHERE account=?1",
                [&f.account],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        generation + 1
    );
}

#[test]
fn retention_shortening_rolls_back_while_a_conversation_has_pending_work() {
    let mut f = Fixture::new();
    publish(&f.store, &f.account, "second", "101", &["INBOX"]);
    f.store.db.execute("UPDATE messages SET sent_at='2020-01-01T00:00:00Z' WHERE account=?1 AND remote_id='second'",[&f.account]).unwrap();
    f.command("flag", "pending-newer", json!({"read":true}))
        .unwrap();
    let before = f.store.accounts().unwrap()["accounts"][0].clone();
    let generation: i64 = f
        .store
        .db
        .query_row(
            "SELECT mail_generation FROM google_accounts WHERE account=?1",
            [&f.account],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(f.store.handle("account_update",Some(&f.account),json!({"name":"Personal","signature_text":"","mail_retention":"90_days","expected_revision":before["metadata_revision"],"request_id":"blocked-shorten"})).unwrap_err().code,"retention_work_pending");
    assert_eq!(f.count("messages"), 2);
    assert_eq!(f.state("pending-newer"), "pending");
    assert_eq!(
        f.store
            .db
            .query_row(
                "SELECT mail_generation FROM google_accounts WHERE account=?1",
                [&f.account],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        generation
    );
}

#[test]
fn account_local_read_access_survives_restart_and_blocks_provider_writes() {
    let mut f = Fixture::new();
    let before = f.store.accounts().unwrap()["accounts"][0].clone();
    let update = json!({"name":before["name"],"signature_text":"","expected_revision":before["metadata_revision"],"request_id":"read-access","sync":{"mail":true,"calendar":false,"mail_access":"read","calendar_access":"read"}});
    f.store
        .handle("account_update", Some(&f.account), update.clone())
        .unwrap();
    assert_eq!(
        f.command("flag", "blocked-read", json!({"read":true}))
            .unwrap_err()
            .code,
        "capability_unavailable"
    );
    assert_eq!(f.flags(), (true, true, true, false));
    drop(f.store);
    let mut reopened = Store::open_live(f.directory.path()).unwrap();
    let accounts = reopened.accounts().unwrap();
    assert_eq!(
        accounts["accounts"][0]["google"]["capabilities"]["mail_write"],
        false
    );
    assert_eq!(
        accounts["accounts"][0]["google"]["granted_capabilities"]["mail_write"],
        true
    );
    reopened
        .handle("account_update", Some(&f.account), update)
        .unwrap();
    assert!(!crate::preferences::google_write_enabled(&reopened.db, &f.account, "mail").unwrap());
}
