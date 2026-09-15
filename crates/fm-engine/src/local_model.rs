//! Bounded, session-owned inference jobs. No model networking on the writer.
use crate::{
    core_tasks::CoreBridge,
    ollama::{self, failure},
};
use fm_assist::ollama::Settings;
use fm_core::{local_model::Prepared, Result};
use fm_google::transport::CancellationToken;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::sync::Semaphore;
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
}
impl Runtime {
    pub fn new(core: CoreBridge) -> Self {
        Self {
            core,
            jobs: Mutex::new(BTreeMap::new()),
            slots: Arc::new(Semaphore::new(16)),
            active: Arc::new(Semaphore::new(1)),
            watches: Mutex::new(BTreeMap::new()),
        }
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
                    job.state["status"] = "canceled".into();
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
                    loop {
                        tokio::select! {biased;_=token.cancelled()=>break,_=tokio::time::sleep(std::time::Duration::from_secs(5))=>{}}
                        let since = since.clone();
                        let rows = runtime
                            .core
                            .call(move |s| s.local_model_arrivals(after, &since))
                            .await;
                        let Ok(rows) = rows else { break };
                        // A full queue does not advance the cursor over unprocessed incoming mail.
                        let mut admitted = true;
                        for m in rows["messages"].as_array().unwrap() {
                            let a = m["account"].as_str().unwrap().to_owned();
                            let c = m["conversation_id"].as_str().unwrap().to_owned();
                            let target = m["id"].clone();
                            let current = runtime
                                .core
                                .call(move |s| s.local_model_results(&a, &c))
                                .await;
                            if current.is_ok_and(|r| {
                                r["items"].as_array().unwrap().iter().any(|i| {
                                    (i["task"] == "classification" || i["task"] == "summary")
                                        && i["target"] == target
                                        && i["stale"] == false
                                })
                            }) {
                                continue;
                            }
                            match runtime.start(&owner,token.clone(),m["account"].as_str(),json!({"task":"message","conversation_id":m["conversation_id"],"message_id":m["id"]})).await {
                                Ok(job)=>loop {
                                    if token.is_cancelled(){return;}
                                    let running=runtime.jobs.lock().unwrap().get(job["job_id"].as_str().unwrap()).is_some_and(|j|j.state["status"]=="queued" || j.state["status"]=="running");
                                    if !running{break;}
                                    tokio::select!{biased;_=token.cancelled()=>return,_=tokio::time::sleep(std::time::Duration::from_millis(200))=>{}}
                                },Err(e)=>{admitted=e.code!="assistance_busy";break;}
                            }
                        }
                        if admitted {
                            after = rows["after"].as_i64();
                        }
                    }
                });
                Ok(json!({"watching":true}))
            }
            "assist_start" => self.start(owner, cancel, account, p).await,
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
    fn state(&self, id: &str, state: Value) {
        if let Some(job) = self.jobs.lock().unwrap().get_mut(id) {
            job.state = state;
        }
    }
    async fn start(
        self: &Arc<Self>,
        owner: &str,
        cancel: CancellationToken,
        account: Option<&str>,
        p: Value,
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
        }
        let id = fm_ipc::random_token();
        let token = cancel.child_token();
        {
            let mut jobs = self.jobs.lock().unwrap();
            jobs.retain(|_, j| !j.cancel.is_cancelled());
            if jobs.len() >= 32 {
                if let Some(key) = jobs
                    .iter()
                    .find(|(_, j)| j.state["status"] != "running" && j.state["status"] != "queued")
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
     let output=ollama::infer(&settings,&task,payload,&categories,&sources,&token).await?;outputs.push(json!({"task":task,"output":output,"sample":true}));
    }else{for plan in plans{
     if token.is_cancelled(){return Err(failure("assistance_canceled","Canceled"));}
     let check=plan.clone();runtime.core.call(move|s|s.local_model_validate(&check)).await?;
     let output=ollama::infer(&plan.settings,&task,plan.payload.clone(),&plan.categories,&plan.sources,&token).await?;
     let stop=token.clone();let result=runtime.core.call(move|s|{if stop.is_cancelled(){return Err(failure("assistance_canceled","Canceled"));}s.local_model_finish(&plan,output)}).await?;
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
            let error = result
                .err()
                .map(|e| json!({"code":e.code,"message":e.message}));
            runtime.state(&job_id,json!({"job_id":job_id,"status":status,"completed":outputs.len(),"total":total,"items":outputs,"error":error}));
        });
        Ok(json!({"job_id":id,"status":"queued"}))
    }
}
