//! Complete labels.list discovery; cached readers never call this worker.
use super::{mail::Step, SyncContext};
use fm_core::{google_labels::prepare_google_labels, CoreError, Result};
use fm_google::{
    gmail_api::{GmailCall, GmailReply},
    GoogleError,
};
use serde_json::{json, Value};
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::OwnedSemaphorePermit;

/// How often the labels worker re-reads the Gmail label list (ten minutes).
pub(crate) const REFRESH_INTERVAL_MS: u64 = 600_000;
/// Schedule for the per-account label-catalog worker.
#[derive(Default)]
pub(crate) struct LabelsWorker {
    /// Job-clock time (ms) of the next refresh; zero refreshes at once.
    next_refresh_ms: u64,
}
impl LabelsWorker {
    /// Refreshes the catalog when due, then tries the one-time link of an
    /// account's existing Gmail labels. Otherwise idles until the next refresh.
    pub(crate) async fn step(&mut self, ctx: &SyncContext, now_ms: u64) -> Result<Step> {
        if now_ms < self.next_refresh_ms {
            return Ok(Step::Idle(Duration::from_millis(
                self.next_refresh_ms - now_ms,
            )));
        }
        refresh(ctx).await?;
        initial_import(ctx).await;
        self.next_refresh_ms = now_ms.saturating_add(REFRESH_INTERVAL_MS);
        Ok(Step::Progress)
    }
}
/// A newly added account links its existing Gmail labels once, right after the
/// first catalog refresh, so model labeling can use them without a manual sync.
/// Nothing is created at Google. A failed attempt retries on the next refresh.
async fn initial_import(ctx: &SyncContext) {
    let account = ctx.account.as_str().to_owned();
    let imported = ctx
        .core_call(move |store| store.google_labels_initial_import(&account))
        .await;
    if imported.is_ok_and(|result| result.is_some()) {
        ctx.refresh
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }
}
/// Reads the complete Gmail label list and publishes it as the account's
/// catalog, staged in chunks. Serves `google_labels_refresh` and the worker.
/// Requires the mail-read grant.
pub(crate) async fn refresh(ctx: &SyncContext) -> Result<Value> {
    let account = ctx.account.as_str().to_owned();
    ctx.core_call(move |store| {
        let allowed = store
            .google_account_bindings()?
            .into_iter()
            .find(|v| v.account == account)
            .is_some_and(|binding| fm_google::auth::ScopeSet::new(&binding.scopes).mail_read());
        if !allowed {
            return Err(failure(
                "capability_unavailable",
                "Google mail reading permission is required",
            ));
        }
        Ok(())
    })
    .await?;
    let call = GmailCall::labels(&ctx.account).map_err(provider)?;
    let (reply, memory) = ctx.gmail_prepared(call).await.map_err(provider)?;
    let GmailReply::Labels(labels) = reply else {
        return Err(invalid());
    };
    let observed_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as u64;
    let (prepared, memory) =
        tokio::task::spawn_blocking(move || (prepare_google_labels(labels), memory))
            .await
            .map_err(|_| failure("engine_stopped", "Google label preparation stopped"))?;
    let prepared = prepared?;
    let memory = Arc::new(memory);
    // Earlier refreshes finish before this shared preparation lease admits the
    // HTTP request. Bind publication to that current catalog, not a revision
    // captured before waiting for quota/memory behind another refresh.
    let account = ctx.account.as_str().to_owned();
    let cached = memory_core(ctx, &memory, move |store| {
        store.google_labels(&account, &json!({"limit":1}))
    })
    .await?;
    let revision = cached["catalog_revision"].as_i64().ok_or_else(invalid)?;
    // `None` means the store cleaned up one part of an abandoned refresh;
    // repeat with the same manifest until it grants a lease.
    let mut lease = loop {
        let account = ctx.account.as_str().to_owned();
        let generation = ctx.connection_generation;
        let manifest = prepared.manifest();
        if let Some(lease) = memory_core(ctx, &memory, move |store| {
            store.google_labels_begin(&account, generation, revision, &manifest)
        })
        .await?
        {
            break lease;
        }
        tokio::task::yield_now().await;
    };
    while let Some(chunk) = prepared.chunk(lease.offset)? {
        lease = memory_core(ctx, &memory, move |store| {
            store.google_labels_stage(&lease, chunk)
        })
        .await?;
        tokio::task::yield_now().await;
    }
    let result = memory_core(ctx, &memory, move |store| {
        store.google_labels_publish(&lease, observed_at)
    })
    .await?;
    loop {
        let account = ctx.account.as_str().to_owned();
        if !memory_core(ctx, &memory, move |store| {
            store.google_labels_cleanup(&account)
        })
        .await?
        {
            break;
        }
        tokio::task::yield_now().await;
    }
    Ok(result)
}

/// Returns `params[key]` when it is the only parameter and a short identifier
/// without control characters.
pub(crate) fn request_parameter<'a>(params: &'a Value, key: &str) -> Result<&'a str> {
    if params
        .as_object()
        .is_none_or(|p| p.len() != 1 || !p.contains_key(key))
    {
        return Err(failure(
            "invalid_request",
            "Supply the label-sync identifier",
        ));
    }
    params[key]
        .as_str()
        .filter(|v| !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control))
        .ok_or_else(|| failure("invalid_request", "Invalid label-sync identifier"))
}
/// A background label-name sync started by `google_labels_sync`. Clients poll
/// it with `google_labels_sync_status`, since it can outlast one request.
pub(crate) struct SyncJob {
    /// Session that started the job; only it can see or cancel the job.
    pub owner: String,
    pub account: String,
    /// The client's request id, so a retried start returns this job.
    pub request: String,
    pub cancel: fm_google::transport::CancellationToken,
    pub task: tokio::task::JoinHandle<()>,
    result: Arc<std::sync::Mutex<Value>>,
}
impl SyncJob {
    /// Spawns the sync. It keeps the account's sync job alive through `owner`
    /// and stops when the starting session ends.
    pub(super) fn start(
        ctx: SyncContext,
        owner: super::OperationOwner,
        owner_id: String,
        owner_cancel: fm_google::transport::CancellationToken,
        request: String,
    ) -> Self {
        let cancel = ctx.cancel.child_token();
        let ctx = SyncContext {
            cancel: cancel.clone(),
            ..ctx
        };
        let account = ctx.account.as_str().to_owned();
        let result = Arc::new(std::sync::Mutex::new(json!({"state":"running"})));
        let task_result = result.clone();
        let task_request = request.clone();
        let task = tokio::spawn(async move {
            let _owner = owner;
            let relay_cancel = ctx.cancel.clone();
            let relay = tokio::spawn(async move {
                tokio::select! { _=owner_cancel.cancelled()=>relay_cancel.cancel(), _=relay_cancel.cancelled()=>{} }
            });
            let outcome = sync_names(&ctx, json!({"request_id":task_request})).await;
            *task_result.lock().unwrap() = match outcome {
                Ok(value) => json!({"state":"complete", "result":value}),
                Err(error) => {
                    json!({"state":if ctx.cancel.is_cancelled(){"cancelled"}else{"failed"},"error":{"code":error.code,"message":error.message,"details":error.details}})
                }
            };
            ctx.cancel.cancel();
            relay.abort();
            let _ = relay.await;
        });
        Self {
            owner: owner_id,
            account,
            request,
            cancel,
            task,
            result,
        }
    }
    /// Returns the job's state (`running`, `complete`, `cancelled` or
    /// `failed`) tagged with `id`. A task that ended without recording a
    /// result reports `failed` with `engine_stopped`.
    pub fn snapshot(&self, id: &str) -> Value {
        let mut value = self.result.lock().unwrap().clone();
        if value["state"] == "running" && self.task.is_finished() {
            value = json!({"state":"failed","error":{"code":"engine_stopped","message":"Label sync stopped. Sync again to check Google."}});
        }
        value["job_id"] = json!(id);
        value
    }
}
/// Merges local label names with Gmail's: refreshes the catalog, imports and
/// renames per the store's merge plan, creates missing labels in Gmail one at
/// a time, then queues label memberships for upload. Stops at the first
/// creation that is not confirmed, so an uncertain create is never repeated
/// blindly.
pub(crate) async fn sync_names(ctx: &SyncContext, params: Value) -> Result<Value> {
    let request = request_parameter(&params, "request_id")?.to_owned();
    refresh(ctx).await?;
    let account = ctx.account.as_str().to_owned();
    let plan = ctx
        .core_call(move |store| store.google_labels_merge(&account, &request))
        .await?;
    let mut created = 0;
    let mut unresolved = 0;
    for item in plan["pending"].as_array().unwrap() {
        let category = item["id"].as_str().unwrap().to_owned();
        let name = item["name"].as_str().unwrap().to_owned();
        let call = GmailCall::create_label(&ctx.account, &name).map_err(provider)?;
        let (a, c, n) = (
            ctx.account.as_str().to_owned(),
            category.clone(),
            name.clone(),
        );
        // Journal the create before sending it. `false` means an earlier
        // attempt is unresolved or already succeeded, or the label changed.
        if !ctx
            .core_call(move |store| store.google_label_create_begin(&a, &c, &n))
            .await?
        {
            unresolved += 1;
            continue;
        }
        let reply = ctx.gmail_mutation(call).await;
        let (label, rejected, error) = match reply {
            Ok(GmailReply::Label(label)) if label.name == name => (Some(label.id), false, None),
            Ok(GmailReply::LabelCreateRejected) => {
                (None, true, Some("google_label_name_rejected".to_owned()))
            }
            Ok(_) => (None, false, Some("invalid_google_response".to_string())),
            Err(error) => {
                let rejected = matches!(
                    error,
                    GoogleError::PermissionDenied
                        | GoogleError::ReauthorizationRequired
                        | GoogleError::RateLimited { .. }
                );
                (None, rejected, Some(provider(error).code))
            }
        };
        let a = ctx.account.as_str().to_owned();
        let accepted = label.is_some();
        let failure_code = error.clone();
        // Persist a received outcome even when this UI owner has just closed.
        ctx.core
            .call(move |store| {
                store.google_label_create_finish(
                    &a,
                    &category,
                    &name,
                    label.as_deref(),
                    rejected,
                    error.as_deref(),
                )
            })
            .await?;
        if accepted {
            created += 1;
        } else {
            return Err(failure(
                failure_code
                    .as_deref()
                    .unwrap_or("google_label_create_unknown"),
                if rejected {
                    "Google did not create the label. Check its name and your mail permissions before syncing again."
                } else {
                    "The label creation outcome is unknown. Sync again to check Google before creating another label."
                },
            ));
        }
    }
    if created > 0 {
        refresh(ctx).await?;
    }
    let mut queued = 0;
    loop {
        let account = ctx.account.as_str().to_owned();
        let count = ctx
            .core_call(move |store| store.google_label_memberships_queue(&account))
            .await?;
        queued += count;
        if count == 0 {
            break;
        }
        tokio::task::yield_now().await;
    }
    ctx.refresh
        .send_modify(|revision| *revision = revision.wrapping_add(1));
    let pulled = plan["pulled"].as_u64().unwrap_or(0);
    let renamed = plan["renamed"].as_u64().unwrap_or(0);
    let skipped = plan["skipped"].as_u64().unwrap_or(0);
    let missing = plan["missing"].as_u64().unwrap_or(0);
    let message = if unresolved > 0 {
        format!("{pulled} imported · {created} created in Google · {unresolved} uncertain creations still need Google readback.")
    } else if skipped > 0 {
        format!("{pulled} imported · {created} created · {skipped} labels could not be merged (name conflict or account limit).")
    } else if missing > 0 {
        format!("{pulled} imported · {created} created · {missing} linked labels are no longer in Google. Remove those labels here before adding replacements.")
    } else if pulled + created + renamed == 0 {
        "Already in sync · just now".into()
    } else {
        format!(
            "{pulled} imported · {created} created in Google · {renamed} names updated · just now"
        )
    };
    Ok(
        json!({"pulled":pulled,"created":created,"renamed":renamed,"skipped":skipped,"missing":missing,"unresolved":unresolved,"queued_memberships":queued,"message":message}),
    )
}
/// [`SyncContext::core_call`] with a share of the preparation permit held by
/// the queued call and its reply.
async fn memory_core<T, F>(
    ctx: &SyncContext,
    memory: &Arc<OwnedSemaphorePermit>,
    operation: F,
) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&mut fm_core::Store) -> Result<T> + Send + 'static,
{
    let held = memory.clone();
    let (result, _held) = ctx
        .core_call(move |store| Ok((operation(store), held)))
        .await?;
    result
}
fn provider(error: GoogleError) -> CoreError {
    let code = match error {
        GoogleError::RateLimited { .. } => "google_rate_limited",
        GoogleError::TransportInterrupted => "google_transport_interrupted",
        GoogleError::ServerUnavailable => "google_server_unavailable",
        GoogleError::ReauthorizationRequired => "google_reauthorization_required",
        GoogleError::PermissionDenied => "google_permission_denied",
        _ => "invalid_google_response",
    };
    let mut result = failure(code, "Google labels could not be refreshed");
    if let GoogleError::RateLimited { retry_after_secs } = error {
        result.details = json!({"retry_after_secs":retry_after_secs});
    }
    result
}
fn invalid() -> CoreError {
    failure(
        "invalid_google_response",
        "Google label catalog response is invalid",
    )
}
fn failure(code: &str, message: &str) -> CoreError {
    CoreError {
        code: code.into(),
        message: message.into(),
        details: json!({}),
    }
}

#[cfg(test)]
#[path = "labels/tests.rs"]
mod tests;
