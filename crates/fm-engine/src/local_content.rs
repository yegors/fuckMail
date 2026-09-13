//! Local composition content work owns bounded workers outside the Store writer.
use crate::core_tasks::CoreBridge;
use fm_core::{local_content::LocalContentStep, CoreError, Result};
use fm_google::transport::CancellationToken;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Semaphore;

pub(crate) struct Runtime {
    core: CoreBridge,
    preparation: Arc<Semaphore>,
    #[cfg(test)]
    before_prepare: Option<PreparationHook>,
}

#[cfg(test)]
type PreparationHook = Arc<dyn Fn(&CancellationToken) -> Result<()> + Send + Sync>;

impl Runtime {
    pub(crate) fn new(core: CoreBridge) -> Self {
        Self {
            core,
            preparation: Arc::new(Semaphore::new(2)),
            #[cfg(test)]
            before_prepare: None,
        }
    }

    pub(crate) fn handles(method: &str) -> bool {
        matches!(
            method,
            "draft_attachment_import"
                | "draft_save"
                | "draft_capture"
                | "google_draft_source"
                | "google_draft_authoring"
                | "google_draft_copy"
        )
    }

    pub(crate) async fn handle(
        &self,
        cancel: CancellationToken,
        method: &str,
        account: Option<&str>,
        params: Value,
    ) -> Result<Value> {
        if !Self::handles(method) {
            return Err(failure(
                "unsupported_method",
                "Unknown local content operation",
            ));
        }
        let account = account
            .ok_or_else(|| {
                let error = failure("account_required", "Choose an account");
                if method == "google_draft_copy" {
                    authoring_uncommitted(error)
                } else {
                    error
                }
            })?
            .to_owned();
        let method = method.to_owned();
        if matches!(
            method.as_str(),
            "google_draft_authoring" | "google_draft_copy"
        ) {
            use fm_core::google_draft_actions::GoogleDraftAuthoringStep;
            let begin_cancel = cancel.clone();
            let step = self
                .core
                .call(move |store| {
                    if begin_cancel.is_cancelled() {
                        return Err(stopped());
                    }
                    if method == "google_draft_authoring" {
                        store.google_draft_authoring_begin(&account, &params)
                    } else {
                        store.google_draft_copy_begin(&account, &params)
                    }
                })
                .await
                .map_err(authoring_uncommitted)?;
            let plan = match step {
                GoogleDraftAuthoringStep::Reply(reply) => return Ok(reply),
                GoogleDraftAuthoringStep::Prepare(plan) => plan,
            };
            let (proof, permit) = self
                .prepare_held(cancel.clone(), move |cancel| {
                    fm_core::google_draft_actions::prepare_google_draft_authoring(
                        plan, None, &cancel,
                    )
                })
                .await
                .map_err(authoring_uncommitted)?;
            return self
                .core
                .call(move |store| {
                    let _permit = permit;
                    if cancel.is_cancelled() {
                        return Err(authoring_uncommitted(stopped()));
                    }
                    store.google_draft_authoring_finish(proof)
                })
                .await;
        }
        if method == "google_draft_source" {
            let begin_cancel = cancel.clone();
            let plan = self
                .core
                .call(move |store| {
                    if begin_cancel.is_cancelled() {
                        return Err(stopped());
                    }
                    store.google_draft_source_begin(&account, &params)
                })
                .await?;
            return self
                .prepare(cancel, move |cancel| {
                    fm_core::google_draft_inventory::export_google_draft_source(plan, &cancel)
                })
                .await;
        }
        let begin_cancel = cancel.clone();
        // Begin resolves matching durable replies before any source-file access.
        // It snapshots only bounded metadata/authored text and never reads blobs.
        let mut step = tokio::select! {
            biased;
            _=cancel.cancelled()=>return Err(stopped()),
            result=self.core.call(move |store| {
                if begin_cancel.is_cancelled() {return Err(stopped());}
                store.local_content_begin(&method, &account, params)
            })=>result?,
        };
        for _ in 0..2 {
            let plan = match step {
                LocalContentStep::Reply(reply) => return Ok(reply),
                LocalContentStep::Prepare(plan) => plan,
            };
            let prepared = self
                .prepare(cancel.clone(), move |cancel| {
                    fm_core::local_content::prepare_local_content(plan, &cancel)
                })
                .await?;
            if cancel.is_cancelled() {
                return Err(stopped());
            }
            let finish_cancel = cancel.clone();
            // Once this short commit starts it must finish its receipt. Session
            // shutdown awaits handle(), so a disconnected caller cannot drop it.
            step = self
                .core
                .call(move |store| {
                    if finish_cancel.is_cancelled() {
                        return Err(stopped());
                    }
                    store.local_content_finish(prepared)
                })
                .await?;
        }
        match step {
            LocalContentStep::Reply(reply) => Ok(reply),
            LocalContentStep::Prepare(_) => Err(failure(
                "stale_revision",
                "Content changed again during preparation; reload its saved state",
            )),
        }
    }

    async fn prepare<T, F>(&self, cancel: CancellationToken, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(CancellationToken) -> Result<T> + Send + 'static,
    {
        self.prepare_held(cancel, operation)
            .await
            .map(|(value, _permit)| value)
    }

    async fn prepare_held<T, F>(
        &self,
        cancel: CancellationToken,
        operation: F,
    ) -> Result<(T, tokio::sync::OwnedSemaphorePermit)>
    where
        T: Send + 'static,
        F: FnOnce(CancellationToken) -> Result<T> + Send + 'static,
    {
        let permit = tokio::select! {
            biased;
            _=cancel.cancelled()=>return Err(stopped()),
            result=self.preparation.clone().acquire_owned()=>result.map_err(|_|stopped())?,
        };
        // The blocking worker owns the permit. Cancellation or a dropped async
        // waiter cannot admit another large allocation while this worker lives.
        #[cfg(test)]
        let before_prepare = self.before_prepare.clone();
        tokio::task::spawn_blocking(move || {
            if cancel.is_cancelled() {
                return Err(stopped());
            }
            #[cfg(test)]
            if let Some(hook) = before_prepare {
                hook(&cancel)?;
            }
            operation(cancel).map(|value| (value, permit))
        })
        .await
        .map_err(|_| failure("content_unavailable", "Local content preparation failed"))?
    }
}

fn stopped() -> CoreError {
    failure("cancelled", "Local content work stopped")
}

/// Only this completed invocation is proven to have made no authored commit.
/// An earlier lost invocation with the same request remains uncertain.
pub(crate) fn authoring_uncommitted(mut error: CoreError) -> CoreError {
    if !error.details.is_object() {
        error.details = json!({});
    }
    error.details["draft_authoring_invocation_finished_without_commit"] = Value::Bool(true);
    error
}

fn failure(code: &str, message: &str) -> CoreError {
    CoreError {
        code: code.into(),
        message: message.into(),
        details: json!({}),
    }
}

#[cfg(test)]
mod tests;
