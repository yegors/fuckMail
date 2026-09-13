use super::*;
use crate::core_tasks::CoreBridge;
use fm_google::{
    transport::{
        CancellationToken, HttpFuture, HttpMethod, HttpRequest, HttpResponse, HttpTransport,
    },
    AccountId,
};
use std::{
    collections::BTreeSet,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
};
use tokio::sync::{mpsc, Notify, Semaphore};

struct Http {
    body: Mutex<Value>,
    requests: AtomicUsize,
    hold: AtomicBool,
    started: Notify,
    release: Notify,
}
impl HttpTransport for Http {
    fn execute<'a>(&'a self, request: HttpRequest, _: &'a CancellationToken) -> HttpFuture<'a> {
        Box::pin(async move {
            assert_eq!(request.method(), HttpMethod::Get);
            assert_eq!(
                request.url(),
                "https://gmail.googleapis.com/gmail/v1/users/me/labels"
            );
            assert_eq!(
                request.headers()["authorization"],
                "Bearer fixture-label-token"
            );
            self.requests.fetch_add(1, Ordering::SeqCst);
            self.started.notify_one();
            if self.hold.load(Ordering::SeqCst) {
                self.release.notified().await;
            }
            Ok(HttpResponse {
                status: 200,
                body: serde_json::to_vec(&*self.body.lock().unwrap()).unwrap(),
                retry_after_secs: None,
                etag: None,
            })
        })
    }
}
struct Harness {
    ctx: SyncContext,
    http: Arc<Http>,
    writer: std::thread::JoinHandle<()>,
    _directory: tempfile::TempDir,
}
impl Harness {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().to_owned();
        let (sender, mut receiver) = mpsc::channel(8);
        let (ready, wait) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            let mut store = fm_core::Store::open_live(&path).unwrap();
            let account = store
                .bind_google_account(
                    "label-worker",
                    "reader@example.test",
                    &BTreeSet::from(["https://www.googleapis.com/auth/gmail.modify".into()]),
                    None,
                )
                .unwrap();
            ready.send(account).unwrap();
            while let Some(work) = receiver.blocking_recv() {
                if let crate::Work::Provider(call) = work {
                    call.run(&mut store);
                } else {
                    panic!("unexpected writer work");
                }
            }
        });
        let account = AccountId::new(wait.recv_timeout(Duration::from_secs(10)).unwrap()).unwrap();
        let http = Arc::new(Http {
            body: Mutex::new(
                json!({"labels":[{"id":"A","name":"Alpha","type":"user"},{"id":"INBOX","name":"Inbox","type":"system"}]}),
            ),
            requests: AtomicUsize::new(0),
            hold: AtomicBool::new(false),
            started: Notify::new(),
            release: Notify::new(),
        });
        let ctx = SyncContext::for_test(
            CoreBridge::new(sender, Arc::new(Semaphore::new(8))),
            account,
            1,
            http.clone(),
            "fixture-label-token".into(),
            CancellationToken::new(),
        );
        Self {
            ctx,
            http,
            writer,
            _directory: directory,
        }
    }
    async fn cached(&self) -> Value {
        let account = self.ctx.account.as_str().to_owned();
        self.ctx
            .core
            .call(move |s| s.google_labels(&account, &json!({})))
            .await
            .unwrap()
    }
    fn close(self) {
        drop(self.ctx);
        self.writer.join().unwrap();
    }
}

#[tokio::test]
async fn owned_label_cadence_publishes_complete_catalog_while_cached_queries_make_no_http() {
    let h = Harness::new();
    assert_eq!(h.cached().await["available"], false);
    let mut worker = LabelsWorker::default();
    assert_eq!(worker.step(&h.ctx, 0).await.unwrap(), Step::Progress);
    let cached = h.cached().await;
    assert_eq!(cached["catalog_revision"], 1);
    assert_eq!(cached["labels"][0]["selectable"], true);
    assert_eq!(cached["labels"][1]["selectable"], false);
    for _ in 0..3 {
        assert_eq!(h.cached().await, cached);
    }
    assert!(matches!(
        worker.step(&h.ctx, REFRESH_INTERVAL_MS - 1).await.unwrap(),
        Step::Idle(_)
    ));
    assert_eq!(h.http.requests.load(Ordering::SeqCst), 1);
    *h.http.body.lock().unwrap() = json!({"labels":[{"id":"A","name":"Renamed","type":"user"}]});
    assert_eq!(
        worker.step(&h.ctx, REFRESH_INTERVAL_MS).await.unwrap(),
        Step::Progress
    );
    assert_eq!(h.cached().await["labels"][0]["name"], "Renamed");
    assert_eq!(h.http.requests.load(Ordering::SeqCst), 2);
    h.close();
}

#[tokio::test]
async fn concurrent_explicit_and_periodic_refreshes_share_a_current_catalog_revision() {
    let h = Harness::new();
    h.http.hold.store(true, Ordering::SeqCst);
    let first_context = h.ctx.clone();
    let first = tokio::spawn(async move { refresh(&first_context).await });
    tokio::time::timeout(Duration::from_secs(2), h.http.started.notified())
        .await
        .unwrap();
    let second_context = h.ctx.clone();
    let (started, ready) = tokio::sync::oneshot::channel();
    let second = tokio::spawn(async move {
        let _ = started.send(());
        refresh(&second_context).await
    });
    ready.await.unwrap();
    tokio::task::yield_now().await;
    assert_eq!(h.http.requests.load(Ordering::SeqCst), 1);
    h.http.hold.store(false, Ordering::SeqCst);
    h.http.release.notify_one();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), first)
            .await
            .unwrap()
            .unwrap()
            .unwrap()["catalog_revision"],
        1
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), second)
            .await
            .unwrap()
            .unwrap()
            .unwrap()["catalog_revision"],
        2
    );
    assert_eq!(h.http.requests.load(Ordering::SeqCst), 2);
    h.close();
}

#[tokio::test]
async fn connection_change_after_http_admission_retains_prior_published_catalog() {
    let h = Harness::new();
    refresh(&h.ctx).await.unwrap();
    // Consume the first observation before arming the second response barrier.
    h.http.started.notified().await;
    let previous = h.cached().await;
    h.http.hold.store(true, Ordering::SeqCst);
    *h.http.body.lock().unwrap() =
        json!({"labels":[{"id":"B","name":"Must not publish","type":"user"}]});
    let context = h.ctx.clone();
    let running = tokio::spawn(async move { refresh(&context).await });
    tokio::time::timeout(Duration::from_secs(2), h.http.started.notified())
        .await
        .unwrap();
    let account = h.ctx.account.as_str().to_owned();
    h.ctx
        .core
        .call(move |s| {
            s.set_google_connection_state(
                &account,
                fm_core::live_profile::ConnectionState::Disconnected,
            )
        })
        .await
        .unwrap();
    h.http.release.notify_one();
    let result = tokio::time::timeout(Duration::from_secs(2), running)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.unwrap_err().code, "stale_google_work");
    let cached = h.cached().await;
    assert_eq!(cached["labels"], previous["labels"]);
    assert_eq!(cached["catalog_revision"], 1);
    assert_eq!(cached["connection_state"], "disconnected");
    assert_eq!(h.ctx.preparation.available_permits(), 1);
    h.close();
}

#[tokio::test]
async fn invalid_complete_response_cannot_replace_the_last_good_catalog() {
    let h = Harness::new();
    refresh(&h.ctx).await.unwrap();
    let previous = h.cached().await;
    *h.http.body.lock().unwrap() = json!({"labels":[{"id":"duplicate","name":"A","type":"user"},{"id":"duplicate","name":"B","type":"user"}]});
    assert!(refresh(&h.ctx).await.is_err());
    assert_eq!(h.cached().await, previous);
    assert_eq!(h.ctx.preparation.available_permits(), 1);
    h.close();
}
