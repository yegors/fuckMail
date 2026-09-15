//! Cached Google labels and explicit local category bindings. No provider writes.
use crate::{err, fields, id, string, Result, Store};
use fm_google::gmail_api::GmailLabels;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const MAX_LABELS: usize = 10_000;
const MAX_BYTES: usize = 4 * 1024 * 1024;
const CHUNK_LABELS: usize = 64;
const CHUNK_BYTES: usize = 128 * 1024;
const MAX_BINDINGS: i64 = 1000;

#[derive(Clone)]
struct Label {
    id: String,
    name: String,
    kind: String,
}
pub struct PreparedGoogleLabels {
    labels: Vec<Label>,
    fingerprint: String,
}
#[derive(Clone)]
pub struct GoogleLabelManifest {
    count: usize,
    fingerprint: String,
}
pub struct PreparedGoogleLabelChunk {
    fingerprint: String,
    offset: usize,
    labels: Vec<Label>,
}
#[derive(Clone)]
pub struct GoogleLabelLease {
    account: String,
    connection_generation: i64,
    catalog_revision: i64,
    generation: i64,
    fingerprint: String,
    count: usize,
    pub offset: usize,
}

/// Validate, sort and hash outside the Store writer. Chunks cannot be forged by
/// deserializing unchecked provider metadata into a prepared publication type.
pub fn prepare_google_labels(value: GmailLabels) -> Result<PreparedGoogleLabels> {
    if value.labels.len() > MAX_LABELS {
        return Err(bounds());
    }
    let mut labels = Vec::with_capacity(value.labels.len());
    let mut bytes = 0_usize;
    for label in value.labels {
        id(&label.id).map_err(|_| bounds())?;
        if label.name.is_empty()
            || label.name.len() > 4096
            || label.name.chars().any(char::is_control)
            || !matches!(label.label_type.as_str(), "user" | "system")
        {
            return Err(bounds());
        }
        bytes = bytes.saturating_add(label.id.len() + label.name.len() + 32);
        if bytes > MAX_BYTES {
            return Err(bounds());
        }
        labels.push(Label {
            id: label.id,
            name: label.name,
            kind: label.label_type,
        });
    }
    labels.sort_unstable_by(|a, b| a.id.cmp(&b.id));
    let mut digest = Sha256::new();
    for (index, label) in labels.iter().enumerate() {
        if index > 0 && labels[index - 1].id == label.id {
            return Err(bounds());
        }
        for field in [&label.id, &label.name, &label.kind] {
            digest.update((field.len() as u64).to_le_bytes());
            digest.update(field.as_bytes());
        }
    }
    Ok(PreparedGoogleLabels {
        labels,
        fingerprint: format!("{:x}", digest.finalize()),
    })
}
impl PreparedGoogleLabels {
    pub fn manifest(&self) -> GoogleLabelManifest {
        GoogleLabelManifest {
            count: self.labels.len(),
            fingerprint: self.fingerprint.clone(),
        }
    }
    pub fn chunk(&self, offset: usize) -> Result<Option<PreparedGoogleLabelChunk>> {
        if offset > self.labels.len() {
            return Err(stale());
        }
        if offset == self.labels.len() {
            return Ok(None);
        }
        let mut bytes = 0;
        let labels = self.labels[offset..]
            .iter()
            .take(CHUNK_LABELS)
            .take_while(|label| {
                let next = bytes + label.id.len() + label.name.len() + 32;
                if next > CHUNK_BYTES {
                    return false;
                }
                bytes = next;
                true
            })
            .cloned()
            .collect();
        Ok(Some(PreparedGoogleLabelChunk {
            fingerprint: self.fingerprint.clone(),
            offset,
            labels,
        }))
    }
}

impl Store {
    /// None means one bounded abandoned-generation cleanup part completed.
    /// Repeat with the same manifest. An identical interrupted refresh resumes.
    pub fn google_labels_begin(
        &self,
        account: &str,
        connection_generation: i64,
        expected_revision: i64,
        manifest: &GoogleLabelManifest,
    ) -> Result<Option<GoogleLabelLease>> {
        self.labels_connection(account, Some(connection_generation), false)?;
        let tx = self.db.unchecked_transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO google_label_catalogs(account) VALUES(?1)",
            [account],
        )?;
        let current = self.label_state(account)?;
        crate::mutations::check_revision(expected_revision, current.revision)?;
        if current.pending_generation.is_some()
            && (current.pending_connection != Some(connection_generation)
                || current.pending_fingerprint.as_deref() != Some(manifest.fingerprint.as_str())
                || current.pending_count != manifest.count)
        {
            tx.execute("UPDATE google_label_catalogs SET pending_generation=NULL,pending_connection_generation=NULL,pending_fingerprint=NULL,pending_count=0,pending_offset=0 WHERE account=?1", [account])?;
        }
        if cleanup(&tx, account)? > 0 {
            tx.commit()?;
            return Ok(None);
        }
        let current = self.label_state(account)?;
        let generation = match current.pending_generation {
            Some(generation) => generation,
            None => {
                let next = current.next_generation.checked_add(1).ok_or_else(stale)?;
                tx.execute("UPDATE google_label_catalogs SET next_generation=?2,pending_generation=?2,pending_connection_generation=?3,pending_fingerprint=?4,pending_count=?5,pending_offset=0 WHERE account=?1", params![account,next,connection_generation,manifest.fingerprint,manifest.count])?;
                next
            }
        };
        let lease = GoogleLabelLease {
            account: account.into(),
            connection_generation,
            catalog_revision: current.revision,
            generation,
            fingerprint: manifest.fingerprint.clone(),
            count: manifest.count,
            offset: if current.pending_generation.is_some() {
                current.pending_offset
            } else {
                0
            },
        };
        tx.commit()?;
        Ok(Some(lease))
    }
    pub fn google_labels_stage(
        &self,
        lease: &GoogleLabelLease,
        chunk: PreparedGoogleLabelChunk,
    ) -> Result<GoogleLabelLease> {
        self.labels_connection(&lease.account, Some(lease.connection_generation), false)?;
        let tx = self.db.unchecked_transaction()?;
        self.check_label_lease(lease)?;
        if chunk.fingerprint != lease.fingerprint
            || chunk.offset != lease.offset
            || chunk.labels.is_empty()
            || lease.offset + chunk.labels.len() > lease.count
        {
            return Err(stale());
        }
        for (index, label) in chunk.labels.iter().enumerate() {
            tx.execute("INSERT INTO google_label_catalog_items(account,generation,ordinal,label_id,name,label_type) VALUES(?1,?2,?3,?4,?5,?6)", params![lease.account,lease.generation,lease.offset+index,label.id,label.name,label.kind])?;
        }
        let mut next = lease.clone();
        next.offset += chunk.labels.len();
        tx.execute(
            "UPDATE google_label_catalogs SET pending_offset=?2 WHERE account=?1",
            params![lease.account, next.offset],
        )?;
        tx.commit()?;
        Ok(next)
    }
    pub fn google_labels_publish(
        &self,
        lease: &GoogleLabelLease,
        observed_at_ms: u64,
    ) -> Result<Value> {
        self.labels_connection(&lease.account, Some(lease.connection_generation), false)?;
        let tx = self.db.unchecked_transaction()?;
        self.check_label_lease(lease)?;
        if lease.offset != lease.count {
            return Err(stale());
        }
        let next = lease.catalog_revision.checked_add(1).ok_or_else(stale)?;
        tx.execute("UPDATE google_label_catalogs SET revision=?2,generation=?3,connection_generation=?4,observed_at_ms=?5,pending_generation=NULL,pending_connection_generation=NULL,pending_fingerprint=NULL,pending_count=0,pending_offset=0 WHERE account=?1", params![lease.account,next,lease.generation,lease.connection_generation,observed_at_ms.min(i64::MAX as u64) as i64])?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(
            json!({"catalog_revision":next,"labels":lease.count,"observed_at_ms":observed_at_ms,"available":true}),
        )
    }
    pub fn google_labels_cleanup(&self, account: &str) -> Result<bool> {
        Ok(cleanup(&self.db, account)? > 0)
    }
    pub fn google_labels(&self, account: &str, p: &Value) -> Result<Value> {
        fields(p, &["limit", "after", "expected_catalog_revision"])?;
        let (connection, generation, _) = self.labels_connection(account, None, false)?;
        let state = self.label_state(account)?;
        let limit = p
            .get("limit")
            .map(|v| {
                v.as_u64()
                    .filter(|v| (1..=100).contains(v))
                    .ok_or_else(bounds)
            })
            .transpose()?
            .unwrap_or(100);
        let after = p.get("after").map(|_| string(p, "after")).transpose()?;
        if after.is_some() || p.get("expected_catalog_revision").is_some() {
            crate::mutations::check_revision(
                nonnegative(p, "expected_catalog_revision")?,
                state.revision,
            )?;
        }
        let mut statement = self.db.prepare("WITH catalog AS (SELECT label_id,name,label_type,1 AS present FROM google_label_catalog_items WHERE account=?1 AND generation=?2 UNION ALL SELECT m.provider_label_id,NULL,'user',0 FROM google_category_labels m WHERE m.account=?1 AND NOT EXISTS(SELECT 1 FROM google_label_catalog_items l WHERE l.account=?1 AND l.generation=?2 AND l.label_id=m.provider_label_id)) SELECT l.label_id,l.name,l.label_type,l.present,m.category_id FROM catalog l LEFT JOIN google_category_labels m ON m.account=?1 AND m.provider_label_id=l.label_id WHERE ?3 IS NULL OR l.label_id>?3 ORDER BY l.label_id LIMIT ?4")?;
        let mut labels = statement.query_map(params![account,state.generation,after,limit+1],|r| {
            let id: String=r.get(0)?; let kind:String=r.get(2)?; let present:bool=r.get(3)?;
            Ok(json!({"selectable":present && kind=="user" && !crate::google_store::is_system_label(&id),"id":id,"name":r.get::<_,Option<String>>(1)?,"type":kind,"present":present,"category_id":r.get::<_,Option<String>>(4)?}))
        })?.collect::<std::result::Result<Vec<_>,_>>()?;
        let more = labels.len() > limit as usize;
        labels.truncate(limit as usize);
        let next = if more {
            labels
                .last()
                .and_then(|v| v["id"].as_str())
                .map(str::to_owned)
        } else {
            None
        };
        Ok(
            json!({"catalog_revision":state.revision,"available":state.revision>0,"observed_at_ms":state.observed_at,"connection_state":connection,"stale_connection":state.connection_generation!=generation,"labels":labels,"next_after":next}),
        )
    }
    pub fn google_category_bind(&mut self, account: &str, p: &Value) -> Result<Value> {
        fields(
            p,
            &[
                "request_id",
                "expected_catalog_revision",
                "category_id",
                "provider_label_id",
                "create_name",
                "expected_provider_label_id",
            ],
        )?;
        let request = string(p, "request_id")?;
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&json!(["google_category_bind", p]))?)
        );
        let tx = self.db.unchecked_transaction()?;
        let prior: Option<(String, String)> = tx
            .query_row(
                "SELECT fingerprint,result_json FROM operations WHERE account=?1 AND request_id=?2",
                params![account, request],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((old, result)) = prior {
            if old != fingerprint {
                return Err(err(
                    "request_id_conflict",
                    "This request ID identifies a different command",
                ));
            }
            return Ok(serde_json::from_str(&result)?);
        }
        self.labels_connection(account, None, true)?;
        let state = self.label_state(account)?;
        crate::mutations::check_revision(
            nonnegative(p, "expected_catalog_revision")?,
            state.revision,
        )?;
        if state.revision == 0 {
            return Err(err(
                "label_catalog_required",
                "Refresh Google labels before configuring a category",
            ));
        }
        let category = string(p, "category_id")?;
        let label = string(p, "provider_label_id")?;
        let selected:Option<(String,String)>=tx.query_row("SELECT name,label_type FROM google_label_catalog_items WHERE account=?1 AND generation=?2 AND label_id=?3",params![account,state.generation,label],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let (label_name, kind) = selected.ok_or_else(|| {
            err(
                "label_unavailable",
                "The selected Google label is unavailable in this account",
            )
        })?;
        if kind != "user" || crate::google_store::is_system_label(label) {
            return Err(err("invalid_request", "Choose a user-created Google label"));
        }
        let existing: Option<String> = tx
            .query_row(
                "SELECT name FROM categories WHERE account=?1 AND id=?2",
                params![account, category],
                |r| r.get(0),
            )
            .optional()?;
        let old_mapping:Option<String>=tx.query_row("SELECT provider_label_id FROM google_category_labels WHERE account=?1 AND category_id=?2",params![account,category],|r|r.get(0)).optional()?;
        let create = p.get("create_name");
        let name = match (existing, create) {
            (None, Some(value)) => {
                if p.get("expected_provider_label_id").is_some() {
                    return Err(err(
                        "invalid_request",
                        "New categories do not have a previous mapping",
                    ));
                }
                let name = value
                    .as_str()
                    .filter(|v| {
                        !v.trim().is_empty() && v.len() <= 256 && !v.chars().any(char::is_control)
                    })
                    .ok_or_else(bounds)?;
                let count: i64 = tx.query_row(
                    "SELECT count(*) FROM categories WHERE account=?1",
                    [account],
                    |r| r.get(0),
                )?;
                if count >= MAX_BINDINGS {
                    return Err(bounds());
                }
                tx.execute(
                    "INSERT INTO categories(account,id,name) VALUES(?1,?2,?3)",
                    params![account, category, name],
                )?;
                name.to_owned()
            }
            (Some(name), None) => {
                let expected = p.get("expected_provider_label_id").ok_or_else(|| {
                    err("invalid_request", "Specify the previous category mapping")
                })?;
                let expected = if expected.is_null() {
                    None
                } else {
                    Some(string(p, "expected_provider_label_id")?)
                };
                if old_mapping.as_deref() != expected {
                    return Err(err("stale_revision", "The category mapping changed"));
                }
                name
            }
            (Some(_), Some(_)) => {
                return Err(err("already_exists", "The local category already exists"))
            }
            (None, None) => return Err(err("not_found", "The local category does not exist")),
        };
        let used:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM google_category_labels WHERE account=?1 AND provider_label_id=?2 AND category_id!=?3)",params![account,label,category],|r|r.get(0))?;
        if used {
            return Err(err(
                "label_already_mapped",
                "This Google label is already mapped to another category",
            ));
        }
        self.google_map_category(account, category, label)?;
        self.project_google_category(account, category, label)?;
        let result = json!({"request_id":request,"category":{"id":category,"name":name,"provider_label_id":label,"label_name":label_name},"catalog_revision":state.revision,"local_only":true});
        tx.execute("INSERT INTO operations(account,request_id,fingerprint,result_json) VALUES(?1,?2,?3,?4)",params![account,request,fingerprint,serde_json::to_string(&result)?])?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(result)
    }

    /// Materialize a newly linked label from already retained provider state.
    /// Only derived membership changes: never rewrite flags, sources, pending
    /// intent or provider receipts, and never schedule a Google write.
    pub(crate) fn project_google_category(
        &self,
        account: &str,
        category: &str,
        label: &str,
    ) -> Result<usize> {
        if crate::google_store::is_system_label(label) {
            return Err(err(
                "invalid_google_response",
                "Category targets a system label",
            ));
        }
        let desired = "WITH desired AS (
            SELECT s.message_id,m.conversation_id,
                COALESCE(i.value,a.value,EXISTS(SELECT 1 FROM json_each(s.provider_labels_json) j WHERE j.value=?3)) AS present,
                (c.message_id IS NOT NULL) AS previous
            FROM google_message_state s JOIN messages m ON m.account=s.account AND m.id=s.message_id
            LEFT JOIN google_manual_intents i ON i.account=s.account AND i.message_id=s.message_id AND i.field='category' AND i.category_id=?2
            LEFT JOIN assist_category_intents a ON a.account=s.account AND a.message_id=s.message_id AND a.category_id=?2
            LEFT JOIN message_categories c ON c.account=s.account AND c.message_id=s.message_id AND c.category_id=?2
            WHERE s.account=?1 AND s.deleted=0) ";
        let changed = self.db.execute(
            &format!("{desired} UPDATE conversations SET revision=revision+1 WHERE account=?1 AND id IN (SELECT conversation_id FROM desired WHERE present!=previous)"),
            params![account, category, label],
        )?;
        self.db.execute(
            &format!("{desired} INSERT OR IGNORE INTO message_categories(account,message_id,category_id) SELECT ?1,message_id,?2 FROM desired WHERE present=1 AND previous=0"),
            params![account, category, label],
        )?;
        self.db.execute(
            &format!("{desired} DELETE FROM message_categories WHERE account=?1 AND category_id=?2 AND message_id IN (SELECT message_id FROM desired WHERE present=0 AND previous=1)"),
            params![account, category, label],
        )?;
        Ok(changed)
    }

    pub(crate) fn recover_google_category_projections(&self) -> Result<()> {
        if self.profile_kind()? != crate::live_profile::ProfileKind::Live {
            return Ok(());
        }
        let repaired: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM meta WHERE key='google_category_projection_v1')",
            [],
            |r| r.get(0),
        )?;
        if repaired {
            return Ok(());
        }
        let tx = self.db.unchecked_transaction()?;
        let mut statement = tx.prepare("SELECT account,category_id,provider_label_id FROM google_category_labels ORDER BY account,category_id")?;
        let mappings = statement
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(statement);
        let mut changed = 0;
        for (account, category, label) in mappings {
            changed += self.project_google_category(&account, &category, &label)?;
        }
        if changed > 0 {
            tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        }
        tx.execute(
            "INSERT INTO meta(key,value) VALUES('google_category_projection_v1',1)",
            [],
        )?;
        tx.commit()?;
        Ok(())
    }
    fn labels_connection(
        &self,
        account: &str,
        generation: Option<i64>,
        write: bool,
    ) -> Result<(String, i64, BTreeSet<String>)> {
        if generation.is_some() || write {
            self.require_active_account(account)?;
        }
        id(account)?;
        if self.profile_kind()? != crate::live_profile::ProfileKind::Live {
            return Err(err(
                "profile_kind_mismatch",
                "Google labels require a live profile",
            ));
        }
        let row:Option<(String,i64,String)>=self.db.query_row("SELECT connection_state,connection_generation,scopes_json FROM google_accounts WHERE account=?1",[account],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let (state, current, scopes) =
            row.ok_or_else(|| err("not_found", "Google account does not exist"))?;
        let scopes: BTreeSet<String> = serde_json::from_str(&scopes)?;
        if generation.is_some_and(|expected| expected != current || state != "connected") {
            return Err(stale());
        }
        let granted = fm_google::auth::ScopeSet::new(&scopes);
        if write && !granted.mail_write() || generation.is_some() && !granted.mail_read() {
            return Err(err(
                "capability_unavailable",
                "Google mail permission is required",
            ));
        }
        Ok((state, current, scopes))
    }
    fn label_state(&self, account: &str) -> Result<CatalogState> {
        Ok(self.db.query_row("SELECT revision,generation,next_generation,connection_generation,observed_at_ms,pending_generation,pending_connection_generation,pending_fingerprint,pending_count,pending_offset FROM google_label_catalogs WHERE account=?1",[account],|r|Ok(CatalogState{revision:r.get(0)?,generation:r.get(1)?,next_generation:r.get(2)?,connection_generation:r.get(3)?,observed_at:r.get(4)?,pending_generation:r.get(5)?,pending_connection:r.get(6)?,pending_fingerprint:r.get(7)?,pending_count:r.get(8)?,pending_offset:r.get(9)?})).optional()?.unwrap_or_default())
    }
    fn check_label_lease(&self, lease: &GoogleLabelLease) -> Result<()> {
        let state = self.label_state(&lease.account)?;
        if state.revision != lease.catalog_revision
            || state.pending_generation != Some(lease.generation)
            || state.pending_connection != Some(lease.connection_generation)
            || state.pending_fingerprint.as_deref() != Some(lease.fingerprint.as_str())
            || state.pending_count != lease.count
            || state.pending_offset != lease.offset
        {
            return Err(stale());
        }
        Ok(())
    }
}
#[derive(Default)]
struct CatalogState {
    revision: i64,
    generation: i64,
    next_generation: i64,
    connection_generation: i64,
    observed_at: i64,
    pending_generation: Option<i64>,
    pending_connection: Option<i64>,
    pending_fingerprint: Option<String>,
    pending_count: usize,
    pending_offset: usize,
}
fn cleanup(db: &rusqlite::Connection, account: &str) -> Result<usize> {
    Ok(db.execute("DELETE FROM google_label_catalog_items WHERE rowid IN (SELECT l.rowid FROM google_label_catalog_items l JOIN google_label_catalogs c ON c.account=l.account WHERE l.account=?1 AND l.generation!=c.generation AND (c.pending_generation IS NULL OR l.generation!=c.pending_generation) LIMIT 64)",[account])?)
}
fn nonnegative(p: &Value, key: &str) -> Result<i64> {
    p[key]
        .as_i64()
        .filter(|v| *v >= 0)
        .ok_or_else(|| err("invalid_request", "Expected a nonnegative revision"))
}
fn stale() -> crate::CoreError {
    err(
        "stale_google_work",
        "Google label catalog or connection changed",
    )
}
fn bounds() -> crate::CoreError {
    err(
        "invalid_request",
        "Google label metadata exceeds supported bounds",
    )
}

#[cfg(test)]
#[path = "google_labels/tests.rs"]
mod tests;
