use super::*;
use crate::core_tasks::CoreBridge;
use fm_core::live_calendar::{CalendarPage, CalendarSyncMode};
use fm_google::{
    transport::{
        CancellationToken, HttpFuture, HttpRequest, HttpResponse, HttpTransport, TransportError,
    },
    AccountId,
};
use std::{collections::VecDeque, sync::Mutex, time::Duration};
use tokio::sync::{mpsc, Semaphore};

const CAL: &str = "human@example.test";
type Hold = (mpsc::UnboundedSender<()>, Arc<Semaphore>);

#[derive(Default)]
struct Script {
    replies: Mutex<VecDeque<(u16, Value)>>,
    requests: Mutex<Vec<String>>,
    master_hold: Mutex<Option<Hold>>,
    instance_hold: Mutex<Option<Hold>>,
}
impl Script {
    fn push(&self, value: Value) {
        self.replies.lock().unwrap().push_back((200, value));
    }
    fn lookup(&self) {
        self.push(json!({"id":CAL,"accessRole":"owner","timeZone":"UTC"}));
        self.push(master_source());
        self.push(instance_source());
    }
}
impl HttpTransport for Script {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
        cancel: &'a CancellationToken,
    ) -> HttpFuture<'a> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request.url().to_owned());
            let hold = if request.url().ends_with("/events/master") {
                self.master_hold.lock().unwrap().clone()
            } else if request.url().ends_with("/events/opaque-instance") {
                self.instance_hold.lock().unwrap().clone()
            } else {
                None
            };
            if let Some((entered, release)) = hold {
                entered.send(()).unwrap();
                tokio::select! {
                    _=cancel.cancelled()=>return Err(TransportError::Cancelled),
                    permit=release.acquire()=>{permit.unwrap().forget();},
                }
            }
            let (status, body) = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected occurrence HTTP");
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
    fn new(path: &std::path::Path, subject: &str) -> Self {
        let path = path.to_owned();
        let subject = subject.to_owned();
        let (queue, mut receive) = mpsc::channel(8);
        let (ready, wait) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            let mut store = fm_core::Store::open_live(&path).unwrap();
            let scopes = [
                "openid",
                "email",
                "https://www.googleapis.com/auth/calendar.events",
                "https://www.googleapis.com/auth/calendar.calendarlist.readonly",
            ]
            .map(str::to_owned)
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
                items: vec![master_source()],
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
                    call.run(&mut store)
                } else {
                    panic!("provider work only")
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
        drop(self.ctx);
        self.writer.join().unwrap();
    }
}
fn master_source() -> Value {
    json!({"id":"master","etag":"master-v1","status":"confirmed","eventType":"default","summary":"Series","organizer":{"email":CAL,"self":true},"start":{"dateTime":"2026-09-13T09:00:00Z","timeZone":"UTC"},"end":{"dateTime":"2026-09-13T10:00:00Z","timeZone":"UTC"},"recurrence":["RRULE:FREQ=DAILY;COUNT=3"],"extendedProperties":{"private":{"retained":"yes"}}})
}
fn instance_source() -> Value {
    let mut source = master_source();
    source["id"] = "opaque-instance".into();
    source["etag"] = "instance-v1".into();
    source["recurringEventId"] = "master".into();
    source["originalStartTime"] = source["start"].clone();
    source.as_object_mut().unwrap().remove("recurrence");
    source
}
fn params(hint: bool) -> Value {
    let mut p = json!({"calendar_id":CAL,"series_id":"master","original_start":{"kind":"zoned","instant":"2026-09-13T09:00:00Z","time_zone":"UTC"}});
    if hint {
        p["event_id"] = "opaque-instance".into();
    }
    p
}

#[tokio::test]
async fn four_lookup_workers_share_two_seats_without_nested_admission_and_queries_progress() {
    let directories = (0..4)
        .map(|_| tempfile::tempdir().unwrap())
        .collect::<Vec<_>>();
    let mut harnesses = directories
        .iter()
        .enumerate()
        .map(|(i, d)| Harness::new(d.path(), &format!("occurrence-{i}")))
        .collect::<Vec<_>>();
    let memory = Arc::new(Semaphore::new(2));
    let quota = harnesses[0].ctx.quota.clone();
    let gate = Arc::new(Semaphore::new(0));
    let (entered, mut events) = mpsc::unbounded_channel();
    let mut tasks = tokio::task::JoinSet::new();
    for h in &mut harnesses {
        h.ctx.calendar_preparation = memory.clone();
        h.ctx.quota = quota.clone();
        h.http.lookup();
        *h.http.master_hold.lock().unwrap() = Some((entered.clone(), gate.clone()));
        let ctx = h.ctx.clone();
        tasks.spawn(async move { read(&ctx, params(true)).await });
    }
    for _ in 0..2 {
        tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .unwrap()
            .unwrap();
    }
    assert_eq!(memory.available_permits(), 0);
    // Actual Core cached query completes while both admitted HTTP sources wait.
    let status = tokio::time::timeout(
        Duration::from_secs(2),
        harnesses[0]
            .ctx
            .core_call(|store| store.handle("status", None, json!({}))),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(status["profile_kind"], "live");
    gate.add_permits(4);
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(result) = tasks.join_next().await {
            let result = result.unwrap().unwrap();
            assert_eq!(result["event_id"], "opaque-instance");
            assert_eq!(result["etag"], "instance-v1");
        }
    })
    .await
    .unwrap();
    assert_eq!(memory.available_permits(), 2);
    for h in harnesses {
        assert_eq!(h.http.requests.lock().unwrap().len(), 3);
        h.close();
    }
}

#[tokio::test]
async fn terminal_uniqueness_checks_later_pages_and_refuses_exhaustion_and_date_aliases() {
    let directory = tempfile::tempdir().unwrap();
    let h = Harness::new(directory.path(), "resolve-pages");
    let selector = CalendarOccurrenceSelector::parse(&params(false)).unwrap();
    let mut unrelated = instance_source();
    unrelated["id"] = "date-only".into();
    unrelated["originalStartTime"] = json!({"date":"2026-09-13"});
    h.http
        .push(json!({"items":[unrelated],"nextPageToken":"p2"}));
    h.http.push(json!({"items":[],"nextPageToken":"p3"}));
    h.http.push(json!({"items":[instance_source()]}));
    assert_eq!(
        resolve_instance_id(&h.ctx, &selector).await.unwrap(),
        "opaque-instance"
    );
    h.http
        .push(json!({"items":[instance_source()],"nextPageToken":"duplicate"}));
    let mut duplicate = instance_source();
    duplicate["id"] = "another-instance".into();
    h.http.push(json!({"items":[duplicate]}));
    assert_eq!(
        resolve_instance_id(&h.ctx, &selector)
            .await
            .unwrap_err()
            .code,
        "calendar_occurrence_ambiguous"
    );
    for i in 0..4 {
        h.http
            .push(json!({"items":[],"nextPageToken":format!("incomplete-{i}")}));
    }
    assert_eq!(
        resolve_instance_id(&h.ctx, &selector)
            .await
            .unwrap_err()
            .code,
        "calendar_occurrence_unresolved"
    );
    assert_eq!(h.ctx.calendar_preparation.available_permits(), 2);
    h.close();
}

#[tokio::test]
async fn cancelled_instance_read_releases_admissions_and_retains_no_editable_reply() {
    let directory = tempfile::tempdir().unwrap();
    let h = Harness::new(directory.path(), "cancel-instance");
    h.http.lookup();
    let gate = Arc::new(Semaphore::new(0));
    let (entered, mut events) = mpsc::unbounded_channel();
    *h.http.instance_hold.lock().unwrap() = Some((entered, gate));
    let ctx = h.ctx.clone();
    let task = tokio::spawn(async move { read(&ctx, params(true)).await });
    tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(h.ctx.calendar_preparation.available_permits(), 1);
    h.ctx.cancel.cancel();
    assert!(tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap()
        .is_err());
    assert_eq!(h.ctx.calendar_preparation.available_permits(), 2);
    h.close();
}

#[tokio::test]
async fn master_publication_during_held_instance_get_refuses_stale_editor_projection() {
    let directory = tempfile::tempdir().unwrap();
    let h = Harness::new(directory.path(), "stale-master");
    h.http.lookup();
    let gate = Arc::new(Semaphore::new(0));
    let (entered, mut events) = mpsc::unbounded_channel();
    *h.http.instance_hold.lock().unwrap() = Some((entered, gate.clone()));
    let ctx = h.ctx.clone();
    let task = tokio::spawn(async move { read(&ctx, params(true)).await });
    tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .unwrap()
        .unwrap();
    let account = h.ctx.account.as_str().to_owned();
    h.ctx
        .core_call(move |store| {
            let run = store.calendar_sync_begin(&account, CAL, CalendarSyncMode::Full)?;
            let mut changed = master_source();
            changed["etag"] = "master-v2".into();
            changed["summary"] = "Changed master".into();
            let page = CalendarPage {
                input_cursor: None,
                items: vec![changed],
                next_page_token: None,
                next_sync_token: Some("changed".into()),
            };
            store.calendar_stage_page_chunk(&run, &page, 0)?;
            store.calendar_seal_page(&run, &page)?;
            store.calendar_publish_sync(&run)?;
            while store.calendar_finalize_chunk(&run)? > 0 {}
            Ok(())
        })
        .await
        .unwrap();
    gate.add_permits(1);
    assert!(tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap()
        .is_err());
    assert_eq!(h.ctx.calendar_preparation.available_permits(), 2);
    h.close();
}
