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

mod transport;
use transport::{FixtureHttp, FixtureVault};

pub(super) const RECENT_MESSAGES: usize = 60;
pub(super) const BACKFILL_MESSAGES: usize = 4096;
pub(super) const RAW_DELAY_MS: u64 = 16;
const MAX_PUBLICATIONS: usize = 4096;
const REPORT: &str = "verification-google-report.json";
const HOLD_RAW: &str = "verification-google-hold-raw";
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
    pub(super) vault_loads: u64,
    pub(super) vault_stores: u64,
}
impl Counters {
    fn value(&self) -> Value {
        json!({"http_started":self.http_started,"http_completed":self.http_completed,"http_cancelled":self.http_cancelled,"http_rejected":self.http_rejected,
            "raw_started":self.raw_started,"raw_completed":self.raw_completed,"raw_cancelled":self.raw_cancelled,"raw_active":self.raw_active,"raw_held_active":self.raw_held_active,
            "modify_completed":self.modify_completed,"draft_created":self.draft_created,"draft_updated":self.draft_updated,"draft_read":self.draft_read,
            "vault_loads":self.vault_loads,"vault_stores":self.vault_stores})
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
}
impl Fixture {
    pub(crate) fn prepare(profile: &Path, store: &mut Store) -> fm_core::Result<Self> {
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
        let scopes: BTreeSet<String> = [
            "openid",
            "email",
            "https://www.googleapis.com/auth/gmail.modify",
        ]
        .map(str::to_owned)
        .into();
        for account in accounts.iter() {
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
        let base_unix_secs = (started_unix_ms / 1000 / 60) * 60;
        let evidence = Arc::new(Mutex::new(Evidence::default()));
        let hold_raw = Arc::new(AtomicBool::new(false));
        let http = Arc::new(FixtureHttp::new(
            accounts.clone(),
            base_unix_secs,
            evidence.clone(),
            hold_raw.clone(),
        ));
        let vault = Arc::new(FixtureVault::new(
            accounts.clone(),
            base_unix_secs + 3600,
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
        };
        fixture.persist_blocking(false)?;
        Ok(fixture)
    }
    pub(crate) fn auth(
        &self,
        core: CoreBridge,
        cancel: CancellationToken,
    ) -> Arc<google_auth::Runtime> {
        google_auth::Runtime::for_verification(core, self.http.clone(), self.vault.clone(), cancel)
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
        let evidence = self.evidence.lock().unwrap();
        json!({"version":1,"scenario":"google-navigation","engine_pid":std::process::id(),"profile_kind":"live","fixture_revision":0,
            "transport":"in_memory_only","native_vault":false,"status":if !evidence.errors.is_empty(){"failed"}else if final_report{"complete"}else if evidence.counters.http_started>0{"running"}else{"ready"},
            "base_unix_secs":self.base_unix_secs,"started_unix_ms":self.started_unix_ms,"updated_unix_ms":unix_ms(),"accounts":self.accounts.iter().map(Account::value).collect::<Vec<_>>(),
            "config":{"recent_messages_per_account":RECENT_MESSAGES,"recent_threads_per_account":RECENT_MESSAGES/2,"backfill_messages_per_account":BACKFILL_MESSAGES,"raw_delay_ms":RAW_DELAY_MS,"max_publications":MAX_PUBLICATIONS},
            "counters":evidence.counters.value(),"publication_count":evidence.publications.len(),"published_item_count":evidence.item_count,
            "publications":if final_report{evidence.publications.clone()}else{vec![]},"errors":evidence.errors,
            "draft_fixture":{"max_drafts_per_account":16,"max_mime_bytes":2097152,"max_publications":128,"publications":evidence.draft_publications},
            "stop_reason":if final_report{Some("owning_sessions_or_engine_shutdown")}else{None}})
    }
    fn persist_blocking(&self, final_report: bool) -> io::Result<()> {
        let temporary = self.profile.join("verification-google-report.tmp");
        std::fs::write(&temporary, serde_json::to_vec(&self.report(final_report))?)?;
        std::fs::rename(temporary, self.profile.join(REPORT))
    }
    pub(crate) async fn finish(&self, writer_finished: bool) -> io::Result<()> {
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
mod tests;
