use super::*;
use crate::google_sync::Runtime;

const BASE: u64 = 1_789_257_600;

#[tokio::test]
async fn owned_occurrence_runtime_verifies_moved_date_and_sparse_cancelled_identity_without_replay()
{
    let directory = tempfile::tempdir().unwrap();
    let profile = fm_ipc::private_profile(&directory.path().join("profile")).unwrap();
    claim_empty_profile(&profile).unwrap();
    let mut store = Store::open_live(&profile).unwrap();
    let fixture = Fixture::prepare_with_occurrences(
        &profile,
        &mut store,
        false,
        false,
        false,
        None,
        None,
        Some(BASE),
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
                "occurrence-owner",
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
                        "occurrence-owner",
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
    for account in fixture.accounts.iter() {
        let cal = calendar_invitations::calendar(account.key);
        let original_master:String=db.query_row("SELECT json FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id='co0000'",[&account.id,&cal],|row|row.get(0)).unwrap();
        let call = |method: &'static str, params: Value| {
            runtime.handle(
                "occurrence-owner",
                owner.clone(),
                method,
                Some(&account.id),
                params,
            )
        };
        for (index, kind, lost) in [
            (0, "edit", false),
            (1, "edit", true),
            (4, "delete", true),
            (5, "delete", false),
        ] {
            let original =
                chrono::DateTime::from_timestamp((BASE + 3600 + index * 86400) as i64, 0)
                    .unwrap()
                    .to_rfc3339();
            let selector = json!({"calendar_id":cal,"series_id":"co0000","original_start":{"kind":"zoned","instant":original,"time_zone":"UTC"}});
            let resolved = call("google_calendar_occurrence_read", selector)
                .await
                .unwrap();
            assert_eq!(resolved["account"], account.id);
            assert_eq!(resolved["calendar_id"], cal);
            assert_eq!(resolved["event_id"], format!("oi{index:04}"));
            assert_eq!(
                resolved["occurrence"]["expected_master_etag"],
                "\"co0000-v1\""
            );
            let mut request = json!({"request_id":format!("{kind}-{index}"),"kind":kind,"calendar_id":cal,"event_id":resolved["event_id"],"expected_etag":resolved["etag"],"occurrence":resolved["occurrence"]});
            if kind == "edit" {
                request["fields"] = json!({"title":format!("Changed {account_key} {index}",account_key=account.key)});
            }
            let receipt = call("google_calendar_write", request.clone())
                .await
                .unwrap();
            assert_eq!(receipt["intent_version"], 3);
            assert_eq!(
                receipt["state"],
                if lost { "unknown" } else { "acknowledged" },
                "{receipt}"
            );
            let terminal = if lost {
                let reconciled = call(
                    "google_calendar_reconcile",
                    json!({"request_id":request["request_id"]}),
                )
                .await
                .unwrap();
                assert_eq!(reconciled["state"], "reconciled", "{reconciled}");
                assert_eq!(reconciled["outcome"]["kind"], "observed_state");
                reconciled
            } else {
                receipt
            };
            let count = fixture.evidence.lock().unwrap().calendar_publications.len();
            assert_eq!(
                call("google_calendar_write", request).await.unwrap()["state"],
                terminal["state"]
            );
            assert_eq!(
                fixture.evidence.lock().unwrap().calendar_publications.len(),
                count
            );
            let publication = fixture
                .evidence
                .lock()
                .unwrap()
                .calendar_publications
                .last()
                .unwrap()
                .clone();
            assert_eq!(publication["account"], account.id);
            assert_eq!(publication["event"], format!("oi{index:04}"));
            assert_eq!(publication["query"]["sendUpdates"], "all");
            assert_eq!(publication["before"]["recurringEventId"], "co0000");
            if kind == "edit" {
                for field in [
                    "description",
                    "location",
                    "conferenceData",
                    "attachments",
                    "attendees",
                    "reminders",
                    "extendedProperties",
                    "start",
                    "end",
                    "originalStartTime",
                ] {
                    assert_eq!(
                        publication["before"][field], publication["after"][field],
                        "{field}"
                    );
                }
            } else {
                assert_eq!(
                    publication["after"],
                    calendar_occurrences::cancelled(&publication["before"])
                );
            }
        }
        // All-day fallback traverses both pages, including a sparse cancellation.
        let day = chrono::DateTime::from_timestamp(BASE as i64, 0)
            .unwrap()
            .date_naive()
            + chrono::Duration::days(3);
        let resolved=call("google_calendar_occurrence_read",json!({"calendar_id":cal,"series_id":"co0001","original_start":{"kind":"date","date":day.to_string()}})).await.unwrap();
        assert_eq!(resolved["event_id"], "od0002");
        let cancelled_day = day - chrono::Duration::days(1);
        assert!(call("google_calendar_occurrence_read",json!({"calendar_id":cal,"series_id":"co0001","event_id":"od0001","original_start":{"kind":"date","date":cancelled_day.to_string()}})).await.is_err());
        let master_after:String=db.query_row("SELECT json FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id='co0000'",[&account.id,&cal],|row|row.get(0)).unwrap();
        assert_eq!(master_after, original_master);
    }
    runtime.shutdown().await;
    drop(runtime);
    auth.shutdown().await;
    drop(auth);
    drop(core);
    writer.join().unwrap();
    let report = fixture.report(true);
    assert_eq!(report["scenario"], "google-calendar-occurrences");
    assert_eq!(report["counters"]["calendar_mutation_attempts"], 8);
    assert_eq!(report["counters"]["calendar_mutations"], 8);
    assert_eq!(report["counters"]["calendar_lost_responses"], 4);
    assert_eq!(report["counters"]["calendar_acknowledged"], 4);
    assert_eq!(report["counters"]["raw_started"], 0);
    assert_eq!(report["counters"]["vault_stores"], 0);
    assert_eq!(report["errors"], json!([]));
    assert!(report["calendar_fixture"]["reads"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "instances")
        .all(|r| r["query"].get("timeMin").is_none() && r["query"].get("timeMax").is_none()));
}
