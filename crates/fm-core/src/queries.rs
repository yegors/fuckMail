use crate::*;
use rusqlite::params;
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
#[derive(Clone)]
pub(crate) struct ConversationState {
    position: usize,
    unread: bool,
    starred: bool,
    unread_messages: i64,
    starred_messages: i64,
    latest: f64,
    kind: u8,
    kind_incoming: bool,
    kind_at: f64,
    kind_id: String,
}
impl ConversationState {
    fn empty(position: usize) -> Self {
        Self {
            position,
            unread: false,
            starred: false,
            unread_messages: 0,
            starred_messages: 0,
            latest: 0.0,
            kind: 0,
            kind_incoming: false,
            kind_at: 0.0,
            kind_id: String::new(),
        }
    }
    fn absorb(&mut self, row: &rusqlite::Row<'_>) -> rusqlite::Result<()> {
        let unread = row.get::<_, bool>(1)?;
        let starred = row.get::<_, bool>(2)?;
        self.unread |= unread;
        self.starred |= starred;
        self.unread_messages += i64::from(unread);
        self.starred_messages += i64::from(starred);
        let at = row.get::<_, Option<f64>>(3)?.unwrap_or(0.0);
        self.latest = self.latest.max(at);
        let incoming = !row.get::<_, bool>(6)?;
        let id = row.get_ref(5)?.as_str().map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
        })?;
        if self.kind_id.is_empty()
            || incoming && !self.kind_incoming
            || incoming == self.kind_incoming
                && (at > self.kind_at || at == self.kind_at && id > self.kind_id.as_str())
        {
            self.kind = row.get(7)?;
            self.kind_incoming = incoming;
            self.kind_at = at;
            self.kind_id = id.into();
        }
        Ok(())
    }
}
fn kind_name(kind: u8) -> &'static str {
    match kind {
        1 => "people",
        2 => "automated",
        _ => "unknown",
    }
}
const STATE_SELECT:&str = "SELECT m.conversation_id,m.unread,m.starred,julianday(m.sent_at),m.rowid,m.id,m.sent,CASE k.kind WHEN 'people' THEN 1 WHEN 'automated' THEN 2 ELSE 0 END FROM messages m LEFT JOIN message_sender_kinds k ON k.account=m.account AND k.message_id=m.id AND k.source_hash=m.source_hash WHERE m.account=?1";
pub(crate) struct ConversationProjection {
    generation: u64,
    message_count: i64,
    max_rowid: i64,
    states: Arc<HashMap<String, ConversationState>>,
}
pub(crate) type ConversationCache = RefCell<HashMap<String, ConversationProjection>>;
pub(crate) type ProjectionCache = RefCell<HashMap<(String, &'static str), (u64, Value)>>;
pub(crate) struct CoverageProjection {
    generation: u64,
    message_generation: u64,
    max_message_rowid: i64,
    document_count: i64,
    max_document_rowid: i64,
    value: Value,
}
pub(crate) type CoverageCache = RefCell<HashMap<String, CoverageProjection>>;
struct ChangeSnapshot {
    messages: u64,
    coverage: u64,
    search: u64,
    reset_after: u64,
    inserts: Vec<(u64, i64)>,
    coverage_reset_after: u64,
    documents: Vec<(u64, i64)>,
}
#[derive(Default)]
struct Changes {
    hooks: u64,
    calendar: u64,
    messages: u64,
    coverage: u64,
    search: u64,
    reset_after: u64,
    inserts: VecDeque<(u64, i64)>,
    coverage_reset_after: u64,
    documents: VecDeque<(u64, i64)>,
    observed: (u64, u64, u64),
    observed_in_transaction: bool,
}
#[derive(Clone)]
pub(crate) struct MailChanges(Arc<Mutex<Changes>>, String);
impl MailChanges {
    pub(crate) fn install(db: &Connection) -> Self {
        let changes = Arc::new(Mutex::new(Changes {
            observed: (db.total_changes(), 0, 0),
            ..Default::default()
        }));
        let hook = changes.clone();
        db.update_hook(Some(move |action, _: &str, table: &str, rowid| {
            let mut c = hook.lock().unwrap();
            c.hooks += 1;
            if table == "calendar_publication" {
                c.calendar += 1;
            }
            if table == "messages" {
                c.messages += 1;
                c.coverage += 1;
                let generation = c.messages;
                if action == rusqlite::hooks::Action::SQLITE_INSERT {
                    c.inserts.push_back((generation, rowid));
                    if c.inserts.len() > 512 {
                        c.reset_after = c.inserts.pop_front().unwrap().0;
                    }
                } else {
                    c.reset_after = generation;
                    c.inserts.clear();
                    c.coverage_reset_after = c.coverage;
                }
            } else if table == "message_sender_kinds" {
                c.messages += 1;
                c.reset_after = c.messages;
                c.inserts.clear();
            } else if table == "attachments" || table.starts_with("message_fts") {
                c.coverage += 1;
                if table == "message_fts_docsize"
                    && action == rusqlite::hooks::Action::SQLITE_INSERT
                {
                    let generation = c.coverage;
                    c.documents.push_back((generation, rowid));
                    if c.documents.len() > 512 {
                        c.coverage_reset_after = c.documents.pop_front().unwrap().0;
                    }
                } else if table == "attachments" || table == "message_fts_docsize" {
                    c.coverage_reset_after = c.coverage;
                    c.documents.clear();
                }
            }
            if matches!(
                table,
                "messages"
                    | "conversations"
                    | "attachments"
                    | "categories"
                    | "message_categories"
                    | "message_sender_kinds"
            ) || table.starts_with("message_fts")
            {
                c.search += 1;
            }
        }));
        // An opaque pagination identity, not an authorization token. The serial
        // distinguishes stores opened in the same process/timer tick; time and
        // process identity prevent reuse across normal writer restarts.
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let epoch = format!(
            "{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        );
        Self(changes, epoch)
    }
    pub(crate) fn calendar_generation(&self) -> u64 {
        self.0.lock().unwrap().calendar
    }
    pub(crate) fn reset_calendar_generation(&self) {
        self.0.lock().unwrap().calendar = 0;
    }
    fn snapshot(&self, db: &Connection) -> ChangeSnapshot {
        let mut c = self.0.lock().unwrap();
        let total = db.total_changes();
        let (old_total, old_hooks, old_messages) = c.observed;
        if total - old_total != c.hooks - old_hooks {
            // Virtual tables and SQLite's truncate optimization can change rows
            // without a corresponding hook. An observed message-insert delta is
            // safe only after inventory validation below; otherwise rebuild.
            c.coverage += 1;
            c.search += 1;
            if c.messages == old_messages {
                c.messages += 1;
                c.reset_after = c.messages;
                c.inserts.clear();
            }
        }
        let in_transaction = !db.is_autocommit();
        if in_transaction || c.observed_in_transaction {
            // SQLite hooks/total_changes are not reversed on rollback. Never
            // reuse a projection read inside a transaction or after its exit,
            // including SAVEPOINT/ROLLBACK TO without an outer rollback hook.
            c.messages += 1;
            c.coverage += 1;
            c.search += 1;
            c.reset_after = c.messages;
            c.coverage_reset_after = c.coverage;
            c.inserts.clear();
            c.documents.clear();
        }
        c.observed_in_transaction = in_transaction;
        c.observed = (total, c.hooks, c.messages);
        ChangeSnapshot {
            messages: c.messages,
            coverage: c.coverage,
            search: c.search,
            reset_after: c.reset_after,
            inserts: c.inserts.iter().copied().collect(),
            coverage_reset_after: c.coverage_reset_after,
            documents: c.documents.iter().copied().collect(),
        }
    }
}
impl Store {
    pub(crate) fn accounts(&self) -> Result<Value> {
        let mut stmt = self
            .db
            .prepare("SELECT id,name,email,provider FROM accounts ORDER BY rowid")?;
        let accounts = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut out = Vec::new();
        for (account, name, email, provider) in accounts {
            let (unread, starred, messages) = self.account_message_counts(&account)?;
            let count: i64 = self.db.query_row(
                "SELECT COUNT(*) FROM conversations WHERE account=?1",
                [&account],
                |r| r.get(0),
            )?;
            let counts = self.account_counts(&account)?;
            let (metadata_revision, signature_text): (i64, String) = self.db.query_row(
                "SELECT revision,signature_text FROM account_metadata WHERE account=?1",
                [&account],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            out.push(json!({"metadata_revision":metadata_revision,"unread_conversation_count":counts["unread_conversation_count"],"held_conversation_count":counts["held_conversation_count"],"done_conversation_count":counts["done_conversation_count"],"count_scope":"all_account_mail_including_archive","id":account,"name":name,"email":email,"provider":provider,"signature_text":signature_text,"unread_count":unread,"starred_count":starred,"conversation_count":count,"message_count":messages,"coverage":self.coverage(&account)?}));
            let drafts: i64 = self.db.query_row(
                "SELECT COUNT(*) FROM drafts WHERE account=?1 AND discarded=0",
                [&account],
                |r| r.get(0),
            )?;
            out.last_mut().unwrap()["draft_count"] = json!(drafts);
            if let Some(status) = self.google_account_status(&account)? {
                out.last_mut().unwrap()["google"] = status;
            }
        }
        Ok(json!({"accounts":out,"fixture_revision":self.fixture_revision()?}))
    }
    fn cached_projection(
        &self,
        account: &str,
        key: &'static str,
        stamp: u64,
        build: impl FnOnce() -> Result<Value>,
    ) -> Result<Value> {
        let cache_key = (account.to_owned(), key);
        if let Some((old, value)) = self.projection_cache.borrow().get(&cache_key) {
            if *old == stamp {
                return Ok(value.clone());
            }
        }
        let value = build()?;
        self.projection_cache
            .borrow_mut()
            .insert(cache_key, (stamp, value.clone()));
        Ok(value)
    }
    pub(crate) fn account_counts(&self, account: &str) -> Result<Value> {
        let stamp = self.mail_changes.snapshot(&self.db).messages;
        self.cached_projection(account, "counts", stamp, || {
            self.compute_account_counts(account)
        })
    }
    fn account_message_counts(&self, account: &str) -> Result<(i64, i64, i64)> {
        let states = self.conversation_states(account)?;
        let (unread, starred) = states.values().fold((0, 0), |(u, s), state| {
            (u + state.unread_messages, s + state.starred_messages)
        });
        let messages = self.conversation_cache.borrow()[account].message_count;
        Ok((unread, starred, messages))
    }
    pub(crate) fn flag_projection_stamp(&self) -> (u64, u64) {
        let changes = self.mail_changes.snapshot(&self.db);
        (changes.messages, changes.coverage)
    }
    pub(crate) fn refresh_committed_flag_projections(
        &self,
        account: &str,
        cid: &str,
        before: (u64, u64),
    ) {
        // Only the serialized, successfully committed flag path calls this.
        // Its SQL changes flags/revisions/manual intents, never source inventory,
        // routing, kinds or another conversation. Unknown writes and rollbacks
        // keep the update-hook invalidation path; a stale cache is never promoted.
        let changes = self.mail_changes.snapshot(&self.db);
        let Ok(mut state) = self.conversation_state(account, cid) else {
            return;
        };
        for (owner, cached) in self.conversation_cache.borrow_mut().iter_mut() {
            if cached.generation != before.0 {
                continue;
            }
            if owner == account {
                let Some(previous) = cached.states.get(cid) else {
                    continue;
                };
                state.position = previous.position;
                Arc::make_mut(&mut cached.states).insert(cid.to_owned(), state.clone());
            }
            cached.generation = changes.messages;
        }
        for ((owner, key), (generation, _)) in self.projection_cache.borrow_mut().iter_mut() {
            // Counts in the changed account must be recomputed from its refreshed
            // conversation states. Other accounts' known-valid totals are exact.
            if *key == "counts" && owner != account && *generation == before.0 {
                *generation = changes.messages;
            }
        }
        for cached in self.coverage_cache.borrow_mut().values_mut() {
            if cached.generation == before.1 && cached.message_generation == before.0 {
                cached.generation = changes.coverage;
                cached.message_generation = changes.messages;
            }
        }
    }
    fn conversation_state(&self, account: &str, cid: &str) -> Result<ConversationState> {
        let mut state = ConversationState::empty(0);
        let mut statement = self
            .db
            .prepare_cached(&format!("{STATE_SELECT} AND m.conversation_id=?2"))?;
        let mut rows = statement.query(params![account, cid])?;
        while let Some(row) = rows.next()? {
            state.absorb(row)?;
        }
        Ok(state)
    }
    fn conversation_states(
        &self,
        account: &str,
    ) -> Result<Arc<HashMap<String, ConversationState>>> {
        let changes = self.mail_changes.snapshot(&self.db);
        let stamp = changes.messages;
        let reset_after = changes.reset_after;
        let inserts = changes.inserts;
        if let Some(old) = self.conversation_cache.borrow_mut().get_mut(account) {
            if old.generation == stamp {
                return Ok(old.states.clone());
            }
            if old.generation >= reset_after {
                let mut additions = HashSet::new();
                let mut conversations = HashSet::new();
                let mut row_query = self.db.prepare_cached(
                    "SELECT account,conversation_id FROM messages WHERE rowid=?1",
                )?;
                let mut append_only = true;
                for (_, rowid) in inserts
                    .iter()
                    .filter(|(generation, _)| *generation > old.generation)
                {
                    if let Some((owner, cid)) = row_query
                        .query_row([rowid], |r| {
                            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                        })
                        .optional()?
                    {
                        if owner == account {
                            append_only &= *rowid > old.max_rowid;
                            additions.insert(*rowid);
                            conversations.insert(cid);
                        }
                    }
                }
                let actual: i64 = self.db.query_row(
                    "SELECT COUNT(*) FROM messages WHERE account=?1",
                    [account],
                    |r| r.get(0),
                )?;
                // This also catches rollback/reused rowids, REPLACE's unhooked
                // delete, and a truncate mixed with otherwise ordinary inserts.
                if append_only && actual == old.message_count + additions.len() as i64 {
                    let states = Arc::make_mut(&mut old.states);
                    let mut aggregate = self
                        .db
                        .prepare_cached(&format!("{STATE_SELECT} AND m.conversation_id=?2"))?;
                    for cid in conversations {
                        let position = states
                            .get(&cid)
                            .map_or(states.len(), |state| state.position);
                        let mut state = ConversationState::empty(position);
                        let mut rows = aggregate.query(params![account, cid])?;
                        while let Some(row) = rows.next()? {
                            state.absorb(row)?;
                        }
                        states.insert(cid, state);
                    }
                    old.message_count = actual;
                    old.max_rowid = additions.into_iter().fold(old.max_rowid, i64::max);
                    old.generation = stamp;
                    return Ok(old.states.clone());
                }
            }
        }
        let mut stmt = self.db.prepare_cached(STATE_SELECT)?;
        let mut message_count = 0;
        let mut max_rowid = 0;
        let mut states = HashMap::new();
        let mut rows = stmt.query([account])?;
        while let Some(row) = rows.next()? {
            message_count += 1;
            max_rowid = max_rowid.max(row.get::<_, i64>(4)?);
            let position = states.len();
            let state = states
                .entry(row.get::<_, String>(0)?)
                .or_insert_with(|| ConversationState::empty(position));
            state.absorb(row)?;
        }
        let states = Arc::new(states);
        self.conversation_cache.borrow_mut().insert(
            account.into(),
            ConversationProjection {
                generation: stamp,
                message_count,
                max_rowid,
                states: states.clone(),
            },
        );
        Ok(states)
    }
    fn compute_account_counts(&self, account: &str) -> Result<Value> {
        let states = self.conversation_states(account)?;
        let unread = states.values().filter(|s| s.unread).count();
        let held = states.values().filter(|s| s.starred).count();
        let done = states.values().filter(|s| !s.unread && !s.starred).count();
        Ok(
            json!({"conversation_count":states.len(),"unread_conversation_count":unread,"held_conversation_count":held,"done_conversation_count":done,"scope":"all_account_mail_including_archive","held_includes_unread":true}),
        )
    }
    fn coverage(&self, account: &str) -> Result<Value> {
        if self.profile_kind()? == live_profile::ProfileKind::Live {
            let coverage = self.google_live_coverage(account)?;
            return Ok(
                json!({"synthetic":false,"inventory_complete":coverage.inventory_complete,"delta_current":coverage.delta_current,
                "messages":coverage.observed,"indexed_messages":coverage.indexed,"text_index_complete":coverage.indexed==coverage.observed,
                "retained_source_records":coverage.retained_sources,"retained_sources":coverage.retained_sources,"source_verification":"verified_on_acquisition",
                "attachments":null,"recorded_available_attachments":null,"complete_attachments":null,"verified_attachments":null,"attachment_verification":"verified_on_acquisition",
                "messages_with_complete_attachments":coverage.complete_attachments,"unavailable_messages":coverage.unavailable,"offline_complete":coverage.offline_complete,
                "text":format!("Cached text: {}/{} observed messages. {} source records retained; {} items need attention. {}",coverage.indexed,coverage.observed,coverage.retained_sources,coverage.unavailable,if coverage.offline_complete {"Inventory and catch-up are complete at the last sync."} else {"Full offline acquisition is incomplete."})}),
            );
        }
        let changes = self.mail_changes.snapshot(&self.db);
        if let Some(old) = self.coverage_cache.borrow().get(account) {
            if old.generation == changes.coverage {
                return Ok(old.value.clone());
            }
        }
        self.conversation_states(account)?;
        let (message_count, max_message_rowid) = {
            let cache = self.conversation_cache.borrow();
            let state = &cache[account];
            (state.message_count, state.max_rowid)
        };
        // Independent inventories validate bounded append deltas. COUNT(*) on
        // the compact FTS-owned table and MAX(id) use its B-tree directly; do
        // not combine them into a row-by-row aggregate or infer index coverage
        // from message inventory alone.
        let document_count: i64 =
            self.db
                .query_row("SELECT COUNT(*) FROM message_fts_docsize", [], |r| r.get(0))?;
        let max_document_rowid: i64 = self.db.query_row(
            "SELECT COALESCE(MAX(id),0) FROM message_fts_docsize",
            [],
            |r| r.get(0),
        )?;
        let mut delta = None;
        if let Some(old) = self.coverage_cache.borrow().get(account) {
            if old.generation >= changes.coverage_reset_after
                && old.message_generation >= changes.reset_after
            {
                let mut additions = HashSet::new();
                let mut new_documents = HashSet::new();
                let mut source_delta = 0;
                let mut indexed_delta = 0;
                let mut append_only = true;
                let mut message_query = self.db.prepare_cached(
                    "SELECT account,source_hash<>'' FROM messages WHERE rowid=?1",
                )?;
                let mut document_query = self.db.prepare_cached(
                    "SELECT EXISTS(SELECT 1 FROM message_fts_docsize WHERE id=?1)",
                )?;
                for (_, rowid) in changes
                    .inserts
                    .iter()
                    .filter(|(generation, _)| *generation > old.message_generation)
                {
                    if let Some((owner, source)) = message_query
                        .query_row([rowid], |r| {
                            Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?))
                        })
                        .optional()?
                    {
                        if owner == account && additions.insert(*rowid) {
                            append_only &= *rowid > old.max_message_rowid;
                            source_delta += i64::from(source);
                            // Also handles a previously orphaned indexed row
                            // whose normalized message was appended later.
                            indexed_delta += i64::from(
                                document_query.query_row([rowid], |r| r.get::<_, bool>(0))?,
                            );
                        }
                    }
                }
                for (_, rowid) in changes
                    .documents
                    .iter()
                    .filter(|(generation, _)| *generation > old.generation)
                {
                    if document_query.query_row([rowid], |r| r.get::<_, bool>(0))?
                        && new_documents.insert(*rowid)
                    {
                        append_only &= *rowid > old.max_document_rowid;
                        if !additions.contains(rowid) {
                            let owner: Option<String> =
                                message_query.query_row([rowid], |r| r.get(0)).optional()?;
                            indexed_delta += i64::from(owner.as_deref() == Some(account));
                        }
                    }
                }
                if append_only
                    && message_count
                        == old.value["messages"].as_i64().unwrap() + additions.len() as i64
                    && document_count == old.document_count + new_documents.len() as i64
                {
                    delta = Some(self.coverage_from_counts(
                        account,
                        message_count,
                        old.value["retained_source_records"].as_i64().unwrap() + source_delta,
                        old.value["indexed_messages"].as_i64().unwrap() + indexed_delta,
                    )?);
                }
            }
        }
        let value = match delta {
            Some(value) => value,
            None => self.compute_coverage(account)?,
        };
        self.coverage_cache.borrow_mut().insert(
            account.into(),
            CoverageProjection {
                generation: changes.coverage,
                message_generation: changes.messages,
                max_message_rowid,
                document_count,
                max_document_rowid,
                value: value.clone(),
            },
        );
        Ok(value)
    }
    fn compute_coverage(&self, account: &str) -> Result<Value> {
        let (messages, source_records): (i64, i64) = self.db.query_row(
            "SELECT COUNT(*),COALESCE(SUM(source_hash<>''),0) FROM messages WHERE account=?1",
            [account],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let indexed_messages: i64 = self.db.query_row(
            // With this schema's default columnsize=1, FTS5 maintains one
            // compact token-count row per indexed document. Count those actual
            // FTS-owned identities rather than seeking each large content row.
            "SELECT COUNT(*) FROM messages m JOIN message_fts_docsize f ON f.id=m.rowid WHERE m.account=?1",
            [account], |r| r.get(0),
        )?;
        self.coverage_from_counts(account, messages, source_records, indexed_messages)
    }
    fn coverage_from_counts(
        &self,
        account: &str,
        messages: i64,
        source_records: i64,
        indexed_messages: i64,
    ) -> Result<Value> {
        // Recount attachments even on append deltas: REPLACE/truncate can omit
        // update hooks when mixed with otherwise ordinary message insertion.
        let (attachments, recorded_available): (i64, i64) = self.db.query_row(
            "SELECT COUNT(*),COALESCE(SUM(state='available'),0) FROM attachments WHERE account=?1",
            [account],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        // A database record is not proof that its file remains present/intact.
        // List queries count actual indexed rows but never hash the whole corpus.
        // Individual attachment use verifies its bytes; there is no aggregate
        // full-profile verification pass in this API yet.
        let synthetic = self.profile_kind()? == live_profile::ProfileKind::Synthetic;
        Ok(
            json!({"synthetic":synthetic,"inventory_complete":synthetic,"messages":messages,
            "indexed_messages":indexed_messages,"text_index_complete":indexed_messages==messages,
            "retained_source_records":source_records,"retained_sources":null,"source_verification":"not_run",
            "attachments":attachments,"recorded_available_attachments":recorded_available,
            "complete_attachments":null,"verified_attachments":null,"attachment_verification":"not_run",
            "offline_complete":false,
            "text":if synthetic {format!("Synthetic text index: {indexed_messages}/{messages} messages. Full offline content verification has not run; missing/partial attachment fixtures are included.")} else {format!("Cached text index: {indexed_messages}/{messages} downloaded messages. Google inventory and offline content verification have not completed.")}}),
        )
    }
    pub(crate) fn conversations(&self, account: &str, p: &Value) -> Result<Value> {
        self.conversations_ordered(account, p, false)
    }
    fn conversations_ordered(&self, account: &str, p: &Value, recent: bool) -> Result<Value> {
        fields(
            p,
            &[
                "query",
                "field",
                "category",
                "folder",
                "limit",
                "offset",
                "sender",
                "exact_sender",
                "kind",
                "status",
                "has_attachment",
                "after",
                "before",
                "grouped",
                "cursor",
                "collapsed_groups",
            ],
        )?;
        let limit = number(p, "limit", 50, 1, 100)?;
        let mut offset = number(p, "offset", 0, 0, 10_000_000)?;
        let mut collapsed = HashSet::new();
        if let Some(groups) = p.get("collapsed_groups") {
            let groups = groups
                .as_array()
                .filter(|groups| groups.len() <= 3)
                .ok_or_else(|| {
                    err(
                        "invalid_request",
                        "collapsed_groups must be an array of up to three groups",
                    )
                })?;
            for group in groups {
                let group = group
                    .as_str()
                    .filter(|g| ["review", "held", "done"].contains(g))
                    .ok_or_else(|| err("invalid_request", "Unknown collapsed group"))?;
                if !collapsed.insert(group) {
                    return Err(err("invalid_request", "Collapsed groups must be unique"));
                }
            }
        }
        validate_filters(p)?;
        let sender = optional_string(p, "sender", "")?;
        let exact_sender = optional_string(p, "exact_sender", "")?
            .trim()
            .to_lowercase();
        let kind = optional_string(p, "kind", "all")?;
        let sender_pattern = format!(
            "%{}%",
            sender
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        let status = optional_string(p, "status", "all")?;
        let attachment = p.get("has_attachment").and_then(Value::as_bool);
        let after = optional_string(p, "after", "")?;
        let before = optional_string(p, "before", "")?;
        let grouped = match p.get("grouped") {
            None => false,
            Some(Value::Bool(b)) => *b,
            _ => return Err(err("invalid_request", "grouped must be a boolean")),
        };
        let revision = self.revision()?;
        let search_generation = self.mail_changes.snapshot(&self.db).search;
        let epoch = &self.mail_changes.1;
        let mut cursor_scope = p.clone();
        cursor_scope.as_object_mut().unwrap().remove("cursor");
        cursor_scope.as_object_mut().unwrap().remove("offset");
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&json!([account, cursor_scope]))?)
        );
        if let Some(cursor) = p.get("cursor") {
            if p.get("offset").is_some() {
                return Err(err("invalid_request", "Use cursor or offset, not both"));
            }
            let text = cursor
                .as_str()
                .filter(|v| v.len() <= 1024)
                .ok_or_else(|| err("invalid_request", "Invalid conversation cursor"))?;
            let cursor: Value = serde_json::from_str(text)?;
            fields(
                &cursor,
                &[
                    "revision",
                    "offset",
                    "fingerprint",
                    "epoch",
                    "search_generation",
                ],
            )?;
            if cursor["epoch"] != *epoch
                || cursor["search_generation"] != search_generation
                || cursor["fingerprint"] != fingerprint
            {
                return Err(err(
                    "stale_cursor",
                    "Mail or query changed; restart pagination",
                ));
            }
            offset = number(&cursor, "offset", 0, 0, 10_000_000)?;
        }
        let query = optional_string(p, "query", "")?;
        if query.split_whitespace().any(|word| {
            word.split_once(':').is_some_and(|(prefix, value)| {
                !["http", "https"].contains(&prefix)
                    && !prefix.is_empty()
                    && prefix.chars().all(|c| c.is_ascii_alphabetic())
                    && (!value.is_empty()
                        || [
                            "from", "to", "is", "has", "before", "after", "subject", "body",
                            "category", "status",
                        ]
                        .contains(&prefix))
            })
        }) {
            return Err(err(
                "invalid_request",
                "Text operators are unavailable; use typed search filters",
            ));
        }
        if query.len() > 512 {
            return Err(err("invalid_request", "Search is limited to 512 bytes"));
        }
        let field = optional_string(p, "field", "all")?;
        if !["all", "sender", "subject", "body"].contains(&field) {
            return Err(err("invalid_request", "Unknown search field"));
        }
        let folder = optional_string(p, "folder", "inbox")?;
        if !["all", "inbox", "sent"].contains(&folder) {
            return Err(err("invalid_request", "Unknown folder"));
        }
        let category = optional_string(p, "category", "")?;
        if !category.is_empty() {
            id(category)?;
            let exists: bool = self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM categories WHERE account=?1 AND id=?2)",
                params![account, category],
                |r| r.get(0),
            )?;
            if !exists {
                return Err(err("not_found", "Category does not exist in this account"));
            }
        }
        let tokens: Vec<_> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect();
        if tokens.len() > 32 || (!query.trim().is_empty() && tokens.is_empty()) {
            return Err(err("invalid_request", "Search needs 1–32 words"));
        }
        let expression = tokens
            .iter()
            .map(|s| format!("\"{s}\"*"))
            .collect::<Vec<_>>()
            .join(" AND ");
        let expression = if field == "all" || expression.is_empty() {
            expression
        } else {
            format!("{field}:({expression})")
        };
        // One first page per account bounds this cache independently of the
        // corpus and query history. Mail/source hooks and transaction snapshots
        // invalidate it through the existing search generation; presentation,
        // draft and preference writes do not force another full mailbox scan.
        // Validation above still runs on every call, including cursor scope.
        let first_page_key = (account.to_owned(), "first_conversation_page");
        let cache_first_page = !recent && offset == 0;
        let next_cursor = |next: i64| -> Result<String> {
            Ok(serde_json::to_string(
                &json!({"revision":revision,"epoch":epoch,
                "search_generation":search_generation,"offset":next,"fingerprint":fingerprint}),
            )?)
        };
        if cache_first_page {
            if let Some((stamp, cached)) = self.projection_cache.borrow().get(&first_page_key) {
                if *stamp == search_generation && cached["fingerprint"] == fingerprint {
                    let mut result = cached["result"].clone();
                    // Revision is shared with non-mail durable writes. A cache
                    // hit must publish today's envelope, never an old receipt.
                    result["revision"] = revision.into();
                    result["next_cursor"] = match result["next_offset"].as_i64() {
                        Some(next) => next_cursor(next)?.into(),
                        None => Value::Null,
                    };
                    return Ok(result);
                }
            }
        }
        // Evaluate source predicates once. Group exact identities in Core rather
        // than materializing two SQL grouping trees and a temporary join index.
        // The metadata projection is derived from this same writer connection.
        struct Matched {
            first_row: i64,
            other_rows: Vec<i64>,
            latest: String,
        }
        let states = self.conversation_states(account)?;
        let sql = search_matches_sql(!expression.is_empty(), p);
        let mut stmt = self.db.prepare_cached(&sql)?;
        let mut matches: Vec<Option<Matched>> =
            std::iter::repeat_with(|| None).take(states.len()).collect();
        let mut page = Vec::new();
        let mut rows = stmt.query(params![
            account,
            folder,
            category,
            expression,
            limit,
            offset,
            sender_pattern,
            status,
            attachment,
            after,
            before,
            exact_sender
        ])?;
        while let Some(row) = rows.next()? {
            if !exact_sender.is_empty() {
                let candidate = row
                    .get_ref(3)?
                    .as_str()
                    .map_err(|_| err("storage_error", "Invalid sender address"))?;
                if candidate.trim().to_lowercase() != exact_sender {
                    continue;
                }
            }
            let cid = row
                .get_ref(0)?
                .as_str()
                .map_err(|_| err("storage_error", "Invalid conversation identity"))?;
            let (cid, state) = states
                .get_key_value(cid)
                .ok_or_else(|| err("storage_error", "Matching conversation has no source state"))?;
            if kind != "all" && kind != kind_name(state.kind) {
                continue;
            }
            if !match status {
                "review" => state.unread,
                "held" => state.starred,
                "done" => !state.unread && !state.starred,
                _ => true,
            } {
                continue;
            }
            let rowid: i64 = row.get(1)?;
            let at = if grouped || recent {
                ""
            } else {
                row.get_ref(2)?
                    .as_str()
                    .map_err(|_| err("storage_error", "Invalid source timestamp"))?
            };
            match &mut matches[state.position] {
                None => {
                    matches[state.position] = Some(Matched {
                        first_row: rowid,
                        other_rows: Vec::new(),
                        latest: at.into(),
                    });
                    page.push((cid.as_str(), state));
                }
                Some(entry) => {
                    if at > entry.latest.as_str() {
                        entry.latest = at.into();
                    }
                    entry.other_rows.push(rowid);
                }
            }
        }
        let filtered_count = page.len();
        let mut group_counts = [0usize; 3];
        for (_, state) in &page {
            group_counts[if state.unread {
                0
            } else if state.starred {
                1
            } else {
                2
            }] += 1;
        }
        let group_rank = |s: &ConversationState| {
            if s.unread {
                0
            } else if s.starred {
                1
            } else {
                2
            }
        };
        if !collapsed.is_empty() {
            page.retain(|(_, state)| {
                !collapsed.contains(["review", "held", "done"][group_rank(state)])
            });
        }
        let expanded_count = page.len();
        let compare = |(aid, a): &(&str, &ConversationState),
                       (bid, b): &(&str, &ConversationState)| {
            let order = if grouped || recent {
                (if grouped {
                    group_rank(a).cmp(&group_rank(b))
                } else {
                    std::cmp::Ordering::Equal
                })
                .then_with(|| b.latest.total_cmp(&a.latest))
            } else {
                matches[b.position]
                    .as_ref()
                    .unwrap()
                    .latest
                    .cmp(&matches[a.position].as_ref().unwrap().latest)
            };
            order.then_with(|| aid.cmp(bid))
        };
        // Rank only the requested prefix; exact totals still use every match.
        let end = ((offset + limit) as usize).min(page.len());
        if end < page.len() {
            page.select_nth_unstable_by(end, &compare);
            page.truncate(end);
        }
        page.sort_unstable_by(compare);
        let more = (offset + limit) < expanded_count as i64;
        let page = page
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect::<Vec<_>>();
        let mut out = Vec::with_capacity(page.len());
        for (cid, state) in page {
            let matching = matches[state.position].take().unwrap();
            let matched: std::collections::HashSet<i64> = std::iter::once(matching.first_row)
                .chain(matching.other_rows)
                .collect();
            let (subject, revision): (String, i64) = self.db.query_row(
                "SELECT subject,revision FROM conversations WHERE account=?1 AND id=?2",
                params![account, cid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let mut members = self.db.prepare_cached("SELECT id,sent_at,unread,starred,rowid FROM messages WHERE account=?1 AND conversation_id=?2 ORDER BY julianday(sent_at),id LIMIT 201")?;
            let members = members
                .query_map(params![account, cid], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, bool>(2)?,
                        r.get::<_, bool>(3)?,
                        r.get::<_, i64>(4)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if members.is_empty() || members.len() > 200 {
                return Err(err(
                    "limit_exceeded",
                    "This experiment supports 1–200 members per conversation",
                ));
            }
            let matching: Vec<_> = members
                .iter()
                .rev()
                .filter(|m| matched.contains(&m.4))
                .map(|m| m.0.clone())
                .collect();
            let selected = matching
                .first()
                .ok_or_else(|| err("storage_error", "Search result has no matching member"))?;
            let (sender_name, sender_email, body): (String, String, String) = self.db.query_row(
                "SELECT sender_name,sender_email,body FROM messages WHERE account=?1 AND id=?2",
                params![account, selected],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            let mut categories = self.db.prepare_cached("SELECT DISTINCT c.id,c.name FROM message_categories mc JOIN messages m ON m.account=mc.account AND m.id=mc.message_id JOIN categories c ON c.account=mc.account AND c.id=mc.category_id WHERE m.account=?1 AND m.conversation_id=?2 ORDER BY c.id")?;
            let categories = categories
                .query_map(params![account, cid], |r| {
                    Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?}))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let group = if members.iter().any(|m| m.2) {
                "review"
            } else if members.iter().any(|m| m.3) {
                "held"
            } else {
                "done"
            };
            out.push(json!({"kind":kind_name(state.kind),"group":group,"id":cid,"subject":subject,"sender_name":sender_name,"sender_email":sender_email,"snippet":search_snippet(&body,&tokens),"latest_at":members.last().unwrap().1,"message_count":members.len(),"unread":members.iter().any(|m|m.2),"starred":members.iter().any(|m|m.3),"revision":revision,"categories":categories,"message_ids":members.iter().map(|m|m.0.clone()).collect::<Vec<_>>(),"matching_message_ids":if expression.is_empty() && category.is_empty() && sender.is_empty() && exact_sender.is_empty() && after.is_empty() && before.is_empty() && attachment.is_none(){Vec::<String>::new()}else{matching}}));
        }
        let result = json!({"conversations":out,"expanded_conversation_count":expanded_count,"filtered_group_counts":{"review":group_counts[0],"held":group_counts[1],"done":group_counts[2]},"filtered_conversation_count":filtered_count,"account_counts":self.account_counts(account)?,"revision":revision,"grouped":grouped,"date_filter_time_zone":"UTC","next_cursor":if more {Some(next_cursor(offset+limit)?)} else {None},"next_offset":if more{Some(offset+limit)}else{None},"coverage":self.coverage(account)?});
        if cache_first_page {
            self.projection_cache.borrow_mut().insert(
                first_page_key,
                (
                    search_generation,
                    json!({"fingerprint":fingerprint,"result":result.clone()}),
                ),
            );
        }
        Ok(result)
    }
    pub(crate) fn sender_summary(&self, account: &str, p: &Value) -> Result<Value> {
        fields(p, &["email"])?;
        let email = canonical_email(
            p.get("email")
                .and_then(Value::as_str)
                .ok_or_else(|| err("invalid_request", "Sender summary needs an email address"))?,
        )?;
        let result = self.conversations_ordered(
            account,
            &json!({"exact_sender":email,"folder":"all","limit":3}),
            true,
        )?;
        Ok(
            json!({"account":account,"email":email,"conversation_count":result["filtered_conversation_count"],"conversations":result["conversations"]}),
        )
    }
    pub(crate) fn conversation(&self, account: &str, cid: &str) -> Result<Value> {
        id(cid)?;
        let (subject, revision): (String, i64) = self
            .db
            .query_row(
                "SELECT subject,revision FROM conversations WHERE account=?1 AND id=?2",
                params![account, cid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| err("not_found", "Conversation not found in this account"))?;
        let mut stmt=self.db.prepare("SELECT m.id,sender_name,sender_email,recipient,subject,sent_at,body,unread,starred,revision,m.source_hash,COALESCE(k.kind,'unknown') FROM messages m LEFT JOIN message_sender_kinds k ON k.account=m.account AND k.message_id=m.id AND k.source_hash=m.source_hash WHERE m.account=?1 AND conversation_id=?2 ORDER BY julianday(sent_at),m.id LIMIT 201")?;
        let mut messages=stmt.query_map(params![account,cid],|r|Ok(json!({"id":r.get::<_,String>(0)?,"sender_name":r.get::<_,String>(1)?,"sender_email":r.get::<_,String>(2)?,"to":r.get::<_,String>(3)?,"subject":r.get::<_,String>(4)?,"sent_at":r.get::<_,String>(5)?,"text":r.get::<_,String>(6)?,"unread":r.get::<_,bool>(7)?,"starred":r.get::<_,bool>(8)?,"revision":r.get::<_,i64>(9)?,"source_hash":r.get::<_,String>(10)?,"kind":r.get::<_,String>(11)?})))?.collect::<std::result::Result<Vec<_>,_>>()?;
        if messages.is_empty() || messages.len() > 200 {
            return Err(err(
                "limit_exceeded",
                "This experiment supports 1–200 members per conversation",
            ));
        }
        let mut categories = std::collections::BTreeSet::new();
        for message in &mut messages {
            let mid = message["id"].as_str().unwrap().to_owned();
            let hash = message["source_hash"].as_str().unwrap();
            message["source_available"] = self.blob_present(hash).into();
            message.as_object_mut().unwrap().remove("source_hash");
            let mut stmt=self.db.prepare("SELECT id,name,mime_type,size,state FROM attachments WHERE account=?1 AND message_id=?2 ORDER BY id")?;
            message["attachments"]=stmt.query_map(params![account,mid],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"mime_type":r.get::<_,String>(2)?,"size":r.get::<_,i64>(3)?,"state":r.get::<_,String>(4)?})))?.collect::<std::result::Result<Vec<_>,_>>()?.into();
            let mut stmt=self.db.prepare("SELECT category_id FROM message_categories WHERE account=?1 AND message_id=?2 ORDER BY category_id")?;
            let labels = stmt
                .query_map(params![account, mid], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            categories.extend(labels.clone());
            message["categories"] = labels.into();
        }
        let categories = categories
            .into_iter()
            .map(|cid| {
                let name: String = self.db.query_row(
                    "SELECT name FROM categories WHERE account=?1 AND id=?2",
                    params![account, cid],
                    |r| r.get(0),
                )?;
                Ok(json!({"id":cid,"name":name}))
            })
            .collect::<Result<Vec<_>>>()?;
        let ids: Vec<_> = messages.iter().map(|m| m["id"].clone()).collect();
        // The reader and mutation receipt need only this conversation's kind.
        // Reuse the list projection's exact incoming/date/ID/source rules without
        // rebuilding an entire account, particularly inside a flag transaction.
        let state = self.conversation_state(account, cid)?;
        let kind = kind_name(state.kind);
        Ok(
            json!({"kind":kind,"id":cid,"subject":subject,"revision":revision,"message_ids":ids,"categories":categories,"messages":messages}),
        )
    }
    pub(crate) fn categories(&self, account: &str) -> Result<Value> {
        let stamp = self.mail_changes.snapshot(&self.db).search;
        self.cached_projection(account, "categories", stamp, || {
            let mut stmt=self.db.prepare("SELECT c.id,c.name,COUNT(DISTINCT m.conversation_id) FROM categories c LEFT JOIN message_categories mc ON mc.account=c.account AND mc.category_id=c.id LEFT JOIN messages m ON m.account=mc.account AND m.id=mc.message_id WHERE c.account=?1 GROUP BY c.id,c.name ORDER BY c.name")?;
            Ok(json!({"categories":stmt.query_map([account],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"conversation_count":r.get::<_,i64>(2)?})))?.collect::<std::result::Result<Vec<_>,_>>()?}))
        })
    }
    pub(crate) fn view(&self, account: &str) -> Result<Value> {
        let found = self
            .db
            .query_row(
                "SELECT revision,json FROM views WHERE account=?1",
                [account],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?;
        let (revision, view) = match found {
            Some((r, j)) => (r, serde_json::from_str::<Value>(&j)?),
            None => (0, json!({})),
        };
        Ok(json!({"account":account,"revision":revision,"view":view}))
    }
    pub(crate) fn calendar(&self, account: &str, p: &Value) -> Result<Value> {
        fields(p, &["from", "to"])?;
        let synthetic = self.profile_kind()? == crate::live_profile::ProfileKind::Synthetic;
        if !synthetic && (p.get("from").is_none() || p.get("to").is_none()) {
            return Err(err(
                "invalid_request",
                "Live calendar queries need explicit from and to dates",
            ));
        }
        let from = optional_string(p, "from", "2026-09-07")?;
        let to = optional_string(p, "to", "2026-09-14")?;
        let parse = |s: &str| {
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map_err(|_| err("invalid_request", "Calendar needs YYYY-MM-DD dates"))
        };
        let (start, end) = (parse(from)?, parse(to)?);
        if end <= start || (end - start).num_days() > 42 {
            return Err(err("invalid_request", "Calendar range must be 1–42 days"));
        }
        let events = self.project_cached_calendars(account, start, end)?;
        Ok(
            json!({"events":events,"from":from,"to":to,"time_zone":self.calendar_display_zone()?,"fixture_today":if synthetic {Some(FIXTURE_TODAY)} else {None}}),
        )
    }
}
fn optional_string<'a>(p: &'a Value, key: &str, default: &'a str) -> Result<&'a str> {
    match p.get(key) {
        None => Ok(default),
        Some(v) => v
            .as_str()
            .ok_or_else(|| err("invalid_request", "Expected a string")),
    }
}
fn number(p: &Value, key: &str, default: i64, min: i64, max: i64) -> Result<i64> {
    let n = match p.get(key) {
        Some(v) => v
            .as_i64()
            .ok_or_else(|| err("invalid_request", "Expected an integer"))?,
        None => default,
    };
    if n < min || n > max {
        return Err(err("invalid_request", "Number is outside permitted bounds"));
    }
    Ok(n)
}

fn search_snippet(body: &str, tokens: &[&str]) -> String {
    let lower = body.to_lowercase();
    let position = tokens
        .iter()
        .filter_map(|token| lower.find(&token.to_lowercase()))
        .min();
    let start = position
        .map(|p| lower[..p].chars().count().saturating_sub(60))
        .unwrap_or(0);
    let mut chars = body.chars().skip(start);
    let text: String = chars.by_ref().take(180).collect();
    format!(
        "{}{}{}",
        if start > 0 { "… " } else { "" },
        text,
        if chars.next().is_some() { " …" } else { "" }
    )
}

// Structured filters combine on the same source message; status considers the whole conversation.
pub(crate) fn validate_filters(p: &Value) -> Result<()> {
    if let Some(value) = p.get("exact_sender") {
        canonical_email(
            value
                .as_str()
                .ok_or_else(|| err("invalid_request", "Exact sender must be an email address"))?,
        )?;
    }
    if !["all", "people", "automated", "unknown"].contains(&optional_string(p, "kind", "all")?) {
        return Err(err("invalid_request", "Unknown sender kind"));
    }
    let sender = optional_string(p, "sender", "")?;
    if sender.len() > 256 || sender.chars().any(char::is_control) {
        return Err(err(
            "invalid_request",
            "Sender filter exceeds 256 bytes or contains control characters",
        ));
    }
    if !["all", "review", "held", "done"].contains(&optional_string(p, "status", "all")?) {
        return Err(err("invalid_request", "Unknown review status"));
    }
    if p.get("has_attachment").is_some_and(|v| !v.is_boolean()) {
        return Err(err("invalid_request", "has_attachment must be boolean"));
    }
    let after = optional_string(p, "after", "")?;
    let before = optional_string(p, "before", "")?;
    for date in [after, before] {
        if !date.is_empty()
            && (date.len() != 10 || chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err())
        {
            return Err(err(
                "invalid_request",
                "Date filters require YYYY-MM-DD in UTC",
            ));
        }
    }
    if !after.is_empty() && !before.is_empty() && after >= before {
        return Err(err("invalid_request", "after must be earlier than before"));
    }
    Ok(())
}
fn canonical_email(email: &str) -> Result<String> {
    let trimmed = email.trim();
    if email.len() > 256
        || trimmed.is_empty()
        || email.chars().any(char::is_control)
        || fm_mail::validate_address(&fm_mail::Address {
            name: None,
            email: trimmed.into(),
        })
        .is_err()
    {
        return Err(err(
            "invalid_request",
            "Sender must be a valid email address of at most 256 bytes",
        ));
    }
    Ok(trimmed.to_lowercase())
}
pub(crate) fn search_matches_sql(search: bool, p: &Value) -> String {
    // Only add predicates actually requested. Bound optional OR expressions kept
    // sender/body columns in the old query's scan even for unfiltered listings.
    let mut predicate = "m.account=?1".to_owned();
    match p["folder"].as_str().unwrap_or("inbox") {
        "inbox" => predicate.push_str(" AND m.inbox=1"),
        "sent" => predicate.push_str(" AND m.sent=1"),
        _ => {}
    }
    if p["category"].as_str().is_some_and(|s| !s.is_empty()) {
        predicate.push_str(" AND m.id IN (SELECT message_id FROM message_categories WHERE account=?1 AND category_id=?3)");
    }
    if search {
        predicate.push_str(" AND message_fts MATCH ?4");
    }
    if p["sender"].as_str().is_some_and(|s| !s.is_empty()) {
        predicate.push_str(
            " AND (m.sender_name LIKE ?7 ESCAPE '\\' OR m.sender_email LIKE ?7 ESCAPE '\\')",
        );
    }
    if p.get("exact_sender").is_some() {
        // SQLite NOCASE covers the common ASCII candidate path. The pinned
        // mailbox parser also accepts Unicode domains, so retain non-ASCII
        // candidates for the same bounded Rust lowercase comparison as input.
        predicate.push_str(
            " AND (trim(m.sender_email)=?12 COLLATE NOCASE OR m.sender_email GLOB '*[^ -~]*')",
        );
    }
    if let Some(has) = p["has_attachment"].as_bool() {
        predicate.push_str(if has {
            " AND m.id IN (SELECT message_id FROM attachments WHERE account=?1)"
        } else {
            " AND m.id NOT IN (SELECT message_id FROM attachments WHERE account=?1)"
        });
    }
    if p["after"].as_str().is_some_and(|s| !s.is_empty()) {
        predicate.push_str(" AND julianday(m.sent_at)>=julianday(?10)");
    }
    if p["before"].as_str().is_some_and(|s| !s.is_empty()) {
        predicate.push_str(" AND julianday(m.sent_at)<julianday(?11)");
    }
    // Reserve stable numeric slots without forcing unrequested columns into scans.
    let source = if search {
        "message_fts JOIN messages m ON m.rowid=message_fts.rowid"
    } else {
        "messages m"
    };
    let exact_column = if p.get("exact_sender").is_some() {
        "m.sender_email"
    } else {
        "''"
    };
    format!("WITH bindings AS (SELECT ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12) SELECT m.conversation_id,m.rowid,m.sent_at,{exact_column} FROM {source} WHERE {predicate}")
}

#[cfg(test)]
mod projection_tests {
    use super::*;
    #[test]
    fn first_page_navigation_reuses_mail_without_source_reads_and_refreshes_cursor_revision() {
        use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path()).unwrap();
        let params = json!({"folder":"inbox","grouped":true,"limit":2});
        let first = store.conversations("windscribe", &params).unwrap();
        store
            .handle(
                "view_save",
                Some("windscribe"),
                json!({"view":{"readerY":12},
            "expected_revision":0,"request_id":"cached-page-navigation"}),
            )
            .unwrap();
        // The warm request must remain usable when SQLite explicitly refuses
        // another source-table read. This proves actual scan avoidance rather
        // than merely comparing two possibly recomputed projections.
        store
            .db
            .authorizer(Some(|context: AuthContext<'_>| match context.action {
                AuthAction::Read {
                    table_name: "messages" | "conversations",
                    ..
                } => Authorization::Deny,
                _ => Authorization::Allow,
            }));
        let warm = store.conversations("windscribe", &params).unwrap();
        assert_eq!(warm["conversations"], first["conversations"]);
        // Row revisions belong to individual conversations, not to the outer
        // global store revision advanced by the view write.
        for (row, original) in warm["conversations"]
            .as_array()
            .unwrap()
            .iter()
            .zip(first["conversations"].as_array().unwrap())
        {
            assert_eq!(row["revision"], original["revision"]);
        }
        assert_eq!(warm["coverage"], first["coverage"]);
        assert_eq!(warm["revision"], store.revision().unwrap());
        assert_ne!(warm["revision"], first["revision"]);
        let cursor: Value = serde_json::from_str(warm["next_cursor"].as_str().unwrap()).unwrap();
        assert_eq!(cursor["revision"], warm["revision"]);
        assert_eq!(cursor["offset"], 2);
        assert!(store.conversations("personal", &params).is_err());
        assert!(store
            .conversations(
                "windscribe",
                &json!({"folder":"sent","grouped":true,"limit":2})
            )
            .is_err());
        store
            .db
            .authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
        let mut next = params.clone();
        next["cursor"] = warm["next_cursor"].clone();
        let next = store.conversations("windscribe", &next).unwrap();
        assert_eq!(next["conversations"].as_array().unwrap().len(), 2);
        for row in next["conversations"].as_array().unwrap() {
            assert!(first["conversations"]
                .as_array()
                .unwrap()
                .iter()
                .all(|old| old["id"] != row["id"]));
        }
        // The old cursor remains valid through presentation-only writes too.
        let mut old_next = params.clone();
        old_next["cursor"] = first["next_cursor"].clone();
        assert_eq!(store.conversations("windscribe", &old_next).unwrap(), next);
    }
    #[test]
    fn first_page_cache_follows_flags_source_edits_rollback_filters_and_account_scope() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path()).unwrap();
        let params = json!({"folder":"inbox","grouped":true,"limit":100});
        let check = |store: &Store, account: &str| {
            let actual = store.conversations(account, &params).unwrap();
            let mut statement = store.db.prepare("SELECT c.id,c.subject,MAX(m.unread),MAX(m.starred) FROM conversations c JOIN messages m ON m.account=c.account AND m.conversation_id=c.id WHERE c.account=?1 AND EXISTS(SELECT 1 FROM messages source WHERE source.account=c.account AND source.conversation_id=c.id AND source.inbox=1) GROUP BY c.id ORDER BY CASE WHEN MAX(m.unread) THEN 0 WHEN MAX(m.starred) THEN 1 ELSE 2 END,MAX(julianday(m.sent_at)) DESC,c.id LIMIT 100").unwrap();
            let expected = statement
                .query_map([account], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, bool>(2)?,
                        row.get::<_, bool>(3)?,
                    ))
                })
                .unwrap()
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap();
            let rows = actual["conversations"].as_array().unwrap();
            assert_eq!(rows.len(), expected.len());
            for (row, (id, subject, unread, starred)) in rows.iter().zip(expected) {
                assert_eq!(row["id"], id);
                assert_eq!(row["subject"], subject);
                assert_eq!(row["unread"], unread);
                assert_eq!(row["starred"], starred);
            }
            actual
        };
        let original = check(&store, "windscribe");
        let other = check(&store, "personal");
        let before = store
            .conversation("windscribe", "windscribe-thread-000")
            .unwrap();
        store.handle("flag", Some("windscribe"), json!({"id":before["id"], "message_ids":before["message_ids"],
            "expected_revision":before["revision"],"request_id":"cache-page-flag","read":true,"starred":false})).unwrap();
        let flagged = check(&store, "windscribe");
        assert_ne!(flagged["conversations"], original["conversations"]);
        assert_eq!(
            check(&store, "personal")["conversations"],
            other["conversations"]
        );
        store.db.execute_batch("BEGIN; UPDATE messages SET unread=1,starred=1 WHERE account='windscribe' AND conversation_id='windscribe-thread-000'; UPDATE conversations SET subject='Uncommitted source edit' WHERE account='windscribe' AND id='windscribe-thread-000';").unwrap();
        assert_ne!(
            check(&store, "windscribe")["conversations"],
            flagged["conversations"]
        );
        store.db.execute_batch("ROLLBACK;").unwrap();
        assert_eq!(
            check(&store, "windscribe")["conversations"],
            flagged["conversations"]
        );
        store.db.execute("UPDATE conversations SET subject='Committed source edit' WHERE account='windscribe' AND id='windscribe-thread-000'", []).unwrap();
        assert_ne!(
            check(&store, "windscribe")["conversations"],
            flagged["conversations"]
        );
        let filtered = store
            .conversations(
                "windscribe",
                &json!({"folder":"inbox","grouped":true,"limit":100,"status":"review"}),
            )
            .unwrap();
        assert!(filtered["conversations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["unread"] == true));
        check(&store, "windscribe");
        assert_eq!(
            store
                .projection_cache
                .borrow()
                .keys()
                .filter(|(_, key)| *key == "first_conversation_page")
                .count(),
            2
        );
    }
    fn append(store: &Store, id: &str, cid: &str) {
        store.db.execute("INSERT OR IGNORE INTO conversations(account,id,subject,revision) VALUES('windscribe',?1,'Cache fixture',1)",[cid]).unwrap();
        store.db.execute("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash,revision) VALUES('windscribe',?1,?2,?1,'Person','person@example.test','you@example.test','Cache fixture','Source text','2026-09-12T00:00:00Z',1,1,1,0,'',1)",params![id,cid]).unwrap();
        store.db.execute("INSERT INTO message_fts(rowid,sender,subject,body) VALUES(?1,'Person person@example.test','Cache fixture','Source text')",[store.db.last_insert_rowid()]).unwrap();
    }
    fn correct(store: &Store) {
        for account in ["windscribe", "personal"] {
            let actual = store.account_counts(account).unwrap();
            let expected:(i64,i64,i64,i64)=store.db.query_row("SELECT COUNT(*),COALESCE(SUM(u),0),COALESCE(SUM(s),0),COALESCE(SUM(NOT u AND NOT s),0) FROM (SELECT MAX(unread) u,MAX(starred) s FROM messages WHERE account=?1 GROUP BY conversation_id)",[account],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
            assert_eq!(actual["conversation_count"], expected.0);
            assert_eq!(actual["unread_conversation_count"], expected.1);
            assert_eq!(actual["held_conversation_count"], expected.2);
            assert_eq!(actual["done_conversation_count"], expected.3);
            let expected_messages:(i64,i64,i64)=store.db.query_row("SELECT COALESCE(SUM(unread),0),COALESCE(SUM(starred),0),COUNT(*) FROM messages WHERE account=?1",[account],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
            assert_eq!(
                store.account_message_counts(account).unwrap(),
                expected_messages
            );
            let indexed:i64=store.db.query_row("SELECT COUNT(*) FROM messages m JOIN message_fts f ON f.rowid=m.rowid WHERE m.account=?1",[account],|r|r.get(0)).unwrap();
            let coverage = store.coverage(account).unwrap();
            assert_eq!(coverage["indexed_messages"], indexed);
            let (messages,sources):(i64,i64)=store.db.query_row("SELECT COUNT(*),COALESCE(SUM(source_hash<>''),0) FROM messages WHERE account=?1",[account],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
            let (attachments,available):(i64,i64)=store.db.query_row("SELECT COUNT(*),COALESCE(SUM(state='available'),0) FROM attachments WHERE account=?1",[account],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
            assert_eq!(coverage["messages"], messages);
            assert_eq!(coverage["retained_source_records"], sources);
            assert_eq!(coverage["attachments"], attachments);
            assert_eq!(coverage["recorded_available_attachments"], available);
        }
    }
    #[test]
    fn category_counts_survive_navigation_and_follow_direct_membership_changes_and_rollback() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path()).unwrap();
        let original = store.categories("windscribe").unwrap();
        let other = store.categories("personal").unwrap();
        let key = ("windscribe".to_owned(), "categories");
        let stamp = store.projection_cache.borrow()[&key].0;
        store
            .handle(
                "view_save",
                Some("windscribe"),
                json!({"view":{"query":""},"expected_revision":0,"request_id":"category-nav"}),
            )
            .unwrap();
        assert_eq!(store.categories("windscribe").unwrap(), original);
        assert_eq!(store.projection_cache.borrow()[&key].0, stamp);
        store
            .db
            .execute_batch("BEGIN; DELETE FROM message_categories WHERE account='windscribe';")
            .unwrap();
        assert!(store.categories("windscribe").unwrap()["categories"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["conversation_count"] == 0));
        store.db.execute_batch("ROLLBACK;").unwrap();
        assert_eq!(store.categories("windscribe").unwrap(), original);
        store
            .db
            .execute(
                "DELETE FROM message_categories WHERE account='windscribe'",
                [],
            )
            .unwrap();
        assert!(store.categories("windscribe").unwrap()["categories"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["conversation_count"] == 0));
        assert_eq!(store.categories("personal").unwrap(), other);
    }
    #[test]
    fn committed_flags_preserve_other_accounts_and_source_coverage_without_promoting_stale_caches()
    {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path()).unwrap();
        correct(&store);
        let other = store.conversation_states("personal").unwrap();
        let original = store.conversation_states("windscribe").unwrap();
        let coverage = store.coverage("windscribe").unwrap();
        let before = store
            .conversation("windscribe", "windscribe-thread-000")
            .unwrap();
        let command = json!({"id":before["id"],"message_ids":before["message_ids"],
            "expected_revision":before["revision"],"request_id":"scoped-flag","read":true,"starred":false});
        let receipt = store
            .handle("flag", Some("windscribe"), command.clone())
            .unwrap();
        let changed = store.conversation_states("windscribe").unwrap();
        assert!(original["windscribe-thread-000"].unread);
        assert!(!changed["windscribe-thread-000"].unread);
        assert!(!changed["windscribe-thread-000"].starred);
        assert!(Arc::ptr_eq(
            &other,
            &store.conversation_states("personal").unwrap()
        ));
        assert_eq!(store.coverage("windscribe").unwrap(), coverage);
        assert_eq!(
            store.handle("flag", Some("windscribe"), command).unwrap(),
            receipt
        );
        assert!(Arc::ptr_eq(
            &changed,
            &store.conversation_states("windscribe").unwrap()
        ));
        correct(&store);

        // A write outside the narrow flag operation invalidates the old stamp.
        // Neither its changed source coverage nor its former flags may be reused.
        store.db.execute("UPDATE messages SET unread=0,source_hash='' WHERE account='personal' AND id='personal-message-000-000'", []).unwrap();
        let next = store
            .conversation("windscribe", "windscribe-thread-001")
            .unwrap();
        store
            .handle(
                "flag",
                Some("windscribe"),
                json!({"id":next["id"],"message_ids":next["message_ids"],
            "expected_revision":next["revision"],"request_id":"flag-after-external","read":true}),
            )
            .unwrap();
        assert!(!Arc::ptr_eq(
            &other,
            &store.conversation_states("personal").unwrap()
        ));
        correct(&store);

        // A post-update receipt failure rolls the mutation back. Cached states
        // must come from restored rows, never from an uncommitted flag response.
        let before_failure = store
            .conversation("windscribe", "windscribe-thread-002")
            .unwrap();
        store.db.execute_batch("CREATE TEMP TRIGGER fail_flag_receipt BEFORE INSERT ON operations WHEN NEW.request_id='failed-scoped-flag' BEGIN SELECT RAISE(ABORT,'controlled receipt failure'); END;").unwrap();
        assert!(store.handle("flag", Some("windscribe"), json!({"id":before_failure["id"],"message_ids":before_failure["message_ids"],
            "expected_revision":before_failure["revision"],"request_id":"failed-scoped-flag","read":true,"starred":false})).is_err());
        assert_eq!(
            store
                .conversation("windscribe", "windscribe-thread-002")
                .unwrap(),
            before_failure
        );
        correct(&store);
    }
    #[test]
    fn reader_and_flag_receipts_keep_kind_without_loading_an_account_projection() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path()).unwrap();
        for (cid, expected_kind) in [
            ("windscribe-thread-000", "people"),
            ("windscribe-thread-005", "automated"),
        ] {
            let before = store
                .handle("conversation", Some("windscribe"), json!({"id":cid}))
                .unwrap();
            assert_eq!(before["kind"], expected_kind);
            assert!(store.conversation_cache.borrow().is_empty());
            let receipt = store
                .handle(
                    "flag",
                    Some("windscribe"),
                    json!({"id":cid,"message_ids":[before["message_ids"][0]],
                        "expected_revision":before["revision"],"request_id":format!("kind-receipt-{cid}"),"read":true}),
                )
                .unwrap();
            assert_eq!(receipt["kind"], expected_kind);
            assert_eq!(receipt["message_ids"], before["message_ids"]);
            assert_eq!(receipt["messages"][0]["unread"], false);
            for (old, new) in before["messages"]
                .as_array()
                .unwrap()
                .iter()
                .zip(receipt["messages"].as_array().unwrap())
                .skip(1)
            {
                assert_eq!(old, new);
            }
            assert!(store.conversation_cache.borrow().is_empty());
            assert_eq!(
                store
                    .handle("conversation", Some("windscribe"), json!({"id":cid}))
                    .unwrap(),
                receipt
            );
        }
        correct(&store);
    }
    #[test]
    fn projection_cache_skips_navigation_writes_and_handles_appends_replacements_and_rollback() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path()).unwrap();
        correct(&store);
        let original = store.conversation_states("windscribe").unwrap();
        store
            .handle(
                "view_save",
                Some("windscribe"),
                json!({"view":{"query":"cache"},"expected_revision":0,"request_id":"view-cache"}),
            )
            .unwrap();
        store.handle("preferences_save",None,json!({"preferences":{"density":"compact"},"expected_revision":0,"request_id":"pref-cache"})).unwrap();
        store.handle("draft_save",Some("windscribe"),json!({"conversation_id":"windscribe-thread-001","body":"Human text","expected_revision":0,"request_id":"draft-cache"})).unwrap();
        assert!(Arc::ptr_eq(
            &original,
            &store.conversation_states("windscribe").unwrap()
        ));
        drop(original);
        append(&store, "cache-one", "cache-conversation");
        correct(&store);
        append(&store, "cache-two", "cache-conversation");
        correct(&store);
        store
            .db
            .execute_batch(
                "BEGIN; UPDATE messages SET unread=0,starred=0 WHERE id='cache-one'; ROLLBACK;",
            )
            .unwrap();
        correct(&store);
        // REPLACE has an unhooked deletion; the inventory check must rebuild the
        // former conversation as well as the replacement's new identity.
        store.db.execute("INSERT OR REPLACE INTO messages SELECT NULL,account,id,'windscribe-thread-001',remote_id,sender_name,sender_email,recipient,subject,body,sent_at,0,0,inbox,sent,source_hash,revision FROM messages WHERE id='cache-one'",[]).unwrap();
        correct(&store);
        store
            .db
            .execute("DELETE FROM messages WHERE id='cache-two'", [])
            .unwrap();
        correct(&store);
        store.db.execute("DELETE FROM message_fts WHERE rowid=(SELECT rowid FROM messages WHERE id='cache-one')",[]).unwrap();
        correct(&store);
    }
    #[test]
    fn projection_reads_inside_transactions_never_survive_rollback_or_savepoint_restore() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        correct(&store);
        store.db.execute_batch("BEGIN").unwrap();
        append(&store, "rolled-back", "rollback-thread");
        correct(&store);
        store.db.execute_batch("ROLLBACK").unwrap();
        correct(&store);
        store
            .db
            .execute_batch(
                "BEGIN; SAVEPOINT snapshot; UPDATE messages SET unread=0,starred=0,source_hash='';",
            )
            .unwrap();
        correct(&store);
        store.db.execute_batch("ROLLBACK TO snapshot").unwrap();
        correct(&store);
        store.db.execute_batch("COMMIT").unwrap();
        correct(&store);
    }
    #[test]
    fn coverage_append_deltas_preserve_independent_index_source_and_attachment_counts() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        correct(&store);
        append(&store, "coverage-one", "coverage-thread");
        correct(&store);
        let rowid: i64 = store
            .db
            .query_row(
                "SELECT rowid FROM messages WHERE id='coverage-one'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        store
            .db
            .execute("DELETE FROM message_fts WHERE rowid=?1", [rowid])
            .unwrap();
        correct(&store);
        // Backfill an older index identity; its rowid is below the high-water
        // mark, so correctness requires the full-inventory fallback.
        append(&store, "coverage-two", "coverage-thread");
        correct(&store);
        store.db.execute("INSERT INTO message_fts(rowid,sender,subject,body) VALUES(?1,'Person','Backfill','Source')",[rowid]).unwrap();
        correct(&store);
        store
            .db
            .execute(
                "UPDATE messages SET source_hash='retained' WHERE id='coverage-one'",
                [],
            )
            .unwrap();
        store
            .db
            .execute(
                "UPDATE attachments SET state='missing' WHERE account='windscribe'",
                [],
            )
            .unwrap();
        correct(&store);
        // Hidden FTS replacement/deletion and attachment truncate mixed with
        // normal append must not be mistaken for a pure addition batch.
        store.db.execute_batch("DELETE FROM attachments").unwrap();
        store
            .db
            .execute("DELETE FROM message_fts WHERE rowid=?1", [rowid])
            .unwrap();
        append(&store, "coverage-three", "coverage-thread");
        correct(&store);
    }
    #[test]
    fn cursor_survives_non_search_writes_and_rejects_search_changes_and_writer_restart() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path()).unwrap();
        let first = store
            .conversations("windscribe", &json!({"limit":2,"grouped":true}))
            .unwrap();
        let continuation = json!({"limit":2,"grouped":true,"cursor":first["next_cursor"]});
        let expected =
            store.conversations("windscribe", &continuation).unwrap()["conversations"].clone();
        store
            .handle(
                "view_save",
                Some("windscribe"),
                json!({"view":{"query":"Person"},"expected_revision":0,"request_id":"cursor-view"}),
            )
            .unwrap();
        store.handle("preferences_save",None,json!({"preferences":{"density":"compact"},"expected_revision":0,"request_id":"cursor-prefs"})).unwrap();
        store.handle("draft_save",Some("windscribe"),json!({"conversation_id":"windscribe-thread-001","body":"Human text","expected_revision":0,"request_id":"cursor-draft"})).unwrap();
        assert_eq!(
            store.conversations("windscribe", &continuation).unwrap()["conversations"],
            expected
        );
        for statement in [
            "UPDATE messages SET unread=1-unread WHERE account='windscribe' AND id='windscribe-message-001-000'",
            "UPDATE categories SET name=name WHERE account='windscribe'",
            "UPDATE attachments SET state=state WHERE account='windscribe'",
            "DELETE FROM message_fts WHERE rowid=(SELECT MIN(rowid) FROM messages)",
        ] {
            let first=store.conversations("windscribe",&json!({"limit":2,"grouped":true})).unwrap();
            store.db.execute(statement,[]).unwrap();
            let error=store.conversations("windscribe",&json!({"limit":2,"grouped":true,"cursor":first["next_cursor"]})).unwrap_err();
            assert_eq!(error.code,"stale_cursor","{statement}");
        }
        let first = store
            .conversations("windscribe", &json!({"limit":2,"grouped":true}))
            .unwrap();
        drop(store);
        let store = Store::open(temp.path()).unwrap();
        assert_eq!(
            store
                .conversations(
                    "windscribe",
                    &json!({"limit":2,"grouped":true,"cursor":first["next_cursor"]})
                )
                .unwrap_err()
                .code,
            "stale_cursor"
        );
    }
    #[test]
    fn projection_cache_rebuilds_after_delta_overflow_and_unhooked_truncate() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        correct(&store);
        let tx = store.db.unchecked_transaction().unwrap();
        for i in 0..514 {
            append(&store, &format!("cache-{i}"), "cache-overflow");
        }
        tx.commit().unwrap();
        correct(&store);
        store.db.execute_batch("DELETE FROM message_categories; DELETE FROM attachments; DELETE FROM messages; DELETE FROM message_fts;").unwrap();
        correct(&store);
        assert_eq!(
            store.account_counts("windscribe").unwrap()["conversation_count"],
            0
        );
    }
}
