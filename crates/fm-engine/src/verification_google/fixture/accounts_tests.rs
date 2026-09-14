use super::*;
use crate::google_sync::Runtime;

struct Harness {
    _directory: tempfile::TempDir,
    profile: PathBuf,
    fixture: Fixture,
    core: CoreBridge,
    auth: Arc<google_auth::Runtime>,
    sync: Arc<Runtime>,
    cancel: CancellationToken,
    owner: CancellationToken,
    writer: std::thread::JoinHandle<()>,
}
impl Harness {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let profile = fm_ipc::private_profile(&directory.path().join("profile")).unwrap();
        claim_empty_profile(&profile).unwrap();
        let mut store = Store::open_live(&profile).unwrap();
        let fixture = Fixture::prepare_with_accounts(
            &profile,
            &mut store,
            false,
            false,
            false,
            None,
            Some(1_789_300_800),
        )
        .unwrap();
        assert_eq!(
            store.handle("accounts", None, json!({})).unwrap()["accounts"],
            json!([])
        );
        let (queue, mut incoming) = tokio::sync::mpsc::channel(32);
        let trace = fixture.clone();
        let writer = std::thread::spawn(move || {
            let _trace = trace.install_writer_trace();
            while let Some(crate::Work::Provider(call)) = incoming.blocking_recv() {
                call.run(&mut store);
            }
        });
        let core = CoreBridge::new(queue, Arc::new(tokio::sync::Semaphore::new(16)));
        let cancel = CancellationToken::new();
        let auth = fixture.auth(core.clone(), cancel.clone());
        let sync = Runtime::new(core.clone(), auth.clone(), cancel.clone());
        Self {
            _directory: directory,
            profile,
            fixture,
            core,
            auth,
            sync,
            cancel,
            owner: CancellationToken::new(),
            writer,
        }
    }
    fn step(&self, id: u64, identity: &str, outcome: &str, grant: &str, renamed: bool) {
        std::fs::write(self.profile.join("verification-google-account-step.json"),serde_json::to_vec(&json!({"step":id,"identity":identity,"outcome":outcome,"grant":grant,"renamed":renamed})).unwrap()).unwrap();
    }
    async fn begin(&self, account: Option<&str>, revision: Option<i64>) -> Value {
        let mut p =
            json!({"client_config_path":self.profile.join("verification-google-client.json")});
        if let Some(r) = revision {
            p["expected_revision"] = r.into();
        }
        self.auth
            .handle(
                "owner",
                self.owner.clone(),
                if revision.is_some() {
                    "google_auth_restore_begin"
                } else {
                    "google_auth_begin"
                },
                account,
                p,
            )
            .await
            .unwrap()
    }
    async fn completed(&self, reply: &Value) -> Value {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let status = self
                    .auth
                    .handle(
                        "owner",
                        self.owner.clone(),
                        "google_auth_status",
                        None,
                        json!({"attempt_id":reply["attempt_id"]}),
                    )
                    .await
                    .unwrap();
                if ["connected", "failed", "cancelled"].contains(&status["phase"].as_str().unwrap())
                {
                    return status;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap()
    }
    async fn life(&self, account: &str) -> Value {
        let a = account.to_owned();
        self.core
            .call(move |s| s.account_lifecycle(&a))
            .await
            .unwrap()
    }
    async fn remove(&self, account: &str, request: &str, revision: i64) -> Value {
        self.auth
            .handle(
                "owner",
                self.owner.clone(),
                "account_remove",
                Some(account),
                json!({"expected_revision":revision,"request_id":request,"retention":"archive"}),
            )
            .await
            .unwrap()
    }
    async fn close(self) {
        self.cancel.cancel();
        self.sync.shutdown().await;
        self.auth.shutdown().await;
        drop(self.sync);
        drop(self.auth);
        drop(self.core);
        self.writer.join().unwrap();
        self.fixture.finish(true).await.unwrap();
        assert_eq!(self.fixture.report(true)["errors"], json!([]));
    }
}

#[tokio::test]
async fn accounts_fixture_drives_signed_identity_grants_import_remove_and_restore_without_autostart(
) {
    let h = Harness::new();
    assert_eq!(
        h.auth
            .handle(
                "owner",
                h.owner.clone(),
                "verification_google_auth_fixture",
                None,
                json!({})
            )
            .await
            .unwrap(),
        json!({"mode":"accounts-v1","browser_stub_allowed":true})
    );
    for (index, account) in h.fixture.accounts.iter().enumerate() {
        h.step(index as u64 + 1, account.key, "success", "full", false);
        assert_eq!(
            h.completed(&h.begin(None, None).await).await["account"],
            account.id
        );
    }
    assert_eq!(h.fixture.report(false)["counters"]["http_started"], 0);
    for account in h.fixture.accounts.iter() {
        h.sync
            .handle(
                "owner",
                h.owner.clone(),
                "google_sync_start",
                Some(&account.id),
                super::tests::limits(),
            )
            .await
            .unwrap();
    }
    let db = rusqlite::Connection::open(h.profile.join("store.sqlite3")).unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let count: i64 = db
                .query_row("SELECT count(*) FROM messages", [], |r| r.get(0))
                .unwrap();
            let events: i64 = db
                .query_row("SELECT count(*) FROM calendar_visible_sources", [], |r| {
                    r.get(0)
                })
                .unwrap();
            if count == 4 && events == 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    for account in h.fixture.accounts.iter() {
        h.sync
            .handle(
                "owner",
                h.owner.clone(),
                "google_sync_stop",
                Some(&account.id),
                json!({}),
            )
            .await
            .unwrap();
    }
    let account = &h.fixture.accounts[0].id;
    let removed = h.remove(account, "remove", 1).await;
    assert_eq!(removed["lifecycle"]["cleanup"]["state"], "complete");
    let before = h.fixture.report(false)["counters"].clone();
    h.step(3, "personal", "success", "mail_readonly", true);
    let status = h
        .completed(
            &h.begin(
                Some(account),
                Some(removed["lifecycle"]["revision"].as_i64().unwrap()),
            )
            .await,
        )
        .await;
    assert_eq!(status["phase"], "connected");
    assert_eq!(status["account"], *account);
    let a = account.clone();
    let bindings = h
        .core
        .call(move |s| {
            s.google_account_bindings()
                .map(|v| v.into_iter().find(|a2| a2.account == a).unwrap())
        })
        .await
        .unwrap();
    assert_eq!(
        db.query_row("SELECT email FROM accounts WHERE id=?1", [account], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "renamed-personal@example.test"
    );
    assert!(bindings
        .scopes
        .contains("https://www.googleapis.com/auth/gmail.readonly"));
    assert!(!bindings
        .scopes
        .contains("https://www.googleapis.com/auth/gmail.modify"));
    let after = h.fixture.report(false);
    assert_eq!(after["counters"]["http_started"], before["http_started"]);
    assert_eq!(after["account_fixture"]["vault_remove_completed"], 1);
    assert_eq!(after["account_fixture"]["vault_store_completed"], 3);
    let replay = h.remove(account, "remove", 1).await;
    assert_eq!(replay["operation_is_current"], false);
    assert_eq!(
        h.fixture.report(false)["account_fixture"]["vault_remove_attempts"],
        1
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM messages", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        4
    );
    drop(db);
    h.close().await;
}

#[tokio::test]
async fn accounts_fixture_denial_and_cancel_after_store_leave_truthful_removed_cleanup() {
    let h = Harness::new();
    h.step(1, "personal", "denied", "full", false);
    let denied = h.completed(&h.begin(None, None).await).await;
    assert_eq!(denied["phase"], "failed");
    assert_eq!(
        h.fixture.report(false)["account_fixture"]["token_exchanges"],
        0
    );
    h.step(2, "personal", "success", "full", false);
    let joined = h.completed(&h.begin(None, None).await).await;
    let account = joined["account"].as_str().unwrap();
    let removed = h.remove(account, "remove", 1).await;
    let hold = h.profile.join("verification-google-auth-hold-store");
    std::fs::write(&hold, []).unwrap();
    h.step(3, "personal", "success", "full", false);
    let reply = h
        .begin(
            Some(account),
            Some(removed["lifecycle"]["revision"].as_i64().unwrap()),
        )
        .await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if h.fixture.report(false)["account_fixture"]["phase"] == "store_held_after_write" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        h.fixture.report(false)["account_fixture"]["credential_accounts"],
        json!([account])
    );
    h.auth
        .handle(
            "owner",
            h.owner.clone(),
            "google_auth_cancel",
            None,
            json!({"attempt_id":reply["attempt_id"]}),
        )
        .await
        .unwrap();
    std::fs::remove_file(&hold).unwrap();
    assert_eq!(h.completed(&reply).await["phase"], "cancelled");
    let life = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let life = h.life(account).await;
            if life["cleanup"]["state"] == "pending" {
                break life;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(life["state"], "removed");
    assert_eq!(life["cleanup"]["state"], "pending");
    assert_eq!(
        h.remove(account, "cleanup", life["revision"].as_i64().unwrap())
            .await["lifecycle"]["cleanup"]["state"],
        "complete"
    );
    assert_eq!(
        h.fixture.report(false)["account_fixture"]["credential_accounts"],
        json!([])
    );
    h.close().await;
}

#[tokio::test]
async fn concurrent_first_add_cannot_overwrite_a_grant_before_its_metadata_binds() {
    let h = Harness::new();
    let hold = h.profile.join("verification-google-auth-hold-store");
    std::fs::write(&hold, []).unwrap();
    h.step(1, "personal", "success", "full", false);
    let first = h.begin(None, None).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if h.fixture.report(false)["account_fixture"]["vault_store_attempts"] == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    h.step(2, "personal", "success", "mail_readonly", true);
    let refusal = h
        .auth
        .handle(
            "other-owner",
            CancellationToken::new(),
            "google_auth_begin",
            None,
            json!({"client_config_path":h.profile.join("verification-google-client.json")}),
        )
        .await
        .unwrap_err();
    assert_eq!(refusal.code, "login_in_progress");
    assert_eq!(
        h.fixture.report(false)["account_fixture"]["consumed_steps"],
        json!([1])
    );
    assert_eq!(
        h.fixture.report(false)["account_fixture"]["vault_store_attempts"],
        1
    );
    std::fs::remove_file(&hold).unwrap();
    assert_eq!(h.completed(&first).await["phase"], "connected");
    let bindings = h.core.call(|s| s.google_account_bindings()).await.unwrap();
    assert_eq!(bindings.len(), 1);
    let accounts = h
        .core
        .call(|s| s.handle("accounts", None, json!({})))
        .await
        .unwrap();
    assert_eq!(accounts["accounts"][0]["email"], "personal@example.test");
    assert!(bindings[0]
        .scopes
        .contains("https://www.googleapis.com/auth/gmail.modify"));
    assert_eq!(
        h.fixture.report(false)["account_fixture"]["vault_store_completed"],
        1
    );
    h.close().await;
}
