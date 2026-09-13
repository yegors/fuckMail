use clap::Parser;
use fm_ipc::{
    error, response, AnyStream, Discovery, Request, HANDSHAKE_TIMEOUT, IO_TIMEOUT, MAX_CLIENTS,
    PROTOCOL_VERSION,
};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::{mpsc, oneshot, OwnedSemaphorePermit, Semaphore},
};
mod core_tasks;
mod google_auth;
mod google_sync;
mod verification_fixture;

#[derive(Parser)]
#[command(
    version,
    about = "fuckMail local engine — runs only for explicit GUI/CLI sessions"
)]
struct Args {
    #[arg(long)]
    profile: Option<PathBuf>,
    /// Create or open an explicitly selected live profile; never seed demo data.
    #[arg(long, requires = "profile", conflicts_with = "verification_fixture")]
    live_profile: bool,
    #[arg(long, hide = true, value_enum)]
    verification_fixture: Option<verification_fixture::Mode>,
}

enum Work {
    Request(ClientWork),
    VerificationBatch(verification_fixture::Batch),
    Provider(core_tasks::CoreCall),
}

struct ClientWork {
    request: Request,
    reply: oneshot::Sender<Value>,
    cancelled: Arc<AtomicBool>,
    _permit: OwnedSemaphorePermit,
}

const WORK_LIMIT: usize = 32;
const FOREGROUND_BURST_LIMIT: usize = 4;

#[derive(Default)]
struct PendingWork {
    requests: VecDeque<ClientWork>,
    batches: VecDeque<verification_fixture::Batch>,
    provider: VecDeque<core_tasks::CoreCall>,
    foreground_burst: usize,
}
impl PendingWork {
    fn push(&mut self, work: Work) {
        match work {
            Work::Request(work) => self.requests.push_back(work),
            Work::VerificationBatch(batch) => self.batches.push_back(batch),
            Work::Provider(call) => self.provider.push_back(call),
        }
    }

    fn request(&mut self, background_active: bool) -> Option<ClientWork> {
        if background_active && self.foreground_burst >= FOREGROUND_BURST_LIMIT {
            return None;
        }
        let work = self.requests.pop_front()?;
        self.foreground_burst = self.foreground_burst.saturating_add(1);
        Some(work)
    }
}

struct RequestOwner(Arc<AtomicBool>);
impl Drop for RequestOwner {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

struct SessionCancellation(fm_google::transport::CancellationToken);
impl Drop for SessionCancellation {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[cfg(unix)]
type Listener = tokio::net::UnixListener;
#[cfg(windows)]
struct Listener {
    next: tokio::net::windows::named_pipe::NamedPipeServer,
    endpoint: String,
}

#[cfg(unix)]
fn listen(endpoint: &str) -> io::Result<Listener> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
    if let Ok(md) = std::fs::symlink_metadata(endpoint) {
        // Only invoked after acquiring the store's exclusive profile lock.
        if !md.file_type().is_socket() || md.uid() != unsafe { libc::geteuid() } {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "refusing to replace a non-owned socket",
            ));
        }
        std::fs::remove_file(endpoint)?;
    }
    let listener = Listener::bind(endpoint)?;
    std::fs::set_permissions(endpoint, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

#[cfg(windows)]
fn pipe(
    endpoint: &str,
    first: bool,
) -> io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
    let security = fm_ipc::windows::Security::user_only(false)?;
    let mut attrs = security.attributes();
    let mut options = tokio::net::windows::named_pipe::ServerOptions::new();
    options
        .first_pipe_instance(first)
        .reject_remote_clients(true)
        .max_instances(MAX_CLIENTS + 1);
    unsafe {
        options.create_with_security_attributes_raw(endpoint, std::ptr::from_mut(&mut attrs).cast())
    }
}
#[cfg(windows)]
fn listen(endpoint: &str) -> io::Result<Listener> {
    Ok(Listener {
        next: pipe(endpoint, true)?,
        endpoint: endpoint.to_string(),
    })
}

#[cfg(unix)]
async fn accept(listener: &mut Listener) -> io::Result<AnyStream> {
    let (stream, _) = listener.accept().await?;
    if stream.peer_cred()?.uid() != unsafe { libc::geteuid() } {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "peer is not the profile user",
        ));
    }
    Ok(Box::new(stream))
}
#[cfg(windows)]
async fn accept(listener: &mut Listener) -> io::Result<AnyStream> {
    listener.next.connect().await?;
    let next = pipe(&listener.endpoint, false)?;
    Ok(Box::new(std::mem::replace(&mut listener.next, next)))
}

fn authenticated(expected: &str, provided: Option<&str>) -> bool {
    let Some(provided) = provided else {
        return false;
    };
    if expected.len() != provided.len() {
        return false;
    }
    expected
        .bytes()
        .zip(provided.bytes())
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

#[derive(Clone)]
struct GoogleServices {
    auth: Arc<google_auth::Runtime>,
    sync: Arc<google_sync::Runtime>,
}

async fn session(
    stream: AnyStream,
    token: Arc<String>,
    queue: mpsc::Sender<Work>,
    hello_metadata: Arc<Value>,
    foreground_slots: Arc<Semaphore>,
    providers: Option<GoogleServices>,
    engine_cancel: fm_google::transport::CancellationToken,
) -> io::Result<()> {
    let mut stream = BufReader::new(stream);
    let hello = tokio::time::timeout(HANDSHAKE_TIMEOUT, fm_ipc::read_frame(&mut stream))
        .await??
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "no handshake"))?;
    let request: Request = serde_json::from_slice(&hello)?;
    let bad = if request.version != PROTOCOL_VERSION {
        Some(error(
            &request.id,
            "protocol_mismatch",
            "supported protocol is 1",
        ))
    } else if request.method != "hello" || !authenticated(&token, request.params["token"].as_str())
    {
        Some(error(
            &request.id,
            "unauthorized",
            "private engine handshake required",
        ))
    } else {
        None
    };
    if let Some(reply) = bad {
        fm_ipc::write_frame(stream.get_mut(), &reply).await?;
        return Ok(());
    }
    fm_ipc::write_frame(
        stream.get_mut(),
        &response(&request.id, (*hello_metadata).clone()),
    )
    .await?;
    let owner_id = fm_ipc::random_token();
    let owner_cancel = engine_cancel.child_token();
    let _owner_cancel = SessionCancellation(owner_cancel.clone());
    loop {
        // An idle authenticated GUI/watch is an explicit owner, not a stale job.
        if stream.fill_buf().await?.is_empty() {
            return Ok(());
        }
        let frame =
            match tokio::time::timeout(IO_TIMEOUT, fm_ipc::read_frame(&mut stream)).await?? {
                Some(f) => f,
                None => return Ok(()),
            };
        let request: Request = match serde_json::from_slice(&frame) {
            Ok(r) => r,
            Err(_) => {
                fm_ipc::write_frame(
                    stream.get_mut(),
                    &error("", "invalid_request", "invalid request envelope"),
                )
                .await?;
                continue;
            }
        };
        if request.id.is_empty()
            || request.id.len() > 128
            || request.method.len() > 64
            || !request.params.is_object()
        {
            fm_ipc::write_frame(
                stream.get_mut(),
                &error(
                    &request.id,
                    "invalid_request",
                    "request needs a short id/method and object params",
                ),
            )
            .await?;
            continue;
        }
        if request.version != PROTOCOL_VERSION {
            fm_ipc::write_frame(
                stream.get_mut(),
                &error(&request.id, "protocol_mismatch", "supported protocol is 1"),
            )
            .await?;
            continue;
        }
        if google_auth::Runtime::handles(&request.method) {
            let reply = if let Some(provider) = &providers {
                let result = tokio::select! {
                    result = provider.auth.handle(&owner_id, owner_cancel.clone(), &request.method, request.account.as_deref(), request.params) => result,
                    ready = stream.fill_buf() => {
                        if ready?.is_empty() { return Ok(()); }
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "only one request may be in flight per connection"));
                    }
                };
                match result {
                    Ok(result) => response(&request.id, result),
                    Err(e) => error(&request.id, &e.code, &e.message),
                }
            } else {
                error(
                    &request.id,
                    "capability_unavailable",
                    "Connect Google accounts in a separate live profile",
                )
            };
            tokio::time::timeout(IO_TIMEOUT, fm_ipc::write_frame(stream.get_mut(), &reply))
                .await??;
            continue;
        }
        if google_sync::Runtime::handles(&request.method) {
            let result = if let Some(provider) = &providers {
                tokio::select! {
                    result=provider.sync.handle(&owner_id,owner_cancel.clone(),&request.method,request.account.as_deref(),request.params)=>result,
                    ready=stream.fill_buf()=> {if ready?.is_empty(){return Ok(());}return Err(io::Error::new(io::ErrorKind::InvalidData,"only one request may be in flight per connection"));}
                }
            } else {
                Err(fm_core::CoreError {
                    code: "capability_unavailable".into(),
                    message: "Google sync requires a live profile".into(),
                    details: Value::Null,
                })
            };
            let reply = match result {
                Ok(result) => response(&request.id, result),
                Err(e) => error(&request.id, &e.code, &e.message),
            };
            tokio::time::timeout(IO_TIMEOUT, fm_ipc::write_frame(stream.get_mut(), &reply))
                .await??;
            continue;
        }
        let id = request.id.clone();
        let Ok(permit) = foreground_slots.clone().try_acquire_owned() else {
            fm_ipc::write_frame(
                stream.get_mut(),
                &error(&id, "busy", "engine work queue is full; retry later"),
            )
            .await?;
            continue;
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        let request_owner = RequestOwner(Arc::new(AtomicBool::new(false)));
        // The permit follows the job through the FIFO, priority queue and
        // execution, including jobs whose clients disconnect while queued.
        if queue
            .try_send(Work::Request(ClientWork {
                request,
                reply: reply_tx,
                cancelled: Arc::clone(&request_owner.0),
                _permit: permit,
            }))
            .is_err()
        {
            fm_ipc::write_frame(
                stream.get_mut(),
                &error(&id, "busy", "engine work queue is full; retry later"),
            )
            .await?;
            continue;
        }
        // A disconnected owner cancels its queued work and active SQL reads. Once a
        // mutation has begun its short transaction, finish its durable outcome;
        // the same request_id can recover that outcome after reconnecting.
        let reply = tokio::select! {
            reply = reply_rx => reply.map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "engine worker stopped"))?,
            ready = stream.fill_buf() => {
                if ready?.is_empty() { return Ok(()); }
                return Err(io::Error::new(io::ErrorKind::InvalidData, "only one request may be in flight per connection"));
            }
        };
        let reply = if serde_json::to_vec(&reply)?.len() + 1 > fm_ipc::MAX_FRAME {
            error(
                &id,
                "response_too_large",
                "use a smaller page or an attachment file path",
            )
        } else {
            reply
        };
        tokio::time::timeout(IO_TIMEOUT, fm_ipc::write_frame(stream.get_mut(), &reply)).await??;
    }
}

struct Owner(Arc<AtomicUsize>);
impl Drop for Owner {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn cleanup(profile: &Path, endpoint: &str, pid: u32) {
    if fm_ipc::read_discovery(profile).is_ok_and(|d| d.pid == pid) {
        let _ = std::fs::remove_file(profile.join("engine.json"));
    }
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(endpoint);
    }
    #[cfg(windows)]
    {
        let _ = endpoint;
    }
}

#[tokio::main]
async fn main() {
    if let Err(e) = run(Args::parse()).await {
        eprintln!("fm-engine: {e}");
        let code = if e
            .downcast_ref::<fm_core::CoreError>()
            .is_some_and(|e| e.code == "profile_kind_mismatch")
        {
            fm_ipc::ENGINE_PROFILE_KIND_MISMATCH
        } else {
            1
        };
        std::process::exit(code);
    }
}

async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    if args.verification_fixture.is_some() && args.profile.is_none() {
        return Err("verification fixtures require an explicit disposable profile".into());
    }
    let profile = fm_ipc::private_profile(&args.profile.unwrap_or(fm_ipc::default_profile()?))?;
    if let Some(mode) = args.verification_fixture {
        verification_fixture::preflight(mode, &profile)?;
    }
    let mut store = match if args.live_profile {
        fm_core::Store::open_live(&profile)
    } else {
        fm_core::Store::open(&profile)
    } {
        Ok(s) => s,
        Err(e) => {
            // A launch race is benign only if the existing engine responds.
            if let Ok(d) = fm_ipc::read_discovery(&profile) {
                if fm_ipc::connect(&d).await.is_ok() {
                    return Ok(());
                }
            }
            return Err(e.into());
        }
    };
    let mut fixture = args
        .verification_fixture
        .map(|mode| verification_fixture::Fixture::prepare(mode, &profile, &mut store))
        .transpose()?;
    let provider_namespace = (store.profile_kind()? == fm_core::live_profile::ProfileKind::Live)
        .then(|| store.vault_namespace())
        .transpose()?;
    let hello_metadata = Arc::new(handshake_metadata(&mut store)?);
    let endpoint = fm_ipc::endpoint(&profile)?;
    let mut listener = listen(&endpoint)?;
    let token = Arc::new(fm_ipc::random_token());
    let pid = std::process::id();
    fm_ipc::write_discovery(
        &profile,
        &Discovery {
            protocol: 1,
            pid,
            endpoint: endpoint.clone(),
            token: token.to_string(),
        },
    )?;
    let (queue, mut receive) = mpsc::channel::<Work>(WORK_LIMIT);
    let search_fixture = args.verification_fixture == Some(verification_fixture::Mode::Search);
    let foreground_slots = Arc::new(Semaphore::new(
        if search_fixture || provider_namespace.is_some() {
            WORK_LIMIT - verification_fixture::BACKGROUND_LIMIT
        } else {
            WORK_LIMIT
        },
    ));
    let background_slots = Arc::new(Semaphore::new(verification_fixture::BACKGROUND_LIMIT));
    let engine_cancel = fm_google::transport::CancellationToken::new();
    let provider = provider_namespace
        .map(|namespace| {
            google_auth::Runtime::new(
                core_tasks::CoreBridge::new(queue.clone(), background_slots.clone()),
                namespace,
                engine_cancel.clone(),
            )
        })
        .transpose()?;
    let sync = provider.as_ref().map(|auth| {
        google_sync::Runtime::new(
            core_tasks::CoreBridge::new(queue.clone(), background_slots.clone()),
            auth.clone(),
            engine_cancel.clone(),
        )
    });
    let owners = Arc::new(AtomicUsize::new(0));
    let authenticated_work = Arc::new(AtomicBool::new(false));
    let schedule = Arc::new(verification_fixture::Schedule::default());
    let worker_owners = owners.clone();
    let worker_authenticated = authenticated_work.clone();
    let worker_schedule = schedule.clone();
    let worker = std::thread::spawn(move || {
        let mut store = store;
        let mut pending = PendingWork::default();
        let mut ingestion = None;
        let mut closed = false;
        loop {
            // Search mode has 16 foreground and 16 background permits. They
            // remain attached to active and locally queued work, so draining
            // this FIFO does not create another 32-job backlog (global max 32).
            loop {
                match receive.try_recv() {
                    Ok(work) => pending.push(work),
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        closed = true;
                        break;
                    }
                }
            }
            if worker_owners.load(Ordering::SeqCst) == 0 {
                if let Some(fixture) = &mut fixture {
                    if let Some(ingestion) = ingestion.take() {
                        fixture.abandon_ingestion(&ingestion, "final_owner");
                    }
                    for batch in pending.batches.drain(..) {
                        fixture.skip_without_owner(batch);
                    }
                }
            }
            let background_active =
                ingestion.is_some() || !pending.batches.is_empty() || !pending.provider.is_empty();
            let Some(work) = pending.request(background_active) else {
                if background_active {
                    pending.foreground_burst = 0;
                    if let Some(call) = pending.provider.pop_front() {
                        call.run(&mut store);
                        continue;
                    }
                    if let Some(fixture) = &mut fixture {
                        if ingestion.is_none() {
                            ingestion = pending
                                .batches
                                .pop_front()
                                .and_then(|batch| fixture.begin_ingestion(batch));
                        }
                        if let Some(active) = &mut ingestion {
                            if fixture.step_ingestion(&mut store, active) {
                                ingestion = None;
                            }
                        }
                    }
                    continue;
                }
                if closed {
                    break;
                }
                match receive.blocking_recv() {
                    Some(work) => pending.push(work),
                    None => closed = true,
                }
                continue;
            };
            if work.reply.is_closed() {
                continue;
            }
            worker_authenticated.store(true, Ordering::Release);
            let inject = fixture
                .as_ref()
                .is_some_and(|fixture| fixture.should_inject(&work.request));
            let id = work.request.id;
            let result = store.handle_cancellable(
                &work.request.method,
                work.request.account.as_deref(),
                work.request.params,
                work.cancelled,
            );
            if inject && result.is_ok() {
                if let Some(fixture) = &mut fixture {
                    fixture.inject_snapshot(&mut store);
                }
            }
            let reply = match result {
                Ok(result) => response(&id, result),
                Err(e) => error(&id, &e.code, &e.message),
            };
            let _ = work.reply.send(reply);
        }
        if let Some(fixture) = &mut fixture {
            fixture.finish(&worker_schedule);
        }
        // Dropping SQLite closes/checkpoints without bypassing the core's writer lock.
        drop(store);
    });
    let slots = Arc::new(Semaphore::new(MAX_CLIENTS));
    let start = tokio::time::Instant::now();
    let mut ever_connected = false;
    let mut idle_since = None;
    let mut tick = tokio::time::interval(Duration::from_millis(verification_fixture::INTERVAL_MS));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut next_batch = 0;
    let mut clients = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            incoming=accept(&mut listener)=>match incoming {
                Ok(stream)=> {
                    let Ok(permit)=slots.clone().try_acquire_owned() else {drop(stream);continue;};
                    owners.fetch_add(1,Ordering::SeqCst);ever_connected=true;idle_since=None;
                    let owner=Owner(owners.clone());let token=token.clone();let queue=queue.clone();let hello_metadata=hello_metadata.clone();let foreground_slots=foreground_slots.clone();let providers=provider.as_ref().zip(sync.as_ref()).map(|(auth,sync)|GoogleServices{auth:auth.clone(),sync:sync.clone()});let stop=engine_cancel.clone();
                    clients.spawn(async move {
                        let _owner=owner;let _permit=permit;
                        tokio::select! {
                            _=stop.cancelled()=>{},
                            _=session(stream,token,queue,hello_metadata,foreground_slots,providers,stop.clone())=>{},
                        }
                    });
                },
                Err(e)=> {eprintln!("fm-engine: local accept failed: {}",e.kind());tokio::time::sleep(Duration::from_millis(50)).await;}
            },
            _=tick.tick()=> {
                if args.verification_fixture == Some(verification_fixture::Mode::Search)
                    && next_batch < verification_fixture::MAX_BATCHES
                    && authenticated_work.load(Ordering::Acquire)
                    && owners.load(Ordering::SeqCst) > 0 {
                    schedule.attempt();
                    if let Ok(permit) = background_slots.clone().try_acquire_owned() {
                        let (batch,queued_unix_ms)=schedule.batch(next_batch,permit);
                        match queue.try_send(Work::VerificationBatch(batch)) {
                            Ok(())=>{schedule.queued(queued_unix_ms);next_batch+=1;},
                            Err(mpsc::error::TrySendError::Full(_))=>{schedule.queue_full_retries.fetch_add(1,Ordering::Relaxed);},
                            Err(mpsc::error::TrySendError::Closed(_))=>break,
                        }
                    } else {
                        schedule.queue_full_retries.fetch_add(1,Ordering::Relaxed);
                    }
                }
                if owners.load(Ordering::SeqCst)==0 {
                    if ever_connected {
                        let since=idle_since.get_or_insert_with(tokio::time::Instant::now);
                        if since.elapsed()>=Duration::from_millis(350) {break;}
                    } else if start.elapsed()>=Duration::from_secs(10) {break;}
                }
            },
            _=tokio::signal::ctrl_c()=>break,
            _=clients.join_next(), if !clients.is_empty()=>{},
        }
    }
    // Stop accepting before removing discovery. Outstanding client jobs finish
    // durable core writes; normal final-client shutdown already has no clients.
    drop(listener);
    cleanup(&profile, &endpoint, pid);
    engine_cancel.cancel();
    while clients.join_next().await.is_some() {}
    if let Some(sync) = &sync {
        sync.shutdown().await;
    }
    drop(sync);
    if let Some(provider) = &provider {
        provider.shutdown().await;
    }
    drop(provider);
    drop(queue);
    let _ = worker.join();
    Ok(())
}

fn handshake_metadata(store: &mut fm_core::Store) -> fm_core::Result<Value> {
    let status = store.handle("status", None, json!({}))?;
    Ok(
        json!({"protocol":PROTOCOL_VERSION,"schema_version":status["schema_version"],
        "fixture_revision":status["fixture_revision"],"capabilities":status["capabilities"],
        "mode":status["profile_kind"],"max_frame_bytes":fm_ipc::MAX_FRAME,"max_page_size":100,
        "methods":["accounts","status","conversations","conversation","flag","draft_get",
            "draft_save","draft_capture","calendar","attachment","categories","set_category",
            "view_get","view_save","preferences_get","preferences_save","calendar_sources",
            "calendar_combined","account_update","draft_discard","capture_history","calendar_write",
            "calendar_write_ack","calendar_operations","drafts","draft_create","sender_summary",
            "google_auth_begin","google_auth_status","google_auth_cancel","google_disconnect",
            "google_sync_start","google_sync_stop","google_sync_release","google_sync_status","google_sync_busy"]}),
    )
}

#[cfg(test)]
mod session_tests {
    use super::*;

    #[test]
    fn foreground_overtakes_background_with_bounded_fair_work_and_owned_permits() {
        let foreground = Arc::new(Semaphore::new(16));
        let background = Arc::new(Semaphore::new(verification_fixture::BACKGROUND_LIMIT));
        let schedule = verification_fixture::Schedule::default();
        let (queue, mut receive) = mpsc::channel(WORK_LIMIT);
        let mut replies = Vec::new();
        for sequence in 0..16 {
            let (batch, _) =
                schedule.batch(sequence, background.clone().try_acquire_owned().unwrap());
            queue
                .try_send(Work::VerificationBatch(batch))
                .unwrap_or_else(|_| panic!("background queue unexpectedly full"));
        }
        for sequence in 0..16 {
            let (reply, rx) = oneshot::channel();
            replies.push(rx);
            queue.try_send(Work::Request(ClientWork {
                request:serde_json::from_value(json!({"version":1,"id":sequence.to_string(),"method":"status","params":{}})).unwrap(),
                reply,cancelled:Arc::new(AtomicBool::new(false)),_permit:foreground.clone().try_acquire_owned().unwrap(),
            })).unwrap_or_else(|_|panic!("foreground capacity was consumed by background jobs"));
        }
        let mut pending = PendingWork::default();
        while let Ok(work) = receive.try_recv() {
            pending.push(work);
        }
        // FIFO is now empty, but no additional job can enter: the same permits
        // are still held in the local queues, including a selected active batch.
        let active = pending.batches.pop_front().unwrap();
        assert!(foreground.clone().try_acquire_owned().is_err());
        assert!(background.clone().try_acquire_owned().is_err());
        for sequence in 0..FOREGROUND_BURST_LIMIT {
            let request = pending.request(true).unwrap();
            assert_eq!(request.request.id, sequence.to_string());
        }
        assert!(pending.request(true).is_none()); // the background step gets a turn
        pending.foreground_burst = 0;
        assert_eq!(pending.request(true).unwrap().request.id, "4");
        assert_eq!(foreground.available_permits(), 5);
        assert_eq!(background.available_permits(), 0);
        drop(pending);
        assert_eq!(foreground.available_permits(), 16);
        assert_eq!(background.available_permits(), 15);
        drop(active);
        assert_eq!(background.available_permits(), 16);
        drop(replies);
    }

    #[tokio::test]
    async fn hello_preserves_migrated_fixture_revision_and_advertises_additive_methods() {
        for expected_fixture in [1, 3] {
            let temp = tempfile::tempdir().unwrap();
            if expected_fixture == 1 {
                let db = rusqlite::Connection::open(temp.path().join("store.sqlite3")).unwrap();
                db.execute_batch(include_str!("../../fm-core/src/schema.sql"))
                    .unwrap();
                db.execute_batch("INSERT INTO meta VALUES('fixture_revision',1); INSERT INTO accounts VALUES('original','Original identity','original@example.test','synthetic');").unwrap();
            }
            let mut store = fm_core::Store::open(temp.path()).unwrap();
            let metadata = Arc::new(handshake_metadata(&mut store).unwrap());
            let (client, server) = tokio::io::duplex(8192);
            let mut client = BufReader::new(client);
            let (queue, _receive) = mpsc::channel(1);
            let task = tokio::spawn(session(
                Box::new(server),
                Arc::new("fixture-token".into()),
                queue,
                metadata,
                Arc::new(Semaphore::new(WORK_LIMIT)),
                None,
                fm_google::transport::CancellationToken::new(),
            ));
            fm_ipc::write_frame(client.get_mut(), &json!({"version":1,"id":"hello","method":"hello","params":{"token":"fixture-token"}})).await.unwrap();
            let hello: Value =
                serde_json::from_slice(&fm_ipc::read_frame(&mut client).await.unwrap().unwrap())
                    .unwrap();
            assert_eq!(hello["ok"], true);
            assert_eq!(hello["result"]["protocol"], 1);
            assert_eq!(hello["result"]["fixture_revision"], expected_fixture);
            assert_eq!(hello["result"]["schema_version"], fm_core::SCHEMA_VERSION);
            assert_eq!(hello["result"]["capabilities"]["phase1"], true);
            assert_eq!(hello["result"]["capabilities"]["send"], false);
            let methods = hello["result"]["methods"].as_array().unwrap();
            for method in [
                "conversations",
                "flag",
                "preferences_save",
                "draft_discard",
                "capture_history",
                "calendar_combined",
                "calendar_write",
                "calendar_operations",
            ] {
                assert!(methods.contains(&json!(method)), "missing {method}");
            }
            assert!(!methods.contains(&json!("verification_fixture")));
            drop(client);
            tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        }
    }

    #[tokio::test]
    async fn disconnect_marks_only_that_queued_request_cancelled() {
        let (client, server) = tokio::io::duplex(8192);
        let mut client = BufReader::new(client);
        let (queue, mut receive) = mpsc::channel(1);
        let foreground_slots = Arc::new(Semaphore::new(1));
        let task = tokio::spawn(session(
            Box::new(server),
            Arc::new("fixture-token".into()),
            queue,
            Arc::new(json!({"protocol":1,"fixture_revision":2})),
            foreground_slots.clone(),
            None,
            fm_google::transport::CancellationToken::new(),
        ));
        fm_ipc::write_frame(
            client.get_mut(),
            &json!({"version":1,"id":"hello","method":"hello","params":{"token":"fixture-token"}}),
        )
        .await
        .unwrap();
        let hello: Value =
            serde_json::from_slice(&fm_ipc::read_frame(&mut client).await.unwrap().unwrap())
                .unwrap();
        assert_eq!(hello["ok"], true);
        fm_ipc::write_frame(client.get_mut(), &json!({"version":1,"id":"queued","method":"draft_save","account":"windscribe","params":{"body":"unfinished"}})).await.unwrap();
        let work = tokio::time::timeout(Duration::from_secs(2), receive.recv())
            .await
            .unwrap()
            .unwrap();
        let Work::Request(work) = work else {
            panic!("expected client work")
        };
        assert!(!work.cancelled.load(Ordering::Acquire));
        drop(client);
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(work.cancelled.load(Ordering::Acquire));
        assert!(work.reply.is_closed());
        assert_eq!(foreground_slots.available_permits(), 0);
        drop(work);
        assert_eq!(foreground_slots.available_permits(), 1);
    }
}
