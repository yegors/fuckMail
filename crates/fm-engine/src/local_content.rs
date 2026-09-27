//! Local composition content work owns bounded workers outside the Store writer.
use crate::core_tasks::CoreBridge;
use fm_core::{local_content::LocalContentStep, CoreError, Result};
use fm_google::transport::CancellationToken;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Local content work that reads or writes message files: draft saves and
/// captures, attachment imports, message sources, invitations, Sent copies,
/// retained Google draft sources and draft transfers. Each request is a short
/// store "begin", preparation on a blocking worker, and a short "finish".
pub(crate) struct Runtime {
    core: CoreBridge,
    /// Blocking workers for file reading and MIME work (two at a time).
    preparation: Arc<Semaphore>,
    #[cfg(test)]
    before_prepare: Option<PreparationHook>,
}

#[cfg(test)]
type PreparationHook = Arc<dyn Fn(&CancellationToken) -> Result<()> + Send + Sync>;

impl Runtime {
    /// Convert older received attachment copies on bounded workers, then keep
    /// disposable exports within their budget throughout this engine lifetime.
    pub(crate) fn start_storage_maintenance(
        self: &Arc<Self>,
        blobs: fm_core::google_live_mail::BlobStore,
        cancel: CancellationToken,
    ) {
        let runtime = self.clone();
        tokio::spawn(async move {
            while !cancel.is_cancelled() {
                let Ok(Some(plan)) = runtime
                    .core
                    .call(|store| store.attachment_migration_begin())
                    .await
                else {
                    break;
                };
                let Ok(prepared) = runtime
                    .prepare(cancel.clone(), move |stop| {
                        fm_core::attachment_content::prepare_migration(plan, &stop)
                    })
                    .await
                else {
                    break;
                };
                if cancel.is_cancelled() {
                    return;
                }
                if runtime
                    .core
                    .call(move |store| store.attachment_migration_finish(prepared))
                    .await
                    .is_err()
                {
                    break;
                }
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
                }
            }
            while !cancel.is_cancelled() {
                let files = blobs.clone();
                let _ = runtime
                    .prepare(cancel.clone(), move |_| files.prune_exports())
                    .await;
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {}
                }
            }
        });
    }

    pub(crate) fn new(core: CoreBridge) -> Self {
        Self {
            core,
            preparation: Arc::new(Semaphore::new(2)),
            #[cfg(test)]
            before_prepare: None,
        }
    }

    /// Reports whether the session should route `method` to [`Runtime::handle`].
    pub(crate) fn handles(method: &str) -> bool {
        matches!(
            method,
            "draft_attachment_import"
                | "draft_save"
                | "draft_capture"
                | "google_draft_source"
                | "google_draft_authoring"
                | "google_draft_copy"
                | "message_source"
                | "message_invite"
                | "google_sent_message"
                | "draft_transfer"
                | "attachment"
        )
    }

    /// Handles one local content request for `account`. `cancel` stops
    /// preparation, but a finish call that has started always completes, so
    /// its receipt is recorded.
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
        // Move a draft to another account (a From change).
        if method == "draft_transfer" {
            use fm_core::draft_transfer::TransferStep;
            let begin_cancel = cancel.clone();
            let step = self
                .core
                .call(move |store| {
                    if begin_cancel.is_cancelled() {
                        return Err(stopped());
                    }
                    store.draft_transfer_begin(&account, &params)
                })
                .await?;
            let plan = match step {
                TransferStep::Reply(reply) => return Ok(reply),
                TransferStep::Prepare(plan) => plan,
            };
            let (proof, permit) = self
                .prepare_held(cancel.clone(), move |cancel| {
                    fm_core::draft_transfer::prepare(plan, cancel)
                })
                .await?;
            let finish_cancel = cancel.clone();
            let result = self
                .core
                .call(move |store| {
                    if finish_cancel.is_cancelled() {
                        return Err(stopped());
                    }
                    store.draft_transfer_finish(proof)
                })
                .await;
            drop(permit);
            return result;
        }
        // Read-only exports: a sent message's frozen outgoing copy, or a
        // received message's raw source.
        if method == "google_sent_message" {
            let plan = self
                .core
                .call(move |store| store.google_sent_message_begin(&account, &params))
                .await?;
            return self
                .prepare(cancel, move |cancel| {
                    fm_core::google_send::outgoing::read(plan, &cancel)
                })
                .await;
        }
        // Only the lookup uses the store; copying and hashing the file runs
        // on a worker, so previews never wait behind sync's writes.
        if method == "attachment" {
            let plan = self
                .core
                .call(move |store| store.attachment_begin(&account, &params))
                .await?;
            return self.prepare(cancel, move |_| plan.export()).await;
        }
        if method == "message_source" {
            let plan = self
                .core
                .call(move |store| store.message_source_begin(&account, &params))
                .await?;
            return self
                .prepare(cancel, move |cancel| {
                    fm_core::message_source::read(plan, &cancel)
                })
                .await;
        }
        if method == "message_invite" {
            // Read-only: blobs are parsed on a worker, then the short linking
            // query and Calendar projection run on the store.
            let plan = self
                .core
                .call(move |store| store.message_invite_begin(&account, &params))
                .await?;
            let prepared = self
                .prepare(cancel.clone(), move |cancel| {
                    fm_core::message_invite::prepare(plan, &cancel)
                })
                .await?;
            return self
                .core
                .call(move |store| {
                    if cancel.is_cancelled() {
                        return Err(stopped());
                    }
                    store.message_invite_finish(prepared)
                })
                .await;
        }
        // Edit a retained Google draft locally, or copy it into a new local
        // draft. Errors before the commit carry the "finished without commit"
        // marker.
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
        // Export a retained Google draft's source.
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
        // Draft save, capture and attachment import. If the draft changes
        // while its content is prepared, the store asks for one more pass; a
        // second change is reported as `stale_revision`.
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

    /// Runs `operation` on a preparation worker and returns its result.
    async fn prepare<T, F>(&self, cancel: CancellationToken, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(CancellationToken) -> Result<T> + Send + 'static,
    {
        self.prepare_held(cancel, operation)
            .await
            .map(|(value, _permit)| value)
    }

    /// Like [`Runtime::prepare`], but also returns the worker permit, so the
    /// caller can keep the slot until its finish call has used the result.
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
