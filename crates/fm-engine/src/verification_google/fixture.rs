//! The in-memory Google fixture behind `--verification-google`: generated
//! accounts, mail, drafts and calendars served through fake HTTP and vault
//! implementations inside a disposable, marked profile. Compiled only with the
//! `verification-google` feature.
use super::MARKER;
use crate::{core_tasks::CoreBridge, google_auth};
use fm_core::{
    google_live_mail::{GooglePageIdentity, GooglePartReceipt},
    live_profile::{google_account_id, ProfileKind},
    Store,
};
use fm_google::transport::CancellationToken;
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::{BTreeSet, HashSet},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

mod accounts;
mod calendar_invitations;
mod calendar_occurrences;
mod draft_actions;
mod remote_drafts;
mod transport;
use transport::{FixtureHttp, FixtureVault};

pub(super) const RECENT_MESSAGES: usize = 60;
pub(super) const BACKFILL_MESSAGES: usize = 4096;
pub(super) const RAW_DELAY_MS: u64 = 16;
const MAX_PUBLICATIONS: usize = 4096;
const REPORT: &str = "verification-google-report.json";
const HOLD_RAW: &str = "verification-google-hold-raw";
const DRAFT_VERSION_TWO: &str = "verification-google-draft-version-2";
const ACTION_VERSION_TWO: &str = "verification-google-draft-actions-version-2";
const CLIENT: &str = "verification-google.apps.googleusercontent.com";

/// Only a new, empty, private profile may enter this mode. The marker is
/// durable before the Store is opened; failed initialization remains fenced.
pub(crate) fn claim_empty_profile(profile: &Path) -> io::Result<()> {
    if !profile.is_absolute() || !std::fs::symlink_metadata(profile)?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Use an empty absolute verification profile",
        ));
    }
    if std::fs::read_dir(profile)?.next().transpose()?.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Google verification requires an empty disposable profile",
        ));
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut marker = options.open(profile.join(MARKER))?;
    marker.write_all(b"{\"version\":1,\"scenario\":\"google-navigation\",\"isolated\":true}\n")?;
    marker.sync_all()?;
    Ok(())
}

pub(crate) fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Clone)]
pub(super) struct Account {
    pub(super) id: String,
    pub(super) key: &'static str,
    pub(super) subject: String,
    pub(super) email: String,
}
impl Account {
    fn new(key: &'static str) -> fm_core::Result<Self> {
        let subject = format!("verification-google-{key}-v1");
        Ok(Self {
            id: google_account_id(&subject)?,
            key,
            subject,
            email: format!("{key}@example.test"),
        })
    }
    fn value(&self) -> Value {
        json!({"id":self.id,"key":self.key,"subject":self.subject,"email":self.email})
    }
    pub(super) fn token(&self) -> String {
        format!("verification-{}-access-v1", self.key)
    }
}

#[derive(Default)]
pub(super) struct Counters {
    pub(super) http_started: u64,
    pub(super) http_completed: u64,
    pub(super) http_cancelled: u64,
    pub(super) http_rejected: u64,
    pub(super) raw_started: u64,
    pub(super) raw_completed: u64,
    pub(super) raw_cancelled: u64,
    pub(super) raw_active: u64,
    pub(super) raw_held_active: u64,
    pub(super) modify_completed: u64,
    pub(super) draft_created: u64,
    pub(super) draft_updated: u64,
    pub(super) draft_read: u64,
    pub(super) draft_list: u64,
    pub(super) draft_deleted: u64,
    pub(super) draft_delete_unknown: u64,
    pub(super) draft_delete_attempts: u64,
    pub(super) send_completed: u64,
    pub(super) sent_read: u64,
    pub(super) vault_loads: u64,
    pub(super) vault_stores: u64,
    pub(super) calendar_reads: u64,
    pub(super) calendar_mutations: u64,
    pub(super) calendar_mutation_attempts: u64,
    pub(super) calendar_acknowledged: u64,
    pub(super) calendar_lost_responses: u64,
}
impl Counters {
    fn value(&self) -> Value {
        json!({"http_started":self.http_started,"http_completed":self.http_completed,"http_cancelled":self.http_cancelled,"http_rejected":self.http_rejected,
            "raw_started":self.raw_started,"raw_completed":self.raw_completed,"raw_cancelled":self.raw_cancelled,"raw_active":self.raw_active,"raw_held_active":self.raw_held_active,
            "modify_completed":self.modify_completed,"draft_created":self.draft_created,"draft_updated":self.draft_updated,"draft_read":self.draft_read,"draft_list":self.draft_list,
            "send_completed":self.send_completed,"sent_read":self.sent_read,
            "draft_deleted":self.draft_deleted,"draft_delete_unknown":self.draft_delete_unknown,"draft_delete_attempts":self.draft_delete_attempts,
            "vault_loads":self.vault_loads,"vault_stores":self.vault_stores,
            "calendar_reads":self.calendar_reads,"calendar_mutations":self.calendar_mutations,"calendar_mutation_attempts":self.calendar_mutation_attempts,"calendar_acknowledged":self.calendar_acknowledged,"calendar_lost_responses":self.calendar_lost_responses})
    }
}
#[derive(Default)]
pub(super) struct Evidence {
    pub(super) counters: Counters,
    errors: Vec<&'static str>,
    publications: Vec<Value>,
    receipts: HashSet<(String, String)>,
    item_count: usize,
    pub(super) draft_publications: Vec<Value>,
    pub(super) send_publications: Vec<Value>,
    pub(super) remote_draft_reads: Vec<Value>,
    pub(super) draft_action_reads: Vec<Value>,
    pub(super) draft_deletions: Vec<Value>,
    pub(super) calendar_reads: Vec<Value>,
    pub(super) calendar_publications: Vec<Value>,
}
impl Evidence {
    pub(super) fn error(&mut self, code: &'static str) {
        if self.errors.len() < 16 && !self.errors.contains(&code) {
            self.errors.push(code);
        }
    }
    fn publication(
        &mut self,
        page: &GooglePageIdentity,
        receipt: &GooglePartReceipt,
        started: u64,
    ) {
        let ended = unix_ms();
        if ended < started || receipt.item_count == 0 {
            self.error("invalid_publication_observation");
            return;
        }
        let key = (page.account.clone(), receipt.key.clone());
        if self.receipts.contains(&key) {
            return;
        }
        if self.publications.len() >= MAX_PUBLICATIONS {
            self.error("publication_trace_limit");
            return;
        }
        self.receipts.insert(key);
        self.item_count += receipt.item_count;
        self.publications.push(json!({"account":page.account,"lane":page.lane,"receipt_key":receipt.key,"revision":receipt.revision,"item_count":receipt.item_count,
            "start_unix_ms":started,"end_unix_ms":ended}));
    }
}

thread_local! {
    static WRITER_TRACE: RefCell<Option<Arc<Mutex<Evidence>>>> = const { RefCell::new(None) };
}
pub(crate) struct WriterTrace;
impl Drop for WriterTrace {
    fn drop(&mut self) {
        WRITER_TRACE.with(|trace| *trace.borrow_mut() = None);
    }
}
/// Called only after the actual durable Core receipt succeeds. Thread-local
/// registration follows the one writer; ordinary feature unit fixtures have none.
pub(crate) fn record_publication(
    page: &GooglePageIdentity,
    receipt: &GooglePartReceipt,
    started: u64,
) {
    WRITER_TRACE.with(|trace| {
        if let Some(evidence) = trace.borrow().as_ref() {
            evidence.lock().unwrap().publication(page, receipt, started);
        }
    });
}

#[derive(Clone)]
pub(crate) struct Fixture {
    profile: PathBuf,
    accounts: Arc<Vec<Account>>,
    base_unix_secs: u64,
    started_unix_ms: u64,
    evidence: Arc<Mutex<Evidence>>,
    http: Arc<FixtureHttp>,
    vault: Arc<FixtureVault>,
    hold_raw: Arc<AtomicBool>,
    send_enabled: bool,
    remote_drafts_enabled: bool,
    remote_version_two: Arc<AtomicBool>,
    draft_actions_enabled: bool,
    action_version_two: Arc<AtomicBool>,
    calendar_enabled: bool,
    occurrences_enabled: bool,
    accounts_fixture: Option<Arc<accounts::AccountsFixture>>,
}
impl Fixture {
    #[cfg(test)]
    pub(crate) fn prepare(
        profile: &Path,
        store: &mut Store,
        send_enabled: bool,
    ) -> fm_core::Result<Self> {
        Self::prepare_with_drafts(profile, store, send_enabled, false)
    }
    #[cfg(test)]
    pub(crate) fn prepare_with_drafts(
        profile: &Path,
        store: &mut Store,
        send_enabled: bool,
        remote_drafts_enabled: bool,
    ) -> fm_core::Result<Self> {
        Self::prepare_with_actions(profile, store, send_enabled, remote_drafts_enabled, false)
    }
    #[cfg(test)]
    pub(crate) fn prepare_with_actions(
        profile: &Path,
        store: &mut Store,
        send_enabled: bool,
        remote_drafts_enabled: bool,
        draft_actions_enabled: bool,
    ) -> fm_core::Result<Self> {
        Self::prepare_with_calendar(
            profile,
            store,
            send_enabled,
            remote_drafts_enabled,
            draft_actions_enabled,
            None,
        )
    }
    #[cfg(test)]
    pub(crate) fn prepare_with_calendar(
        profile: &Path,
        store: &mut Store,
        send_enabled: bool,
        remote_drafts_enabled: bool,
        draft_actions_enabled: bool,
        calendar_base: Option<u64>,
    ) -> fm_core::Result<Self> {
        Self::prepare_with_accounts(
            profile,
            store,
            send_enabled,
            remote_drafts_enabled,
            draft_actions_enabled,
            calendar_base,
            None,
        )
    }
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_with_accounts(
        profile: &Path,
        store: &mut Store,
        send_enabled: bool,
        remote_drafts_enabled: bool,
        draft_actions_enabled: bool,
        calendar_base: Option<u64>,
        accounts_base: Option<u64>,
    ) -> fm_core::Result<Self> {
        Self::prepare_with_occurrences(
            profile,
            store,
            send_enabled,
            remote_drafts_enabled,
            draft_actions_enabled,
            calendar_base,
            accounts_base,
            None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_with_occurrences(
        profile: &Path,
        store: &mut Store,
        send_enabled: bool,
        remote_drafts_enabled: bool,
        draft_actions_enabled: bool,
        calendar_base: Option<u64>,
        accounts_base: Option<u64>,
        occurrences_base: Option<u64>,
    ) -> fm_core::Result<Self> {
        let calendar_enabled = calendar_base.is_some() || occurrences_base.is_some();
        if [
            send_enabled,
            remote_drafts_enabled,
            draft_actions_enabled,
            calendar_base.is_some(),
            accounts_base.is_some(),
            occurrences_base.is_some(),
        ]
        .into_iter()
        .filter(|v| *v)
        .count()
            > 1
        {
            return Err(core_error(
                "Choose one isolated Google verification scenario",
            ));
        }
        if store.profile_kind()? != ProfileKind::Live
            || !store.handle("accounts", None, json!({}))?["accounts"]
                .as_array()
                .is_some_and(Vec::is_empty)
            || !profile.join(MARKER).is_file()
        {
            return Err(core_error("Verification needs a marked empty live profile"));
        }
        let config = profile.join("verification-google-client.json");
        let bytes = serde_json::to_vec(
            &json!({"installed":{"client_id":CLIENT,"client_secret":"synthetic-desktop-secret"}}),
        )?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&config)?.write_all(&bytes)?;
        let accounts = Arc::new(vec![Account::new("personal")?, Account::new("workspace")?]);
        let scopes: BTreeSet<String> = if calendar_enabled {
            vec![
                "openid",
                "email",
                "https://www.googleapis.com/auth/calendar.events",
                "https://www.googleapis.com/auth/calendar.calendarlist.readonly",
                "https://www.googleapis.com/auth/calendar.events.freebusy",
            ]
        } else {
            vec![
                "openid",
                "email",
                if remote_drafts_enabled {
                    "https://www.googleapis.com/auth/gmail.readonly"
                } else {
                    "https://www.googleapis.com/auth/gmail.modify"
                },
            ]
        }
        .into_iter()
        .map(str::to_owned)
        .collect();
        for account in accounts.iter().filter(|_| accounts_base.is_none()) {
            let id = store.bind_google_account_with_client(
                &account.subject,
                &account.email,
                &scopes,
                None,
                CLIENT,
                config
                    .to_str()
                    .ok_or_else(|| core_error("Verification profile must be Unicode"))?,
            )?;
            if id != account.id {
                return Err(core_error("Verification account identity changed"));
            }
        }
        let started_unix_ms = unix_ms();
        let base_unix_secs = calendar_base
            .or(accounts_base)
            .or(occurrences_base)
            .unwrap_or((started_unix_ms / 1000 / 60) * 60);
        let evidence = Arc::new(Mutex::new(Evidence::default()));
        let hold_raw = Arc::new(AtomicBool::new(false));
        let remote_version_two = Arc::new(AtomicBool::new(false));
        let action_version_two = Arc::new(AtomicBool::new(false));
        let mut http = FixtureHttp::new(
            accounts.clone(),
            base_unix_secs,
            evidence.clone(),
            hold_raw.clone(),
            send_enabled || draft_actions_enabled,
        );
        if remote_drafts_enabled {
            http.enable_remote_drafts(remote_version_two.clone());
        }
        if draft_actions_enabled {
            http.enable_draft_actions(action_version_two.clone());
        }
        if occurrences_base.is_some() {
            http.enable_occurrences();
        } else if calendar_enabled {
            http.enable_calendar();
        }
        if accounts_base.is_some() {
            http.enable_accounts();
        }
        let http = Arc::new(http);
        let accounts_fixture = accounts_base.map(|_| {
            Arc::new(accounts::AccountsFixture::new(
                profile.into(),
                accounts.clone(),
                http.clone(),
                evidence.clone(),
            ))
        });
        let vault = Arc::new(FixtureVault::new(
            accounts.clone(),
            started_unix_ms / 1000 + 3600,
            evidence.clone(),
        ));
        let fixture = Self {
            profile: profile.into(),
            accounts,
            base_unix_secs,
            started_unix_ms,
            evidence,
            http,
            vault,
            hold_raw,
            send_enabled: send_enabled || draft_actions_enabled,
            remote_drafts_enabled,
            remote_version_two,
            draft_actions_enabled,
            action_version_two,
            calendar_enabled,
            occurrences_enabled: occurrences_base.is_some(),
            accounts_fixture,
        };
        fixture.persist_blocking(false)?;
        Ok(fixture)
    }
    pub(crate) fn auth(
        &self,
        core: CoreBridge,
        cancel: CancellationToken,
    ) -> Arc<google_auth::Runtime> {
        if let Some(accounts) = &self.accounts_fixture {
            google_auth::Runtime::for_verification_accounts(
                core,
                accounts.clone(),
                accounts.clone(),
                cancel,
                accounts.clone(),
                self.profile.join("verification-google-client.json"),
            )
        } else {
            google_auth::Runtime::for_verification(
                core,
                self.http.clone(),
                self.vault.clone(),
                cancel,
            )
        }
    }
    pub(crate) fn install_writer_trace(&self) -> WriterTrace {
        WRITER_TRACE.with(|trace| {
            assert!(
                trace.borrow().is_none(),
                "one verification trace per Store writer"
            );
            *trace.borrow_mut() = Some(self.evidence.clone());
        });
        WriterTrace
    }
    pub(crate) fn start_reporter(&self, cancel: CancellationToken) -> tokio::task::JoinHandle<()> {
        let fixture = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _=cancel.cancelled()=>break,
                    _=interval.tick()=>{
                        let copy = fixture.clone();
                        if !matches!(tokio::task::spawn_blocking(move || {
                            match std::fs::symlink_metadata(copy.profile.join(HOLD_RAW)) {
                                Ok(_) => copy.hold_raw.store(true, Ordering::Release),
                                Err(error) if error.kind() == io::ErrorKind::NotFound => {},
                                Err(_) => copy.evidence.lock().unwrap().error("hold_marker_unreadable"),
                            }
                            if copy.remote_drafts_enabled {
                                match std::fs::symlink_metadata(copy.profile.join(DRAFT_VERSION_TWO)) {
                                    Ok(_) => copy.remote_version_two.store(true, Ordering::Release),
                                    Err(error) if error.kind() == io::ErrorKind::NotFound => {},
                                    Err(_) => copy.evidence.lock().unwrap().error("draft_version_marker_unreadable"),
                                }
                            }
                            if copy.draft_actions_enabled {
                                match std::fs::symlink_metadata(copy.profile.join(ACTION_VERSION_TWO)) {
                                    Ok(_) => copy.action_version_two.store(true, Ordering::Release),
                                    Err(error) if error.kind() == io::ErrorKind::NotFound => {},
                                    Err(_) => copy.evidence.lock().unwrap().error("draft_action_marker_unreadable"),
                                }
                            }
                            copy.persist_blocking(false)
                        }).await, Ok(Ok(()))) {
                            fixture.evidence.lock().unwrap().error("progress_report_failed");
                            cancel.cancel();
                            break;
                        }
                    }
                }
            }
        })
    }
    fn report(&self, final_report: bool) -> Value {
        let accounts = self.accounts_fixture.as_ref().map(|a| a.report());
        let evidence = self.evidence.lock().unwrap();
        json!({"version":1,"scenario":if self.occurrences_enabled{"google-calendar-occurrences"}else{"google-navigation"},"engine_pid":std::process::id(),"profile_kind":"live","fixture_revision":0,
            "transport":"in_memory_only","native_vault":false,"status":if !evidence.errors.is_empty(){"failed"}else if final_report{"complete"}else if evidence.counters.http_started>0{"running"}else{"ready"},
            "base_unix_secs":self.base_unix_secs,"started_unix_ms":self.started_unix_ms,"updated_unix_ms":unix_ms(),"accounts":self.accounts.iter().map(Account::value).collect::<Vec<_>>(),
            "config":{"recent_messages_per_account":if accounts.is_some(){2}else if self.calendar_enabled {0}else{RECENT_MESSAGES},"recent_threads_per_account":if accounts.is_some(){1}else if self.calendar_enabled {0}else{RECENT_MESSAGES/2},"backfill_messages_per_account":if accounts.is_some()||self.calendar_enabled {0}else{BACKFILL_MESSAGES},"raw_delay_ms":if self.calendar_enabled {0}else{RAW_DELAY_MS},"max_publications":MAX_PUBLICATIONS},
            "account_fixture":accounts,
            "occurrence_fixture":{"enabled":self.occurrences_enabled,"masters_per_account":calendar_occurrences::MASTERS,"instances_per_account":calendar_occurrences::INSTANCES,"initial_sources_per_account":calendar_occurrences::INITIAL_SOURCES,"page_size":calendar_occurrences::PAGE_SIZE},
            "counters":evidence.counters.value(),"publication_count":evidence.publications.len(),"published_item_count":evidence.item_count,
            "publications":if final_report{evidence.publications.clone()}else{vec![]},"errors":evidence.errors,
            "draft_fixture":{"max_drafts_per_account":16,"max_mime_bytes":2097152,"max_publications":128,"publications":evidence.draft_publications},
            "send_fixture":{"enabled":self.send_enabled,"max_sends_per_account":16,"max_mime_bytes":2097152,"publications":evidence.send_publications},
            "remote_drafts_fixture":{"enabled":self.remote_drafts_enabled,"containers_per_account":remote_drafts::COUNT,"page_size":50,"version_two":self.remote_version_two.load(Ordering::Acquire),"reads":evidence.remote_draft_reads},
            "draft_actions_fixture":{"enabled":self.draft_actions_enabled,"containers_per_account":draft_actions::COUNT,"version_two":self.action_version_two.load(Ordering::Acquire),"reads":evidence.draft_action_reads,"deletions":evidence.draft_deletions},
            "calendar_fixture":{"enabled":self.calendar_enabled||self.accounts_fixture.is_some(),"calendars_per_account":1,"events_per_account":if self.accounts_fixture.is_some(){1}else if self.occurrences_enabled{calendar_occurrences::INITIAL_SOURCES}else{calendar_invitations::COUNT},"base_unix_secs":self.base_unix_secs,"reads":evidence.calendar_reads,"publications":evidence.calendar_publications},
            "stop_reason":if final_report{Some("owning_sessions_or_engine_shutdown")}else{None}})
    }
    fn persist_blocking(&self, final_report: bool) -> io::Result<()> {
        let temporary = self.profile.join("verification-google-report.tmp");
        std::fs::write(&temporary, serde_json::to_vec(&self.report(final_report))?)?;
        std::fs::rename(temporary, self.profile.join(REPORT))
    }
    pub(crate) async fn finish(&self, writer_finished: bool) -> io::Result<()> {
        if let Some(accounts) = &self.accounts_fixture {
            accounts.check_drained();
        }
        {
            let mut evidence = self.evidence.lock().unwrap();
            if !writer_finished {
                evidence.error("core_writer_shutdown_failed");
            }
            if evidence.counters.raw_active != 0 || evidence.counters.raw_held_active != 0 {
                evidence.error("provider_request_survived_shutdown");
            }
        }
        let copy = self.clone();
        tokio::task::spawn_blocking(move || copy.persist_blocking(true))
            .await
            .map_err(io::Error::other)??;
        if self.evidence.lock().unwrap().errors.is_empty() {
            Ok(())
        } else {
            Err(io::Error::other(
                "Google verification failed; inspect its sanitized report",
            ))
        }
    }
}
fn core_error(message: &str) -> fm_core::CoreError {
    fm_core::CoreError {
        code: "verification_refused".into(),
        message: message.into(),
        details: Value::Null,
    }
}

#[cfg(test)]
mod accounts_tests;
#[cfg(test)]
mod calendar_tests;
#[cfg(test)]
mod occurrence_tests;
#[cfg(test)]
mod tests;
