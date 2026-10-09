//! Cached Google labels, explicit local category bindings and the bounded
//! projection of linked labels onto stored mail. No provider writes.
use crate::{err, fields, id, string, Result, Store};
use fm_google::gmail_api::GmailLabels;
use rusqlite::{params, Connection, OptionalExtension};
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
/// A validated Gmail label list, sorted by ID and fingerprinted, built by
/// [`prepare_google_labels`].
pub struct PreparedGoogleLabels {
    labels: Vec<Label>,
    fingerprint: String,
}
/// Count and content fingerprint of a prepared label list. A refresh with the
/// same manifest resumes where an interrupted one stopped.
#[derive(Clone)]
pub struct GoogleLabelManifest {
    count: usize,
    fingerprint: String,
}
/// A bounded slice of a prepared list (at most 64 labels or about 128 KiB),
/// staged with [`Store::google_labels_stage`].
pub struct PreparedGoogleLabelChunk {
    fingerprint: String,
    offset: usize,
    labels: Vec<Label>,
}
/// Claim on one pending catalog refresh, from [`Store::google_labels_begin`].
/// Each staged chunk returns the next lease.
#[derive(Clone)]
pub struct GoogleLabelLease {
    account: String,
    connection_generation: i64,
    catalog_revision: i64,
    generation: i64,
    fingerprint: String,
    count: usize,
    /// Number of labels staged so far; the next chunk starts here.
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
    /// Returns the list's count and fingerprint.
    pub fn manifest(&self) -> GoogleLabelManifest {
        GoogleLabelManifest {
            count: self.labels.len(),
            fingerprint: self.fingerprint.clone(),
        }
    }
    /// Returns the chunk starting at `offset`, or `None` once every label is
    /// staged. Pass the current lease's `offset`.
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
    /// Starts or resumes staging a new label catalog generation.
    ///
    /// None means one bounded abandoned-generation cleanup part completed.
    /// Repeat with the same manifest. An identical interrupted refresh resumes.
    /// A pending refresh with a different manifest or connection is dropped.
    /// Fails with `stale_revision` if `expected_revision` is not the current
    /// catalog revision.
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
    /// Stages one chunk into the pending generation and returns the advanced
    /// lease. The chunk must start exactly at the lease's offset; progress is
    /// saved, so an interrupted refresh resumes from here.
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
    /// Makes a fully staged generation the current catalog and bumps its
    /// revision. `observed_at_ms` is when Gmail was read, in Unix milliseconds.
    /// The old generation's rows are removed later by cleanup.
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
    /// Deletes up to 64 rows of superseded catalog generations and reports
    /// whether any were deleted, so callers can repeat until it returns false.
    pub fn google_labels_cleanup(&self, account: &str) -> Result<bool> {
        Ok(cleanup(&self.db, account)? > 0)
    }
    /// Pages through the cached label catalog in label-ID order, with each
    /// label's linked category.
    ///
    /// Labels a category links to that are no longer in the catalog are listed
    /// with `present: false`. `p` may set `limit` (1-100), `after` (the previous
    /// page's `next_after`) and `expected_catalog_revision`, which is required
    /// when paging so a refresh between pages is detected.
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
    /// Links a local category to a Gmail user label from the cached catalog,
    /// creating the category when `create_name` is given.
    ///
    /// Relinking an existing category needs `expected_provider_label_id` (or
    /// null) to match its current link. A label can back only one category.
    /// Existing messages are re-projected from their Gmail labels; nothing is
    /// written to Gmail. Replays by `request_id`.
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
        link_category(self, &tx, account, category, label)?;
        let result = json!({"request_id":request,"category":{"id":category,"name":name,"provider_label_id":label,"label_name":label_name},"catalog_revision":state.revision,"local_only":true});
        tx.execute("INSERT INTO operations(account,request_id,fingerprint,result_json) VALUES(?1,?2,?3,?4)",params![account,request,fingerprint,serde_json::to_string(&result)?])?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(result)
    }

    /// Advances one of the account's pending category projections by one
    /// bounded part (see [`PROJECTION_ROWS`]), so other store requests are
    /// served between parts. Returns false when none was pending.
    ///
    /// Linking a label records its projection durably and applies the first
    /// part at once; callers repeat this until it returns false. Until then
    /// the category's membership may be partly applied. Only derived
    /// membership and conversation revisions change: never flags, sources,
    /// pending intent or provider receipts, and no Google write is scheduled.
    pub fn google_category_projection_step(&self, account: &str) -> Result<bool> {
        id(account)?;
        let prefix = format!("{PROJECTION_PREFIX}{account}\n");
        // Keys sort by category after the account's prefix, which ends in a
        // newline; no key of this account reaches the next character.
        let upper = format!("{PROJECTION_PREFIX}{account}\u{b}");
        let key: Option<String> = self
            .db
            .query_row(
                "SELECT key FROM meta WHERE key>?1 AND key<?2 ORDER BY key LIMIT 1",
                params![prefix, upper],
                |r| r.get(0),
            )
            .optional()?;
        let Some(key) = key else {
            return Ok(false);
        };
        let tx = self.db.unchecked_transaction()?;
        project_part(&tx, account, &key[prefix.len()..])?;
        tx.commit()?;
        Ok(true)
    }

    /// Schedules a projection of every linked category once per live profile
    /// (recorded in `meta`), so stored memberships come to match the retained
    /// Gmail labels. Runs when the store opens; the parts are applied later
    /// by [`Store::google_category_projection_step`], as is any projection a
    /// closed store left unfinished.
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
        let mut statement = tx.prepare(
            "SELECT account,category_id FROM google_category_labels ORDER BY account,category_id",
        )?;
        let mappings = statement
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(statement);
        for (account, category) in mappings {
            begin_projection(&tx, &account, &category)?;
        }
        tx.execute(
            "INSERT INTO meta(key,value) VALUES('google_category_projection_v1',1)",
            [],
        )?;
        tx.commit()?;
        Ok(())
    }
    /// Checks the Google binding for label work. With `generation`, it must be
    /// current, connected and granted mail read; with `write`, mail writes must
    /// be granted and enabled. Returns the connection state, current
    /// generation and scopes.
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
        if write
            && (!granted.mail_write()
                || !crate::preferences::google_write_enabled(&self.db, account, "mail")?)
            || generation.is_some() && !granted.mail_read()
        {
            return Err(err(
                "capability_unavailable",
                "Google mail permission is required",
            ));
        }
        Ok((state, current, scopes))
    }
    /// The catalog row, or all-zero defaults before the first refresh.
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
/// Deletes up to 64 catalog rows that belong to neither the current nor the
/// pending generation. Bounded so one call never holds the writer long.
fn cleanup(db: &rusqlite::Connection, account: &str) -> Result<usize> {
    Ok(db.execute("DELETE FROM google_label_catalog_items WHERE rowid IN (SELECT l.rowid FROM google_label_catalog_items l JOIN google_label_catalogs c ON c.account=l.account WHERE l.account=?1 AND l.generation!=c.generation AND (c.pending_generation IS NULL OR l.generation!=c.pending_generation) LIMIT 64)",[account])?)
}
/// Rows of `google_message_state`, across all accounts, one projection part
/// reads. Small enough that a part never holds the store long on a large
/// mailbox; smaller in tests so a few hundred messages span several parts.
pub(crate) const PROJECTION_ROWS: i64 = if cfg!(test) { 256 } else { 5_000 };
/// `meta` key prefix of a pending category projection. The full key is
/// `prefix + account + "\n" + category`, and its value is the last
/// `google_message_state` rowid already projected. Identifiers never contain
/// control characters, so the newline separates them unambiguously.
const PROJECTION_PREFIX: &str = "google_category_projection\n";
/// The changes one projection part makes: each undeleted Gmail message in the
/// rowid range `(?3, ?4]` of account `?1` whose membership in category `?2`
/// differs from what its saved intent, or else label `?5`, says. A pending
/// manual choice wins over a model's choice, which wins over the label.
/// `NOT INDEXED` keeps the read on the rowid range: the account's key index
/// would visit every message of the account.
pub(crate) const PROJECTION_CHANGES: &str = "SELECT message_id,conversation_id,present FROM (
    SELECT s.message_id,m.conversation_id,
        COALESCE(i.value,a.value,EXISTS(SELECT 1 FROM json_each(s.provider_labels_json) j WHERE j.value=?5)) AS present,
        (c.message_id IS NOT NULL) AS previous
    FROM google_message_state s NOT INDEXED JOIN messages m ON m.account=s.account AND m.id=s.message_id
    LEFT JOIN google_manual_intents i ON i.account=s.account AND i.message_id=s.message_id AND i.field='category' AND i.category_id=?2
    LEFT JOIN assist_category_intents a ON a.account=s.account AND a.message_id=s.message_id AND a.category_id=?2
    LEFT JOIN message_categories c ON c.account=s.account AND c.message_id=s.message_id AND c.category_id=?2
    WHERE s.rowid>?3 AND s.rowid<=?4 AND s.account=?1 AND s.deleted=0)
WHERE present!=previous";

fn projection_key(account: &str, category: &str) -> String {
    format!("{PROJECTION_PREFIX}{account}\n{category}")
}
/// Records that `category`'s membership must be derived again from the
/// retained Gmail labels, from the first stored message on. Restarts a
/// projection already under way, as when the category is linked to another
/// label.
fn begin_projection(db: &Connection, account: &str, category: &str) -> Result<()> {
    db.execute(
        "INSERT INTO meta(key,value) VALUES(?1,0) ON CONFLICT(key) DO UPDATE SET value=0",
        [projection_key(account, category)],
    )?;
    Ok(())
}
/// Links `category` to Gmail `label` and applies the first part of its
/// projection; [`Store::google_category_projection_step`] applies the rest.
/// Run inside the caller's transaction.
pub(crate) fn link_category(
    store: &Store,
    db: &Connection,
    account: &str,
    category: &str,
    label: &str,
) -> Result<()> {
    store.google_map_category(account, category, label)?;
    begin_projection(db, account, category)?;
    project_part(db, account, category)?;
    Ok(())
}
/// Applies the next part of `category`'s pending projection: at most
/// [`PROJECTION_ROWS`] rows after the saved cursor. Advances the cursor, or
/// drops the projection once it has passed the last row or the category is
/// no longer linked. Run inside the caller's transaction, so a part and its
/// cursor commit together.
fn project_part(db: &Connection, account: &str, category: &str) -> Result<()> {
    let key = projection_key(account, category);
    let cursor: Option<i64> = db
        .query_row("SELECT value FROM meta WHERE key=?1", [&key], |r| r.get(0))
        .optional()?;
    let Some(cursor) = cursor else {
        return Ok(());
    };
    // Mail of an account being removed is left to its cleanup.
    let label: Option<String> = db.query_row("SELECT g.provider_label_id FROM google_category_labels g JOIN accounts a ON a.id=g.account WHERE g.account=?1 AND g.category_id=?2 AND a.lifecycle_state='active'",params![account,category],|r|r.get(0)).optional()?;
    let Some(label) = label else {
        db.execute("DELETE FROM meta WHERE key=?1", [&key])?;
        return Ok(());
    };
    if crate::google_store::is_system_label(&label) {
        return Err(err(
            "invalid_google_response",
            "Category targets a system label",
        ));
    }
    // The part ends at the last of the next rows of any account, so it
    // reads a bounded number of rows however the accounts interleave.
    let end: Option<i64> = db
        .prepare_cached("SELECT rowid FROM google_message_state NOT INDEXED WHERE rowid>?1 ORDER BY rowid LIMIT 1 OFFSET ?2")?
        .query_row(params![cursor, PROJECTION_ROWS - 1], |r| r.get(0))
        .optional()?;
    let changes = db
        .prepare_cached(PROJECTION_CHANGES)?
        .query_map(
            params![account, category, cursor, end.unwrap_or(i64::MAX), label],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, bool>(2)?,
                ))
            },
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut conversations = BTreeSet::new();
    for (message, conversation, present) in changes {
        if present {
            db.prepare_cached("INSERT OR IGNORE INTO message_categories(account,message_id,category_id) VALUES(?1,?2,?3)")?.execute(params![account, message, category])?;
        } else {
            db.prepare_cached("DELETE FROM message_categories WHERE account=?1 AND message_id=?2 AND category_id=?3")?.execute(params![account, message, category])?;
        }
        conversations.insert(conversation);
    }
    for conversation in &conversations {
        db.prepare_cached(
            "UPDATE conversations SET revision=revision+1 WHERE account=?1 AND id=?2",
        )?
        .execute(params![account, conversation])?;
    }
    if !conversations.is_empty() {
        db.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
    }
    match end {
        Some(end) => db.execute("UPDATE meta SET value=?2 WHERE key=?1", params![key, end])?,
        None => db.execute("DELETE FROM meta WHERE key=?1", [&key])?,
    };
    Ok(())
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
