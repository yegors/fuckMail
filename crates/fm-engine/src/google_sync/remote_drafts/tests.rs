use super::*;
use crate::core_tasks::CoreBridge;
use fm_google::{
    transport::{
        CancellationToken, HttpFuture, HttpMethod, HttpRequest, HttpResponse, HttpTransport,
        TransportError,
    },
    AccountId,
};
use std::collections::{BTreeSet, HashMap};
use tokio::sync::{mpsc, Notify, Semaphore};

#[derive(Default)]
struct Gate {
    entered: Notify,
    release: Notify,
}
struct Http {
    count: usize,
    state: Mutex<HttpState>,
}
#[derive(Default)]
struct HttpState {
    calls: Vec<(String, String)>,
    expired: bool,
    expire_once: bool,
    hold: Option<Arc<Gate>>,
    missing: Option<usize>,
    malformed: Option<usize>,
    oversized: Option<usize>,
    wrong_container: Option<usize>,
    version: u8,
    error_once: Option<u16>,
}
fn source(token: &str, index: usize, version: u8) -> Vec<u8> {
    format!("From: Foreign Author <other@example.test>\r\nTo: One <one@example.test>, Two <two@example.test>\r\nCc: copy@example.test\r\nBcc: blind@example.test\r\nSubject: Remote {index} version {version}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{token} retained source {index} v{version}").into_bytes()
}
impl HttpTransport for Http {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
        cancel: &'a CancellationToken,
    ) -> HttpFuture<'a> {
        Box::pin(async move {
            assert_eq!(
                request.method(),
                HttpMethod::Get,
                "inventory must never write"
            );
            assert!(request.body_bytes().is_empty());
            let token = request.headers()["authorization"]
                .to_str()
                .unwrap()
                .strip_prefix("Bearer ")
                .unwrap()
                .to_owned();
            let path = request
                .url()
                .strip_prefix("https://gmail.googleapis.com/gmail/v1/users/me/")
                .unwrap()
                .to_owned();
            let (status, body, hold) = {
                let mut state = self.state.lock().unwrap();
                state.calls.push((token.clone(), path.clone()));
                if let Some(status) = state.error_once.take() {
                    return Ok(HttpResponse {
                        status,
                        body: br#"{"error":{"code":403}}"#.to_vec(),
                        retry_after_secs: None,
                        etag: None,
                    });
                }
                if path.starts_with("drafts?") {
                    assert!(path.contains("maxResults=50"));
                    let offset = path
                        .split('&')
                        .find_map(|item| item.strip_prefix("pageToken=p"))
                        .map(|v| v.parse::<usize>().unwrap())
                        .unwrap_or(0);
                    if offset > 0 && state.expire_once && !state.expired {
                        state.expired = true;
                        (400, json!({"error":{"code":400}}), None)
                    } else {
                        let end = (offset + 50).min(self.count);
                        let mut rows = (offset..end).map(|i| json!({"id":format!("d{i}"),"message":{"id":format!("listed-{i}"),"threadId":format!("t{i}")}})).collect::<Vec<_>>();
                        // Duplicate IDs inside a bounded provider page are legal
                        // input for the durable page's idempotent outcomes.
                        if self.count == 3 {
                            rows.push(rows[0].clone());
                        }
                        (
                            200,
                            json!({"drafts":rows,"nextPageToken":(end<self.count).then(||format!("p{end}")),"resultSizeEstimate":999999}),
                            None,
                        )
                    }
                } else {
                    let index = path
                        .strip_prefix("drafts/d")
                        .unwrap()
                        .strip_suffix("?format=raw")
                        .unwrap()
                        .parse::<usize>()
                        .unwrap();
                    let hold = state.hold.take();
                    if state.oversized == Some(index) {
                        return Err(TransportError::ResponseTooLarge);
                    }
                    if state.missing == Some(index) {
                        (404, json!({"error":{"code":404}}), hold)
                    } else {
                        let raw = if state.malformed == Some(index) {
                            "%%%".into()
                        } else {
                            URL_SAFE_NO_PAD.encode(source(&token, index, state.version))
                        };
                        (
                            200,
                            json!({"id":if state.wrong_container==Some(index) {"wrong-container".to_owned()} else {format!("d{index}")},"message":{"id":format!("actual-{index}-v{}",state.version),"threadId":format!("t{index}"),"raw":raw}}),
                            hold,
                        )
                    }
                }
            };
            if let Some(hold) = hold {
                hold.entered.notify_one();
                tokio::select! {_=cancel.cancelled()=>return Err(TransportError::Cancelled),_=hold.release.notified()=>{}}
            }
            if cancel.is_cancelled() {
                return Err(TransportError::Cancelled);
            }
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
    directory: tempfile::TempDir,
    ctx: Vec<SyncContext>,
    http: Arc<Http>,
    writer: std::thread::JoinHandle<()>,
}
impl Harness {
    fn new(count: usize) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("profile");
        let (send, mut receive) = mpsc::channel(8);
        let (ready, wait) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            let mut store = fm_core::Store::open_live(&path).unwrap();
            let accounts = ["one", "two"].map(|key| {
                store
                    .bind_google_account(
                        &format!("remote-reader-{key}"),
                        &format!("{key}@example.test"),
                        &BTreeSet::from(["https://www.googleapis.com/auth/gmail.readonly".into()]),
                        None,
                    )
                    .unwrap()
            });
            ready.send(accounts).unwrap();
            while let Some(crate::Work::Provider(call)) = receive.blocking_recv() {
                call.run(&mut store);
            }
        });
        let accounts = wait.recv_timeout(Duration::from_secs(10)).unwrap();
        let core = CoreBridge::new(send, Arc::new(Semaphore::new(8)));
        let http = Arc::new(Http {
            count,
            state: Mutex::new(HttpState {
                version: 1,
                ..Default::default()
            }),
        });
        let mut ctx = accounts
            .into_iter()
            .enumerate()
            .map(|(i, account)| {
                SyncContext::for_test(
                    core.clone(),
                    AccountId::new(account).unwrap(),
                    1,
                    http.clone(),
                    format!("reader-{i}"),
                    CancellationToken::new(),
                )
            })
            .collect::<Vec<_>>();
        ctx[1].quota = ctx[0].quota.clone();
        ctx[1].preparation = ctx[0].preparation.clone();
        Self {
            directory,
            ctx,
            http,
            writer,
        }
    }
    async fn finish(&self, index: usize, worker: &mut RemoteDraftWorker) {
        for _ in 0..250 {
            let step = tokio::time::timeout(Duration::from_secs(5), worker.step(&self.ctx[index]))
                .await
                .unwrap()
                .unwrap();
            if matches!(step, Step::Idle(_)) {
                return;
            }
        }
        panic!("bounded fixture did not complete");
    }
    async fn status(&self, index: usize) -> Value {
        let account = self.ctx[index].account.as_str().to_owned();
        self.ctx[index]
            .core
            .call(move |store| store.google_draft_inventory_status(&account))
            .await
            .unwrap()
    }
    async fn rows(&self, index: usize) -> Vec<Value> {
        let account = self.ctx[index].account.as_str().to_owned();
        self.ctx[index]
            .core
            .call(move |store| store.handle("drafts", Some(&account), json!({"limit":100})))
            .await
            .unwrap()["drafts"]
            .as_array()
            .unwrap()
            .clone()
    }
    fn close(self) {
        drop(self.ctx);
        self.writer.join().unwrap();
    }
}

#[tokio::test]
async fn two_accounts_multipage_changed_get_identity_and_unavailable_items_preserve_read_only_sources(
) {
    let h = Harness::new(52);
    {
        let mut state = h.http.state.lock().unwrap();
        state.missing = Some(1);
        state.malformed = Some(2);
        state.oversized = Some(3);
        state.wrong_container = Some(4);
    }
    let (mut one, mut two) = (RemoteDraftWorker::default(), RemoteDraftWorker::default());
    tokio::join!(h.finish(0, &mut one), h.finish(1, &mut two));
    for index in 0..2 {
        let status = h.status(index).await;
        assert_eq!(status["enumeration_complete"], true);
        assert_eq!(status["estimated_total"], 999999);
        assert_eq!(status["source_unavailable"], 4);
        let rows = h.rows(index).await;
        assert!(!rows
            .iter()
            .any(|row| row["remote_source"]["provider_draft_id"] == "wrong-container"));
        let row = rows
            .iter()
            .find(|row| row["remote_source"]["provider_draft_id"] == "d0")
            .unwrap();
        assert_eq!(row["origin"], "google_snapshot");
        assert_eq!(row["read_only"], true);
        assert!(row["google"].is_null() && row["google_send"].is_null());
        assert_eq!(row["remote_source"]["message_id"], "actual-0-v1");
        let request = json!({"draft_id":row["draft_id"],"source_revision":row["remote_source"]["source_revision"]});
        let account = h.ctx[index].account.as_str().to_owned();
        let runtime = crate::local_content::Runtime::new(h.ctx[index].core.clone());
        let exported = runtime
            .handle(
                CancellationToken::new(),
                "google_draft_source",
                Some(&account),
                request,
            )
            .await
            .unwrap();
        assert_eq!(
            std::fs::read(exported["path"].as_str().unwrap()).unwrap(),
            source(&format!("reader-{index}"), 0, 1)
        );
    }
    let before = h.http.state.lock().unwrap().calls.len();
    h.status(0).await;
    h.rows(0).await;
    assert!(matches!(one.step(&h.ctx[0]).await.unwrap(), Step::Idle(_)));
    assert_eq!(before, h.http.state.lock().unwrap().calls.len());
    let db = rusqlite::Connection::open_with_flags(
        h.directory.path().join("profile/store.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM messages", [], |row| row
            .get::<_, u64>(0))
            .unwrap(),
        0,
        "draft inventory creates no received arrivals"
    );
    drop(db);
    h.close();
}

#[tokio::test]
async fn cancelled_raw_resumes_saved_page_without_relisting_and_cached_queries_progress() {
    let mut h = Harness::new(3);
    let mut worker = RemoteDraftWorker::default();
    worker.step(&h.ctx[0]).await.unwrap(); // begin
    worker.step(&h.ctx[0]).await.unwrap(); // saved page
    let gate = Arc::new(Gate::default());
    h.http.state.lock().unwrap().hold = Some(gate.clone());
    let ctx = h.ctx[0].clone();
    let work = tokio::spawn(async move { worker.step(&ctx).await });
    tokio::time::timeout(Duration::from_secs(3), gate.entered.notified())
        .await
        .unwrap();
    assert!(h.status(0).await["pending_items"].as_u64().unwrap() > 0);
    assert_eq!(h.ctx[0].preparation.available_permits(), 0);
    h.ctx[0].cancel.cancel();
    assert!(work.await.unwrap().is_err());
    assert_eq!(h.ctx[0].preparation.available_permits(), 1);
    h.ctx[0].cancel = CancellationToken::new();
    h.finish(0, &mut RemoteDraftWorker::default()).await;
    assert_eq!(
        h.http
            .state
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|(_, path)| path.starts_with("drafts?"))
            .count(),
        1,
        "resume must use stored page first"
    );
    assert_eq!(h.status(0).await["enumeration_complete"], true);
    h.close();
}

#[tokio::test]
async fn expired_continuation_restarts_without_losing_versions_and_refresh_retains_old_export() {
    let h = Harness::new(51);
    h.http.state.lock().unwrap().expire_once = true;
    h.finish(0, &mut RemoteDraftWorker::default()).await;
    assert!(h.http.state.lock().unwrap().expired);
    let rows = h.rows(0).await;
    let row = rows
        .iter()
        .find(|row| row["remote_source"]["provider_draft_id"] == "d0")
        .unwrap();
    let old = json!({"draft_id":row["draft_id"],"source_revision":row["remote_source"]["source_revision"]});
    h.http.state.lock().unwrap().version = 2;
    h.finish(0, &mut RemoteDraftWorker::default()).await;
    let account = h.ctx[0].account.as_str().to_owned();
    let runtime = crate::local_content::Runtime::new(h.ctx[0].core.clone());
    let exported = runtime
        .handle(
            CancellationToken::new(),
            "google_draft_source",
            Some(&account),
            old,
        )
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(exported["path"].as_str().unwrap()).unwrap(),
        source("reader-0", 0, 1)
    );
    let rows = h.rows(0).await;
    let row = rows
        .iter()
        .find(|row| row["remote_source"]["provider_draft_id"] == "d0")
        .unwrap();
    assert_eq!(row["remote_source"]["message_id"], "actual-0-v2");
    let counts =
        h.http
            .state
            .lock()
            .unwrap()
            .calls
            .iter()
            .fold(HashMap::new(), |mut result, (_, path)| {
                *result.entry(path.clone()).or_insert(0_usize) += 1;
                result
            });
    assert!(counts["drafts?maxResults=50"] >= 3);
    drop(runtime);
    h.close();
}

#[tokio::test]
async fn a_late_raw_result_cannot_publish_after_connection_generation_changes() {
    let h = Harness::new(1);
    let mut worker = RemoteDraftWorker::default();
    worker.step(&h.ctx[0]).await.unwrap();
    worker.step(&h.ctx[0]).await.unwrap();
    let gate = Arc::new(Gate::default());
    h.http.state.lock().unwrap().hold = Some(gate.clone());
    let account = h.ctx[0].account.as_str().to_owned();
    let intervene = async {
        tokio::time::timeout(Duration::from_secs(3), gate.entered.notified())
            .await
            .unwrap();
        h.ctx[0]
            .core
            .call(move |store| {
                store.set_google_connection_state(
                    &account,
                    fm_core::live_profile::ConnectionState::Disconnected,
                )
            })
            .await
            .unwrap();
        gate.release.notify_one();
    };
    let (result, ()) = tokio::join!(worker.step(&h.ctx[0]), intervene);
    assert_eq!(result.unwrap_err().code, "stale_google_work");
    assert!(h.rows(0).await.is_empty());
    assert_eq!(h.status(0).await["pending_items"], 1);
    assert_eq!(h.ctx[0].preparation.available_permits(), 1);
    h.close();
}

#[tokio::test]
async fn provider_permission_pause_is_visible_and_explicit_refresh_resumes_the_same_scan() {
    let h = Harness::new(1);
    h.http.state.lock().unwrap().error_once = Some(403);
    let progress = Arc::new(Mutex::new(json!({})));
    let ctx = h.ctx[0].clone();
    let reported = progress.clone();
    let task = tokio::spawn(async move { run(ctx, reported).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        while progress.lock().unwrap()["remote_drafts"] != "paused" {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        progress.lock().unwrap()["remote_drafts_error"],
        "google_permission_denied"
    );
    let cached = h.status(0).await;
    assert_eq!(cached["enumeration_complete"], false);
    assert!(
        cached["paused"].is_null(),
        "cached cursor state is distinct from current runtime failure"
    );
    assert_eq!(h.http.state.lock().unwrap().calls.len(), 1);
    let scope = scope(&h.ctx[0]);
    let refreshed = h.ctx[0]
        .core
        .call(move |store| store.google_draft_inventory_begin(&scope))
        .await
        .unwrap();
    assert_eq!(refreshed["generation"], cached["generation"]);
    h.ctx[0].remote_drafts_wake.notify_one();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if h.status(0).await["enumeration_complete"] == true {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    h.ctx[0].cancel.cancel();
    task.await.unwrap();
    assert_eq!(progress.lock().unwrap()["remote_drafts"], "stopped");
    assert_eq!(h.ctx[0].preparation.available_permits(), 1);
    h.close();
}
