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
    creates: AtomicUsize,
    lose_create: AtomicBool,
}
impl HttpTransport for Http {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
        cancel: &'a CancellationToken,
    ) -> HttpFuture<'a> {
        Box::pin(async move {
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
                tokio::select! { _=self.release.notified()=>{}, _=cancel.cancelled()=>return Err(fm_google::transport::TransportError::Cancelled) }
            }
            if request.method() == HttpMethod::Post {
                let body: Value = serde_json::from_slice(request.body_bytes()).unwrap();
                assert_eq!(body["labelListVisibility"], "labelShow");
                let ordinal = self.creates.fetch_add(1, Ordering::SeqCst) + 1;
                let label =
                    json!({"id":format!("Created_{ordinal}"),"name":body["name"],"type":"user"});
                self.body.lock().unwrap()["labels"]
                    .as_array_mut()
                    .unwrap()
                    .push(label.clone());
                if self.lose_create.swap(false, Ordering::SeqCst) {
                    return Err(fm_google::transport::TransportError::Network);
                }
                return Ok(HttpResponse {
                    status: 200,
                    body: serde_json::to_vec(&label).unwrap(),
                    retry_after_secs: None,
                    etag: None,
                });
            }
            assert_eq!(request.method(), HttpMethod::Get);
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
        Self::with_scope("https://www.googleapis.com/auth/gmail.modify")
    }
    fn with_scope(scope: &'static str) -> Self {
        Self::with_writer_hook(scope, Box::new(|_| {}))
    }
    /// A harness whose store writer calls `before` with each call's origin
    /// just before running it.
    fn with_writer_hook(scope: &'static str, mut before: Box<dyn FnMut(&str) + Send>) -> Self {
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
                    &BTreeSet::from([scope.into()]),
                    None,
                )
                .unwrap();
            ready.send(account).unwrap();
            while let Some(work) = receiver.blocking_recv() {
                if let crate::Work::Provider(call) = work {
                    before(call.origin);
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
            creates: AtomicUsize::new(0),
            lose_create: AtomicBool::new(false),
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
    async fn categories(&self) -> Vec<Value> {
        let account = self.ctx.account.as_str().to_owned();
        let catalog = self
            .ctx
            .core
            .call(move |s| s.label_catalog(&account))
            .await
            .unwrap();
        catalog["categories"].as_array().unwrap().clone()
    }
    fn close(self) {
        drop(self.ctx);
        self.writer.join().unwrap();
    }
}

#[tokio::test]
async fn explicit_name_sync_imports_creates_and_reconciles_an_interrupted_create() {
    let h = Harness::new();
    let account = h.ctx.account.as_str().to_owned();
    h.ctx
        .core
        .call(move |s| {
            s.label_edit(
                &account,
                "label_create",
                &json!({"id":"local","name":"Research","request_id":"new-local"}),
            )
        })
        .await
        .unwrap();
    h.http.lose_create.store(true, Ordering::SeqCst);
    let first = sync_names(&h.ctx, json!({"request_id":"sync-first"}))
        .await
        .unwrap_err();
    assert_eq!(first.code, "google_transport_interrupted");
    assert_eq!(h.http.creates.load(Ordering::SeqCst), 1);
    let second = sync_names(&h.ctx, json!({"request_id":"sync-check"}))
        .await
        .unwrap();
    assert_eq!(second["created"], 0);
    assert_eq!(second["unresolved"], 0);
    assert_eq!(
        h.http.creates.load(Ordering::SeqCst),
        1,
        "checking an unknown create must not submit another POST"
    );
    let account = h.ctx.account.as_str().to_owned();
    let catalog = h
        .ctx
        .core
        .call(move |s| s.label_catalog(&account))
        .await
        .unwrap();
    assert_eq!(catalog["categories"].as_array().unwrap().len(), 2);
    let local = catalog["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "local")
        .unwrap();
    assert_eq!(local["provider_label_id"], "Created_1");
    assert_eq!(local["creation_state"], "created");
    let account = h.ctx.account.as_str().to_owned();
    h.ctx.core.call(move|s|s.label_edit(&account,"label_describe",&json!({"id":"local","expected_description":"","description":"Papers and findings","request_id":"describe-local"}))).await.unwrap();
    h.http.body.lock().unwrap()["labels"][2]["name"] = json!("Renamed research");
    assert_eq!(
        sync_names(&h.ctx, json!({"request_id":"sync-rename"}))
            .await
            .unwrap()["renamed"],
        1
    );
    let account = h.ctx.account.as_str().to_owned();
    let catalog = h
        .ctx
        .core
        .call(move |s| s.label_catalog(&account))
        .await
        .unwrap();
    let local = catalog["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "local")
        .unwrap();
    assert_eq!(local["name"], "Renamed research");
    assert_eq!(local["description"], "Papers and findings");
    h.close();
}

#[tokio::test]
async fn label_job_is_nonblocking_and_stops_with_its_owner() {
    let h = Harness::new();
    h.http.hold.store(true, Ordering::SeqCst);
    let owner_cancel = CancellationToken::new();
    let owners = Arc::new(Mutex::new(std::collections::BTreeMap::from([(
        "owner".to_owned(),
        owner_cancel.clone(),
    )])));
    let owner = super::super::OperationOwner {
        owners: owners.clone(),
        key: "owner".to_owned(),
    };
    let job = SyncJob::start(
        h.ctx.clone(),
        owner,
        "gui".to_owned(),
        owner_cancel.clone(),
        "background-sync".to_owned(),
    );
    h.http.started.notified().await;
    assert_eq!(job.snapshot("job")["state"], "running");
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), h.cached())
            .await
            .unwrap()["available"],
        false,
        "cached queries stay available while HTTP is held"
    );
    owner_cancel.cancel();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !job.task.is_finished() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(job.snapshot("job")["state"], "cancelled");
    assert!(owners.lock().unwrap().is_empty());
    assert_eq!(h.http.creates.load(Ordering::SeqCst), 0);
    job.task.await.unwrap();
    h.close();
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

#[tokio::test]
async fn first_catalog_refresh_imports_a_new_accounts_gmail_labels_once() {
    let h = Harness::new();
    let mut worker = LabelsWorker::default();
    let before = *h.ctx.refresh.borrow();
    assert_eq!(worker.step(&h.ctx, 0).await.unwrap(), Step::Progress);
    let categories = h.categories().await;
    assert_eq!(categories.len(), 1, "system labels are not imported");
    assert_eq!(categories[0]["name"], "Alpha");
    assert_eq!(categories[0]["provider_label_id"], "A");
    assert!(*h.ctx.refresh.borrow() > before);
    assert_eq!(h.http.creates.load(Ordering::SeqCst), 0);
    // Later Gmail labels wait for an explicit sync, so removed labels stay removed.
    h.http.body.lock().unwrap()["labels"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"B","name":"Beta","type":"user"}));
    assert_eq!(
        worker.step(&h.ctx, REFRESH_INTERVAL_MS).await.unwrap(),
        Step::Progress
    );
    assert_eq!(h.categories().await.len(), 1);
    let account = h.ctx.account.as_str().to_owned();
    let again = h
        .ctx
        .core
        .call(move |s| s.google_labels_initial_import(&account))
        .await
        .unwrap();
    assert!(again.is_none());
    h.close();
}

#[tokio::test]
async fn read_only_account_refreshes_labels_without_importing_them() {
    let h = Harness::with_scope("https://www.googleapis.com/auth/gmail.readonly");
    let mut worker = LabelsWorker::default();
    assert_eq!(worker.step(&h.ctx, 0).await.unwrap(), Step::Progress);
    assert_eq!(h.cached().await["labels"][0]["name"], "Alpha");
    assert!(h.categories().await.is_empty());
    let account = h.ctx.account.as_str().to_owned();
    let pending = h
        .ctx
        .core
        .call(move |s| s.google_labels_initial_import(&account))
        .await
        .unwrap();
    assert!(
        pending.is_none(),
        "the manual path remains after a read-only first sync"
    );
    h.close();
}

#[tokio::test]
async fn name_sync_links_one_label_per_store_call_so_other_requests_are_answered_between() {
    let (paused, wait_paused) = std::sync::mpsc::channel::<()>();
    let (release, wait_release) = std::sync::mpsc::channel::<()>();
    let merges = Arc::new(AtomicUsize::new(0));
    let counted = merges.clone();
    let h = Harness::with_writer_hook(
        "https://www.googleapis.com/auth/gmail.modify",
        Box::new(move |origin| {
            // The merge is the one store call of the sync that returns a JSON value.
            if origin.contains(
                "core_call<serde_json::value::Value, fm_engine::google_sync::labels::sync_names",
            ) && counted.fetch_add(1, Ordering::SeqCst) == 0
            {
                paused.send(()).unwrap();
                wait_release.recv().unwrap();
            }
        }),
    );
    *h.http.body.lock().unwrap() = json!({"labels":[{"id":"A","name":"Alpha","type":"user"},{"id":"B","name":"Beta","type":"user"},{"id":"INBOX","name":"Inbox","type":"system"}]});
    let context = h.ctx.clone();
    let sync =
        tokio::spawn(async move { sync_names(&context, json!({"request_id":"two-labels"})).await });
    tokio::task::spawn_blocking(move || wait_paused.recv_timeout(Duration::from_secs(10)))
        .await
        .unwrap()
        .unwrap();
    // Queued while the first merge call waits to run, so it is answered
    // before the sync can queue its next call.
    let account = h.ctx.account.as_str().to_owned();
    let core = h.ctx.core.clone();
    let probe = tokio::spawn(async move {
        core.call(move |s| {
            let catalog = s.label_catalog(&account)?;
            Ok(catalog["categories"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row["provider_label_id"].is_string())
                .count())
        })
        .await
    });
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    release.send(()).unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(10), probe)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        1,
        "the first store call links one label"
    );
    let result = tokio::time::timeout(Duration::from_secs(10), sync)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result["pulled"], 2);
    assert!(merges.load(Ordering::SeqCst) >= 3);
    let categories = h.categories().await;
    assert_eq!(categories.len(), 2);
    assert!(categories
        .iter()
        .all(|row| row["provider_label_id"].is_string()));
    h.close();
}
