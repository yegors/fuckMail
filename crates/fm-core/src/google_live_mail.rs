//! Live Gmail acquisition: bounded visible parts, durable provider-page completion.
//! Preparation owns filesystem/CPU work. Store commands below perform SQL only.
//!
//! Each account syncs through three lanes that share one history baseline, read
//! from the Gmail profile before any listing starts:
//!
//! - `Recent` lists mail after the caller's recent cutoff, so current mail is
//!   usable before the backfill ends.
//! - `Inventory` lists every message inside the retention window.
//! - `Delta` replays `history.list` from the baseline, then from each committed
//!   history ID.
//!
//! Work moves in provider pages. A page's plan is recorded before any of its
//! messages are fetched, so a restart resumes the exact IDs and continuation
//! token instead of listing again, which could return different ones. Each
//! message is prepared off the writer, staged durably, then published in parts
//! of at most [`MAX_PART_ITEMS`] items and [`MAX_PART_BYTES`] normalized bytes,
//! each in one transaction with a replayable receipt. A page finishes, and its
//! lane checkpoint advances, only once every planned item has a committed
//! outcome.
//!
//! Two counters fence stale work. The account's mail generation increases when
//! Gmail history expires or the retention window changes; lanes and pages of an
//! older generation become unusable. The connection generation increases on
//! every connect, reconnect or connection-state change; a page carries the value
//! it was admitted under, and preparation made under an older value must be
//! verified again before it can publish.
pub use crate::content::BlobStore;
use crate::{err, Result, Store};
use fm_google::sync::{Checkpoint, PagePlan, Phase};
use fm_google::AccountId;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[path = "google_live_mail/preparation.rs"]
mod preparation;
pub use preparation::{
    prepare_google_item, prepare_google_item_since, verify_staged_google_item, ItemIssue,
    PreparedGoogleItem, StoredGoogleItem,
};
#[cfg(test)]
#[path = "google_live_mail/tests.rs"]
mod tests;

/// The most prepared items one publication part commits together. Pending-work
/// reads, reconciliation pages and retries use the same bound.
pub const MAX_PART_ITEMS: usize = 10;
/// The most normalized manifest bytes one publication part may hold; a single
/// prepared item larger than this is refused during preparation.
pub const MAX_PART_BYTES: usize = 512 * 1024;
/// The most distinct message IDs one recorded page may plan. A history page can
/// fan out to this many changes.
const MAX_PAGE_ITEMS: usize = 5000;

/// One of an account's three acquisition streams, each with its own checkpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailLane {
    /// Lists recent mail first, so current mail appears before the backfill ends.
    Recent,
    /// Lists every message inside the retention window.
    Inventory,
    /// Replays Gmail history from the shared baseline, then from each committed
    /// history ID.
    Delta,
}
impl MailLane {
    fn key(self) -> &'static str {
        match self {
            Self::Recent => "recent",
            Self::Inventory => "inventory",
            Self::Delta => "delta",
        }
    }
}

/// A lane's stored position in the account's current mail generation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MailLaneState {
    pub lane: MailLane,
    /// The checkpoint the lane's next page must start from.
    pub checkpoint: Checkpoint,
    /// The Gmail search the lane lists with (`after:<unix seconds>`); `None`
    /// lists everything. Always `None` for `Delta`.
    pub query: Option<String>,
    /// For `Recent` and `Inventory`, whether the listing has ended. For
    /// `Delta`, whether the latest history scan caught up; finishing the
    /// inventory clears it.
    pub complete: bool,
}

/// Immutable page and authorization epoch. A renewed account invalidates old work.
///
/// Every later call for the page presents this value. It goes stale when the
/// account reconnects, when its mail generation changes, or when the recorded
/// plan no longer matches.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GooglePageIdentity {
    pub account: String,
    pub lane: MailLane,
    /// The account's mail generation when the page was recorded.
    pub generation: u64,
    /// The lane checkpoint revision the plan starts from. With the account,
    /// lane and generation it keys the page.
    pub expected_revision: u64,
    /// The connection generation this identity was issued under.
    pub connection_generation: u64,
    /// The history ID the page was read at: `history.list`'s top-level
    /// `historyId`, or the listing's baseline. It orders outcomes that carry
    /// no history ID of their own, such as deletions.
    pub observed_history_id: String,
    fingerprint: String,
}

/// What a planned message ID needs before its page can finish.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoogleItemAction {
    /// Download the message's raw source.
    Fetch,
    /// Record a deletion Gmail reported; nothing is downloaded.
    Delete,
    /// Record the ID as seen in this generation without downloading it.
    Observe,
}
impl GoogleItemAction {
    fn key(self) -> &'static str {
        match self {
            Self::Fetch => "fetch",
            Self::Delete => "delete",
            Self::Observe => "observe",
        }
    }
}
/// A planned item without a committed outcome, as returned by
/// [`Store::google_live_pending`].
pub struct PendingGoogleItem {
    pub remote_id: String,
    pub action: GoogleItemAction,
    /// A manifest staged earlier, for example before a restart or reconnect.
    /// It is untrusted until [`verify_staged_google_item`] re-hashes its
    /// blobs, after which the message need not be downloaded again.
    pub staged: Option<StoredGoogleItem>,
}
/// Proof that one publication part committed. Replaying the same part returns
/// an equal receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GooglePartReceipt {
    /// Hash of the page fingerprint and each item's ID and manifest digest.
    pub key: String,
    /// The store revision the part committed at.
    pub revision: i64,
    pub item_count: usize,
}

/// How much of an account's Gmail is held locally, for status and progress.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GoogleMailCoverage {
    /// The account's current mail generation.
    pub generation: u64,
    /// The inventory listing has ended in this generation.
    pub inventory_complete: bool,
    /// The last history scan caught up under the current connection and the
    /// delta lane is idle.
    pub delta_current: bool,
    /// Inventory complete, history current, no page in progress and no
    /// message unavailable.
    pub offline_complete: bool,
    /// Messages recorded and not deleted.
    pub observed: u64,
    /// Of those, messages whose raw source is stored.
    pub retained_sources: u64,
    /// Messages parsed and searchable.
    pub indexed: u64,
    /// Messages whose attachments are all stored.
    pub complete_attachments: u64,
    /// Messages missing their source, index or attachments, or not yet seen
    /// in the current generation.
    pub unavailable: u64,
    /// Messages Google lists for the inventory's query, once counted.
    pub listed: Option<u64>,
    /// Messages counted so far while that count runs.
    pub listing: Option<u64>,
}

/// A message whose latest acquisition outcome recorded an issue, as listed by
/// [`Store::google_live_gaps`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GoogleMailGap {
    pub remote_id: String,
    /// The issue key, such as `item_too_large` or `malformed_mime`.
    pub issue: String,
    /// The raw source is stored even though the message was not fully imported.
    pub source_retained: bool,
}

/// The stored form of a page: the provider plan and the history ID it was
/// read at. Its JSON hash is the page fingerprint.
#[derive(Serialize, Deserialize)]
struct RecordedPage {
    plan: PagePlan,
    observed_history_id: String,
}
fn stale() -> crate::CoreError {
    err(
        "stale_google_work",
        "Google work belongs to an older account, generation or page",
    )
}
fn number(value: &str) -> Result<u64> {
    value.parse().map_err(|_| {
        err(
            "invalid_google_history",
            "Invalid Google history identifier",
        )
    })
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
/// Returns the account's `(mail_generation, connection_generation)` once it is
/// active, connected and granted Gmail read access.
fn active_account(db: &Connection, account: &str) -> Result<(u64, u64)> {
    crate::account_lifecycle::active(db, account)?;
    let row: Option<(u64, u64, String, String)> = db.query_row(
        "SELECT mail_generation,connection_generation,connection_state,scopes_json FROM google_accounts WHERE account=?1",
        [account], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
    ).optional()?;
    let Some((generation, revision, state, scopes)) = row else {
        return Err(stale());
    };
    let scopes: BTreeSet<String> = serde_json::from_str(&scopes)?;
    if state != "connected" || !fm_google::auth::ScopeSet::new(&scopes).mail_read() {
        return Err(err(
            "google_mail_unavailable",
            "Google mail reading is not authorized",
        ));
    }
    Ok((generation, revision))
}

/// The `meta` keys of an account's inventory count: the generation it counts,
/// the number counted and whether it finished.
pub(crate) fn mail_count_keys(account: &str) -> [String; 3] {
    ["generation", "messages", "complete"].map(|part| format!("mail_count_{part}:{account}"))
}

impl Store {
    /// How many messages Google lists for the inventory's query, counted apart
    /// from downloading so progress has a total: `(counted, complete)`
    /// for the account's current mail generation, or None before a count.
    ///
    /// A count kept for an earlier generation reads as `None`. Fails with
    /// `stale_google_work` if the account has no Google binding.
    pub fn google_mail_count(&self, account: &str) -> Result<Option<(u64, bool)>> {
        let generation: i64 = self
            .db
            .query_row(
                "SELECT mail_generation FROM google_accounts WHERE account=?1",
                [account],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(stale)?;
        let [generation_key, messages_key, complete_key] = mail_count_keys(account);
        let value = |key: &str| -> Result<Option<i64>> {
            Ok(self
                .db
                .query_row("SELECT value FROM meta WHERE key=?1", [key], |r| r.get(0))
                .optional()?)
        };
        if value(&generation_key)? != Some(generation) {
            return Ok(None);
        }
        Ok(value(&messages_key)?.map(|n| {
            (
                n.max(0) as u64,
                value(&complete_key).ok().flatten() == Some(1),
            )
        }))
    }
    /// Records a running or finished count for `generation`. Returns false,
    /// writing nothing, once the account has moved to another generation.
    ///
    /// The generation check and the write share one transaction, so a count
    /// cannot land on a window that changed underneath it. Bumps the store
    /// revision.
    pub fn google_mail_count_record(
        &self,
        account: &str,
        generation: u64,
        counted: u64,
        complete: bool,
    ) -> Result<bool> {
        let tx = self.db.unchecked_transaction()?;
        let current: Option<i64> = tx
            .query_row(
                "SELECT mail_generation FROM google_accounts WHERE account=?1",
                [account],
                |r| r.get(0),
            )
            .optional()?;
        if current != Some(generation as i64) {
            return Ok(false);
        }
        let [generation_key, messages_key, complete_key] = mail_count_keys(account);
        for (key, value) in [
            (generation_key, generation as i64),
            (messages_key, counted as i64),
            (complete_key, complete as i64),
        ] {
            tx.execute("INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, value])?;
        }
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(true)
    }
    /// Returns the start of the account's mail retention window in Unix
    /// seconds, or `None` when all mail is kept.
    ///
    /// The value is fixed when the retention is chosen, not rolling. Messages
    /// whose Gmail internal date is earlier are not stored.
    pub fn google_mail_since(&self, account: &str) -> Result<Option<i64>> {
        Ok(self.db.query_row(
            "SELECT mail_since FROM account_metadata WHERE account=?1",
            [account],
            |r| r.get(0),
        )?)
    }
    /// Start all lanes from a baseline captured before listing. Reopening preserves
    /// the stored cutoff and cursors, regardless of newly supplied defaults.
    ///
    /// `baseline` is the Gmail profile's `historyId`; the delta lane replays
    /// from it, so nothing that changes during the listing is missed.
    /// `recent_after_secs` is the Unix-seconds cutoff for the recent lane,
    /// raised to the retention start when that is later. Fails with
    /// `google_mail_unavailable` unless the account is connected with Gmail
    /// read access.
    pub fn google_live_begin(
        &mut self,
        account: &str,
        baseline: &str,
        recent_after_secs: i64,
    ) -> Result<Vec<MailLaneState>> {
        let (generation, _) = active_account(&self.db, account)?;
        number(baseline)?;
        let existing = self.google_live_lanes(account)?;
        if !existing.is_empty() {
            return Ok(existing);
        }
        let tx = self.db.unchecked_transaction()?;
        insert_lanes(&tx, account, generation, baseline, recent_after_secs)?;
        tx.commit()?;
        self.google_live_lanes(account)
    }

    /// Returns the account's three lanes for its current mail generation, or
    /// an empty list before [`Store::google_live_begin`].
    ///
    /// Fails like `google_live_begin` when the account cannot read Gmail, and
    /// with `stale_google_work` if only some lanes exist.
    pub fn google_live_lanes(&self, account: &str) -> Result<Vec<MailLaneState>> {
        let (generation, _) = active_account(&self.db, account)?;
        self.google_live_stored_lanes(account, generation)
    }

    /// Reads the lanes for `generation` without checking the connection, so
    /// coverage can still be reported while the account awaits reconnection.
    fn google_live_stored_lanes(
        &self,
        account: &str,
        generation: u64,
    ) -> Result<Vec<MailLaneState>> {
        let mut result = Vec::new();
        for lane in [MailLane::Recent, MailLane::Inventory, MailLane::Delta] {
            let row: Option<(String,String,bool)> = self.db.query_row(
                "SELECT checkpoint_json,query_json,complete FROM google_live_lanes WHERE account=?1 AND lane=?2 AND generation=?3",
                params![account,lane.key(),generation], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
            ).optional()?;
            if let Some((checkpoint, query, complete)) = row {
                result.push(MailLaneState {
                    lane,
                    checkpoint: serde_json::from_str(&checkpoint)?,
                    query: serde_json::from_str(&query)?,
                    complete,
                });
            }
        }
        if !result.is_empty() && result.len() != 3 {
            return Err(stale());
        }
        Ok(result)
    }

    /// History expiry starts a new reconciliation generation; no mail, intent,
    /// draft, source or past receipt is deleted by this operation.
    ///
    /// Replaces the lanes with fresh ones from `baseline` under the next mail
    /// generation, which makes every page and prepared item of the old one
    /// stale. Fails with `stale_google_work` unless `expected_generation` is
    /// still current, so two callers cannot both advance it.
    pub fn google_live_resync(
        &mut self,
        account: &str,
        expected_generation: u64,
        baseline: &str,
        recent_after_secs: i64,
    ) -> Result<Vec<MailLaneState>> {
        let (generation, _) = active_account(&self.db, account)?;
        if generation != expected_generation {
            return Err(stale());
        }
        number(baseline)?;
        let next = generation
            .checked_add(1)
            .filter(|v| *v <= i64::MAX as u64)
            .ok_or_else(stale)?;
        let tx = self.db.unchecked_transaction()?;
        tx.execute(
            "UPDATE google_accounts SET mail_generation=?2 WHERE account=?1",
            params![account, next],
        )?;
        tx.execute("DELETE FROM google_live_lanes WHERE account=?1", [account])?;
        // A message counts as unavailable until it is seen in the current
        // generation, so after the bump every recorded message is unavailable
        // until the new inventory observes it again.
        tx.execute(
            "UPDATE google_mail_metrics SET unavailable=observed WHERE account=?1",
            [account],
        )?;
        insert_lanes(&tx, account, next, baseline, recent_after_secs)?;
        tx.commit()?;
        self.google_live_lanes(account)
    }

    /// `observed_history_id` is history.list's top-level historyId, or the fixed
    /// initial baseline for an inventory page. It fences deletion observations.
    ///
    /// Records `plan` as the lane's next page and returns its identity. The
    /// plan must start from the lane's stored checkpoint. Beginning an
    /// identical plan again is idempotent and returns the same page; a
    /// different plan for the same checkpoint fails with `stale_google_work`.
    pub fn google_live_begin_page(
        &mut self,
        lane: MailLane,
        plan: &PagePlan,
        observed_history_id: &str,
    ) -> Result<GooglePageIdentity> {
        validate_plan(lane, plan, observed_history_id)?;
        let account = plan.expected.account.as_str();
        let (generation, connection_generation) = active_account(&self.db, account)?;
        if generation != plan.expected.generation {
            return Err(stale());
        }
        let record = RecordedPage {
            plan: plan.clone(),
            observed_history_id: observed_history_id.into(),
        };
        let json = serde_json::to_string(&record)?;
        let fingerprint = hash(json.as_bytes());
        let page = GooglePageIdentity {
            account: account.into(),
            lane,
            generation,
            expected_revision: plan.expected.revision,
            connection_generation,
            observed_history_id: observed_history_id.into(),
            fingerprint,
        };
        // One page per lane checkpoint. A recorded page can only be resumed
        // with the exact same plan, never replaced by a fresh listing.
        let previous: Option<(String,bool,u64)> = self.db.query_row("SELECT fingerprint,complete,connection_generation FROM google_live_pages WHERE account=?1 AND lane=?2 AND generation=?3 AND expected_revision=?4",params![account,lane.key(),generation,page.expected_revision],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((previous, complete, epoch)) = previous {
            if previous != page.fingerprint || (complete && epoch != connection_generation) {
                return Err(stale());
            }
            // Reauthorization invalidates in-flight proofs. Persisted preparation
            // may be rebound only by the recovery verifier using this new identity.
            if !complete {
                self.db.execute("UPDATE google_live_pages SET connection_generation=?5 WHERE account=?1 AND lane=?2 AND generation=?3 AND expected_revision=?4",params![account,lane.key(),generation,page.expected_revision,connection_generation])?;
            }
            return Ok(page);
        }
        ensure_checkpoint(&self.db, &page, &plan.expected)?;
        self.db.execute("INSERT INTO google_live_pages(account,lane,generation,expected_revision,connection_generation,fingerprint,plan_json) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![account,lane.key(),generation,page.expected_revision,connection_generation,page.fingerprint,json])?;
        Ok(page)
    }

    /// Resume the exact saved provider page before issuing another list/history
    /// request. Re-fetching that request can produce different IDs or tokens.
    ///
    /// Returns `None` when the lane has no unfinished page at its current
    /// checkpoint. A resumed page is rebound to the current connection
    /// generation, so its staged items must be verified before publishing.
    pub fn google_live_resume_page(
        &mut self,
        account: &str,
        lane: MailLane,
    ) -> Result<Option<GooglePageIdentity>> {
        let state = self
            .google_live_lanes(account)?
            .into_iter()
            .find(|state| state.lane == lane)
            .ok_or_else(stale)?;
        let json:Option<String>=self.db.query_row("SELECT plan_json FROM google_live_pages WHERE account=?1 AND lane=?2 AND generation=?3 AND expected_revision=?4 AND complete=0",params![account,lane.key(),state.checkpoint.generation,state.checkpoint.revision],|r|r.get(0)).optional()?;
        let Some(json) = json else {
            return Ok(None);
        };
        let record: RecordedPage = serde_json::from_str(&json)?;
        if record.plan.expected != state.checkpoint {
            return Err(stale());
        }
        self.google_live_begin_page(lane, &record.plan, &record.observed_history_id)
            .map(Some)
    }

    /// Bounded work read; manifests contain no source bytes. Network failures stay
    /// pending. Permanent item outcomes advance only through committed receipts.
    ///
    /// Returns up to `limit` (1 to [`MAX_PART_ITEMS`]) planned items without a
    /// committed outcome, in remote-ID order, each with any manifest staged
    /// for it. A finished page returns an empty list.
    pub fn google_live_pending(
        &self,
        page: &GooglePageIdentity,
        limit: usize,
    ) -> Result<Vec<PendingGoogleItem>> {
        if limit == 0 || limit > MAX_PART_ITEMS {
            return Err(err("google_part_limit", "Invalid pending work limit"));
        }
        let (record, complete) = load_page(&self.db, page)?;
        if complete {
            return Ok(Vec::new());
        }
        ensure_checkpoint(&self.db, page, &record.plan.expected)?;
        let mut result = Vec::new();
        let mut statement=self.db.prepare("SELECT remote_id FROM google_live_page_items WHERE account=?1 AND lane=?2 AND generation=?3 AND expected_revision=?4 AND applied_revision IS NOT NULL")?;
        let applied = statement
            .query_map(
                params![
                    page.account,
                    page.lane.key(),
                    page.generation,
                    page.expected_revision
                ],
                |r| r.get::<_, String>(0),
            )?
            .collect::<std::result::Result<BTreeSet<_>, _>>()?;
        for remote in planned_ids(&record.plan) {
            if applied.contains(&remote) {
                continue;
            }
            let row: Option<(Option<String>,Option<i64>)> = self.db.query_row("SELECT manifest_json,applied_revision FROM google_live_page_items WHERE account=?1 AND lane=?2 AND generation=?3 AND expected_revision=?4 AND remote_id=?5",params![page.account,page.lane.key(),page.generation,page.expected_revision,remote],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            if row.as_ref().is_some_and(|(_, applied)| applied.is_some()) {
                continue;
            }
            result.push(PendingGoogleItem {
                action: action(&record.plan, &remote)?,
                remote_id: remote,
                staged: row.and_then(|(json, _)| json).map(StoredGoogleItem),
            });
            if result.len() == limit {
                break;
            }
        }
        Ok(result)
    }

    /// Persists a prepared item's manifest so a restart can verify and publish
    /// it without downloading the message again.
    ///
    /// Staging does not make the item visible; only
    /// [`Store::google_live_commit_part`] does. Restaging replaces an
    /// unpublished manifest. For an item already published, the same digest
    /// is a no-op and a different one fails with `stale_google_work`.
    pub fn google_live_stage_prepared(
        &mut self,
        page: &GooglePageIdentity,
        item: &PreparedGoogleItem,
    ) -> Result<()> {
        let (record, complete) = load_page(&self.db, page)?;
        if complete {
            return Err(stale());
        }
        ensure_checkpoint(&self.db, page, &record.plan.expected)?;
        validate_prepared(&self.profile, page, item, &record.plan)?;
        let old: Option<(Option<String>,Option<i64>)> = self.db.query_row("SELECT manifest_hash,applied_revision FROM google_live_page_items WHERE account=?1 AND lane=?2 AND generation=?3 AND expected_revision=?4 AND remote_id=?5",params![page.account,page.lane.key(),page.generation,page.expected_revision,item.manifest.remote_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((fingerprint, applied)) = old {
            if applied.is_some() {
                return if fingerprint.as_deref() == Some(&item.digest) {
                    Ok(())
                } else {
                    Err(stale())
                };
            }
        }
        self.db.execute("INSERT INTO google_live_page_items(account,lane,generation,expected_revision,remote_id,ordinal,required_action,manifest_json,manifest_hash) VALUES(?1,?2,?3,?4,?5,0,?6,?7,?8) ON CONFLICT(account,lane,generation,expected_revision,remote_id) DO UPDATE SET manifest_json=excluded.manifest_json,manifest_hash=excluded.manifest_hash",params![page.account,page.lane.key(),page.generation,page.expected_revision,item.manifest.remote_id,action(&record.plan,&item.manifest.remote_id)?.key(),item.json,item.digest])?;
        Ok(())
    }

    /// No parsing, filesystem access or large-content hashing in this method.
    /// Source/FTS/intent overlays/coverage and exact part receipt share one commit.
    ///
    /// Publishes 1 to [`MAX_PART_ITEMS`] items, totalling at most
    /// [`MAX_PART_BYTES`], that were each staged first with the same digest.
    /// Committing the same set again returns the original receipt without
    /// writing; overlapping a different committed part fails with
    /// `stale_google_work`. Bumps the store revision.
    pub fn google_live_commit_part(
        &mut self,
        page: &GooglePageIdentity,
        items: &[PreparedGoogleItem],
    ) -> Result<GooglePartReceipt> {
        if items.is_empty()
            || items.len() > MAX_PART_ITEMS
            || items.iter().map(|i| i.json.len()).sum::<usize>() > MAX_PART_BYTES
        {
            return Err(err(
                "google_part_limit",
                "Google publication part exceeds its item or normalized-byte limit",
            ));
        }
        let (record, complete) = load_page(&self.db, page)?;
        let mut ids = BTreeSet::new();
        let mut digests = Vec::new();
        for item in items {
            validate_prepared(&self.profile, page, item, &record.plan)?;
            if !ids.insert(&item.manifest.remote_id) {
                return Err(stale());
            }
            digests.push((&item.manifest.remote_id, &item.digest));
        }
        digests.sort();
        // The receipt key names the page and the exact item contents, in an
        // order-independent way, so a replay can be recognised.
        let key = hash(serde_json::to_vec(&(&page.fingerprint, digests))?.as_slice());
        let tx = self.db.unchecked_transaction()?;
        // Every item must be staged with this digest. Items already applied
        // must all belong to this very part, which is then a replay.
        let mut applied = Vec::new();
        for item in items {
            let row: (Option<String>,Option<String>,Option<i64>) = tx.query_row("SELECT manifest_hash,part_key,applied_revision FROM google_live_page_items WHERE account=?1 AND lane=?2 AND generation=?3 AND expected_revision=?4 AND remote_id=?5",params![page.account,page.lane.key(),page.generation,page.expected_revision,item.manifest.remote_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?.ok_or_else(stale)?;
            if row.0.as_deref() != Some(&item.digest) {
                return Err(stale());
            }
            if let Some(revision) = row.2 {
                if row.1.as_deref() != Some(&key) {
                    return Err(stale());
                }
                applied.push(revision);
            }
        }
        if !applied.is_empty() {
            if applied.len() != items.len() || applied.iter().any(|r| *r != applied[0]) {
                return Err(stale());
            }
            return Ok(GooglePartReceipt {
                key,
                revision: applied[0],
                item_count: items.len(),
            });
        }
        if complete {
            return Err(stale());
        }
        ensure_checkpoint(&tx, page, &record.plan.expected)?;
        for item in items {
            publish_item(&tx, page, &item.manifest)?;
        }
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        let revision = tx.query_row("SELECT value FROM meta WHERE key='revision'", [], |r| {
            r.get(0)
        })?;
        for item in items {
            tx.execute("UPDATE google_live_page_items SET part_key=?6,applied_revision=?7 WHERE account=?1 AND lane=?2 AND generation=?3 AND expected_revision=?4 AND remote_id=?5",params![page.account,page.lane.key(),page.generation,page.expected_revision,item.manifest.remote_id,key,revision])?;
        }
        tx.commit()?;
        Ok(GooglePartReceipt {
            key,
            revision,
            item_count: items.len(),
        })
    }

    /// Completes a page whose planned items all have committed outcomes and
    /// advances its lane to the plan's next checkpoint, which it returns.
    ///
    /// Finishing an already finished page returns the same checkpoint. Fails
    /// with `google_page_pending` while any item is unpublished. Finishing the
    /// inventory marks the delta lane incomplete, and a caught-up history scan
    /// records the account's last mail sync time. Bumps the store revision.
    pub fn google_live_finish_page(&mut self, page: &GooglePageIdentity) -> Result<Checkpoint> {
        let (record, complete) = load_page(&self.db, page)?;
        if complete {
            return Ok(record.plan.next);
        }
        let tx = self.db.unchecked_transaction()?;
        ensure_checkpoint(&tx, page, &record.plan.expected)?;
        let applied: usize = tx.query_row("SELECT count(*) FROM google_live_page_items WHERE account=?1 AND lane=?2 AND generation=?3 AND expected_revision=?4 AND applied_revision IS NOT NULL",params![page.account,page.lane.key(),page.generation,page.expected_revision],|r|r.get(0))?;
        if applied != planned_ids(&record.plan).len() {
            return Err(err(
                "google_page_pending",
                "Provider page has unfinished item outcomes",
            ));
        }
        let lane_complete = if page.lane == MailLane::Delta {
            record.plan.caught_up
        } else {
            record.plan.inventory_finished
        };
        tx.execute("UPDATE google_live_lanes SET checkpoint_json=?4,complete=?5 WHERE account=?1 AND lane=?2 AND generation=?3",params![page.account,page.lane.key(),page.generation,serde_json::to_string(&record.plan.next)?,lane_complete])?;
        if page.lane == MailLane::Inventory && lane_complete {
            // A catch-up performed before enumeration ended cannot close offline coverage.
            tx.execute(
                "UPDATE google_live_lanes SET complete=0 WHERE account=?1 AND lane='delta'",
                [&page.account],
            )?;
        }
        if page.lane == MailLane::Delta
            && lane_complete
            && matches!(
                record.plan.expected.phase,
                Phase::Delta { .. } | Phase::CatchUp { .. }
            )
        {
            // Only a completed provider history response, never begin_delta or replay.
            tx.execute("INSERT INTO meta(key,value) VALUES(?1,unixepoch()) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [format!("last_mail_sync:{}", page.account)])?;
        }
        tx.execute("UPDATE google_live_pages SET complete=1 WHERE account=?1 AND lane=?2 AND generation=?3 AND expected_revision=?4",params![page.account,page.lane.key(),page.generation,page.expected_revision])?;
        // Completed pages retain hashes and part receipts for exact replay, but
        // their normalized staging payloads are no longer needed for recovery.
        tx.execute("UPDATE google_live_page_items SET manifest_json=NULL WHERE account=?1 AND lane=?2 AND generation=?3 AND expected_revision=?4 AND applied_revision IS NOT NULL",params![page.account,page.lane.key(),page.generation,page.expected_revision])?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        // A fresh profile's first sync must not keep open-time empty-table plans.
        self.refresh_planner_statistics();
        Ok(record.plan.next)
    }

    /// Returns when a history scan last caught up for the account, in Unix
    /// seconds, or `None` if none has.
    pub fn google_last_mail_sync(&self, account: &str) -> Result<Option<i64>> {
        Ok(self
            .db
            .query_row(
                "SELECT value FROM meta WHERE key=?1",
                [format!("last_mail_sync:{account}")],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Summarises how much of the account's Gmail is held locally.
    ///
    /// Works while the account is disconnected or awaiting reauthorization;
    /// history is then never reported current. Fails with
    /// `stale_google_work` if the account has no Google binding.
    pub fn google_live_coverage(&self, account: &str) -> Result<GoogleMailCoverage> {
        let (generation, state,connection_generation): (u64, String,u64) = self
            .db
            .query_row(
                "SELECT mail_generation,connection_state,connection_generation FROM google_accounts WHERE account=?1",
                [account],
                |r| Ok((r.get(0)?, r.get(1)?,r.get(2)?)),
            )
            .optional()?
            .ok_or_else(stale)?;
        let lanes = self.google_live_stored_lanes(account, generation)?;
        let inventory_complete = lanes
            .iter()
            .any(|l| l.lane == MailLane::Inventory && l.complete);
        let completed_delta = lanes
            .iter()
            .find(|l| l.lane == MailLane::Delta && l.complete && l.checkpoint.phase == Phase::Idle);
        // The page that moved the lane to this checkpoint is keyed by the
        // revision before it. History is current only if that page finished
        // under the present connection, not one from before a reconnect.
        let delta_current = if let Some(delta) = completed_delta.filter(|_| state == "connected") {
            self.db.query_row("SELECT EXISTS(SELECT 1 FROM google_live_pages WHERE account=?1 AND lane='delta' AND generation=?2 AND expected_revision=?3 AND connection_generation=?4 AND complete=1)",params![account,generation,delta.checkpoint.revision.checked_sub(1).ok_or_else(stale)?,connection_generation],|r|r.get(0))?
        } else {
            false
        };
        let (observed,retained_sources,indexed,complete_attachments,unavailable):(u64,u64,u64,u64,u64)=self.db.query_row(
            "SELECT observed,retained_sources,indexed,complete_attachments,unavailable FROM google_mail_metrics WHERE account=?1",
            [account],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)),
        ).optional()?.unwrap_or_default();
        let pending: bool = self.db.query_row("SELECT EXISTS(SELECT 1 FROM google_live_pages WHERE account=?1 AND generation=?2 AND complete=0)",params![account,generation],|r|r.get(0))?;
        let count = self.google_mail_count(account)?;
        Ok(GoogleMailCoverage {
            generation,
            inventory_complete,
            delta_current,
            offline_complete: inventory_complete && delta_current && !pending && unavailable == 0,
            observed,
            retained_sources,
            indexed,
            complete_attachments,
            unavailable,
            listed: count.filter(|c| c.1).map(|c| c.0),
            listing: count.filter(|c| !c.1).map(|c| c.0),
        })
    }

    /// Paginated permanent item outcomes for status/retry UI, including accounts
    /// awaiting reauthorization. Retained IDs awaiting reconciliation are exposed
    /// separately by google_live_reconciliation_page.
    ///
    /// Returns up to `limit` (1 to 100) undeleted messages with an issue, in
    /// remote-ID order after `after`.
    pub fn google_live_gaps(
        &self,
        account: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<GoogleMailGap>> {
        crate::id(account)?;
        if limit == 0 || limit > 100 {
            return Err(err("google_part_limit", "Invalid Google gap page size"));
        }
        if let Some(id) = after {
            crate::id(id)?;
        }
        let mut statement=self.db.prepare("SELECT remote_id,issue,source_state='retained' FROM google_mail_coverage WHERE account=?1 AND remote_id>?2 AND issue IS NOT NULL AND deleted=0 ORDER BY remote_id LIMIT ?3")?;
        let rows = statement.query_map(params![account, after.unwrap_or(""), limit], |r| {
            Ok(GoogleMailGap {
                remote_id: r.get(0)?,
                issue: r.get(1)?,
                source_retained: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// After enumeration, individually recheck retained IDs absent from every
    /// current-generation observation. Absence alone neither deletes nor proves
    /// a complete cache. At most one publication part is requested per plan.
    ///
    /// Returns `None` once every stored message has been seen in this
    /// generation. The plan fetches each ID again, so each outcome, including a
    /// deletion, comes from Gmail itself rather than from absence. It builds on
    /// the finished inventory lane's checkpoint and must be begun on that
    /// lane; fails with `stale_google_work` before the inventory has finished.
    pub fn google_live_reconciliation_page(&self, account: &str) -> Result<Option<PagePlan>> {
        let (generation, _) = active_account(&self.db, account)?;
        let lane = self
            .google_live_lanes(account)?
            .into_iter()
            .find(|l| l.lane == MailLane::Inventory && l.complete)
            .ok_or_else(stale)?;
        let mut statement=self.db.prepare("SELECT remote_id FROM google_mail_coverage WHERE account=?1 AND observed_generation<?2 AND deleted=0 ORDER BY observed_generation,remote_id LIMIT ?3")?;
        let ids = statement
            .query_map(params![account, generation, MAX_PART_ITEMS], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<std::result::Result<BTreeSet<_>, _>>()?;
        if ids.is_empty() {
            return Ok(None);
        }
        let expected = lane.checkpoint;
        let next = Checkpoint {
            revision: expected.revision.checked_add(1).ok_or_else(stale)?,
            ..expected.clone()
        };
        Ok(Some(PagePlan {
            expected,
            next,
            observed_message_ids: BTreeSet::new(),
            fetch_message_ids: ids,
            explicitly_deleted_ids: BTreeSet::new(),
            inventory_finished: true,
            caught_up: false,
        }))
    }

    /// An explicit retry of known incomplete items after inventory enumeration.
    /// Permanent gaps are not automatically selected every poll.
    ///
    /// `remote_ids` must be 1 to [`MAX_PART_ITEMS`] distinct IDs, each of an
    /// undeleted message missing its source, index or attachments; otherwise
    /// the call fails with `google_part_limit` or `stale_google_work`. The plan
    /// is begun on the inventory lane like a reconciliation page.
    pub fn google_live_retry_page(&self, account: &str, remote_ids: &[String]) -> Result<PagePlan> {
        let lane = self
            .google_live_lanes(account)?
            .into_iter()
            .find(|l| l.lane == MailLane::Inventory && l.complete)
            .ok_or_else(stale)?;
        let ids: BTreeSet<String> = remote_ids.iter().cloned().collect();
        if ids.is_empty() || ids.len() != remote_ids.len() || ids.len() > MAX_PART_ITEMS {
            return Err(err("google_part_limit", "Invalid Google retry item count"));
        }
        for id in &ids {
            crate::id(id)?;
            let incomplete:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM google_mail_coverage WHERE account=?1 AND remote_id=?2 AND deleted=0 AND (source_state!='retained' OR index_state!='indexed' OR attachment_state!='complete'))",params![account,id],|r|r.get(0))?;
            if !incomplete {
                return Err(stale());
            }
        }
        let expected = lane.checkpoint;
        let next = Checkpoint {
            revision: expected.revision.checked_add(1).ok_or_else(stale)?,
            ..expected.clone()
        };
        Ok(PagePlan {
            expected,
            next,
            observed_message_ids: BTreeSet::new(),
            fetch_message_ids: ids,
            explicitly_deleted_ids: BTreeSet::new(),
            inventory_finished: true,
            caught_up: false,
        })
    }
}

/// Creates the three lanes for `generation`, all from `baseline`. The listing
/// lanes start a full listing; the delta lane starts replaying history from the
/// same baseline straight away, so changes made during the listing are caught.
fn insert_lanes(
    db: &Connection,
    account: &str,
    generation: u64,
    baseline: &str,
    cutoff: i64,
) -> Result<()> {
    db.execute(
        "INSERT OR IGNORE INTO google_mail_metrics(account) VALUES(?1)",
        [account],
    )?;
    let base = Checkpoint::begin_full(
        AccountId::new(account).map_err(|_| stale())?,
        generation,
        baseline.into(),
    )
    .map_err(|_| stale())?;
    for lane in [MailLane::Recent, MailLane::Inventory, MailLane::Delta] {
        let mut checkpoint = base.clone();
        if lane == MailLane::Delta {
            checkpoint.phase = Phase::Delta {
                start_history_id: baseline.into(),
                page_token: None,
            };
        }
        let since: Option<i64> = db.query_row(
            "SELECT mail_since FROM account_metadata WHERE account=?1",
            [account],
            |r| r.get(0),
        )?;
        // Gmail's `after:` takes Unix seconds. Recent mail never reaches back
        // past the retention start; the inventory covers the whole window.
        let query = match lane {
            MailLane::Recent => Some(format!("after:{}", cutoff.max(since.unwrap_or(cutoff)))),
            MailLane::Inventory => since.map(|s| format!("after:{s}")),
            MailLane::Delta => None,
        };
        db.execute("INSERT INTO google_live_lanes(account,lane,generation,checkpoint_json,query_json) VALUES(?1,?2,?3,?4,?5)",params![account,lane.key(),generation,serde_json::to_string(&checkpoint)?,serde_json::to_string(&query)?])?;
    }
    Ok(())
}
/// Every ID the page names; each needs a committed outcome before the page ends.
fn planned_ids(plan: &PagePlan) -> BTreeSet<String> {
    plan.observed_message_ids
        .iter()
        .chain(&plan.fetch_message_ids)
        .chain(&plan.explicitly_deleted_ids)
        .cloned()
        .collect()
}
/// The action `id` needs. Fetch wins over observe, since plans usually list
/// fetched IDs as observed too.
fn action(plan: &PagePlan, id: &str) -> Result<GoogleItemAction> {
    if plan.fetch_message_ids.contains(id) {
        Ok(GoogleItemAction::Fetch)
    } else if plan.explicitly_deleted_ids.contains(id) {
        Ok(GoogleItemAction::Delete)
    } else if plan.observed_message_ids.contains(id) {
        Ok(GoogleItemAction::Observe)
    } else {
        Err(stale())
    }
}
/// Refuses a plan that could not have come from this lane's checkpoint.
fn validate_plan(lane: MailLane, plan: &PagePlan, history: &str) -> Result<()> {
    let observed = number(history)?;
    // The page cannot have been read before the history point it starts from.
    let baseline = match &plan.expected.phase {
        Phase::Full {
            baseline_history_id,
            ..
        } => Some(baseline_history_id),
        Phase::Delta {
            start_history_id, ..
        }
        | Phase::CatchUp {
            start_history_id, ..
        } => Some(start_history_id),
        _ => plan.expected.committed_history_id.as_ref(),
    };
    if baseline.is_some_and(|value| value.parse::<u64>().map_or(true, |value| value > observed)) {
        return Err(stale());
    }
    // A plan advances exactly one revision within one generation. Only the
    // delta lane replays history; the listing lanes plan listing pages, plus
    // the reconciliation and retry pages of a finished inventory.
    let ids = planned_ids(plan);
    if plan.expected.account != plan.next.account
        || plan.expected.generation != plan.next.generation
        || plan.expected.revision.checked_add(1) != Some(plan.next.revision)
        || ids.len() > MAX_PAGE_ITEMS
        || ids.iter().any(|id| crate::id(id).is_err())
        || !plan
            .fetch_message_ids
            .is_disjoint(&plan.explicitly_deleted_ids)
        || (lane == MailLane::Delta && plan.inventory_finished)
        || (lane != MailLane::Delta
            && (!(matches!(plan.expected.phase, Phase::Full { .. })
                || (lane == MailLane::Inventory
                    && matches!(plan.expected.phase, Phase::CatchUp { .. })
                    && plan.inventory_finished))
                || plan.caught_up))
    {
        return Err(stale());
    }
    Ok(())
}
/// Loads the recorded page and whether it is finished, once `page` still
/// matches the account's generations and the stored fingerprint.
fn load_page(db: &Connection, page: &GooglePageIdentity) -> Result<(RecordedPage, bool)> {
    let (generation, revision) = active_account(db, &page.account)?;
    if generation != page.generation || revision != page.connection_generation {
        return Err(stale());
    }
    let (fingerprint,json,complete,epoch):(String,String,bool,u64)=db.query_row("SELECT fingerprint,plan_json,complete,connection_generation FROM google_live_pages WHERE account=?1 AND lane=?2 AND generation=?3 AND expected_revision=?4",params![page.account,page.lane.key(),page.generation,page.expected_revision],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?.ok_or_else(stale)?;
    if fingerprint != page.fingerprint || epoch != page.connection_generation {
        return Err(stale());
    }
    let record: RecordedPage = serde_json::from_str(&json)?;
    if record.observed_history_id != page.observed_history_id {
        return Err(stale());
    }
    Ok((record, complete))
}
/// Requires the lane to still sit at the page's starting checkpoint, so a page
/// cannot write after its lane has moved past it.
fn ensure_checkpoint(
    db: &Connection,
    page: &GooglePageIdentity,
    expected: &Checkpoint,
) -> Result<()> {
    let json:String=db.query_row("SELECT checkpoint_json FROM google_live_lanes WHERE account=?1 AND lane=?2 AND generation=?3",params![page.account,page.lane.key(),page.generation],|r|r.get(0)).optional()?.ok_or_else(stale)?;
    if serde_json::from_str::<Checkpoint>(&json)? != *expected {
        return Err(stale());
    }
    Ok(())
}
/// Requires a prepared item to belong to this profile and this exact page
/// identity, and its outcome to fit the action the plan asked for.
fn validate_prepared(
    profile: &std::path::Path,
    page: &GooglePageIdentity,
    item: &PreparedGoogleItem,
    plan: &PagePlan,
) -> Result<()> {
    // Comparing the whole identity also rejects items prepared under an
    // earlier connection generation of the same page.
    if item.profile != profile || item.page != *page || item.json.len() > MAX_PART_BYTES {
        return Err(stale());
    }
    let required = action(plan, &item.manifest.remote_id)?;
    if (required == GoogleItemAction::Delete
        && item.manifest.disposition != preparation::Disposition::Missing)
        || (required == GoogleItemAction::Observe
            && item.manifest.disposition != preparation::Disposition::Observed)
    {
        return Err(stale());
    }
    Ok(())
}

/// Applies one item's outcome and adjusts the account's coverage counters by
/// the difference it made, which keeps them exact without recounting.
fn publish_item(
    db: &Connection,
    page: &GooglePageIdentity,
    item: &preparation::Manifest,
) -> Result<()> {
    let previous = metric_contribution(db, page, &item.remote_id)?;
    publish_item_inner(db, page, item)?;
    let next = metric_contribution(db, page, &item.remote_id)?;
    db.execute("UPDATE google_mail_metrics SET observed=observed+?2,retained_sources=retained_sources+?3,indexed=indexed+?4,complete_attachments=complete_attachments+?5,unavailable=unavailable+?6 WHERE account=?1",params![page.account,next[0]-previous[0],next[1]-previous[1],next[2]-previous[2],next[3]-previous[3],next[4]-previous[4]])?;
    Ok(())
}

/// What one message adds to each counter: observed, retained source, indexed,
/// complete attachments, unavailable. A deleted or unknown message adds nothing.
fn metric_contribution(
    db: &Connection,
    page: &GooglePageIdentity,
    remote: &str,
) -> Result<[i64; 5]> {
    Ok(db.query_row("SELECT 1,source_state='retained',index_state='indexed',attachment_state='complete',source_state!='retained' OR index_state!='indexed' OR attachment_state!='complete' OR observed_generation!=?3 FROM google_mail_coverage WHERE account=?1 AND remote_id=?2 AND deleted=0",params![page.account,remote,page.generation],|r|Ok([r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?])).optional()?.unwrap_or_default())
}

/// Writes one outcome to the mail tables and the coverage row, unless newer
/// history already covers the message.
fn publish_item_inner(
    db: &Connection,
    page: &GooglePageIdentity,
    item: &preparation::Manifest,
) -> Result<()> {
    use preparation::Disposition;
    let remote = &item.remote_id;
    // Older than the retention window: it counts as handled but is not kept.
    if item.disposition == Disposition::OutsideRetention {
        return Ok(());
    }
    // A page prepared before this rule may still carry a spam message.
    if item.disposition == Disposition::Spam
        || item
            .message
            .as_ref()
            .is_some_and(|message| message.labels.iter().any(|label| label == "SPAM"))
    {
        return forget_spam(db, &page.account, Some(remote));
    }
    let previous: Option<(Option<String>, bool)> = db
        .query_row(
            "SELECT history_id,deleted FROM google_mail_coverage WHERE account=?1 AND remote_id=?2",
            params![page.account, remote],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let incoming = number(
        item.history_id
            .as_deref()
            .unwrap_or(&page.observed_history_id),
    )?;
    // Pages from different lanes can arrive out of history order. An outcome
    // older than the stored one must not overwrite it, and a deletion
    // recorded at the same history ID outranks anything but another deletion.
    let stale_item = previous.as_ref().is_some_and(|(history, deleted)| {
        history
            .as_deref()
            .and_then(|h| h.parse::<u64>().ok())
            .is_some_and(|h| {
                h > incoming
                    || (h == incoming && *deleted && item.disposition != Disposition::Missing)
            })
    });
    let inventory = (page.lane == MailLane::Inventory).then_some(page.generation);
    // An outcome that changes nothing still counts as seeing the message in
    // this generation, so reconciliation does not fetch it again.
    if stale_item || (item.disposition == Disposition::Observed && previous.is_some()) {
        db.execute("UPDATE google_mail_coverage SET observed_generation=?3 WHERE account=?1 AND remote_id=?2",params![page.account,remote,page.generation])?;
        if inventory.is_some() {
            db.execute("UPDATE google_mail_coverage SET last_inventory_generation=?3 WHERE account=?1 AND remote_id=?2",params![page.account,remote,inventory])?;
        }
        return Ok(());
    }
    if let Some(message) = &item.message {
        crate::google_store::publish_message(
            db,
            &page.account,
            message,
            &item.source.as_ref().ok_or_else(stale)?.hash,
        )
        .map_err(|_| {
            err(
                "google_publication_failed",
                "Google message publication failed",
            )
        })?;
    }
    let deleted = item.disposition == Disposition::Missing;
    // A failed response without message metadata has no message history fence.
    // The page's global watermark must not block later repair of an older message.
    let stored_history = item
        .history_id
        .clone()
        .or_else(|| deleted.then(|| page.observed_history_id.clone()));
    // A deletion keeps the local message and its source; it only leaves every
    // mailbox view, and its conversation revision moves so readers refresh.
    if deleted {
        db.execute("UPDATE google_message_state SET deleted=1,history_id=?3 WHERE account=?1 AND message_id IN (SELECT id FROM messages WHERE account=?1 AND remote_id=?2)",params![page.account,remote,incoming.to_string()])?;
        db.execute("UPDATE messages SET inbox=0,sent=0,trashed=0,revision=revision+1 WHERE account=?1 AND remote_id=?2",params![page.account,remote])?;
        db.execute("UPDATE conversations SET revision=revision+1 WHERE account=?1 AND id IN (SELECT conversation_id FROM messages WHERE account=?1 AND remote_id=?2)",params![page.account,remote])?;
    }
    let ready = item.disposition == Disposition::Ready;
    // The upsert keeps earlier values wherever this outcome has none, and a
    // deletion keeps the stored source, index and attachment states.
    let source_state = if item.source.is_some() {
        "retained"
    } else {
        "unavailable"
    };
    db.execute("INSERT INTO google_mail_coverage(account,remote_id,last_inventory_generation,remote_thread_id,history_id,source_state,index_state,attachment_state,source_hash,source_size,issue,internal_date_ms,deleted,observed_generation) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14) ON CONFLICT(account,remote_id) DO UPDATE SET last_inventory_generation=coalesce(excluded.last_inventory_generation,google_mail_coverage.last_inventory_generation),remote_thread_id=coalesce(excluded.remote_thread_id,google_mail_coverage.remote_thread_id),history_id=coalesce(excluded.history_id,google_mail_coverage.history_id),source_state=CASE WHEN excluded.deleted=1 THEN google_mail_coverage.source_state ELSE excluded.source_state END,index_state=CASE WHEN excluded.deleted=1 THEN google_mail_coverage.index_state ELSE excluded.index_state END,attachment_state=CASE WHEN excluded.deleted=1 THEN google_mail_coverage.attachment_state ELSE excluded.attachment_state END,source_hash=coalesce(excluded.source_hash,google_mail_coverage.source_hash),source_size=coalesce(excluded.source_size,google_mail_coverage.source_size),issue=excluded.issue,internal_date_ms=coalesce(excluded.internal_date_ms,google_mail_coverage.internal_date_ms),deleted=excluded.deleted,observed_generation=excluded.observed_generation",params![page.account,remote,inventory,item.remote_thread_id,stored_history,source_state,if ready {"indexed"} else {"unindexed"},if ready {"complete"} else {"incomplete"},item.source.as_ref().map(|s|&s.hash),item.source.as_ref().map(|s|s.size),item.issue.map(|i|i.key()),item.internal_date_ms,deleted,page.generation])?;
    Ok(())
}

/// Drops local copies of Gmail Spam: one remote message, or every stored
/// message whose Gmail labels include SPAM. A message that a draft or a
/// pending change still uses stays until that work is done.
pub(crate) fn forget_spam(db: &Connection, account: &str, remote: Option<&str>) -> Result<()> {
    // `purge_messages` takes its input from this temporary table.
    db.execute_batch("CREATE TEMP TABLE IF NOT EXISTS retention_messages(id TEXT PRIMARY KEY,conversation_id TEXT NOT NULL,remote_id TEXT NOT NULL,source_hash TEXT NOT NULL,source_rowid INTEGER NOT NULL); DELETE FROM retention_messages;")?;
    db.execute("INSERT INTO retention_messages SELECT m.id,m.conversation_id,m.remote_id,m.source_hash,m.rowid FROM messages m JOIN google_message_state g ON g.account=m.account AND g.message_id=m.id WHERE m.account=?1 AND (m.remote_id=?2 OR (?2 IS NULL AND EXISTS(SELECT 1 FROM json_each(g.provider_labels_json) WHERE value='SPAM'))) AND NOT EXISTS(SELECT 1 FROM google_mail_mutations w WHERE w.account=m.account AND w.message_id=m.id AND w.state IN ('pending','in_flight','reconcile')) AND NOT EXISTS(SELECT 1 FROM drafts d WHERE d.account=m.account AND d.source_message_id=m.id AND d.discarded=0)",params![account,remote])?;
    crate::account_retention::purge_messages(db, account)?;
    if let Some(remote) = remote {
        // Never stored, or already gone: it is no longer part of the mailbox.
        db.execute("DELETE FROM google_mail_coverage WHERE account=?1 AND remote_id=?2 AND NOT EXISTS(SELECT 1 FROM messages WHERE account=?1 AND remote_id=?2)",params![account,remote])?;
    }
    db.execute_batch("DROP TABLE retention_messages")?;
    Ok(())
}

impl Store {
    /// Once per profile: remove spam kept by builds that synced it.
    pub(crate) fn forget_stored_spam(&self) -> Result<()> {
        let done: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM meta WHERE key='spam_forgotten')",
            [],
            |r| r.get(0),
        )?;
        if done {
            return Ok(());
        }
        let tx = self.db.unchecked_transaction()?;
        let accounts: Vec<String> = {
            let mut statement = tx.prepare("SELECT DISTINCT account FROM google_message_state WHERE provider_labels_json LIKE '%\"SPAM\"%'")?;
            let rows = statement
                .query_map([], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?;
            rows
        };
        for account in &accounts {
            forget_spam(&tx, account, None)?;
            // The purge bypasses the per-item counter updates, so recount.
            tx.execute("UPDATE google_mail_metrics SET observed=(SELECT count(*) FROM google_mail_coverage WHERE account=?1 AND deleted=0),retained_sources=(SELECT count(*) FROM google_mail_coverage WHERE account=?1 AND deleted=0 AND source_state='retained'),indexed=(SELECT count(*) FROM google_mail_coverage WHERE account=?1 AND deleted=0 AND index_state='indexed'),complete_attachments=(SELECT count(*) FROM google_mail_coverage WHERE account=?1 AND deleted=0 AND attachment_state='complete') WHERE account=?1",[account])?;
        }
        tx.execute("INSERT INTO meta(key,value) VALUES('spam_forgotten',1)", [])?;
        if !accounts.is_empty() {
            tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        }
        tx.commit()?;
        Ok(())
    }
}
