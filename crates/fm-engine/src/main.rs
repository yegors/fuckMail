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
mod diagnostics;
mod google_auth;
mod google_sync;
mod local_content;
mod local_model;
mod ollama;
mod openai_local;
mod remote_content;
mod verification_fixture;
mod verification_google;

#[derive(Parser)]
#[command(
    version,
    about = "fuckMail local engine — runs only for explicit GUI/CLI sessions"
)]
struct Args {
    #[arg(long)]
    profile: Option<PathBuf>,
    /// Create or open an explicitly selected live profile; never seed demo data.
    #[arg(long, requires = "profile", conflicts_with_all = ["verification_fixture", "demo_profile"])]
    live_profile: bool,
    /// Explicit synthetic examples; ordinary new profiles are always empty/live.
    #[arg(long)]
    demo_profile: bool,
    #[arg(long, hide = true, value_enum)]
    verification_fixture: Option<verification_fixture::Mode>,
    /// In-memory provider verification, compiled only in an isolated developer build.
    #[cfg(feature = "verification-google")]
    #[arg(
        long,
        hide = true,
        requires = "live_profile",
        conflicts_with = "verification_fixture"
    )]
    verification_google: bool,
    /// Permit bounded generated submissions only in the isolated provider fixture.
    #[cfg(feature = "verification-google")]
    #[arg(long, hide = true, requires = "verification_google")]
    verification_google_send: bool,
    /// Seed read-only generated remote Drafts only in the isolated provider fixture.
    #[cfg(feature = "verification-google")]
    #[arg(
        long,
        hide = true,
        requires = "verification_google",
        conflicts_with = "verification_google_send"
    )]
    verification_google_drafts: bool,
    #[cfg(feature = "verification-google")]
    #[arg(long, hide = true, requires = "verification_google", conflicts_with_all = ["verification_google_send", "verification_google_drafts"])]
    verification_google_draft_actions: bool,
    #[cfg(feature = "verification-google")]
    #[arg(long, hide = true, requires_all = ["verification_google", "verification_google_calendar_base"], conflicts_with_all = ["verification_google_send", "verification_google_drafts", "verification_google_draft_actions"])]
    verification_google_calendar: bool,
    #[cfg(feature = "verification-google")]
    #[arg(long, hide = true, requires = "verification_google_calendar", value_parser = calendar_fixture_base)]
    verification_google_calendar_base: Option<u64>,
    #[cfg(feature = "verification-google")]
    #[arg(long, hide = true, requires_all = ["verification_google", "verification_google_accounts_base"], conflicts_with_all = ["verification_google_send", "verification_google_drafts", "verification_google_draft_actions", "verification_google_calendar"])]
    verification_google_accounts: bool,
    #[cfg(feature = "verification-google")]
    #[arg(long, hide = true, requires = "verification_google_accounts", value_parser = calendar_fixture_base)]
    verification_google_accounts_base: Option<u64>,
    #[cfg(feature = "verification-google")]
    #[arg(long, hide = true, requires_all = ["verification_google", "verification_google_occurrences_base"], conflicts_with_all = ["verification_google_send", "verification_google_drafts", "verification_google_draft_actions", "verification_google_calendar", "verification_google_accounts"])]
    verification_google_occurrences: bool,
    #[cfg(feature = "verification-google")]
    #[arg(long, hide = true, requires = "verification_google_occurrences", value_parser = calendar_fixture_base)]
    verification_google_occurrences_base: Option<u64>,
}

#[cfg(feature = "verification-google")]
fn calendar_fixture_base(value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .ok()
        .filter(|n| (946_684_800..4_102_444_800).contains(n) && n % 60 == 0)
        .ok_or_else(|| "Use a UTC minute-aligned timestamp between 2000 and 2100".into())
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

async fn await_sync_reply<F, R>(
    work: F,
    stream: &mut R,
    owner: &fm_google::transport::CancellationToken,
    finish_on_close: bool,
) -> io::Result<Option<fm_core::Result<Value>>>
where
    F: std::future::Future<Output = fm_core::Result<Value>>,
    R: tokio::io::AsyncBufRead + Unpin,
{
    if owner.is_cancelled() {
        return Ok(None);
    }
    let mut work = std::pin::pin!(work);
    tokio::select! {
        biased;
        _=owner.cancelled()=> {
            if finish_on_close {let _=work.await;}
            Ok(None)
        },
        result=&mut work=>Ok(Some(result)),
        ready=stream.fill_buf()=> {
            owner.cancel();
            // An admitted provider mutation must record its result even when
            // the reply has no recipient. Cancellation stops HTTP/preflight;
            // the worker then persists accepted or uncertain exact-attempt state.
            if finish_on_close {let _=work.await;}
            if ready?.is_empty() {Ok(None)} else {
                Err(io::Error::new(io::ErrorKind::InvalidData,"only one request may be in flight per connection"))
            }
        }
    }
}

async fn session_write<W: tokio::io::AsyncWrite + Unpin>(
    stream: &mut W,
    reply: &Value,
    cancel: &fm_google::transport::CancellationToken,
) -> io::Result<()> {
    tokio::select! {
        biased;
        _=cancel.cancelled()=>Err(io::Error::new(io::ErrorKind::Interrupted,"engine session stopped")),
        result=tokio::time::timeout(IO_TIMEOUT,fm_ipc::write_frame(stream,reply))=>result?,
    }
}

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

#[derive(Clone)]
struct SessionServices {
    google: Option<GoogleServices>,
    content: Arc<local_content::Runtime>,
    assistance: Arc<local_model::Runtime>,
    remote: Arc<remote_content::Runtime>,
}

#[cfg(test)]
impl SessionServices {
    fn for_test(queue: mpsc::Sender<Work>) -> Self {
        Self {
            google: None,
            assistance: Arc::new(local_model::Runtime::new(core_tasks::CoreBridge::new(
                queue.clone(),
                Arc::new(Semaphore::new(16)),
            ))),
            content: Arc::new(local_content::Runtime::new(core_tasks::CoreBridge::new(
                queue.clone(),
                Arc::new(Semaphore::new(verification_fixture::BACKGROUND_LIMIT)),
            ))),
            remote: Arc::new(remote_content::Runtime::new(
                core_tasks::CoreBridge::new(queue, Arc::new(Semaphore::new(4))),
                &std::env::temp_dir(),
                None,
            )),
        }
    }
}

async fn session(
    stream: AnyStream,
    token: Arc<String>,
    queue: mpsc::Sender<Work>,
    hello_metadata: Arc<Value>,
    foreground_slots: Arc<Semaphore>,
    services: SessionServices,
    engine_cancel: fm_google::transport::CancellationToken,
) -> io::Result<()> {
    // Cancellation is cooperative inside session_inner. In particular, never
    // drop an admitted provider mutation before its exact outcome is recorded.
    let result = session_inner(
        stream,
        token,
        queue,
        hello_metadata,
        foreground_slots,
        services,
        engine_cancel.clone(),
    )
    .await;
    if engine_cancel.is_cancelled() {
        Ok(())
    } else {
        result
    }
}

async fn session_inner(
    stream: AnyStream,
    token: Arc<String>,
    queue: mpsc::Sender<Work>,
    hello_metadata: Arc<Value>,
    foreground_slots: Arc<Semaphore>,
    services: SessionServices,
    engine_cancel: fm_google::transport::CancellationToken,
) -> io::Result<()> {
    let mut stream = BufReader::new(stream);
    let hello = tokio::select! {
        biased;
        _=engine_cancel.cancelled()=>return Ok(()),
        result=tokio::time::timeout(HANDSHAKE_TIMEOUT,fm_ipc::read_frame(&mut stream))=>result??.ok_or_else(||io::Error::new(io::ErrorKind::UnexpectedEof,"no handshake"))?,
    };
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
        session_write(stream.get_mut(), &reply, &engine_cancel).await?;
        return Ok(());
    }
    session_write(
        stream.get_mut(),
        &response(&request.id, (*hello_metadata).clone()),
        &engine_cancel,
    )
    .await?;
    let owner_id = fm_ipc::random_token();
    let owner_cancel = engine_cancel.child_token();
    let _owner_cancel = SessionCancellation(owner_cancel.clone());
    loop {
        // An idle authenticated GUI/watch is an explicit owner, not a stale job.
        let empty = tokio::select! {
            biased;
            _=engine_cancel.cancelled()=>return Ok(()),
            ready=stream.fill_buf()=>ready?.is_empty(),
        };
        if empty {
            return Ok(());
        }
        let frame = tokio::select! {
            biased;
            _=engine_cancel.cancelled()=>return Ok(()),
            result=tokio::time::timeout(IO_TIMEOUT,fm_ipc::read_frame(&mut stream))=>match result?? {Some(frame)=>frame,None=>return Ok(())},
        };
        let request: Request = match serde_json::from_slice(&frame) {
            Ok(r) => r,
            Err(_) => {
                session_write(
                    stream.get_mut(),
                    &error("", "invalid_request", "invalid request envelope"),
                    &engine_cancel,
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
            session_write(
                stream.get_mut(),
                &error(
                    &request.id,
                    "invalid_request",
                    "request needs a short id/method and object params",
                ),
                &engine_cancel,
            )
            .await?;
            continue;
        }
        if request.version != PROTOCOL_VERSION {
            session_write(
                stream.get_mut(),
                &error(&request.id, "protocol_mismatch", "supported protocol is 1"),
                &engine_cancel,
            )
            .await?;
            continue;
        }
        if local_model::Runtime::handles(&request.method) {
            let work = services.assistance.handle(
                &owner_id,
                owner_cancel.clone(),
                &request.method,
                request.account.as_deref(),
                request.params,
            );
            let Some(result) = await_sync_reply(work, &mut stream, &owner_cancel, false).await?
            else {
                return Ok(());
            };
            let reply = match result {
                Ok(v) => response(&request.id, v),
                Err(e) => error(&request.id, &e.code, &e.message),
            };
            session_write(stream.get_mut(), &reply, &engine_cancel).await?;
            continue;
        }
        if request.method == "account_remove" || request.method == "google_disconnect" {
            services
                .assistance
                .cancel_account(request.account.as_deref());
        }
        if request.method == "account_remove" {
            services.remote.forget_account(request.account.as_deref());
        }
        if remote_content::Runtime::handles(&request.method) {
            let work = services.remote.handle(
                owner_cancel.clone(),
                &request.method,
                request.account.as_deref(),
                request.params,
            );
            // Closing the connection cancels an image fetch; nothing is cached for it.
            let Some(result) = await_sync_reply(work, &mut stream, &owner_cancel, false).await?
            else {
                return Ok(());
            };
            let reply = match result {
                Ok(v) => response(&request.id, v),
                Err(e) => error(&request.id, &e.code, &e.message),
            };
            let reply = if serde_json::to_vec(&reply)?.len() + 1 > fm_ipc::MAX_FRAME {
                error(
                    &request.id,
                    "response_too_large",
                    "Message content exceeds its bound",
                )
            } else {
                reply
            };
            session_write(stream.get_mut(), &reply, &engine_cancel).await?;
            continue;
        }
        if google_auth::Runtime::handles(&request.method) {
            let reply = if let Some(provider) = &services.google {
                let work = provider.auth.handle(
                    &owner_id,
                    owner_cancel.clone(),
                    &request.method,
                    request.account.as_deref(),
                    request.params,
                );
                let finish_on_close = request.method == "account_remove";
                let Some(result) =
                    await_sync_reply(work, &mut stream, &owner_cancel, finish_on_close).await?
                else {
                    return Ok(());
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
            session_write(stream.get_mut(), &reply, &engine_cancel).await?;
            continue;
        }
        if google_sync::Runtime::handles(&request.method) {
            let result = if let Some(provider) = &services.google {
                let finish_on_close = matches!(
                    request.method.as_str(),
                    "google_calendar_write"
                        | "google_calendar_reconcile"
                        | "google_calendar_resume"
                        | "google_draft_sync"
                        | "google_draft_reconcile"
                        | "google_draft_resolve"
                        | "google_draft_adopt"
                        | "google_draft_delete"
                        | "google_draft_delete_reconcile"
                        | "google_send"
                        | "google_send_reconcile"
                );
                let work = provider.sync.handle(
                    &owner_id,
                    owner_cancel.clone(),
                    &request.method,
                    request.account.as_deref(),
                    request.params,
                );
                let Some(result) =
                    await_sync_reply(work, &mut stream, &owner_cancel, finish_on_close).await?
                else {
                    return Ok(());
                };
                // Pausing an account pauses its local model work too.
                if result.is_ok() {
                    if let Some(account) = request.account.as_deref() {
                        match request.method.as_str() {
                            "google_sync_release" => services.assistance.hold(account),
                            "google_sync_start" => services.assistance.release(account).await,
                            _ => {}
                        }
                    }
                }
                result
            } else {
                Err(fm_core::CoreError {
                    code: "capability_unavailable".into(),
                    message: "Google sync requires a live profile".into(),
                    details: if request.method == "google_send" {
                        json!({"send_invocation_finished_before_dispatch":true})
                    } else if request.method == "google_draft_delete" {
                        json!({"draft_delete_invocation_finished_before_dispatch":true})
                    } else if request.method == "google_draft_adopt" {
                        json!({"draft_authoring_invocation_finished_without_commit":true})
                    } else {
                        Value::Null
                    },
                })
            };
            let reply = match result {
                Ok(result) => response(&request.id, result),
                Err(e) => {
                    let mut reply = error(&request.id, &e.code, &e.message);
                    if request.method == "google_draft_adopt"
                        && e.details["draft_authoring_invocation_finished_without_commit"] == true
                    {
                        reply["error"]["details"] =
                            json!({"draft_authoring_invocation_finished_without_commit":true});
                    }
                    if request.method == "google_draft_delete"
                        && e.details["draft_delete_invocation_finished_before_dispatch"] == true
                    {
                        reply["error"]["details"] =
                            json!({"draft_delete_invocation_finished_before_dispatch":true});
                    }
                    if request.method == "google_send" {
                        for field in [
                            "send_invocation_finished_before_dispatch",
                            "send_request_obsolete_without_dispatch",
                        ] {
                            if e.details[field] == true {
                                if !reply["error"]["details"].is_object() {
                                    reply["error"]["details"] = json!({});
                                }
                                reply["error"]["details"][field] = Value::Bool(true);
                            }
                        }
                    }
                    reply
                }
            };
            session_write(stream.get_mut(), &reply, &engine_cancel).await?;
            continue;
        }
        if local_content::Runtime::handles(&request.method) {
            let Ok(_permit) = foreground_slots.clone().try_acquire_owned() else {
                let mut reply = error(
                    &request.id,
                    "busy",
                    "engine work queue is full; retry later",
                );
                if request.method == "google_draft_copy" {
                    reply["error"]["details"] =
                        json!({"draft_authoring_invocation_finished_without_commit":true});
                }
                session_write(stream.get_mut(), &reply, &engine_cancel).await?;
                continue;
            };
            let work = services.content.handle(
                owner_cancel.clone(),
                &request.method,
                request.account.as_deref(),
                request.params,
            );
            // Keep worker permits and any admitted short receipt alive through
            // client close. Cancellation stops preparation before another commit.
            let Some(result) = await_sync_reply(work, &mut stream, &owner_cancel, true).await?
            else {
                return Ok(());
            };
            let reply = match result {
                Ok(result) => response(&request.id, result),
                Err(e) => {
                    let mut reply = error(&request.id, &e.code, &e.message);
                    if request.method == "google_draft_copy"
                        && e.details["draft_authoring_invocation_finished_without_commit"] == true
                    {
                        reply["error"]["details"] =
                            json!({"draft_authoring_invocation_finished_without_commit":true});
                    }
                    reply
                }
            };
            let reply = if serde_json::to_vec(&reply)?.len() + 1 > fm_ipc::MAX_FRAME {
                error(
                    &request.id,
                    "response_too_large",
                    "Local content response exceeds its bound",
                )
            } else {
                reply
            };
            session_write(stream.get_mut(), &reply, &engine_cancel).await?;
            continue;
        }
        let cadence_changed = (request.method == "preferences_save"
            && ["sync_every_minutes", "google_sync_services"]
                .iter()
                .any(|key| request.params["preferences"].get(key).is_some()))
            || (request.method == "account_update"
                && (request.params.get("sync").is_some()
                    || request.params.get("mail_retention").is_some()));
        let id = request.id.clone();
        let Ok(permit) = foreground_slots.clone().try_acquire_owned() else {
            session_write(
                stream.get_mut(),
                &error(&id, "busy", "engine work queue is full; retry later"),
                &engine_cancel,
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
            session_write(
                stream.get_mut(),
                &error(&id, "busy", "engine work queue is full; retry later"),
                &engine_cancel,
            )
            .await?;
            continue;
        }
        // A disconnected owner cancels its queued work and active SQL reads. Once a
        // mutation has begun its short transaction, finish its durable outcome;
        // the same request_id can recover that outcome after reconnecting.
        let reply = tokio::select! {
            biased;
            _=engine_cancel.cancelled()=>return Ok(()),
            reply = reply_rx => reply.map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "engine worker stopped"))?,
            ready = stream.fill_buf() => {
                if ready?.is_empty() { return Ok(()); }
                return Err(io::Error::new(io::ErrorKind::InvalidData, "only one request may be in flight per connection"));
            }
        };
        if cadence_changed && reply["ok"] == true {
            if let Some(provider) = &services.google {
                provider.sync.cadence_changed().await;
            }
        }
        let reply = if serde_json::to_vec(&reply)?.len() + 1 > fm_ipc::MAX_FRAME {
            error(
                &id,
                "response_too_large",
                "use a smaller page or an attachment file path",
            )
        } else {
            reply
        };
        session_write(stream.get_mut(), &reply, &engine_cancel).await?;
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
    #[cfg(feature = "verification-google")]
    if !args.verification_google {
        return Err(
            "This isolated build requires --verification-google and a new disposable live profile"
                .into(),
        );
    }
    if args.verification_fixture.is_some() && args.profile.is_none() {
        return Err("verification fixtures require an explicit disposable profile".into());
    }
    let require_live = args.live_profile || (args.profile.is_none() && !args.demo_profile);
    let selected_profile = match args.profile {
        Some(profile) => profile,
        None if args.demo_profile => fm_ipc::demo_profile()?,
        None => fm_ipc::default_profile()?,
    };
    let profile = fm_ipc::private_profile(&selected_profile)?;
    // This refusal is present in ordinary builds too. A verification profile
    // must never fall through to real Google services on a later startup.
    verification_google::refuse_marked_profile(&profile)?;
    #[cfg(feature = "verification-google")]
    if args.verification_google {
        verification_google::claim_empty_profile(&profile)?;
    }
    if let Some(mode) = args.verification_fixture {
        verification_fixture::preflight(mode, &profile)?;
    }
    let mut store = match if args.demo_profile || args.verification_fixture.is_some() {
        fm_core::Store::open_synthetic(&profile)
    } else if require_live || !profile.join("store.sqlite3").exists() {
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
            if e.code == "schema_mismatch" || e.code == "profile_kind_mismatch" {
                let _ = fm_ipc::write_startup_refusal(
                    &profile,
                    &fm_ipc::StartupRefusal {
                        pid: std::process::id(),
                        code: e.code.clone(),
                        message: e.message.clone(),
                    },
                );
            }
            return Err(e.into());
        }
    };
    fm_ipc::clear_startup_refusal(&profile);
    let mut fixture = args
        .verification_fixture
        .map(|mode| verification_fixture::Fixture::prepare(mode, &profile, &mut store))
        .transpose()?;
    #[cfg(feature = "verification-google")]
    let google_fixture = args
        .verification_google
        .then(|| {
            verification_google::Fixture::prepare_with_occurrences(
                &profile,
                &mut store,
                args.verification_google_send,
                args.verification_google_drafts,
                args.verification_google_draft_actions,
                args.verification_google_calendar_base,
                args.verification_google_accounts_base,
                args.verification_google_occurrences_base,
            )
        })
        .transpose()?;
    diagnostics::initialize(&profile);
    #[cfg(not(feature = "verification-google"))]
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
    // Every profile now has bounded local content preparation. Reserve its
    // short Core calls alongside Google/fixture work without enlarging the FIFO.
    let foreground_slots = Arc::new(Semaphore::new(
        WORK_LIMIT - verification_fixture::BACKGROUND_LIMIT,
    ));
    let background_slots = Arc::new(Semaphore::new(verification_fixture::BACKGROUND_LIMIT));
    let engine_cancel = fm_google::transport::CancellationToken::new();
    #[cfg(not(feature = "verification-google"))]
    let provider = provider_namespace
        .map(|namespace| {
            google_auth::Runtime::new(
                core_tasks::CoreBridge::new(queue.clone(), background_slots.clone()),
                namespace,
                engine_cancel.clone(),
            )
        })
        .transpose()?;
    #[cfg(feature = "verification-google")]
    let provider = Some(
        google_fixture
            .as_ref()
            .ok_or("Verification fixture was not initialized")?
            .auth(
                core_tasks::CoreBridge::new(queue.clone(), background_slots.clone()),
                engine_cancel.clone(),
            ),
    );
    let sync = provider.as_ref().map(|auth| {
        google_sync::Runtime::new(
            core_tasks::CoreBridge::new(queue.clone(), background_slots.clone()),
            auth.clone(),
            engine_cancel.clone(),
        )
    });
    let services = SessionServices {
        assistance: Arc::new(local_model::Runtime::new(core_tasks::CoreBridge::new(
            queue.clone(),
            background_slots.clone(),
        ))),
        google: provider
            .as_ref()
            .zip(sync.as_ref())
            .map(|(auth, sync)| GoogleServices {
                auth: auth.clone(),
                sync: sync.clone(),
            }),
        content: Arc::new(local_content::Runtime::new(core_tasks::CoreBridge::new(
            queue.clone(),
            background_slots.clone(),
        ))),
        remote: Arc::new(remote_content::Runtime::new(
            core_tasks::CoreBridge::new(queue.clone(), background_slots.clone()),
            &profile,
            store.vault_namespace().ok(),
        )),
    };
    services.remote.start_link_indexer(engine_cancel.clone());
    let owners = Arc::new(AtomicUsize::new(0));
    let authenticated_work = Arc::new(AtomicBool::new(false));
    let schedule = Arc::new(verification_fixture::Schedule::default());
    let worker_owners = owners.clone();
    let worker_authenticated = authenticated_work.clone();
    let worker_schedule = schedule.clone();
    #[cfg(feature = "verification-google")]
    let writer_google_fixture = google_fixture.clone();
    #[cfg(feature = "verification-google")]
    let google_reporter = google_fixture
        .as_ref()
        .map(|fixture| fixture.start_reporter(engine_cancel.clone()));
    let worker = std::thread::spawn(move || {
        #[cfg(feature = "verification-google")]
        let _trace = writer_google_fixture
            .as_ref()
            .map(|f| f.install_writer_trace());
        let mut store = store;
        let mut pending = PendingWork::default();
        let mut ingestion = None;
        let mut closed = false;
        loop {
            // All profiles have 16 foreground and 16 background permits. They
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
                    let owner=Owner(owners.clone());let token=token.clone();let queue=queue.clone();let hello_metadata=hello_metadata.clone();let foreground_slots=foreground_slots.clone();let services=services.clone();let stop=engine_cancel.clone();
                    clients.spawn(async move {
                        let _owner=owner;let _permit=permit;
                        let _=session(stream,token,queue,hello_metadata,foreground_slots,services,stop).await;
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
    drop(services);
    if let Some(sync) = &sync {
        sync.shutdown().await;
    }
    drop(sync);
    if let Some(provider) = &provider {
        provider.shutdown().await;
    }
    drop(provider);
    drop(queue);
    let writer_finished = worker.join().is_ok();
    #[cfg(feature = "verification-google")]
    if let Some(fixture) = google_fixture {
        if let Some(reporter) = google_reporter {
            let _ = reporter.await;
        }
        fixture.finish(writer_finished).await?;
    }
    #[cfg(not(feature = "verification-google"))]
    let _ = writer_finished;
    Ok(())
}

fn handshake_metadata(store: &mut fm_core::Store) -> fm_core::Result<Value> {
    let status = store.handle("status", None, json!({}))?;
    let methods = [
        "assist_jobs",
        "assist_backlog",
        "assist_catch_up",
        "assist_progress",
        "assist_catalog",
        "assist_settings_get",
        "assist_settings_save",
        "assist_discover",
        "assist_start",
        "assist_job",
        "assist_cancel",
        "assist_watch",
        "assist_results",
        "assist_correct",
        "assist_accept",
        "accounts",
        "status",
        "conversations",
        "conversation",
        "flag",
        "draft_get",
        "message_source",
        "google_sent",
        "google_sent_message",
        "draft_save",
        "draft_capture",
        "draft_attachment_import",
        "calendar",
        "attachment",
        "categories",
        "label_catalog",
        "label_create",
        "label_describe",
        "label_remove",
        "label_auto_read",
        "label_auto_spam",
        "set_category",
        "view_get",
        "view_save",
        "preferences_get",
        "preferences_save",
        "calendar_sources",
        "calendar_reminders",
        "calendar_reminder_ack",
        "mail_notifications",
        "calendar_combined",
        "calendar_search",
        "account_update",
        "draft_discard",
        "capture_history",
        "calendar_write",
        "calendar_write_ack",
        "calendar_operations",
        "drafts",
        "draft_create",
        "sender_summary",
        "recipient_suggestions",
        "search_senders",
        "message_html",
        "message_image",
        "link_host_summary",
        "link_pattern_counts",
        "remote_proxy_test",
        "remote_proxy_password",
        "remote_image_cache",
        "draft_transfer",
        "draft_transfer_status",
        "draft_transfer_retry",
        "google_auth_configuration",
        "google_auth_begin",
        "google_auth_restore_begin",
        "google_auth_status",
        "google_auth_cancel",
        "google_disconnect",
        "accounts_archived",
        "account_remove_preview",
        "account_remove",
        "account_remove_status",
        "account_archive_delete",
        "google_sync_start",
        "google_sync_refresh",
        "google_sync_stop",
        "google_sync_release",
        "google_sync_status",
        "google_sync_busy",
        "mail_filing",
        "mail_undo",
        "google_mail_mutation_status",
        "google_calendar_write",
        "google_calendar_reconcile",
        "google_calendar_operation",
        "google_calendar_resume",
        "google_calendar_abandon",
        "google_calendar_occurrence_read",
        "google_labels",
        "google_labels_refresh",
        "google_labels_sync",
        "google_labels_sync_status",
        "google_labels_sync_cancel",
        "google_category_bind",
        "google_draft_sync",
        "google_draft_status",
        "google_draft_reconcile",
        "google_draft_resolve",
        "google_send",
        "google_send_status",
        "google_send_reconcile",
        "google_send_abandon",
        "google_drafts_refresh",
        "google_drafts_status",
        "google_draft_sources",
        "google_draft_source",
        "google_draft_authoring",
        "google_draft_copy",
        "google_draft_adopt",
        "google_draft_delete",
        "google_draft_delete_status",
        "google_draft_delete_reconcile",
        "google_draft_delete_abandon",
        "google_spam_list",
        "google_spam_message",
        "google_remote_search",
        "google_remote_thread",
        "google_remote_attachment",
        "google_unsubscribe_mail",
        "google_bin_counts",
        "google_not_spam",
        "google_delete_forever",
        "google_empty",
        "message_unsubscribe",
    ];
    Ok(
        json!({"protocol":PROTOCOL_VERSION,"schema_version":status["schema_version"],
        "fixture_revision":status["fixture_revision"],"capabilities":status["capabilities"],
        "mode":status["profile_kind"],"max_frame_bytes":fm_ipc::MAX_FRAME,"max_page_size":100,
        "methods":methods.as_slice()}),
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
    async fn hello_reports_fresh_fixture_revision_and_advertises_additive_methods() {
        for expected_fixture in [3] {
            let temp = tempfile::tempdir().unwrap();
            let mut store = fm_core::Store::open(temp.path()).unwrap();
            let metadata = Arc::new(handshake_metadata(&mut store).unwrap());
            let (client, server) = tokio::io::duplex(8192);
            let mut client = BufReader::new(client);
            let (queue, _receive) = mpsc::channel(1);
            let task = tokio::spawn(session(
                Box::new(server),
                Arc::new("fixture-token".into()),
                queue.clone(),
                metadata,
                Arc::new(Semaphore::new(WORK_LIMIT)),
                SessionServices::for_test(queue.clone()),
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
                "calendar_search",
                "calendar_write",
                "calendar_operations",
                "google_send",
                "google_send_status",
                "google_send_reconcile",
                "google_send_abandon",
                "google_drafts_refresh",
                "google_drafts_status",
                "google_draft_sources",
                "google_draft_source",
                "google_draft_authoring",
                "google_draft_copy",
                "google_draft_adopt",
                "google_draft_delete",
                "google_draft_delete_status",
                "google_draft_delete_reconcile",
                "google_draft_delete_abandon",
            ] {
                assert!(methods.contains(&json!(method)), "missing {method}");
            }
            assert!(!methods.contains(&json!("verification_fixture")));
            fm_ipc::write_frame(client.get_mut(), &json!({"version":1,"id":"send","method":"google_send","account":"original","params":{"draft_id":"local","expected_revision":1,"expected_google_revision":0,"request_id":"never-provider"}})).await.unwrap();
            let refused: Value =
                serde_json::from_slice(&fm_ipc::read_frame(&mut client).await.unwrap().unwrap())
                    .unwrap();
            assert_eq!(refused["error"]["code"], "capability_unavailable");
            assert_eq!(
                refused["error"]["details"]["send_invocation_finished_before_dispatch"],
                true
            );
            fm_ipc::write_frame(client.get_mut(), &json!({"version":1,"id":"delete","method":"google_draft_delete","account":"original","params":{"draft_id":"remote","source_revision":1,"request_id":"never-delete"}})).await.unwrap();
            let refused: Value =
                serde_json::from_slice(&fm_ipc::read_frame(&mut client).await.unwrap().unwrap())
                    .unwrap();
            assert_eq!(refused["error"]["code"], "capability_unavailable");
            assert_eq!(
                refused["error"]["details"]["draft_delete_invocation_finished_before_dispatch"],
                true
            );
            fm_ipc::write_frame(client.get_mut(), &json!({"version":1,"id":"adopt","method":"google_draft_adopt","account":"original","params":{"draft_id":"remote","source_revision":1,"request_id":"never-adopt"}})).await.unwrap();
            let refused: Value =
                serde_json::from_slice(&fm_ipc::read_frame(&mut client).await.unwrap().unwrap())
                    .unwrap();
            assert_eq!(refused["error"]["code"], "capability_unavailable");
            assert_eq!(
                refused["error"]["details"]["draft_authoring_invocation_finished_without_commit"],
                true
            );
            drop(client);
            tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        }
    }

    #[tokio::test]
    async fn closed_or_pipelined_clients_finish_their_cancelled_provider_receipt() {
        use tokio::io::AsyncWriteExt;
        for pipelined in [false, true] {
            let (mut client, server) = tokio::io::duplex(64);
            let mut stream = BufReader::new(server);
            let owner = fm_google::transport::CancellationToken::new();
            let recorded = Arc::new(AtomicBool::new(false));
            let saved = recorded.clone();
            let cancelled = owner.clone();
            let work = async move {
                cancelled.cancelled().await;
                tokio::task::yield_now().await;
                saved.store(true, Ordering::Release);
                Ok(json!({"state":"unknown"}))
            };
            if pipelined {
                client.write_all(b"unexpected next request").await.unwrap();
            } else {
                client.shutdown().await.unwrap();
            }
            let result = tokio::time::timeout(
                Duration::from_secs(2),
                await_sync_reply(work, &mut stream, &owner, true),
            )
            .await
            .unwrap();
            assert!(recorded.load(Ordering::Acquire));
            assert!(owner.is_cancelled());
            if pipelined {
                assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
            } else {
                assert!(result.unwrap().is_none());
            }
        }
    }

    #[tokio::test]
    async fn completed_provider_reply_is_returned_without_cancelling_owner() {
        let (_client, server) = tokio::io::duplex(64);
        let mut stream = BufReader::new(server);
        let owner = fm_google::transport::CancellationToken::new();
        let reply = await_sync_reply(
            async { Ok(json!({"state":"acknowledged"})) },
            &mut stream,
            &owner,
            true,
        )
        .await
        .unwrap()
        .unwrap()
        .unwrap();
        assert_eq!(reply["state"], "acknowledged");
        assert!(!owner.is_cancelled());
    }

    #[tokio::test]
    async fn engine_cancel_drains_admitted_receipt_with_client_still_connected() {
        use tokio::sync::Notify;
        let (_client, server) = tokio::io::duplex(64);
        let engine = fm_google::transport::CancellationToken::new();
        let owner = engine.child_token();
        let entered = Arc::new(Notify::new());
        let persisting = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let recorded = Arc::new(AtomicBool::new(false));
        let (started, writing, finish, saved) = (
            entered.clone(),
            persisting.clone(),
            release.clone(),
            recorded.clone(),
        );
        let operation_owner = owner.clone();
        let task = tokio::spawn(async move {
            let work = async move {
                started.notify_one();
                operation_owner.cancelled().await;
                writing.notify_one();
                finish.notified().await;
                saved.store(true, Ordering::Release);
                Ok(json!({"state":"unknown"}))
            };
            await_sync_reply(work, &mut BufReader::new(server), &owner, true).await
        });
        tokio::time::timeout(Duration::from_secs(2), entered.notified())
            .await
            .unwrap();
        engine.cancel();
        tokio::time::timeout(Duration::from_secs(2), persisting.notified())
            .await
            .unwrap();
        assert!(
            !task.is_finished(),
            "shutdown must wait for the admitted receipt"
        );
        release.notify_one();
        assert!(tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .is_none());
        assert!(recorded.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn actual_sessions_cancel_idle_and_partial_reads_without_dropping_clients() {
        use tokio::io::AsyncWriteExt;
        for stage in ["handshake", "partial_handshake", "idle", "partial_request"] {
            let (client, server) = tokio::io::duplex(4096);
            let mut client = BufReader::new(client);
            let engine = fm_google::transport::CancellationToken::new();
            let (queue, _receive) = mpsc::channel(1);
            let task = tokio::spawn(session(
                Box::new(server),
                Arc::new("fixture-token".into()),
                queue.clone(),
                Arc::new(json!({"protocol":1})),
                Arc::new(Semaphore::new(1)),
                SessionServices::for_test(queue.clone()),
                engine.clone(),
            ));
            if stage == "partial_handshake" {
                client.get_mut().write_all(b"{\"version\":").await.unwrap();
            }
            if matches!(stage, "idle" | "partial_request") {
                fm_ipc::write_frame(client.get_mut(),&json!({"version":1,"id":"hello","method":"hello","params":{"token":"fixture-token"}})).await.unwrap();
                fm_ipc::read_frame(&mut client).await.unwrap().unwrap();
                if stage == "partial_request" {
                    client
                        .get_mut()
                        .write_all(b"{\"version\":1,")
                        .await
                        .unwrap();
                }
            }
            tokio::task::yield_now().await;
            assert!(!task.is_finished());
            engine.cancel();
            tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .unwrap_or_else(|_| panic!("session hung at {stage}"))
                .unwrap()
                .unwrap();
            drop(client);
        }
    }

    #[tokio::test]
    async fn actual_session_engine_cancel_releases_queued_request_ownership() {
        let (client, server) = tokio::io::duplex(8192);
        let mut client = BufReader::new(client);
        let engine = fm_google::transport::CancellationToken::new();
        let (queue, mut receive) = mpsc::channel(1);
        let slots = Arc::new(Semaphore::new(1));
        let task = tokio::spawn(session(
            Box::new(server),
            Arc::new("fixture-token".into()),
            queue.clone(),
            Arc::new(json!({"protocol":1})),
            slots.clone(),
            SessionServices::for_test(queue.clone()),
            engine.clone(),
        ));
        fm_ipc::write_frame(
            client.get_mut(),
            &json!({"version":1,"id":"hello","method":"hello","params":{"token":"fixture-token"}}),
        )
        .await
        .unwrap();
        fm_ipc::read_frame(&mut client).await.unwrap().unwrap();
        fm_ipc::write_frame(
            client.get_mut(),
            &json!({"version":1,"id":"queued","method":"status","params":{}}),
        )
        .await
        .unwrap();
        let Work::Request(work) = tokio::time::timeout(Duration::from_secs(2), receive.recv())
            .await
            .unwrap()
            .unwrap()
        else {
            panic!("expected local request");
        };
        assert!(!work.cancelled.load(Ordering::Acquire));
        engine.cancel();
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(work.cancelled.load(Ordering::Acquire));
        assert!(work.reply.is_closed());
        assert_eq!(slots.available_permits(), 0);
        drop(work);
        assert_eq!(slots.available_permits(), 1);
        drop(client);
    }

    #[tokio::test]
    async fn actual_session_cancels_backpressured_handshake_and_error_replies() {
        use tokio::io::AsyncReadExt;
        for oversized_hello in [true, false] {
            let (client, server) = tokio::io::duplex(64);
            let mut client = BufReader::new(client);
            let engine = fm_google::transport::CancellationToken::new();
            let (queue, _receive) = mpsc::channel(1);
            let metadata = if oversized_hello {
                json!({"protocol":1,"padding":"x".repeat(512)})
            } else {
                json!({"protocol":1})
            };
            let task = tokio::spawn(session(
                Box::new(server),
                Arc::new("fixture-token".into()),
                queue.clone(),
                Arc::new(metadata),
                Arc::new(Semaphore::new(0)),
                SessionServices::for_test(queue.clone()),
                engine.clone(),
            ));
            fm_ipc::write_frame(client.get_mut(),&json!({"version":1,"id":"hello","method":"hello","params":{"token":"fixture-token"}})).await.unwrap();
            if !oversized_hello {
                fm_ipc::read_frame(&mut client).await.unwrap().unwrap();
                fm_ipc::write_frame(
                    client.get_mut(),
                    &json!({"version":1,"id":"q".repeat(128),"method":"status","params":{}}),
                )
                .await
                .unwrap();
            }
            let mut byte = [0];
            tokio::time::timeout(Duration::from_secs(2), client.read_exact(&mut byte))
                .await
                .unwrap()
                .unwrap();
            assert!(!task.is_finished(), "reply must still be backpressured");
            engine.cancel();
            tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            drop(client);
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
            queue.clone(),
            Arc::new(json!({"protocol":1,"fixture_revision":2})),
            foreground_slots.clone(),
            SessionServices::for_test(queue.clone()),
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
        fm_ipc::write_frame(client.get_mut(), &json!({"version":1,"id":"queued","method":"flag","account":"windscribe","params":{"id":"not-yet-executed","read":true}})).await.unwrap();
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
