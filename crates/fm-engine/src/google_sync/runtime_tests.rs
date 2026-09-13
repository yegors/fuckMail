//! Shared runtime admission checks; no OAuth, credentials or provider network.
use super::*;
use fm_google::transport::HttpResponse;
use std::{
    future::{poll_fn, Future},
    pin::pin,
    sync::atomic::{AtomicUsize, Ordering},
    task::Poll,
};

#[derive(Default)]
struct RawHttp(AtomicUsize);
impl HttpTransport for RawHttp {
    fn execute<'a>(&'a self, request: HttpRequest, _: &'a CancellationToken) -> HttpFuture<'a> {
        Box::pin(async move {
            assert_eq!(request.method(), HttpMethod::Get);
            assert!(request
                .url()
                .starts_with("https://gmail.googleapis.com/gmail/v1/users/me/messages/"));
            assert!(request.url().ends_with("?format=raw"));
            assert_eq!(
                request.headers()["authorization"],
                "Bearer synthetic-access"
            );
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(HttpResponse {
                status: 404,
                body: b"{}".to_vec(),
                retry_after_secs: None,
                etag: None,
            })
        })
    }
}

struct Contexts {
    first: SyncContext,
    second: SyncContext,
    http: Arc<RawHttp>,
    // These checks exercise transport admission only. Any attempted Store call
    // remains queued and causes a timeout; no alternate Store is substituted.
    _writer_queue: tokio::sync::mpsc::Receiver<crate::Work>,
}
impl Contexts {
    fn new(project: u32, account: u32, in_flight: usize) -> Self {
        let budget = ServiceBudget::from_confirmed_limits(project, account).unwrap();
        let mut config = SchedulerConfig::new(budget, budget);
        config.max_in_flight = in_flight;
        config.max_reads_per_account = 2;
        let quota = quota::QuotaGate::new(config).unwrap();
        let preparation = Arc::new(Semaphore::new(1));
        let calendar_preparation = Arc::new(Semaphore::new(2));
        let http = Arc::new(RawHttp::default());
        let (send, receive) = tokio::sync::mpsc::channel(2);
        let core = CoreBridge::new(send, Arc::new(Semaphore::new(2)));
        let first = SyncContext {
            account: AccountId::new("synthetic-first").unwrap(),
            connection_generation: 1,
            cancel: CancellationToken::new(),
            core: core.clone(),
            http: http.clone(),
            tokens: TokenSource::Fixture("synthetic-access".into()),
            quota: quota.clone(),
            preparation: preparation.clone(),
            calendar_preparation: calendar_preparation.clone(),
        };
        let second = SyncContext {
            account: AccountId::new("synthetic-second").unwrap(),
            connection_generation: 1,
            cancel: CancellationToken::new(),
            core,
            http: http.clone(),
            tokens: TokenSource::Fixture("synthetic-access".into()),
            quota,
            preparation,
            calendar_preparation,
        };
        Self {
            first,
            second,
            http,
            _writer_queue: receive,
        }
    }
}

fn raw_call(ctx: &SyncContext, class: WorkClass) -> GmailCall {
    GmailCall::raw_message(&ctx.account, "synthetic-message")
        .unwrap()
        .with_read_class(class)
        .unwrap()
}

#[test]
fn actual_quota_configuration_preserves_service_units_and_headroom() {
    let limits = json!({"gmail_project_units_per_minute":1_200_000,"gmail_account_units_per_minute":6000,"calendar_project_units_per_minute":10_000,"calendar_account_units_per_minute":600});
    let config = confirmed_limits(&limits).unwrap();
    assert_eq!(config.gmail.project_units_per_minute, 960_000);
    assert_eq!(config.gmail.account_units_per_minute, 4800);
    assert_eq!(config.calendar.project_units_per_minute, 8000);
    assert_eq!(config.calendar.account_units_per_minute, 480);
    assert_eq!(
        (
            config.max_in_flight,
            config.max_reads_per_account,
            config.max_accounts,
            config.max_queued
        ),
        (4, 2, 128, 1024)
    );
    let mut missing = limits.clone();
    missing
        .as_object_mut()
        .unwrap()
        .remove("calendar_account_units_per_minute");
    assert!(confirmed_limits(&missing).is_err());
    let mut invented = limits.clone();
    invented["automatic_defaults"] = json!(true);
    assert!(confirmed_limits(&invented).is_err());
    for invalid in [
        json!(0),
        json!(-1),
        json!("6000"),
        json!(100_000_001),
        json!(100.5),
    ] {
        let mut changed = limits.clone();
        changed["gmail_account_units_per_minute"] = invalid;
        assert!(confirmed_limits(&changed).is_err());
    }
}

#[test]
fn transient_retries_remain_recoverable_after_fast_attempts_and_preserve_provider_delay() {
    for code in [
        "google_transport_interrupted",
        "google_server_unavailable",
        "google_unavailable",
        "calendar_rate_limited",
    ] {
        let error = failure(code, "synthetic transient failure");
        for attempt in [6, 7, 100, u32::MAX] {
            let wait = retry_wait(&error, attempt).expect("offline work must remain recoverable");
            assert!((Duration::from_secs(300)..Duration::from_secs(301)).contains(&wait));
        }
    }
    let limited = CoreError {
        code: "google_rate_limited".into(),
        message: "synthetic cooldown".into(),
        details: json!({"retry_after_secs":900}),
    };
    for attempt in [0, 6] {
        let wait = retry_wait(&limited, attempt).unwrap();
        assert!((Duration::from_secs(900)..Duration::from_secs(901)).contains(&wait));
    }
    for code in [
        "cancelled",
        "stale_google_work",
        "google_reauthorization_required",
        "google_permission_denied",
        "google_invalid_response",
        "storage_error",
    ] {
        assert!(retry_wait(&failure(code, "synthetic terminal failure"), 0).is_none());
        assert!(retry_wait(&failure(code, "synthetic terminal failure"), 6).is_none());
    }
}

#[tokio::test]
async fn quota_waiting_backfill_does_not_hold_memory_needed_by_other_accounts_recent_mail() {
    let h = Contexts::new(100, 100, 4);
    // 80 admitted units/minute: background may spend only 60 of them.
    let spent = h
        .first
        .quota
        .acquire(
            Work {
                account: h.first.account.clone(),
                service: Service::Gmail,
                class: WorkClass::BackfillRead,
                units: 60,
            },
            &h.first.cancel,
        )
        .await
        .unwrap();
    drop(spent);
    let mut blocked = pin!(h
        .first
        .gmail_prepared(raw_call(&h.first, WorkClass::BackfillRead)));
    assert!(matches!(
        poll_fn(|cx| Poll::Ready(blocked.as_mut().poll(cx))).await,
        Poll::Pending
    ));
    assert_eq!(
        h.first.preparation.available_permits(),
        1,
        "quota wait must precede memory admission"
    );
    let (_, memory) = tokio::time::timeout(
        Duration::from_secs(2),
        h.second
            .gmail_prepared(raw_call(&h.second, WorkClass::UserRead)),
    )
    .await
    .expect("reserved quota and memory must serve recent mail while backfill is throttled")
    .unwrap();
    assert_eq!(h.http.0.load(Ordering::SeqCst), 1);
    assert_eq!(h.first.preparation.available_permits(), 0);
    drop(memory);
    h.first.cancel.cancel();
    assert!(blocked.await.is_err());
    assert_eq!(h.first.preparation.available_permits(), 1);
}

#[tokio::test]
async fn cancellation_while_waiting_for_memory_releases_admitted_http_capacity() {
    let h = Contexts::new(1000, 1000, 2);
    let memory = h.first.preparation.clone().acquire_owned().await.unwrap();
    let mut blocked = pin!(h
        .first
        .gmail_prepared(raw_call(&h.first, WorkClass::UserRead)));
    assert!(matches!(
        poll_fn(|cx| Poll::Ready(blocked.as_mut().poll(cx))).await,
        Poll::Pending
    ));
    assert_eq!(h.http.0.load(Ordering::SeqCst), 0);
    h.first.cancel.cancel();
    assert!(blocked.await.is_err());
    let independent = h
        .second
        .quota
        .acquire(
            Work {
                account: AccountId::new("synthetic-third").unwrap(),
                service: Service::Calendar,
                class: WorkClass::UserRead,
                units: 1,
            },
            &h.second.cancel,
        )
        .await
        .unwrap();
    drop(memory);
    let (_, memory) = tokio::time::timeout(
        Duration::from_secs(2),
        h.second
            .gmail_prepared(raw_call(&h.second, WorkClass::UserRead)),
    )
    .await
    .expect("cancelled admission must release its slot")
    .unwrap();
    assert_eq!(h.http.0.load(Ordering::SeqCst), 1);
    drop(memory);
    drop(independent);
}

#[tokio::test]
async fn raw_response_keeps_memory_lease_until_preparation_releases_it() {
    let h = Contexts::new(1000, 1000, 4);
    let (_, first_memory) = h
        .first
        .gmail_prepared(raw_call(&h.first, WorkClass::UserRead))
        .await
        .unwrap();
    let mut second = pin!(h
        .second
        .gmail_prepared(raw_call(&h.second, WorkClass::DeltaRead)));
    assert!(matches!(
        poll_fn(|cx| Poll::Ready(second.as_mut().poll(cx))).await,
        Poll::Pending
    ));
    assert_eq!(
        h.http.0.load(Ordering::SeqCst),
        1,
        "a completed HTTP call still owns downloaded RAW memory"
    );
    drop(first_memory);
    let (_, second_memory) = tokio::time::timeout(Duration::from_secs(2), second)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(h.http.0.load(Ordering::SeqCst), 2);
    drop(second_memory);
    assert_eq!(h.first.preparation.available_permits(), 1);
}

#[tokio::test]
async fn calendar_page_memory_is_shared_and_quota_waiters_do_not_occupy_it() {
    let h = Contexts::new(100, 100, 4);
    let spent = h
        .first
        .quota
        .acquire(
            Work {
                account: h.first.account.clone(),
                service: Service::Calendar,
                class: WorkClass::BackfillRead,
                units: 60,
            },
            &h.first.cancel,
        )
        .await
        .unwrap();
    drop(spent);
    let mut blocked = pin!(h.first.calendar_request(WorkClass::BackfillRead));
    assert!(matches!(
        poll_fn(|cx| Poll::Ready(blocked.as_mut().poll(cx))).await,
        Poll::Pending
    ));
    assert_eq!(h.first.calendar_preparation.available_permits(), 2);
    let (api, first) = tokio::time::timeout(
        Duration::from_secs(2),
        h.second.calendar_request(WorkClass::UserRead),
    )
    .await
    .unwrap()
    .unwrap();
    drop(api);
    let (api, second) = h
        .second
        .calendar_request(WorkClass::UserRead)
        .await
        .unwrap();
    drop(api);
    assert_eq!(h.first.calendar_preparation.available_permits(), 0);
    let mut third = pin!(h.second.calendar_request(WorkClass::UserRead));
    assert!(matches!(
        poll_fn(|cx| Poll::Ready(third.as_mut().poll(cx))).await,
        Poll::Pending
    ));
    drop(first);
    let (api, third_memory) = tokio::time::timeout(Duration::from_secs(2), third)
        .await
        .unwrap()
        .unwrap();
    drop(api);
    h.first.cancel.cancel();
    assert!(blocked.await.is_err());
    drop(second);
    drop(third_memory);
    assert_eq!(h.first.calendar_preparation.available_permits(), 2);
}

#[tokio::test]
async fn calendar_preparation_cancellation_releases_quota_capacity() {
    let h = Contexts::new(1000, 1000, 2);
    let first = h.first.calendar_prepare_permit().await.unwrap();
    let second = h.second.calendar_prepare_permit().await.unwrap();
    let mut waiting = pin!(h.first.calendar_request(WorkClass::UserRead));
    assert!(matches!(
        poll_fn(|cx| Poll::Ready(waiting.as_mut().poll(cx))).await,
        Poll::Pending
    ));
    h.first.cancel.cancel();
    assert!(waiting.await.is_err());
    drop(first);
    drop(second);
    let admitted = tokio::time::timeout(
        Duration::from_secs(2),
        h.second.calendar_request(WorkClass::UserRead),
    )
    .await
    .unwrap()
    .unwrap();
    drop(admitted);
    assert_eq!(h.second.calendar_preparation.available_permits(), 2);
}

#[tokio::test]
async fn admitted_transport_is_one_shot_and_mutations_use_only_their_costed_lane() {
    let h = Contexts::new(1000, 1000, 4);
    let http = h
        .first
        .transport(raw_call(&h.first, WorkClass::UserRead).work());
    let request = || {
        HttpRequest::new(
            HttpMethod::Get,
            "https://gmail.googleapis.com/gmail/v1/users/me/messages/m?format=raw",
        )
        .unwrap()
    };
    http.execute(request(), &h.first.cancel).await.unwrap();
    assert!(matches!(
        http.execute(request(), &h.first.cancel).await,
        Err(TransportError::InvalidRequest)
    ));
    assert_eq!(h.http.0.load(Ordering::SeqCst), 1);
    let mut work = Work {
        account: h.first.account.clone(),
        service: Service::Calendar,
        class: WorkClass::CalendarMutation,
        units: 1,
    };
    let patch = HttpRequest::new(
        HttpMethod::Patch,
        "https://www.googleapis.com/calendar/v3/calendars/c/events/e",
    )
    .unwrap();
    assert!(!mutation_request_matches(&work, &patch));
    work.units = 3;
    assert!(mutation_request_matches(&work, &patch));
    assert!(!mutation_request_matches(
        &work,
        &HttpRequest::new(
            HttpMethod::Post,
            "https://www.googleapis.com/calendar/v3/freeBusy"
        )
        .unwrap()
    ));
    let call = GmailCall::modify_message(&h.first.account, "m", &["STARRED".into()], &[]).unwrap();
    assert!(matches!(
        h.first.gmail(call).await,
        Err(GoogleError::PermissionDenied)
    ));
    let call = GmailCall::send_message(
        &h.first.account,
        b"From: x@example.test\r\n\r\nHuman text",
        None,
    )
    .unwrap();
    assert!(matches!(
        h.first.gmail_mutation(call).await,
        Err(GoogleError::PermissionDenied)
    ));
    assert_eq!(h.http.0.load(Ordering::SeqCst), 1);
}

struct CachedVault {
    account: AccountId,
    token: fm_google::auth::StoredToken,
    loads: AtomicUsize,
    stores: AtomicUsize,
}
impl fm_google::auth::CredentialVault for CachedVault {
    fn load(
        &self,
        account: &AccountId,
    ) -> std::result::Result<Option<fm_google::auth::StoredToken>, GoogleError> {
        assert_eq!(account, &self.account);
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(Some(self.token.clone()))
    }
    fn store(
        &self,
        _: &AccountId,
        _: fm_google::auth::StoredToken,
    ) -> std::result::Result<(), GoogleError> {
        self.stores.fetch_add(1, Ordering::SeqCst);
        Err(GoogleError::StoreUnavailable)
    }
}

struct ParkedMailHttp {
    calls: AtomicUsize,
    cancellations: AtomicUsize,
    entered: tokio::sync::Notify,
    cancelled: tokio::sync::Notify,
    responses: Semaphore,
}
impl Default for ParkedMailHttp {
    fn default() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            cancellations: AtomicUsize::new(0),
            entered: tokio::sync::Notify::new(),
            cancelled: tokio::sync::Notify::new(),
            responses: Semaphore::new(0),
        }
    }
}
impl HttpTransport for ParkedMailHttp {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
        cancel: &'a CancellationToken,
    ) -> HttpFuture<'a> {
        Box::pin(async move {
            // A fresh saved grant must not launch an OAuth flow or token refresh.
            assert_eq!(request.method(), HttpMethod::Get);
            let suffix = request
                .url()
                .strip_prefix("https://gmail.googleapis.com/gmail/v1/users/me/")
                .expect("sync cannot invoke OAuth endpoints");
            assert_eq!(
                request.headers()["authorization"],
                "Bearer synthetic-cached-access"
            );
            let body = if suffix == "profile" {
                json!({"emailAddress":"reader@example.test","messagesTotal":0,"threadsTotal":0,"historyId":"100"})
            } else if suffix.starts_with("messages?") {
                json!({"messages":[],"resultSizeEstimate":0})
            } else if suffix.starts_with("history?") {
                json!({"history":[],"historyId":"100"})
            } else {
                panic!("unexpected synthetic mail request");
            };
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.entered.notify_one();
            tokio::select! {
                _=cancel.cancelled()=> {
                    self.cancellations.fetch_add(1,Ordering::SeqCst);
                    self.cancelled.notify_one();
                    Err(TransportError::Cancelled)
                },
                permit=self.responses.acquire()=> {
                    permit.unwrap().forget();
                    Ok(HttpResponse {status:200,body:serde_json::to_vec(&body).unwrap(),retry_after_secs:None,etag:None})
                },
            }
        })
    }
}
impl ParkedMailHttp {
    async fn wait_calls(&self, expected: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let entered = self.entered.notified();
                if self.calls.load(Ordering::SeqCst) >= expected {
                    break;
                }
                entered.await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "expected worker request {expected}, observed {}",
                self.calls.load(Ordering::SeqCst)
            )
        });
    }
    async fn wait_cancelled(&self) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let cancelled = self.cancelled.notified();
                if self.cancellations.load(Ordering::SeqCst) > 0 {
                    break;
                }
                cancelled.await;
            }
        })
        .await
        .expect("last owner must cancel pending HTTP");
    }
}

struct RuntimeHarness {
    runtime: Arc<Runtime>,
    auth: Arc<google_auth::Runtime>,
    core: CoreBridge,
    account: String,
    http: Arc<ParkedMailHttp>,
    vault: Arc<CachedVault>,
    _directory: tempfile::TempDir,
    writer: std::thread::JoinHandle<()>,
}
impl RuntimeHarness {
    fn new() -> Self {
        const CLIENT: &str = "synthetic-runtime.apps.googleusercontent.com";
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("synthetic-desktop.json");
        std::fs::write(
            &config_path,
            serde_json::to_vec(&json!({"installed":{"client_id":CLIENT,"client_secret":"synthetic-desktop-secret"}})).unwrap(),
        )
        .unwrap();
        let profile = directory.path().join("profile");
        let (send, mut receive) = tokio::sync::mpsc::channel(8);
        let (ready, wait) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            let mut store = Store::open_live(&profile).unwrap();
            let account = store
                .bind_google_account_with_client(
                    "synthetic-runtime-subject",
                    "reader@example.test",
                    &std::collections::BTreeSet::from([
                        "https://www.googleapis.com/auth/gmail.modify".into(),
                    ]),
                    None,
                    CLIENT,
                    config_path.to_str().unwrap(),
                )
                .unwrap();
            ready.send(account).unwrap();
            while let Some(work) = receive.blocking_recv() {
                if let crate::Work::Provider(call) = work {
                    call.run(&mut store);
                } else {
                    panic!("unexpected runtime fixture work");
                }
            }
        });
        let account = wait.recv_timeout(Duration::from_secs(10)).unwrap();
        let core = CoreBridge::new(send, Arc::new(Semaphore::new(8)));
        let http = Arc::new(ParkedMailHttp::default());
        let vault = Arc::new(CachedVault {
            account: AccountId::new(account.clone()).unwrap(),
            token: fm_google::auth::StoredToken {
                access_token: "synthetic-cached-access".into(),
                refresh_token: "synthetic-refresh".into(),
                expires_at_secs: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + 3600,
            },
            loads: AtomicUsize::new(0),
            stores: AtomicUsize::new(0),
        });
        let cancel = CancellationToken::new();
        let auth = google_auth::Runtime::for_test(
            core.clone(),
            http.clone(),
            vault.clone(),
            cancel.clone(),
        );
        let runtime = Runtime::new(core.clone(), auth.clone(), cancel);
        Self {
            runtime,
            auth,
            core,
            account,
            http,
            vault,
            _directory: directory,
            writer,
        }
    }
    async fn start(&self, owner: &str, cancel: CancellationToken) -> Value {
        self.runtime.handle(owner,cancel,"google_sync_start",Some(&self.account),json!({"gmail_project_units_per_minute":100_000,"gmail_account_units_per_minute":10_000,"calendar_project_units_per_minute":100_000,"calendar_account_units_per_minute":10_000})).await.unwrap()
    }
    async fn release(&self, owner: &str) -> Value {
        self.runtime
            .handle(
                owner,
                CancellationToken::new(),
                "google_sync_release",
                Some(&self.account),
                json!({}),
            )
            .await
            .unwrap()
    }
    async fn wait_owner_count(&self, expected: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let count = self
                    .runtime
                    .state
                    .lock()
                    .await
                    .jobs
                    .get(&self.account)
                    .unwrap()
                    .owners
                    .lock()
                    .unwrap()
                    .len();
                if count == expected {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("owner release was not processed");
    }
    async fn wait_stopped(&self) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let status = self
                    .runtime
                    .handle(
                        "observer",
                        CancellationToken::new(),
                        "google_sync_status",
                        Some(&self.account),
                        json!({}),
                    )
                    .await
                    .unwrap();
                if status["accounts"][0]["active"] == false {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("last owner's worker did not finish");
    }
    async fn close(self) {
        self.runtime.shutdown().await;
        self.auth.shutdown().await;
        drop(self.runtime);
        drop(self.auth);
        drop(self.core);
        self.writer.join().unwrap();
    }
}

async fn two_owner_lifecycle(disconnect: bool) {
    let h = RuntimeHarness::new();
    assert_eq!(
        h.http.calls.load(Ordering::SeqCst),
        0,
        "creating runtimes does not start sync or OAuth"
    );
    let gui = CancellationToken::new();
    let cli = CancellationToken::new();
    assert_eq!(h.start("gui", gui.clone()).await["started"], true);
    h.http.wait_calls(1).await;
    assert_eq!(h.start("cli", cli.clone()).await["started"], false);
    assert_eq!(h.start("cli", cli.clone()).await["started"], false);
    h.wait_owner_count(2).await;
    if disconnect {
        gui.cancel();
    } else {
        assert_eq!(h.release("gui").await["active"], true);
    }
    h.wait_owner_count(1).await;
    assert_eq!(h.http.cancellations.load(Ordering::SeqCst), 0);
    h.http.responses.add_permits(1);
    // This is actual new worker HTTP after the first owner has left, proving
    // continued ownership rather than merely an unchanged status flag.
    h.http.wait_calls(2).await;
    if disconnect {
        cli.cancel();
    } else {
        assert_eq!(h.release("cli").await["active"], false);
    }
    h.http.wait_cancelled().await;
    h.wait_stopped().await;
    assert_eq!(h.http.calls.load(Ordering::SeqCst), 2);
    assert_eq!(h.http.cancellations.load(Ordering::SeqCst), 1);
    assert!(h.vault.loads.load(Ordering::SeqCst) >= 2);
    assert_eq!(
        h.vault.stores.load(Ordering::SeqCst),
        0,
        "sync with a fresh grant neither reconnects nor changes credentials"
    );
    h.close().await;
}

#[tokio::test]
async fn releasing_one_real_runtime_owner_preserves_other_owner_and_final_release_cancels() {
    two_owner_lifecycle(false).await;
}

#[tokio::test]
async fn disconnecting_one_real_runtime_owner_preserves_other_owner_until_last_disconnect() {
    two_owner_lifecycle(true).await;
}

#[tokio::test]
async fn temporary_calendar_request_owner_survives_gui_exit_and_preserves_normal_cli_owner() {
    let h = RuntimeHarness::new();
    let gui = CancellationToken::new();
    let cli = CancellationToken::new();
    h.start("gui", gui.clone()).await;
    h.http.wait_calls(1).await;
    let (context, operation) = h
        .runtime
        .operation_context(&h.account, "cli", cli.clone())
        .await
        .unwrap();
    gui.cancel();
    h.wait_owner_count(1).await;
    assert!(!context.cancel.is_cancelled());
    h.http.responses.add_permits(1);
    h.http.wait_calls(2).await;
    // A separate sync start by this CLI remains registered when its temporary
    // operation ends; the two registrations must not share a removable key.
    assert_eq!(h.start("cli", cli.clone()).await["started"], false);
    h.wait_owner_count(2).await;
    drop(operation);
    h.wait_owner_count(1).await;
    assert!(!context.cancel.is_cancelled());
    assert_eq!(h.release("cli").await["active"], false);
    h.http.wait_cancelled().await;
    h.wait_stopped().await;
    drop(context);
    h.close().await;
}
