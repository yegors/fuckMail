//! A staged provider batch. Core atomically persists CompletedBatch and its cursor.
use crate::{error, EventKey, Result};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// What one sync run covers. A sync token is only valid for the same scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncScope {
    pub account: String,
    pub calendar: String,
    /// Names the exact list query, such as
    /// [`EVENT_SYNC_FINGERPRINT`](crate::google_api::EVENT_SYNC_FINGERPRINT).
    pub query_fingerprint: String,
}
/// Every page of one sync run, ready to commit together with its token.
#[derive(Debug, Clone)]
pub struct CompletedBatch {
    pub scope: SyncScope,
    /// Latest source per event ID across all pages, cancelled tombstones included.
    pub replacements: BTreeMap<String, Value>,
    /// Token for the next incremental run.
    pub next_sync_token: String,
    /// The run started without a sync token, so `replacements` is the whole
    /// calendar and anything absent from it is gone.
    pub full_replace: bool,
}
/// What [`SyncBatch::apply_page`] needs next.
#[derive(Debug)]
pub enum PageResult {
    /// Request this page token next.
    NextPage(String),
    /// The last page arrived; commit the batch.
    Complete(CompletedBatch),
}
/// Stages the pages of one Calendar sync run and releases them only once
/// complete, so a partial run never replaces stored events.
#[derive(Debug)]
pub struct SyncBatch {
    scope: SyncScope,
    full_replace: bool,
    staged: BTreeMap<String, Value>,
    expected_page: Option<String>,
    seen_pages: BTreeSet<String>,
    complete: bool,
}
impl SyncBatch {
    /// Starts a run. Without a stored sync token it is a full replacement.
    /// Fails with `invalid_scope` when any scope field is empty.
    pub fn new(scope: SyncScope, has_sync_token: bool) -> Result<Self> {
        if scope.account.is_empty()
            || scope.calendar.is_empty()
            || scope.query_fingerprint.is_empty()
        {
            return Err(error(
                "invalid_scope",
                "A sync batch needs a stable account/calendar/query scope",
            ));
        }
        Ok(Self {
            scope,
            full_replace: !has_sync_token,
            staged: BTreeMap::new(),
            expected_page: None,
            seen_pages: BTreeSet::new(),
            complete: false,
        })
    }
    /// Stages one page. `page_token` is the token the page was requested with:
    /// `None` for the first page, then the last [`PageResult::NextPage`] value.
    ///
    /// A later version of an event replaces an earlier one. Fails with
    /// `scope_mismatch` for another scope, an out-of-order page or a completed
    /// batch; `invalid_page` unless exactly one of `next_page` and `next_sync`
    /// is set, or for a missing ID or an empty or repeated cursor; and
    /// `limit_exceeded` past 2,500 items per page, 10,000 staged items or about
    /// 100 pages.
    pub fn apply_page(
        &mut self,
        scope: &SyncScope,
        page_token: Option<&str>,
        items: Vec<Value>,
        next_page: Option<String>,
        next_sync: Option<String>,
    ) -> Result<PageResult> {
        if self.complete || scope != &self.scope || page_token != self.expected_page.as_deref() {
            return Err(error(
                "scope_mismatch",
                "Unexpected scope, page token or completed batch",
            ));
        }
        if next_page.is_some() == next_sync.is_some() {
            return Err(error(
                "invalid_page",
                "A page has exactly one next-page or final sync token",
            ));
        }
        if items.len() > 2500
            || self.staged.len() + items.len() > 10000
            || self.seen_pages.len() > 100
        {
            return Err(error("limit_exceeded", "Sync batch limit exceeded"));
        }
        let mut incoming = Vec::new();
        for item in items {
            let id = item["id"]
                .as_str()
                .filter(|v| !v.is_empty())
                .ok_or_else(|| {
                    error(
                        "invalid_page",
                        "Every event/tombstone needs its provider ID",
                    )
                })?
                .to_owned();
            incoming.push((id, item));
        }
        // A repeated cursor means the pages are cycling; refuse it rather than loop.
        if let Some(next) = next_page {
            if next.is_empty() || self.seen_pages.contains(&next) {
                return Err(error("invalid_page", "Repeated/empty pagination cursor"));
            }
            self.seen_pages.insert(next.clone());
            self.expected_page = Some(next.clone());
            self.staged.extend(incoming);
            Ok(PageResult::NextPage(next))
        } else {
            let token = next_sync.unwrap();
            if token.is_empty() {
                return Err(error("invalid_page", "Empty final sync token"));
            }
            self.staged.extend(incoming);
            self.complete = true;
            Ok(PageResult::Complete(CompletedBatch {
                scope: self.scope.clone(),
                replacements: std::mem::take(&mut self.staged),
                next_sync_token: token,
                full_replace: self.full_replace,
            }))
        }
    }
    /// A 410 discards the staged delta and requests a full replacement. Existing
    /// visible Core data and durable user intent remain until that full batch commits.
    pub fn expired_token(self) -> Result<Self> {
        Self::new(self.scope, false)
    }
    /// Returns the key of `event` within this batch's account and calendar.
    pub fn target(&self, event: &str) -> EventKey {
        EventKey {
            account: self.scope.account.clone(),
            calendar: self.scope.calendar.clone(),
            event: event.into(),
        }
    }
}
