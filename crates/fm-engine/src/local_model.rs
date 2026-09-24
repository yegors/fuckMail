//! Bounded, session-owned inference jobs. No model networking on the writer.
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
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::sync::Semaphore;
mod backlog;
struct Job {
    owner: String,
    account: Option<String>,
    cancel: CancellationToken,
    state: Value,
}
pub struct Runtime {
    core: CoreBridge,
    jobs: Mutex<BTreeMap<String, Job>>,
    slots: Arc<Semaphore>,
    active: Arc<Semaphore>,
    watches: Mutex<BTreeMap<String, CancellationToken>>,
    /// Observed seconds per message for each account, averaged over recent
    /// completed messages; the Accounts view estimates its remaining time from it.
    speeds: Mutex<BTreeMap<String, f64>>,
    /// Accounts whose sync was paused in this engine session. Pausing an account
    /// pauses both of its stages; resuming queues what the watcher skipped.
    held: Mutex<std::collections::BTreeSet<String>>,
}
/// Failures of the runner or its authority, not of one message: a queue should
/// stop and retry later rather than record every remaining message as failed.
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
impl Runtime {
    pub fn new(core: CoreBridge) -> Self {
        Self {
            core,
            jobs: Mutex::new(BTreeMap::new()),
            slots: Arc::new(Semaphore::new(16)),
            active: Arc::new(Semaphore::new(1)),
            watches: Mutex::new(BTreeMap::new()),
            speeds: Mutex::new(BTreeMap::new()),
            held: Mutex::new(Default::default()),
        }
    }
    pub fn hold(&self, account: &str) {
        self.held.lock().unwrap().insert(account.to_owned());
    }
    pub async fn release(&self, account: &str) {
        if self.held.lock().unwrap().remove(account) {
            let account = account.to_owned();
            let _ = self
                .core
                .call(move |s| s.local_model_requeue_account(&account))
                .await;
        }
    }
    fn record_speed(&self, account: &str, seconds: f64) {
        let mut speeds = self.speeds.lock().unwrap();
        let average = speeds.entry(account.to_owned()).or_insert(seconds);
        *average = *average * 0.8 + seconds * 0.2;
    }
    /// Cheap per-account progress for the Accounts view and account tabs.
    async fn progress(&self, account: Option<&str>) -> Result<Value> {
        let account = account.map(str::to_owned);
        let mut progress = self
            .core
            .call(move |s| s.local_model_progress(account.as_deref()))
            .await?;
        let speeds = self.speeds.lock().unwrap().clone();
        let jobs: Vec<(String, u64, u64)> = self
            .jobs
            .lock()
            .unwrap()
            .values()
            .filter(|j| {
                j.state["kind"] == "backlog"
                    && matches!(j.state["status"].as_str(), Some("queued" | "running"))
                    && !j.cancel.is_cancelled()
            })
            .filter_map(|j| {
                Some((
                    j.account.clone()?,
                    j.state["total"].as_u64().unwrap_or(0),
                    j.state["processed"]
                        .as_u64()
                        .or(j.state["completed"].as_u64())
                        .unwrap_or(0),
                ))
            })
            .collect();
        let held = self.held.lock().unwrap().clone();
        for row in progress["accounts"].as_array_mut().unwrap() {
            let id = row["account"].as_str().unwrap_or_default().to_owned();
            row["seconds_per_message"] = json!(speeds.get(&id));
            row["held"] = json!(held.contains(&id));
            // A running Catch up moves its remaining messages from unprocessed to queued.
            let remaining: u64 = jobs
                .iter()
                .filter(|(a, _, _)| *a == id)
                .map(|(_, total, done)| total.saturating_sub(*done))
                .sum();
            let unprocessed = row["unprocessed"].as_u64().unwrap_or(0);
            let moved = remaining.min(unprocessed);
            row["queued"] = json!(row["queued"].as_u64().unwrap_or(0) + moved);
            row["unprocessed"] = json!(unprocessed - moved);
            row["catching_up"] = json!(remaining > 0);
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
                | "assist_batch_start"
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
            "assist_batch_start" => &["range"],
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
        match method {
            "assist_progress" => self.progress(account).await,
            "assist_backlog" => self.backlog(owner, account, cancel).await,
            "assist_batch_start" => self.start_backlog(owner, cancel, account, &p).await,
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
                for job in self.jobs.lock().unwrap().values() {
                    job.cancel.cancel();
                }
                for watch in self.watches.lock().unwrap().values() {
                    watch.cancel();
                }
                self.watches.lock().unwrap().clear();
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
                    if matches!(
                        job.state["status"].as_str(),
                        Some("queued" | "scanning" | "running")
                    ) {
                        job.state["status"] = "canceled".into();
                    }
                }
                Ok(job.state.clone())
            }
            "assist_watch" => {
                if let Some(old) = self.watches.lock().unwrap().remove(owner) {
                    old.cancel();
                }
                let state = self.core.call(|s| s.local_model_settings()).await?;
                let settings: Settings = serde_json::from_value(state["settings"].clone())
                    .map_err(|_| failure("storage_error", "Invalid model settings"))?;
                if settings.model.is_none() {
                    return Ok(json!({"watching":false}));
                }
                let token = cancel.child_token();
                self.watches
                    .lock()
                    .unwrap()
                    .insert(owner.into(), token.clone());
                let since = chrono::Utc::now().to_rfc3339();
                let start = self.core.call(|s| s.local_model_arrivals(None, "")).await?;
                let mut after = start["after"].as_i64();
                let runtime = self.clone();
                let owner = owner.to_owned();
                tokio::spawn(async move {
                    let mut delay = std::time::Duration::ZERO;
                    let mut unavailable = false;
                    loop {
                        tokio::select! {biased;_=token.cancelled()=>break,_=tokio::time::sleep(delay)=>{}}
                        let since = since.clone();
                        let rows = runtime
                            .core
                            .call(move |s| s.local_model_arrivals(after, &since))
                            .await;
                        let Ok(rows) = rows else { break };
                        // Core returns only messages still missing an enabled result, in
                        // sync order. A full queue does not advance past unprocessed mail.
                        let mut admitted = true;
                        for m in rows["messages"].as_array().unwrap() {
                            // A paused account's mail is queued again when it resumes.
                            if m["account"].as_str().is_some_and(|a| runtime.held.lock().unwrap().contains(a)) {
                                continue;
                            }
                            let request = json!({"task":m["task"],"conversation_id":m["conversation_id"],"message_id":m["id"]});
                            match runtime
                                .start(
                                    &owner,
                                    token.clone(),
                                    m["account"].as_str(),
                                    request,
                                    m["arrival"] == true,
                                )
                                .await
                            {
                                Ok(job) => {
                                    let outcome = loop {
                                        if token.is_cancelled() {
                                            return;
                                        }
                                        let state = runtime
                                            .jobs
                                            .lock()
                                            .unwrap()
                                            .get(job["job_id"].as_str().unwrap())
                                            .map(|j| j.state.clone());
                                        if !state.as_ref().is_some_and(|s| {
                                            s["status"] == "queued" || s["status"] == "running"
                                        }) {
                                            break state;
                                        }
                                        tokio::select! {biased;_=token.cancelled()=>return,_=tokio::time::sleep(std::time::Duration::from_millis(200))=>{}}
                                    };
                                    // An unreachable model fails every message the same way. Keep
                                    // the queue position and retry later instead of skipping mail.
                                    if outcome.is_some_and(|s| {
                                        s["status"] == "failed"
                                            && runner_failure(
                                                s["error"]["code"].as_str().unwrap_or_default(),
                                            )
                                    }) {
                                        admitted = false;
                                        unavailable = true;
                                        break;
                                    }
                                }
                                Err(e) if e.code == "assistance_busy" => {
                                    admitted = false;
                                    break;
                                }
                                // One message that cannot be prepared must not skip the rest.
                                Err(_) => continue,
                            }
                        }
                        if admitted {
                            after = rows["after"].as_i64();
                        }
                        // Work through synced history promptly; poll for new mail otherwise.
                        delay = if std::mem::take(&mut unavailable) {
                            std::time::Duration::from_secs(30)
                        } else if admitted && rows["more"] == true {
                            std::time::Duration::from_millis(50)
                        } else {
                            std::time::Duration::from_secs(5)
                        };
                    }
                });
                Ok(json!({"watching":true}))
            }
            // Client requests are never arrivals, so they cannot trigger Auto-read.
            "assist_start" => self.start(owner, cancel, account, p, false).await,
            _ => Err(failure("unsupported_method", "Unknown assistance method")),
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
                && matches!(
                    state["status"].as_str(),
                    Some("queued" | "scanning" | "running")
                )
            {
                state["status"] = "canceled".into();
            }
            job.state = state;
        }
    }
    async fn start(
        self: &Arc<Self>,
        owner: &str,
        cancel: CancellationToken,
        account: Option<&str>,
        p: Value,
        arrival: bool,
    ) -> Result<Value> {
        let task = p["task"]
            .as_str()
            .filter(|t| ["summary", "classification", "message"].contains(t))
            .ok_or_else(|| failure("invalid_request", "Choose summary or classification"))?
            .to_owned();
        let sample = p["sample"].as_bool().unwrap_or(false);
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| failure("assistance_busy", "Local model queue is full"))?;
        let mut plans: Vec<Prepared> = Vec::new();
        let mut sample_settings = None;
        let mut sample_payload =
            fm_assist::ollama::sample(p["sample_case"].as_str().unwrap_or("personal"))
                .map_err(|s| failure("invalid_request", &s))?;
        if sample {
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
            if let Some(account) = account.filter(|a| !a.is_empty()) {
                let a = account.to_owned();
                let catalog = self
                    .core
                    .call(move |s| s.local_model_handle("assist_catalog", Some(&a), &json!({})))
                    .await?;
                let mut labels = catalog["categories"].clone();
                if let Some(labels) = labels.as_array_mut() {
                    for label in labels {
                        label["description"] = settings
                            .accounts
                            .get(account)
                            .and_then(|a| a.descriptions.get(label["id"].as_str().unwrap()))
                            .cloned()
                            .unwrap_or_default()
                            .into();
                    }
                }
                sample_payload["labels"] = labels;
            }
            sample_settings = Some(settings);
        } else {
            if p.get("settings").is_some() {
                return Err(failure(
                    "invalid_request",
                    "Unsaved settings are for synthetic samples only",
                ));
            }
            let account = account
                .ok_or_else(|| failure("account_required", "Choose an account"))?
                .to_owned();
            let conversation = p["conversation_id"].as_str().unwrap_or_default().to_owned();
            let target = p["message_id"].as_str().unwrap_or_default().to_owned();
            let t = task.clone();
            plans = self
                .core
                .call(move |s| s.local_model_batch(&account, &conversation, &t, &target))
                .await?;
            if arrival {
                for plan in &mut plans {
                    plan.arrival = true;
                }
            }
        }
        let id = fm_ipc::random_token();
        let token = cancel.child_token();
        {
            let mut jobs = self.jobs.lock().unwrap();
            jobs.retain(|_, j| !j.cancel.is_cancelled());
            if jobs.len() >= 32 {
                if let Some(key) = jobs
                    .iter()
                    .find(|(_, j)| {
                        !matches!(
                            j.state["status"].as_str(),
                            Some("running" | "queued" | "scanning")
                        )
                    })
                    .map(|(k, _)| k.clone())
                {
                    jobs.remove(&key);
                }
            }
            jobs.insert(id.clone(),Job{owner:owner.into(),account:account.map(str::to_owned),cancel:token.clone(),state:json!({"job_id":id,"status":"queued","completed":0,"total":if sample{1}else{plans.len()}})});
        }
        let runtime = self.clone();
        let job_id = id.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let active = tokio::select! {biased;_=token.cancelled()=>None,p=runtime.active.clone().acquire_owned()=>p.ok()};
            let Some(_active) = active else {
                runtime.state(&job_id, json!({"job_id":job_id,"status":"canceled"}));
                return;
            };
            let mut outputs = Vec::new();
            let total = if sample { 1 } else { plans.len() };
            runtime.state(
                &job_id,
                json!({"job_id":job_id,"status":"running","completed":0,"total":total}),
            );
            let result:Result<()>=async {
    if let Some(settings)=sample_settings{
     let categories:Vec<String>=sample_payload["labels"].as_array().unwrap().iter().map(|v|v["id"].as_str().unwrap().to_owned()).collect();let sources=vec!["sample-message".to_owned()];
     let payload=sample_payload;
     let started=Instant::now();
     diagnostics::event("Model","started",json!({"job_ref":&job_id[..8],"target_ref":diagnostics::reference("message","sample-message"),"task":task,"origin":"sample","input_chars":payload.to_string().chars().count(),
        "provider":settings.provider.as_str(),"model":settings.model(&task).map(|m|m.name.as_str()),
        "model_digest":settings.model(&task).map(|m|m.digest.as_str())}));
     // The synthetic sample shows its timing and, when invalid, the raw reply.
     let measure=Mutex::new(Value::Null);
     let result=ollama::infer_measured(&settings,&task,payload,&categories,&sources,&token,&measure).await;
     let mut metrics=measure.lock().unwrap().clone();
     let total_ms=started.elapsed().as_millis() as u64;
     let raw=metrics.as_object_mut().and_then(|m|m.remove("raw")).unwrap_or(Value::Null);
     if !metrics.is_object(){metrics=json!({});}
     metrics["total_ms"]=json!(total_ms);
     metrics["tokens_per_second"]=json!(metrics["output_tokens"].as_f64().zip(metrics["generation_ms"].as_f64()).filter(|(_,ms)|*ms>0.0).map(|(t,ms)|(t*10000.0/ms).round()/10.0));
     let output=match result{Ok(output)=>output,Err(mut e)=>{e.details=json!({"metrics":metrics,"raw":raw});return Err(e);}};
     diagnostics::event("Model","completed",analysis_fields(&job_id,"","sample-message",&task,"sample",&output,false,started));
     outputs.push(json!({"task":task,"output":output,"sample":true,"metrics":metrics,"raw":raw}));
    }else{for plan in plans{
     if token.is_cancelled(){return Err(failure("assistance_canceled","Canceled"));}
     let check=plan.clone();runtime.core.call(move|s|s.local_model_validate(&check)).await?;
     let started=Instant::now();
     let origin=if plan.arrival{"arrival"}else{"manual"};
     diagnostics::event("Model","started",json!({"job_ref":&job_id[..8],"account_ref":diagnostics::reference("account",&plan.account),"target_ref":diagnostics::reference("message",&plan.target),"task":task,"origin":origin,"input_chars":plan.payload.to_string().chars().count(),"partial":plan.partial,"source_count":plan.sources.len(),"label_choices":plan.categories.len(),
        "provider":plan.settings.provider.as_str(),"model":plan.settings.model(&task).map(|m|m.name.as_str()),
        "model_digest":plan.settings.model(&task).map(|m|m.digest.as_str())}));
     let output=ollama::infer(&plan.settings,&task,plan.payload.clone(),&plan.categories,&plan.sources,&token).await?;
     runtime.record_speed(&plan.account,started.elapsed().as_secs_f64());
     let mut facts=analysis_fields(&job_id,&plan.account,&plan.target,&task,origin,&output,plan.partial,started);
     let stop=token.clone();let result=runtime.core.call(move|s|{if stop.is_cancelled(){return Err(failure("assistance_canceled","Canceled"));}s.local_model_finish(&plan,output)}).await?;
     facts["label_state"]=if task=="message" {result["items"].as_array().and_then(|items|items.iter().find(|item|item["task"]=="classification")).map(|item|item["label_state"].clone()).unwrap_or(Value::Null)}else{result["label_state"].clone()};
     facts["prompt_sha256"]=if task=="message" {result["items"].as_array().and_then(|items|items.first()).map(|item|item["prompt_sha256"].clone()).unwrap_or(Value::Null)}else{result["prompt_sha256"].clone()};
     diagnostics::event("Model","completed",facts);
     outputs.push(result);runtime.state(&job_id,json!({"job_id":job_id,"status":"running","completed":outputs.len(),"total":total}));
    }}Ok(())
   }.await;
            let status = if token.is_cancelled() {
                "canceled"
            } else if result.is_ok() {
                "completed"
            } else {
                "failed"
            };
            let error = result.err().map(|e| {
                let mut error = json!({"code":e.code,"message":e.message});
                // Only the synthetic sample attaches measurements or its raw reply.
                if sample && e.details.is_object() {
                    error["metrics"] = e.details["metrics"].clone();
                    error["raw"] = e.details["raw"].clone();
                }
                error
            });
            if let Some(error) = &error {
                diagnostics::event("Model","failed",json!({"job_ref":&job_id[..8],"task":task,"code":error["code"],"completed":outputs.len(),"total":total}));
            } else if status == "canceled" {
                diagnostics::event("Model","canceled",json!({"job_ref":&job_id[..8],"task":task,"completed":outputs.len(),"total":total}));
            }
            runtime.state(&job_id,json!({"job_id":job_id,"status":status,"completed":outputs.len(),"total":total,"items":outputs,"error":error}));
        });
        Ok(json!({"job_id":id,"status":"queued"}))
    }
}

fn analysis_fields(job_id: &str, account: &str, target: &str, task: &str, origin: &str, output: &Value, partial: bool, started: Instant) -> Value {
    json!({"job_ref":&job_id[..8],"account_ref":diagnostics::reference("account",account),"target_ref":diagnostics::reference("message",target),"task":task,"origin":origin,"elapsed_ms":started.elapsed().as_millis(),
        "summary_chars":output["summary"].as_str().map(|s|s.chars().count()),
        "sender_kind":output["sender_kind"],"category_ids":output["category_ids"],
        "category_abstain":output["category_abstain"],"partial":partial})
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    #[test]
    fn records_classification_and_unicode_summary_length_without_mail_text() {
        let output = json!({"summary":"Hi 🌍", "sender_kind":"human", "category_ids":["work"],
            "category_abstain":false, "reason":"private explanation", "source_ids":["private-message"]});
        let facts = analysis_fields("abcdefgh-more", "private-account", "private-message", "message", "arrival", &output, true, Instant::now());
        assert_eq!(facts["summary_chars"], 4);
        assert_eq!(facts["sender_kind"], "human");
        assert_eq!(facts["category_ids"], json!(["work"]));
        assert_eq!(facts["partial"], true);
        assert!(!facts.to_string().contains("private explanation"));
        assert!(!facts.to_string().contains("private-message"));
    }
}
