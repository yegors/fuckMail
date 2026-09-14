use super::*;

async fn life(h: &Harness, account: &str) -> Value {
    let account = account.to_owned();
    h.runtime
        .inner
        .core
        .call(move |store| store.account_lifecycle(&account))
        .await
        .unwrap()
}
async fn remove(h: &Harness, account: &str, request: &str, revision: i64) -> Result<Value> {
    h.runtime
        .handle(
            "owner",
            CancellationToken::new(),
            "account_remove",
            Some(account),
            json!({"request_id":request,"expected_revision":revision,"retention":"archive"}),
        )
        .await
}
async fn restore(h: &Harness, account: &str, revision: i64, subject: &str) -> Value {
    let reply = h
        .runtime
        .handle(
            "owner",
            CancellationToken::new(),
            "google_auth_restore_begin",
            Some(account),
            json!({"client_config_path":h.config(),"expected_revision":revision}),
        )
        .await
        .unwrap();
    h.signed_callback(
        &reply,
        subject,
        "renamed@example.invalid",
        &fm_google::auth::GOOGLE_SCOPES,
    )
    .await;
    reply
}

#[tokio::test]
async fn local_removal_restore_and_historical_replay_preserve_identity_without_provider_work() {
    let h = Harness::new();
    let account = h.expired_account().await;
    let work = h.runtime.account_work_cancel(&account).await.unwrap();
    let removed = remove(&h, &account, "remove", 1).await.unwrap();
    assert_eq!(removed["lifecycle"]["state"], "removed");
    assert_eq!(removed["lifecycle"]["cleanup"]["state"], "complete");
    assert!(work.is_cancelled());
    assert_eq!(h.http.calls.load(Ordering::SeqCst), 0);
    assert_eq!(h.vault.removals.load(Ordering::SeqCst), 1);
    assert!(h
        .vault
        .load(&AccountId::new(account.clone()).unwrap())
        .unwrap()
        .is_none());
    let error = h
        .runtime
        .handle(
            "owner",
            CancellationToken::new(),
            "google_auth_begin",
            Some(&account),
            json!({"client_config_path":h.config()}),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "account_removed");
    let revision = removed["lifecycle"]["revision"].as_i64().unwrap();
    let wrong = restore(&h, &account, revision, "another-subject").await;
    assert_eq!(
        completed(&h.runtime, "owner", wrong["attempt_id"].as_str().unwrap()).await["error"]
            ["code"],
        "identity_mismatch"
    );
    assert_eq!(h.vault.stores.load(Ordering::SeqCst), 1);
    assert_eq!(life(&h, &account).await, removed["lifecycle"]);
    let reply = restore(&h, &account, revision, "expired-subject").await;
    let result = completed(&h.runtime, "owner", reply["attempt_id"].as_str().unwrap()).await;
    assert_eq!(result["phase"], "connected");
    assert_eq!(result["account"], account);
    assert_eq!(life(&h, &account).await["state"], "active");
    assert_eq!(h.vault.stores.load(Ordering::SeqCst), 2);
    let new_work = h.runtime.account_work_cancel(&account).await.unwrap();
    let historical = remove(&h, &account, "remove", 1).await.unwrap();
    assert_eq!(historical["operation_is_current"], false);
    assert_eq!(historical["operation"], removed["operation"]);
    assert_eq!(historical["lifecycle"]["state"], "active");
    assert_eq!(h.vault.removals.load(Ordering::SeqCst), 1);
    assert!(!new_work.is_cancelled());
    assert!(h
        .vault
        .load(&AccountId::new(account).unwrap())
        .unwrap()
        .is_some());
    assert_eq!(
        h.http.calls.load(Ordering::SeqCst),
        4,
        "only signed OAuth exchanges and keys, no provider acquisition"
    );
    h.close().await;
}

#[tokio::test]
async fn duplicate_removal_waiter_revalidates_after_lock_and_failed_cleanup_needs_explicit_retry() {
    let h = Harness::new();
    let account = h.expired_account().await;
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    *h.vault.remove_pause.lock().unwrap() = Some(StorePause {
        entered,
        release: blocked,
    });
    let release = StoreRelease(Some(release));
    let runtime = h.runtime.clone();
    let selected = account.clone();
    let first = tokio::spawn(async move {
        runtime
            .handle(
                "first",
                CancellationToken::new(),
                "account_remove",
                Some(&selected),
                json!({"request_id":"remove","expected_revision":1,"retention":"archive"}),
            )
            .await
    });
    timeout(Duration::from_secs(3), waiting)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(life(&h, &account).await["cleanup"]["state"], "in_flight");
    let runtime = h.runtime.clone();
    let selected = account.clone();
    let second = tokio::spawn(async move {
        runtime
            .handle(
                "second",
                CancellationToken::new(),
                "account_remove",
                Some(&selected),
                json!({"request_id":"remove","expected_revision":1,"retention":"archive"}),
            )
            .await
    });
    h.vault.remove_failure.store(true, Ordering::SeqCst);
    drop(release);
    let failed = timeout(Duration::from_secs(3), first)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let _duplicate = timeout(Duration::from_secs(3), second)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(h.vault.removals.load(Ordering::SeqCst), 1);
    assert_eq!(failed["lifecycle"]["state"], "removed");
    assert_eq!(failed["lifecycle"]["cleanup"]["state"], "failed");
    assert!(h
        .vault
        .load(&AccountId::new(account.clone()).unwrap())
        .unwrap()
        .is_some());
    assert_eq!(
        remove(&h, &account, "remove", 1).await.unwrap()["lifecycle"]["cleanup"]["state"],
        "failed"
    );
    assert_eq!(h.vault.removals.load(Ordering::SeqCst), 1);
    h.vault.remove_failure.store(false, Ordering::SeqCst);
    let retried = remove(
        &h,
        &account,
        "retry",
        failed["lifecycle"]["revision"].as_i64().unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(retried["lifecycle"]["cleanup"]["state"], "complete");
    assert_eq!(h.vault.removals.load(Ordering::SeqCst), 2);
    assert_eq!(h.http.calls.load(Ordering::SeqCst), 0);
    h.close().await;
}

#[tokio::test]
async fn a_previously_admitted_duplicate_cannot_delete_a_later_restored_grant() {
    use fm_core::account_lifecycle::AccountRemovalStep;
    let h = Harness::new();
    let account = h.expired_account().await;
    let selected = account.clone();
    let (first, duplicate) = h
        .runtime
        .inner
        .core
        .call(move |store| {
            let params = json!({"request_id":"remove","expected_revision":1,"retention":"archive"});
            let AccountRemovalStep::Cleanup(first) =
                store.account_remove_begin(&selected, &params)?
            else {
                panic!("new removal lease");
            };
            let AccountRemovalStep::Cleanup(duplicate) =
                store.account_remove_begin(&selected, &params)?
            else {
                panic!("same pending removal lease");
            };
            Ok((first, duplicate))
        })
        .await
        .unwrap();
    let removed = h
        .runtime
        .cleanup_account(first, &CancellationToken::new())
        .await
        .unwrap();
    let reply = restore(
        &h,
        &account,
        removed["lifecycle"]["revision"].as_i64().unwrap(),
        "expired-subject",
    )
    .await;
    assert_eq!(
        completed(&h.runtime, "owner", reply["attempt_id"].as_str().unwrap()).await["phase"],
        "connected"
    );
    let restored = life(&h, &account).await;
    let account_id = AccountId::new(account.clone()).unwrap();
    let lock = h
        .runtime
        .inner
        .coordinator
        .account_guard(&account_id, &CancellationToken::new())
        .await
        .unwrap();
    let runtime = h.runtime.clone();
    let mut pending = tokio::spawn(async move {
        runtime
            .cleanup_account(duplicate, &CancellationToken::new())
            .await
    });
    assert!(timeout(Duration::from_millis(50), &mut pending)
        .await
        .is_err());
    assert_eq!(h.vault.removals.load(Ordering::SeqCst), 1);
    drop(lock);
    assert!(timeout(Duration::from_secs(3), pending)
        .await
        .unwrap()
        .unwrap()
        .is_err());
    assert_eq!(life(&h, &account).await, restored);
    assert_eq!(h.vault.removals.load(Ordering::SeqCst), 1);
    assert!(h.vault.load(&account_id).unwrap().is_some());
    h.close().await;
}

#[tokio::test]
async fn cancelled_or_failed_restore_after_native_store_keeps_truthful_cleanup_needed() {
    for cancelled_restore in [true, false] {
        let h = Harness::new();
        let account = h.expired_account().await;
        let removed = remove(&h, &account, "remove", 1).await.unwrap();
        let (entered, release) = h.pause_after_vault_write();
        let reply = restore(
            &h,
            &account,
            removed["lifecycle"]["revision"].as_i64().unwrap(),
            "expired-subject",
        )
        .await;
        timeout(Duration::from_secs(3), entered)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            life(&h, &account).await["cleanup"]["state"],
            "restore_in_flight"
        );
        if cancelled_restore {
            h.runtime
                .handle(
                    "owner",
                    CancellationToken::new(),
                    "google_auth_cancel",
                    None,
                    json!({"attempt_id":reply["attempt_id"]}),
                )
                .await
                .unwrap();
        } else {
            let connection =
                rusqlite::Connection::open(h.directory.path().join("profile/store.sqlite3"))
                    .unwrap();
            connection.execute_batch("CREATE TRIGGER reject_restore_commit BEFORE UPDATE ON google_accounts WHEN NEW.connection_state='connected' BEGIN SELECT RAISE(ABORT,'fixture refuses binding'); END").unwrap();
        }
        drop(release);
        let status = completed(&h.runtime, "owner", reply["attempt_id"].as_str().unwrap()).await;
        assert_eq!(
            status["phase"],
            if cancelled_restore {
                "cancelled"
            } else {
                "failed"
            }
        );
        let pending = life(&h, &account).await;
        assert_eq!(pending["state"], "removed");
        assert_eq!(pending["cleanup"]["state"], "pending");
        assert!(h
            .vault
            .load(&AccountId::new(account.clone()).unwrap())
            .unwrap()
            .is_some());
        let done = remove(
            &h,
            &account,
            "restore-cleanup",
            pending["revision"].as_i64().unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(done["lifecycle"]["cleanup"]["state"], "complete");
        assert!(h
            .vault
            .load(&AccountId::new(account).unwrap())
            .unwrap()
            .is_none());
        h.close().await;
    }
}
