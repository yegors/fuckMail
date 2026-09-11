//! One SQLite writer and durable synthetic profile shared by every client.
use fs2::FileExt;
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};
pub mod assist_store;
#[cfg(test)]
mod benchmark;
pub mod calendar_store;
mod cancellation;
mod capture;
mod content;
mod fixture;
pub mod google_store;
mod mutations;
mod queries;
pub const SCHEMA_VERSION: u32 = 4;
pub const FIXTURE_REVISION: u32 = 1;
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
}
impl Store {
    pub fn open(profile: &Path) -> Result<Self> {
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
        let db = Connection::open(profile.join("store.sqlite3"))?;
        db.busy_timeout(std::time::Duration::from_secs(2))?;
        db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA trusted_schema=OFF;")?;
        let version: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(err("schema_mismatch", "This profile needs a newer engine"));
        }
        if version == 0 {
            db.execute_batch(include_str!("schema.sql"))?;
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
        let mut store = Self {
            db,
            profile: profile.to_owned(),
            _lock: lock,
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
            store.seed()?;
        }
        store.recover_captures()?;
        Ok(store)
    }
    pub fn handle(&mut self, method: &str, account: Option<&str>, params: Value) -> Result<Value> {
        if !params.is_object() {
            return Err(err("invalid_request", "Parameters must be an object"));
        }
        if method == "accounts" {
            fields(&params, &[])?;
            return self.accounts();
        }
        if method == "status" {
            fields(&params, &[])?;
            return Ok(
                json!({"schema_version":SCHEMA_VERSION,"fixture_revision":FIXTURE_REVISION,"revision":self.revision()?,"synthetic":true,"fixture_today":FIXTURE_TODAY,"capabilities":{"mail":true,"drafts":true,"calendar_read":true,"send":false,"models":false}}),
            );
        }
        if ![
            "conversations",
            "conversation",
            "flag",
            "draft_get",
            "draft_save",
            "draft_capture",
            "calendar",
            "attachment",
            "categories",
            "set_category",
            "view_get",
            "view_save",
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
            "conversations" => self.conversations(account, &params),
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
            "draft_get" => {
                fields(&params, &["conversation_id"])?;
                self.draft(account, string(&params, "conversation_id")?)
            }
            "draft_capture" => self.draft_capture(account, &params),
            "view_get" => {
                fields(&params, &[])?;
                self.view(account)
            }
            "flag" | "set_category" | "draft_save" | "view_save" => {
                self.mutate(method, account, &params)
            }
            _ => unreachable!(),
        }
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
