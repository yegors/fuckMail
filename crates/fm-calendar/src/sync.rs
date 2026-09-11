//! A staged provider batch. Core atomically persists CompletedBatch and its cursor.
use crate::{error, EventKey, Result};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncScope {
    pub account: String,
    pub calendar: String,
    pub query_fingerprint: String,
}
#[derive(Debug, Clone)]
pub struct CompletedBatch {
    pub scope: SyncScope,
    pub replacements: BTreeMap<String, Value>,
    pub next_sync_token: String,
    pub full_replace: bool,
}
#[derive(Debug)]
pub enum PageResult {
    NextPage(String),
    Complete(CompletedBatch),
}
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
            return Err(error(
                "limit_exceeded",
                "Synthetic sync batch limit exceeded",
            ));
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
    pub fn target(&self, event: &str) -> EventKey {
        EventKey {
            account: self.scope.account.clone(),
            calendar: self.scope.calendar.clone(),
            event: event.into(),
        }
    }
}
