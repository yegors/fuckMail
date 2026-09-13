//! Explicit Google read sessions share quotas, one Store writer and cancellation.
pub(crate) mod calendar;
pub(crate) mod calendar_writes;
pub(crate) mod drafts;
pub(crate) mod labels;
pub(crate) mod mail;
pub(crate) mod mail_mutations;
mod quota;
pub(crate) mod remote_drafts;
#[cfg(test)]
mod runtime_tests;
pub(crate) mod send;

use crate::{core_tasks::CoreBridge, google_auth};
use fm_calendar::google_api::GoogleCalendarApi;
use fm_core::{live_profile::ConnectionState, CoreError, Result, Store};
#[cfg(test)]
use fm_google::scheduler::ServiceBudget;
use fm_google::{
    auth::ScopeSet,
    gmail_api::{GmailApi, GmailCall, GmailReply},
    scheduler::{CooldownScope, SchedulerConfig, Service, Work, WorkClass},
    transport::{
        CancellationToken, HttpFuture, HttpMethod, HttpRequest, HttpTransport, TransportError,
    },
    AccountId, GoogleError,
};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore};

#[derive(Clone)]
enum TokenSource {
    Native(Arc<google_auth::Runtime>),
    #[cfg(test)]
    Fixture(String),
}
impl TokenSource {
    async fn load(&self, account: &AccountId, cancel: &CancellationToken) -> Result<String> {
        match self {
            Self::Native(auth) => auth.access_token(account.as_str(), cancel).await,
            #[cfg(test)]
            Self::Fixture(token) => Ok(token.clone()),
        }
    }
}

#[derive(Clone)]
pub(crate) struct SyncContext {
    pub(crate) account: AccountId,
    pub(crate) connection_generation: i64,
    pub(crate) cancel: CancellationToken,
    core: CoreBridge,
    http: Arc<dyn HttpTransport>,
    tokens: TokenSource,
    quota: Arc<quota::QuotaGate>,
    preparation: Arc<Semaphore>,
    calendar_preparation: Arc<Semaphore>,
    remote_drafts_wake: Arc<Notify>,
}
impl SyncContext {
    pub(crate) async fn core_call<T, F>(&self, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Store) -> Result<T> + Send + 'static,
    {
        let account = self.account.as_str().to_owned();
        let generation = self.connection_generation;
        let cancel = self.cancel.clone();
        tokio::select! {
            _=self.cancel.cancelled()=>Err(failure("cancelled","Google work stopped")),
            result=self.core.call(move |store| {
                if cancel.is_cancelled() { return Err(failure("cancelled","Google work stopped")); }
                if !store.google_account_bindings()?.iter().any(|binding| binding.account==account && binding.state==ConnectionState::Connected && binding.connection_generation==generation) {
                    return Err(failure("stale_google_work","Google connection changed"));
                }
                operation(store)
            })=>result,
        }
    }
    pub(crate) async fn prepare_permit(&self) -> Result<OwnedSemaphorePermit> {
        tokio::select! {
            _=self.cancel.cancelled()=>Err(failure("cancelled","Google work stopped")),
            permit=self.preparation.clone().acquire_owned()=>permit.map_err(|_|failure("cancelled","Google work stopped")),
        }
    }
    pub(crate) async fn gmail(
        &self,
        call: GmailCall,
    ) -> std::result::Result<GmailReply, GoogleError> {
        if call.work().account != self.account || !call.work().class.is_read() {
            return Err(GoogleError::PermissionDenied);
        }
        let http = Arc::new(self.transport(call.work()));
        // Transport replaces this non-secret placeholder only after quota admission.
        GmailApi::new(self.account.clone(), http)
            .execute(call, "engine-managed", &self.cancel)
            .await
    }
    pub(crate) async fn gmail_mutation(
        &self,
        call: GmailCall,
    ) -> std::result::Result<GmailReply, GoogleError> {
        use fm_google::gmail_api::GmailMethod;
        if call.work().account != self.account
            || !matches!(
                call.method(),
                GmailMethod::MessageModify
                    | GmailMethod::MessageTrash
                    | GmailMethod::MessageRestore
            )
        {
            return Err(GoogleError::PermissionDenied);
        }
        let http = Arc::new(self.transport(call.work()));
        GmailApi::new(self.account.clone(), http)
            .execute(call, "engine-managed", &self.cancel)
            .await
    }
    pub(crate) async fn gmail_prepared(
        &self,
        call: GmailCall,
    ) -> std::result::Result<(GmailReply, OwnedSemaphorePermit), GoogleError> {
        let work = call.work();
        if work.account != self.account
            || !matches!(
                call.method(),
                fm_google::gmail_api::GmailMethod::MessageRaw
                    | fm_google::gmail_api::GmailMethod::LabelsList
                    | fm_google::gmail_api::GmailMethod::DraftGet
                    | fm_google::gmail_api::GmailMethod::DraftsList
            )
        {
            return Err(GoogleError::PermissionDenied);
        }
        let quota = self
            .quota
            .acquire(work.clone(), &self.cancel)
            .await
            .map_err(|_| GoogleError::TransportInterrupted)?;
        let memory = self
            .prepare_permit()
            .await
            .map_err(|_| GoogleError::TransportInterrupted)?;
        let mut http = self.transport(work);
        http.admitted = true;
        http.permit = Mutex::new(Some(quota));
        let reply = GmailApi::new(self.account.clone(), Arc::new(http))
            .execute(call, "engine-managed", &self.cancel)
            .await?;
        Ok((reply, memory))
    }
    pub(crate) async fn gmail_draft_request(
        &self,
        method: fm_google::gmail_api::GmailMethod,
    ) -> Result<(
        GmailApi<QuotaTransport>,
        OwnedSemaphorePermit,
        Arc<AtomicBool>,
    )> {
        use fm_google::gmail_api::GmailMethod;
        if !matches!(method, GmailMethod::DraftCreate | GmailMethod::DraftUpdate) {
            return Err(failure(
                "invalid_request",
                "Only draft create/update is admitted",
            ));
        }
        self.gmail_upload_request(method, WorkClass::DraftMutation)
            .await
    }
    pub(crate) async fn gmail_send_request(
        &self,
        method: fm_google::gmail_api::GmailMethod,
    ) -> Result<(
        GmailApi<QuotaTransport>,
        OwnedSemaphorePermit,
        Arc<AtomicBool>,
    )> {
        use fm_google::gmail_api::GmailMethod;
        if !matches!(method, GmailMethod::MessageSend | GmailMethod::DraftSend) {
            return Err(failure(
                "invalid_request",
                "Only explicit message/draft submission is admitted",
            ));
        }
        self.gmail_upload_request(method, WorkClass::Submission)
            .await
    }
    async fn gmail_upload_request(
        &self,
        method: fm_google::gmail_api::GmailMethod,
        class: WorkClass,
    ) -> Result<(
        GmailApi<QuotaTransport>,
        OwnedSemaphorePermit,
        Arc<AtomicBool>,
    )> {
        let work = Work {
            account: self.account.clone(),
            service: Service::Gmail,
            class,
            units: method.quota_units(),
        };
        // Queued work retains identifiers only. Admission precedes frozen MIME
        // loading and base64 encoding, and this same lease is charged at HTTP.
        let quota = self
            .quota
            .acquire(work.clone(), &self.cancel)
            .await
            .map_err(|_| failure("cancelled", "Google draft work stopped"))?;
        let memory = self.prepare_permit().await?;
        let mut http = self.transport(work);
        http.admitted = true;
        http.permit = Mutex::new(Some(quota));
        let dispatched = Arc::new(AtomicBool::new(false));
        http.dispatched = Some(dispatched.clone());
        Ok((
            GmailApi::new(self.account.clone(), Arc::new(http)),
            memory,
            dispatched,
        ))
    }
    pub(crate) async fn calendar_request(
        &self,
        class: WorkClass,
    ) -> Result<(GoogleCalendarApi<QuotaTransport>, OwnedSemaphorePermit)> {
        if !class.is_read() {
            return Err(failure(
                "invalid_request",
                "Calendar acquisition requires a read operation",
            ));
        }
        let work = Work {
            account: self.account.clone(),
            service: Service::Calendar,
            class,
            units: 1,
        };
        // Quota waiters carry identifiers only. A quota-paused account cannot
        // occupy a decoded-page slot needed by another account's current view.
        let quota = self
            .quota
            .acquire(work.clone(), &self.cancel)
            .await
            .map_err(|_| failure("cancelled", "Calendar acquisition stopped"))?;
        let memory = self.calendar_prepare_permit().await?;
        let mut http = self.transport(work);
        http.admitted = true;
        http.permit = Mutex::new(Some(quota));
        Ok((
            GoogleCalendarApi::new(self.account.clone(), Arc::new(http)),
            memory,
        ))
    }
    pub(crate) async fn calendar_prepare_permit(&self) -> Result<OwnedSemaphorePermit> {
        tokio::select! {
            _=self.cancel.cancelled()=>Err(failure("cancelled","Calendar preparation stopped")),
            permit=self.calendar_preparation.clone().acquire_owned()=>permit.map_err(|_|failure("cancelled","Calendar preparation stopped")),
        }
    }
    pub(crate) async fn calendar_mutation_request(
        &self,
        units: u32,
    ) -> Result<GoogleCalendarApi<QuotaTransport>> {
        if !matches!(units, 1 | 3) {
            return Err(failure("invalid_request", "Invalid Calendar mutation cost"));
        }
        let work = Work {
            account: self.account.clone(),
            service: Service::Calendar,
            class: WorkClass::CalendarMutation,
            units,
        };
        let quota = self
            .quota
            .acquire(work.clone(), &self.cancel)
            .await
            .map_err(|_| failure("cancelled", "Calendar operation stopped"))?;
        let mut http = self.transport(work);
        http.admitted = true;
        http.permit = Mutex::new(Some(quota));
        Ok(GoogleCalendarApi::new(self.account.clone(), Arc::new(http)))
    }
    fn transport(&self, work: Work) -> QuotaTransport {
        QuotaTransport {
            context: self.clone(),
            work,
            admitted: false,
            permit: Mutex::new(None),
            used: AtomicBool::new(false),
            dispatched: None,
        }
    }
    #[cfg(test)]
    pub(crate) fn for_test(
        core: CoreBridge,
        account: AccountId,
        connection_generation: i64,
        http: Arc<dyn HttpTransport>,
        access_token: String,
        cancel: CancellationToken,
    ) -> Self {
        let budget = ServiceBudget::from_confirmed_limits(100_000, 10_000).unwrap();
        Self {
            core,
            account,
            connection_generation,
            http,
            tokens: TokenSource::Fixture(access_token),
            cancel,
            quota: quota::QuotaGate::new(SchedulerConfig::new(budget, budget)).unwrap(),
            preparation: Arc::new(Semaphore::new(1)),
            calendar_preparation: Arc::new(Semaphore::new(2)),
            remote_drafts_wake: Arc::new(Notify::new()),
        }
    }
}

pub(crate) struct QuotaTransport {
    context: SyncContext,
    work: Work,
    admitted: bool,
    permit: Mutex<Option<quota::Lease>>,
    used: AtomicBool,
    dispatched: Option<Arc<AtomicBool>>,
}
impl HttpTransport for QuotaTransport {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
        cancel: &'a CancellationToken,
    ) -> HttpFuture<'a> {
        Box::pin(async move {
            if self.used.swap(true, Ordering::AcqRel) {
                return Err(TransportError::InvalidRequest);
            }
            let admitted = self.permit.lock().unwrap().take();
            // Typed mutation workers have a separate one-shot lane. Read workers
            // cannot borrow that lane. Explicit submission has its own exact
            // typed allowlist and never borrows a draft-write quota lease.
            let read_request = request.method() == HttpMethod::Get
                || (self.work.service == Service::Calendar
                    && request.method() == HttpMethod::Post
                    && request.url() == "https://www.googleapis.com/calendar/v3/freeBusy");
            let allowed = if self.work.class.is_read() {
                read_request
            } else {
                matches!(
                    (self.work.service, self.work.class),
                    (Service::Gmail, WorkClass::StateMutation)
                        | (Service::Gmail, WorkClass::DraftMutation)
                        | (Service::Gmail, WorkClass::Submission)
                        | (Service::Calendar, WorkClass::CalendarMutation)
                ) && mutation_request_matches(&self.work, &request)
            };
            if !allowed {
                return Err(TransportError::InvalidRequest);
            }
            let permit = if self.admitted {
                admitted.ok_or(TransportError::InvalidRequest)?
            } else {
                self.context
                    .quota
                    .acquire(self.work.clone(), cancel)
                    .await?
            };
            let token = self
                .context
                .tokens
                .load(&self.context.account, cancel)
                .await
                .map_err(|_| TransportError::Cancelled)?;
            let request = request.header("authorization", &format!("Bearer {token}"))?;
            permit.dispatch(cancel)?;
            if let Some(dispatched) = &self.dispatched {
                dispatched.store(true, Ordering::Release);
            }
            let response = self.context.http.execute(request, cancel).await;
            if let Ok(reply) = &response {
                if let GoogleError::RateLimited { retry_after_secs } = fm_google::classify_api_error(
                    reply.status,
                    &reply.body,
                    reply.retry_after_secs,
                    false,
                ) {
                    let delay = retry_after_secs.unwrap_or(60).max(1);
                    // Provider reason may not identify the exhausted quota. Pausing
                    // this service's project is conservative and keeps the other
                    // service available; actual configured budgets remain separate.
                    self.context
                        .quota
                        .cooldown(CooldownScope::Project(self.work.service), delay);
                    self.context.quota.cooldown(
                        CooldownScope::Account(self.context.account.clone(), self.work.service),
                        delay,
                    );
                }
            }
            response
        })
    }
}

fn mutation_request_matches(work: &Work, request: &HttpRequest) -> bool {
    let path = request.url().split('?').next().unwrap_or("");
    match work.service {
        Service::Gmail => {
            if work.class == WorkClass::Submission {
                return request.method() == HttpMethod::Post
                    && work.units == 100
                    && matches!(
                        request.url(),
                        "https://gmail.googleapis.com/gmail/v1/users/me/messages/send"
                            | "https://gmail.googleapis.com/gmail/v1/users/me/drafts/send"
                    );
            }
            if work.class == WorkClass::DraftMutation {
                let root = "https://gmail.googleapis.com/gmail/v1/users/me/drafts";
                return match (request.method(), work.units) {
                    (HttpMethod::Post, 10) => path == root,
                    (HttpMethod::Put, 15) => path
                        .strip_prefix(&format!("{root}/"))
                        .is_some_and(|id| !id.is_empty() && !id.contains('/')),
                    _ => false,
                };
            }
            if work.class != WorkClass::StateMutation {
                return false;
            }
            let Some(tail) =
                path.strip_prefix("https://gmail.googleapis.com/gmail/v1/users/me/messages/")
            else {
                return false;
            };
            let parts: Vec<_> = tail.split('/').collect();
            request.method() == HttpMethod::Post
                && parts.len() == 2
                && !parts[0].is_empty()
                && matches!(
                    (parts[1], work.units),
                    ("modify" | "untrash", 5) | ("trash", 20)
                )
        }
        Service::Calendar => {
            let Some(tail) = path.strip_prefix("https://www.googleapis.com/calendar/v3/calendars/")
            else {
                return false;
            };
            let parts: Vec<_> = tail.split('/').collect();
            if parts.len() < 2 || parts[0].is_empty() || parts[1] != "events" {
                return false;
            }
            match (request.method(), work.units, parts.len()) {
                (HttpMethod::Post, 1, 2) => true,
                (HttpMethod::Patch, 3, 3)
                | (HttpMethod::Put, 1, 3)
                | (HttpMethod::Delete, 1, 3) => !parts[2].is_empty(),
                _ => false,
            }
        }
    }
}

fn failure(code: &str, message: &str) -> CoreError {
    CoreError {
        code: code.into(),
        message: message.into(),
        details: Value::Null,
    }
}

fn confirmed_limits(params: &Value) -> Result<SchedulerConfig> {
    SchedulerConfig::from_confirmed_json(params).map_err(|_| {
        failure(
            "invalid_request",
            "Supply the four actual Google Console quota limits within the supported range",
        )
    })
}

struct Job {
    context: SyncContext,
    owners: Arc<std::sync::Mutex<std::collections::BTreeMap<String, CancellationToken>>>,
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
    progress: Arc<std::sync::Mutex<Value>>,
}
struct OperationOwner {
    owners: Arc<Mutex<std::collections::BTreeMap<String, CancellationToken>>>,
    key: String,
}
impl Drop for OperationOwner {
    fn drop(&mut self) {
        self.owners.lock().unwrap().remove(&self.key);
    }
}
struct RuntimeState {
    quota: Option<(Value, Arc<quota::QuotaGate>)>,
    jobs: std::collections::BTreeMap<String, Job>,
    stopping: bool,
}
pub(crate) struct Runtime {
    core: CoreBridge,
    auth: Arc<google_auth::Runtime>,
    cancel: CancellationToken,
    preparation: Arc<Semaphore>,
    calendar_preparation: Arc<Semaphore>,
    state: tokio::sync::Mutex<RuntimeState>,
}
impl Runtime {
    pub(crate) fn new(
        core: CoreBridge,
        auth: Arc<google_auth::Runtime>,
        cancel: CancellationToken,
    ) -> Arc<Self> {
        Arc::new(Self {
            core,
            auth,
            cancel,
            preparation: Arc::new(Semaphore::new(1)),
            calendar_preparation: Arc::new(Semaphore::new(2)),
            state: tokio::sync::Mutex::new(RuntimeState {
                quota: None,
                jobs: Default::default(),
                stopping: false,
            }),
        })
    }
    pub(crate) fn handles(method: &str) -> bool {
        matches!(
            method,
            "google_sync_start"
                | "google_sync_stop"
                | "google_sync_release"
                | "google_sync_status"
                | "google_sync_busy"
                | "google_calendar_write"
                | "google_calendar_reconcile"
                | "google_calendar_operation"
                | "google_calendar_resume"
                | "google_calendar_abandon"
                | "google_labels_refresh"
                | "google_drafts_refresh"
                | "google_draft_sync"
                | "google_draft_status"
                | "google_draft_reconcile"
                | "google_draft_resolve"
                | "google_send"
                | "google_send_status"
                | "google_send_reconcile"
                | "google_send_abandon"
        )
    }
    async fn operation_context(
        &self,
        account: &str,
        owner_id: &str,
        owner_cancel: CancellationToken,
    ) -> Result<(SyncContext, OperationOwner)> {
        if owner_cancel.is_cancelled() || self.cancel.is_cancelled() {
            return Err(failure("cancelled", "Google operation stopped"));
        }
        let state = self.state.lock().await;
        if owner_cancel.is_cancelled() || self.cancel.is_cancelled() {
            return Err(failure("cancelled", "Google operation stopped"));
        }
        let job = state
            .jobs
            .get(account)
            .filter(|job| !job.task.is_finished())
            .ok_or_else(|| {
                failure(
                    "sync_not_running",
                    "Start this account's sync session first",
                )
            })?;
        let mut owners = job.owners.lock().unwrap();
        if job.cancel.is_cancelled() || state.stopping {
            return Err(failure(
                "sync_not_running",
                "Start this account's sync session first",
            ));
        }
        owners.retain(|_, cancel| !cancel.is_cancelled());
        if owners.len() >= 128 {
            return Err(failure("busy", "Too many Google operation owners"));
        }
        let key = format!("operation:{owner_id}:{}", fm_ipc::random_token());
        owners.insert(key.clone(), owner_cancel);
        Ok((
            job.context.clone(),
            OperationOwner {
                owners: job.owners.clone(),
                key,
            },
        ))
    }
    pub(crate) async fn handle(
        &self,
        owner_id: &str,
        owner_cancel: CancellationToken,
        method: &str,
        account: Option<&str>,
        params: Value,
    ) -> Result<Value> {
        if method == "google_sync_status" {
            if params.as_object().is_none_or(|p| !p.is_empty()) {
                return Err(failure(
                    "invalid_request",
                    "Sync status takes no parameters",
                ));
            }
            let state = self.state.lock().await;
            let accounts:Vec<Value>=state.jobs.iter().filter(|(id,_)|account.is_none_or(|a|a==id.as_str())).map(|(id,job)| {
                let active=!job.task.is_finished() && !job.cancel.is_cancelled();
                let owned=active && job.owners.lock().unwrap().get(owner_id).is_some_and(|cancel|!cancel.is_cancelled());
                json!({"account":id,"active":active,"owned":owned,"progress":job.progress.lock().unwrap().clone()})
            }).collect();
            return Ok(
                json!({"accounts":accounts,"configured":state.quota.is_some(),"limits":state.quota.as_ref().map(|(limits,_)|limits)}),
            );
        }
        let account = account
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                let error = failure("account_required", "Choose a Google account");
                if method == "google_send" {
                    send::predispatch_error(error)
                } else {
                    error
                }
            })?
            .to_owned();
        if method == "google_drafts_refresh" {
            if params.as_object().is_none_or(|object| !object.is_empty()) {
                return Err(failure(
                    "invalid_request",
                    "Draft inventory refresh takes no parameters",
                ));
            }
            let (context, _owner) = self
                .operation_context(&account, owner_id, owner_cancel.clone())
                .await?;
            // This RPC only changes bounded durable metadata. The sole account
            // worker owns HTTP and keeps running only while a sync owner exists.
            let scope = remote_drafts::scope(&context);
            let wake = context.remote_drafts_wake.clone();
            let reply = context
                .core_call(move |store| {
                    if owner_cancel.is_cancelled() {
                        return Err(failure("cancelled", "Draft inventory refresh stopped"));
                    }
                    let reply = store.google_draft_inventory_begin(&scope)?;
                    // Commit and wake belong together even if the requesting
                    // socket closes before its short metadata reply is read.
                    wake.notify_one();
                    Ok(reply)
                })
                .await?;
            return Ok(reply);
        }
        if method.starts_with("google_draft_") {
            let writing = matches!(method, "google_draft_sync" | "google_draft_resolve");
            drafts::validate(&params, writing)?;
            if writing && ((method == "google_draft_resolve") != params.get("action").is_some()) {
                return Err(failure(
                    "invalid_request",
                    "Choose the matching draft sync or conflict action",
                ));
            }
            if method == "google_draft_status" {
                let draft_id = params["draft_id"].as_str().unwrap().to_owned();
                return self
                    .core
                    .call(move |store| store.google_draft_status(&account, &draft_id))
                    .await;
            }
        }
        if method == "google_send" || method.starts_with("google_send_") {
            send::validate(method, &params).map_err(|error| {
                if method == "google_send" {
                    send::predispatch_error(error)
                } else {
                    error
                }
            })?;
            if method == "google_send_status" {
                let request_id = params["request_id"].as_str().unwrap().to_owned();
                return self
                    .core
                    .call(move |store| store.google_send_status(&account, &request_id))
                    .await;
            }
            if method == "google_send_abandon" {
                return self
                    .core
                    .call(move |store| {
                        if owner_cancel.is_cancelled() {
                            return Err(failure("cancelled", "Google send recovery stopped"));
                        }
                        store.google_send_abandon(&account, &params)
                    })
                    .await;
            }
        }
        if method == "google_calendar_operation" {
            operation_params(&params)?;
            let request_id = params["request_id"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 256)
                .ok_or_else(|| failure("invalid_request", "Choose a Calendar operation"))?
                .to_owned();
            return self
                .core
                .call(move |store| store.calendar_live_status(&account, &request_id))
                .await;
        }
        if method == "google_calendar_abandon" {
            if params.as_object().is_none_or(|p| {
                p.len() != 2
                    || !p.contains_key("request_id")
                    || !p.contains_key("expected_revision")
            }) {
                return Err(failure(
                    "invalid_request",
                    "Choose a Calendar operation and its current revision",
                ));
            }
            let request_id = operation_id(&params)?.to_owned();
            let expected = params["expected_revision"]
                .as_i64()
                .filter(|v| *v >= 0)
                .ok_or_else(|| {
                    failure(
                        "invalid_request",
                        "Choose the current Calendar operation revision",
                    )
                })?;
            return self
                .core
                .call(move |store| {
                    if owner_cancel.is_cancelled() {
                        return Err(failure("cancelled", "Calendar operation stopped"));
                    }
                    store.calendar_live_cancel(&account, &request_id, expected)
                })
                .await;
        }
        if matches!(
            method,
            "google_calendar_write"
                | "google_calendar_reconcile"
                | "google_calendar_resume"
                | "google_labels_refresh"
                | "google_draft_sync"
                | "google_draft_reconcile"
                | "google_draft_resolve"
                | "google_send"
                | "google_send_reconcile"
        ) {
            let (context, _operation_owner) = self
                .operation_context(&account, owner_id, owner_cancel.clone())
                .await
                .map_err(|error| {
                    if method == "google_send" {
                        send::predispatch_error(error)
                    } else {
                        error
                    }
                })?;
            let cancel = context.cancel.child_token();
            let relay_cancel = cancel.clone();
            let relay = tokio::spawn(async move {
                tokio::select! {_=owner_cancel.cancelled()=>relay_cancel.cancel(),_=relay_cancel.cancelled()=>{}}
            });
            let context = SyncContext { cancel, ..context };
            // Account generation is checked by Core before dispatch. The request
            // owns this explicit operation; accepted outcome recording survives
            // cancellation and never authorizes a second provider attempt.
            let result = async {
                match method {
                    "google_send" => send::execute(&context, params).await,
                    "google_send_reconcile" => {
                        send::reconcile(&context, params["request_id"].as_str().unwrap()).await
                    }
                    "google_draft_sync" | "google_draft_resolve" => {
                        drafts::execute(&context, params).await
                    }
                    "google_draft_reconcile" => {
                        drafts::reconcile(&context, params["draft_id"].as_str().unwrap()).await
                    }
                    "google_calendar_write" => calendar_writes::execute(&context, params).await,
                    "google_calendar_resume" => {
                        operation_params(&params)?;
                        let request_id = operation_id(&params)?.to_owned();
                        let account = context.account.as_str().to_owned();
                        let request = context
                            .core_call(move |store| {
                                store.calendar_live_request(&account, &request_id)
                            })
                            .await?;
                        calendar_writes::execute(&context, request).await
                    }
                    "google_labels_refresh" => {
                        if params.as_object().is_none_or(|p| !p.is_empty()) {
                            return Err(failure(
                                "invalid_request",
                                "Label refresh takes no parameters",
                            ));
                        }
                        labels::refresh(&context).await
                    }
                    _ => {
                        operation_params(&params)?;
                        calendar_writes::reconcile(&context, operation_id(&params)?).await
                    }
                }
            }
            .await;
            context.cancel.cancel();
            relay.abort();
            return result;
        }
        if method == "google_sync_release" {
            if params.as_object().is_none_or(|p| !p.is_empty()) {
                return Err(failure(
                    "invalid_request",
                    "Sync release takes no parameters",
                ));
            }
            let mut active = false;
            if let Some(job) = self.state.lock().await.jobs.get(&account) {
                let mut owners = job.owners.lock().unwrap();
                owners.remove(owner_id);
                owners.retain(|_, cancel| !cancel.is_cancelled());
                active =
                    !owners.is_empty() && !job.cancel.is_cancelled() && !job.task.is_finished();
                if owners.is_empty() {
                    job.cancel.cancel();
                }
            }
            return Ok(json!({"account":account,"session_stopped":true,"active":active}));
        }
        if method == "google_sync_busy" {
            if params.as_object().is_none_or(|p| {
                p.len() != 3
                    || p.keys()
                        .any(|key| !matches!(key.as_str(), "from" | "to" | "time_zone"))
            }) {
                return Err(failure(
                    "invalid_request",
                    "Choose a Calendar date range and IANA time zone",
                ));
            }
            let from = params["from"]
                .as_str()
                .and_then(|v| chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").ok())
                .ok_or_else(|| failure("invalid_request", "Invalid Calendar start date"))?;
            let to = params["to"]
                .as_str()
                .and_then(|v| chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").ok())
                .ok_or_else(|| failure("invalid_request", "Invalid Calendar end date"))?;
            let zone = params["time_zone"]
                .as_str()
                .filter(|v| v.len() <= 256)
                .ok_or_else(|| failure("invalid_request", "Invalid Calendar time zone"))?;
            if to <= from || (to - from).num_days() > 42 {
                return Err(failure(
                    "invalid_request",
                    "Free/busy requests cover at most 42 days",
                ));
            }
            let window = fm_calendar::Window::from_dates(from, to, zone)
                .map_err(|_| failure("invalid_request", "Invalid Calendar window"))?;
            let context = self
                .state
                .lock()
                .await
                .jobs
                .get(&account)
                .filter(|job| !job.task.is_finished())
                .map(|job| job.context.clone())
                .ok_or_else(|| {
                    failure(
                        "sync_not_running",
                        "Start this account's sync session first",
                    )
                })?;
            let count = tokio::select! { _=owner_cancel.cancelled()=>return Err(failure("cancelled","Calendar request stopped")), result=calendar::refresh_busy(&context,window)=>result? };
            return Ok(
                json!({"account":account,"calendars":count,"from":params["from"],"to":params["to"],"time_zone":zone}),
            );
        }
        if method == "google_sync_stop" {
            if params.as_object().is_none_or(|p| !p.is_empty()) {
                return Err(failure("invalid_request", "Sync stop takes no parameters"));
            }
            let job = self.state.lock().await.jobs.remove(&account);
            if let Some(job) = job {
                job.cancel.cancel();
                let _ = job.task.await;
            }
            return Ok(json!({"account":account,"stopped":true}));
        }
        if method != "google_sync_start" {
            return Err(failure("unknown_method", "Unknown Google sync method"));
        }
        let config = confirmed_limits(&params)?;
        if owner_cancel.is_cancelled() || self.cancel.is_cancelled() {
            return Err(failure("cancelled", "Google work stopped"));
        }
        let id = account.clone();
        let binding = self
            .core
            .call(move |store| {
                store
                    .google_account_bindings()?
                    .into_iter()
                    .find(|binding| binding.account == id)
                    .ok_or_else(|| failure("account_not_found", "Google account does not exist"))
            })
            .await?;
        if binding.state != ConnectionState::Connected {
            return Err(failure(
                "reauthorization_required",
                "Reconnect this Google account",
            ));
        }
        let scopes = ScopeSet::new(&binding.scopes);
        let has_mail = scopes.mail_read();
        let has_mail_write = scopes.mail_write();
        let has_calendar = binding
            .scopes
            .contains("https://www.googleapis.com/auth/calendar.calendarlist.readonly")
            && binding
                .scopes
                .contains("https://www.googleapis.com/auth/calendar.events");
        if !has_mail && !has_calendar {
            return Err(failure(
                "permission_denied",
                "Grant access to mail or Calendar before syncing",
            ));
        }
        let account_cancel = self.auth.account_work_cancel(&account).await?;
        let mut state = self.state.lock().await;
        if state.stopping
            || owner_cancel.is_cancelled()
            || self.cancel.is_cancelled()
            || account_cancel.is_cancelled()
        {
            return Err(failure("cancelled", "Google work stopped"));
        }
        if let Some((limits, _)) = &state.quota {
            if limits != &params {
                return Err(failure(
                    "quota_configuration_conflict",
                    "Restart the engine to change shared project quota limits",
                ));
            }
        }
        if let Some(job) = state.jobs.get(&account) {
            if !job.task.is_finished()
                && !job.cancel.is_cancelled()
                && job.context.connection_generation == binding.connection_generation
            {
                let mut owners = job.owners.lock().unwrap();
                owners.retain(|_, cancel| !cancel.is_cancelled());
                if owners.len() >= 128 && !owners.contains_key(owner_id) {
                    return Err(failure("busy", "Too many sync sessions"));
                }
                if !job.cancel.is_cancelled() {
                    owners.insert(owner_id.to_owned(), owner_cancel);
                    return Ok(json!({"account":account,"started":false,"active":true}));
                }
            }
        }
        let quota = match &state.quota {
            Some((_, gate)) => gate.clone(),
            None => {
                let gate = quota::QuotaGate::new(config)
                    .map_err(|_| failure("invalid_request", "Google limits are invalid"))?;
                state.quota = Some((params, gate.clone()));
                gate
            }
        };
        let account_id = AccountId::new(account.clone())
            .map_err(|_| failure("invalid_request", "Invalid Google account"))?;
        quota.register(account_id.clone()).map_err(|_| {
            failure(
                "account_limit",
                "This engine session has reached its Google account limit",
            )
        })?;
        let cancel = account_cancel.child_token();
        let context = SyncContext {
            core: self.core.clone(),
            account: account_id,
            connection_generation: binding.connection_generation,
            cancel: cancel.clone(),
            http: self.auth.http_transport(),
            tokens: TokenSource::Native(self.auth.clone()),
            quota,
            preparation: self.preparation.clone(),
            calendar_preparation: self.calendar_preparation.clone(),
            remote_drafts_wake: Arc::new(Notify::new()),
        };
        let progress = Arc::new(std::sync::Mutex::new(
            json!({"mail":if has_mail {"starting"} else {"not_granted"},"mail_mutations":if has_mail_write {"starting"} else {"not_granted"},"drafts":if has_mail_write {"starting"} else {"not_granted"},"remote_drafts":if has_mail {"starting"} else {"not_granted"},"labels":if has_mail {"starting"} else {"not_granted"},"calendar":if has_calendar {"starting"} else {"not_granted"},"mail_steps":0,"calendar_pages":0}),
        ));
        let task_progress = progress.clone();
        let owners = Arc::new(std::sync::Mutex::new(std::collections::BTreeMap::from([(
            owner_id.to_owned(),
            owner_cancel,
        )])));
        let task_owners = owners.clone();
        let job_context = context.clone();
        let engine_cancel = self.cancel.clone();
        let job_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            let mut owner_tick = tokio::time::interval(Duration::from_millis(50));
            owner_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut workers = tokio::task::JoinSet::new();
            if has_mail {
                workers.spawn(mail_loop(context.clone(), task_progress.clone()));
                workers.spawn(labels_loop(context.clone(), task_progress.clone()));
                workers.spawn(remote_drafts::run(context.clone(), task_progress.clone()));
            }
            if has_mail_write {
                workers.spawn(mail_mutation_loop(context.clone(), task_progress.clone()));
                workers.spawn(draft_loop(context.clone(), task_progress.clone()));
            }
            if has_calendar {
                workers.spawn(calendar_loop(context.clone(), task_progress.clone()));
            }
            loop {
                tokio::select! {
                    _=owner_tick.tick()=> {let mut owners=task_owners.lock().unwrap();owners.retain(|_,cancel|!cancel.is_cancelled());if owners.is_empty(){job_cancel.cancel();break;}},
                    _=engine_cancel.cancelled()=>break,
                    _=job_cancel.cancelled()=>break,
                    next=workers.join_next()=> { if next.is_none() { break; } },
                }
            }
            job_cancel.cancel();
            // Cooperative shutdown waits for admitted preparation to finish;
            // no detached parsing or private Store access survives the owner.
            while workers.join_next().await.is_some() {}
        });
        state.jobs.insert(
            account.clone(),
            Job {
                context: job_context,
                owners,
                cancel,
                task,
                progress,
            },
        );
        Ok(json!({"account":account,"started":true,"active":true}))
    }
    pub(crate) async fn shutdown(&self) {
        let jobs = {
            let mut state = self.state.lock().await;
            state.stopping = true;
            std::mem::take(&mut state.jobs)
        };
        for job in jobs.values() {
            job.cancel.cancel();
        }
        for (_, job) in jobs {
            let _ = job.task.await;
        }
    }
}
fn progress_state(
    progress: &std::sync::Mutex<Value>,
    service: &str,
    state: &str,
    error: Option<&str>,
) {
    let mut value = progress.lock().unwrap();
    value[service] = json!(state);
    value[format!("{service}_error")] = json!(error);
}
async fn pause(cancel: &CancellationToken, delay: Duration) -> bool {
    let mut remaining = delay;
    while !remaining.is_zero() {
        let chunk = remaining.min(Duration::from_secs(60));
        tokio::select! { _=cancel.cancelled()=>return false,_=tokio::time::sleep(chunk)=>{} }
        remaining = remaining.saturating_sub(chunk);
    }
    !cancel.is_cancelled()
}
fn retry_wait(error: &CoreError, attempt: u32) -> Option<Duration> {
    if !(error.code.contains("rate_limited")
        || error.code.contains("transport")
        || error.code.contains("server_unavailable")
        || error.code == "google_unavailable")
    {
        return None;
    }
    let provider = error.details["retry_after_secs"].as_u64().unwrap_or(0);
    // Six rapid read retries are followed by a parked connectivity poll.
    // A temporarily offline owner must recover without losing its mail worker.
    let base = if attempt >= 6 {
        300
    } else {
        (1_u64 << attempt).min(60)
    };
    let jitter = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_millis();
    Some(Duration::from_secs(provider.max(base)) + Duration::from_millis(u64::from(jitter)))
}
async fn mail_loop(ctx: SyncContext, progress: Arc<std::sync::Mutex<Value>>) {
    let mut worker = mail::MailWorker::new(ctx.account.clone());
    let clock = std::time::Instant::now();
    let mut failures = 0;
    while !ctx.cancel.is_cancelled() {
        match worker
            .step(
                &ctx,
                clock.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            )
            .await
        {
            Ok(step) => {
                failures = 0;
                progress_state(&progress, "mail", "running", None);
                let delay = match step {
                    mail::Step::Progress => {
                        let mut value = progress.lock().unwrap();
                        let parts = value["mail_steps"].as_u64().unwrap_or(0);
                        value["mail_steps"] = json!(parts + 1);
                        Duration::from_millis(1)
                    }
                    mail::Step::Idle(delay) => {
                        progress_state(&progress, "mail", "waiting", None);
                        delay
                    }
                };
                if !pause(&ctx.cancel, delay).await {
                    break;
                }
            }
            Err(error) => {
                progress_state(&progress, "mail", "paused", Some(&error.code));
                let Some(delay) = retry_wait(&error, failures) else {
                    return;
                };
                failures = failures.saturating_add(1).min(6);
                if !pause(&ctx.cancel, delay).await {
                    break;
                }
            }
        }
    }
    progress_state(&progress, "mail", "stopped", None);
}
fn operation_params(params: &Value) -> Result<()> {
    if params
        .as_object()
        .is_none_or(|p| p.len() != 1 || !p.contains_key("request_id"))
    {
        return Err(failure("invalid_request", "Choose a Calendar operation"));
    }
    Ok(())
}
fn operation_id(params: &Value) -> Result<&str> {
    params["request_id"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
        .ok_or_else(|| failure("invalid_request", "Choose a Calendar operation"))
}
async fn labels_loop(ctx: SyncContext, progress: Arc<std::sync::Mutex<Value>>) {
    let mut worker = labels::LabelsWorker::default();
    let clock = std::time::Instant::now();
    let mut failures = 0;
    while !ctx.cancel.is_cancelled() {
        let now = clock.elapsed().as_millis().min(u64::MAX as u128) as u64;
        let delay = match worker.step(&ctx, now).await {
            Ok(mail::Step::Progress) => {
                failures = 0;
                progress_state(&progress, "labels", "current", None);
                Duration::from_millis(1)
            }
            Ok(mail::Step::Idle(delay)) => delay,
            Err(error) => {
                progress_state(&progress, "labels", "paused", Some(&error.code));
                let delay = retry_wait(&error, failures).or_else(|| {
                    matches!(
                        error.code.as_str(),
                        "stale_revision" | "invalid_google_response" | "invalid_request"
                    )
                    .then_some(Duration::from_millis(labels::REFRESH_INTERVAL_MS))
                });
                let Some(delay) = delay else {
                    return;
                };
                failures = failures.saturating_add(1).min(6);
                delay
            }
        };
        if !pause(&ctx.cancel, delay).await {
            break;
        }
    }
    progress_state(&progress, "labels", "stopped", None);
}
async fn mail_mutation_loop(ctx: SyncContext, progress: Arc<std::sync::Mutex<Value>>) {
    let mut worker = mail_mutations::MailMutationWorker::new(ctx.account.clone());
    let mut failures = 0;
    while !ctx.cancel.is_cancelled() {
        // Persisted retry deadlines use wall time so a restart does not reset
        // them relative to the preceding process's uptime.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        let delay = match worker.step(&ctx, now).await {
            Ok(step) => {
                failures = 0;
                progress_state(&progress, "mail_mutations", "running", None);
                match step {
                    mail::Step::Progress => Duration::from_millis(1),
                    mail::Step::Idle(delay) => delay,
                }
            }
            Err(error) => {
                progress_state(&progress, "mail_mutations", "paused", Some(&error.code));
                let Some(delay) = retry_wait(&error, failures) else {
                    return;
                };
                failures = failures.saturating_add(1).min(6);
                delay
            }
        };
        if !pause(&ctx.cancel, delay).await {
            break;
        }
    }
    progress_state(&progress, "mail_mutations", "stopped", None);
}
async fn draft_loop(ctx: SyncContext, progress: Arc<std::sync::Mutex<Value>>) {
    let mut failures: u32 = 0;
    while !ctx.cancel.is_cancelled() {
        let delay = match drafts::step(&ctx).await {
            Ok(step) => {
                failures = 0;
                progress_state(&progress, "drafts", "running", None);
                match step {
                    mail::Step::Progress => Duration::from_millis(1),
                    mail::Step::Idle(delay) => delay,
                }
            }
            Err(error) => {
                progress_state(&progress, "drafts", "paused", Some(&error.code));
                if error.code == "cancelled"
                    || (error.code == "stale_google_work"
                        && ctx.core_call(|_| Ok(())).await.is_err())
                {
                    break;
                }
                failures = failures.saturating_add(1).min(6);
                // Core only returns eligible unsent work or acknowledged reads.
                // Unknown/in-flight outcomes never re-enter this upload loop.
                retry_wait(&error, failures).unwrap_or(Duration::from_secs(1))
            }
        };
        if !pause(&ctx.cancel, delay).await {
            break;
        }
    }
    progress_state(&progress, "drafts", "stopped", None);
}
async fn calendar_loop(ctx: SyncContext, progress: Arc<std::sync::Mutex<Value>>) {
    let mut refresh = std::time::Instant::now() - Duration::from_secs(60);
    let mut failures = 0;
    while !ctx.cancel.is_cancelled() {
        let refresh_catalog = refresh.elapsed() >= Duration::from_secs(60);
        match calendar::run_pass(&ctx, refresh_catalog).await {
            Ok(summary) => {
                if refresh_catalog {
                    refresh = std::time::Instant::now();
                }
                failures = 0;
                {
                    let mut value = progress.lock().unwrap();
                    let pages = value["calendar_pages"].as_u64().unwrap_or(0);
                    value["calendar_pages"] = json!(pages + summary.pages);
                    value["calendar_failures"] = json!(summary.failures);
                    value["calendars"] = json!(summary.calendars);
                }
                progress_state(
                    &progress,
                    "calendar",
                    if summary.failures > 0 {
                        "partial"
                    } else if summary.more {
                        "running"
                    } else {
                        "waiting"
                    },
                    summary.first_failure.as_deref(),
                );
                if !pause(
                    &ctx.cancel,
                    if summary.more {
                        Duration::from_millis(1)
                    } else {
                        Duration::from_secs(60)
                    },
                )
                .await
                {
                    break;
                }
            }
            Err(error) => {
                progress_state(&progress, "calendar", "paused", Some(&error.code));
                let Some(delay) = retry_wait(&error, failures) else {
                    return;
                };
                failures = failures.saturating_add(1).min(6);
                if !pause(&ctx.cancel, delay).await {
                    break;
                }
            }
        }
    }
    progress_state(&progress, "calendar", "stopped", None);
}
