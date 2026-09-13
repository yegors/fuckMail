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

pub(crate) const REFRESH_INTERVAL_MS: u64 = 600_000;
#[derive(Default)]
pub(crate) struct LabelsWorker {
    next_refresh_ms: u64,
}
impl LabelsWorker {
    pub(crate) async fn step(&mut self, ctx: &SyncContext, now_ms: u64) -> Result<Step> {
        if now_ms < self.next_refresh_ms {
            return Ok(Step::Idle(Duration::from_millis(
                self.next_refresh_ms - now_ms,
            )));
        }
        refresh(ctx).await?;
        self.next_refresh_ms = now_ms.saturating_add(REFRESH_INTERVAL_MS);
        Ok(Step::Progress)
    }
}
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
