use clap::Parser;
use fm_ipc::{
    error, response, AnyStream, Discovery, Request, HANDSHAKE_TIMEOUT, IO_TIMEOUT, MAX_CLIENTS,
    PROTOCOL_VERSION,
};
use serde_json::{json, Value};
use std::{
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
    sync::{mpsc, oneshot, Semaphore},
};

#[derive(Parser)]
#[command(
    version,
    about = "fuckMail local engine — runs only for explicit GUI/CLI sessions"
)]
struct Args {
    #[arg(long)]
    profile: Option<PathBuf>,
}

struct Work {
    request: Request,
    reply: oneshot::Sender<Value>,
    cancelled: Arc<AtomicBool>,
}

struct RequestOwner(Arc<AtomicBool>);
impl Drop for RequestOwner {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
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
        .max_instances((MAX_CLIENTS + 1) as usize);
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

async fn session(
    stream: AnyStream,
    token: Arc<String>,
    queue: mpsc::Sender<Work>,
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
    fm_ipc::write_frame(stream.get_mut(),&response(&request.id,json!({"protocol":1,"schema_version":fm_core::SCHEMA_VERSION,"fixture_revision":1,"mode":"synthetic","max_frame_bytes":fm_ipc::MAX_FRAME,"max_page_size":100,"methods":["accounts","status","conversations","conversation","flag","draft_get","draft_save","draft_capture","calendar","attachment","categories","set_category","view_get","view_save"]}))).await?;
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
        let id = request.id.clone();
        let (reply_tx, reply_rx) = oneshot::channel();
        let request_owner = RequestOwner(Arc::new(AtomicBool::new(false)));
        // Bounded global work queue, one request in flight per connection.
        if queue
            .try_send(Work {
                request,
                reply: reply_tx,
                cancelled: Arc::clone(&request_owner.0),
            })
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
        std::process::exit(1);
    }
}

async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let profile = fm_ipc::private_profile(&args.profile.unwrap_or(fm_ipc::default_profile()?))?;
    let store = match fm_core::Store::open(&profile) {
        Ok(s) => s,
        Err(e) => {
            // A launch race is benign only if the existing engine responds.
            if let Ok(d) = fm_ipc::read_discovery(&profile) {
                if fm_ipc::connect(&d).await.is_ok() {
                    return Ok(());
                }
            }
            return Err(format!("cannot open profile: {e}").into());
        }
    };
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
    let (queue, mut receive) = mpsc::channel::<Work>(32);
    let worker = std::thread::spawn(move || {
        let mut store = store;
        while let Some(work) = receive.blocking_recv() {
            if work.reply.is_closed() {
                continue;
            }
            let id = work.request.id;
            let result = store.handle_cancellable(
                &work.request.method,
                work.request.account.as_deref(),
                work.request.params,
                work.cancelled,
            );
            let reply = match result {
                Ok(result) => response(&id, result),
                Err(e) => error(&id, &e.code, &e.message),
            };
            let _ = work.reply.send(reply);
        }
        // Dropping SQLite closes/checkpoints without bypassing the core's writer lock.
        drop(store);
    });
    let owners = Arc::new(AtomicUsize::new(0));
    let slots = Arc::new(Semaphore::new(MAX_CLIENTS));
    let start = tokio::time::Instant::now();
    let mut ever_connected = false;
    let mut idle_since = None;
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    loop {
        tokio::select! {
            incoming=accept(&mut listener)=>match incoming {
                Ok(stream)=> {
                    let Ok(permit)=slots.clone().try_acquire_owned() else {drop(stream);continue;};
                    owners.fetch_add(1,Ordering::SeqCst);ever_connected=true;idle_since=None;
                    let owner=Owner(owners.clone());let token=token.clone();let queue=queue.clone();
                    tokio::spawn(async move {let _owner=owner;let _permit=permit;let _=session(stream,token,queue).await;});
                },
                Err(e)=> {eprintln!("fm-engine: local accept failed: {}",e.kind());tokio::time::sleep(Duration::from_millis(50)).await;}
            },
            _=tick.tick()=> {
                if owners.load(Ordering::SeqCst)==0 {
                    if ever_connected {
                        let since=idle_since.get_or_insert_with(tokio::time::Instant::now);
                        if since.elapsed()>=Duration::from_millis(350) {break;}
                    } else if start.elapsed()>=Duration::from_secs(10) {break;}
                }
            },
            _=tokio::signal::ctrl_c()=>break,
        }
    }
    // Stop accepting before removing discovery. Outstanding client jobs finish
    // durable core writes; normal final-client shutdown already has no clients.
    drop(listener);
    cleanup(&profile, &endpoint, pid);
    drop(queue);
    if owners.load(Ordering::SeqCst) == 0 {
        let _ = worker.join();
    }
    Ok(())
}

#[cfg(test)]
mod session_tests {
    use super::*;

    #[tokio::test]
    async fn disconnect_marks_only_that_queued_request_cancelled() {
        let (client, server) = tokio::io::duplex(8192);
        let mut client = BufReader::new(client);
        let (queue, mut receive) = mpsc::channel(1);
        let task = tokio::spawn(session(
            Box::new(server),
            Arc::new("fixture-token".into()),
            queue,
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
        assert!(!work.cancelled.load(Ordering::Acquire));
        drop(client);
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(work.cancelled.load(Ordering::Acquire));
        assert!(work.reply.is_closed());
    }
}
