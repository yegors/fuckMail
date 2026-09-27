//! Mail read queries: accounts and their counts, the conversation list with
//! search, filters, grouping and paging, the reader, labels and the
//! per-account calendar view.
//!
//! Each account's per-conversation state (folders, unread, starred, latest
//! date, sender kind) is projected into memory once. TEMP triggers then log
//! every conversation a write may have changed, so later reads refresh only
//! those. An SQLite update hook counts writes into generations that key the
//! derived caches (counts, coverage, the first list page) and that clients
//! poll through `status`.
use crate::*;
mod ui_changes;
use rusqlite::params;
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
/// One conversation's list-relevant state, folded from its messages.
#[derive(Clone)]
pub(crate) struct ConversationState {
    /// Stable slot in the account projection. Slots are never reused while
    /// the projection lives, so removed conversations leave gaps.
    position: usize,
    /// Folder membership bits: 1 all (not in Trash), 2 Inbox, 4 Sent,
    /// 8 Trash, 16 received. A conversation is in a folder when any of its
    /// messages is.
    folders: u8,
    unread: bool,
    starred: bool,
    unread_messages: i64,
    starred_messages: i64,
    /// Julian day of the newest message; 0 when no message has a date.
    latest: f64,
    /// Sender kind of the newest incoming message (sent mail only when
    /// nothing was received): 0 unknown, 1 people, 2 automated.
    kind: u8,
    kind_incoming: bool,
    kind_at: f64,
    kind_id: String,
    messages: i64,
    max_rowid: i64,
}
impl ConversationState {
    fn empty(position: usize) -> Self {
        Self {
            position,
            messages: 0,
            max_rowid: 0,
            folders: 0,
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
    /// Folds one message row, in `STATE_SELECT` column order, into the state.
    fn absorb(&mut self, row: &rusqlite::Row<'_>) -> rusqlite::Result<()> {
        let unread = row.get::<_, bool>(1)?;
        let starred = row.get::<_, bool>(2)?;
        self.messages += 1;
        self.max_rowid = self.max_rowid.max(row.get(4)?);
        self.unread |= unread;
        self.starred |= starred;
        self.unread_messages += i64::from(unread);
        self.starred_messages += i64::from(starred);
        let at = row.get::<_, Option<f64>>(3)?.unwrap_or(0.0);
        self.latest = self.latest.max(at);
        let incoming = !row.get::<_, bool>(6)?;
        let inbox = row.get::<_, bool>(8)?;
        let trash = row.get::<_, bool>(9)?;
        self.folders |= if trash {
            8
        } else {
            1 | if incoming {
                16 | if inbox { 2 } else { 0 }
            } else {
                4
            }
        };
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
/// Per-message rows for one account's state projection. A sender kind counts
/// only while it was computed from the message's current source.
const STATE_SELECT:&str = "SELECT m.conversation_id,m.unread,m.starred,julianday(m.sent_at),m.rowid,m.id,m.sent,CASE k.kind WHEN 'people' THEN 1 WHEN 'automated' THEN 2 ELSE 0 END,m.inbox,m.trashed FROM messages m LEFT JOIN message_sender_kinds k ON k.account=m.account AND k.message_id=m.id AND k.source_hash=m.source_hash WHERE m.account=?1";
/// One account's cached conversation states and the change-log position
/// they reflect.
pub(crate) struct ConversationProjection {
    /// Last `mail_state_log` entry already applied to `states`.
    log_seq: i64,
    /// Bumped whenever `states` changes; keys derived projections.
    version: u64,
    next_position: usize,
    message_count: i64,
    max_rowid: i64,
    states: Arc<HashMap<String, ConversationState>>,
}
/// Above this many changed conversations one scan is cheaper than per-thread reads.
const INCREMENTAL_STATE_LIMIT: usize = 512;
/// Consumed change-log entries are dropped once this many have accumulated.
const STATE_LOG_PRUNE: i64 = 4096;
/// TEMP triggers record every conversation whose projected state may have
/// changed. They live and roll back with this connection's transactions, and
/// FTS or WITHOUT ROWID writes elsewhere no longer force a full rescan.
/// Recursive triggers make REPLACE's implicit deletion fire the delete trigger;
/// no schema trigger writes its own table, so nothing else recurses.
pub(crate) const STATE_LOG_SQL: &str = "PRAGMA temp_store=MEMORY;
PRAGMA recursive_triggers=ON;
CREATE TEMP TABLE mail_state_log(seq INTEGER PRIMARY KEY AUTOINCREMENT,account TEXT NOT NULL,conversation_id TEXT NOT NULL);
CREATE TEMP TRIGGER mail_state_insert AFTER INSERT ON messages BEGIN
 INSERT INTO mail_state_log(account,conversation_id) VALUES(NEW.account,NEW.conversation_id);
END;
CREATE TEMP TRIGGER mail_state_update AFTER UPDATE OF rowid,account,id,conversation_id,sent_at,unread,starred,inbox,sent,trashed,source_hash ON messages BEGIN
 INSERT INTO mail_state_log(account,conversation_id) VALUES(OLD.account,OLD.conversation_id);
 INSERT INTO mail_state_log(account,conversation_id) SELECT NEW.account,NEW.conversation_id WHERE NEW.account IS NOT OLD.account OR NEW.conversation_id IS NOT OLD.conversation_id;
END;
CREATE TEMP TRIGGER mail_state_delete AFTER DELETE ON messages BEGIN
 INSERT INTO mail_state_log(account,conversation_id) VALUES(OLD.account,OLD.conversation_id);
END;
CREATE TEMP TRIGGER mail_kind_insert AFTER INSERT ON message_sender_kinds BEGIN
 INSERT INTO mail_state_log(account,conversation_id) SELECT account,conversation_id FROM messages WHERE account=NEW.account AND id=NEW.message_id;
END;
CREATE TEMP TRIGGER mail_kind_update AFTER UPDATE ON message_sender_kinds BEGIN
 INSERT INTO mail_state_log(account,conversation_id) SELECT account,conversation_id FROM messages WHERE account=OLD.account AND id=OLD.message_id;
 INSERT INTO mail_state_log(account,conversation_id) SELECT account,conversation_id FROM messages WHERE account=NEW.account AND id=NEW.message_id AND (NEW.account IS NOT OLD.account OR NEW.message_id IS NOT OLD.message_id);
END;
CREATE TEMP TRIGGER mail_kind_delete AFTER DELETE ON message_sender_kinds BEGIN
 INSERT INTO mail_state_log(account,conversation_id) SELECT account,conversation_id FROM messages WHERE account=OLD.account AND id=OLD.message_id;
END;";
/// Conversation-state projections by account.
pub(crate) type ConversationCache = RefCell<HashMap<String, ConversationProjection>>;
/// Derived replies by `(account, name)`, each with the stamp it was built at.
/// A lookup with a different stamp rebuilds.
pub(crate) type ProjectionCache = RefCell<HashMap<(String, &'static str), (u64, Value)>>;
/// A synthetic-profile coverage reply plus the inventories it was built from,
/// so appended messages and index rows can be applied as a delta.
pub(crate) struct CoverageProjection {
    generation: u64,
    message_generation: u64,
    max_message_rowid: i64,
    document_count: i64,
    max_document_rowid: i64,
    value: Value,
}
/// Coverage projections by account.
pub(crate) type CoverageCache = RefCell<HashMap<String, CoverageProjection>>;
/// A consistent copy of `Changes` taken by `MailChanges::snapshot`.
struct ChangeSnapshot {
    ui: ui_changes::UiChanges,
    model_progress: u64,
    messages: u64,
    coverage: u64,
    search: u64,
    reset_after: u64,
    inserts: Vec<(u64, i64)>,
    coverage_reset_after: u64,
    documents: Vec<(u64, i64)>,
}
/// Write counters maintained by the connection's update hook. Each
/// generation only grows while the store is open (the calendar one is reset
/// once at open), and a cache built at generation `g` is valid while the
/// generation is still `g`.
#[derive(Default)]
struct Changes {
    ui: ui_changes::UiChanges,
    /// Mail/model rows used by the local model's progress counts.
    model_progress: u64,
    /// Update-hook calls, compared with `total_changes()` to detect writes
    /// the hook never saw.
    hooks: u64,
    /// Writes to `calendar_publication`.
    calendar: u64,
    /// Writes to `messages` or `message_sender_kinds`.
    messages: u64,
    /// Writes that can change coverage: messages, attachments, FTS tables.
    coverage: u64,
    /// Writes that can change list or search results.
    search: u64,
    /// `inserts` is complete only for message generations after this one.
    reset_after: u64,
    /// Recent message inserts as `(messages generation, rowid)`, at most 512.
    inserts: VecDeque<(u64, i64)>,
    /// `documents` is complete only for coverage generations after this one.
    coverage_reset_after: u64,
    /// Recent FTS document inserts as `(coverage generation, rowid)`, at most 512.
    documents: VecDeque<(u64, i64)>,
    /// `(total_changes, hooks, messages)` at the previous snapshot.
    observed: (u64, u64, u64),
    /// Whether the previous snapshot was taken inside a transaction.
    observed_in_transaction: bool,
}
/// Shared write counters for one connection, plus an opaque epoch string that
/// ties list cursors to this open store.
#[derive(Clone)]
pub(crate) struct MailChanges(Arc<Mutex<Changes>>, String);
impl MailChanges {
    /// Installs the update hook on `db` and starts counting from zero.
    pub(crate) fn install(db: &Connection) -> Self {
        let changes = Arc::new(Mutex::new(Changes {
            observed: (db.total_changes(), 0, 0),
            ..Default::default()
        }));
        let hook = changes.clone();
        db.update_hook(Some(move |action, _: &str, table: &str, rowid| {
            let mut c = hook.lock().unwrap();
            c.hooks += 1;
            c.ui.observe(table);
            if matches!(
                table,
                "messages"
                    | "google_message_state"
                    | "local_model_results"
                    | "local_model_failures"
                    | "accounts"
            ) {
                c.model_progress += 1;
            }
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
                    | "google_category_labels"
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
    /// Calendar publications since open; reported as `calendar_generation`.
    pub(crate) fn calendar_generation(&self) -> u64 {
        self.0.lock().unwrap().calendar
    }
    /// Zeroes the calendar counter. Opening calls it after recovery, so
    /// clients see the counter start at 0.
    pub(crate) fn reset_calendar_generation(&self) {
        self.0.lock().unwrap().calendar = 0;
    }
    /// Returns the current generations, first invalidating everything the
    /// hook cannot vouch for: writes it did not see, and any read inside or
    /// just after a transaction, since a rollback does not undo hook counts.
    fn snapshot(&self, db: &Connection) -> ChangeSnapshot {
        let mut c = self.0.lock().unwrap();
        let total = db.total_changes();
        let (old_total, old_hooks, old_messages) = c.observed;
        if total - old_total != c.hooks - old_hooks {
            c.ui.invalidate();
            c.model_progress += 1;
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
            c.ui.invalidate();
            c.model_progress += 1;
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
            ui: c.ui,
            model_progress: c.model_progress,
            messages: c.messages,
            coverage: c.coverage,
            search: c.search,
            reset_after: c.reset_after,
            inserts: c.inserts.iter().copied().collect(),
            coverage_reset_after: c.coverage_reset_after,
            documents: c.documents.iter().copied().collect(),
        }
    }

    /// Uses the same conservative handling of missed hooks and transactions as
    /// the mail projections. Policy and time bounds are checked by the caller.
    pub(crate) fn model_progress_generation(&self, db: &Connection) -> u64 {
        self.snapshot(db).model_progress
    }

    pub(crate) fn ui_generations(&self, db: &Connection) -> Value {
        self.snapshot(db).ui.json(&self.1)
    }
}
impl Store {
    /// The search generation, reported as `mail_generation`. It changes
    /// whenever list or search results might.
    pub(crate) fn mail_generation(&self) -> u64 {
        self.mail_changes.snapshot(&self.db).search
    }
    pub(crate) fn accounts(&self) -> Result<Value> {
        self.accounts_including_removed(false)
    }
    /// Lists accounts in creation order with their counts, coverage,
    /// retention, draft count, lifecycle and, for Google accounts, sync status.
    /// Accounts that are not active appear only with `include_removed`;
    /// removed ones are marked `read_only`.
    pub(crate) fn accounts_including_removed(&self, include_removed: bool) -> Result<Value> {
        let mut stmt = self
            .db
            .prepare("SELECT id,name,email,provider FROM accounts WHERE (?1 OR lifecycle_state='active') ORDER BY rowid")?;
        let accounts = stmt
            .query_map([include_removed], |r| {
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
            let (metadata_revision, signature_text, mail_retention, mail_since): (i64, String, String, Option<i64>) = self.db.query_row(
                "SELECT revision,signature_text,mail_retention,mail_since FROM account_metadata WHERE account=?1",
                [&account],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
            // Events stored for this account's calendars, for the Accounts card.
            let calendar_events: i64 = self.db.query_row(
                "SELECT count(*) FROM calendar_sources s LEFT JOIN calendar_event_state e ON e.account=s.account AND e.calendar_id=s.calendar_id AND e.event_id=s.event_id WHERE s.account=?1 AND COALESCE(e.visible,1)=1",
                [&account],
                |r| r.get(0),
            )?;
            let color = crate::preferences::account_color(&self.db, &account)?;
            out.push(json!({"color":color,"calendar_events":calendar_events,"metadata_revision":metadata_revision,"unread_conversation_count":counts["unread_conversation_count"],"held_conversation_count":counts["held_conversation_count"],"done_conversation_count":counts["done_conversation_count"],"inbox_review_count":counts["inbox_review_count"],"inbox_held_count":counts["inbox_held_count"],"count_scope":"all_account_mail_including_archive","id":account,"name":name,"email":email,"provider":provider,"signature_text":signature_text,"unread_count":unread,"starred_count":starred,"conversation_count":count,"message_count":messages,"coverage":self.coverage(&account)?}));
            out.last_mut().unwrap()["mail_retention"] = json!(mail_retention);
            out.last_mut().unwrap()["mail_since"] = json!(mail_since);
            let oldest: Option<String> = self.db.query_row(
                "SELECT min(sent_at) FROM messages WHERE account=?1",
                [&account],
                |r| r.get(0),
            )?;
            out.last_mut().unwrap()["mail_oldest_at"] = json!(oldest);
            let drafts: i64 = self.db.query_row(
                "SELECT COUNT(*) FROM drafts d WHERE account=?1 AND discarded=0 AND COALESCE(json_extract(d.google_json,'$.submission.last_accepted_draft_revision'),0)<d.revision AND (origin='local' OR NOT EXISTS(SELECT 1 FROM google_draft_sources s WHERE s.account=d.account AND s.draft_id=d.draft_id AND s.is_current=1 AND s.linked_local_draft_id IS NOT NULL))",
                [&account],
                |r| r.get(0),
            )?;
            out.last_mut().unwrap()["draft_count"] = json!(drafts);
            out.last_mut().unwrap()["last_mail_sync_at"] =
                json!(self.google_last_mail_sync(&account)?);
            // Both names describe the oldest locally retained message.
            out.last_mut().unwrap()["cached_since"] = json!(oldest);
            let lifecycle = self.account_lifecycle(&account)?;
            out.last_mut().unwrap()["read_only"] = json!(lifecycle["state"] == "removed");
            out.last_mut().unwrap()["lifecycle"] = lifecycle;
            if let Some(status) = self.google_account_status(&account)? {
                out.last_mut().unwrap()["google"] = status;
            }
        }
        Ok(json!({"accounts":out,"fixture_revision":self.fixture_revision()?}))
    }
    /// Returns the cached `key` reply for `account` if it was built at
    /// `stamp`, otherwise builds and caches a new one.
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
    /// Conversation counts for the whole account (archive included): per
    /// folder, and per review group, where a starred unread conversation
    /// counts as both to review and held.
    pub(crate) fn account_counts(&self, account: &str) -> Result<Value> {
        let states = self.conversation_states(account)?;
        match self.conversation_version(account) {
            Some(version) => self.cached_projection(account, "counts", version, || {
                Ok(Self::counts_from_states(&states))
            }),
            None => Ok(Self::counts_from_states(&states)),
        }
    }
    fn account_message_counts(&self, account: &str) -> Result<(i64, i64, i64)> {
        let states = self.conversation_states(account)?;
        Ok(states.values().fold((0, 0, 0), |(u, s, m), state| {
            (
                u + state.unread_messages,
                s + state.starred_messages,
                m + state.messages,
            )
        }))
    }
    /// The `(messages, coverage)` generations before a flag write, for
    /// `refresh_committed_flag_projections`.
    pub(crate) fn flag_projection_stamp(&self) -> (u64, u64) {
        let changes = self.mail_changes.snapshot(&self.db);
        (changes.messages, changes.coverage)
    }
    /// Carries coverage caches that were current at `before` forward to the
    /// generations after a committed flag write, which cannot change coverage.
    pub(crate) fn refresh_committed_flag_projections(
        &self,
        _account: &str,
        _cid: &str,
        before: (u64, u64),
    ) {
        // Only the serialized, successfully committed flag path calls this.
        // Its SQL changes flags/revisions/manual intents, never source inventory,
        // routing, kinds or another conversation. Conversation states follow the
        // change log; only the coverage inventory would otherwise be recounted.
        let changes = self.mail_changes.snapshot(&self.db);
        for cached in self.coverage_cache.borrow_mut().values_mut() {
            if cached.generation == before.1 && cached.message_generation == before.0 {
                cached.generation = changes.coverage;
                cached.message_generation = changes.messages;
            }
        }
    }
    /// Reads one conversation's state straight from SQL. Its `position` is 0;
    /// callers that store it assign the slot.
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
    /// Returns every conversation state for `account`, keyed by conversation ID.
    ///
    /// Reuses the cached projection when the change log has nothing new,
    /// re-reads up to `INCREMENTAL_STATE_LIMIT` changed conversations, and
    /// otherwise rescans the account.
    fn conversation_states(
        &self,
        account: &str,
    ) -> Result<Arc<HashMap<String, ConversationState>>> {
        let latest: i64 =
            self.db
                .query_row("SELECT COALESCE(MAX(seq),0) FROM mail_state_log", [], |r| {
                    r.get(0)
                })?;
        // Inside a transaction a read may see changes that later roll back:
        // apply them to a copy and keep the committed projection unchanged.
        let persist = self.db.is_autocommit();
        let mut cache = self.conversation_cache.borrow_mut();
        if let Some(old) = cache.get_mut(account) {
            if old.log_seq == latest {
                return Ok(old.states.clone());
            }
            let mut changed = self.db.prepare_cached(
                "SELECT DISTINCT conversation_id FROM mail_state_log WHERE seq>?1 AND account=?2",
            )?;
            let changed = changed
                .query_map(params![old.log_seq, account], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if changed.is_empty() && persist {
                // Only other accounts changed.
                old.log_seq = latest;
                return Ok(old.states.clone());
            }
            if changed.len() <= INCREMENTAL_STATE_LIMIT {
                let mut copy = HashMap::new();
                let states = if persist {
                    Arc::make_mut(&mut old.states)
                } else {
                    copy = (*old.states).clone();
                    &mut copy
                };
                let (mut message_count, mut max_rowid, mut next_position) =
                    (old.message_count, old.max_rowid, old.next_position);
                for cid in changed {
                    let mut state = self.conversation_state(account, &cid)?;
                    let previous = states.remove(&cid);
                    message_count -= previous.as_ref().map_or(0, |p| p.messages);
                    if state.messages == 0 {
                        continue;
                    }
                    message_count += state.messages;
                    max_rowid = max_rowid.max(state.max_rowid);
                    state.position = previous.map_or_else(
                        || {
                            next_position += 1;
                            next_position - 1
                        },
                        |p| p.position,
                    );
                    states.insert(cid, state);
                }
                if !persist {
                    return Ok(Arc::new(copy));
                }
                old.log_seq = latest;
                old.version += 1;
                old.message_count = message_count;
                old.max_rowid = max_rowid;
                old.next_position = next_position;
                let states = old.states.clone();
                drop(cache);
                self.prune_state_log()?;
                return Ok(states);
            }
        }
        self.state_scans.set(self.state_scans.get() + 1);
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
        if persist {
            let version = cache.get(account).map_or(0, |p| p.version) + 1;
            cache.insert(
                account.into(),
                ConversationProjection {
                    log_seq: latest,
                    version,
                    next_position: states.len(),
                    message_count,
                    max_rowid,
                    states: states.clone(),
                },
            );
            drop(cache);
            self.prune_state_log()?;
        }
        Ok(states)
    }
    /// Drops change-log entries every cached projection has applied. With
    /// nothing cached, a later read scans once and needs no history.
    fn prune_state_log(&self) -> Result<()> {
        let floor = self
            .conversation_cache
            .borrow()
            .values()
            .map(|p| p.log_seq)
            .min();
        let first: i64 =
            self.db
                .query_row("SELECT COALESCE(MIN(seq),0) FROM mail_state_log", [], |r| {
                    r.get(0)
                })?;
        if let Some(floor) = floor {
            if floor - first >= STATE_LOG_PRUNE {
                self.db
                    .execute("DELETE FROM mail_state_log WHERE seq<=?1", [floor])?;
            }
        }
        Ok(())
    }
    /// Identifies the conversation states an account's derived projections
    /// were built from. Changes only when those states change.
    fn conversation_version(&self, account: &str) -> Option<u64> {
        self.db.is_autocommit().then_some(())?;
        self.conversation_cache
            .borrow()
            .get(account)
            .map(|p| p.version)
    }
    #[cfg(test)]
    fn compute_account_counts(&self, account: &str) -> Result<Value> {
        Ok(Self::counts_from_states(
            &*self.conversation_states(account)?,
        ))
    }
    fn counts_from_states(states: &HashMap<String, ConversationState>) -> Value {
        let unread = states.values().filter(|s| s.unread).count();
        let held = states.values().filter(|s| s.starred).count();
        let done = states.values().filter(|s| !s.unread && !s.starred).count();
        // What the Inbox's To review and Delayed sections hold: Trash and
        // archived mail are not waiting on the person.
        let inbox = || states.values().filter(|s| s.folders & 2 != 0);
        let inbox_review = inbox().filter(|s| s.unread).count();
        let inbox_held = inbox().filter(|s| s.starred).count();
        let mut totals = [0usize; 5];
        for state in states.values() {
            for (index, count) in totals.iter_mut().enumerate() {
                *count += usize::from(state.folders & (1 << index) != 0);
            }
        }
        let folders = (totals[0], totals[1], totals[2], totals[3], totals[4]);
        json!({"folder_conversation_counts":{"all":folders.0,"inbox":folders.1,"sent":folders.2,"trash":folders.3,"received":folders.4},"conversation_count":states.len(),"unread_conversation_count":unread,"held_conversation_count":held,"done_conversation_count":done,"inbox_review_count":inbox_review,"inbox_held_count":inbox_held,"scope":"all_account_mail_including_archive","held_includes_unread":true})
    }
    /// How much of the account's mail is cached, indexed and retained.
    ///
    /// Live profiles report Google acquisition coverage. Synthetic profiles
    /// count local inventories, applying bounded append-only deltas to the
    /// cached reply when possible and recounting otherwise.
    fn coverage(&self, account: &str) -> Result<Value> {
        if self.profile_kind()? == live_profile::ProfileKind::Live {
            let coverage = self.google_live_coverage(account)?;
            return Ok(
                json!({"synthetic":false,"inventory_complete":coverage.inventory_complete,"delta_current":coverage.delta_current,
                "messages":coverage.observed,"indexed_messages":coverage.indexed,"text_index_complete":coverage.indexed==coverage.observed,
                "retained_source_records":coverage.retained_sources,"retained_sources":coverage.retained_sources,"source_verification":"verified_on_acquisition",
                "attachments":null,"recorded_available_attachments":null,"complete_attachments":null,"verified_attachments":null,"attachment_verification":"verified_on_acquisition",
                "messages_with_complete_attachments":coverage.complete_attachments,"unavailable_messages":coverage.unavailable,"undownloadable_messages":coverage.undownloadable,"offline_complete":coverage.offline_complete,
                "text":format!("Cached text: {}/{} observed messages. {} source records retained; {} items need attention. {}",coverage.indexed,coverage.observed,coverage.retained_sources,coverage.unavailable,if coverage.offline_complete {"Inventory and catch-up are complete at the last sync."} else {"Full offline acquisition is incomplete."})}),
            );
        }
        let changes = self.mail_changes.snapshot(&self.db);
        if let Some(old) = self.coverage_cache.borrow().get(account) {
            if old.generation == changes.coverage {
                return Ok(old.value.clone());
            }
        }
        let (message_count, max_message_rowid) = self
            .conversation_states(account)?
            .values()
            .fold((0, 0), |(count, max), s| {
                (count + s.messages, max.max(s.max_rowid))
            });
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
        // Try a delta: only when every insert since the cached reply is still
        // in the bounded history, every one is an append (rowid above the old
        // maximum), and both inventories grew by exactly those rows.
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
    /// Recounts synthetic coverage from SQL without using the cache.
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
            "SELECT COUNT(*),COALESCE(SUM(state='available'),0) FROM attachments WHERE account=?1 AND message_id IS NOT NULL",
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
    /// Serves the `conversations` method: one page of the account's
    /// conversation list.
    pub(crate) fn conversations(&self, account: &str, p: &Value) -> Result<Value> {
        self.conversations_ordered(account, p, false)
    }
    /// Lists one page of conversations matching a folder, search text and
    /// typed filters.
    ///
    /// Filters and search terms apply to individual messages, and a
    /// conversation matches when any message does; review status (unread,
    /// held, done) and sender kind come from the whole conversation. Rows are
    /// ordered newest first, optionally grouped by review status (`grouped`),
    /// and paged by `offset` or by an opaque `cursor`. A plain cursor turns
    /// `stale_cursor` once mail changes; with `live_paging` it carries the
    /// last row's sort key and continues after it instead. Totals always
    /// cover every match, not just the page.
    ///
    /// With `recent` (used by `sender_summary`), ungrouped rows sort by the
    /// conversation's newest message rather than its newest matching one, and
    /// the first page is not cached.
    fn conversations_ordered(&self, account: &str, p: &Value, recent: bool) -> Result<Value> {
        // Validate the request shape and every typed filter before reading.
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
                "exact_person",
                "kind",
                "status",
                "has_attachment",
                "after",
                "before",
                "grouped",
                "cursor",
                "collapsed_groups",
                "live_paging",
                "month_zone",
                "collapsed_months",
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
        // One exact address: the sender only, or anyone on a message.
        let exact_sender = [
            optional_string(p, "exact_sender", "")?,
            optional_string(p, "exact_person", "")?,
        ]
        .concat()
        .trim()
        .to_lowercase();
        let kind = optional_string(p, "kind", "all")?;
        // Substring match; LIKE's own wildcards in the input match literally.
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
        let live_paging = match p.get("live_paging") {
            None => false,
            Some(Value::Bool(value)) => *value,
            _ => return Err(err("invalid_request", "live_paging must be a boolean")),
        };
        // Cursors. The epoch binds a cursor to this open store and the
        // fingerprint to this exact query minus its paging fields, so a cursor
        // cannot be replayed against another query or a restarted engine. A
        // plain cursor also pins the search generation; a live-paging cursor
        // instead carries the last row's sort key, `[group rank, date, id]`.
        let mut anchor = Value::Null;
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
                    "anchor",
                ],
            )?;
            if cursor["epoch"] != *epoch
                || (!live_paging && cursor["search_generation"] != search_generation)
                || cursor["fingerprint"] != fingerprint
            {
                return Err(err(
                    "stale_cursor",
                    "Mail or query changed; restart pagination",
                ));
            }
            offset = number(&cursor, "offset", 0, 0, 10_000_000)?;
            if live_paging {
                anchor = cursor["anchor"].clone();
                if anchor.as_array().is_none_or(|a| a.len() != 3)
                    || anchor[0].as_u64().is_none_or(|rank| rank > 2)
                    || anchor[1].as_str().is_none() && anchor[1].as_f64().is_none()
                    || anchor[2]
                        .as_str()
                        .is_none_or(|id| id.is_empty() || id.len() > 256)
                {
                    return Err(err("invalid_request", "Invalid live page anchor"));
                }
            }
        }
        // Parse the search box text and reconcile it with the typed filters.
        let query = optional_string(p, "query", "")?;
        if query.len() > 512 {
            return Err(err("invalid_request", "Search is limited to 512 bytes"));
        }
        let field = optional_string(p, "field", "all")?;
        if !["all", "sender", "subject", "body"].contains(&field) {
            return Err(err("invalid_request", "Unknown search field"));
        }
        let folder = optional_string(p, "folder", "inbox")?;
        if !["all", "inbox", "received", "sent", "trash"].contains(&folder) {
            return Err(err("invalid_request", "Unknown folder"));
        }
        let text = search_query::parse(query)?;
        // Typed filters and the text must agree; a contradiction is refused
        // rather than silently preferring one of them.
        let status = match (status, text.is) {
            (typed, Some(read)) if typed == "all" || typed == read => read,
            (_, Some(_)) => {
                return Err(err(
                    "invalid_request",
                    "is: conflicts with the status filter",
                ))
            }
            (typed, None) => typed,
        };
        let attachment = match (attachment, text.has_attachment) {
            (Some(false), true) => {
                return Err(err(
                    "invalid_request",
                    "has:attachment conflicts with the attachment filter",
                ))
            }
            (_, true) => Some(true),
            (typed, false) => typed,
        };
        let (label_id, label_name) = match &text.label {
            Some(name) => {
                let (id, name) = self.label_named(account, name)?;
                (id, Some(name))
            }
            None => (String::new(), None),
        };
        let search_terms = if query.trim().is_empty() {
            Value::Null
        } else {
            text.terms(label_name.as_deref())
        };
        // from: is the sender, to: a recipient and a bare address either, in every folder.
        let person_roles: Vec<PersonRole> =
            std::iter::repeat_n(PersonRole::Sender, text.from.len())
                .chain(std::iter::repeat_n(PersonRole::Recipient, text.to.len()))
                .chain(std::iter::repeat_n(PersonRole::Either, text.people.len()))
                .collect();
        let from_patterns: Vec<String> = text
            .from
            .iter()
            .chain(&text.to)
            .chain(&text.people)
            .map(|value| search_query::like_pattern(value))
            .collect();
        let month_zone = match p.get("month_zone") {
            None => None,
            Some(value) => Some(
                value
                    .as_str()
                    .filter(|zone| zone.len() <= 64)
                    .and_then(|zone| zone.parse::<fm_calendar::Tz>().ok())
                    .ok_or_else(|| {
                        err("invalid_request", "month_zone must be an IANA time zone")
                    })?,
            ),
        };
        let mut collapsed_months = HashSet::new();
        if let Some(months) = p.get("collapsed_months") {
            let months = months
                .as_array()
                .filter(|months| months.len() <= 256 && month_zone.is_some())
                .ok_or_else(|| {
                    err(
                        "invalid_request",
                        "collapsed_months needs month_zone and at most 256 months",
                    )
                })?;
            for month in months {
                let month = month
                    .as_str()
                    .filter(|m| valid_month(m))
                    .ok_or_else(|| err("invalid_request", "Collapsed months use YYYY-MM"))?;
                if !collapsed_months.insert(month) {
                    return Err(err("invalid_request", "Collapsed months must be unique"));
                }
            }
        }
        // Labels filter as toggles: mail with any listed label (comma-separated
        // ids). A label: in the query decides instead.
        let mut categories: Vec<&str> = if label_id.is_empty() {
            optional_string(p, "category", "")?
                .split(',')
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            Vec::new()
        };
        categories.sort_unstable();
        categories.dedup();
        if categories.len() > 32 {
            return Err(err("invalid_request", "Filter by up to 32 labels"));
        }
        for category in &categories {
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
        let category = if categories.is_empty() {
            String::new()
        } else {
            serde_json::to_string(&categories)?
        };
        let mut effective = p.clone();
        effective["category"] = json!(category);
        // Bare words match the subject or message text. The CLI's `field` can
        // narrow them to one column, the sender column included.
        let word_text = text.words.join(" ");
        let tokens: Vec<&str> = search_query::tokens(&word_text)
            .chain(text.subject.iter().flat_map(|v| search_query::tokens(v)))
            .chain(text.body.iter().flat_map(|v| search_query::tokens(v)))
            .collect();
        let untokenized = text
            .subject
            .iter()
            .chain(&text.body)
            .any(|v| search_query::tokens(v).next().is_none());
        // Text made only of punctuation cannot use the index; refuse it unless
        // another criterion (a person, label, attachment or status) narrows it.
        let only_symbols = !text.words.is_empty()
            && tokens.is_empty()
            && from_patterns.is_empty()
            && label_id.is_empty()
            && !text.has_attachment
            && text.is.is_none();
        if tokens.len() > 32 || untokenized || only_symbols {
            return Err(err("invalid_request", "Search needs 1–32 words"));
        }
        let columns = if field == "all" {
            "{subject body}"
        } else {
            field
        };
        // Each phrase must appear as written. The index finds its
        // words in order, the last as a prefix; the message text then decides
        // the punctuation and spacing between them.
        let expression = text
            .phrases
            .iter()
            .map(|v| search_query::fts_phrase(columns, v))
            .chain(
                text.subject
                    .iter()
                    .map(|v| search_query::fts_phrase("subject", v)),
            )
            .chain(
                text.body
                    .iter()
                    .map(|v| search_query::fts_phrase("body", v)),
            )
            .flatten()
            .collect::<Vec<_>>()
            .join(" AND ");
        let phrase_column = match field {
            "subject" => TextColumn::Subject,
            "body" => TextColumn::Body,
            "sender" => TextColumn::Sender,
            _ => TextColumn::SubjectOrBody,
        };
        // The index ignores punctuation and spacing, so every phrase except a
        // single plain word is re-checked against the folded message text
        // while rows are scanned below.
        let phrase_checks: Vec<(TextColumn, String)> = text
            .phrases
            .iter()
            .map(|v| (phrase_column, v))
            .chain(text.subject.iter().map(|v| (TextColumn::Subject, v)))
            .chain(text.body.iter().map(|v| (TextColumn::Body, v)))
            .filter(|(_, v)| search_query::tokens(v).next().is_some())
            .map(|(column, v)| (column, search_query::fold(v)))
            .filter(|(_, v)| search_query::phrase_needs_check(v))
            .collect();
        // Previews start near the first phrase that can be in the text.
        let snippet_phrases: Vec<String> = text
            .phrases
            .iter()
            .chain(&text.body)
            .filter(|v| search_query::tokens(v).next().is_some())
            .map(|v| search_query::fold(v))
            .collect();
        // First-page cache. One first page per account bounds this cache
        // independently of the corpus and query history. Mail/source hooks and
        // transaction snapshots invalidate it through the existing search
        // generation; presentation, draft and preference writes do not force
        // another full mailbox scan. Validation above still runs on every
        // call, including cursor scope.
        let first_page_key = (account.to_owned(), "first_conversation_page");
        let cache_first_page = !recent && offset == 0;
        let next_cursor = |next: i64, anchor: &Value| -> Result<String> {
            Ok(serde_json::to_string(
                &json!({"revision":revision,"epoch":epoch,
                "search_generation":search_generation,"offset":next,"fingerprint":fingerprint,"anchor":anchor}),
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
                        Some(next) => {
                            let prior: Value = serde_json::from_str(
                                result["next_cursor"].as_str().unwrap_or("{}"),
                            )?;
                            next_cursor(next, &prior["anchor"])?.into()
                        }
                        None => Value::Null,
                    };
                    return Ok(result);
                }
            }
        }
        // Matching. Evaluate source predicates once. Group exact identities in
        // Core rather than materializing two SQL grouping trees and a temporary
        // join index. The metadata projection is derived from this same writer
        // connection.
        struct Matched {
            first_row: i64,
            other_rows: Vec<i64>,
            // `sent_at` text of the newest matching message; empty when the
            // order uses the conversation state's date instead.
            latest: String,
        }
        let states = self.conversation_states(account)?;
        let folder_bit = match folder {
            "all" => 1,
            "inbox" => 2,
            "sent" => 4,
            "trash" => 8,
            _ => 16,
        };
        let source_filtered = !expression.is_empty()
            || !category.is_empty()
            || !sender.is_empty()
            || !exact_sender.is_empty()
            || attachment.is_some()
            || !after.is_empty()
            || !before.is_empty()
            || !from_patterns.is_empty()
            || !label_id.is_empty();
        // A grouped list with no message-level filter is answered from the
        // conversation states alone, without reading any message rows.
        let simple = grouped && !source_filtered;
        let sql = search_matches_sql_with(
            !expression.is_empty(),
            &effective,
            &TextPredicates {
                attachment,
                people: &person_roles,
                label: !label_id.is_empty(),
                texts: !phrase_checks.is_empty(),
            },
        );
        let mut stmt = self.db.prepare_cached(&sql)?;
        // Positions are stable slots; removed threads leave gaps.
        let slots = states.values().map(|s| s.position + 1).max().unwrap_or(0);
        let mut matches: Vec<Option<Matched>> =
            std::iter::repeat_with(|| None).take(slots).collect();
        let mut page = Vec::new();
        // The twelve typed slots are always bound, in the order
        // `search_matches_sql_with` numbers them; text predicates follow.
        let mut values: Vec<&dyn rusqlite::ToSql> = vec![
            &account,
            &folder,
            &category,
            &expression,
            &limit,
            &offset,
            &sender_pattern,
            &status,
            &attachment,
            &after,
            &before,
            &exact_sender,
        ];
        values.extend(from_patterns.iter().map(|v| v as &dyn rusqlite::ToSql));
        if !label_id.is_empty() {
            values.push(&label_id);
        }
        let mut rows = stmt.query(values.as_slice())?;
        if simple {
            page.extend(
                states
                    .iter()
                    .filter(|(_, state)| {
                        state.folders & folder_bit != 0
                            && (kind == "all" || kind == kind_name(state.kind))
                            && match status {
                                "review" => state.unread,
                                "held" => state.starred,
                                "done" => !state.unread && !state.starred,
                                _ => true,
                            }
                    })
                    .map(|(id, state)| (id.as_str(), state)),
            );
        }
        // Otherwise scan the matching messages. Apply the checks SQL leaves to
        // Rust (phrase punctuation, exact addresses, conversation kind and
        // status), then fold each row into its conversation's slot.
        if !simple {
            while let Some(row) = rows.next()? {
                if !phrase_checks.is_empty() {
                    let mut folded: [Option<String>; 3] = [None, None, None];
                    let mut found = true;
                    for (column, phrase) in &phrase_checks {
                        let mut has = |index: usize| -> Result<bool> {
                            if folded[index].is_none() {
                                let text = row
                                    .get_ref(4 + index)?
                                    .as_str_or_null()
                                    .map_err(|_| err("storage_error", "Invalid message text"))?;
                                folded[index] = Some(search_query::fold(text.unwrap_or("")));
                            }
                            Ok(search_query::contains_phrase(
                                folded[index].as_deref().unwrap_or(""),
                                phrase,
                            ))
                        };
                        found = match column {
                            TextColumn::Subject => has(0)?,
                            TextColumn::Body => has(1)?,
                            TextColumn::Sender => has(2)?,
                            TextColumn::SubjectOrBody => has(0)? || has(1)?,
                        };
                        if !found {
                            break;
                        }
                    }
                    if !found {
                        continue;
                    }
                }
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
                let (cid, state) = states.get_key_value(cid).ok_or_else(|| {
                    err("storage_error", "Matching conversation has no source state")
                })?;
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
        }
        // Grouping and ordering. The filtered and per-group totals cover every
        // match, before collapsed groups or months and paging remove rows.
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
        // Done is presented by month in the client's zone. Counts cover the
        // whole filtered set; a collapsed month's rows are left out of paging.
        let mut done_months: BTreeMap<String, usize> = BTreeMap::new();
        if let Some(zone) = month_zone {
            for (_, state) in &page {
                if group_rank(state) == 2 {
                    *done_months
                        .entry(month_key(zone, state.latest))
                        .or_default() += 1;
                }
            }
        }
        if !collapsed.is_empty() || !collapsed_months.is_empty() {
            page.retain(|(_, state)| {
                let rank = group_rank(state);
                let month_collapsed = rank == 2
                    && month_zone.is_some_and(|zone| {
                        collapsed_months.contains(month_key(zone, state.latest).as_str())
                    });
                !(collapsed.contains(["review", "held", "done"][rank]) || month_collapsed)
            });
        }
        let expanded_count = page.len();
        // Review group first when grouped, then newest first, then conversation
        // ID so ties never reorder between pages. Grouped and `recent` lists
        // date a conversation by its newest message (a Julian day); plain
        // lists by its newest matching message's `sent_at`, compared as text.
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
        // Paging. A live-paging anchor replaces the offset: keep only rows that
        // sort after the last row the client saw, so rows inserted ahead of it
        // or removed (the anchor row included) neither repeat nor skip others.
        let page_offset = if anchor.is_null() { offset } else { 0 };
        if !anchor.is_null() {
            let rank = anchor[0].as_u64().unwrap() as usize;
            let last_id = anchor[2].as_str().unwrap();
            if (grouped || recent) != anchor[1].is_number() {
                return Err(err("invalid_request", "Invalid live page ordering"));
            }
            page.retain(|(id, state)| {
                let order = (if grouped {
                    group_rank(state).cmp(&rank)
                } else {
                    std::cmp::Ordering::Equal
                })
                .then_with(|| {
                    if grouped || recent {
                        anchor[1].as_f64().unwrap().total_cmp(&state.latest)
                    } else {
                        anchor[1]
                            .as_str()
                            .unwrap()
                            .cmp(&matches[state.position].as_ref().unwrap().latest)
                    }
                })
                .then_with(|| id.cmp(&last_id));
                order.is_gt()
            });
        }
        let remaining = page.len();
        // Rank only the requested prefix; exact totals still use every match.
        let end = ((page_offset + limit) as usize).min(page.len());
        if end < page.len() {
            page.select_nth_unstable_by(end, &compare);
            page.truncate(end);
        }
        page.sort_unstable_by(compare);
        let more = (page_offset + limit) < remaining as i64;
        let page = page
            .into_iter()
            .skip(page_offset as usize)
            .take(limit as usize)
            .collect::<Vec<_>>();
        let next_anchor = if live_paging {
            page.last()
                .map(|(id, state)| {
                    json!([
                        if grouped { group_rank(state) } else { 0 },
                        if grouped || recent {
                            json!(state.latest)
                        } else {
                            json!(matches[state.position].as_ref().unwrap().latest)
                        },
                        id
                    ])
                })
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        // Rows, built for the page only. Each row takes its sender, recipient
        // and snippet from the newest matching message; in a simple list every
        // message in the folder matches.
        let mut out = Vec::with_capacity(page.len());
        for (cid, state) in page {
            let matched: HashSet<i64> = if simple {
                let mut query = self.db.prepare_cached("SELECT rowid FROM messages WHERE account=?1 AND conversation_id=?2 AND ((?3='all' AND trashed=0) OR (?3='inbox' AND trashed=0 AND inbox=1 AND sent=0) OR (?3='sent' AND trashed=0 AND sent=1) OR (?3='trash' AND trashed=1) OR (?3='received' AND trashed=0 AND sent=0))")?;
                let rows =
                    query.query_map(params![account, cid, folder], |r| r.get::<_, i64>(0))?;
                rows.collect::<std::result::Result<_, _>>()?
            } else {
                let matching = matches[state.position].take().unwrap();
                std::iter::once(matching.first_row)
                    .chain(matching.other_rows)
                    .collect()
            };
            let (subject, revision): (String, i64) = self.db.query_row(
                "SELECT subject,revision FROM conversations WHERE account=?1 AND id=?2",
                params![account, cid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            // Oldest first. LIMIT 201 detects a conversation over the
            // 200-message bound, which is refused rather than truncated.
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
                    "This view supports conversations of 1–200 messages",
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
            let (sender_name, sender_email, body, recipient): (String, String, String, String) = self.db.query_row(
                "SELECT sender_name,sender_email,body,recipient FROM messages WHERE account=?1 AND id=?2",
                params![account, selected],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
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
            let attachment_count:i64=self.db.prepare_cached("SELECT COUNT(*) FROM messages m JOIN attachments a ON a.account=m.account AND a.message_id=m.id WHERE m.account=?1 AND m.conversation_id=?2")?.query_row(params![account,cid],|r|r.get(0))?;
            let summary = self.local_model_preview(account, cid, selected)?;
            let (others, other_names) = self.conversation_others(account, cid, &sender_email)?;
            out.push(json!({"others":others,"other_names":other_names,"summary":summary,"kind":kind_name(state.kind),"group":group,"id":cid,"subject":subject,"sender_name":sender_name,"sender_email":sender_email,"recipient":recipient,"snippet":search_snippet(&body,&snippet_phrases),"latest_at":members.last().unwrap().1,"message_count":members.len(),"attachment_count":attachment_count,"unread":members.iter().any(|m|m.2),"starred":members.iter().any(|m|m.3),"revision":revision,"categories":categories,"message_ids":members.iter().map(|m|m.0.clone()).collect::<Vec<_>>(),"matching_message_ids":if source_filtered{matching}else{Vec::<String>::new()}}));
        }
        // Newest month first; conversations without a date sort last.
        let done_months = month_zone.map(|_| {
            let undated = done_months.remove(UNDATED);
            done_months
                .iter()
                .rev()
                .map(|(month, count)| json!({"month":month,"count":count}))
                .chain(undated.map(|count| json!({"month":UNDATED,"count":count})))
                .collect::<Vec<_>>()
        });
        let result = json!({"conversations":out,"folder_conversation_count":self.account_counts(account)?["folder_conversation_counts"][folder],"expanded_conversation_count":expanded_count,"filtered_group_counts":{"review":group_counts[0],"held":group_counts[1],"done":group_counts[2]},"done_months":done_months,"search_terms":search_terms,"filtered_conversation_count":filtered_count,"account_counts":self.account_counts(account)?,"revision":revision,"grouped":grouped,"date_filter_time_zone":"UTC","next_cursor":if more {Some(next_cursor(offset+limit, &next_anchor)?)} else {None},"next_offset":if more{Some(offset+limit)}else{None},"coverage":self.coverage(account)?});
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
    /// The three most recent conversations outside Trash that `email` is on,
    /// as sender or recipient, plus the total count of such conversations.
    pub(crate) fn sender_summary(&self, account: &str, p: &Value) -> Result<Value> {
        fields(p, &["email"])?;
        let email = canonical_email(
            p.get("email")
                .and_then(Value::as_str)
                .ok_or_else(|| err("invalid_request", "Sender summary needs an email address"))?,
        )?;
        // Conversations the person is on: as sender, or on To or Cc.
        let result = self.conversations_ordered(
            account,
            &json!({"exact_person":email,"folder":"all","limit":3}),
            true,
        )?;
        Ok(
            json!({"account":account,"email":email,"conversation_count":result["filtered_conversation_count"],"conversations":result["conversations"]}),
        )
    }
    /// The reader view of one conversation: every message oldest first with
    /// its text, flags, attachments and labels, plus the conversation's kind
    /// and mailing-list line. It only reads; marking messages read is a
    /// separate `flag` write.
    ///
    /// Fails with `not_found` for an unknown conversation and
    /// `limit_exceeded` above 200 messages.
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
        let mut stmt=self.db.prepare("SELECT m.id,sender_name,sender_email,recipient,subject,sent_at,body,unread,starred,revision,m.source_hash,COALESCE(k.kind,'unknown'),m.sent FROM messages m LEFT JOIN message_sender_kinds k ON k.account=m.account AND k.message_id=m.id AND k.source_hash=m.source_hash WHERE m.account=?1 AND conversation_id=?2 ORDER BY julianday(sent_at),m.id LIMIT 201")?;
        let mut messages=stmt.query_map(params![account,cid],|r|Ok(json!({"id":r.get::<_,String>(0)?,"sender_name":r.get::<_,String>(1)?,"sender_email":r.get::<_,String>(2)?,"to":r.get::<_,String>(3)?,"subject":r.get::<_,String>(4)?,"sent_at":r.get::<_,String>(5)?,"text":r.get::<_,String>(6)?,"unread":r.get::<_,bool>(7)?,"starred":r.get::<_,bool>(8)?,"revision":r.get::<_,i64>(9)?,"source_hash":r.get::<_,String>(10)?,"kind":r.get::<_,String>(11)?,"sent":r.get::<_,bool>(12)?})))?.collect::<std::result::Result<Vec<_>,_>>()?;
        if messages.is_empty() || messages.len() > 200 {
            return Err(err(
                "limit_exceeded",
                "This view supports conversations of 1–200 messages",
            ));
        }
        let mut categories = std::collections::BTreeSet::new();
        for message in &mut messages {
            let mid = message["id"].as_str().unwrap().to_owned();
            let hash = message["source_hash"].as_str().unwrap().to_owned();
            message["source_available"] = self.blob_present(&hash).into();
            message["source_version"] = hash.into();
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
        // Only a received latest message gets the mailing-list line.
        let latest = messages.last().unwrap();
        let mailing_list = if latest["sent"] == true {
            Value::Null
        } else {
            self.mailing_list(account, cid, latest["sender_email"].as_str().unwrap_or(""))?
        };
        Ok(
            json!({"kind":kind,"id":cid,"subject":subject,"revision":revision,"message_ids":ids,"categories":categories,"messages":messages,"mailing_list":mailing_list}),
        )
    }
    /// The account label a search `label:` names: its exact name ignoring
    /// case, or else the only label whose name begins with it.
    fn label_named(&self, account: &str, name: &str) -> Result<(String, String)> {
        let wanted = name.to_lowercase();
        let mut stmt = self
            .db
            .prepare_cached("SELECT id,name FROM categories WHERE account=?1 ORDER BY name,id")?;
        let labels = stmt
            .query_map([account], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if let Some(found) = labels.iter().find(|(_, n)| n.to_lowercase() == wanted) {
            return Ok(found.clone());
        }
        let mut prefixed = labels
            .into_iter()
            .filter(|(_, n)| n.to_lowercase().starts_with(&wanted));
        match (prefixed.next(), prefixed.next()) {
            (Some(found), None) => Ok(found),
            (None, _) => Err(err("invalid_request", &format!("No label named “{name}”"))),
            _ => Err(err(
                "invalid_request",
                &format!("label:{name} matches more than one label"),
            )),
        }
    }
    /// The account's labels by name, each with the number of conversations
    /// carrying it; live profiles add the bound Gmail label ID.
    pub(crate) fn categories(&self, account: &str) -> Result<Value> {
        let stamp = self.mail_changes.snapshot(&self.db).search;
        self.cached_projection(account, "categories", stamp, || {
            let live = self.profile_kind()? == crate::live_profile::ProfileKind::Live;
            let mut stmt=self.db.prepare("SELECT c.id,c.name,COUNT(DISTINCT m.conversation_id),g.provider_label_id FROM categories c LEFT JOIN message_categories mc ON mc.account=c.account AND mc.category_id=c.id LEFT JOIN messages m ON m.account=mc.account AND m.id=mc.message_id LEFT JOIN google_category_labels g ON g.account=c.account AND g.category_id=c.id WHERE c.account=?1 GROUP BY c.id,c.name,g.provider_label_id ORDER BY c.name")?;
            Ok(json!({"categories":stmt.query_map([account],|r| {
                let mut row=json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"conversation_count":r.get::<_,i64>(2)?});
                if live { row["provider_label_id"]=json!(r.get::<_,Option<String>>(3)?); }
                Ok(row)
            })?.collect::<std::result::Result<Vec<_>,_>>()?}))
        })
    }
    /// The account's saved client view state (an opaque JSON object) and its
    /// revision; revision 0 and `{}` when nothing was saved.
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
    /// One account's cached calendar events between `from` and `to`
    /// (`YYYY-MM-DD`, 1 to 42 days). Synthetic profiles default to the demo
    /// week; live profiles must pass both dates.
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
/// Reads optional string parameter `key`, or `default` when it is absent.
/// A present non-string value fails with `invalid_request`.
fn optional_string<'a>(p: &'a Value, key: &str, default: &'a str) -> Result<&'a str> {
    match p.get(key) {
        None => Ok(default),
        Some(v) => v
            .as_str()
            .ok_or_else(|| err("invalid_request", "Expected a string")),
    }
}
/// Reads optional integer parameter `key` (or `default`), which must lie in
/// `min..=max`.
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

/// A row's preview: readable text (HTML source becomes its text) without web
/// addresses, starting near the first search token.
fn search_snippet(body: &str, phrases: &[String]) -> String {
    let body = fm_mail::preview::preview_source(body);
    // The first place a phrase matches, as the search decided.
    let position = phrases
        .iter()
        .filter_map(|phrase| search_query::find_phrase(&body, phrase))
        .min();
    let start = position.map(|p| p.saturating_sub(60)).unwrap_or(0);
    let window: String = body.chars().skip(start).take(2000).collect();
    let clean = fm_mail::preview::without_links(&window);
    let mut chars = clean.chars();
    let text: String = chars.by_ref().take(180).collect();
    format!(
        "{}{}{}",
        if start > 0 { "… " } else { "" },
        text,
        if chars.next().is_some() { " …" } else { "" }
    )
}

/// Validates the typed list filters: exact sender or person, sender kind,
/// sender substring, review status, attachment flag and UTC date bounds.
///
/// Structured filters combine on the same source message; status considers
/// the whole conversation.
pub(crate) fn validate_filters(p: &Value) -> Result<()> {
    for key in ["exact_sender", "exact_person"] {
        if let Some(value) = p.get(key) {
            canonical_email(
                value.as_str().ok_or_else(|| {
                    err("invalid_request", "Exact sender must be an email address")
                })?,
            )?;
        }
    }
    if p.get("exact_sender").is_some() && p.get("exact_person").is_some() {
        return Err(err(
            "invalid_request",
            "Use either an exact sender or an exact person",
        ));
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
/// Trims, validates and lowercases an address used as an exact filter.
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
/// Done conversations whose latest message has no usable date.
const UNDATED: &str = "undated";

/// `YYYY-MM` of a Julian day in `zone`, or `undated`.
fn month_key(zone: fm_calendar::Tz, julian_day: f64) -> String {
    let seconds = ((julian_day - 2_440_587.5) * 86_400.0).round();
    if julian_day <= 0.0 || !seconds.is_finite() {
        return UNDATED.into();
    }
    chrono::DateTime::from_timestamp(seconds as i64, 0)
        .map(|at| at.with_timezone(&zone).format("%Y-%m").to_string())
        .unwrap_or_else(|| UNDATED.into())
}

fn valid_month(month: &str) -> bool {
    month == UNDATED
        || (month.len() == 7
            && month.as_bytes()[4] == b'-'
            && chrono::NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d").is_ok())
}

/// Which side of a message a search name or address must be on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PersonRole {
    Sender,
    /// To or Cc. The first To address is on the message row; the link index
    /// adds every From/To/Cc person once it has read the source.
    Recipient,
    Either,
}

/// Predicates the search text adds beyond the typed filters in the request.
#[derive(Default)]
pub(crate) struct TextPredicates<'a> {
    /// Keep only messages with (`Some(true)`) or without (`Some(false)`) an
    /// attachment.
    pub attachment: Option<bool>,
    /// One LIKE pattern slot per name or address, in order.
    pub people: &'a [PersonRole],
    /// Add one slot, after the people, for a single label ID from `label:`.
    pub label: bool,
    /// Also return subject, body and sender text (columns 4–6) for phrase checks.
    pub texts: bool,
}

/// Where a search phrase must appear.
#[derive(Clone, Copy)]
enum TextColumn {
    Subject,
    Body,
    Sender,
    SubjectOrBody,
}

/// The benchmark's fixed twelve-slot form, without text predicates.
#[cfg(test)]
pub(crate) fn search_matches_sql(search: bool, p: &Value) -> String {
    let text = TextPredicates {
        attachment: p["has_attachment"].as_bool(),
        ..TextPredicates::default()
    };
    search_matches_sql_with(search, p, &text)
}

/// A message listing the exact address `?12` as any From/To/Cc person the link
/// index has read, or as the first To of a message it has not read yet. One
/// list, so the outer scan stays on its index instead of reading every row.
const EXACT_LISTED: &str = "m.rowid IN (SELECT mp.message_rowid FROM message_people mp WHERE mp.person=(SELECT id FROM people WHERE email=?12) UNION ALL SELECT rowid FROM messages WHERE rowid>COALESCE((SELECT indexed_rowid FROM link_index_state),0) AND account=?1 AND trim(recipient)=?12 COLLATE NOCASE)";

/// Builds the message-matching query for a conversation list.
///
/// Parameter slots are fixed: `?1` account, `?2` folder, `?3` label IDs as a
/// JSON array, `?4` FTS expression, `?5` limit, `?6` offset, `?7` sender
/// LIKE pattern, `?8` status, `?9` attachment flag, `?10` after, `?11`
/// before, `?12` exact address; then one slot per `text.people` entry and
/// one for `text.label`. Every slot is bound even when unused, and the
/// `bindings` CTE references them all so SQLite accepts the binding. The
/// folder and attachment choice are written into the SQL, and the caller
/// applies status, kind and paging, so `?2`, `?5`, `?6`, `?8` and `?9` go
/// unused. Each row is `conversation_id, rowid, sent_at, exact address`, plus
/// subject, body and sender text when `text.texts`.
pub(crate) fn search_matches_sql_with(search: bool, p: &Value, text: &TextPredicates) -> String {
    // Only add predicates actually requested. Bound optional OR expressions kept
    // sender/body columns in the old query's scan even for unfiltered listings.
    let mut predicate = "m.account=?1".to_owned();
    match p["folder"].as_str().unwrap_or("inbox") {
        "inbox" => predicate.push_str(" AND m.inbox=1 AND m.sent=0 AND m.trashed=0"),
        "received" => predicate.push_str(" AND m.sent=0 AND m.trashed=0"),
        "sent" => predicate.push_str(" AND m.sent=1 AND m.trashed=0"),
        "trash" => predicate.push_str(" AND m.trashed=1"),
        _ => predicate.push_str(" AND m.trashed=0"),
    }
    // ?3 is a JSON array of label ids; any of them matches.
    if p["category"].as_str().is_some_and(|s| !s.is_empty()) {
        predicate.push_str(" AND m.id IN (SELECT message_id FROM message_categories WHERE account=?1 AND category_id IN (SELECT value FROM json_each(?3)))");
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
    if p.get("exact_person").is_some() {
        // The sender as above, the first To address on the row, or any
        // From/To/Cc person the link index has read from the source.
        predicate.push_str(&format!(
            " AND (trim(m.sender_email)=?12 COLLATE NOCASE OR m.sender_email GLOB '*[^ -~]*' OR {EXACT_LISTED})"
        ));
    }
    if let Some(has) = text.attachment {
        predicate.push_str(if has {
            " AND m.id IN (SELECT message_id FROM attachments WHERE account=?1 AND message_id IS NOT NULL)"
        } else {
            " AND m.id NOT IN (SELECT message_id FROM attachments WHERE account=?1 AND message_id IS NOT NULL)"
        });
    }
    if p["after"].as_str().is_some_and(|s| !s.is_empty()) {
        predicate.push_str(" AND julianday(m.sent_at)>=julianday(?10)");
    }
    if p["before"].as_str().is_some_and(|s| !s.is_empty()) {
        predicate.push_str(" AND julianday(m.sent_at)<julianday(?11)");
    }
    // Text predicates take the slots after the twelve typed ones, in order.
    let mut slots = 12;
    for role in text.people {
        slots += 1;
        let sender = format!(
            "m.sender_name LIKE ?{slots} ESCAPE '\\' OR m.sender_email LIKE ?{slots} ESCAPE '\\'"
        );
        let recipient = format!("m.recipient LIKE ?{slots} ESCAPE '\\' OR EXISTS (SELECT 1 FROM message_people mp JOIN people p ON p.id=mp.person WHERE mp.message_rowid=m.rowid AND p.email<>lower(trim(m.sender_email)) AND (p.email LIKE ?{slots} ESCAPE '\\' OR p.name LIKE ?{slots} ESCAPE '\\'))");
        predicate.push_str(&match role {
            PersonRole::Sender => format!(" AND ({sender})"),
            PersonRole::Recipient => format!(" AND ({recipient})"),
            PersonRole::Either => format!(" AND ({sender} OR {recipient})"),
        });
    }
    if text.label {
        slots += 1;
        predicate.push_str(&format!(" AND m.id IN (SELECT message_id FROM message_categories WHERE account=?1 AND category_id=?{slots})"));
    }
    let bindings = (1..=slots)
        .map(|slot| format!("?{slot}"))
        .collect::<Vec<_>>()
        .join(",");
    // Reserve stable numeric slots without forcing unrequested columns into scans.
    let source = if search {
        "message_fts JOIN messages m ON m.rowid=message_fts.rowid"
    } else {
        "messages m"
    };
    // The exact-address check in Rust compares this column; a listed
    // recipient already matched exactly, so it reports the wanted address.
    let exact_column = if p.get("exact_sender").is_some() {
        "m.sender_email".to_owned()
    } else if p.get("exact_person").is_some() {
        format!("CASE WHEN {EXACT_LISTED} THEN ?12 ELSE m.sender_email END")
    } else {
        "''".to_owned()
    };
    let text_columns = if text.texts {
        ",m.subject,m.body,m.sender_name||' '||m.sender_email"
    } else {
        ""
    };
    format!("WITH bindings AS (SELECT {bindings}) SELECT m.conversation_id,m.rowid,m.sent_at,{exact_column}{text_columns} FROM {source} WHERE {predicate}")
}

#[cfg(test)]
mod projection_tests {
    use super::*;
    #[test]
    fn inbox_counts_leave_out_trash_and_archived_mail() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        store.db.execute("UPDATE messages SET unread=0,starred=0,inbox=1,sent=0,trashed=0 WHERE account='windscribe'", []).unwrap();
        let ids: Vec<String> = {
            let mut statement = store.db.prepare("SELECT DISTINCT conversation_id FROM messages WHERE account='windscribe' ORDER BY conversation_id LIMIT 4").unwrap();
            let rows = statement
                .query_map([], |r| r.get(0))
                .unwrap()
                .collect::<std::result::Result<_, _>>()
                .unwrap();
            rows
        };
        for (id, update) in ids.iter().zip([
            "unread=1",
            "unread=1,inbox=0",
            "unread=1,trashed=1",
            "starred=1,trashed=1",
        ]) {
            store.db.execute(&format!("UPDATE messages SET {update} WHERE account='windscribe' AND conversation_id=?1"), [id]).unwrap();
        }
        let counts = store.compute_account_counts("windscribe").unwrap();
        assert_eq!(counts["inbox_review_count"], 1);
        assert_eq!(counts["inbox_held_count"], 0);
        assert_eq!(counts["unread_conversation_count"], 3);
        assert_eq!(counts["held_conversation_count"], 1);
    }
    #[test]
    fn received_scope_excludes_sent_only_and_trashed_members_but_keeps_full_threads() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let (cid, received): (String, String) = store.db.query_row(
            "SELECT conversation_id,id FROM messages WHERE account='windscribe' AND conversation_id IN (SELECT conversation_id FROM messages WHERE account='windscribe' GROUP BY conversation_id HAVING count(*)>1) LIMIT 1",
            [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        store
            .db
            .execute(
                "UPDATE messages SET sent=1,trashed=0,inbox=1 WHERE account='windscribe'",
                [],
            )
            .unwrap();
        store.db.execute("UPDATE messages SET sent=0,inbox=0,body='Incoming detail for UI3' WHERE account='windscribe' AND id=?1", [&received]).unwrap();
        let result = store
            .conversations(
                "windscribe",
                &json!({"folder":"received","grouped":true,"limit":100}),
            )
            .unwrap();
        assert_eq!(result["folder_conversation_count"], 1);
        assert_eq!(result["conversations"].as_array().unwrap().len(), 1);
        assert_eq!(result["conversations"][0]["id"], cid);
        assert_eq!(
            result["conversations"][0]["snippet"],
            "Incoming detail for UI3"
        );
        let thread = store.conversation("windscribe", &cid).unwrap();
        assert!(thread["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["sent"] == true));
        assert!(thread["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["sent"] == false));
        // Receiving a conversation does not invent Google INBOX membership.
        let google_inbox = store
            .conversations("windscribe", &json!({"folder":"inbox","limit":100}))
            .unwrap();
        assert_eq!(google_inbox["folder_conversation_count"], 0);
        store
            .db
            .execute(
                "UPDATE messages SET trashed=1 WHERE account='windscribe' AND id=?1",
                [&received],
            )
            .unwrap();
        drop(store);
        let store = Store::open(dir.path()).unwrap();
        let empty = store
            .conversations("windscribe", &json!({"folder":"received"}))
            .unwrap();
        assert_eq!(empty["folder_conversation_count"], 0);
        assert!(empty["conversations"].as_array().unwrap().is_empty());
        assert!(!store
            .conversations("windscribe", &json!({"folder":"sent"}))
            .unwrap()["conversations"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn folder_totals_ignore_search_filters_and_refresh_after_trash() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let all = store
            .conversations("windscribe", &json!({"folder":"all","limit":100}))
            .unwrap();
        let count = all["filtered_conversation_count"].as_i64().unwrap();
        let empty = store
            .conversations(
                "windscribe",
                &json!({"folder":"all","query":"no-such-subject-982764"}),
            )
            .unwrap();
        assert_eq!(empty["filtered_conversation_count"], 0);
        assert_eq!(empty["folder_conversation_count"], count);
        let cid = all["conversations"][0]["id"].as_str().unwrap();
        store
            .db
            .execute(
                "UPDATE messages SET trashed=1 WHERE account='windscribe' AND conversation_id=?1",
                [cid],
            )
            .unwrap();
        let changed = store
            .conversations(
                "windscribe",
                &json!({"folder":"all","query":"no-such-subject-982764"}),
            )
            .unwrap();
        assert_eq!(changed["folder_conversation_count"], count - 1);
        let trash = store
            .conversations("windscribe", &json!({"folder":"trash","limit":100}))
            .unwrap();
        assert_eq!(
            trash["folder_conversation_count"],
            trash["filtered_conversation_count"]
        );
    }

    #[test]
    fn trash_folder_is_explicit_and_restoration_does_not_invent_inbox_membership() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        let cid = "windscribe-thread-001";
        store
            .db
            .execute(
                "UPDATE messages SET trashed=1 WHERE account='windscribe' AND conversation_id=?1",
                [cid],
            )
            .unwrap();
        for folder in ["inbox", "sent", "all", "trash"] {
            let page = store
                .conversations("windscribe", &json!({"folder":folder,"limit":100}))
                .unwrap();
            let present = page["conversations"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["id"] == cid);
            assert_eq!(
                present,
                folder == "trash",
                "Unexpected membership in {folder}"
            );
        }
        store.db.execute("UPDATE messages SET trashed=0,inbox=0 WHERE account='windscribe' AND conversation_id=?1",[cid]).unwrap();
        for folder in ["inbox", "all", "trash"] {
            let page = store
                .conversations("windscribe", &json!({"folder":folder,"limit":100}))
                .unwrap();
            let present = page["conversations"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["id"] == cid);
            assert_eq!(
                present,
                folder == "all",
                "Restoration changed {folder} unexpectedly"
            );
        }
    }
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
            let mut statement = store.db.prepare("SELECT c.id,c.subject,MAX(m.unread),MAX(m.starred) FROM conversations c JOIN messages m ON m.account=c.account AND m.conversation_id=c.id WHERE c.account=?1 AND EXISTS(SELECT 1 FROM messages source WHERE source.account=c.account AND source.conversation_id=c.id AND source.inbox=1 AND source.sent=0 AND source.trashed=0) GROUP BY c.id ORDER BY CASE WHEN MAX(m.unread) THEN 0 WHEN MAX(m.starred) THEN 1 ELSE 2 END,MAX(julianday(m.sent_at)) DESC,c.id LIMIT 100").unwrap();
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
    type StateRow = (u8, bool, bool, i64, i64, u64, u8, String, i64, i64);
    fn state_rows(states: &HashMap<String, ConversationState>) -> BTreeMap<String, StateRow> {
        states
            .iter()
            .map(|(cid, s)| {
                (
                    cid.clone(),
                    (
                        s.folders,
                        s.unread,
                        s.starred,
                        s.unread_messages,
                        s.starred_messages,
                        s.latest.to_bits(),
                        s.kind,
                        s.kind_id.clone(),
                        s.messages,
                        s.max_rowid,
                    ),
                )
            })
            .collect()
    }
    /// The cached projection must equal what a full scan builds now.
    fn same_as_fresh_scan(store: &Store, account: &str) {
        let cached = state_rows(&store.conversation_states(account).unwrap());
        let kept = store.conversation_cache.borrow_mut().remove(account);
        let fresh = state_rows(&store.conversation_states(account).unwrap());
        if let Some(kept) = kept {
            store
                .conversation_cache
                .borrow_mut()
                .insert(account.into(), kept);
        }
        assert_eq!(
            cached, fresh,
            "{account} projection drifted from its messages"
        );
    }
    #[test]
    fn sync_style_writes_update_only_changed_conversations() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        correct(&store);
        let scans = store.state_scans.get();
        // Text-index and people/link index rows (virtual and WITHOUT ROWID
        // tables) used to force a rescan of every message.
        store.db.execute_batch("INSERT INTO people(email,name) VALUES('scan@example.com','Scan');
            INSERT INTO message_people(message_rowid,person) SELECT rowid,(SELECT id FROM people WHERE email='scan@example.com') FROM messages WHERE account='windscribe' LIMIT 3;
            DELETE FROM message_fts WHERE rowid=(SELECT min(rowid) FROM messages WHERE account='windscribe');
            INSERT INTO message_fts(rowid,sender,subject,body) VALUES((SELECT min(rowid) FROM messages WHERE account='windscribe'),'a','b','c');").unwrap();
        correct(&store);
        // Flags, Trash, a move between threads, a deletion and a kind change.
        store.db.execute_batch("UPDATE messages SET unread=1-unread,revision=revision+1 WHERE rowid IN (SELECT rowid FROM messages WHERE account='windscribe' ORDER BY rowid LIMIT 4);
            UPDATE messages SET trashed=1 WHERE rowid=(SELECT max(rowid) FROM messages WHERE account='personal');
            UPDATE messages SET conversation_id=(SELECT max(conversation_id) FROM messages WHERE account='windscribe') WHERE rowid=(SELECT min(rowid) FROM messages WHERE account='windscribe');
            UPDATE message_sender_kinds SET kind=CASE kind WHEN 'people' THEN 'automated' ELSE 'people' END WHERE rowid=(SELECT min(rowid) FROM message_sender_kinds);
            PRAGMA foreign_keys=OFF;
            DELETE FROM messages WHERE rowid=(SELECT max(rowid) FROM messages WHERE account='windscribe');
            PRAGMA foreign_keys=ON;").unwrap();
        correct(&store);
        for account in ["windscribe", "personal"] {
            same_as_fresh_scan(&store, account);
        }
        // same_as_fresh_scan scans on purpose, once per account.
        assert_eq!(store.state_scans.get(), scans + 2);
    }
    #[test]
    fn listing_after_a_thread_is_removed_and_another_added() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        let page = json!({"folder":"all","grouped":true,"limit":100});
        store.conversations("windscribe", &page).unwrap();
        store.db.execute_batch("PRAGMA foreign_keys=OFF;
            DELETE FROM messages WHERE account='windscribe' AND conversation_id=(SELECT min(conversation_id) FROM messages WHERE account='windscribe');
            PRAGMA foreign_keys=ON;").unwrap();
        append(&store, "slot-one", "slot-thread-one");
        append(&store, "slot-two", "slot-thread-two");
        let rows = store.conversations("windscribe", &page).unwrap();
        let ids: Vec<_> = rows["conversations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].clone())
            .collect();
        assert!(ids.contains(&json!("slot-thread-one")) && ids.contains(&json!("slot-thread-two")));
        correct(&store);
        same_as_fresh_scan(&store, "windscribe");
    }
    #[test]
    fn rolled_back_changes_never_reach_committed_states() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        let before = store.account_counts("windscribe").unwrap();
        store
            .db
            .execute_batch("BEGIN; UPDATE messages SET unread=1 WHERE account='windscribe';")
            .unwrap();
        let inside = store.account_counts("windscribe").unwrap();
        assert_eq!(
            inside["unread_conversation_count"],
            inside["conversation_count"]
        );
        store.db.execute_batch("ROLLBACK;").unwrap();
        assert_eq!(store.account_counts("windscribe").unwrap(), before);
        store
            .db
            .execute_batch(
                "SAVEPOINT probe; UPDATE messages SET starred=1 WHERE account='windscribe';",
            )
            .unwrap();
        assert_ne!(store.account_counts("windscribe").unwrap(), before);
        store
            .db
            .execute_batch("ROLLBACK TO probe; RELEASE probe;")
            .unwrap();
        assert_eq!(store.account_counts("windscribe").unwrap(), before);
        correct(&store);
        same_as_fresh_scan(&store, "windscribe");
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
            let mut durable_reader = receipt.clone();
            durable_reader.as_object_mut().unwrap().remove("mail_undo");
            assert_eq!(
                store
                    .handle("conversation", Some("windscribe"), json!({"id":cid}))
                    .unwrap(),
                durable_reader
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
        store.db.execute("INSERT OR REPLACE INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash,revision,trashed) SELECT account,id,'windscribe-thread-001',remote_id,sender_name,sender_email,recipient,subject,body,sent_at,0,0,inbox,sent,source_hash,revision,trashed FROM messages WHERE id='cache-one'",[]).unwrap();
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

    #[test]
    fn label_toggles_match_any_listed_label_and_a_label_query_decides() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path()).unwrap();
        let ids = |store: &mut Store, p: Value| -> std::collections::BTreeSet<String> {
            let mut p = p;
            p["folder"] = json!("all");
            p["limit"] = json!(100);
            store
                .handle("conversations", Some("windscribe"), p)
                .unwrap()["conversations"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["id"].as_str().unwrap().to_owned())
                .collect()
        };
        let infra = ids(&mut store, json!({"category":"infra"}));
        let finance = ids(&mut store, json!({"category":"finance"}));
        assert!(!infra.is_empty() && !finance.is_empty());
        let either = ids(&mut store, json!({"category":"infra,finance"}));
        assert_eq!(either, infra.union(&finance).cloned().collect());
        assert_eq!(
            ids(&mut store, json!({"category":"finance,infra,infra"})),
            either
        );
        // label: in the query decides; the toggles step aside.
        assert_eq!(
            ids(
                &mut store,
                json!({"category":"infra","query":"label:finance"})
            ),
            finance
        );
        assert_eq!(
            store
                .handle(
                    "conversations",
                    Some("windscribe"),
                    json!({"category":"infra,missing"})
                )
                .unwrap_err()
                .code,
            "not_found"
        );
        store
            .handle(
                "view_save",
                Some("windscribe"),
                json!({"view":{"category":"infra,finance"},"expected_revision":0,"request_id":"toggles"}),
            )
            .unwrap();
    }
}
