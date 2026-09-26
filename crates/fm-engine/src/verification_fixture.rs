//! Explicit native verification scenarios, never a public command or provider.
//! Every source is generated here and goes through Google's existing Core bridge.
//!
//! Selected by the hidden `--verification-fixture` engine flag with an
//! explicit, disposable profile. `snapshot` seeds a fresh profile with a
//! two-message conversation and publishes one generated arrival when the
//! client marks that conversation read. `search` streams up to 3,000 batches
//! of 25 generated messages into a prepared large profile while clients keep
//! working, to measure foreground latency under background ingestion. Both
//! write a `verification-ingestion.json` report into the profile.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use clap::ValueEnum;
use fm_core::{google_store::GooglePageStore, Store};
use fm_google::{
    sync::{
        advance, decode_raw_message, prepare_full_page, prepare_history_page, GmailTransport,
        HistoryPage, ListPage, MessageRef, PagePlan, PageStore, Phase, RawMessage,
    },
    AccountId, GoogleError,
};
use fm_ipc::Request;
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::OwnedSemaphorePermit;

pub const MAX_BATCHES: usize = 3000;
pub const INTERVAL_MS: u64 = 100;
const BATCH_SIZE: usize = 25;
const ACCOUNT: &str = "windscribe";
const SUBJECT: &str = "Snapshot arrival verification";
pub const BACKGROUND_LIMIT: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    Snapshot,
    Search,
}

pub struct Batch {
    pub sequence: usize,
    queued: Instant,
    queued_unix_ms: u64,
    queue_full_retries: usize,
    _permit: Option<OwnedSemaphorePermit>,
}
impl Batch {
    pub fn new(sequence: usize, queue_full_retries: usize) -> Self {
        Self {
            sequence,
            queued: Instant::now(),
            queued_unix_ms: unix_ms(),
            queue_full_retries,
            _permit: None,
        }
    }
}

#[derive(Default)]
pub struct Schedule {
    pub first_scheduled_unix_ms: AtomicU64,
    pub last_scheduled_unix_ms: AtomicU64,
    pub last_attempt_unix_ms: AtomicU64,
    pub queue_full_retries: AtomicUsize,
    pub scheduled_batches: AtomicUsize,
}
impl Schedule {
    pub fn attempt(&self) {
        self.last_attempt_unix_ms
            .store(unix_ms(), Ordering::Relaxed);
    }
    pub fn queued(&self, queued_unix_ms: u64) {
        let _ = self.first_scheduled_unix_ms.compare_exchange(
            0,
            queued_unix_ms,
            Ordering::Relaxed,
            Ordering::Relaxed,
        );
        self.last_scheduled_unix_ms
            .store(queued_unix_ms, Ordering::Relaxed);
        self.scheduled_batches.fetch_add(1, Ordering::Relaxed);
    }
    pub fn batch(&self, sequence: usize, permit: OwnedSemaphorePermit) -> (Batch, u64) {
        let mut batch = Batch::new(sequence, self.queue_full_retries.load(Ordering::Relaxed));
        batch._permit = Some(permit);
        let queued = batch.queued_unix_ms;
        (batch, queued)
    }
}

pub struct Fixture {
    mode: Mode,
    started: Instant,
    report_path: PathBuf,
    report: Value,
}

/// One logical 25-source page. Staging is durable but invisible to mail queries;
/// only its final existing GooglePageStore transaction publishes the whole page.
pub struct Ingestion {
    batch: Batch,
    started: Instant,
    started_unix_ms: u64,
    sources: Sources,
    plan: Option<PagePlan>,
    remaining: VecDeque<String>,
    staged: usize,
    setup_ms: f64,
    staging_ms: f64,
    commit_ms: f64,
    commit_started_unix_ms: u64,
    commit_finished_unix_ms: u64,
    max_step_ms: f64,
}

impl Ingestion {
    fn step(&mut self, store: &mut Store) -> io::Result<bool> {
        let started = Instant::now();
        let completed = if self.plan.is_none() {
            self.prepare_page(store)?;
            self.setup_ms += started.elapsed().as_secs_f64() * 1000.0;
            false
        } else if let Some(id) = self.remaining.pop_front() {
            let plan = self.plan.as_ref().unwrap();
            let raw = self
                .sources
                .get_raw(&plan.expected.account, &id)
                .map_err(google_error)?;
            let content = decode_raw_message(raw, &id).map_err(google_error)?;
            GooglePageStore::new(store)
                .stage_message(&plan.expected, content)
                .map_err(google_error)?;
            self.staged += 1;
            self.staging_ms += started.elapsed().as_secs_f64() * 1000.0;
            false
        } else {
            // This is the only visible publication. Nothing fallible follows
            // the commit before the caller records the full logical batch.
            self.commit_started_unix_ms = unix_ms();
            GooglePageStore::new(store)
                .commit_page(self.plan.as_ref().unwrap())
                .map_err(google_error)?;
            self.commit_finished_unix_ms = unix_ms();
            self.commit_ms += started.elapsed().as_secs_f64() * 1000.0;
            true
        };
        self.max_step_ms = self
            .max_step_ms
            .max(started.elapsed().as_secs_f64() * 1000.0);
        Ok(completed)
    }

    fn prepare_page(&mut self, store: &mut Store) -> io::Result<()> {
        let mut checkpoint = store
            .google_begin_full(ACCOUNT, "100")
            .map_err(io::Error::other)?;
        // The first full page is already a durable, visible batch. Finish its
        // empty catch-up at the next setup boundary, so a checkpoint failure
        // can never misreport an already published page as abandoned staging.
        if let Phase::CatchUp {
            start_history_id,
            page_token,
        } = &checkpoint.phase
        {
            let mut empty = Sources {
                messages: Vec::new(),
                history_id: next_history(&checkpoint)?,
            };
            let page = empty
                .history(&checkpoint.account, start_history_id, page_token.as_deref())
                .map_err(google_error)?;
            let plan = prepare_history_page(&checkpoint, page).map_err(google_error)?;
            GooglePageStore::new(store)
                .commit_page(&plan)
                .map_err(google_error)?;
            checkpoint = plan.next;
        }
        let history = next_history(&checkpoint)?;
        for message in &mut self.sources.messages {
            message.history_id = history.clone();
        }
        self.sources.history_id = history;
        if checkpoint.phase == Phase::Idle {
            let plan = checkpoint.begin_delta().map_err(google_error)?;
            GooglePageStore::new(store)
                .commit_page(&plan)
                .map_err(google_error)?;
            checkpoint = plan.next;
        }
        let plan = match &checkpoint.phase {
            Phase::Full { page_token, .. } => {
                let page = self
                    .sources
                    .list(&checkpoint.account, page_token.as_deref(), true)
                    .map_err(google_error)?;
                prepare_full_page(&checkpoint, page).map_err(google_error)?
            }
            Phase::Delta {
                start_history_id,
                page_token,
            } => {
                let page = self
                    .sources
                    .history(&checkpoint.account, start_history_id, page_token.as_deref())
                    .map_err(google_error)?;
                prepare_history_page(&checkpoint, page).map_err(google_error)?
            }
            _ => return Err(io::Error::other("unexpected verification checkpoint phase")),
        };
        if plan.fetch_message_ids.len() != BATCH_SIZE {
            return Err(io::Error::other(
                "verification page must fetch exactly 25 sources",
            ));
        }
        self.remaining = plan.fetch_message_ids.iter().cloned().collect();
        self.plan = Some(plan);
        Ok(())
    }
}

fn next_history(checkpoint: &fm_google::sync::Checkpoint) -> io::Result<String> {
    checkpoint
        .committed_history_id
        .as_deref()
        .unwrap_or("100")
        .parse::<u64>()
        .map_err(io::Error::other)?
        .checked_add(1)
        .map(|value| value.to_string())
        .ok_or_else(|| io::Error::other("verification history exhausted"))
}

pub fn preflight(mode: Mode, profile: &Path) -> io::Result<()> {
    let exists = profile.join("store.sqlite3").exists();
    if mode == Mode::Snapshot && exists {
        return Err(io::Error::other(
            "snapshot verification requires a fresh explicit profile",
        ));
    }
    if mode == Mode::Search && !exists {
        return Err(io::Error::other(
            "search verification requires a retained generated 100k profile",
        ));
    }
    if profile.join("verification-ingestion.json").exists() {
        return Err(io::Error::other(
            "verification report already exists; use a fresh disposable profile copy",
        ));
    }
    Ok(())
}

impl Fixture {
    pub fn prepare(mode: Mode, profile: &Path, store: &mut Store) -> io::Result<Self> {
        if mode == Mode::Search {
            validate_search_seed(store)?;
        }
        let mut fixture = Self::initialized(mode, profile);
        if mode == Mode::Snapshot {
            import_sources(store, vec![snapshot_source(0), snapshot_source(1)])?;
            let row = snapshot_row(store)?;
            if row["message_ids"].as_array().map(Vec::len) != Some(2) {
                return Err(io::Error::other(
                    "snapshot seed did not publish exactly two members",
                ));
            }
            fixture.report["conversation_id"] = row["id"].clone();
            fixture.report["original_message_ids"] = row["message_ids"].clone();
            fixture.report["arrival_message_id"] = Value::Null;
            fixture.report["seeded_messages"] = json!(2);
        }
        fixture.persist()?;
        Ok(fixture)
    }

    fn initialized(mode: Mode, profile: &Path) -> Self {
        Self {
            mode,
            started: Instant::now(),
            report_path: profile.join("verification-ingestion.json"),
            report: json!({"version":1,"mode":if mode==Mode::Snapshot {"snapshot"} else {"search"},
                "status":"ready","account":ACCOUNT,"engine_pid":std::process::id(),
                "started_unix_ms":unix_ms(),"started_offset_ms":0,
                "first_scheduled_unix_ms":null,"last_scheduled_unix_ms":null,
                "first_commit_unix_ms":null,"last_commit_unix_ms":null,"finished_unix_ms":null,
                "max_batches":if mode==Mode::Snapshot {1} else {MAX_BATCHES},
                "interval_ms":INTERVAL_MS,"messages_per_batch":if mode==Mode::Snapshot {1} else {BATCH_SIZE},
                "seeded_messages":0,"trigger_count":0,"committed_batches":0,"committed_messages":0,
                "skipped_without_owner":0,"queue_full_retries":0,"batches":[],"errors":[],
                "scheduler_version":2,"scheduler":{"max_resident_work":crate::WORK_LIMIT,
                    "foreground_limit":crate::WORK_LIMIT-BACKGROUND_LIMIT,
                    "background_limit":BACKGROUND_LIMIT,
                    "sources_per_step":1,"foreground_burst_limit":4},
                "abandoned_staged_messages":0,"abandoned_batches":[]}),
        }
    }

    pub fn should_inject(&self, request: &Request) -> bool {
        self.mode == Mode::Snapshot
            && self.report["trigger_count"] == 0
            && self.report["errors"].as_array().is_some_and(Vec::is_empty)
            && request.method == "flag"
            && request.account.as_deref() == Some(ACCOUNT)
            && request.params["id"] == self.report["conversation_id"]
            && request.params["message_ids"] == self.report["original_message_ids"]
            && request.params["read"] == true
            && request.params["starred"] == false
    }

    pub fn inject_snapshot(&mut self, store: &mut Store) {
        self.report["trigger_count"] = json!(1);
        let batch = Batch::new(0, 0);
        let result = self
            .commit_batch(store, batch, vec![snapshot_source(2)])
            .and_then(|()| {
                let row = snapshot_row(store)?;
                let originals = self.report["original_message_ids"].as_array().unwrap();
                let arrivals: Vec<_> = row["message_ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|id| !originals.contains(id))
                    .cloned()
                    .collect();
                if arrivals.len() != 1 {
                    return Err(io::Error::other(
                        "snapshot injection did not publish exactly one new member",
                    ));
                }
                self.report["arrival_message_id"] = arrivals[0].clone();
                self.report["status"] = json!("complete");
                self.persist()
            });
        if let Err(error) = result {
            self.fail(error);
        }
    }

    pub fn skip_without_owner(&mut self, _batch: Batch) {
        self.report["skipped_without_owner"] =
            json!(self.report["skipped_without_owner"].as_u64().unwrap() + 1);
    }

    pub fn begin_ingestion(&mut self, batch: Batch) -> Option<Ingestion> {
        if self.mode != Mode::Search || !self.report["errors"].as_array().unwrap().is_empty() {
            return None;
        }
        if batch.sequence >= MAX_BATCHES {
            self.fail(io::Error::other("verification batch bound exceeded"));
            return None;
        }
        let messages = (0..BATCH_SIZE)
            .map(|member| search_source(batch.sequence, member))
            .collect();
        Some(Ingestion {
            batch,
            started: Instant::now(),
            started_unix_ms: unix_ms(),
            sources: Sources {
                messages,
                history_id: String::new(),
            },
            plan: None,
            remaining: VecDeque::new(),
            staged: 0,
            setup_ms: 0.0,
            staging_ms: 0.0,
            commit_ms: 0.0,
            max_step_ms: 0.0,
            commit_started_unix_ms: 0,
            commit_finished_unix_ms: 0,
        })
    }

    /// Returns true when the page completed or failed. The caller yields back
    /// to its foreground queue after every invocation, never inside a commit.
    pub fn step_ingestion(&mut self, store: &mut Store, ingestion: &mut Ingestion) -> bool {
        match ingestion.step(store) {
            Ok(false) => false,
            Ok(true) => {
                let timing = json!({"setup_ms":ingestion.setup_ms,"staging_ms":ingestion.staging_ms,
                    "commit_ms":ingestion.commit_ms,"stage_steps":ingestion.staged,"max_step_ms":ingestion.max_step_ms,
                    "commit_started_unix_ms":ingestion.commit_started_unix_ms,"commit_finished_unix_ms":ingestion.commit_finished_unix_ms});
                if let Err(error) = self.record_batch(
                    &ingestion.batch,
                    ingestion.started,
                    ingestion.started_unix_ms,
                    BATCH_SIZE,
                    timing,
                ) {
                    self.fail(error);
                }
                true
            }
            Err(error) => {
                self.abandon_ingestion(ingestion, "ingestion_error");
                self.fail(error);
                true
            }
        }
    }

    pub fn abandon_ingestion(&mut self, ingestion: &Ingestion, reason: &str) {
        self.report["abandoned_staged_messages"] = json!(
            self.report["abandoned_staged_messages"].as_u64().unwrap() + ingestion.staged as u64
        );
        self.report["abandoned_batches"]
            .as_array_mut()
            .unwrap()
            .push(json!({
            "sequence":ingestion.batch.sequence,"queued_unix_ms":ingestion.batch.queued_unix_ms,
            "started_unix_ms":ingestion.started_unix_ms,"abandoned_unix_ms":unix_ms(),
            "staged_messages":ingestion.staged,"committed_messages":0,"reason":reason}));
    }

    fn commit_batch(
        &mut self,
        store: &mut Store,
        batch: Batch,
        messages: Vec<RawMessage>,
    ) -> io::Result<()> {
        let started = Instant::now();
        let started_unix_ms = unix_ms();
        let count = messages.len();
        import_sources(store, messages)?;
        self.record_batch(&batch, started, started_unix_ms, count, json!({}))
    }

    fn record_batch(
        &mut self,
        batch: &Batch,
        started: Instant,
        started_unix_ms: u64,
        count: usize,
        timing: Value,
    ) -> io::Result<()> {
        let finished = Instant::now();
        let finished_unix_ms = unix_ms();
        if self.report["first_scheduled_unix_ms"].is_null() {
            self.report["first_scheduled_unix_ms"] = json!(batch.queued_unix_ms);
            self.report["first_commit_unix_ms"] = json!(finished_unix_ms);
        }
        self.report["last_scheduled_unix_ms"] = json!(batch.queued_unix_ms);
        self.report["last_commit_unix_ms"] = json!(finished_unix_ms);
        self.report["committed_batches"] =
            json!(self.report["committed_batches"].as_u64().unwrap() + 1);
        self.report["committed_messages"] =
            json!(self.report["committed_messages"].as_u64().unwrap() + count as u64);
        self.report["queue_full_retries"] = json!(batch.queue_full_retries);
        self.report["batches"].as_array_mut().unwrap().push(json!({
            "sequence":batch.sequence,"queued_unix_ms":batch.queued_unix_ms,
            "started_unix_ms":started_unix_ms,"finished_unix_ms":finished_unix_ms,
            "queued_offset_ms":batch.queued.duration_since(self.started).as_secs_f64()*1000.0,
            "started_offset_ms":started.duration_since(self.started).as_secs_f64()*1000.0,
            "finished_offset_ms":finished.duration_since(self.started).as_secs_f64()*1000.0,
            "queue_wait_ms":started.duration_since(batch.queued).as_secs_f64()*1000.0,
            "ingestion_ms":finished.duration_since(started).as_secs_f64()*1000.0,
            "committed_messages":count,"queue_full_retries":batch.queue_full_retries,"cooperative":timing
        }));
        self.report["status"] = json!("running");
        // Outside snapshot mode, persist every 25th batch rather than rewriting
        // the growing timing list each time. Final-owner shutdown writes all rows.
        if self.mode == Mode::Snapshot || batch.sequence % 25 == 0 {
            self.persist()?;
        }
        Ok(())
    }

    pub fn finish(&mut self, schedule: &Schedule) {
        if self.mode == Mode::Search {
            self.report["first_scheduled_unix_ms"] =
                json!(schedule.first_scheduled_unix_ms.load(Ordering::Relaxed));
            self.report["last_scheduled_unix_ms"] =
                json!(schedule.last_scheduled_unix_ms.load(Ordering::Relaxed));
            self.report["producer_last_attempt_unix_ms"] =
                json!(schedule.last_attempt_unix_ms.load(Ordering::Relaxed));
            self.report["queue_full_retries"] =
                json!(schedule.queue_full_retries.load(Ordering::Relaxed));
            self.report["scheduled_batches"] =
                json!(schedule.scheduled_batches.load(Ordering::Relaxed));
        }
        self.report["finished_unix_ms"] = json!(unix_ms());
        self.report["finished_offset_ms"] = json!(self.started.elapsed().as_secs_f64() * 1000.0);
        if self.report["errors"].as_array().unwrap().is_empty() {
            if self.mode == Mode::Search {
                self.report["status"] = json!("complete");
            }
            self.report["stop_reason"] = json!("final_owner_or_engine_shutdown");
        }
        if let Err(error) = self.persist() {
            eprintln!("fm-engine: verification report: {error}");
        }
    }

    fn fail(&mut self, error: io::Error) {
        eprintln!("fm-engine: verification ingestion: {error}");
        self.report["status"] = json!("failed");
        if self.report["errors"].as_array().unwrap().len() < 4 {
            self.report["errors"]
                .as_array_mut()
                .unwrap()
                .push(json!(error.to_string()));
        }
        if let Err(error) = self.persist() {
            eprintln!("fm-engine: verification report: {error}");
        }
    }

    fn persist(&self) -> io::Result<()> {
        let temporary = self.report_path.with_extension("json.tmp");
        std::fs::write(&temporary, serde_json::to_vec(&self.report)?)?;
        std::fs::rename(temporary, &self.report_path)
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn validate_search_seed(store: &mut Store) -> io::Result<()> {
    let accounts = store
        .handle("accounts", None, json!({}))
        .map_err(io::Error::other)?;
    if ![json!(2), json!(3)].contains(&accounts["fixture_revision"])
        || accounts["accounts"].as_array().map(Vec::len) != Some(3)
        || accounts["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| !a["provider"].as_str().unwrap_or("").contains("(synthetic)"))
        || store
            .google_checkpoint(ACCOUNT)
            .map_err(io::Error::other)?
            .is_some()
    {
        return Err(io::Error::other(
            "search verification requires an unused generated fixture-2 or fixture-3 copy",
        ));
    }
    for account in ["windscribe", "personal"] {
        let result = store
            .handle(
                "conversations",
                Some(account),
                json!({
                    "query":"Search benchmark","field":"body","folder":"all","limit":1
                }),
            )
            .map_err(io::Error::other)?;
        if result["filtered_conversation_count"] != 50_000
            || !result["conversations"][0]["id"]
                .as_str()
                .unwrap_or("")
                .starts_with(&format!("benchmark-{account}-thread-"))
        {
            return Err(io::Error::other(
                "search verification requires exactly 100k generated benchmark records",
            ));
        }
    }
    Ok(())
}

fn snapshot_row(store: &mut Store) -> io::Result<Value> {
    let result = store
        .handle(
            "conversations",
            Some(ACCOUNT),
            json!({
                "query":SUBJECT,"field":"subject","folder":"all","limit":2
            }),
        )
        .map_err(io::Error::other)?;
    if result["filtered_conversation_count"] != 1 {
        return Err(io::Error::other(
            "snapshot verification thread is missing or ambiguous",
        ));
    }
    store
        .handle(
            "conversation",
            Some(ACCOUNT),
            json!({"id":result["conversations"][0]["id"]}),
        )
        .map_err(io::Error::other)
}

fn snapshot_source(member: usize) -> RawMessage {
    let id = format!("verification-snapshot-{member}");
    let mut message = source(
        &id,
        "verification-snapshot-thread",
        SUBJECT,
        "Verification author <verification@fixture.example>",
        &format!("Sat, 01 Aug 2026 00:0{member}:00 +0000"),
        &format!("Snapshot verification source member {member}."),
    );
    if member < 2 {
        message.label_ids.push("STARRED".into());
    }
    message
}

fn search_source(batch: usize, member: usize) -> RawMessage {
    let id = format!("verification-search-{batch:04}-{member:02}");
    source(
        &id,
        &format!("verification-thread-{id}"),
        "Concurrent source verification",
        "Alex O’Hara <alex.o'hara+build@windscribe.example>",
        "Tue, 01 Sep 2026 12:00:00 +0000",
        &format!("Concurrent source deployment verification {id}"),
    )
}

fn source(
    id: &str,
    thread: &str,
    subject: &str,
    sender: &str,
    date: &str,
    body: &str,
) -> RawMessage {
    let mime = format!("From: {sender}\r\nTo: You <you@windscribe.example>\r\nSubject: {subject}\r\nDate: {date}\r\nMessage-ID: <{id}@fixture.example>\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{body}\r\n");
    RawMessage {
        id: id.into(),
        thread_id: thread.into(),
        label_ids: vec!["INBOX".into(), "UNREAD".into()],
        history_id: "101".into(),
        raw: URL_SAFE_NO_PAD.encode(mime),
    }
}

struct Sources {
    messages: Vec<RawMessage>,
    history_id: String,
}
impl GmailTransport for Sources {
    fn list(&mut self, _: &AccountId, _: Option<&str>, _: bool) -> Result<ListPage, GoogleError> {
        Ok(ListPage {
            messages: self
                .messages
                .iter()
                .map(|m| MessageRef {
                    id: m.id.clone(),
                    thread_id: m.thread_id.clone(),
                })
                .collect(),
            next_page_token: None,
            result_size_estimate: Some(self.messages.len() as u64),
        })
    }
    fn history(
        &mut self,
        _: &AccountId,
        _: &str,
        _: Option<&str>,
    ) -> Result<HistoryPage, GoogleError> {
        serde_json::from_value(json!({"historyId":self.history_id,"history":[{"id":self.history_id,
            "messagesAdded":self.messages.iter().map(|m|json!({"message":{"id":m.id,"threadId":m.thread_id}})).collect::<Vec<_>>() }]}))
            .map_err(|_|GoogleError::InvalidResponse("invalid fixed verification history"))
    }
    fn get_raw(&mut self, _: &AccountId, id: &str) -> Result<RawMessage, GoogleError> {
        self.messages
            .iter()
            .find(|m| m.id == id)
            .cloned()
            .ok_or(GoogleError::NotFound)
    }
}

fn import_sources(store: &mut Store, mut messages: Vec<RawMessage>) -> io::Result<()> {
    let mut checkpoint = store
        .google_begin_full(ACCOUNT, "100")
        .map_err(io::Error::other)?;
    let history = checkpoint
        .committed_history_id
        .as_deref()
        .unwrap_or("100")
        .parse::<u64>()
        .map_err(io::Error::other)?
        .checked_add(1)
        .ok_or_else(|| io::Error::other("verification history exhausted"))?
        .to_string();
    for message in &mut messages {
        message.history_id = history.clone();
    }
    if checkpoint.phase == Phase::Idle {
        let plan = checkpoint.begin_delta().map_err(google_error)?;
        GooglePageStore::new(store)
            .commit_page(&plan)
            .map_err(google_error)?;
        checkpoint = plan.next;
    }
    let mut sources = Sources {
        messages,
        history_id: history,
    };
    advance(
        &mut checkpoint,
        &mut sources,
        &mut GooglePageStore::new(store),
    )
    .map_err(google_error)?;
    if matches!(checkpoint.phase, Phase::CatchUp { .. }) {
        sources.messages.clear();
        advance(
            &mut checkpoint,
            &mut sources,
            &mut GooglePageStore::new(store),
        )
        .map_err(google_error)?;
    }
    Ok(())
}
fn google_error(error: GoogleError) -> io::Error {
    io::Error::other(format!("verification source: {error:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search_count(store: &mut Store) -> u64 {
        store
            .handle(
                "conversations",
                Some(ACCOUNT),
                json!({"query":"Concurrent source verification",
            "field":"subject","folder":"all","limit":1}),
            )
            .unwrap()["filtered_conversation_count"]
            .as_u64()
            .unwrap()
    }

    #[test]
    fn cooperative_staging_yields_invisible_sources_and_publishes_whole_pages() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path()).unwrap();
        // Unit fixture construction avoids generating 100k records; production
        // prepare still requires the unchanged inventory guard tested below.
        let mut fixture = Fixture::initialized(Mode::Search, temp.path());
        for sequence in 0..2 {
            let mut ingestion = fixture.begin_ingestion(Batch::new(sequence, 0)).unwrap();
            assert!(!fixture.step_ingestion(&mut store, &mut ingestion)); // checkpoint setup
            assert_eq!(ingestion.staged, 0);
            for staged in 1..=BATCH_SIZE {
                assert!(!fixture.step_ingestion(&mut store, &mut ingestion));
                assert_eq!(ingestion.staged, staged);
                // An actual foreground query can run after each source. No
                // partial page can enter mail counts or the FTS projection.
                assert_eq!(search_count(&mut store), (sequence * BATCH_SIZE) as u64);
            }
            assert!(fixture.step_ingestion(&mut store, &mut ingestion));
            assert_eq!(
                search_count(&mut store),
                ((sequence + 1) * BATCH_SIZE) as u64
            );
            let report = &fixture.report["batches"][sequence];
            assert_eq!(report["committed_messages"], 25);
            assert_eq!(report["cooperative"]["stage_steps"], 25);
            assert!(
                report["cooperative"]["commit_started_unix_ms"]
                    .as_u64()
                    .unwrap()
                    >= report["started_unix_ms"].as_u64().unwrap()
            );
            assert!(
                report["cooperative"]["commit_finished_unix_ms"]
                    .as_u64()
                    .unwrap()
                    <= report["finished_unix_ms"].as_u64().unwrap()
            );
        }
        assert_eq!(fixture.report["errors"], json!([]));
        assert_eq!(fixture.report["committed_messages"], 50);
        assert_eq!(
            store.google_checkpoint(ACCOUNT).unwrap().unwrap().phase,
            Phase::Idle
        );
    }

    #[test]
    fn abandoned_staging_is_reported_and_never_enters_visible_mail() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path()).unwrap();
        let mut fixture = Fixture::initialized(Mode::Search, temp.path());
        let mut ingestion = fixture.begin_ingestion(Batch::new(0, 0)).unwrap();
        for _ in 0..4 {
            assert!(!fixture.step_ingestion(&mut store, &mut ingestion));
        }
        assert_eq!(ingestion.staged, 3);
        fixture.abandon_ingestion(&ingestion, "final_owner");
        drop(ingestion);
        fixture.skip_without_owner(Batch::new(1, 0));
        fixture.finish(&Schedule::default());
        assert_eq!(search_count(&mut store), 0);
        drop(store);
        let report: Value =
            serde_json::from_slice(&std::fs::read(&fixture.report_path).unwrap()).unwrap();
        assert_eq!(report["status"], "complete");
        assert_eq!(report["batches"], json!([]));
        assert_eq!(report["committed_messages"], 0);
        assert_eq!(report["abandoned_staged_messages"], 3);
        assert_eq!(report["abandoned_batches"][0]["staged_messages"], 3);
        assert_eq!(report["abandoned_batches"][0]["committed_messages"], 0);
        assert_eq!(report["skipped_without_owner"], 1);
        let db = rusqlite::Connection::open_with_flags(
            temp.path().join("store.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let staged:usize = db.query_row("SELECT COUNT(*) FROM google_staged_messages WHERE remote_id LIKE 'verification-search-%'",[],|row|row.get(0)).unwrap();
        let published: usize = db
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE remote_id LIKE 'verification-search-%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(staged, 3);
        assert_eq!(published, 0);
    }

    #[test]
    fn snapshot_uses_source_ingestion_and_preserves_the_original_receipt() {
        let temp = tempfile::tempdir().unwrap();
        preflight(Mode::Snapshot, temp.path()).unwrap();
        let mut store = Store::open(temp.path()).unwrap();
        let mut fixture = Fixture::prepare(Mode::Snapshot, temp.path(), &mut store).unwrap();
        let row = snapshot_row(&mut store).unwrap();
        let params = json!({"id":row["id"],"message_ids":row["message_ids"],"expected_revision":row["revision"],
            "request_id":"native-done","read":true,"starred":false});
        let request: Request = serde_json::from_value(
            json!({"version":1,"id":"done","method":"flag","account":ACCOUNT,"params":params}),
        )
        .unwrap();
        assert!(fixture.should_inject(&request));
        let receipt = store.handle("flag", Some(ACCOUNT), params.clone()).unwrap();
        fixture.inject_snapshot(&mut store);
        assert!(!fixture.should_inject(&request));
        assert_eq!(fixture.report["errors"], json!([]));
        assert_eq!(fixture.report["trigger_count"], 1);
        assert_eq!(fixture.report["committed_messages"], 1);
        assert_eq!(receipt["messages"].as_array().unwrap().len(), 2);
        assert_eq!(
            store.handle("flag", Some(ACCOUNT), params).unwrap(),
            receipt
        );
        let arrived = store
            .handle("conversation", Some(ACCOUNT), json!({"id":row["id"]}))
            .unwrap();
        assert_eq!(arrived["messages"].as_array().unwrap().len(), 3);
        let newest = arrived["messages"].as_array().unwrap().last().unwrap();
        assert_eq!(newest["id"], fixture.report["arrival_message_id"]);
        assert_eq!(newest["unread"], true);
        assert_eq!(newest["starred"], false);
        assert_eq!(newest["source_available"], true);
        assert_eq!(newest["text"], "Snapshot verification source member 2.\r\n");
        fixture.finish(&Schedule::default());
        let report: Value =
            serde_json::from_slice(&std::fs::read(&fixture.report_path).unwrap()).unwrap();
        assert_eq!(report["status"], "complete");
        assert!(preflight(Mode::Snapshot, temp.path()).is_err());
    }

    #[test]
    fn normal_fixture_is_refused_for_search_without_starting_an_import() {
        let temp = tempfile::tempdir().unwrap();
        assert!(preflight(Mode::Search, temp.path()).is_err());
        let mut store = Store::open(temp.path()).unwrap();
        assert!(Fixture::prepare(Mode::Search, temp.path(), &mut store).is_err());
        assert!(store.google_checkpoint(ACCOUNT).unwrap().is_none());
        assert!(!temp.path().join("verification-ingestion.json").exists());
    }

    #[test]
    fn search_sources_match_combined_filters_and_publish_distinct_members() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path()).unwrap();
        import_sources(
            &mut store,
            (0..BATCH_SIZE).map(|i| search_source(0, i)).collect(),
        )
        .unwrap();
        import_sources(
            &mut store,
            (0..BATCH_SIZE).map(|i| search_source(1, i)).collect(),
        )
        .unwrap();
        let result=store.handle("conversations",Some(ACCOUNT),json!({"query":"deployment","field":"body",
            "sender":"alex","status":"review","has_attachment":false,"after":"2026-09-01","before":"2026-09-12",
            "folder":"all","grouped":true,"limit":100})).unwrap();
        assert_eq!(result["filtered_conversation_count"], 50);
        assert_eq!(result["conversations"].as_array().unwrap().len(), 50);
        for row in result["conversations"].as_array().unwrap() {
            assert_eq!(row["message_count"], 1);
            assert_eq!(row["latest_at"], "2026-09-01T12:00:00Z");
            let detail = store
                .handle("conversation", Some(ACCOUNT), json!({"id":row["id"]}))
                .unwrap();
            assert_eq!(detail["messages"][0]["source_available"], true);
        }
    }
}
