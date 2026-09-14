use super::*;
use crate::{core_tasks::CoreBridge, google_sync::SyncContext};
use fm_core::live_calendar::{CalendarPage, CalendarSyncMode};
use fm_google::{
    transport::{
        CancellationToken, HttpFuture, HttpRequest, HttpResponse, HttpTransport, TransportError,
    },
    AccountId,
};
use serde_json::json;
use std::{
    collections::{BTreeSet, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{mpsc, Semaphore};
const CAL: &str = "human@example.test";
type ScriptReply = std::result::Result<(u16, Value), TransportError>;
type EventHold = (Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>);
#[derive(Default)]
struct Script {
    steps: Mutex<VecDeque<ScriptReply>>,
    requests: Mutex<Vec<(String, Value)>>,
    cancel_on_write: Mutex<bool>,
    event_barrier: Mutex<Option<Arc<tokio::sync::Barrier>>>,
    event_hold: Mutex<Option<EventHold>>,
    cooldown_after_event: Mutex<Option<Arc<crate::google_sync::quota::QuotaGate>>>,
}
impl Script {
    fn push(&self, status: u16, body: Value) {
        self.steps.lock().unwrap().push_back(Ok((status, body)));
    }
    fn preflight(&self) {
        self.push(200, json!({"id":CAL,"accessRole":"owner","timeZone":"UTC"}));
        self.push(200, event());
    }
}
impl HttpTransport for Script {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
        cancel: &'a CancellationToken,
    ) -> HttpFuture<'a> {
        Box::pin(async move {
            assert_eq!(request.headers()["authorization"], "Bearer synthetic-token");
            self.requests.lock().unwrap().push((
                format!("{:?} {}", request.method(), request.url()),
                serde_json::from_slice(request.body_bytes()).unwrap_or(Value::Null),
            ));
            let barrier = self.event_barrier.lock().unwrap().clone();
            if request.method() == fm_google::transport::HttpMethod::Get
                && request.url().contains("/events/")
            {
                let hold = self.event_hold.lock().unwrap().clone();
                if let Some((entered, release)) = hold {
                    entered.notify_one();
                    release.notified().await;
                }
                if let Some(barrier) = barrier {
                    barrier.wait().await;
                }
                if let Some(quota) = self.cooldown_after_event.lock().unwrap().clone() {
                    quota.cooldown(
                        fm_google::scheduler::CooldownScope::Project(
                            fm_google::scheduler::Service::Calendar,
                        ),
                        60,
                    );
                }
            }
            if request.method() == fm_google::transport::HttpMethod::Patch
                && *self.cancel_on_write.lock().unwrap()
            {
                cancel.cancel();
                return Err(TransportError::Cancelled);
            }
            let (status, body) = self
                .steps
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected request")?;
            Ok(HttpResponse {
                status,
                body: serde_json::to_vec(&body).unwrap(),
                retry_after_secs: None,
                etag: None,
            })
        })
    }
}
struct Harness {
    ctx: SyncContext,
    http: Arc<Script>,
    writer: std::thread::JoinHandle<()>,
}
impl Harness {
    fn new(path: &std::path::Path) -> Self {
        Self::with_subject(path, "write-worker-subject")
    }
    fn with_subject(path: &std::path::Path, subject: &str) -> Self {
        let path = path.to_owned();
        let subject = subject.to_owned();
        let (queue, mut receive) = mpsc::channel(8);
        let (ready, wait) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            let mut store = fm_core::Store::open_live(&path).unwrap();
            let scopes: BTreeSet<String> = [
                "openid",
                "email",
                "https://www.googleapis.com/auth/calendar.events",
                "https://www.googleapis.com/auth/calendar.calendarlist.readonly",
            ]
            .map(String::from)
            .into();
            let account = store
                .bind_google_account(&subject, CAL, &scopes, None)
                .unwrap();
            let catalog = store
                .calendar_catalog_begin(&account, CalendarSyncMode::Full)
                .unwrap();
            let page = CalendarPage {
                input_cursor: None,
                items: vec![json!({"id":CAL,"accessRole":"owner","timeZone":"UTC"})],
                next_page_token: None,
                next_sync_token: Some("catalog".into()),
            };
            store.calendar_stage_page_chunk(&catalog, &page, 0).unwrap();
            store.calendar_seal_page(&catalog, &page).unwrap();
            while store.calendar_catalog_finalize_chunk(&catalog).unwrap() > 0 {}
            store.calendar_publish_sync(&catalog).unwrap();
            let events = store
                .calendar_sync_begin(&account, CAL, CalendarSyncMode::Full)
                .unwrap();
            let page = CalendarPage {
                items: vec![event()],
                next_sync_token: Some("events".into()),
                ..page
            };
            store.calendar_stage_page_chunk(&events, &page, 0).unwrap();
            store.calendar_seal_page(&events, &page).unwrap();
            store.calendar_publish_sync(&events).unwrap();
            while store.calendar_finalize_chunk(&events).unwrap() > 0 {}
            ready.send(account).unwrap();
            while let Some(work) = receive.blocking_recv() {
                if let crate::Work::Provider(call) = work {
                    call.run(&mut store);
                } else {
                    panic!("unexpected test work")
                }
            }
        });
        let account = wait.recv_timeout(Duration::from_secs(10)).unwrap();
        let http = Arc::new(Script::default());
        let ctx = SyncContext::for_test(
            CoreBridge::new(queue, Arc::new(Semaphore::new(8))),
            AccountId::new(account).unwrap(),
            1,
            http.clone(),
            "synthetic-token".into(),
            CancellationToken::new(),
        );
        Self { ctx, http, writer }
    }
    fn close(self) {
        assert!(self.http.steps.lock().unwrap().is_empty());
        drop(self.ctx);
        self.writer.join().unwrap();
    }
}
fn event() -> Value {
    json!({"id":"event1","etag":"v1","summary":"Before","organizer":{"email":CAL,"self":true},"start":{"dateTime":"2026-09-12T09:00:00Z"},"end":{"dateTime":"2026-09-12T10:00:00Z"},"conferenceData":{"conferenceSolution":{"key":{"type":"hangoutsMeet"}},"signature":"retained","entryPoints":[]},"attachments":[{"fileUrl":"https://drive.example.test/file"}]})
}
fn request() -> Value {
    json!({"request_id":"edit","kind":"edit","calendar_id":CAL,"event_id":"event1","expected_etag":"v1","fields":{"title":"After"}})
}
#[tokio::test]
async fn conditional_edit_preserves_omitted_fields_and_completed_replay_has_no_http() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path());
    h.http.preflight();
    let mut updated = event();
    updated["etag"] = "v2".into();
    updated["summary"] = "After".into();
    h.http.push(200, updated);
    assert_eq!(
        execute(&h.ctx, request()).await.unwrap()["state"],
        "acknowledged"
    );
    assert_eq!(
        execute(&h.ctx, request()).await.unwrap()["state"],
        "acknowledged"
    );
    let requests = h.http.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[2].1, json!({"summary":"After"}));
    assert!(requests[2].0.starts_with("Patch"));
    drop(requests);
    h.close();
}
#[tokio::test]
async fn precondition_failure_keeps_intent_and_never_automatically_retries() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path());
    h.http.preflight();
    h.http.push(412, json!({}));
    assert_eq!(
        execute(&h.ctx, request()).await.unwrap()["state"],
        "conflict"
    );
    assert_eq!(
        execute(&h.ctx, request()).await.unwrap()["state"],
        "conflict"
    );
    assert_eq!(h.http.requests.lock().unwrap().len(), 3);
    h.close();
}
#[tokio::test]
async fn cooldown_learned_after_claim_returns_ready_without_provider_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path());
    // Reproduce a concurrent response introducing cooldown after this request's
    // quota reservation and exact Core claim, before the transport dispatch.
    let api = h.ctx.calendar_mutation_request(3).await.unwrap();
    let memory = Arc::new(h.ctx.calendar_prepare_permit().await.unwrap());
    let account = h.ctx.account.as_str().to_owned();
    let quota = h.ctx.quota.clone();
    let lease = memory_core(&h.ctx, &memory, move |store| {
        store.calendar_live_prepare(&account, &request())?;
        let epoch = store.calendar_checkpoint(&account, CAL)?["permission_epoch"]
            .as_i64()
            .unwrap();
        let lease = store.calendar_live_claim(&account, "edit", 0, 1, epoch, Some(event()))?;
        quota.cooldown(
            fm_google::scheduler::CooldownScope::Project(fm_google::scheduler::Service::Calendar),
            60,
        );
        Ok(lease)
    })
    .await
    .unwrap();
    let write = GoogleWrite {
        target: serde_json::from_value(lease.dispatch["target"].clone()).unwrap(),
        if_match: "v1".into(),
        body: lease.dispatch["body"].clone(),
        send_updates: "all",
        notification_owner: "google_calendar_api",
    };
    let error = api
        .apply_write("engine-managed", &write, &h.ctx.cancel)
        .await
        .err()
        .expect("cooldown must refuse dispatch");
    assert!(matches!(
        error,
        ApiError::Transport(TransportError::Cooldown { .. })
    ));
    assert_eq!(read_error(error.clone()).code, "google_rate_limited");
    assert!(h.http.requests.lock().unwrap().is_empty());
    let saved = lease.clone();
    let receipt = memory_core(&h.ctx, &memory, move |store| {
        store.calendar_live_record_outcome(&saved, write_error(error))
    })
    .await
    .unwrap();
    assert_eq!(receipt["state"], "ready");
    assert_eq!(receipt["request_id"], "edit");
    assert_eq!(
        operation(&h.ctx, "edit", &memory)
            .await
            .unwrap()
            .lease
            .unwrap()
            .dispatch,
        lease.dispatch
    );
    drop(memory);
    drop(api);
    assert_eq!(h.ctx.calendar_preparation.available_permits(), 2);
    h.close();
}
#[tokio::test]
async fn reconciliation_held_read_cannot_overwrite_newer_permission_membership_or_cache() {
    for change in ["source", "permission", "membership"] {
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::new(dir.path());
        h.http.preflight();
        h.http
            .steps
            .lock()
            .unwrap()
            .push_back(Err(TransportError::Network));
        assert_eq!(
            execute(&h.ctx, request()).await.unwrap()["state"],
            "unknown"
        );
        h.http
            .push(200, json!({"id":CAL,"accessRole":"owner","timeZone":"UTC"}));
        let mut observed = event();
        observed["summary"] = "After".into();
        observed["etag"] = "observed-v2".into();
        h.http.push(200, observed);
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        *h.http.event_hold.lock().unwrap() = Some((entered.clone(), release.clone()));
        let ctx = h.ctx.clone();
        let task = tokio::spawn(async move { reconcile(&ctx, "edit").await });
        tokio::time::timeout(Duration::from_secs(3), entered.notified())
            .await
            .unwrap();
        let account = h.ctx.account.as_str().to_owned();
        let expected = h
            .ctx
            .core
            .call(move |store| {
                if change == "permission" {
                    let scope = store.calendar_sync_scope(&account, CAL)?;
                    store.calendar_observe_permission(
                        &account,
                        CAL,
                        1,
                        scope["permission_epoch"].as_i64().unwrap(),
                        fm_calendar::Access::Owner,
                        "UTC",
                    )?;
                    Ok(event())
                } else if change == "membership" {
                    let before = store.calendar_sync_scope(&account, CAL)?;
                    let run = store.calendar_catalog_begin(&account, CalendarSyncMode::Full)?;
                    let page = CalendarPage {
                        input_cursor: None,
                        items: vec![],
                        next_page_token: None,
                        next_sync_token: Some("empty-catalog".into()),
                    };
                    store.calendar_stage_page_chunk(&run, &page, 0)?;
                    store.calendar_seal_page(&run, &page)?;
                    while store.calendar_catalog_finalize_chunk(&run)? > 0 {}
                    store.calendar_publish_sync(&run)?;
                    let after = store.calendar_sync_scope(&account, CAL)?;
                    assert_eq!(after["membership_denied"], true);
                    assert_eq!(after["permission_epoch"], before["permission_epoch"]);
                    assert_eq!(after["revision"], before["revision"]);
                    Ok(event())
                } else {
                    let mut newer = event();
                    newer["etag"] = "newer-v3".into();
                    newer["summary"] = "Newer concurrent source".into();
                    let run = store.calendar_sync_begin(&account, CAL, CalendarSyncMode::Full)?;
                    let page = CalendarPage {
                        input_cursor: None,
                        items: vec![newer.clone()],
                        next_page_token: None,
                        next_sync_token: Some("newer-token".into()),
                    };
                    store.calendar_stage_page_chunk(&run, &page, 0)?;
                    store.calendar_seal_page(&run, &page)?;
                    store.calendar_publish_sync(&run)?;
                    while store.calendar_finalize_chunk(&run)? > 0 {}
                    Ok(newer)
                }
            })
            .await
            .unwrap();
        // The writer stayed available while the actual GET was held.
        release.notify_one();
        let receipt = tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(receipt["state"], "reconciled");
        assert_eq!(receipt["outcome"]["observed_etag"], "observed-v2");
        assert_eq!(receipt["outcome"]["cache_published"], false);
        let db = rusqlite::Connection::open(dir.path().join("store.sqlite3")).unwrap();
        let raw: String = db.query_row("SELECT json FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id='event1'",
            [h.ctx.account.as_str(), CAL], |row| row.get(0)).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&raw).unwrap(), expected);
        assert_eq!(
            h.http
                .requests
                .lock()
                .unwrap()
                .iter()
                .filter(|(method, _)| method.starts_with("Patch"))
                .count(),
            1
        );
        assert_eq!(h.ctx.calendar_preparation.available_permits(), 2);
        h.close();
    }
}

#[tokio::test]
async fn lost_response_reconciles_with_reads_only() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path());
    h.http.preflight();
    h.http
        .steps
        .lock()
        .unwrap()
        .push_back(Err(TransportError::Network));
    assert_eq!(
        execute(&h.ctx, request()).await.unwrap()["state"],
        "unknown"
    );
    h.http
        .push(200, json!({"id":CAL,"accessRole":"owner","timeZone":"UTC"}));
    let mut observed = event();
    observed["summary"] = "After".into();
    observed["etag"] = "v2".into();
    h.http.push(200, observed.clone());
    let receipt = reconcile(&h.ctx, "edit").await.unwrap();
    assert_eq!(receipt["state"], "reconciled");
    assert_eq!(receipt["intent_version"], 2);
    assert_eq!(receipt["outcome"]["kind"], "observed_state");
    assert_eq!(receipt["outcome"]["notification_delivery"], "unverified");
    assert_eq!(receipt["outcome"]["cache_published"], true);
    let db = rusqlite::Connection::open(dir.path().join("store.sqlite3")).unwrap();
    let raw: String = db.query_row("SELECT json FROM calendar_visible_sources WHERE account=?1 AND calendar_id=?2 AND event_id='event1'",
        [h.ctx.account.as_str(), CAL], |row| row.get(0)).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&raw).unwrap(), observed);
    assert_eq!(
        execute(&h.ctx, request()).await.unwrap()["state"],
        "reconciled"
    );
    assert_eq!(
        reconcile(&h.ctx, "edit").await.unwrap()["state"],
        "reconciled"
    );
    assert_eq!(
        h.http
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|(request, _)| request.starts_with("Patch"))
            .count(),
        1
    );
    h.close();
}
#[tokio::test]
async fn cancellation_after_dispatch_still_records_unknown_outcome() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path());
    h.http.preflight();
    *h.http.cancel_on_write.lock().unwrap() = true;
    let result = execute(&h.ctx, request()).await.unwrap();
    assert_eq!(result["state"], "unknown");
    let account = h.ctx.account.as_str().to_owned();
    assert_eq!(
        h.ctx
            .core
            .call(move |s| s.calendar_live_status(&account, "edit"))
            .await
            .unwrap()["state"],
        "unknown"
    );
    h.close();
}

#[tokio::test]
async fn cancellation_after_committed_claim_keeps_lease_until_exact_outcome_is_retained() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path());
    let memory = Arc::new(h.ctx.calendar_prepare_permit().await.unwrap());
    let context = h.ctx.clone();
    let (claimed, observe) = tokio::sync::oneshot::channel();
    let (release, held) = std::sync::mpsc::channel();
    let mut task = tokio::spawn(async move {
        let account = context.account.as_str().to_owned();
        let cancel = context.cancel.clone();
        memory_core(&context, &memory, move |store| {
            store.calendar_live_prepare(&account, &request())?;
            let epoch = store.calendar_checkpoint(&account, CAL)?["permission_epoch"]
                .as_i64()
                .unwrap();
            let lease = store.calendar_live_claim(&account, "edit", 0, 1, epoch, Some(event()))?;
            cancel.cancel();
            claimed.send(()).unwrap();
            held.recv_timeout(Duration::from_secs(3)).unwrap();
            Ok(lease)
        })
        .await
    });
    tokio::time::timeout(Duration::from_secs(3), observe)
        .await
        .unwrap()
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(40), &mut task)
            .await
            .is_err(),
        "a committed claim cannot be discarded on cancellation"
    );
    assert_eq!(h.ctx.calendar_preparation.available_permits(), 1);
    release.send(()).unwrap();
    let lease = task.await.unwrap().unwrap();
    assert_eq!(lease.attempt, 1);
    let receipt = h
        .ctx
        .core
        .call(move |s| s.calendar_live_record_outcome(&lease, CalendarWriteOutcome::Unknown))
        .await
        .unwrap();
    assert_eq!(receipt["state"], "unknown");
    assert!(h.http.requests.lock().unwrap().is_empty());
    assert_eq!(h.ctx.calendar_preparation.available_permits(), 2);
    h.close();
}

#[tokio::test]
async fn create_delete_and_self_rsvp_use_exact_scoped_requests() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path());
    let create = json!({"request_id":"create","kind":"create","calendar_id":CAL,"fields":{"title":"New event","start":"2026-09-12","end":"2026-09-13","all_day":true,"time_zone":"UTC"}});
    let account = h.ctx.account.as_str().to_owned();
    let prepared = create.clone();
    let target = h
        .ctx
        .core_call(move |s| s.calendar_live_prepare(&account, &prepared))
        .await
        .unwrap()["target"]
        .clone();
    h.http
        .push(200, json!({"id":CAL,"accessRole":"owner","timeZone":"UTC"}));
    h.http.push(200,json!({"id":target["event"],"etag":"new-v1","summary":"New event","start":{"date":"2026-09-12"},"end":{"date":"2026-09-13"}}));
    assert_eq!(
        execute(&h.ctx, create).await.unwrap()["state"],
        "acknowledged"
    );
    h.http.preflight();
    h.http.push(204, Value::Null);
    assert_eq!(execute(&h.ctx,json!({"request_id":"delete","kind":"delete","calendar_id":CAL,"event_id":"event1","expected_etag":"v1"})).await.unwrap()["state"],"acknowledged");
    h.http
        .push(200, json!({"id":CAL,"accessRole":"owner","timeZone":"UTC"}));
    let mut invitation = event();
    invitation["attendees"] = json!([{"email":CAL,"self":true,"responseStatus":"needsAction"},{"email":"other@example.test","responseStatus":"accepted"}]);
    h.http.push(200, invitation.clone());
    invitation["etag"] = "v2".into();
    invitation["attendees"][0]["responseStatus"] = "tentative".into();
    h.http.push(200, invitation);
    assert_eq!(execute(&h.ctx,json!({"request_id":"rsvp","kind":"rsvp","calendar_id":CAL,"event_id":"event1","expected_etag":"v1","response":"tentative"})).await.unwrap()["state"],"acknowledged");
    let requests = h.http.requests.lock().unwrap();
    assert!(requests[1].0.starts_with("Post"));
    assert_eq!(requests[1].1["id"], target["event"]);
    assert!(requests[4].0.starts_with("Delete"));
    assert_eq!(
        requests[7].1,
        json!({"attendeesOmitted":true,"attendees":[{"email":CAL,"responseStatus":"tentative"}]})
    );
    drop(requests);
    h.close();
}

#[tokio::test]
async fn four_accounts_share_http_slots_without_nested_mutation_read_deadlock() {
    for memory_seats in [4, 2] {
        let budget =
            fm_google::scheduler::ServiceBudget::from_confirmed_limits(10000, 10000).unwrap();
        let quota = crate::google_sync::quota::QuotaGate::new(
            fm_google::scheduler::SchedulerConfig::new(budget, budget),
        )
        .unwrap();
        let memory = Arc::new(Semaphore::new(memory_seats));
        let barrier = Arc::new(tokio::sync::Barrier::new(4));
        let dirs = (0..4)
            .map(|_| tempfile::tempdir().unwrap())
            .collect::<Vec<_>>();
        let mut harnesses = Vec::new();
        let mut tasks = tokio::task::JoinSet::new();
        for (index, dir) in dirs.iter().enumerate() {
            let mut h = Harness::with_subject(dir.path(), &format!("parallel-calendar-{index}"));
            h.ctx.quota = quota.clone();
            h.ctx.calendar_preparation = memory.clone();
            // With four memory seats all fresh GETs must fit in the four HTTP
            // slots. Holding a mutation slot first makes this barrier impossible.
            // The second run exercises the actual two-seat production bound.
            if memory_seats == 4 {
                *h.http.event_barrier.lock().unwrap() = Some(barrier.clone());
            }
            h.http.preflight();
            let mut updated = event();
            updated["summary"] = "After".into();
            updated["etag"] = "v2".into();
            h.http.push(200, updated);
            let context = h.ctx.clone();
            tasks.spawn(async move { execute(&context, request()).await });
            harnesses.push(h);
        }
        tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(result) = tasks.join_next().await {
                assert_eq!(result.unwrap().unwrap()["state"], "acknowledged");
            }
        })
        .await
        .expect("fresh GETs cannot wait behind unused mutation HTTP slots");
        assert_eq!(memory.available_permits(), memory_seats);
        for h in harnesses {
            h.close();
        }
    }
}

#[tokio::test]
async fn cancellation_during_mutation_cooldown_preserves_unsent_ready_without_holding_memory() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::new(dir.path());
    h.http.preflight();
    *h.http.cooldown_after_event.lock().unwrap() = Some(h.ctx.quota.clone());
    let context = h.ctx.clone();
    let task = tokio::spawn(async move { execute(&context, request()).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let account = h.ctx.account.as_str().to_owned();
            let state = h
                .ctx
                .core
                .call(move |s| s.calendar_live_status(&account, "edit"))
                .await;
            if state.is_ok_and(|state| state["state"] == "ready") {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    // Allow the worker to drop its stage result and enter the quota wait.
    tokio::task::yield_now().await;
    assert_eq!(h.ctx.calendar_preparation.available_permits(), 2);
    h.ctx.cancel.cancel();
    assert_eq!(task.await.unwrap().unwrap_err().code, "cancelled");
    let account = h.ctx.account.as_str().to_owned();
    let saved = account.clone();
    assert_eq!(
        h.ctx
            .core
            .call(move |s| s.calendar_live_status(&saved, "edit"))
            .await
            .unwrap()["state"],
        "ready"
    );
    assert_eq!(h.http.requests.lock().unwrap().len(), 2);
    h.close();
    let store = fm_core::Store::open_live(dir.path()).unwrap();
    assert_eq!(
        store.calendar_live_status(&account, "edit").unwrap()["state"],
        "ready"
    );
}

#[tokio::test]
async fn cancelled_queued_core_payload_retains_memory_until_writer_discards_it() {
    let (queue, mut receive) = mpsc::channel(8);
    let context = SyncContext::for_test(
        CoreBridge::new(queue, Arc::new(Semaphore::new(8))),
        AccountId::new("queued-calendar-source").unwrap(),
        1,
        Arc::new(Script::default()),
        "synthetic-token".into(),
        CancellationToken::new(),
    );
    let memory = Arc::new(context.calendar_prepare_permit().await.unwrap());
    let child = context.clone();
    let task = tokio::spawn(async move {
        let source = json!({"payload":"x".repeat(2*1024*1024)});
        memory_core(&child, &memory, move |_| Ok(source)).await
    });
    let queued = receive.recv().await.unwrap();
    context.cancel.cancel();
    assert!(
        !task.is_finished(),
        "queued work must finish on the writer before its result returns"
    );
    assert_eq!(
        context.calendar_preparation.available_permits(),
        1,
        "queued source must still own its memory seat after caller cancellation"
    );
    let dir = tempfile::tempdir().unwrap();
    let mut store = fm_core::Store::open_live(dir.path()).unwrap();
    if let crate::Work::Provider(call) = queued {
        call.run(&mut store);
    } else {
        panic!("unexpected queued test work")
    }
    assert_eq!(task.await.unwrap().unwrap_err().code, "cancelled");
    assert_eq!(context.calendar_preparation.available_permits(), 2);
}
