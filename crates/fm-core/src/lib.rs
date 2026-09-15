//! One SQLite writer and durable synthetic profile shared by every client.
use fs2::FileExt;
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};
pub mod account_lifecycle;
pub mod assist_store;
#[cfg(test)]
mod benchmark;
mod calendar_editor_options;
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
pub mod draft_discard_cleanup;
pub mod draft_transfer;
mod fixture;
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
#[cfg(test)]
mod sender_kind_tests;
#[cfg(test)]
mod test_profiles;
pub const SCHEMA_VERSION: u32 = 29;
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
            // Kind and fixture sentinel are in the first schema transaction.
            // A crash between migrations therefore cannot turn live into demo.
            let marker = if requested == Some(live_profile::ProfileKind::Live) {
                live_profile::live_initialization_marker(profile, true)?;
                "INSERT INTO meta VALUES('profile_kind',1); INSERT INTO meta VALUES('fixture_revision',0);"
            } else {
                "INSERT INTO meta VALUES('profile_kind',0);"
            };
            db.execute_batch(&include_str!("schema.sql").replace(
                "PRAGMA user_version=2;",
                &format!("{marker}\nPRAGMA user_version=2;"),
            ))?;
        }
        if version == 1 {
            db.execute_batch(include_str!("migration-002.sql"))?;
        }
        // Read the committed version again: a fresh profile starts at the same
        // historical schema used by upgrade fixtures, then follows these steps.
        let version: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 2 {
            db.execute_batch(&format!(
                "BEGIN IMMEDIATE;\n{}\n{}\nPRAGMA user_version=3;\nCOMMIT;",
                include_str!("migration-003-drafts.sql"),
                include_str!("migration-003-google.sql")
            ))?;
        }
        let version: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 3 {
            db.execute_batch(&format!(
                "BEGIN IMMEDIATE;\n{}\n{}\nPRAGMA user_version=4;\nCOMMIT;",
                include_str!("migration-004-calendar.sql"),
                include_str!("migration-004-assist.sql")
            ))?;
        }
        let version: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 4 {
            db.execute_batch(&format!(
                "BEGIN IMMEDIATE;\n{}\nPRAGMA user_version=5;\nCOMMIT;",
                include_str!("migration-005.sql")
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
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 5 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-006.sql"))?;
            store.freeze_migrated_drafts()?;
            tx.execute_batch("PRAGMA user_version=6")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 6 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-007.sql"))?;
            // Revision 3 is written by the atomic fresh seed. If interrupted
            // before this migration, reopen completes its explicit annotations.
            // Retained revision 1/2 profiles never receive fixture annotations.
            if store.fixture_revision()? == 3 {
                fixture::seed_sender_kinds(&tx)?;
            }
            tx.execute_batch("PRAGMA user_version=7")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 7 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-008.sql"))?;
            tx.execute_batch("PRAGMA user_version=8")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 8 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-009-runtime.sql"))?;
            tx.execute_batch(include_str!("migration-009-mail.sql"))?;
            tx.execute_batch(include_str!("migration-009-calendar.sql"))?;
            tx.execute_batch("PRAGMA user_version=9")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 9 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-010-mail.sql"))?;
            tx.execute_batch(include_str!("migration-010-calendar-writes.sql"))?;
            tx.execute_batch("PRAGMA user_version=10")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 10 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-011-labels.sql"))?;
            tx.execute_batch("PRAGMA user_version=11")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 11 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-012-attachments.sql"))?;
            tx.execute_batch("PRAGMA user_version=12")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 12 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-013-google-drafts.sql"))?;
            tx.execute_batch("PRAGMA user_version=13")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 13 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-014-google-send.sql"))?;
            tx.execute_batch("PRAGMA user_version=14")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 14 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-015-google-draft-inventory.sql"))?;
            tx.execute_batch("PRAGMA user_version=15")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 15 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-016-google-draft-actions.sql"))?;
            tx.execute_batch("PRAGMA user_version=16")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 16 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-017-calendar-invitations.sql"))?;
            tx.execute_batch("PRAGMA user_version=17")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 17 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-018-account-lifecycle.sql"))?;
            tx.execute_batch("PRAGMA user_version=18")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 18 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-019-calendar-occurrences.sql"))?;
            tx.execute_batch("PRAGMA user_version=19")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 19 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-020-calendar-workflows.sql"))?;
            tx.execute_batch("PRAGMA user_version=20")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 20 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-021-ollama.sql"))?;
            tx.execute_batch("PRAGMA user_version=21")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 21 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch("CREATE TABLE local_model_label_overrides(account TEXT NOT NULL,message_id TEXT NOT NULL,PRIMARY KEY(account,message_id),FOREIGN KEY(account,message_id) REFERENCES messages(account,id) ON DELETE CASCADE); CREATE TABLE local_model_settings_history(revision INTEGER PRIMARY KEY,settings_json TEXT NOT NULL); INSERT INTO local_model_settings_history SELECT revision,settings_json FROM local_model_settings;")?;
            let old: Option<String> = tx
                .query_row(
                    "SELECT settings_json FROM local_model_settings WHERE id=1",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(old) = old {
                let settings = fm_assist::ollama::upgrade_settings(serde_json::from_str(&old)?)
                    .map_err(|s| err("storage_error", &s))?;
                tx.execute("UPDATE local_model_settings SET revision=revision+1,settings_json=?1 WHERE id=1",[serde_json::to_string(&settings)?])?;
            }
            tx.execute_batch("PRAGMA user_version=22")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 22 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch("INSERT INTO local_model_settings_history SELECT revision,settings_json FROM local_model_settings;")?;
            let old: Option<String> = tx
                .query_row(
                    "SELECT settings_json FROM local_model_settings WHERE id=1",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(old) = old {
                let settings = fm_assist::ollama::upgrade_settings(serde_json::from_str(&old)?)
                    .map_err(|s| err("storage_error", &s))?;
                tx.execute("UPDATE local_model_settings SET revision=revision+1,settings_json=?1 WHERE id=1", [serde_json::to_string(&settings)?])?;
            }
            tx.execute_batch("PRAGMA user_version=23")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 23 {
            let tx = store.db.unchecked_transaction()?;
            let old: Option<String> = tx
                .query_row(
                    "SELECT settings_json FROM local_model_settings WHERE id=1",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(old) = old {
                let mut settings: Value = serde_json::from_str(&old)?;
                let mut request: Value = serde_json::from_str(
                    settings["prompt"]
                        .as_str()
                        .ok_or_else(|| err("storage_error", "Missing prompt"))?,
                )?;
                if request["options"]["num_predict"] == 1024 {
                    tx.execute_batch("INSERT INTO local_model_settings_history SELECT revision,settings_json FROM local_model_settings")?;
                    request["options"]["num_predict"] = json!(4096);
                    settings["prompt"] = serde_json::to_string_pretty(&request)?.into();
                    tx.execute("UPDATE local_model_settings SET revision=revision+1,settings_json=?1 WHERE id=1",[serde_json::to_string(&settings)?])?;
                }
            }
            tx.execute_batch("PRAGMA user_version=24")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 24 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-025-local-providers.sql"))?;
            tx.execute_batch("PRAGMA user_version=25")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 25 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-026-label-creation.sql"))?;
            tx.execute_batch("PRAGMA user_version=26")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 26 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-027-draft-transfers.sql"))?;
            tx.execute_batch("PRAGMA user_version=27")?;
            tx.commit()?;
        }
        let version: u32 = store
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 27 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-028-draft-discard.sql"))?;
            tx.execute_batch("PRAGMA user_version=28")?;
            tx.commit()?;
        }
        let version: u32 = store.db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 28 {
            let tx = store.db.unchecked_transaction()?;
            tx.execute_batch(include_str!("migration-029-draft-discard-cleanup.sql"))?;
            tx.execute_batch("PRAGMA user_version=29")?;
            tx.commit()?;
        }
        store.validate_profile_identity()?;
        store.recover_google_category_projections()?;
        store.recover_account_cleanup()?;
        store.recover_captures()?;
        store.recover_google_drafts()?;
        store.recover_google_sends()?;
        store.recover_google_draft_deletions()?;
        store.calendar_live_recover()?;
        store.mail_changes.reset_calendar_generation();
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
                json!({"schema_version":SCHEMA_VERSION,"fixture_revision":self.fixture_revision()?,"revision":self.revision()?,"calendar_generation":self.mail_changes.calendar_generation(),"synthetic":synthetic,"profile_kind":if synthetic {"synthetic"} else {"live"},"fixture_today":if synthetic {Some(FIXTURE_TODAY)} else {None},"capabilities":{"mail":true,"drafts":true,"calendar_read":true,"phase1":synthetic,"typed_filters":true,"preferences":true,"capture_history":synthetic,"calendar_write_local":synthetic,"send":!synthetic,"models":true,"google_auth":!synthetic,"google_sync":!synthetic}}),
            );
        }
        match method {
            "preferences_get" => {
                fields(&params, &[])?;
                return self.preferences();
            }
            "preferences_save" => return self.preferences_save(&params),
            "calendar_sources" => {
                fields(&params, &[])?;
                return self.calendar_sources();
            }
            "calendar_combined" => return self.calendar_combined(&params),
            "calendar_search" => return self.calendar_search(&params),
            "calendar_reminders" => return self.calendar_reminders(&params),
            "calendar_reminder_ack" => return self.calendar_reminder_ack(account, &params),
            "mail_notifications" => return self.mail_notifications(&params),
            _ => {}
        }
        if ![
            "account_remove_preview",
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
            "label_create",
            "label_describe",
            "label_remove",
            "set_category",
            "mail_filing",
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
            "label_create" | "label_describe" | "label_remove" => {
                self.label_edit(account, method, &params)
            }
            "flag" | "set_category" | "mail_filing" | "draft_create" | "draft_save"
            | "draft_discard" | "draft_undo_discard" | "view_save" => {
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
