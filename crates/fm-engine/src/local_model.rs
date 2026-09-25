//! The local model's one queue and its explicit requests. One inference runs
//! at a time. Order: an email the person asked for, then new mail, then Catch
//! up, newest first (the order comes from Core). No model networking on the
//! writer.
use crate::{
    core_tasks::CoreBridge,
    diagnostics,
    ollama::{self, failure},
};
use fm_assist::ollama::Settings;
use fm_core::{local_model::Prepared, Result};
use fm_google::transport::CancellationToken;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{Notify, Semaphore};
struct Job {
    owner: String,
    account: Option<String>,
    cancel: CancellationToken,
    state: Value,
}
#[derive(Default)]
struct Worker {
    /// Sessions that asked for the queue to run (`assist_watch`). The worker
    /// stops when all of them have closed: closing the GUI ends GUI work.
    owners: BTreeMap<String, CancellationToken>,
    running: bool,
    /// Why the queue is waiting, when the model itself cannot be used.
    unavailable: Option<Value>,
    /// The account and message being analysed now.
    current: Option<(String, String)>,
}
pub struct Runtime {
    core: CoreBridge,
    jobs: Mutex<BTreeMap<String, Job>>,
    /// Explicit requests admitted at once; more are refused as busy.
    slots: Arc<Semaphore>,
    /// The one inference seat. Tokio's semaphore is first-come first-served,
    /// so an explicit request waits at most for the email in progress.
    active: Arc<Semaphore>,
    worker: Mutex<Worker>,
    wake: Notify,
    /// Observed seconds per message for each account, averaged over recent
    /// completed messages; the Accounts view estimates its remaining time from it.
    speeds: Mutex<BTreeMap<String, f64>>,
    /// Accounts whose sync was paused in this engine session. Pausing an
    /// account pauses both of its stages.
    held: Mutex<BTreeSet<String>>,
    /// Settings whose model was last confirmed installed and local. Checked
    /// on first use and after model errors, not before every email.
    qualified: Mutex<Option<String>>,
}
/// Failures of the runner or its authority, not of one message: the queue
/// waits and retries rather than marking every remaining message failed.
fn runner_failure(code: &str) -> bool {
    matches!(
        code,
        "assistance_unavailable"
            | "assistance_disabled"
            | "engine_stopped"
            | "account_removed"
            | "account_not_found"
            | "model_changed"
            | "model_unavailable"
    )
}
/// Failures that say nothing about the email: it stays queued.
fn transient(code: &str) -> bool {
    matches!(code, "assistance_canceled" | "assistance_stale")
}
impl Runtime {
    pub fn new(core: CoreBridge) -> Self {
        Self {
            core,
            jobs: Mutex::new(BTreeMap::new()),
            slots: Arc::new(Semaphore::new(8)),
            active: Arc::new(Semaphore::new(1)),
            worker: Mutex::new(Worker::default()),
            wake: Notify::new(),
            speeds: Mutex::new(BTreeMap::new()),
            held: Mutex::new(Default::default()),
            qualified: Mutex::new(None),
        }
    }
    pub fn hold(&self, account: &str) {
        self.held.lock().unwrap().insert(account.to_owned());
    }
    pub async fn release(&self, account: &str) {
        if self.held.lock().unwrap().remove(account) {
            self.wake.notify_one();
        }
    }
    fn record_speed(&self, account: &str, seconds: f64) {
        let mut speeds = self.speeds.lock().unwrap();
        let average = speeds.entry(account.to_owned()).or_insert(seconds);
        *average = *average * 0.8 + seconds * 0.2;
    }
    /// Confirm the model once per settings; forget it after model errors.
    async fn qualify(&self, settings: &Settings, cancel: &CancellationToken) -> Result<()> {
        let key = serde_json::to_string(&(&settings.provider, &settings.endpoint, &settings.model))
            .unwrap_or_default();
        if self.qualified.lock().unwrap().as_deref() == Some(key.as_str()) {
            return Ok(());
        }
        ollama::qualify(settings, cancel).await?;
        *self.qualified.lock().unwrap() = Some(key);
        Ok(())
    }
    /// Run one prepared email through the model and save it.
    async fn analyse(
        &self,
        plan: Prepared,
        origin: &str,
        job: &str,
        cancel: &CancellationToken,
    ) -> Result<Value> {
        let check = plan.clone();
        self.core.call(move |s| s.local_model_validate(&check)).await?;
        let started = Instant::now();
        let model = plan.settings.model.clone();
        diagnostics::event("Model","started",json!({"job_ref":&job[..8],"account_ref":diagnostics::reference("account",&plan.account),"target_ref":diagnostics::reference("message",&plan.target),"task":"message","origin":origin,
            "input_chars":plan.email["text"].as_str().map_or(0,|t|t.chars().count()),"partial":plan.partial,"label_choices":plan.label_ids().len(),
            "provider":plan.settings.provider.as_str(),"model":model.as_ref().map(|m|m.name.as_str()),"model_digest":model.as_ref().map(|m|m.digest.as_str())}));
        let result = async {
            self.qualify(&plan.settings, cancel).await?;
            let measure = Mutex::new(Value::Null);
            ollama::infer(&plan.settings, &plan.email, &plan.labels, cancel, &measure).await
        }
        .await;
        let output = match result {
            Ok(output) => output,
            Err(e) => {
                if runner_failure(&e.code) || e.code == "model_changed" {
                    *self.qualified.lock().unwrap() = None;
                }
                diagnostics::event("Model","failed",json!({"job_ref":&job[..8],"origin":origin,"code":e.code,"elapsed_ms":started.elapsed().as_millis()}));
                return Err(e);
            }
        };
        self.record_speed(&plan.account, started.elapsed().as_secs_f64());
        let mut facts = analysis_fields(job, &plan.account, &plan.target, origin, &output, plan.partial, started);
        let stop = cancel.clone();
        let saved = self
            .core
            .call(move |s| {
                if stop.is_cancelled() {
                    return Err(failure("assistance_canceled", "Canceled"));
                }
                s.local_model_finish(&plan, output)
            })
            .await?;
        facts["label_state"] = saved["label_state"].clone();
        facts["prompt_sha256"] = saved["prompt_sha256"].clone();
        diagnostics::event("Model", "completed", facts);
        Ok(saved)
    }
    /// Cheap per-account progress for the Accounts view and account tabs.
    async fn progress(&self, account: Option<&str>) -> Result<Value> {
        let account = account.map(str::to_owned);
        let mut progress = self
            .core
            .call(move |s| s.local_model_progress(account.as_deref()))
            .await?;
        let speeds = self.speeds.lock().unwrap().clone();
        let held = self.held.lock().unwrap().clone();
        let worker = self.worker.lock().unwrap();
        for row in progress["accounts"].as_array_mut().unwrap() {
            let id = row["account"].as_str().unwrap_or_default().to_owned();
            row["seconds_per_message"] = json!(speeds.get(&id));
            row["held"] = json!(held.contains(&id));
            row["running"] = json!(worker.running);
            row["model_error"] = worker.unavailable.clone().unwrap_or(Value::Null);
            row["current"] = json!(worker
                .current
                .as_ref()
                .filter(|(a, _)| *a == id)
                .map(|(_, m)| m));
        }
        Ok(progress)
    }
    pub fn handles(method: &str) -> bool {
        matches!(
            method,
            "assist_discover"
                | "assist_start"
                | "assist_job"
                | "assist_cancel"
                | "assist_settings_save"
                | "assist_watch"
                | "assist_jobs"
                | "assist_backlog"
                | "assist_catch_up"
                | "assist_progress"
        )
    }
    pub async fn handle(
        self: &Arc<Self>,
        owner: &str,
        cancel: CancellationToken,
        method: &str,
        account: Option<&str>,
        p: Value,
    ) -> Result<Value> {
        let allowed: &[&str] = match method {
            "assist_discover" => &["endpoint", "provider"],
            "assist_start" => &[
                "task",
                "sample",
                "settings",
                "conversation_id",
                "message_id",
                "sample_case",
            ],
            "assist_job" | "assist_cancel" => &["job_id"],
            "assist_settings_save" => &["expected_revision", "settings"],
            "assist_catch_up" => &["range"],
            _ => &[],
        };
        if !p.is_object()
            || p.as_object()
                .unwrap()
                .keys()
                .any(|k| !allowed.contains(&k.as_str()))
        {
            return Err(failure(
                "invalid_request",
                "Unknown local model request fields",
            ));
        }
        let account_required = || {
            account
                .filter(|a| !a.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| failure("account_required", "Choose an account"))
        };
        match method {
            "assist_progress" => self.progress(account).await,
            "assist_backlog" => {
                let account = account_required()?;
                self.core
                    .call(move |s| s.local_model_catch_up_counts(&account))
                    .await
            }
            "assist_catch_up" => {
                let account = account_required()?;
                let range = p["range"].as_str().unwrap_or_default().to_owned();
                self.core
                    .call(move |s| s.local_model_catch_up(&account, &range))
                    .await?;
                self.wake.notify_one();
                self.progress(account_required().ok().as_deref()).await
            }
            "assist_jobs" => Ok(
                json!({"items":self.jobs.lock().unwrap().values().filter(|j|j.owner==owner).map(|j|j.state.clone()).collect::<Vec<_>>()}),
            ),
            "assist_discover" => {
                let settings = self.core.call(|s| s.local_model_settings()).await?;
                let configured: Settings = serde_json::from_value(settings["settings"].clone())
                    .map_err(|_| failure("invalid_request", "Invalid local model settings"))?;
                let endpoint = p["endpoint"].as_str().unwrap_or(&configured.endpoint);
                let provider = if let Some(value) = p.get("provider") {
                    serde_json::from_value(value.clone())
                        .map_err(|_| failure("invalid_request", "Unknown local model provider"))?
                } else {
                    configured.provider
                };
                if provider == fm_assist::ollama::Provider::Ollama {
                    ollama::discover(endpoint, &cancel).await
                } else {
                    crate::openai_local::discover(endpoint, &cancel).await
                }
            }
            "assist_settings_save" => {
                let result = self
                    .core
                    .call(move |s| s.local_model_handle("assist_settings_save", None, &p))
                    .await?;
                // Work in progress finishes; the queue picks up the new
                // policy with its next email.
                *self.qualified.lock().unwrap() = None;
                self.worker.lock().unwrap().unavailable = None;
                self.wake.notify_one();
                Ok(result)
            }
            "assist_job" | "assist_cancel" => {
                let id = p["job_id"].as_str().unwrap_or_default();
                let mut jobs = self.jobs.lock().unwrap();
                let job = jobs
                    .get_mut(id)
                    .filter(|j| j.owner == owner)
                    .ok_or_else(|| failure("not_found", "Job not found in this session"))?;
                if method == "assist_cancel" {
                    job.cancel.cancel();
                    if matches!(job.state["status"].as_str(), Some("queued" | "running")) {
                        job.state["status"] = "canceled".into();
                    }
                }
                Ok(job.state.clone())
            }
            "assist_watch" => {
                let state = self.core.call(|s| s.local_model_settings()).await?;
                let configured = state["settings"]["model"].is_object();
                let spawn = {
                    let mut worker = self.worker.lock().unwrap();
                    worker.owners.retain(|_, token| !token.is_cancelled());
                    worker.owners.insert(owner.into(), cancel.child_token());
                    !std::mem::replace(&mut worker.running, true)
                };
                if spawn {
                    let runtime = self.clone();
                    tokio::spawn(async move { runtime.run().await });
                } else {
                    self.wake.notify_one();
                }
                Ok(json!({"watching":true,"model_configured":configured}))
            }
            // Only the queue worker marks mail as an arrival for Auto-read.
            "assist_start" => self.start(owner, cancel, account, p).await,
            _ => Err(failure("unsupported_method", "Unknown assistance method")),
        }
    }
    /// Wait for `delay`, a wake-up or all owners closing. None when the
    /// worker should stop; Some(true) after a wake-up.
    async fn wait(&self, delay: Duration) -> Option<bool> {
        let owners: Vec<CancellationToken> = {
            let mut worker = self.worker.lock().unwrap();
            worker.owners.retain(|_, token| !token.is_cancelled());
            if worker.owners.is_empty() {
                worker.running = false;
                worker.current = None;
                return None;
            }
            worker.owners.values().cloned().collect()
        };
        let closed = async {
            for token in owners {
                token.cancelled().await;
            }
        };
        tokio::select! {
            _ = closed => Some(false),
            _ = self.wake.notified() => Some(true),
            _ = tokio::time::sleep(delay) => Some(false),
        }
    }
    /// A token cancelled when every owner of the queue has closed. The caller
    /// cancels it after each email, which also ends its watcher task.
    fn owners_token(&self) -> CancellationToken {
        let token = CancellationToken::new();
        let owners: Vec<CancellationToken> =
            self.worker.lock().unwrap().owners.values().cloned().collect();
        let watch = token.clone();
        tokio::spawn(async move {
            let closed = async {
                for owner in owners {
                    owner.cancelled().await;
                }
            };
            tokio::select! {
                _ = watch.cancelled() => {}
                _ = closed => watch.cancel(),
            }
        });
        token
    }
    async fn run(self: Arc<Self>) {
        // Mail dated from here on is an arrival, eligible for label Auto-read.
        let since = chrono::Utc::now();
        let mut last: Option<String> = None;
        let mut delay = Duration::ZERO;
        // Emails fetched from the queue, and where each account's batch stopped.
        let mut batch: std::collections::VecDeque<Value> = Default::default();
        let mut cursors: Vec<Value> = Vec::new();
        // New mail (a larger newest row) sends the queue back to its top.
        let mut newest: Option<i64> = None;
        // The store revision when the queue was last found empty. Until the
        // store changes or something wakes the worker, there is nothing new.
        let mut empty_at: Option<i64> = None;
        while let Some(woken) = self.wait(delay).await {
            delay = Duration::ZERO;
            let Ok((revision, row)) = self
                .core
                .call(|s| Ok((s.local_model_revision()?, s.local_model_newest_row()?)))
                .await
            else {
                delay = Duration::from_secs(5);
                continue;
            };
            if woken || newest != Some(row) {
                batch.clear();
                cursors.clear();
                newest = Some(row);
                empty_at = None;
            }
            if batch.is_empty() {
                if empty_at == Some(revision) {
                    delay = Duration::from_secs(5);
                    continue;
                }
                let held: Vec<String> = self.held.lock().unwrap().iter().cloned().collect();
                let (after, from) = (last.clone(), cursors.clone());
                match self
                    .core
                    .call(move |s| s.local_model_next(&held, after.as_deref(), 32, &from))
                    .await
                {
                    // Past the end of every batch: look once more from the top,
                    // which also retries emails that failed once.
                    Ok(rows) if rows.is_empty() && !cursors.is_empty() => {
                        cursors.clear();
                        continue;
                    }
                    Ok(rows) if rows.is_empty() => {
                        empty_at = Some(revision);
                        delay = Duration::from_secs(5);
                        continue;
                    }
                    Ok(rows) => {
                        let end = rows.last().unwrap().clone();
                        cursors.retain(|c| !(c["account"] == end["account"] && c["class"] == end["class"]));
                        cursors.push(end);
                        batch.extend(rows);
                    }
                    Err(_) => {
                        delay = Duration::from_secs(5);
                        continue;
                    }
                }
            }
            let next = batch.pop_front().unwrap();
            let account = next["account"].as_str().unwrap_or_default().to_owned();
            let conversation = next["conversation_id"].as_str().unwrap_or_default().to_owned();
            let target = next["id"].as_str().unwrap_or_default().to_owned();
            last = Some(account.clone());
            // A paused sync, or an explicit request that already did this email.
            let (a, t) = (account.clone(), target.clone());
            if self.held.lock().unwrap().contains(&account)
                || !self
                    .core
                    .call(move |s| s.local_model_pending(&a, &t))
                    .await
                    .unwrap_or(false)
            {
                continue;
            }
            let arrival = next["class"] == "new"
                && chrono::DateTime::parse_from_rfc3339(next["date"].as_str().unwrap_or_default())
                    .is_ok_and(|date| {
                        date >= since && date <= chrono::Utc::now() + chrono::Duration::minutes(1)
                    });
            let token = self.owners_token();
            let seat = tokio::select! {biased;_=token.cancelled()=>None,p=self.active.clone().acquire_owned()=>p.ok()};
            let Some(_seat) = seat else {
                continue;
            };
            self.worker.lock().unwrap().current = Some((account.clone(), target.clone()));
            let (a, c, t) = (account.clone(), conversation.clone(), target.clone());
            let result = match self
                .core
                .call(move |s| s.local_model_prepare(&a, &c, &t))
                .await
            {
                Ok(mut plan) => {
                    plan.arrival = arrival;
                    let job = fm_ipc::random_token();
                    self.analyse(plan, if arrival { "arrival" } else { "queue" }, &job, &token)
                        .await
                }
                Err(e) => Err(e),
            };
            token.cancel();
            self.worker.lock().unwrap().current = None;
            match result {
                Ok(_) => self.worker.lock().unwrap().unavailable = None,
                Err(e) if runner_failure(&e.code) => {
                    // The model cannot be used: keep every email queued and retry.
                    self.worker.lock().unwrap().unavailable =
                        Some(json!({"code":e.code,"message":e.message}));
                    batch.clear();
                    cursors.clear();
                    delay = Duration::from_secs(30);
                }
                Err(e) if transient(&e.code) => {}
                Err(e) => {
                    let _ = self
                        .core
                        .call(move |s| s.local_model_fail(&account, &target, &e.code, &e.message))
                        .await;
                }
            }
        }
    }
    pub fn cancel_account(&self, account: Option<&str>) {
        for job in self.jobs.lock().unwrap().values() {
            if job.account.as_deref() == account {
                job.cancel.cancel();
            }
        }
    }
    fn state(&self, id: &str, mut state: Value) {
        if let Some(job) = self.jobs.lock().unwrap().get_mut(id) {
            if job.cancel.is_cancelled()
                && matches!(state["status"].as_str(), Some("queued" | "running"))
            {
                state["status"] = "canceled".into();
            }
            job.state = state;
        }
    }
    /// An explicit request: one email (Summarize, Resummarize, Try again), or
    /// the settings Test with a built-in sample and possibly unsaved settings.
    async fn start(
        self: &Arc<Self>,
        owner: &str,
        cancel: CancellationToken,
        account: Option<&str>,
        p: Value,
    ) -> Result<Value> {
        if p.get("task")
            .is_some_and(|t| !["message", "summary", "classification"].contains(&t.as_str().unwrap_or_default()))
        {
            return Err(failure("invalid_request", "Unknown local model task"));
        }
        let sample = p["sample"].as_bool().unwrap_or(false);
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| failure("assistance_busy", "Local model queue is full"))?;
        enum Work {
            Sample(Box<Settings>, Value, Value),
            Message(Box<Prepared>),
        }
        let work = if sample {
            let state = self.core.call(|s| s.local_model_settings()).await?;
            let settings: Settings = serde_json::from_value(
                p.get("settings")
                    .cloned()
                    .unwrap_or(state["settings"].clone()),
            )
            .map_err(|_| failure("invalid_request", "Invalid sample settings"))?;
            settings
                .validate()
                .map_err(|s| failure("invalid_request", &s))?;
            let (email, mut labels) =
                fm_assist::ollama::sample(p["sample_case"].as_str().unwrap_or("personal"))
                    .map_err(|s| failure("invalid_request", &s))?;
            // With an account, the Test uses its real labels and descriptions.
            if let Some(account) = account.filter(|a| !a.is_empty()) {
                let a = account.to_owned();
                let catalog = self
                    .core
                    .call(move |s| s.local_model_handle("assist_catalog", Some(&a), &json!({})))
                    .await?;
                if let Some(rows) = catalog["categories"].as_array() {
                    let policy = settings.accounts.get(account).cloned().unwrap_or_default();
                    labels = json!(rows.iter().map(|row| {
                        let id = row["id"].as_str().unwrap_or_default();
                        json!({"id":id,"name":row["name"],"description":policy.descriptions.get(id).cloned().unwrap_or_default()})
                    }).collect::<Vec<_>>());
                }
            }
            Work::Sample(Box::new(settings), email, labels)
        } else {
            if p.get("settings").is_some() {
                return Err(failure(
                    "invalid_request",
                    "Unsaved settings are for synthetic samples only",
                ));
            }
            let account = account
                .filter(|a| !a.is_empty())
                .ok_or_else(|| failure("account_required", "Choose an account"))?
                .to_owned();
            let conversation = p["conversation_id"].as_str().unwrap_or_default().to_owned();
            let target = p["message_id"].as_str().unwrap_or_default().to_owned();
            if target.is_empty() {
                return Err(failure(
                    "invalid_request",
                    "Choose one received message to summarize",
                ));
            }
            let mut plan = self
                .core
                .call(move |s| s.local_model_prepare(&account, &conversation, &target))
                .await?;
            plan.overwrite = true;
            Work::Message(Box::new(plan))
        };
        let id = fm_ipc::random_token();
        let token = cancel.child_token();
        {
            let mut jobs = self.jobs.lock().unwrap();
            jobs.retain(|_, j| !j.cancel.is_cancelled());
            if jobs.len() >= 32 {
                if let Some(key) = jobs
                    .iter()
                    .find(|(_, j)| !matches!(j.state["status"].as_str(), Some("running" | "queued")))
                    .map(|(k, _)| k.clone())
                {
                    jobs.remove(&key);
                }
            }
            jobs.insert(id.clone(),Job{owner:owner.into(),account:account.map(str::to_owned),cancel:token.clone(),state:json!({"job_id":id,"status":"queued","completed":0,"total":1})});
        }
        let runtime = self.clone();
        let job_id = id.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let seat = tokio::select! {biased;_=token.cancelled()=>None,p=runtime.active.clone().acquire_owned()=>p.ok()};
            let Some(_seat) = seat else {
                runtime.state(&job_id, json!({"job_id":job_id,"status":"canceled"}));
                return;
            };
            runtime.state(&job_id, json!({"job_id":job_id,"status":"running","completed":0,"total":1}));
            let result: Result<Value> = match work {
                Work::Sample(settings, email, labels) => {
                    let started = Instant::now();
                    let measure = Mutex::new(Value::Null);
                    let result = async {
                        runtime.qualify(&settings, &token).await?;
                        ollama::infer(&settings, &email, &labels, &token, &measure).await
                    }
                    .await;
                    let mut metrics = measure.lock().unwrap().clone();
                    let raw = metrics
                        .as_object_mut()
                        .and_then(|m| m.remove("raw"))
                        .unwrap_or(Value::Null);
                    if !metrics.is_object() {
                        metrics = json!({});
                    }
                    metrics["total_ms"] = json!(started.elapsed().as_millis() as u64);
                    metrics["tokens_per_second"] = json!(metrics["output_tokens"].as_f64().zip(metrics["generation_ms"].as_f64()).filter(|(_,ms)|*ms>0.0).map(|(t,ms)|(t*10000.0/ms).round()/10.0));
                    match result {
                        Ok(output) => Ok(json!({"task":"message","output":output,"sample":true,"metrics":metrics,"raw":raw})),
                        Err(mut e) => {
                            e.details = json!({"metrics":metrics,"raw":raw});
                            Err(e)
                        }
                    }
                }
                Work::Message(plan) => {
                    let (account, target) = (plan.account.clone(), plan.target.clone());
                    let result = runtime.analyse(*plan, "manual", &job_id, &token).await;
                    if let Err(e) = &result {
                        if !runner_failure(&e.code) && !transient(&e.code) {
                            let (code, message) = (e.code.clone(), e.message.clone());
                            let _ = runtime
                                .core
                                .call(move |s| s.local_model_fail(&account, &target, &code, &message))
                                .await;
                        }
                    }
                    result
                }
            };
            let status = if token.is_cancelled() {
                "canceled"
            } else if result.is_ok() {
                "completed"
            } else {
                "failed"
            };
            let (items, error) = match result {
                Ok(item) => (vec![item], None),
                Err(e) => {
                    let mut error = json!({"code":e.code,"message":e.message});
                    // Only the synthetic sample attaches measurements or its raw reply.
                    if sample && e.details.is_object() {
                        error["metrics"] = e.details["metrics"].clone();
                        error["raw"] = e.details["raw"].clone();
                    }
                    (vec![], Some(error))
                }
            };
            runtime.state(&job_id,json!({"job_id":job_id,"status":status,"completed":items.len(),"total":1,"items":items,"error":error}));
        });
        Ok(json!({"job_id":id,"status":"queued"}))
    }
}

fn analysis_fields(job_id: &str, account: &str, target: &str, origin: &str, output: &Value, partial: bool, started: Instant) -> Value {
    json!({"job_ref":&job_id[..8],"account_ref":diagnostics::reference("account",account),"target_ref":diagnostics::reference("message",target),"task":"message","origin":origin,"elapsed_ms":started.elapsed().as_millis(),
        "summary_chars":output["summary"].as_str().map(|s|s.chars().count()),
        "sender_kind":output["sender_kind"],"category_ids":output["category_ids"],"partial":partial})
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    #[test]
    fn records_classification_and_unicode_summary_length_without_mail_text() {
        let output = json!({"summary":"Hi 🌍", "sender_kind":"human", "category_ids":["work"]});
        let facts = analysis_fields("abcdefgh-more", "private-account", "private-message", "arrival", &output, true, Instant::now());
        assert_eq!(facts["summary_chars"], 4);
        assert_eq!(facts["sender_kind"], "human");
        assert_eq!(facts["category_ids"], json!(["work"]));
        assert_eq!(facts["partial"], true);
        assert!(!facts.to_string().contains("Hi 🌍"));
        assert!(!facts.to_string().contains("private-message"));
    }
    #[test]
    fn model_problems_hold_the_queue_and_email_problems_count_against_the_email() {
        for code in ["assistance_unavailable", "model_unavailable", "model_changed"] {
            assert!(runner_failure(code));
        }
        for code in ["invalid_assistance_result", "assistance_timeout", "assistance_output_limit", "assistance_limit"] {
            assert!(!runner_failure(code) && !transient(code), "{code}");
        }
        assert!(transient("assistance_stale"));
    }
}
