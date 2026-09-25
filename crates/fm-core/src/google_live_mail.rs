//! Live Gmail acquisition: bounded visible parts, durable provider-page completion.
//! Preparation owns filesystem/CPU work. Store commands below perform SQL only.
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

pub const MAX_PART_ITEMS: usize = 10;
pub const MAX_PART_BYTES: usize = 512 * 1024;
const MAX_PAGE_ITEMS: usize = 5000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailLane {
    Recent,
    Inventory,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MailLaneState {
    pub lane: MailLane,
    pub checkpoint: Checkpoint,
    pub query: Option<String>,
    pub complete: bool,
}

/// Immutable page and authorization epoch. A renewed account invalidates old work.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GooglePageIdentity {
    pub account: String,
    pub lane: MailLane,
    pub generation: u64,
    pub expected_revision: u64,
    pub connection_generation: u64,
    pub observed_history_id: String,
    fingerprint: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoogleItemAction {
    Fetch,
    Delete,
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
pub struct PendingGoogleItem {
    pub remote_id: String,
    pub action: GoogleItemAction,
    pub staged: Option<StoredGoogleItem>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GooglePartReceipt {
    pub key: String,
    pub revision: i64,
    pub item_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GoogleMailCoverage {
    pub generation: u64,
    pub inventory_complete: bool,
    pub delta_current: bool,
    pub offline_complete: bool,
    pub observed: u64,
    pub retained_sources: u64,
    pub indexed: u64,
    pub complete_attachments: u64,
    pub unavailable: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GoogleMailGap {
    pub remote_id: String,
    pub issue: String,
    pub source_retained: bool,
}

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

impl Store {
    pub fn google_mail_since(&self, account: &str) -> Result<Option<i64>> {
        Ok(self.db.query_row(
            "SELECT mail_since FROM account_metadata WHERE account=?1",
            [account],
            |r| r.get(0),
        )?)
    }
    /// Start all lanes from a baseline captured before listing. Reopening preserves
    /// the stored cutoff and cursors, regardless of newly supplied defaults.
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

    pub fn google_live_lanes(&self, account: &str) -> Result<Vec<MailLaneState>> {
        let (generation, _) = active_account(&self.db, account)?;
        self.google_live_stored_lanes(account, generation)
    }

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
        let key = hash(serde_json::to_vec(&(&page.fingerprint, digests))?.as_slice());
        let tx = self.db.unchecked_transaction()?;
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
        })
    }

    /// Paginated permanent item outcomes for status/retry UI, including accounts
    /// awaiting reauthorization. Retained IDs awaiting reconciliation are exposed
    /// separately by google_live_reconciliation_page.
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
        let query = match lane {
            MailLane::Recent => Some(format!("after:{}", cutoff.max(since.unwrap_or(cutoff)))),
            MailLane::Inventory => since.map(|s| format!("after:{s}")),
            MailLane::Delta => None,
        };
        db.execute("INSERT INTO google_live_lanes(account,lane,generation,checkpoint_json,query_json) VALUES(?1,?2,?3,?4,?5)",params![account,lane.key(),generation,serde_json::to_string(&checkpoint)?,serde_json::to_string(&query)?])?;
    }
    Ok(())
}
fn planned_ids(plan: &PagePlan) -> BTreeSet<String> {
    plan.observed_message_ids
        .iter()
        .chain(&plan.fetch_message_ids)
        .chain(&plan.explicitly_deleted_ids)
        .cloned()
        .collect()
}
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
fn validate_plan(lane: MailLane, plan: &PagePlan, history: &str) -> Result<()> {
    let observed = number(history)?;
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
fn validate_prepared(
    profile: &std::path::Path,
    page: &GooglePageIdentity,
    item: &PreparedGoogleItem,
    plan: &PagePlan,
) -> Result<()> {
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

fn metric_contribution(
    db: &Connection,
    page: &GooglePageIdentity,
    remote: &str,
) -> Result<[i64; 5]> {
    Ok(db.query_row("SELECT 1,source_state='retained',index_state='indexed',attachment_state='complete',source_state!='retained' OR index_state!='indexed' OR attachment_state!='complete' OR observed_generation!=?3 FROM google_mail_coverage WHERE account=?1 AND remote_id=?2 AND deleted=0",params![page.account,remote,page.generation],|r|Ok([r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?])).optional()?.unwrap_or_default())
}

fn publish_item_inner(
    db: &Connection,
    page: &GooglePageIdentity,
    item: &preparation::Manifest,
) -> Result<()> {
    use preparation::Disposition;
    let remote = &item.remote_id;
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
    if deleted {
        db.execute("UPDATE google_message_state SET deleted=1,history_id=?3 WHERE account=?1 AND message_id IN (SELECT id FROM messages WHERE account=?1 AND remote_id=?2)",params![page.account,remote,incoming.to_string()])?;
        db.execute("UPDATE messages SET inbox=0,sent=0,trashed=0,revision=revision+1 WHERE account=?1 AND remote_id=?2",params![page.account,remote])?;
        db.execute("UPDATE conversations SET revision=revision+1 WHERE account=?1 AND id IN (SELECT conversation_id FROM messages WHERE account=?1 AND remote_id=?2)",params![page.account,remote])?;
    }
    let ready = item.disposition == Disposition::Ready;
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
