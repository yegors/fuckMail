//! Read-only Google draft inventory. RAW and part preparation never hold Store.
use crate::{content::BlobStore, err, Result, Store};
use fm_google::transport::CancellationToken;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::PathBuf};

pub mod actions;
mod preparation;
pub use preparation::{export_google_draft_source, prepare_google_draft_import};
#[cfg(test)]
mod tests;

pub const MAX_INVENTORY_PAGE: usize = 50;
pub const MAX_IMPORT_RAW_BYTES: usize = fm_google::gmail_api::MAX_RAW_MIME;
// Each page ID probes two partial indexes of current sources. CROSS JOIN
// preserves that bounded outer loop even when an account has long history.
const SOURCE_PAGE_SQL: &str = "SELECT s.source_revision,s.draft_id,s.linked_local_draft_id,s.observed_at,s.metadata_json,s.seen_generation,s.last_observed_at FROM json_each(?2) ids CROSS JOIN google_draft_sources s INDEXED BY google_draft_current_owner WHERE s.account=?1 AND s.draft_id=ids.value AND s.is_current=1 UNION SELECT s.source_revision,s.draft_id,s.linked_local_draft_id,s.observed_at,s.metadata_json,s.seen_generation,s.last_observed_at FROM json_each(?2) ids CROSS JOIN google_draft_sources s INDEXED BY google_draft_current_link WHERE s.account=?1 AND s.linked_local_draft_id=ids.value AND s.is_current=1 ORDER BY source_revision DESC";

#[derive(Clone)]
pub struct GoogleDraftReadScope {
    pub account: String,
    pub connection_generation: i64,
}

pub struct GoogleDraftInventoryPage {
    pub drafts: Vec<GoogleDraftListedId>,
    pub next_page_token: Option<String>,
    pub result_size_estimate: Option<u64>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct GoogleDraftListedId {
    pub draft_id: String,
    pub message_id: String,
}

pub enum GoogleDraftInventoryStep {
    List(GoogleDraftListPlan),
    Fetch(GoogleDraftImportPlan),
    Complete(Value),
}
#[derive(Clone)]
pub struct GoogleDraftListPlan {
    scope: GoogleDraftReadScope,
    profile: PathBuf,
    generation: i64,
    revision: i64,
    token: Option<String>,
}
impl GoogleDraftListPlan {
    pub fn page_token(&self) -> Option<&str> {
        self.token.as_deref()
    }
    pub fn max_results(&self) -> u16 {
        MAX_INVENTORY_PAGE as u16
    }
}

pub struct GoogleDraftImportPlan {
    scope: GoogleDraftReadScope,
    blobs: BlobStore,
    generation: i64,
    page_revision: i64,
    item: GoogleDraftListedId,
}
impl GoogleDraftImportPlan {
    pub fn provider_draft_id(&self) -> &str {
        &self.item.draft_id
    }
    pub fn listed_message_id(&self) -> &str {
        &self.item.message_id
    }
}
pub enum GoogleDraftImportOutcome {
    Available {
        draft_id: String,
        message_id: String,
        thread_id: String,
        raw: Vec<u8>,
    },
    Missing,
    Unavailable {
        code: String,
    },
}
pub struct PreparedGoogleDraftImport {
    plan: GoogleDraftImportPlan,
    source: Source,
    preview: Option<Preview>,
    parts: Vec<Part>,
    cancel: CancellationToken,
}

pub struct GoogleDraftSourcePlan {
    blobs: BlobStore,
    account: String,
    draft_id: String,
    source_revision: i64,
    hash: String,
    size: u64,
    name: String,
    mime_type: String,
    attachment_id: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct PendingItem {
    id: GoogleDraftListedId,
    done: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct PendingPage {
    revision: i64,
    items: Vec<PendingItem>,
    next: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Inventory {
    generation: i64,
    revision: i64,
    connection_generation: i64,
    token: Option<String>,
    pending: Option<PendingPage>,
    complete: bool,
    paused: Option<String>,
    expired_restarts: u32,
    estimate: Option<u64>,
    observed_items: i64,
    observed_this_scan: i64,
    source_retained: i64,
    source_unavailable: i64,
    preview_incomplete: i64,
    started_at: i64,
    completed_at: Option<i64>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
struct Source {
    provider_draft_id: String,
    message_id: Option<String>,
    thread_id: Option<String>,
    sha256: Option<String>,
    size: Option<u64>,
    source_state: String,
    preview_state: String,
    issues: Vec<String>,
    part_manifest_sha256: Option<String>,
    part_manifest_size: Option<u64>,
    attachment_ids: Vec<String>,
    possible_unknown_create: bool,
    candidate_request_ids: Vec<String>,
    raw_headers: Value,
}
struct Preview {
    routing: Value,
    body: String,
    html: Option<String>,
    from_name: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Part {
    id: String,
    name: String,
    mime_type: String,
    size: u64,
    sha256: String,
    content_id: Option<String>,
    disposition: Option<String>,
    ordinal: usize,
}

fn invalid() -> crate::CoreError {
    err("invalid_request", "Invalid Google draft inventory request")
}
fn stale() -> crate::CoreError {
    err(
        "stale_google_work",
        "Remote draft inventory or account connection changed",
    )
}
fn now() -> Result<i64> {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| invalid())?
            .as_millis(),
    )
    .map_err(|_| invalid())
}
fn cancelled(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        Err(err("cancelled", "Remote draft work stopped"))
    } else {
        Ok(())
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn identifier(value: &str) -> Result<()> {
    crate::id(value).map(|_| ())
}
fn encode<T: Serialize>(value: &T) -> Result<String> {
    let text = serde_json::to_string(value)?;
    if text.len() > 65536 {
        return Err(err(
            "limit_exceeded",
            "Remote draft metadata exceeds its bound",
        ));
    }
    Ok(text)
}

impl Store {
    fn inventory_scope(&self, scope: &GoogleDraftReadScope) -> Result<()> {
        identifier(&scope.account)?;
        let row: Option<(i64,String,String)> = self.db.query_row(
            "SELECT connection_generation,connection_state,scopes_json FROM google_accounts WHERE account=?1",
            [&scope.account], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let Some((generation, state, scopes)) = row else {
            return Err(stale());
        };
        let scopes: BTreeSet<String> = serde_json::from_str(&scopes)?;
        if generation != scope.connection_generation
            || state != "connected"
            || !fm_google::auth::ScopeSet::new(&scopes).mail_read()
        {
            return Err(stale());
        }
        Ok(())
    }
    fn inventory(&self, account: &str) -> Result<Option<Inventory>> {
        let text: Option<String> = self
            .db
            .query_row(
                "SELECT state_json FROM google_draft_inventory WHERE account=?1",
                [account],
                |r| r.get(0),
            )
            .optional()?;
        text.map(|s| serde_json::from_str(&s).map_err(Into::into))
            .transpose()
    }
    fn save_inventory(&self, account: &str, state: &Inventory) -> Result<()> {
        self.db.execute("INSERT INTO google_draft_inventory(account,state_json) VALUES(?1,?2) ON CONFLICT(account) DO UPDATE SET state_json=excluded.state_json",params![account,encode(state)?])?;
        self.db.execute(
            "UPDATE meta SET value=CAST(value AS INTEGER)+1 WHERE key='revision'",
            [],
        )?;
        Ok(())
    }
    pub fn google_draft_inventory_begin(&mut self, scope: &GoogleDraftReadScope) -> Result<Value> {
        self.inventory_scope(scope)?;
        let old = self.inventory(&scope.account)?;
        let state =
            if let Some(mut state) = old.clone().filter(|s| !s.complete && s.paused.is_none()) {
                if state.connection_generation != scope.connection_generation {
                    state.connection_generation = scope.connection_generation;
                    state.revision += 1;
                }
                state
            } else {
                Inventory {
                    generation: old.as_ref().map_or(1, |s| s.generation + 1),
                    revision: old.as_ref().map_or(1, |s| s.revision + 1),
                    connection_generation: scope.connection_generation,
                    token: None,
                    pending: None,
                    complete: false,
                    paused: None,
                    expired_restarts: 0,
                    estimate: None,
                    observed_items: old.as_ref().map_or(0, |s| s.observed_items),
                    observed_this_scan: 0,
                    source_retained: old.as_ref().map_or(0, |s| s.source_retained),
                    source_unavailable: old.as_ref().map_or(0, |s| s.source_unavailable),
                    preview_incomplete: old.as_ref().map_or(0, |s| s.preview_incomplete),
                    started_at: now()?,
                    completed_at: None,
                }
            };
        self.save_inventory(&scope.account, &state)?;
        self.google_draft_inventory_status(&scope.account)
    }
    pub fn google_draft_inventory_next(
        &self,
        scope: &GoogleDraftReadScope,
    ) -> Result<GoogleDraftInventoryStep> {
        self.inventory_scope(scope)?;
        let Some(state) = self.inventory(&scope.account)? else {
            return Ok(GoogleDraftInventoryStep::Complete(
                self.google_draft_inventory_status(&scope.account)?,
            ));
        };
        if state.connection_generation != scope.connection_generation {
            return Err(stale());
        }
        if state.complete || state.paused.is_some() {
            return Ok(GoogleDraftInventoryStep::Complete(
                self.google_draft_inventory_status(&scope.account)?,
            ));
        }
        if let Some(page) = &state.pending {
            let item = page.items.iter().find(|i| !i.done).ok_or_else(stale)?;
            return Ok(GoogleDraftInventoryStep::Fetch(GoogleDraftImportPlan {
                scope: scope.clone(),
                blobs: self.google_blob_store(),
                generation: state.generation,
                page_revision: page.revision,
                item: item.id.clone(),
            }));
        }
        Ok(GoogleDraftInventoryStep::List(GoogleDraftListPlan {
            scope: scope.clone(),
            profile: self.profile.clone(),
            generation: state.generation,
            revision: state.revision,
            token: state.token,
        }))
    }
    fn check_list(&self, plan: &GoogleDraftListPlan) -> Result<Inventory> {
        self.inventory_scope(&plan.scope)?;
        let state = self.inventory(&plan.scope.account)?.ok_or_else(stale)?;
        if plan.profile != self.profile
            || state.generation != plan.generation
            || state.revision != plan.revision
            || state.token != plan.token
            || state.pending.is_some()
            || state.complete
            || state.paused.is_some()
            || state.connection_generation != plan.scope.connection_generation
        {
            return Err(stale());
        }
        Ok(state)
    }
    pub fn google_draft_inventory_list_finish(
        &mut self,
        plan: GoogleDraftListPlan,
        page: GoogleDraftInventoryPage,
    ) -> Result<Value> {
        let mut state = self.check_list(&plan)?;
        if page.drafts.len() > MAX_INVENTORY_PAGE
            || page.next_page_token.as_ref().is_some_and(|s| {
                s.is_empty()
                    || s.len() > 4096
                    || s.chars().any(char::is_control)
                    || Some(s) == plan.token.as_ref()
            })
        {
            return Err(invalid());
        }
        let mut seen = BTreeSet::new();
        let mut items = Vec::new();
        for id in page.drafts {
            identifier(&id.draft_id)?;
            identifier(&id.message_id)?;
            if seen.insert(id.draft_id.clone()) {
                items.push(PendingItem { id, done: false });
            }
        }
        state.revision += 1;
        state.estimate = page.result_size_estimate;
        if items.is_empty() {
            state.token = page.next_page_token;
            state.complete = state.token.is_none();
            if state.complete {
                state.completed_at = Some(now()?);
            }
        } else {
            state.pending = Some(PendingPage {
                revision: state.revision,
                items,
                next: page.next_page_token,
            });
        }
        self.save_inventory(&plan.scope.account, &state)?;
        self.google_draft_inventory_status(&plan.scope.account)
    }
    pub fn google_draft_inventory_list_expired(
        &mut self,
        plan: GoogleDraftListPlan,
    ) -> Result<Value> {
        let mut state = self.check_list(&plan)?;
        if plan.token.is_none() {
            return Err(invalid());
        }
        state.revision += 1;
        state.expired_restarts += 1;
        if state.expired_restarts > 3 {
            state.paused = Some("expired_page_token".into());
        } else {
            state.generation += 1;
            state.token = None;
            state.estimate = None;
            state.observed_this_scan = 0;
            state.started_at = now()?;
        }
        self.save_inventory(&plan.scope.account, &state)?;
        self.google_draft_inventory_status(&plan.scope.account)
    }
    pub fn google_draft_inventory_status(&self, account: &str) -> Result<Value> {
        identifier(account)?;
        let state = self.inventory(account)?;
        Ok(
            json!({"account":account,"generation":state.as_ref().map_or(0,|s|s.generation),
            "revision":state.as_ref().map_or(0,|s|s.revision),"enumeration_complete":state.as_ref().is_some_and(|s|s.complete),
            "paused":state.as_ref().and_then(|s|s.paused.clone()),"connection_generation":state.as_ref().map(|s|s.connection_generation),
            "pending_items":state.as_ref().and_then(|s|s.pending.as_ref()).map_or(0,|p|p.items.iter().filter(|i|!i.done).count()),
            "estimated_total":state.as_ref().and_then(|s|s.estimate),"observed_items":state.as_ref().map_or(0,|s|s.observed_items),"observed_this_scan":state.as_ref().map_or(0,|s|s.observed_this_scan),"source_retained":state.as_ref().map_or(0,|s|s.source_retained),
            "source_unavailable":state.as_ref().map_or(0,|s|s.source_unavailable),"preview_incomplete":state.as_ref().map_or(0,|s|s.preview_incomplete),"started_at":state.as_ref().map(|s|s.started_at),
            "completed_at":state.as_ref().and_then(|s|s.completed_at),"absence_means_sent":false}),
        )
    }

    /// Persisted provenance is authoritative; a client cannot opt into writing.
    pub(crate) fn check_draft_writable(&self, account: &str, p: &Value) -> Result<()> {
        let (id, legacy) = crate::composition::selector(p)?;
        let origin:Option<String>=self.db.query_row("SELECT origin FROM drafts WHERE account=?1 AND ((?3 AND legacy_reply=1 AND conversation_id=?2) OR (NOT ?3 AND draft_id=?2))",params![account,id,legacy],|r|r.get(0)).optional()?;
        if origin.as_deref().is_some_and(|s| s != "local") {
            return Err(err(
                "draft_read_only",
                "This retained remote draft is read-only; adoption is not available",
            ));
        }
        Ok(())
    }
    fn known_draft_owner(&self, account: &str, remote: &str) -> Result<(Option<String>, bool)> {
        let mut stmt=self.db.prepare("SELECT draft_id FROM drafts WHERE account=?1 AND origin='local' AND json_extract(google_json,'$.baseline.draft_id')=?2 UNION SELECT draft_id FROM drafts WHERE account=?1 AND origin='local' AND json_extract(google_json,'$.adoption.target.provider_draft_id')=?2 UNION SELECT json_extract(json,'$.draft_id') FROM capture_operations WHERE account=?1 AND execution_kind='google' AND COALESCE(json_extract(json,'$.kind'),'draft')='draft' AND json_extract(json,'$.target')=?2 UNION SELECT json_extract(json,'$.draft_id') FROM capture_operations WHERE account=?1 AND execution_kind='google' AND json_extract(json,'$.ack.draft_id')=?2 UNION SELECT json_extract(json,'$.draft_id') FROM capture_operations WHERE account=?1 AND execution_kind='google' AND json_extract(json,'$.kind')='send' AND json_extract(json,'$.provider_draft_id')=?2 LIMIT 2")?;
        let rows = stmt
            .query_map(params![account, remote], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok((
            if rows.len() == 1 {
                rows.first().cloned()
            } else {
                None
            },
            rows.len() > 1,
        ))
    }
    pub fn google_draft_import_finish(
        &mut self,
        proof: PreparedGoogleDraftImport,
    ) -> Result<Value> {
        cancelled(&proof.cancel)?;
        let plan = proof.plan;
        if plan.blobs.profile != self.profile {
            return Err(stale());
        }
        self.inventory_scope(&plan.scope)?;
        let mut state = self.inventory(&plan.scope.account)?.ok_or_else(stale)?;
        if state.generation != plan.generation
            || state.connection_generation != plan.scope.connection_generation
        {
            return Err(stale());
        }
        let page = state
            .pending
            .as_mut()
            .filter(|p| p.revision == plan.page_revision)
            .ok_or_else(stale)?;
        let item = page
            .items
            .iter_mut()
            .find(|i| i.id.draft_id == plan.item.draft_id)
            .ok_or_else(stale)?;
        if item.done {
            return self.google_draft_inventory_status(&plan.scope.account);
        }
        let account = &plan.scope.account;
        let tx = self.db.unchecked_transaction()?;
        self.publish_draft_source(
            account,
            &plan.item.draft_id,
            proof.source,
            proof.preview,
            proof.parts,
            &mut state,
        )?;
        let page = state.pending.as_mut().ok_or_else(stale)?;
        let item = page
            .items
            .iter_mut()
            .find(|i| i.id.draft_id == plan.item.draft_id)
            .ok_or_else(stale)?;
        item.done = true;
        if page.items.iter().all(|i| i.done) {
            state.token = page.next.clone();
            state.pending = None;
            state.complete = state.token.is_none();
            if state.complete {
                state.completed_at = Some(now()?);
            }
        }
        state.revision += 1;
        self.save_inventory(account, &state)?;
        tx.commit()?;
        self.google_draft_inventory_status(account)
    }

    fn publish_draft_source(
        &self,
        account: &str,
        remote: &str,
        mut source: Source,
        preview: Option<Preview>,
        parts: Vec<Part>,
        state: &mut Inventory,
    ) -> Result<()> {
        let (owner, ambiguous) = self.known_draft_owner(account, remote)?;
        if ambiguous {
            source.issues.push("ambiguous_local_target".into());
        }
        let uncertain: Vec<(String, String)> = {
            let mut stmt=self.db.prepare("SELECT id,blob_hash FROM capture_operations WHERE account=?1 AND execution_kind='google' AND COALESCE(json_extract(json,'$.kind'),'draft')='draft' AND json_extract(json,'$.state')='acceptance_unknown' AND json_extract(json,'$.target') IS NULL AND json_extract(json,'$.ack') IS NULL ORDER BY id LIMIT 17")?;
            let rows = stmt
                .query_map([account], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        source.possible_unknown_create = !uncertain.is_empty();
        source.candidate_request_ids = uncertain
            .into_iter()
            .filter(|(_, h)| Some(h) == source.sha256.as_ref())
            .take(16)
            .map(|(id, _)| id)
            .collect();
        let old:Option<(String,Option<String>,String,i64)>=self.db.query_row("SELECT draft_id,linked_local_draft_id,metadata_json,seen_generation FROM google_draft_sources WHERE account=?1 AND provider_draft_id=?2 AND is_current=1",params![account,remote],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        let draft_id = old
            .as_ref()
            .map(|r| r.0.clone())
            .or_else(|| owner.clone())
            .unwrap_or_else(|| {
                format!(
                    "google-draft-{}",
                    hash(format!("{account}\0{remote}").as_bytes())
                )
            });
        let linked = owner
            .as_ref()
            .filter(|id| **id != draft_id)
            .cloned()
            .or_else(|| old.as_ref().and_then(|r| r.1.clone()));
        let raw = encode(&source)?;
        let changed = old.as_ref().is_none_or(|r| r.2 != raw || r.1 != linked);
        let old_source: Option<Source> = old
            .as_ref()
            .map(|r| serde_json::from_str(&r.2))
            .transpose()?;
        state.observed_items += i64::from(old_source.is_none());
        state.observed_this_scan += i64::from(old.as_ref().is_none_or(|r| r.3 != state.generation));
        state.source_retained += i64::from(source.source_state == "available")
            - i64::from(
                old_source
                    .as_ref()
                    .is_some_and(|s| s.source_state == "available"),
            );
        state.source_unavailable += i64::from(source.source_state != "available")
            - i64::from(
                old_source
                    .as_ref()
                    .is_some_and(|s| s.source_state != "available"),
            );
        state.preview_incomplete += i64::from(source.preview_state != "complete")
            - i64::from(
                old_source
                    .as_ref()
                    .is_some_and(|s| s.preview_state != "complete"),
            );
        let tx = &self.db;
        if !tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM drafts WHERE account=?1 AND draft_id=?2)",
            params![account, draft_id],
            |r| r.get::<_, bool>(0),
        )? {
            let empty = json!({"from":"","subject":"","to_recipients":[],"cc_recipients":[],"bcc_recipients":[],"references":[],"reply_to_message_id":null,"reply_all_available":false,"source_unavailable":true});
            tx.execute("INSERT INTO drafts(account,draft_id,mode,routing_json,body,revision,origin,created_at,updated_at) VALUES(?1,?2,'new',?3,'',1,'google_snapshot',?4,?4)",params![account,draft_id,serde_json::to_string(&empty)?,now()?/1000])?;
        }
        if changed {
            for part in &parts {
                tx.execute("INSERT OR IGNORE INTO attachments(account,id,message_id,name,mime_type,size,state,blob_hash) VALUES(?1,?2,NULL,?3,?4,?5,'available',?6)",params![account,part.id,part.name,part.mime_type,part.size,part.sha256])?;
            }
            let revision:i64=tx.query_row("SELECT COALESCE(MAX(source_revision),0)+1 FROM google_draft_sources WHERE account=?1",[account],|r|r.get(0))?;
            if let Some(link) = &linked {
                tx.execute("UPDATE google_draft_sources SET linked_local_draft_id=?3 WHERE account=?1 AND provider_draft_id=?2",params![account,remote,link])?;
            }
            tx.execute("UPDATE google_draft_sources SET is_current=0 WHERE account=?1 AND provider_draft_id=?2 AND is_current=1",params![account,remote])?;
            tx.execute("INSERT INTO google_draft_sources(account,source_revision,provider_draft_id,draft_id,linked_local_draft_id,is_current,seen_generation,observed_at,last_observed_at,metadata_json) VALUES(?1,?2,?3,?4,?5,1,?6,?7,?7,?8)",params![account,revision,remote,draft_id,linked,state.generation,now()?,raw])?;
            if let Some(preview) = preview {
                tx.execute("UPDATE drafts SET routing_json=?3,body=?4,body_html=?5,from_name=?6,attachments_json=?7,revision=revision+1,updated_at=?8 WHERE account=?1 AND draft_id=?2 AND origin='google_snapshot'",params![account,draft_id,serde_json::to_string(&preview.routing)?,preview.body,preview.html,preview.from_name,serde_json::to_string(&source.attachment_ids)?,now()?/1000])?;
            }
        } else {
            tx.execute("UPDATE google_draft_sources SET seen_generation=?3,last_observed_at=?4 WHERE account=?1 AND provider_draft_id=?2 AND is_current=1",params![account,remote,state.generation,now()?])?;
        }
        Ok(())
    }

    pub(crate) fn remote_draft_sources(
        &self,
        account: &str,
        ids: &[String],
    ) -> Result<std::collections::BTreeMap<String, Value>> {
        let state = self.inventory(account)?;
        let mut stmt = self.db.prepare(SOURCE_PAGE_SQL)?;
        let mut rows = stmt.query(params![account, serde_json::to_string(ids)?])?;
        let wanted: BTreeSet<&str> = ids.iter().map(String::as_str).collect();
        let mut out = std::collections::BTreeMap::new();
        while let Some(row) = rows.next()? {
            let draft: String = row.get(1)?;
            let linked: Option<String> = row.get(2)?;
            let mut value = source_value(
                row.get(0)?,
                row.get(3)?,
                linked.clone(),
                &row.get::<_, String>(4)?,
            )?;
            source_freshness(&mut value, row.get(5)?, row.get(6)?, state.as_ref());
            for id in std::iter::once(Some(draft))
                .chain(std::iter::once(linked))
                .flatten()
            {
                if wanted.contains(id.as_str()) {
                    out.entry(id).or_insert_with(|| value.clone());
                }
            }
            if out.len() == wanted.len() {
                break;
            }
        }
        Ok(out)
    }
    pub(crate) fn remote_draft_source(&self, account: &str, id: &str) -> Result<Value> {
        Ok(self
            .remote_draft_sources(account, &[id.into()])?
            .remove(id)
            .unwrap_or(Value::Null))
    }
    pub fn google_draft_source_history(&self, account: &str, p: &Value) -> Result<Value> {
        crate::fields(p, &["draft_id", "limit", "offset"])?;
        let id = crate::string(p, "draft_id")?;
        identifier(account)?;
        identifier(id)?;
        let state = self.inventory(account)?;
        let limit = page_number(p, "limit", 30, 1, 100)?;
        let offset = page_number(p, "offset", 0, 0, 10_000_000)?;
        let deletion_history = self.google_delete_history(account, id, limit, offset)?;
        let total:i64=self.db.query_row("SELECT COUNT(*) FROM google_draft_sources WHERE account=?1 AND (draft_id=?2 OR linked_local_draft_id=?2)",params![account,id],|r|r.get(0))?;
        let mut stmt=self.db.prepare("SELECT source_revision,observed_at,linked_local_draft_id,metadata_json,is_current,seen_generation,last_observed_at FROM google_draft_sources WHERE account=?1 AND (draft_id=?2 OR linked_local_draft_id=?2) ORDER BY source_revision DESC LIMIT ?3 OFFSET ?4")?;
        let mut rows = stmt.query(params![account, id, limit, offset])?;
        let mut out = Vec::new();
        let mut bytes = serde_json::to_vec(&deletion_history)?.len() + 1024;
        while let Some(row) = rows.next()? {
            let mut value = source_value(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                &row.get::<_, String>(3)?,
            )?;
            value["is_current"] = json!(row.get::<_, bool>(4)?);
            source_freshness(&mut value, row.get(5)?, row.get(6)?, state.as_ref());
            let size = serde_json::to_vec(&value)?.len();
            if bytes + size > 512 * 1024 {
                break;
            }
            bytes += size;
            out.push(value);
        }
        let next = offset + out.len() as i64;
        let mut reply = json!({"account":account,"draft_id":id,"sources":out,"total":total,"next_offset":if next<total {Some(next)} else {None}});
        for (key, value) in deletion_history.as_object().ok_or_else(invalid)? {
            reply[key] = value.clone();
        }
        Ok(reply)
    }
    pub fn google_draft_source_begin(
        &self,
        account: &str,
        p: &Value,
    ) -> Result<GoogleDraftSourcePlan> {
        crate::fields(p, &["draft_id", "source_revision", "attachment_id"])?;
        identifier(account)?;
        let id = crate::string(p, "draft_id")?;
        identifier(id)?;
        let revision = page_number(p, "source_revision", 0, 1, i64::MAX)?;
        let raw:Option<String>=self.db.query_row("SELECT metadata_json FROM google_draft_sources WHERE account=?1 AND source_revision=?2 AND (draft_id=?3 OR linked_local_draft_id=?3)",params![account,revision,id],|r|r.get(0)).optional()?;
        let source: Source = serde_json::from_str(&raw.ok_or_else(|| {
            err(
                "not_found",
                "Retained draft source is outside this account/version",
            )
        })?)?;
        let aid = p
            .get("attachment_id")
            .map(|_| crate::string(p, "attachment_id"))
            .transpose()?;
        if let Some(aid) = aid {
            identifier(aid)?;
        }
        let (hash, size, name, mime_type) = if let Some(aid) = aid {
            if !source.attachment_ids.iter().any(|v| v == aid) {
                return Err(err(
                    "not_found",
                    "Attachment is outside this retained source revision",
                ));
            }
            self.db.query_row("SELECT blob_hash,size,name,mime_type FROM attachments WHERE account=?1 AND id=?2 AND state='available'",params![account,aid],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?
        } else {
            (
                source
                    .sha256
                    .ok_or_else(|| err("content_unavailable", "Remote source was not retained"))?,
                source.size.ok_or_else(invalid)?,
                format!("draft-source-{revision}.eml"),
                "message/rfc822".into(),
            )
        };
        Ok(GoogleDraftSourcePlan {
            blobs: self.google_blob_store(),
            account: account.into(),
            draft_id: id.into(),
            source_revision: revision,
            hash,
            size,
            name,
            mime_type,
            attachment_id: aid.map(Into::into),
        })
    }
}

fn source_freshness(value: &mut Value, seen: i64, last_observed: i64, state: Option<&Inventory>) {
    value["seen_generation"] = json!(seen);
    value["last_observed_at"] = json!(last_observed);
    value["latest_scan_generation"] = json!(state.map_or(0, |s| s.generation));
    value["seen_in_latest_scan"] = json!(state.is_some_and(|s| s.generation == seen));
    value["latest_scan_complete"] = json!(state.is_some_and(|s| s.complete));
    value["absence_means_sent"] = json!(false);
}

fn source_value(revision: i64, observed: i64, linked: Option<String>, raw: &str) -> Result<Value> {
    let mut value: Value = serde_json::from_str(raw)?;
    value["source_revision"] = json!(revision);
    value["observed_at"] = json!(observed);
    value["linked_local_draft_id"] = json!(linked);
    value
        .as_object_mut()
        .ok_or_else(invalid)?
        .remove("part_manifest_sha256");
    value.as_object_mut().unwrap().remove("part_manifest_size");
    Ok(value)
}
fn page_number(p: &Value, key: &str, default: i64, min: i64, max: i64) -> Result<i64> {
    let n = p
        .get(key)
        .map(|v| v.as_i64().ok_or_else(invalid))
        .transpose()?
        .unwrap_or(default);
    if n < min || n > max {
        Err(invalid())
    } else {
        Ok(n)
    }
}
