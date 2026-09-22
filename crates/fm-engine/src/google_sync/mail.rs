//! Gmail read acquisition. Every step publishes at most one bounded Store part.
//! HTTP, MIME parsing and recovery hashing never execute on the Store writer.
use super::SyncContext;
use fm_core::{
    google_live_mail::{
        prepare_google_item_since, verify_staged_google_item, GoogleItemAction, GooglePageIdentity,
        MailLane, MailLaneState, PreparedGoogleItem, MAX_PART_BYTES, MAX_PART_ITEMS,
    },
    CoreError, Result,
};
use fm_google::{
    gmail_api::{GmailCall, GmailReply, RawOutcome},
    scheduler::WorkClass,
    sync::{prepare_full_page, prepare_history_page, Checkpoint, PagePlan, Phase},
    AccountId, GoogleError,
};
use serde_json::{json, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const RECENT_SECONDS: u64 = 30 * 24 * 60 * 60;
const PARTS_BEFORE_DELTA: u8 = 4;
// IDs are cheap, but a smaller page makes the initial useful result independent
// of an account's total history. History records can still fan out to 5,000 IDs.
const LIST_PAGE_SIZE: u16 = 100;
const HISTORY_PAGE_SIZE: u16 = 100;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Progress,
    Idle(Duration),
}

pub(crate) struct MailWorker {
    account: AccountId,
    non_delta_parts: u8,
    next_delta_ms: u64,
}

impl MailWorker {
    pub(crate) fn new(account: AccountId) -> Self {
        Self {
            account,
            non_delta_parts: 0,
            next_delta_ms: 0,
        }
    }

    pub(crate) fn request_refresh(&mut self) {
        self.next_delta_ms = 0;
        self.non_delta_parts = PARTS_BEFORE_DELTA;
    }

    pub(crate) async fn step(&mut self, ctx: &SyncContext, now_ms: u64) -> Result<Step> {
        if ctx.account != self.account {
            return Err(failure("stale_google_work", "Google account work changed"));
        }
        check_cancel(ctx)?;
        let account = self.account.as_str().to_owned();
        let lanes = ctx
            .core_call(move |store| store.google_live_lanes(&account))
            .await?;
        if lanes.is_empty() {
            let baseline = self.baseline(ctx).await?;
            let account = self.account.as_str().to_owned();
            let cutoff = recent_cutoff();
            ctx.core_call(move |store| {
                store.google_live_begin(&account, &baseline, cutoff)?;
                Ok(())
            })
            .await?;
            return Ok(Step::Progress);
        }
        let recent = lane(&lanes, MailLane::Recent)?;
        let inventory = lane(&lanes, MailLane::Inventory)?;
        let delta = lane(&lanes, MailLane::Delta)?;
        let delta_due = !delta.complete
            || delta.checkpoint.phase != Phase::Idle
            || now_ms >= self.next_delta_ms;
        let selected = if !recent.complete {
            if delta_due && self.non_delta_parts >= PARTS_BEFORE_DELTA {
                delta
            } else {
                recent
            }
        } else if delta_due && (inventory.complete || self.non_delta_parts >= PARTS_BEFORE_DELTA) {
            delta
        } else {
            inventory
        };

        // Saved IDs and the exact continuation always win over another HTTP
        // list/history response, including after cancellation or reconnect.
        let account = self.account.as_str().to_owned();
        let selected_lane = selected.lane;
        let saved = ctx
            .core_call(move |store| store.google_live_resume_page(&account, selected_lane))
            .await?;
        let page = if let Some(page) = saved {
            page
        } else if selected.lane == MailLane::Inventory && selected.complete {
            let account = self.account.as_str().to_owned();
            let plan = ctx
                .core_call(move |store| store.google_live_reconciliation_page(&account))
                .await?;
            let Some(plan) = plan else {
                return Ok(Step::Idle(Duration::from_millis(
                    self.next_delta_ms.saturating_sub(now_ms).max(1),
                )));
            };
            let history = full_baseline(&plan.expected)?.to_owned();
            self.begin_page(ctx, selected.lane, plan, history).await?
        } else {
            match self.fetch_page(ctx, selected).await? {
                PageResult::Page(page) => page,
                PageResult::HistoryExpired => {
                    // Capture a fresh baseline before replacement enumeration;
                    // source bytes, human intent and authored work stay retained.
                    let baseline = self.baseline(ctx).await?;
                    let account = self.account.as_str().to_owned();
                    let generation = selected.checkpoint.generation;
                    let cutoff = recent_cutoff();
                    ctx.core_call(move |store| {
                        store.google_live_resync(&account, generation, &baseline, cutoff)?;
                        Ok(())
                    })
                    .await?;
                    self.non_delta_parts = 0;
                    self.next_delta_ms = 0;
                    return Ok(Step::Progress);
                }
            }
        };

        let completed = self.publish_part(ctx, &page).await?;
        if page.lane == MailLane::Delta {
            self.non_delta_parts = 0;
            if completed.is_some_and(|checkpoint| checkpoint.phase == Phase::Idle) {
                let interval = ctx
                    .core_call(|store| store.google_sync_interval_ms())
                    .await?;
                self.next_delta_ms = now_ms.saturating_add(interval);
            }
        } else {
            self.non_delta_parts = self.non_delta_parts.saturating_add(1);
        }
        Ok(Step::Progress)
    }

    async fn baseline(&self, ctx: &SyncContext) -> Result<String> {
        let call = GmailCall::profile(&self.account).map_err(provider_error)?;
        match ctx.gmail(call).await.map_err(provider_error)? {
            GmailReply::Profile(profile) => Ok(profile.history_id),
            _ => Err(unexpected()),
        }
    }

    async fn begin_page(
        &self,
        ctx: &SyncContext,
        lane: MailLane,
        plan: PagePlan,
        observed_history_id: String,
    ) -> Result<GooglePageIdentity> {
        ctx.core_call(move |store| store.google_live_begin_page(lane, &plan, &observed_history_id))
            .await
    }

    async fn fetch_page(&self, ctx: &SyncContext, state: &MailLaneState) -> Result<PageResult> {
        let checkpoint = &state.checkpoint;
        let (plan, history) = match &checkpoint.phase {
            Phase::Full {
                baseline_history_id,
                page_token,
            } => {
                let call = GmailCall::list_messages(
                    &self.account,
                    page_token.as_deref(),
                    state.query.as_deref(),
                    true,
                    LIST_PAGE_SIZE,
                )
                .and_then(|call| call.with_read_class(read_class(state.lane)))
                .map_err(provider_error)?;
                let GmailReply::Messages(page) = ctx.gmail(call).await.map_err(provider_error)?
                else {
                    return Err(unexpected());
                };
                (
                    prepare_full_page(checkpoint, page).map_err(provider_error)?,
                    baseline_history_id.clone(),
                )
            }
            Phase::CatchUp {
                start_history_id,
                page_token,
            }
            | Phase::Delta {
                start_history_id,
                page_token,
            } if state.lane == MailLane::Delta => {
                let call = GmailCall::history(
                    &self.account,
                    start_history_id,
                    page_token.as_deref(),
                    HISTORY_PAGE_SIZE,
                )
                .map_err(provider_error)?;
                let response = match ctx.gmail(call).await {
                    Err(GoogleError::HistoryExpired) => return Ok(PageResult::HistoryExpired),
                    other => other.map_err(provider_error)?,
                };
                let GmailReply::History(page) = response else {
                    return Err(unexpected());
                };
                let history = page.history_id.clone();
                (
                    prepare_history_page(checkpoint, page).map_err(provider_error)?,
                    history,
                )
            }
            Phase::Idle if state.lane == MailLane::Delta => (
                checkpoint.begin_delta().map_err(provider_error)?,
                checkpoint
                    .committed_history_id
                    .clone()
                    .ok_or_else(unexpected)?,
            ),
            Phase::NeedsFullSync if state.lane == MailLane::Delta => {
                return Ok(PageResult::HistoryExpired)
            }
            _ => return Err(unexpected()),
        };
        self.begin_page(ctx, state.lane, plan, history)
            .await
            .map(PageResult::Page)
    }

    async fn publish_part(
        &self,
        ctx: &SyncContext,
        page: &GooglePageIdentity,
    ) -> Result<Option<Checkpoint>> {
        let owned_page = page.clone();
        let (blobs, pending, since) = ctx
            .core_call(move |store| {
                Ok((
                    store.google_blob_store(),
                    store.google_live_pending(&owned_page, MAX_PART_ITEMS)?,
                    store.google_mail_since(&owned_page.account)?,
                ))
            })
            .await?;
        let mut prepared = Vec::<PreparedGoogleItem>::new();
        let mut bytes = 0;
        for item in pending {
            check_cancel(ctx)?;
            // Network quota is admitted before the shared memory/CPU permit,
            // which then spans RAW download, decoding, publication and parsing.
            // Throttled backfill cannot occupy the only slot needed by recent
            // mail, and no queue retains several large downloaded bodies.
            let (permit, outcome) = if item.staged.is_some() {
                (ctx.prepare_permit().await?, None)
            } else {
                match item.action {
                    GoogleItemAction::Fetch => {
                        let call = GmailCall::raw_message(&self.account, &item.remote_id)
                            .and_then(|call| call.with_read_class(read_class(page.lane)))
                            .map_err(provider_error)?;
                        let (reply, permit) =
                            ctx.gmail_prepared(call).await.map_err(provider_error)?;
                        let GmailReply::Raw(outcome) = reply else {
                            return Err(unexpected());
                        };
                        (permit, Some(outcome))
                    }
                    GoogleItemAction::Delete => {
                        (ctx.prepare_permit().await?, Some(RawOutcome::Missing))
                    }
                    GoogleItemAction::Observe => (ctx.prepare_permit().await?, None),
                }
            };
            let blobs = blobs.clone();
            let owned_page = page.clone();
            let cancel = ctx.cancel.clone();
            let item = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                if let Some(staged) = item.staged {
                    verify_staged_google_item(&blobs, &owned_page, staged, &cancel)
                } else {
                    prepare_google_item_since(
                        &blobs,
                        &owned_page,
                        &item.remote_id,
                        outcome,
                        &cancel,
                        since,
                    )
                }
            })
            .await
            .map_err(|_| {
                failure(
                    "google_preparation_failed",
                    "Google content preparation failed",
                )
            })??;
            let owned_page = page.clone();
            let item = ctx
                .core_call(move |store| {
                    store.google_live_stage_prepared(&owned_page, &item)?;
                    Ok(item)
                })
                .await?;
            // If this record does not fit, its durable manifest is resumed in
            // the next step. Only the current bounded part becomes visible.
            if bytes + item.normalized_bytes() > MAX_PART_BYTES {
                break;
            }
            bytes += item.normalized_bytes();
            prepared.push(item);
        }
        let owned_page = page.clone();
        ctx.core_call(move |store| {
            if !prepared.is_empty() {
                #[cfg(feature = "verification-google")]
                let started = crate::verification_google::unix_ms();
                let receipt = store.google_live_commit_part(&owned_page, &prepared)?;
                #[cfg(feature = "verification-google")]
                crate::verification_google::record_publication(&owned_page, &receipt, started);
                #[cfg(not(feature = "verification-google"))]
                let _ = receipt;
            }
            if store.google_live_pending(&owned_page, 1)?.is_empty() {
                store.google_live_finish_page(&owned_page).map(Some)
            } else {
                Ok(None)
            }
        })
        .await
    }
}

enum PageResult {
    Page(GooglePageIdentity),
    HistoryExpired,
}

fn lane(lanes: &[MailLaneState], wanted: MailLane) -> Result<&MailLaneState> {
    lanes
        .iter()
        .find(|state| state.lane == wanted)
        .ok_or_else(unexpected)
}

fn read_class(lane: MailLane) -> WorkClass {
    match lane {
        MailLane::Recent => WorkClass::UserRead,
        MailLane::Inventory => WorkClass::BackfillRead,
        MailLane::Delta => WorkClass::DeltaRead,
    }
}

fn full_baseline(checkpoint: &Checkpoint) -> Result<&str> {
    match &checkpoint.phase {
        Phase::Full {
            baseline_history_id,
            ..
        } => Ok(baseline_history_id),
        Phase::CatchUp {
            start_history_id, ..
        } => Ok(start_history_id),
        _ => Err(unexpected()),
    }
}

fn recent_cutoff() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_sub(RECENT_SECONDS)
        .min(i64::MAX as u64) as i64
}

fn check_cancel(ctx: &SyncContext) -> Result<()> {
    if ctx.cancel.is_cancelled() {
        Err(failure("cancelled", "Google mail work was cancelled"))
    } else {
        Ok(())
    }
}

fn unexpected() -> CoreError {
    failure(
        "google_invalid_response",
        "Google mail returned an invalid response",
    )
}

fn failure(code: &str, message: &str) -> CoreError {
    CoreError {
        code: code.into(),
        message: message.into(),
        details: Value::Null,
    }
}

fn provider_error(error: GoogleError) -> CoreError {
    match error {
        GoogleError::RateLimited { retry_after_secs } => CoreError {
            code: "google_rate_limited".into(),
            message: "Google mail is paused by its rate limit".into(),
            details: json!({"retry_after_secs":retry_after_secs}),
        },
        GoogleError::ReauthorizationRequired => failure(
            "google_reauthorization_required",
            "Reconnect this Google account to continue mail sync",
        ),
        GoogleError::PermissionDenied => failure(
            "google_permission_denied",
            "Google mail access is not permitted",
        ),
        GoogleError::ServerUnavailable => failure(
            "google_server_unavailable",
            "Google mail is temporarily unavailable",
        ),
        GoogleError::TransportInterrupted => failure(
            "google_transport_interrupted",
            "The Google mail connection was interrupted",
        ),
        GoogleError::StoreUnavailable => failure(
            "google_store_unavailable",
            "Google mail could not access local storage",
        ),
        GoogleError::StaleCheckpoint => failure("stale_google_work", "Google mail work changed"),
        _ => unexpected(),
    }
}

#[cfg(test)]
mod tests;
