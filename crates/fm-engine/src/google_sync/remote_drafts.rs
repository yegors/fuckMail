//! One resumable Gmail draft inventory per owned account job, repeated every
//! minute so drafts made, changed or deleted in Gmail follow here.
use super::{drafts, failure, mail::Step, progress_state, retry_wait, SyncContext};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use fm_core::{
    google_draft_inventory::{
        prepare_google_draft_import, GoogleDraftImportOutcome, GoogleDraftInventoryPage,
        GoogleDraftInventoryStep, GoogleDraftListedId, GoogleDraftReadScope,
    },
    CoreError, Result,
};
use fm_google::{
    gmail_api::{GmailCall, GmailReply},
    scheduler::WorkClass,
    GoogleError,
};
use serde_json::{json, Value};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

/// How often Gmail's drafts are listed again while the account syncs.
const RESCAN: Duration = Duration::from_secs(60);

/// The account and connection generation that inventory steps are checked against.
pub(super) fn scope(ctx: &SyncContext) -> GoogleDraftReadScope {
    GoogleDraftReadScope {
        account: ctx.account.as_str().to_owned(),
        connection_generation: ctx.connection_generation,
    }
}

/// Walks Gmail's drafts list and imports new or changed drafts, one
/// store-directed step at a time. Nothing is written to Google.
#[derive(Default)]
pub(crate) struct RemoteDraftWorker {
    /// A scan has been started and not yet completed.
    begun: bool,
    /// Latest inventory status, copied into sync progress by [`run`].
    status: Option<Value>,
}
impl RemoteDraftWorker {
    /// Runs the step the store asks for: begin a scan, list one page of draft
    /// ids, or fetch one draft's RAW source. A completed scan idles for a minute.
    pub(crate) async fn step(&mut self, ctx: &SyncContext) -> Result<Step> {
        if !self.begun {
            let scope = scope(ctx);
            self.status = Some(
                ctx.core_call(move |store| store.google_draft_inventory_begin(&scope))
                    .await?,
            );
            self.begun = true;
            return Ok(Step::Progress);
        }
        let scope = scope(ctx);
        let next = ctx
            .core_call(move |store| store.google_draft_inventory_next(&scope))
            .await?;
        match next {
            GoogleDraftInventoryStep::Complete(status) => {
                self.status = Some(status);
                // The next pass starts a new scan; unchanged drafts cost one list.
                self.begun = false;
                Ok(Step::Idle(RESCAN))
            }
            GoogleDraftInventoryStep::List(plan) => {
                let call =
                    GmailCall::list_drafts(&ctx.account, plan.page_token(), plan.max_results())
                        .and_then(|call| call.with_read_class(WorkClass::BackfillRead))
                        .map_err(provider)?;
                let (reply, permit) = ctx.gmail_prepared(call).await.map_err(provider)?;
                let memory = Arc::new(permit);
                match reply {
                    GmailReply::Drafts(page) => {
                        // JSON decoding and a provider response cannot advance a
                        // cursor. The bounded durable page must be accepted first.
                        let page = drafts::prepare(ctx, &memory, move |_| {
                            if page.drafts.len()
                                > fm_core::google_draft_inventory::MAX_INVENTORY_PAGE
                            {
                                return Err(failure(
                                    "invalid_google_response",
                                    "Google draft page exceeded its item bound",
                                ));
                            }
                            Ok(GoogleDraftInventoryPage {
                                drafts: page
                                    .drafts
                                    .into_iter()
                                    .map(|draft| GoogleDraftListedId {
                                        draft_id: draft.id,
                                        message_id: draft.message.id,
                                    })
                                    .collect(),
                                next_page_token: page.next_page_token,
                                result_size_estimate: page.result_size_estimate,
                            })
                        })
                        .await?;
                        self.status = Some(
                            drafts::memory_core(ctx, &memory, move |store| {
                                store.google_draft_inventory_list_finish(plan, page)
                            })
                            .await?,
                        );
                    }
                    GmailReply::DraftPageExpired => {
                        self.status = Some(
                            drafts::memory_core(ctx, &memory, move |store| {
                                store.google_draft_inventory_list_expired(plan)
                            })
                            .await?,
                        );
                    }
                    _ => {
                        return Err(failure(
                            "invalid_google_response",
                            "Expected a Google draft inventory page",
                        ))
                    }
                }
                Ok(Step::Progress)
            }
            GoogleDraftInventoryStep::Fetch(plan) => {
                let call = GmailCall::get_draft(&ctx.account, plan.provider_draft_id())
                    .and_then(|call| call.with_read_class(WorkClass::BackfillRead))
                    .map_err(provider)?;
                let (remote, permit) = match ctx.gmail_prepared(call).await {
                    Ok((GmailReply::Draft(draft), permit)) => (Ok(Some(draft)), permit),
                    Ok(_) => {
                        return Err(failure(
                            "invalid_google_response",
                            "Expected the requested Google draft RAW",
                        ))
                    }
                    Err(GoogleError::NotFound) => (Ok(None), ctx.prepare_permit().await?),
                    Err(GoogleError::InvalidResponse(reason)) => {
                        (Err(reason), ctx.prepare_permit().await?)
                    }
                    Err(error) => return Err(provider(error)),
                };
                let memory = Arc::new(permit);
                let proof = drafts::prepare(ctx, &memory, move |cancel| {
                    let outcome = match remote {
                        Ok(Some(draft)) => match URL_SAFE_NO_PAD.decode(
                            draft
                                .message
                                .raw
                                .as_deref()
                                .unwrap_or_default()
                                .trim_end_matches('='),
                        ) {
                            Ok(raw) => GoogleDraftImportOutcome::Available {
                                draft_id: draft.id,
                                message_id: draft.message.id,
                                thread_id: draft.message.thread_id,
                                raw,
                            },
                            Err(_) => GoogleDraftImportOutcome::Unavailable {
                                code: "invalid_google_response".into(),
                            },
                        },
                        Ok(None) => GoogleDraftImportOutcome::Missing,
                        Err(reason) => GoogleDraftImportOutcome::Unavailable {
                            code: if reason == "provider response exceeds byte limit" {
                                "source_too_large"
                            } else {
                                "invalid_google_response"
                            }
                            .into(),
                        },
                    };
                    // Stable draft identity is checked by the adapter and Core;
                    // the GET's newer contained-message version is valid input.
                    prepare_google_draft_import(plan, outcome, &cancel)
                })
                .await?;
                self.status = Some(
                    drafts::memory_core(ctx, &memory, move |store| {
                        store.google_draft_import_finish(proof)
                    })
                    .await?,
                );
                Ok(Step::Progress)
            }
        }
    }
}

/// The remote-drafts worker loop. Besides its own schedule it wakes on
/// `google_drafts_refresh` and on the job's refresh signal.
pub(super) async fn run(ctx: SyncContext, progress: Arc<Mutex<Value>>) {
    let mut worker = RemoteDraftWorker::default();
    let mut failures = 0;
    let mut refresh = ctx.refresh.subscribe();
    while !ctx.cancel.is_cancelled() {
        if !super::service_ready(&ctx, &progress, "mail", "remote_drafts", &mut refresh).await {
            continue;
        }
        if ctx
            .credential_blocked
            .load(std::sync::atomic::Ordering::Acquire)
        {
            progress_state(
                &progress,
                "remote_drafts",
                "paused",
                Some("vault_unavailable"),
            );
            if !super::wait_sync(&ctx, &mut refresh, None).await {
                break;
            }
            continue;
        }
        let mut wait_for_refresh = false;
        let delay = match worker.step(&ctx).await {
            Ok(step) => {
                failures = 0;
                if let Some(status) = worker.status.take() {
                    progress.lock().unwrap()["remote_drafts_inventory"] = status;
                }
                match step {
                    Step::Progress => {
                        progress_state(&progress, "remote_drafts", "running", None);
                        Duration::from_millis(1)
                    }
                    Step::Idle(delay) => {
                        progress_state(&progress, "remote_drafts", "waiting", None);
                        delay
                    }
                }
            }
            Err(error) => {
                progress_state(&progress, "remote_drafts", "paused", Some(&error.code));
                if error.code == "cancelled"
                    || (error.code == "stale_google_work"
                        && ctx.core_call(|_| Ok(())).await.is_err())
                {
                    break;
                }
                // Transient errors back off. Other errors park the worker
                // until an explicit wake or refresh, with no timer; a stale
                // check that the probe above did not confirm retries at once.
                let delay = retry_wait(&error, failures).unwrap_or_else(|| {
                    if error.code == "stale_google_work" {
                        Duration::from_millis(1)
                    } else {
                        wait_for_refresh = true;
                        Duration::from_secs(300)
                    }
                });
                failures = failures.saturating_add(1).min(6);
                delay
            }
        };
        tokio::select! {
            _=ctx.cancel.cancelled()=>break,
            _=ctx.remote_drafts_wake.notified()=>{},
            _=refresh.changed()=>{},
            _=super::pause(&ctx.cancel, delay), if !wait_for_refresh=>{},
        }
    }
    progress_state(&progress, "remote_drafts", "stopped", None);
}

/// Maps a Gmail error to an engine error code, keeping any retry-after hint.
fn provider(error: GoogleError) -> CoreError {
    let code = match error {
        GoogleError::RateLimited { .. } => "google_rate_limited",
        GoogleError::TransportInterrupted => "google_transport_interrupted",
        GoogleError::ServerUnavailable => "google_server_unavailable",
        GoogleError::ReauthorizationRequired => "google_reauthorization_required",
        GoogleError::PermissionDenied => "google_permission_denied",
        _ => "invalid_google_response",
    };
    let mut error_value = failure(code, "Google draft inventory could not continue");
    if let GoogleError::RateLimited { retry_after_secs } = error {
        error_value.details = json!({"retry_after_secs":retry_after_secs});
    }
    error_value
}

#[cfg(test)]
mod tests;
