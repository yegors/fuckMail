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

#[tokio::test]
async fn owned_recurring_v4_series_rsvp_create_following_and_cancel_workflow() {
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
    store.handle("preferences_save",None,json!({"request_id":"recurring-display-zone","expected_revision":0,"preferences":{"calendar_display_time_zone":"UTC"}})).unwrap();
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
        let call = |method: &'static str, params: Value| {
            runtime.handle(
                "occurrence-owner",
                owner.clone(),
                method,
                Some(&account.id),
                params,
            )
        };
        let original = chrono::DateTime::from_timestamp((BASE + 3600) as i64, 0)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let selected = json!({"calendar_id":cal,"series_id":"co0000","original_start":{"kind":"zoned","instant":original,"time_zone":"UTC"},"event_id":"oi0000","scope":"series","action":"edit"});
        let resolved = call("google_calendar_occurrence_read", selected)
            .await
            .unwrap();
        assert_eq!(resolved["event_id"], "co0000");
        let edit = json!({"request_id":"v4-series","kind":"edit","calendar_id":cal,"event_id":resolved["event_id"],"expected_etag":resolved["etag"],"recurring":resolved["recurring"],"fields":{"title":"Updated whole series"}});
        let edited = call("google_calendar_write", edit.clone()).await.unwrap();
        assert_eq!(edited["state"], "acknowledged");
        assert_eq!(edited["intent_version"], 4);
        assert_eq!(call("google_calendar_write", edit).await.unwrap(), edited);
        let selected = json!({"calendar_id":cal,"series_id":"co0000","original_start":{"kind":"zoned","instant":original,"time_zone":"UTC"},"event_id":"oi0000","scope":"occurrence","action":"rsvp"});
        let resolved = call("google_calendar_occurrence_read", selected)
            .await
            .unwrap();
        assert_eq!(resolved["event"]["title"], "Updated whole series");
        let rsvp = json!({"request_id":"v4-rsvp","kind":"rsvp","calendar_id":cal,"event_id":resolved["event_id"],"expected_etag":resolved["etag"],"recurring":resolved["recurring"],"response":"declined"});
        assert_eq!(
            call("google_calendar_write", rsvp).await.unwrap()["state"],
            "acknowledged"
        );
        let page = core
            .call(|store| {
                store.handle(
                    "calendar_combined",
                    None,
                    json!({"from":"2026-09-13","to":"2026-09-20"}),
                )
            })
            .await
            .unwrap();
        let rows: Vec<_> = page["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| {
                row["account"] == account.id && row["occurrence_key"]["series"] == "co0000"
            })
            .collect();
        assert_eq!(
            rows.len(),
            5,
            "The moved occurrence remains outside this window"
        );
        let exact_original = chrono::DateTime::parse_from_rfc3339(&original)
            .unwrap()
            .to_rfc3339();
        for row in rows {
            assert_eq!(
                row["self_response"],
                if row["occurrence_key"]["original_start"] == exact_original {
                    "declined"
                } else {
                    "accepted"
                }
            );
            assert_eq!(row["title"], "Updated whole series");
        }
        let create = json!({"request_id":"v4-create","kind":"create","calendar_id":cal,"fields":{"title":"Fresh simple series","start":"2026-09-14T09:00:00Z","end":"2026-09-14T10:00:00Z","all_day":false,"time_zone":"UTC","recurrence":["RRULE:FREQ=DAILY;COUNT=3"]}});
        let created = call("google_calendar_write", create.clone()).await.unwrap();
        assert_eq!(created["state"], "acknowledged");
        let series = created["target"]["event"].as_str().unwrap();
        assert!(series.starts_with('f'));
        let selected = json!({"calendar_id":cal,"series_id":series,"original_start":{"kind":"zoned","instant":"2026-09-15T09:00:00Z","time_zone":"UTC"},"scope":"following","action":"edit"});
        let resolved = call("google_calendar_occurrence_read", selected)
            .await
            .unwrap();
        assert!(resolved["following_notice"]
            .as_str()
            .unwrap()
            .contains("exceptions"));
        let mut context = resolved["recurring"].clone();
        context["reset_future_exceptions"] = true.into();
        let split = json!({"request_id":"v4-following","kind":"edit","calendar_id":cal,"event_id":resolved["event_id"],"expected_etag":resolved["etag"],"recurring":context,"fields":{"title":"Following human edit"}});
        let completed = call("google_calendar_write", split.clone()).await.unwrap();
        assert_eq!(completed["state"], "acknowledged");
        assert_eq!(completed["following_complete"], true);
        assert_eq!(completed["can_resume"], false);
        assert_eq!(completed["steps"].as_array().unwrap().len(), 2);
        assert_eq!(
            call("google_calendar_write", split).await.unwrap(),
            completed
        );
        let replacement = completed["dispatch_event_id"].as_str().unwrap();
        assert_ne!(replacement, series);
        let raw:String=db.query_row("SELECT json FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",[&account.id,&cal,replacement],|r|r.get(0)).unwrap();
        let actual: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(actual["summary"], "Following human edit");
        assert_eq!(actual["recurrence"], json!(["RRULE:COUNT=2;FREQ=DAILY"]));
        let raw:String=db.query_row("SELECT json FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id=?3",[&account.id,&cal,series],|r|r.get(0)).unwrap();
        let trimmed: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(trimmed["recurrence"], json!(["RRULE:COUNT=1;FREQ=DAILY"]));
        assert_eq!(trimmed["summary"], "Fresh simple series");
        let delete = json!({"request_id":"v4-cancel-series","kind":"delete","calendar_id":cal,"event_id":replacement,"expected_etag":actual["etag"],"recurring":{"scope":"series","series_id":replacement,"expected_master_etag":actual["etag"]}});
        assert_eq!(
            call("google_calendar_write", delete).await.unwrap()["state"],
            "acknowledged"
        );
    }
    runtime.shutdown().await;
    drop(runtime);
    auth.shutdown().await;
    drop(auth);
    drop(core);
    writer.join().unwrap();
    let report = fixture.report(true);
    assert_eq!(report["errors"], json!([]));
    assert_eq!(report["counters"]["calendar_mutation_attempts"], 12);
    assert_eq!(report["counters"]["calendar_acknowledged"], 12);
    assert_eq!(report["counters"]["raw_started"], 0);
    assert_eq!(report["counters"]["vault_stores"], 0);
    let publications = report["calendar_fixture"]["publications"]
        .as_array()
        .unwrap();
    for account in fixture.accounts.iter() {
        let entries = publications
            .iter()
            .filter(|p| p["account"] == account.id)
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 6);
        assert_eq!(
            entries[0]["body"],
            json!({"summary":"Updated whole series","eventLabelId":"fixture-label"})
        );
        assert_eq!(
            entries[0]["before"]["conferenceData"],
            entries[0]["after"]["conferenceData"]
        );
        assert_eq!(
            entries[0]["before"]["attachments"],
            entries[0]["after"]["attachments"]
        );
        assert_eq!(
            entries[1]["body"]["attendees"],
            json!([{"email":account.email,"responseStatus":"declined"}])
        );
        assert_eq!(
            entries[1]["before"]["attendees"][1],
            entries[1]["after"]["attendees"][1]
        );
        assert_eq!(entries[3]["method"], "PATCH");
        assert_eq!(
            entries[3]["body"],
            json!({"recurrence":["RRULE:COUNT=1;FREQ=DAILY"]})
        );
        assert_eq!(entries[4]["method"], "POST");
        assert_eq!(
            entries[4]["body"]["recurrence"],
            json!(["RRULE:COUNT=2;FREQ=DAILY"])
        );
    }
}
