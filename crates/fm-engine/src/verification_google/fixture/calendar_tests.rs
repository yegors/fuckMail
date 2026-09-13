use super::*;
use crate::google_sync::Runtime;

#[tokio::test]
async fn calendar_only_owned_runtime_imports_two_accounts_and_preserves_invitation_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let profile = fm_ipc::private_profile(&directory.path().join("profile")).unwrap();
    claim_empty_profile(&profile).unwrap();
    let mut store = Store::open_live(&profile).unwrap();
    let fixture = Fixture::prepare_with_calendar(
        &profile,
        &mut store,
        false,
        false,
        false,
        Some(1_789_300_800),
    )
    .unwrap();
    let (queue, mut incoming) = tokio::sync::mpsc::channel(32);
    let writer = std::thread::spawn(move || {
        while let Some(work) = incoming.blocking_recv() {
            if let crate::Work::Provider(call) = work {
                call.run(&mut store)
            } else {
                panic!("provider work only")
            }
        }
    });
    let core = CoreBridge::new(queue, Arc::new(tokio::sync::Semaphore::new(16)));
    let cancel = CancellationToken::new();
    let auth = fixture.auth(core.clone(), cancel.clone());
    let runtime = Runtime::new(core.clone(), auth.clone(), cancel.clone());
    let owner = CancellationToken::new();
    for account in fixture.accounts.iter() {
        runtime
            .handle(
                "invitation-owner",
                owner.clone(),
                "google_sync_start",
                Some(&account.id),
                super::tests::limits(),
            )
            .await
            .unwrap();
    }
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let mut ready = true;
            for account in fixture.accounts.iter() {
                let status = runtime
                    .handle(
                        "invitation-owner",
                        owner.clone(),
                        "google_sync_status",
                        Some(&account.id),
                        json!({}),
                    )
                    .await
                    .unwrap();
                assert_eq!(status["accounts"][0]["progress"]["mail"], "not_granted");
                let phase = &status["accounts"][0]["progress"]["calendar"];
                assert_ne!(phase, "paused", "{status}");
                assert_ne!(phase, "partial", "{status}");
                ready &= phase == "waiting";
            }
            if ready {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let db = rusqlite::Connection::open(profile.join("store.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM calendar_visible_sources", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap(),
        14
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM messages", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    for account in fixture.accounts.iter() {
        let cal = calendar_invitations::calendar(account.key);
        let call = |method: &'static str, params: Value| {
            runtime.handle(
                "invitation-owner",
                owner.clone(),
                method,
                Some(&account.id),
                params,
            )
        };
        let mut etag = "\"ci0000-v1\"".to_owned();
        for (index, guests) in [
            json!(["alpha@example.test", "beta@example.test"]),
            json!(["alpha@example.test"]),
            json!(["alpha@example.test", "beta@example.test"]),
            json!([]),
        ]
        .into_iter()
        .enumerate()
        {
            let params = json!({"request_id":format!("roster-{index}"),"kind":"edit","calendar_id":cal,"event_id":"ci0000","expected_etag":etag,"fields":{"attendees":guests}});
            let result = call("google_calendar_write", params).await.unwrap();
            assert_eq!(result["state"], "acknowledged", "{result}");
            assert_eq!(result["intent_version"], 2);
            assert_eq!(result["outcome"]["evidence_origin"], "provider_response");
            assert_eq!(result["outcome"]["notification_delivery"], "unverified");
            let publication = fixture
                .evidence
                .lock()
                .unwrap()
                .calendar_publications
                .last()
                .unwrap()
                .clone();
            assert_eq!(publication["query"]["eventLabelVersion"], "1");
            assert_eq!(publication["expected_quota_units"], 3);
            assert_eq!(publication["query"]["sendUpdates"], "all");
            if let Some(rows) = publication["body"]["attendees"].as_array() {
                for row in rows {
                    for key in ["id", "self", "organizer", "asyncOperation"] {
                        assert!(
                            row.get(key).is_none(),
                            "read-only attendee {key} on PATCH wire"
                        );
                    }
                }
            }
            let before = &publication["before"];
            let after = &publication["after"];
            for key in [
                "conferenceData",
                "attachments",
                "reminders",
                "extendedProperties",
                "organizer",
                "creator",
                "colorId",
                "eventLabelId",
            ] {
                assert_eq!(before[key], after[key], "{key}");
            }
            if index == 0 {
                assert!(publication["body"].get("attendees").is_none());
            }
            let rows = after["attendees"].as_array().unwrap();
            assert!(rows.iter().any(|r| r["email"] == account.email
                && r["organizer"] == true
                && r["self"] == true));
            if index < 3 {
                assert_eq!(
                    rows.iter()
                        .find(|r| r["email"] == "alpha@example.test")
                        .unwrap()["comment"],
                    "Preserved reply"
                );
            }
            if index == 2 {
                assert_eq!(
                    rows.iter()
                        .find(|r| r["email"] == "beta@example.test")
                        .unwrap()["responseStatus"],
                    "needsAction"
                );
            }
            if index == 3 {
                assert_eq!(rows.len(), 1);
            }
            etag = after["etag"].as_str().unwrap().into();
        }
        let before_count = fixture.evidence.lock().unwrap().calendar_publications.len();
        for event in ["ci0002", "ci0003"] {
            let error=call("google_calendar_write",json!({"request_id":format!("refuse-{event}"),"kind":"edit","calendar_id":cal,"event_id":event,"expected_etag":format!("\"{event}-v1\""),"fields":{"attendees":[]}})).await.unwrap_err();
            assert_eq!(error.code, "incomplete_attendees");
        }
        assert_eq!(
            fixture.evidence.lock().unwrap().calendar_publications.len(),
            before_count
        );
        let title=call("google_calendar_write",json!({"request_id":"omitted-title","kind":"edit","calendar_id":cal,"event_id":"ci0002","expected_etag":"\"ci0002-v1\"","fields":{"title":"Title with untouched incomplete roster"}})).await.unwrap();
        assert_eq!(title["state"], "acknowledged");
        for (event, kind) in [("ci0001", "edit"), ("ci0004", "delete")] {
            let mut params = json!({"request_id":format!("lost-{event}"),"kind":kind,"calendar_id":cal,"event_id":event,"expected_etag":format!("\"{event}-v1\"" )});
            if kind == "edit" {
                params["fields"] = json!({"title":"Observed after lost response","attendees":["alpha@example.test"]});
            }
            assert_eq!(
                call("google_calendar_write", params.clone()).await.unwrap()["state"],
                "unknown"
            );
            let count = fixture.evidence.lock().unwrap().calendar_publications.len();
            let receipt = call(
                "google_calendar_reconcile",
                json!({"request_id":params["request_id"]}),
            )
            .await
            .unwrap();
            assert_eq!(receipt["state"], "reconciled", "{receipt}");
            assert_eq!(receipt["outcome"]["kind"], "observed_state");
            assert_eq!(receipt["outcome"]["notification_delivery"], "unverified");
            assert_eq!(receipt["outcome"]["cache_published"], true, "{receipt}");
            let observed = fixture
                .evidence
                .lock()
                .unwrap()
                .calendar_publications
                .last()
                .unwrap()["after"]
                .clone();
            let retained: String = db.query_row("SELECT json FROM calendar_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",
                [&account.id, &cal, event], |row| row.get(0)).unwrap();
            assert_eq!(serde_json::from_str::<Value>(&retained).unwrap(), observed);
            let visible: i64 = db.query_row("SELECT count(*) FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",
                [&account.id, &cal, event], |row| row.get(0)).unwrap();
            assert_eq!(visible, if kind == "delete" { 0 } else { 1 });
            assert_eq!(
                call("google_calendar_write", params).await.unwrap()["state"],
                "reconciled"
            );
            assert_eq!(
                fixture.evidence.lock().unwrap().calendar_publications.len(),
                count
            );
        }
        assert_eq!(call("google_calendar_write",json!({"request_id":"delete","kind":"delete","calendar_id":cal,"event_id":"ci0005","expected_etag":"\"ci0005-v1\""})).await.unwrap()["state"],"acknowledged");
        assert_eq!(call("google_calendar_write",json!({"request_id":"rsvp","kind":"rsvp","calendar_id":cal,"event_id":"ci0006","expected_etag":"\"ci0006-v1\"","response":"tentative"})).await.unwrap()["state"],"acknowledged");
        let publication = fixture
            .evidence
            .lock()
            .unwrap()
            .calendar_publications
            .last()
            .unwrap()
            .clone();
        assert_eq!(
            publication["body"],
            json!({"attendeesOmitted":true,"attendees":[{"email":account.email,"responseStatus":"tentative"}]})
        );
        let before = publication["before"]["attendees"].as_array().unwrap();
        let after = publication["after"]["attendees"].as_array().unwrap();
        assert_eq!(before.len(), after.len());
        assert_eq!(&before[1..], &after[1..]);
    }
    runtime.shutdown().await;
    drop(runtime);
    auth.shutdown().await;
    drop(auth);
    drop(core);
    writer.join().unwrap();
    let report = fixture.report(true);
    assert_eq!(report["counters"]["calendar_mutations"], 18);
    assert_eq!(report["counters"]["calendar_lost_responses"], 4);
    assert_eq!(report["counters"]["calendar_acknowledged"], 14);
    assert_eq!(report["counters"]["raw_started"], 0);
    assert_eq!(report["counters"]["vault_stores"], 0);
    assert!(report["errors"].as_array().unwrap().is_empty(), "{report}");
}
