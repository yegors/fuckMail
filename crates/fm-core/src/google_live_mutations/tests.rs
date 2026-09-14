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
fn missing_scope_or_mapping_rolls_back_local_change_and_journal() {
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
    assert_eq!(
        f.command(
            "set_category",
            "category",
            json!({"category_id":"category","present":true})
        )
        .unwrap_err()
        .code,
        "label_mapping_required"
    );
    assert_eq!(f.count("operations"), 0);
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
