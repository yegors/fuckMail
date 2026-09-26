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
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How far back the recent lane lists: 30 days.
const RECENT_SECONDS: u64 = 30 * 24 * 60 * 60;
/// Listing parts published before a due history check takes a turn, so new
/// mail keeps arriving during a long backfill.
const PARTS_BEFORE_DELTA: u8 = 4;
// IDs are cheap, but a smaller page makes the initial useful result independent
// of an account's total history. History records can still fan out to 5,000 IDs.
const LIST_PAGE_SIZE: u16 = 100;
const HISTORY_PAGE_SIZE: u16 = 100;
// The inventory count lists IDs only, so it takes Gmail's largest page, and
// counts for up to this long per inventory step: the total arrives in about a
// minute however slowly the mail itself downloads.
const COUNT_PAGE_SIZE: u16 = 500;
const COUNT_STEP_BUDGET: Duration = Duration::from_secs(2);

/// What a worker step did, which sets how soon the loop steps again.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// Work was done and more may be ready; step again at once.
    Progress,
    /// Nothing is due; wait this long unless a refresh arrives first.
    Idle(Duration),
}

/// One account's Gmail acquisition, advanced one bounded step at a time.
///
/// Durable progress lives in the store's lanes and pages; this struct holds
/// only scheduling state, so a new worker resumes where the last one stopped.
pub(crate) struct MailWorker {
    account: AccountId,
    /// Listing parts since the delta lane last ran.
    non_delta_parts: u8,
    /// When the next history check is due, on the caller's monotonic clock (ms).
    next_delta_ms: u64,
    count: Option<InventoryCount>,
    /// Told, as each step picks its lane, whether it downloads older mail
    /// (true) or checks for new mail (false).
    lane_observer: Option<Box<dyn Fn(bool) + Send + Sync>>,
}

/// Counts what the inventory will list, apart from downloading it, so the
/// account's progress has a total. Held in memory; a restart counts
/// again from the start.
struct InventoryCount {
    generation: u64,
    page_token: Option<String>,
    counted: u64,
    complete: bool,
}

impl MailWorker {
    pub(crate) fn new(account: AccountId) -> Self {
        Self {
            account,
            non_delta_parts: 0,
            next_delta_ms: 0,
            count: None,
            lane_observer: None,
        }
    }

    /// Reports each step's lane before its requests start: `true` for the
    /// recent and inventory listings, which download older mail in the
    /// background, `false` for a history check for new mail.
    pub(crate) fn observe_lanes(&mut self, observer: impl Fn(bool) + Send + Sync + 'static) {
        self.lane_observer = Some(Box::new(observer));
    }

    /// Makes the next step check Gmail history, even while a listing lane
    /// still has parts to publish.
    pub(crate) fn request_refresh(&mut self) {
        self.next_delta_ms = 0;
        self.non_delta_parts = PARTS_BEFORE_DELTA;
    }

    /// Runs one unit of work: starts the lanes, publishes one part of one
    /// page, or restarts after Gmail history expired.
    ///
    /// `now_ms` is a monotonic clock in milliseconds. An error leaves durable
    /// state consistent, so a later step resumes the saved page.
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
            // The baseline is read before any listing, so the delta lane
            // replays every change made while the listings run.
            let baseline = self.baseline(ctx).await?;
            let account = self.account.as_str().to_owned();
            let cutoff = recent_cutoff();
            ctx.core_call(move |store| {
                store.google_live_begin(&account, &baseline, cutoff)?;
                Ok(())
            })
            .await?;
            crate::diagnostics::event(
                "Sync",
                "mail_baseline_started",
                json!({"account_ref":crate::diagnostics::reference("account",ctx.account.as_str()),"connection_generation":ctx.connection_generation}),
            );
            return Ok(Step::Progress);
        }
        let recent = lane(&lanes, MailLane::Recent)?;
        let inventory = lane(&lanes, MailLane::Inventory)?;
        let delta = lane(&lanes, MailLane::Delta)?;
        // Recent mail first, then the inventory. While a listing runs, a due
        // history check gets a turn after every PARTS_BEFORE_DELTA listing
        // parts; once the inventory is complete it runs whenever due. An
        // unfinished history scan is always due.
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
        if let Some(observe) = &self.lane_observer {
            observe(selected.lane != MailLane::Delta);
        }

        // While the inventory downloads, each of its steps also counts it,
        // page after page for up to COUNT_STEP_BUDGET. Counting is best
        // effort: a failed page is tried again next step and never stops the
        // download.
        if selected.lane == MailLane::Inventory && !selected.complete {
            let started = Instant::now();
            loop {
                match self.count_page(ctx, selected).await {
                    Ok(true) if started.elapsed() < COUNT_STEP_BUDGET => continue,
                    Ok(_) => break,
                    Err(error) => {
                        check_cancel(ctx)?;
                        crate::diagnostics::event(
                            "Sync",
                            "mail_count_failed",
                            json!({"account_ref":crate::diagnostics::reference("account",ctx.account.as_str()),"code":error.code}),
                        );
                        break;
                    }
                }
            }
        }

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
            // A finished inventory only rechecks stored messages it did not
            // see this generation; with none left there is nothing to do
            // until the next history check.
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
                    crate::diagnostics::event(
                        "Sync",
                        "mail_history_expired",
                        json!({"account_ref":crate::diagnostics::reference("account",ctx.account.as_str()),"lane":format!("{:?}",selected.lane),
                        "generation":generation,"connection_generation":ctx.connection_generation}),
                    );
                    self.non_delta_parts = 0;
                    self.next_delta_ms = 0;
                    return Ok(Step::Progress);
                }
            }
        };

        let completed = self.publish_part(ctx, &page).await?;
        if page.lane == MailLane::Delta {
            self.non_delta_parts = 0;
            // The poll interval starts only once a scan has fully caught up.
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

    /// Reads the mailbox's current `historyId` from the Gmail profile.
    async fn baseline(&self, ctx: &SyncContext) -> Result<String> {
        let call = GmailCall::profile(&self.account).map_err(provider_error)?;
        match ctx.gmail(call).await.map_err(provider_error)? {
            GmailReply::Profile(profile) => Ok(profile.history_id),
            _ => Err(unexpected()),
        }
    }

    /// Records `plan` durably before any of its messages are fetched.
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

    /// Lists one page of the inventory's query for its count, unless the
    /// count for this generation is complete. True while pages remain.
    async fn count_page(&mut self, ctx: &SyncContext, state: &MailLaneState) -> Result<bool> {
        let generation = state.checkpoint.generation;
        // Page tokens are not stored, so only a finished count survives a
        // restart; a partial one starts again from zero.
        if self
            .count
            .as_ref()
            .is_none_or(|count| count.generation != generation)
        {
            let account = self.account.as_str().to_owned();
            let stored = ctx
                .core_call(move |store| store.google_mail_count(&account))
                .await?;
            self.count = Some(match stored {
                Some((counted, true)) => InventoryCount {
                    generation,
                    page_token: None,
                    counted,
                    complete: true,
                },
                _ => InventoryCount {
                    generation,
                    page_token: None,
                    counted: 0,
                    complete: false,
                },
            });
        }
        let Some(count) = self.count.as_mut().filter(|count| !count.complete) else {
            return Ok(false);
        };
        // The same query and scope as the inventory's own listing.
        let query = match state.query.as_deref() {
            Some(query) => format!("{query} -in:spam"),
            None => "-in:spam".to_owned(),
        };
        let call = GmailCall::list_messages(
            &self.account,
            count.page_token.as_deref(),
            Some(&query),
            true,
            COUNT_PAGE_SIZE,
        )
        .and_then(|call| call.with_read_class(WorkClass::BackfillRead))
        .map_err(provider_error)?;
        let GmailReply::Messages(page) = ctx.gmail(call).await.map_err(provider_error)? else {
            return Err(unexpected());
        };
        count.counted += page.messages.len() as u64;
        count.complete = page.next_page_token.is_none();
        count.page_token = page.next_page_token;
        let (account, counted, complete) = (
            self.account.as_str().to_owned(),
            count.counted,
            count.complete,
        );
        let current = ctx
            .core_call(move |store| {
                store.google_mail_count_record(&account, generation, counted, complete)
            })
            .await?;
        if !current {
            // The window changed underneath; the next step starts a new count.
            self.count = None;
            return Ok(false);
        }
        Ok(!complete)
    }

    /// Requests the lane's next provider page and records its plan: a
    /// `messages.list` page for a listing, or a `history.list` page for the
    /// delta lane. An idle delta lane records an empty page that starts the
    /// next scan from its committed history ID.
    async fn fetch_page(&self, ctx: &SyncContext, state: &MailLaneState) -> Result<PageResult> {
        let checkpoint = &state.checkpoint;
        let (plan, history) = match &checkpoint.phase {
            Phase::Full {
                baseline_history_id,
                page_token,
            } => {
                // Trash is kept; Spam is never downloaded and is read live.
                let query = match state.query.as_deref() {
                    Some(query) => format!("{query} -in:spam"),
                    None => "-in:spam".to_owned(),
                };
                let call = GmailCall::list_messages(
                    &self.account,
                    page_token.as_deref(),
                    Some(&query),
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

    /// Prepares the page's next pending items, publishes them as one bounded
    /// part, and finishes the page once nothing is pending. Returns the lane's
    /// new checkpoint when the page finished.
    ///
    /// Items run one at a time so that at most one decoded message body is in
    /// memory per worker. Each is staged before joining the part, so work
    /// done before a cancellation or crash is verified rather than downloaded
    /// again.
    async fn publish_part(
        &self,
        ctx: &SyncContext,
        page: &GooglePageIdentity,
    ) -> Result<Option<Checkpoint>> {
        let started = std::time::Instant::now();
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
        let item_count = prepared.len();
        let completed = ctx
            .core_call(move |store| {
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
            .await?;
        crate::diagnostics::event(
            "Sync",
            "mail_part_published",
            json!({"account_ref":crate::diagnostics::reference("account",&page.account),"lane":format!("{:?}",page.lane),
            "generation":page.generation,"connection_generation":page.connection_generation,
            "items":item_count,"normalized_bytes":bytes,"page_complete":completed.is_some(),
            "elapsed_ms":started.elapsed().as_millis()}),
        );
        Ok(completed)
    }
}

enum PageResult {
    Page(GooglePageIdentity),
    /// Gmail no longer has history from the lane's start point; the account
    /// must list everything again under a new generation.
    HistoryExpired,
}

fn lane(lanes: &[MailLaneState], wanted: MailLane) -> Result<&MailLaneState> {
    lanes
        .iter()
        .find(|state| state.lane == wanted)
        .ok_or_else(unexpected)
}

/// The quota class each lane's reads are charged to. Recent mail counts as a
/// read the person is waiting on; the inventory is background work that the
/// scheduler keeps from starving the rest.
fn read_class(lane: MailLane) -> WorkClass {
    match lane {
        MailLane::Recent => WorkClass::UserRead,
        MailLane::Inventory => WorkClass::BackfillRead,
        MailLane::Delta => WorkClass::DeltaRead,
    }
}

/// The history ID a finished inventory's extra pages are fenced by: the
/// listing baseline its checkpoint still carries.
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

/// The recent lane's cutoff in Unix seconds, from the wall clock.
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

/// Maps a Gmail client error to the stable error code the sync loop reports
/// and bases its retry decision on.
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
