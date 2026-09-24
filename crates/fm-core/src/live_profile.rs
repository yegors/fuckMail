//! Live profile identity and verified-provider account bindings. No credentials.
use crate::{err, Result, Store};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::{io::Write, path::Path};

const LIVE_MARKER: &[u8] = b"fuckMail live profile v1\n";

pub(crate) fn live_initialization_marker(profile: &Path, create: bool) -> Result<bool> {
    let path = profile.join("live-profile");
    if create && !path.exists() {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path)?;
        file.write_all(LIVE_MARKER)?;
        file.sync_all()?;
        crate::content::sync_directory(profile)?;
    }
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() != LIVE_MARKER.len() as u64
                || std::fs::read(path)? != LIVE_MARKER
            {
                return Err(err(
                    "profile_invalid",
                    "Live profile initialization marker is invalid",
                ));
            }
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileKind {
    Synthetic,
    Live,
}
impl ProfileKind {
    pub(crate) fn from_stored(value: i64) -> Result<Self> {
        match value {
            0 => Ok(Self::Synthetic),
            1 => Ok(Self::Live),
            _ => Err(err("profile_invalid", "Unknown profile kind")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Connected,
    Disconnected,
    ReauthorizationRequired,
}
impl ConnectionState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Disconnected => "disconnected",
            Self::ReauthorizationRequired => "reauthorization_required",
        }
    }
}

/// Returned only to the engine's provider coordinator, never as credential data.
pub struct GoogleAccountBinding {
    pub account: String,
    pub subject: String,
    pub scopes: BTreeSet<String>,
    pub state: ConnectionState,
    pub connection_generation: i64,
}

#[derive(Clone)]
pub struct GoogleClientReference {
    pub client_id: String,
    pub config_path: String,
}

/// Stable local identity uses Google's verified immutable `sub`, never email.
pub fn google_account_id(subject: &str) -> Result<String> {
    if subject.is_empty() || subject.len() > 255 || subject.chars().any(char::is_control) {
        return Err(err("invalid_identity", "Invalid Google account identity"));
    }
    Ok(format!("google-{:x}", Sha256::digest(subject.as_bytes())))
}

impl Store {
    pub fn profile_kind(&self) -> Result<ProfileKind> {
        let value =
            self.db
                .query_row("SELECT value FROM meta WHERE key='profile_kind'", [], |r| {
                    r.get(0)
                })?;
        ProfileKind::from_stored(value)
    }

    /// Opaque per-profile namespace prevents credentials crossing profile stores.
    pub fn vault_namespace(&self) -> Result<String> {
        Ok(self.db.query_row(
            "SELECT vault_namespace FROM profile_identity WHERE singleton=1",
            [],
            |r| r.get(0),
        )?)
    }

    pub(crate) fn validate_profile_identity(&self) -> Result<()> {
        let kind = self.profile_kind()?;
        let fixture = self.fixture_revision()?;
        if (kind == ProfileKind::Live) != (fixture == 0)
            || fixture < 0
            || fixture > i64::from(crate::FIXTURE_REVISION)
        {
            return Err(err(
                "profile_invalid",
                "Profile kind and fixture identity disagree",
            ));
        }
        let namespace = self.vault_namespace()?;
        if namespace.len() != 64 || !namespace.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(err("profile_invalid", "Profile vault namespace is invalid"));
        }
        if kind == ProfileKind::Live {
            let unbound: bool = self.db.query_row("SELECT EXISTS(SELECT 1 FROM accounts a LEFT JOIN google_accounts g ON g.account=a.id WHERE a.provider!='google' OR g.account IS NULL)", [], |r| r.get(0))?;
            if unbound {
                return Err(err(
                    "profile_invalid",
                    "Live profile contains an unbound account",
                ));
            }
        }
        Ok(())
    }

    /// Internal engine API, after OAuth validates identity and persists the grant
    /// in this profile's vault. Deliberately absent from the public IPC allowlist.
    pub fn bind_google_account(
        &mut self,
        subject: &str,
        email: &str,
        scopes: &BTreeSet<String>,
        expected_account: Option<&str>,
    ) -> Result<String> {
        self.bind_google_account_internal(subject, email, scopes, expected_account, None)
    }

    /// Read-only admission before durable vault persistence. Repeated at commit.
    pub fn check_google_connection(
        &self,
        subject: &str,
        email: &str,
        scopes: &BTreeSet<String>,
        expected_account: Option<&str>,
        client_id: &str,
    ) -> Result<String> {
        let account = self.validate_google_binding(subject, email, scopes, expected_account)?;
        self.check_binding_active(&account)?;
        self.check_google_client_id(client_id)?;
        Ok(account)
    }

    pub(crate) fn check_google_client_id(&self, client_id: &str) -> Result<()> {
        if client_id.is_empty() || client_id.len() > 1024 || client_id.chars().any(char::is_control)
        {
            return Err(err("invalid_client", "Invalid desktop OAuth client"));
        }
        let different: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM google_account_clients WHERE client_id!=?1)",
            [client_id],
            |r| r.get(0),
        )?;
        if different {
            return Err(err(
                "client_mismatch",
                "Use the desktop OAuth client already connected to this profile",
            ));
        }
        Ok(())
    }

    pub fn bind_google_account_with_client(
        &mut self,
        subject: &str,
        email: &str,
        scopes: &BTreeSet<String>,
        expected_account: Option<&str>,
        client_id: &str,
        config_path: &str,
    ) -> Result<String> {
        self.check_google_connection(subject, email, scopes, expected_account, client_id)?;
        validate_client_path(config_path)?;
        self.bind_google_account_internal(
            subject,
            email,
            scopes,
            expected_account,
            Some((client_id, config_path)),
        )
    }

    pub fn google_account_client(&self, account: &str) -> Result<Option<GoogleClientReference>> {
        Ok(self
            .db
            .query_row(
                "SELECT client_id,config_path FROM google_account_clients WHERE account=?1",
                [account],
                |r| {
                    Ok(GoogleClientReference {
                        client_id: r.get(0)?,
                        config_path: r.get(1)?,
                    })
                },
            )
            .optional()?)
    }

    pub(crate) fn validate_google_binding(
        &self,
        subject: &str,
        email: &str,
        scopes: &BTreeSet<String>,
        expected_account: Option<&str>,
    ) -> Result<String> {
        if self.profile_kind()? != ProfileKind::Live {
            return Err(err(
                "profile_kind_mismatch",
                "Connect Google accounts in a separate live profile",
            ));
        }
        let base = google_account_id(subject)?;
        let existing: Option<String> = self
            .db
            .query_row(
                "SELECT account FROM google_accounts WHERE google_subject=?1",
                [subject],
                |r| r.get(0),
            )
            .optional()?;
        let replacement: Option<String> = self.db.query_row("SELECT replacement_account FROM deleted_account_archives WHERE base_account=?1 ORDER BY rowid DESC LIMIT 1",[&base],|r|r.get(0)).optional()?;
        let account = existing.or(replacement).unwrap_or(base);
        if expected_account.is_some_and(|expected| expected != account) {
            return Err(err(
                "identity_mismatch",
                "Reconnect selected a different Google account",
            ));
        }
        if email.is_empty()
            || email.len() > 320
            || !email.contains('@')
            || email.chars().any(char::is_control)
            || scopes.len() > 64
            || scopes
                .iter()
                .any(|s| s.is_empty() || s.len() > 512 || s.chars().any(char::is_whitespace))
        {
            return Err(err(
                "invalid_identity",
                "Invalid Google account metadata or scopes",
            ));
        }
        let existing: Option<String> = self
            .db
            .query_row(
                "SELECT google_subject FROM google_accounts WHERE account=?1",
                [&account],
                |r| r.get(0),
            )
            .optional()?;
        if existing
            .as_deref()
            .is_some_and(|existing| existing != subject)
        {
            return Err(err(
                "identity_mismatch",
                "Stored Google account identity differs",
            ));
        }
        if expected_account.is_some() && existing.is_none() {
            return Err(err("not_found", "Reconnect account is not in this profile"));
        }
        Ok(account)
    }

    fn bind_google_account_internal(
        &mut self,
        subject: &str,
        email: &str,
        scopes: &BTreeSet<String>,
        expected_account: Option<&str>,
        client: Option<(&str, &str)>,
    ) -> Result<String> {
        let account = self.validate_google_binding(subject, email, scopes, expected_account)?;
        self.check_binding_active(&account)?;
        let tx = self.db.unchecked_transaction()?;
        write_binding(&tx, &account, subject, email, scopes, client)?;
        tx.commit()?;
        Ok(account)
    }

    fn check_binding_active(&self, account: &str) -> Result<()> {
        let exists: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=?1)",
            [account],
            |r| r.get(0),
        )?;
        if exists {
            self.require_active_account(account)?;
        }
        Ok(())
    }

    pub fn google_account_bindings(&self) -> Result<Vec<GoogleAccountBinding>> {
        let mut statement = self.db.prepare("SELECT account,google_subject,scopes_json,connection_state,connection_generation FROM google_accounts ORDER BY account")?;
        let rows = statement
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(account, subject, scopes, state, connection_generation)| {
                Ok(GoogleAccountBinding {
                    account,
                    subject,
                    scopes: serde_json::from_str(&scopes)?,
                    state: serde_json::from_value(json!(state))?,
                    connection_generation,
                })
            })
            .collect()
    }

    /// Stops provider scheduling without removing downloaded or authored content.
    pub fn set_google_connection_state(
        &mut self,
        account: &str,
        state: ConnectionState,
    ) -> Result<()> {
        self.require_active_account(account)?;
        let tx = self.db.unchecked_transaction()?;
        if tx.execute(
            "UPDATE google_accounts SET connection_state=?2,revision=revision+1,connection_generation=connection_generation+1 WHERE account=?1",
            params![account, state.as_str()],
        )? != 1
        {
            return Err(err("not_found", "Google account is not in this profile"));
        }
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn google_account_status(&self, account: &str) -> Result<Option<Value>> {
        let raw: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT connection_state,scopes_json FROM google_accounts WHERE account=?1",
                [account],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        raw.map(|(state,scopes)| {
            let scopes: BTreeSet<String> = serde_json::from_str(&scopes)?;
            let coverage=self.google_live_coverage(account)?;
            let started:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM google_live_lanes WHERE account=?1)",[account],|row|row.get(0))?;
            let active=self.account_lifecycle(account)?["state"]=="active";
            let connected=state=="connected" && active;
            let mail_write=active && fm_google::auth::ScopeSet::new(&scopes).mail_write() && crate::preferences::google_write_enabled(&self.db,account,"mail")?;
            let calendar_write=connected && fm_google::auth::ScopeSet::new(&scopes).calendar_events_write() && crate::preferences::google_write_enabled(&self.db,account,"calendar")?;
            Ok(json!({"connection_state":state,"granted_scopes":scopes,
                "granted_capabilities":{"mail_read":fm_google::auth::ScopeSet::new(&scopes).mail_read(),"mail_write":fm_google::auth::ScopeSet::new(&scopes).mail_write(),"calendar_read":fm_google::auth::ScopeSet::new(&scopes).calendar_events_read(),"calendar_write":fm_google::auth::ScopeSet::new(&scopes).calendar_events_write()},
                "capabilities":{"mail_read":active,"drafts":active,"mail_write":mail_write,"mail_delivery":mail_write && connected,"send":mail_write && connected,"calendar_read":active,"calendar_write":calendar_write},
                "sync":{"state":if !started {"not_started"} else if coverage.offline_complete {"caught_up"} else {"incomplete"},"inventory_complete":coverage.inventory_complete,"delta_current":coverage.delta_current,"offline_complete":coverage.offline_complete,"observed_messages":coverage.observed,"retained_sources":coverage.retained_sources,"indexed_messages":coverage.indexed,"unavailable_messages":coverage.unavailable}}))
        }).transpose()
    }
}

pub(crate) fn validate_client_path(config_path: &str) -> Result<()> {
    if !Path::new(config_path).is_absolute()
        || config_path.len() > 32768
        || config_path.chars().any(char::is_control)
    {
        return Err(err(
            "invalid_client",
            "Choose an absolute desktop OAuth configuration path",
        ));
    }
    Ok(())
}
pub(crate) fn write_binding(
    tx: &rusqlite::Transaction<'_>,
    account: &str,
    subject: &str,
    email: &str,
    scopes: &BTreeSet<String>,
    client: Option<(&str, &str)>,
) -> Result<()> {
    let added: bool = tx.query_row(
        "SELECT NOT EXISTS(SELECT 1 FROM accounts WHERE id=?1)",
        [account],
        |r| r.get(0),
    )?;
    if added {
        // A new account imports its Gmail labels once, before model labeling needs them.
        tx.execute(
            "INSERT OR IGNORE INTO meta(key,value) VALUES(?1,1)",
            [crate::label_sync::initial_import_key(account)],
        )?;
    }
    tx.execute("INSERT INTO accounts(id,name,email,provider) VALUES(?1,?2,?2,'google') ON CONFLICT(id) DO UPDATE SET name=CASE WHEN accounts.name=accounts.email THEN excluded.email ELSE accounts.name END,email=excluded.email",params![account,email])?;
    tx.execute(
        "INSERT OR IGNORE INTO account_metadata(account,signature_text,mail_retention,mail_since) VALUES(?1,'','1_year',?2)",
        params![account,crate::preferences::mail_retention_since("1_year")?],
    )?;
    tx.execute("INSERT INTO google_accounts(account,google_subject,scopes_json,connection_state) VALUES(?1,?2,?3,'connected') ON CONFLICT(account) DO UPDATE SET scopes_json=excluded.scopes_json,connection_state='connected',revision=google_accounts.revision+1,connection_generation=google_accounts.connection_generation+1",params![account,subject,serde_json::to_string(scopes)?])?;
    if let Some((client_id, config_path)) = client {
        tx.execute("INSERT INTO google_account_clients(account,client_id,config_path) VALUES(?1,?2,?3) ON CONFLICT(account) DO UPDATE SET client_id=excluded.client_id,config_path=excluded.config_path",params![account,client_id,config_path])?;
    }
    tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
    Ok(())
}

#[cfg(test)]
#[path = "live_profile_tests.rs"]
mod tests;
