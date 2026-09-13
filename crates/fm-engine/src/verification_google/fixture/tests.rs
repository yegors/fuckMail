use super::*;
use fm_google::{
    gmail_api::{GmailApi, GmailCall, GmailReply, RawOutcome},
    transport::{HttpMethod, HttpRequest, HttpTransport},
    AccountId,
};

fn setup() -> (tempfile::TempDir, Store, Fixture) {
    let directory = tempfile::tempdir().unwrap();
    let profile = fm_ipc::private_profile(&directory.path().join("profile")).unwrap();
    claim_empty_profile(&profile).unwrap();
    let mut store = Store::open_live(&profile).unwrap();
    let fixture = Fixture::prepare(&profile, &mut store).unwrap();
    (directory, store, fixture)
}
fn api(fixture: &Fixture, index: usize) -> GmailApi<FixtureHttp> {
    GmailApi::new(
        AccountId::new(fixture.accounts[index].id.clone()).unwrap(),
        fixture.http.clone(),
    )
}
fn account(fixture: &Fixture, index: usize) -> AccountId {
    AccountId::new(fixture.accounts[index].id.clone()).unwrap()
}
fn limits() -> Value {
    json!({"gmail_project_units_per_minute":1_000_000,"gmail_account_units_per_minute":500_000,"calendar_project_units_per_minute":10_000,"calendar_account_units_per_minute":5_000})
}

#[test]
fn empty_claim_and_bootstrap_never_adopt_existing_data_or_seed_mail() {
    let directory = tempfile::tempdir().unwrap();
    let occupied = fm_ipc::private_profile(&directory.path().join("occupied")).unwrap();
    std::fs::write(occupied.join("authored.txt"), b"retained").unwrap();
    assert!(claim_empty_profile(&occupied).is_err());
    assert!(!occupied.join(MARKER).exists());
    assert_eq!(
        std::fs::read(occupied.join("authored.txt")).unwrap(),
        b"retained"
    );
    let (_directory, mut store, fixture) = setup();
    assert!(claim_empty_profile(&fixture.profile).is_err());
    assert!(super::super::refuse_marked_profile(&fixture.profile).is_err());
    let status = store.handle("status", None, json!({})).unwrap();
    assert_eq!(status["fixture_revision"], 0);
    for owner in fixture.accounts.iter() {
        assert_eq!(
            store
                .handle("conversations", Some(&owner.id), json!({}))
                .unwrap()["conversations"],
            json!([])
        );
    }
    assert_eq!(fixture.report(false)["status"], "ready");
    assert_eq!(fixture.report(false)["counters"]["http_started"], 0);
    assert_eq!(fixture.report(false)["counters"]["vault_loads"], 0);
}

#[tokio::test]
async fn actual_gmail_adapter_routes_keep_account_identity_pages_and_review_readback() {
    let (_directory, _store, fixture) = setup();
    let cancel = CancellationToken::new();
    let first = account(&fixture, 0);
    let token = fixture.accounts[0].token();
    let api = api(&fixture, 0);
    let GmailReply::Profile(profile) = api
        .execute(GmailCall::profile(&first).unwrap(), &token, &cancel)
        .await
        .unwrap()
    else {
        panic!("profile reply");
    };
    assert_eq!(profile.email_address, "personal@example.test");
    assert_eq!(profile.messages_total, 4156);
    let GmailReply::Messages(recent) = api
        .execute(
            GmailCall::list_messages(&first, None, Some("after:100"), true, 100).unwrap(),
            &token,
            &cancel,
        )
        .await
        .unwrap()
    else {
        panic!("recent list");
    };
    assert_eq!(recent.messages.len(), 60);
    assert_eq!(recent.messages[0].id, "vr0000");
    assert_eq!(recent.messages[0].thread_id, recent.messages[1].thread_id);
    assert!(recent.next_page_token.is_none());
    let GmailReply::Messages(initial) = api
        .execute(
            GmailCall::list_messages(&first, None, None, true, 100).unwrap(),
            &token,
            &cancel,
        )
        .await
        .unwrap()
    else {
        panic!("initial inventory");
    };
    assert_eq!(initial.messages.len(), 100);
    assert_eq!(initial.next_page_token.as_deref(), Some("inventory-100"));
    let GmailReply::Messages(next) = api
        .execute(
            GmailCall::list_messages(&first, initial.next_page_token.as_deref(), None, true, 100)
                .unwrap(),
            &token,
            &cancel,
        )
        .await
        .unwrap()
    else {
        panic!("next inventory");
    };
    assert_eq!(next.messages[0].id, "vb0040");
    let mut bodies = Vec::new();
    for index in 0..2 {
        let GmailReply::Raw(RawOutcome::Available(raw)) = super::tests::api(&fixture, index)
            .execute(
                GmailCall::raw_message(&account(&fixture, index), "vr0001").unwrap(),
                &fixture.accounts[index].token(),
                &cancel,
            )
            .await
            .unwrap()
        else {
            panic!("RAW reply");
        };
        let bytes = base64::Engine::decode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            &raw.message.raw,
        )
        .unwrap();
        let source = String::from_utf8(bytes).unwrap();
        assert!(source.contains(&format!(
            "To: {}@example.test\r\n",
            fixture.accounts[index].key
        )));
        assert!(source.contains("\r\nSubject: Verification "));
        assert!(source.contains(
            "recent/0001. This fixed source exists only in the disposable acquisition fixture."
        ));
        assert_eq!(raw.message.id, "vr0001");
        assert_eq!(raw.message.thread_id, "tr0000");
        bodies.push(source);
    }
    assert_ne!(bodies[0], bodies[1]);
    api.execute(
        GmailCall::modify_message(&first, "vr0001", &[], &["UNREAD".into()]).unwrap(),
        &token,
        &cancel,
    )
    .await
    .unwrap();
    let GmailReply::MessageState(Some(reviewed)) = api
        .execute(
            GmailCall::message_state(&first, "vr0001").unwrap(),
            &token,
            &cancel,
        )
        .await
        .unwrap()
    else {
        panic!("state");
    };
    assert_eq!(reviewed.label_ids, vec!["INBOX"]);
    assert_eq!(reviewed.history_id, "101");
    let GmailReply::MessageState(Some(other)) = super::tests::api(&fixture, 1)
        .execute(
            GmailCall::message_state(&account(&fixture, 1), "vr0001").unwrap(),
            &fixture.accounts[1].token(),
            &cancel,
        )
        .await
        .unwrap()
    else {
        panic!("other state");
    };
    assert!(other.label_ids.contains(&"UNREAD".into()));
    let GmailReply::History(history) = api
        .execute(
            GmailCall::history(&first, "100", None, 100).unwrap(),
            &token,
            &cancel,
        )
        .await
        .unwrap()
    else {
        panic!("history");
    };
    assert_eq!(history.history.len(), 1);
    assert_eq!(history.history[0].labels_removed[0].message.id, "vr0001");
    assert!(fixture.evidence.lock().unwrap().errors.is_empty());
}

#[tokio::test]
async fn unknown_tokens_oauth_origins_and_unsupported_writes_fail_without_fallback() {
    let (_directory, _store, fixture) = setup();
    for (method,url,token,body) in [
        (HttpMethod::Get,"https://gmail.googleapis.com/gmail/v1/users/me/profile","unknown",None),
        (HttpMethod::Post,"https://oauth2.googleapis.com/token","known",Some(json!({}))),
        (HttpMethod::Post,"https://gmail.googleapis.com/gmail/v1/users/me/messages/vr0001/modify","known",Some(json!({"addLabelIds":["STARRED"],"removeLabelIds":[]}))),
        (HttpMethod::Get,"https://gmail.googleapis.com/gmail/v1/users/me/messages?maxResults=100&includeSpamTrash=true&pageToken=inventory-999999","known",None),
    ] {
        let token = if token=="known" {fixture.accounts[0].token()} else {token.into()};
        let mut request=HttpRequest::new(method,url).unwrap().header("authorization",&format!("Bearer {token}")).unwrap();
        if let Some(body)=body {request=request.body(serde_json::to_vec(&body).unwrap(),"application/json").unwrap();}
        assert!(fixture.http.execute(request,&CancellationToken::new()).await.is_err());
    }
    let report = fixture.report(false);
    assert_eq!(report["counters"]["http_rejected"], 4);
    assert_eq!(report["counters"]["http_completed"], 0);
    assert_eq!(report["counters"]["modify_completed"], 0);
    assert_eq!(report["transport"], "in_memory_only");
    assert!(!report.to_string().contains("Bearer"));
}

#[tokio::test]
async fn held_raw_cancellation_balances_activity_without_success_or_publication() {
    let (_directory, _store, fixture) = setup();
    fixture.hold_raw.store(true, Ordering::Release);
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();
    let task_fixture = fixture.clone();
    let task = tokio::spawn(async move {
        api(&task_fixture, 0)
            .execute(
                GmailCall::raw_message(&account(&task_fixture, 0), "vb0000").unwrap(),
                &task_fixture.accounts[0].token(),
                &task_cancel,
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        while fixture.evidence.lock().unwrap().counters.raw_held_active == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    cancel.cancel();
    assert!(task.await.unwrap().is_err());
    let report = fixture.report(false);
    assert_eq!(report["counters"]["raw_cancelled"], 1);
    assert_eq!(report["counters"]["http_cancelled"], 1);
    assert_eq!(report["counters"]["raw_active"], 0);
    assert_eq!(report["counters"]["raw_held_active"], 0);
    assert_eq!(report["counters"]["raw_completed"], 0);
    assert_eq!(report["publication_count"], 0);
    assert!(fixture.evidence.lock().unwrap().errors.is_empty());
}

#[tokio::test]
async fn actual_runtime_uses_cached_tokens_and_traces_only_committed_writer_parts() {
    for reject_commit in [false, true] {
        let (_directory, mut store, fixture) = setup();
        if reject_commit {
            let db = rusqlite::Connection::open(fixture.profile.join("store.sqlite3")).unwrap();
            db.execute_batch("CREATE TRIGGER refuse_verified_part BEFORE UPDATE OF applied_revision ON google_live_page_items WHEN NEW.applied_revision IS NOT NULL BEGIN SELECT RAISE(ABORT,'fixture commit refusal'); END").unwrap();
        }
        let (send, mut receive) = tokio::sync::mpsc::channel(32);
        let writer_fixture = fixture.clone();
        let writer = std::thread::spawn(move || {
            let _trace = writer_fixture.install_writer_trace();
            while let Some(work) = receive.blocking_recv() {
                if let crate::Work::Provider(call) = work {
                    call.run(&mut store);
                } else {
                    panic!("provider work only");
                }
            }
        });
        let core = CoreBridge::new(send, Arc::new(tokio::sync::Semaphore::new(16)));
        let cancel = CancellationToken::new();
        let auth = fixture.auth(core.clone(), cancel.clone());
        assert_eq!(
            auth.handle(
                "owner",
                CancellationToken::new(),
                "google_auth_begin",
                None,
                json!({})
            )
            .await
            .unwrap_err()
            .code,
            "verification_refused"
        );
        let selected = fixture.accounts[0].id.clone();
        assert_eq!(
            auth.access_token(&selected, &cancel).await.unwrap(),
            fixture.accounts[0].token()
        );
        let runtime = crate::google_sync::Runtime::new(core.clone(), auth.clone(), cancel.clone());
        runtime
            .handle(
                "owner",
                CancellationToken::new(),
                "google_sync_start",
                Some(&selected),
                limits(),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                let status = runtime
                    .handle(
                        "owner",
                        CancellationToken::new(),
                        "google_sync_status",
                        Some(&selected),
                        json!({}),
                    )
                    .await
                    .unwrap();
                if reject_commit {
                    if status["accounts"][0]["progress"]["mail"] == "paused" {
                        break;
                    }
                } else if !fixture.evidence.lock().unwrap().publications.is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        runtime.shutdown().await;
        drop(runtime);
        auth.shutdown().await;
        drop(auth);
        drop(core);
        writer.join().unwrap();
        fixture.finish(true).await.unwrap();
        let report = fixture.report(true);
        assert!(report["counters"]["vault_loads"].as_u64().unwrap() > 0);
        assert_eq!(report["counters"]["vault_stores"], 0);
        let db = rusqlite::Connection::open(fixture.profile.join("store.sqlite3")).unwrap();
        let mut statement=db.prepare("SELECT account,part_key,applied_revision,count(*) FROM google_live_page_items WHERE applied_revision IS NOT NULL GROUP BY account,part_key,applied_revision ORDER BY part_key").unwrap();
        let receipts = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, u64>(3)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let mut observed = report["publications"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                (
                    row["account"].as_str().unwrap().to_owned(),
                    row["receipt_key"].as_str().unwrap().to_owned(),
                    row["revision"].as_i64().unwrap(),
                    row["item_count"].as_u64().unwrap(),
                )
            })
            .collect::<Vec<_>>();
        observed.sort_by(|left, right| left.1.cmp(&right.1));
        assert_eq!(observed, receipts);
        let count: i64 = db
            .query_row("SELECT count(*) FROM messages", [], |row| row.get(0))
            .unwrap();
        if reject_commit {
            assert!(observed.is_empty());
            assert_eq!(count, 0);
        } else {
            assert!(!observed.is_empty());
            assert!(count >= 10);
        }
    }
}
