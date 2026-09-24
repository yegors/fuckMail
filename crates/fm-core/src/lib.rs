//! One SQLite writer and durable synthetic profile shared by every client.
use fs2::FileExt;
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};
mod account_archives;
pub mod account_lifecycle;
mod account_retention;
pub mod assist_backlog;
pub mod assist_store;
#[cfg(test)]
mod benchmark;
mod calendar_editor_options;
mod calendar_mail_activity;
mod calendar_projection;
mod calendar_reminders;
mod calendar_search;
pub mod calendar_store;
mod cancellation;
mod capture;
mod composition;
#[cfg(test)]
mod composition_tests;
mod content;
mod draft_discard;
pub mod draft_transfer;
mod fixture;
#[cfg(test)]
mod fresh_schema_tests;
pub mod google_draft_inventory;
pub mod label_sync;
pub mod local_labels;
pub mod local_model;
mod recipient_suggestions;
pub use google_draft_inventory::actions as google_draft_actions;
pub mod google_drafts;
pub mod google_labels;
pub mod google_live_mail;
pub mod google_live_mutations;
pub mod google_send;
pub mod google_store;
pub mod html_content;
pub mod link_index;
pub mod live_calendar;
mod live_calendar_invitations;
pub mod live_calendar_occurrences;
pub mod live_calendar_recurring;
pub mod live_calendar_writes;
pub mod live_profile;
pub mod local_content;
pub mod message_source;
mod mutations;
mod phase1_calendar;
#[cfg(test)]
mod phase1_tests;
mod preferences;
mod queries;
pub mod remote_prefs;
mod search_query;
mod search_senders;
#[cfg(test)]
mod sender_kind_tests;
pub mod view_cache;
const RESET_BASELINE_VERSION: u32 = 32;
// Append one SQL step per version after the fresh baseline. Each step commits
// with its own user_version update before normal recovery begins.
const POST_RESET_MIGRATIONS: &[&str] = &[
    // 33: Calendar mail counts read one date range instead of the whole mailbox.
    "CREATE INDEX google_mail_coverage_by_date ON google_mail_coverage(account,internal_date_ms) WHERE deleted=0;",
    // 34: link hosts per message, for link hover cards and blocked-domain counts.
    link_index::SCHEMA,
    // 35: people per message, for the list's "+N"; the link pass runs again to fill them.
    concat!(
        "CREATE TABLE people(id INTEGER PRIMARY KEY,email TEXT NOT NULL UNIQUE,name TEXT NOT NULL) STRICT;",
        "CREATE TABLE message_people(message_rowid INTEGER NOT NULL REFERENCES messages(rowid) ON DELETE CASCADE,person INTEGER NOT NULL REFERENCES people(id),PRIMARY KEY(message_rowid,person)) STRICT, WITHOUT ROWID;",
        "UPDATE link_index_state SET indexed_rowid=0;"
    ),
];
pub const SCHEMA_VERSION: u32 = RESET_BASELINE_VERSION + POST_RESET_MIGRATIONS.len() as u32;
pub const FIXTURE_REVISION: u32 = 3;
pub const FIXTURE_TODAY: &str = "2026-09-11";
#[derive(Debug)]
pub struct CoreError {
    pub code: String,
    pub message: String,
    pub details: Value,
}
impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for CoreError {}
pub type Result<T> = std::result::Result<T, CoreError>;
pub(crate) fn err(code: &str, message: &str) -> CoreError {
    CoreError {
        code: code.into(),
        message: message.into(),
        details: Value::Null,
    }
}
impl From<rusqlite::Error> for CoreError {
    fn from(value: rusqlite::Error) -> Self {
        err("storage_error", &format!("SQLite: {value}"))
    }
}
impl From<std::io::Error> for CoreError {
    fn from(value: std::io::Error) -> Self {
        err("storage_error", &format!("Local storage: {}", value.kind()))
    }
}
impl From<serde_json::Error> for CoreError {
    fn from(_: serde_json::Error) -> Self {
        err("invalid_request", "Invalid JSON data")
    }
}
pub struct Store {
    pub(crate) db: Connection,
    pub(crate) profile: PathBuf,
    _lock: File,
    pub(crate) conversation_cache: queries::ConversationCache,
    pub(crate) projection_cache: queries::ProjectionCache,
    pub(crate) coverage_cache: queries::CoverageCache,
    pub(crate) mail_changes: queries::MailChanges,
    pub(crate) planner_refreshed_at: std::cell::Cell<Option<std::time::Instant>>,
}
impl Store {
    pub fn open(profile: &Path) -> Result<Self> {
        Self::open_profile(profile, None)
    }

    /// Explicit live creation never seeds demo content, including after a crash.
    /// Existing profiles must already be live; this never converts a demo store.
    pub fn open_live(profile: &Path) -> Result<Self> {
        Self::open_profile(profile, Some(live_profile::ProfileKind::Live))
    }

    /// Explicit demo creation never opens or converts an existing live store.
    pub fn open_synthetic(profile: &Path) -> Result<Self> {
        Self::open_profile(profile, Some(live_profile::ProfileKind::Synthetic))
    }

    fn open_profile(profile: &Path, requested: Option<live_profile::ProfileKind>) -> Result<Self> {
        std::fs::create_dir_all(profile)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(profile.join("store.lock"))?;
        lock.try_lock_exclusive()
            .map_err(|_| err("store_in_use", "This profile already has a writer"))?;
        // Persist live creation intent before the first SQLite transaction. A
        // killed first transaction must not make a later ordinary open seed demo.
        let marker = live_profile::live_initialization_marker(
            profile,
            requested == Some(live_profile::ProfileKind::Live)
                && !profile.join("store.sqlite3").exists(),
        )?;
        if marker && requested == Some(live_profile::ProfileKind::Synthetic) {
            return Err(err(
                "profile_kind_mismatch",
                "A live profile cannot be opened as a demo profile",
            ));
        }
        let requested = if marker {
            Some(live_profile::ProfileKind::Live)
        } else {
            requested
        };
        let db = Connection::open(profile.join("store.sqlite3"))?;
        db.busy_timeout(std::time::Duration::from_secs(2))?;
        db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA trusted_schema=OFF;")?;
        let version: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(err("schema_mismatch", "This profile needs a newer engine"));
        }
        if version != 0 && version < RESET_BASELINE_VERSION {
            return Err(err(
                "schema_mismatch",
                "This profile was made by an older fuckMail and cannot be opened. Move its folder aside, then try again to start fresh and reconnect your accounts.",
            ));
        }
        if version >= RESET_BASELINE_VERSION {
            for (index, sql) in POST_RESET_MIGRATIONS.iter().enumerate() {
                let next = RESET_BASELINE_VERSION + index as u32 + 1;
                if version < next {
                    let tx = db.unchecked_transaction()?;
                    tx.execute_batch(sql)?;
                    tx.pragma_update(None, "user_version", next)?;
                    tx.commit()?;
                }
            }
        }
        if version != 0 {
            let stored_kind: Option<i64> = db
                .query_row("SELECT value FROM meta WHERE key='profile_kind'", [], |r| {
                    r.get(0)
                })
                .optional()?;
            let kind = live_profile::ProfileKind::from_stored(stored_kind.unwrap_or(0))?;
            if requested.is_some_and(|requested| requested != kind) {
                return Err(err(
                    "profile_kind_mismatch",
                    "The selected profile kind differs from the requested profile kind",
                ));
            }
        }
        if version == 0 {
            // Kind, fixture sentinel and final schema version commit together.
            // A killed first transaction cannot turn live into demo.
            let marker = if requested == Some(live_profile::ProfileKind::Live) {
                live_profile::live_initialization_marker(profile, true)?;
                "INSERT INTO meta VALUES('profile_kind',1); INSERT INTO meta VALUES('fixture_revision',0);"
            } else {
                "INSERT INTO meta VALUES('profile_kind',0);"
            };
            db.execute_batch(&format!(
                "BEGIN IMMEDIATE;\n{}\nINSERT INTO meta VALUES('revision',1);\n{marker}\nINSERT INTO profile_identity VALUES(1,lower(hex(randomblob(32))));\nINSERT INTO calendar_publication VALUES(1,0);\nPRAGMA user_version={SCHEMA_VERSION};\nCOMMIT;",
                include_str!("fresh-schema.sql")
            ))?;
        }
        let mut store = Self {
            mail_changes: queries::MailChanges::install(&db),
            db,
            profile: profile.to_owned(),
            _lock: lock,
            projection_cache: Default::default(),
            coverage_cache: Default::default(),
            conversation_cache: Default::default(),
            planner_refreshed_at: Default::default(),
        };
        let seeded: Option<i64> = store
            .db
            .query_row(
                "SELECT value FROM meta WHERE key='fixture_revision'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if seeded.is_none() {
            let live: bool = store.db.query_row(
                "SELECT COALESCE((SELECT value FROM meta WHERE key='profile_kind'),0)=1",
                [],
                |r| r.get(0),
            )?;
            if live {
                return Err(err(
                    "profile_invalid",
                    "Live profile fixture marker is missing",
                ));
            }
            store.seed()?;
        }
        store.cleanup_deleted_archive_blobs()?;
        store.validate_profile_identity()?;
        store.recover_google_category_projections()?;
        store.recover_account_cleanup()?;
        store.recover_captures()?;
        store.recover_google_drafts()?;
        store.recover_google_sends()?;
        store.recover_google_draft_deletions()?;
        store.calendar_live_recover()?;
        store.mail_changes.reset_calendar_generation();
        store.db.execute_batch("PRAGMA optimize=0x10002;")?;
        Ok(store)
    }
    pub fn handle(&mut self, method: &str, account: Option<&str>, params: Value) -> Result<Value> {
        if !params.is_object() {
            return Err(err("invalid_request", "Parameters must be an object"));
        }
        if self.profile_kind()? == live_profile::ProfileKind::Live
            && matches!(
                method,
                "draft_capture" | "calendar_write_ack" | "calendar_write"
            )
        {
            return Err(err(
                "capability_unavailable",
                "This operation is not available for a live account yet",
            ));
        }
        // Direct Rust callers retain a synchronous facade. The engine routes
        // these methods through the same opaque plan on its blocking workers.
        if matches!(
            method,
            "draft_save" | "draft_capture" | "draft_attachment_import"
        ) {
            let account = account.ok_or_else(|| err("account_required", "Choose an account"))?;
            let mut step = self.local_content_begin(method, account, params)?;
            for _ in 0..2 {
                match step {
                    local_content::LocalContentStep::Reply(reply) => return Ok(reply),
                    local_content::LocalContentStep::Prepare(plan) => {
                        let proof = local_content::prepare_local_content(
                            plan,
                            &fm_google::transport::CancellationToken::new(),
                        )?;
                        step = self.local_content_finish(proof)?;
                    }
                }
            }
            return match step {
                local_content::LocalContentStep::Reply(reply) => Ok(reply),
                _ => Err(err(
                    "stale_preparation",
                    "Local content changed during preparation",
                )),
            };
        }
        if method.starts_with("assist_") {
            return self.local_model_handle(method, account, &params);
        }
        if method == "accounts" {
            fields(&params, &["include_removed"])?;
            let include_removed = match params.get("include_removed") {
                None => false,
                Some(Value::Bool(v)) => *v,
                _ => return Err(err("invalid_request", "include_removed must be boolean")),
            };
            return self.accounts_including_removed(include_removed);
        }
        if method == "status" {
            fields(&params, &[])?;
            let synthetic = self.profile_kind()? == live_profile::ProfileKind::Synthetic;
            // These describe callable local interfaces. Account grants and
            // connectivity independently gate provider delivery and writes.
            return Ok(
                json!({"schema_version":SCHEMA_VERSION,"fixture_revision":self.fixture_revision()?,"revision":self.revision()?,"calendar_generation":self.mail_changes.calendar_generation(),"mail_generation":self.mail_generation(),"preferences_revision":self.preferences_revision()?,"synthetic":synthetic,"profile_kind":if synthetic {"synthetic"} else {"live"},"fixture_today":if synthetic {Some(FIXTURE_TODAY)} else {None},"capabilities":{"mail":true,"drafts":true,"calendar_read":true,"phase1":synthetic,"typed_filters":true,"preferences":true,"capture_history":synthetic,"calendar_write_local":synthetic,"send":!synthetic,"models":true,"google_auth":!synthetic,"google_sync":!synthetic}}),
            );
        }
        match method {
            "preferences_get" => {
                fields(&params, &[])?;
                return self.preferences();
            }
            "preferences_save" => return self.preferences_save(&params),
            "link_pattern_counts" => return self.link_pattern_counts(&params),
            "calendar_sources" => {
                fields(&params, &[])?;
                return self.calendar_sources();
            }
            "calendar_combined" => return self.calendar_combined(&params),
            "calendar_search" => return self.calendar_search(&params),
            "calendar_mail_activity" => return self.calendar_mail_activity(account, &params),
            "calendar_reminders" => return self.calendar_reminders(&params),
            "calendar_reminder_ack" => return self.calendar_reminder_ack(account, &params),
            "mail_notifications" => return self.mail_notifications(&params),
            _ => {}
        }
        if ![
            "account_remove_preview",
            "account_archive_delete",
            "account_remove_status",
            "conversations",
            "google_sent",
            "conversation",
            "flag",
            "draft_get",
            "google_drafts_status",
            "google_draft_sources",
            "google_draft_delete_status",
            "google_draft_delete_abandon",
            "draft_save",
            "draft_capture",
            "calendar",
            "attachment",
            "categories",
            "label_catalog",
            "label_auto_read",
            "label_create",
            "label_describe",
            "label_remove",
            "set_category",
            "mail_filing",
            "mail_undo",
            "google_mail_mutation_status",
            "google_labels",
            "google_category_bind",
            "view_get",
            "view_save",
            "account_update",
            "draft_discard",
            "draft_discard_status",
            "draft_undo_discard",
            "capture_history",
            "calendar_write",
            "calendar_write_ack",
            "calendar_operations",
            "drafts",
            "draft_create",
            "sender_summary",
            "recipient_suggestions",
            "search_senders",
            "link_host_summary",
            "draft_transfer_status",
            "draft_transfer_retry",
        ]
        .contains(&method)
        {
            return Err(err(
                "unsupported_method",
                "Method is not available in this phase",
            ));
        }
        let account = account.ok_or_else(|| err("account_required", "Choose an account"))?;
        id(account)?;
        if method == "account_archive_delete" {
            return self.account_archive_delete(account, &params);
        }
        if !self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=?1)",
            [account],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(err("not_found", "Account does not exist in this profile"));
        }
        match method {
            "account_remove_preview" => {
                fields(&params, &[])?;
                self.account_remove_preview(account)
            }
            "account_remove_status" => {
                fields(&params, &["request_id"])?;
                self.account_remove_status(account, string(&params, "request_id")?)
            }
            "conversations" => self.conversations(account, &params),
            "google_sent" => self.google_sent(account, &params),
            "sender_summary" => self.sender_summary(account, &params),
            "recipient_suggestions" => self.recipient_suggestions(account, &params),
            "search_senders" => self.search_senders(account, &params),
            "link_host_summary" => self.link_host_summary(account, &params),
            "draft_transfer_status" => {
                fields(&params, &["request_id"])?;
                self.draft_transfer_status(account, string(&params, "request_id")?)
            }
            "draft_transfer_retry" => self.draft_transfer_retry(account, &params),
            "conversation" => {
                fields(&params, &["id"])?;
                self.conversation(account, string(&params, "id")?)
            }
            "categories" => {
                fields(&params, &[])?;
                self.categories(account)
            }
            "calendar" => self.calendar(account, &params),
            "attachment" => {
                fields(&params, &["id"])?;
                self.attachment(account, string(&params, "id")?)
            }
            "google_drafts_status" => {
                fields(&params, &[])?;
                self.google_draft_inventory_status(account)
            }
            "google_draft_sources" => self.google_draft_source_history(account, &params),
            "google_draft_delete_status" => {
                fields(&params, &["request_id"])?;
                self.google_draft_delete_status(account, string(&params, "request_id")?)
            }
            "google_draft_delete_abandon" => self.google_draft_delete_abandon(account, &params),
            "draft_get" => {
                fields(&params, &["conversation_id", "draft_id"])?;
                self.composition(account, &params)
            }
            "drafts" => self.drafts(account, &params),
            "draft_capture" => self.draft_capture(account, &params),
            "capture_history" => self.capture_history(account, &params),
            "account_update" => self.account_update(account, &params),
            "calendar_write" => self.calendar_write(account, &params),
            "calendar_write_ack" => self.calendar_write_ack(account, &params),
            "calendar_operations" => self.calendar_operations(account, &params),
            "view_get" => {
                fields(&params, &[])?;
                self.view(account)
            }
            "google_mail_mutation_status" => {
                fields(&params, &["request_id"])?;
                if params.get("request_id").is_some() {
                    self.google_mail_mutation_request_status(
                        account,
                        string(&params, "request_id")?,
                    )
                } else {
                    self.google_mail_mutation_status(account)
                }
            }
            "google_labels" => self.google_labels(account, &params),
            "google_category_bind" => self.google_category_bind(account, &params),
            "label_catalog" => {
                fields(&params, &[])?;
                self.label_catalog(account)
            }
            "label_create" | "label_describe" | "label_remove" | "label_auto_read" => {
                self.label_edit(account, method, &params)
            }
            "flag" | "set_category" | "mail_filing" | "mail_undo" | "draft_create"
            | "draft_save" | "draft_discard" | "draft_undo_discard" | "view_save" => {
                self.mutate(method, account, &params)
            }
            "draft_discard_status" => self.draft_discard_status(account, &params),
            _ => unreachable!(),
        }
    }
    pub(crate) fn fixture_revision(&self) -> Result<i64> {
        Ok(self.db.query_row(
            "SELECT value FROM meta WHERE key='fixture_revision'",
            [],
            |r| r.get(0),
        )?)
    }
    pub(crate) fn revision(&self) -> Result<i64> {
        Ok(self
            .db
            .query_row("SELECT value FROM meta WHERE key='revision'", [], |r| {
                r.get(0)
            })?)
    }
}
impl Store {
    /// Open-time statistics predate a fresh profile's first sync. Refresh them
    /// as sync grows the store; SQLite bounds the work and skips unchanged tables.
    pub(crate) fn refresh_planner_statistics(&self) {
        const INTERVAL: std::time::Duration = std::time::Duration::from_secs(300);
        if !self.db.is_autocommit()
            || self
                .planner_refreshed_at
                .get()
                .is_some_and(|at| at.elapsed() < INTERVAL)
        {
            return;
        }
        if self.db.execute_batch("PRAGMA optimize=0x10002;").is_ok() {
            self.planner_refreshed_at
                .set(Some(std::time::Instant::now()));
        }
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        // SQLite bounds the analysis work. Refresh statistics after ingestion.
        let _ = self.db.execute_batch("PRAGMA optimize;");
    }
}

pub(crate) fn fields(value: &Value, allowed: &[&str]) -> Result<()> {
    if value
        .as_object()
        .is_none_or(|o| o.keys().any(|k| !allowed.contains(&k.as_str())))
    {
        return Err(err("invalid_request", "Unknown request field"));
    }
    Ok(())
}
pub(crate) fn id(value: &str) -> Result<&str> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(err("invalid_request", "Invalid identifier"));
    }
    Ok(value)
}
pub(crate) fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    id(value[key]
        .as_str()
        .ok_or_else(|| err("invalid_request", "Missing string parameter"))?)
}
