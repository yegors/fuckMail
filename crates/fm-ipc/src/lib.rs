//! Local IPC between the engine and its clients: newline-delimited JSON frames
//! over a per-user Unix socket or Windows named pipe, engine discovery through
//! the profile's `engine.json`, and [`Client`], which launches the engine on
//! demand and authenticates with the discovery token. The `fm` CLI uses
//! [`Client`]; the Qt GUI speaks the same protocol from C++.
//!
//! Both ends restrict the peer to the same OS user: on Unix each side checks
//! the socket's peer credentials, and on Windows the pipe's DACL admits only
//! the user while the client verifies the server's process and user. The
//! token then proves the client can read the private profile. This crate
//! never opens the mail database.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

#[cfg(windows)]
pub mod windows;

/// Version carried in every frame and in `engine.json`; peers refuse any other.
pub const PROTOCOL_VERSION: u32 = 1;
/// Largest frame in either direction, in bytes, including the trailing newline.
pub const MAX_FRAME: usize = 1024 * 1024;
/// Most client sessions one engine serves at once; extra connections are closed.
pub const MAX_CLIENTS: usize = 16;
/// How long the engine waits for a new connection's `hello` frame.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// Default limit for writing one frame or waiting for one reply.
pub const IO_TIMEOUT: Duration = Duration::from_secs(15);
/// Total time [`Client`] spends discovering, launching and greeting the engine.
pub const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
/// Exit code of an engine that refuses a profile of the other kind (live or
/// demo).
///
/// Engine refusal is permanent; other launch failures retain bounded race recovery.
pub const ENGINE_PROFILE_KIND_MISMATCH: i32 = 12;

/// One request frame: `{version, id, method, account?, params}`.
///
/// `id` is echoed in the response, `account` scopes account-bound methods and
/// `params` is a method-specific object. Unknown fields are refused. The
/// engine also refuses empty or over-128-byte IDs, methods over 64 bytes and
/// non-object `params`.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub id: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(default)]
    pub params: Value,
}

/// The profile's `engine.json`, written by the running engine.
#[derive(Serialize, Deserialize)]
pub struct Discovery {
    pub protocol: u32,
    /// Engine process ID; clients require the connected peer to have it.
    pub pid: u32,
    /// Socket path or pipe name, from [`endpoint`].
    pub endpoint: String,
    /// 64 hex characters, new on every engine start. Clients send it in
    /// `hello` to prove they can read the private profile.
    pub token: String,
}

/// Builds a success frame: `{version, id, ok: true, result}`.
pub fn response(id: &str, result: Value) -> Value {
    json!({"version": PROTOCOL_VERSION,"id":id,"ok":true,"result":result})
}
/// Builds an error frame: `{version, id, ok: false, error: {code, message}}`.
pub fn error(id: &str, code: &str, message: &str) -> Value {
    json!({"version":PROTOCOL_VERSION,"id":id,"ok":false,"error":{"code":code,"message":message}})
}

/// Reads one newline-terminated frame, newline included.
///
/// A bounded line reader: never let `read_line` allocate an unbounded peer frame.
/// Returns `Ok(None)` on a clean end of stream between frames,
/// `UnexpectedEof` if the stream ends mid-frame, and `InvalidData` as soon as
/// a frame would exceed [`MAX_FRAME`].
pub async fn read_frame<R: AsyncBufRead + Unpin>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut out = Vec::with_capacity(4096);
    loop {
        let chunk = reader.fill_buf().await?;
        if chunk.is_empty() {
            return if out.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "incomplete IPC frame",
                ))
            };
        }
        let newline = chunk.iter().position(|b| *b == b'\n');
        let take = newline.map_or(chunk.len(), |n| n + 1);
        if out.len() + take > MAX_FRAME {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "IPC frame exceeds 1 MiB",
            ));
        }
        out.extend_from_slice(&chunk[..take]);
        reader.consume(take);
        if newline.is_some() {
            return Ok(Some(out));
        }
    }
}

/// Writes `value` as one JSON line and flushes. Errors with `InvalidData`,
/// before writing anything, if the frame would exceed [`MAX_FRAME`].
pub async fn write_frame<W: AsyncWrite + Unpin>(writer: &mut W, value: &Value) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    if bytes.len() + 1 > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "response too large; use a smaller page",
        ));
    }
    bytes.push(b'\n');
    writer.write_all(&bytes).await?;
    writer.flush().await
}

/// Returns 32 bytes from the OS random source as 64 lowercase hex characters.
pub fn random_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The ordinary profile path, `fuckMail/default` under the user's data
/// directory: `~/Library/Application Support` on macOS, `%LOCALAPPDATA%` on
/// Windows, and `$XDG_DATA_HOME` or `~/.local/share` elsewhere. Nothing is
/// created; errors with `NotFound` if the base cannot be located.
pub fn default_profile() -> io::Result<PathBuf> {
    profile_path("default")
}

/// Explicit demo selection never shares the ordinary live profile.
pub fn demo_profile() -> io::Result<PathBuf> {
    profile_path("demo-v1")
}

fn profile_path(name: &str) -> io::Result<PathBuf> {
    #[cfg(target_os = "macos")]
    let base =
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"));
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    Ok(base
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "cannot locate user data directory; pass --profile",
            )
        })?
        .join("fuckMail")
        .join(name))
}

/// Creates the profile directory if needed, checks that it is private, and
/// returns its canonical path.
///
/// `path` must be absolute. On Unix the directory must be owned by this user
/// with mode 0700; a symlink, a foreign owner or any group/other permission
/// is refused, not fixed. On Windows a reparse point or foreign owner is
/// refused, and the directory's DACL is then reset to user-only (see
/// `windows::private_profile`). Refusals are `PermissionDenied`.
pub fn private_profile(path: &Path) -> io::Result<PathBuf> {
    if !path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "profile must be an absolute path",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt};
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(path)?;
        let md = std::fs::symlink_metadata(path)?;
        if !md.is_dir()
            || md.file_type().is_symlink()
            // SAFETY: geteuid takes no arguments, cannot fail and has no side effects.
            || md.uid() != unsafe { libc::geteuid() }
            || md.mode() & 0o077 != 0
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "profile must be a private, user-owned directory (mode 0700)",
            ));
        }
    }
    #[cfg(windows)]
    windows::private_profile(path)?;
    std::fs::canonicalize(path)
}

/// Reads and checks the profile's `engine.json`.
///
/// It must be a regular file of at most 4 KiB, not a symlink, and on Unix
/// owned by this user with no group/other permissions (`PermissionDenied`
/// otherwise). An unknown protocol or a token that is not 64 characters is
/// `InvalidData`. A missing file is `NotFound`.
pub fn read_discovery(profile: &Path) -> io::Result<Discovery> {
    let path = profile.join("engine.json");
    let md = std::fs::symlink_metadata(&path)?;
    if !md.is_file() || md.file_type().is_symlink() || md.len() > 4096 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "invalid engine discovery file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: geteuid takes no arguments, cannot fail and has no side effects.
        if md.uid() != unsafe { libc::geteuid() } || md.mode() & 0o077 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "engine discovery file is not private",
            ));
        }
    }
    let d: Discovery = serde_json::from_slice(&std::fs::read(path)?)?;
    if d.protocol != PROTOCOL_VERSION || d.token.len() != 64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "incompatible engine discovery",
        ));
    }
    Ok(d)
}

/// Replaces `engine.json` with `d` through a private temp file that is
/// fsynced and renamed into place. On Windows the old file is removed first,
/// so a reader may briefly find none.
pub fn write_discovery(profile: &Path, d: &Discovery) -> io::Result<()> {
    replace_private_json(
        profile,
        &format!(".engine-{}.json", d.pid),
        "engine.json",
        d,
    )
}

/// Written by an engine that permanently refuses its profile (an older or
/// different-kind store). Clients match the pid they launched and stop
/// retrying, so the reason reaches the person instead of a startup timeout.
pub const STARTUP_REFUSAL_FILE: &str = "engine-refusal.json";

/// A permanent startup refusal: the refusing engine's PID, an error code and
/// a message for the person.
#[derive(Serialize, Deserialize)]
pub struct StartupRefusal {
    pub pid: u32,
    pub code: String,
    pub message: String,
}

/// Records `refusal` in [`STARTUP_REFUSAL_FILE`], replacing it the same way
/// as [`write_discovery`].
pub fn write_startup_refusal(profile: &Path, refusal: &StartupRefusal) -> io::Result<()> {
    replace_private_json(
        profile,
        &format!(".engine-refusal-{}.json", refusal.pid),
        STARTUP_REFUSAL_FILE,
        refusal,
    )
}

/// The refusal recorded by exactly this engine process, if any.
pub fn read_startup_refusal(profile: &Path, pid: u32) -> Option<StartupRefusal> {
    let path = profile.join(STARTUP_REFUSAL_FILE);
    let md = std::fs::symlink_metadata(&path).ok()?;
    if !md.is_file() || md.len() > 4096 {
        return None;
    }
    let refusal: StartupRefusal = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    (refusal.pid == pid).then_some(refusal)
}

/// Removes any recorded refusal, ignoring errors. The engine calls it once it
/// has opened the profile.
pub fn clear_startup_refusal(profile: &Path) {
    let _ = std::fs::remove_file(profile.join(STARTUP_REFUSAL_FILE));
}

fn replace_private_json(
    profile: &Path,
    temp_name: &str,
    name: &str,
    value: &impl Serialize,
) -> io::Result<()> {
    use std::io::Write;
    // The temp file is created new and private, so it can never be a
    // pre-placed file or link, and it is durable before it becomes visible.
    let temp = profile.join(temp_name);
    #[cfg(unix)]
    let mut f = {
        use std::os::unix::fs::OpenOptionsExt;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        opts.mode(0o600);
        opts.open(&temp)?
    };
    #[cfg(windows)]
    let mut f = windows::create_private_file(&temp)?;
    serde_json::to_writer(&mut f, value)?;
    f.write_all(b"\n")?;
    f.sync_all()?;
    drop(f);
    #[cfg(windows)]
    {
        // Engine holds the profile's exclusive writer lock.
        if profile.join(name).exists() {
            std::fs::remove_file(profile.join(name))?;
        }
    }
    std::fs::rename(temp, profile.join(name))
}

/// The IPC endpoint for a profile.
///
/// On Unix this is `engine.sock` inside the profile or, when that path is too
/// long for a socket address, a hashed name in a private
/// `/tmp/fuckmail-<uid>` directory. On Windows it is a named pipe
/// `\\.\pipe\fuckmail-<hash>` derived from the lowercased profile path.
pub fn endpoint(profile: &Path) -> io::Result<String> {
    #[cfg(unix)]
    {
        let p = profile.join("engine.sock");
        // `sun_path` holds about 104 bytes on macOS and 108 on Linux.
        if p.as_os_str().len() >= 100 {
            // A per-user private runtime directory also works for deep checkout paths.
            use sha2::{Digest, Sha256};
            let hash = format!(
                "{:x}",
                Sha256::digest(profile.as_os_str().as_encoded_bytes())
            );
            // SAFETY: geteuid takes no arguments, cannot fail and has no side effects.
            let base =
                PathBuf::from("/tmp").join(format!("fuckmail-{}", unsafe { libc::geteuid() }));
            let base = private_profile(&base)?;
            return Ok(base
                .join(format!("{}.sock", &hash[..24]))
                .to_string_lossy()
                .into_owned());
        }
        Ok(p.to_string_lossy().into_owned())
    }
    #[cfg(windows)]
    {
        // Windows paths are case-insensitive, so one profile maps to one pipe
        // however its path is spelled.
        use sha2::{Digest, Sha256};
        let hash = format!(
            "{:x}",
            Sha256::digest(profile.to_string_lossy().to_lowercase().as_bytes())
        );
        Ok(format!(r"\\.\pipe\fuckmail-{}", &hash[..32]))
    }
}

/// Any bidirectional async byte stream, so sockets and pipes share one type.
pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}
/// A connected engine socket or pipe.
pub type AnyStream = Box<dyn Stream>;

/// Connects to the engine named in `discovery` and verifies the peer before
/// any byte is exchanged.
///
/// On Unix the peer's UID must be this user and, where the OS reports it, its
/// PID must be `discovery.pid`. On Windows the pipe server's PID must match
/// and its process must belong to this user. A mismatch is `PermissionDenied`.
/// The caller still has to send `hello` with the token.
pub async fn connect(discovery: &Discovery) -> io::Result<AnyStream> {
    #[cfg(unix)]
    {
        let stream = tokio::net::UnixStream::connect(&discovery.endpoint).await?;
        let peer = stream.peer_cred()?;
        // SAFETY: geteuid takes no arguments, cannot fail and has no side effects.
        if peer.uid() != unsafe { libc::geteuid() }
            || peer.pid().is_some_and(|pid| pid as u32 != discovery.pid)
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "unexpected engine peer",
            ));
        }
        Ok(Box::new(stream))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        let stream =
            tokio::net::windows::named_pipe::ClientOptions::new().open(&discovery.endpoint)?;
        // SAFETY: `stream` owns this connected pipe and remains alive throughout
        // verification; no other owner can close its handle during this call.
        unsafe { windows::verify_server(stream.as_raw_handle(), discovery.pid)? };
        Ok(Box::new(stream))
    }
}

/// An authenticated connection to a profile's engine, as used by the `fm` CLI.
///
/// One request is in flight at a time, and replies are matched by ID.
pub struct Client {
    stream: BufReader<AnyStream>,
    next_id: u64,
}
impl Client {
    /// Connects to the profile's engine, launching it if needed, and
    /// completes the `hello` handshake. Accepts a profile of either kind.
    ///
    /// Without a live engine it starts `engine` (default: `fm-engine` beside
    /// the current executable) up to 10 times, backing off 1–4 s after each
    /// exit. Errors: `PermissionDenied` for an unexpected peer or a rejected
    /// handshake, `InvalidData` for incompatible discovery or a recorded
    /// startup refusal, and `TimedOut` after [`STARTUP_TIMEOUT`].
    pub async fn connect(profile: &Path, engine: Option<&Path>) -> io::Result<Self> {
        Self::connect_profile(profile, engine, false, false).await
    }

    /// Explicit live initialization must also refuse an already-running demo.
    /// A demo profile or engine is `InvalidInput`.
    pub async fn connect_live(profile: &Path, engine: Option<&Path>) -> io::Result<Self> {
        Self::connect_profile(profile, engine, true, false).await
    }

    /// Synthetic creation is available only to an explicitly selected demo/test.
    /// A live profile or engine is `InvalidInput`.
    pub async fn connect_demo(profile: &Path, engine: Option<&Path>) -> io::Result<Self> {
        Self::connect_profile(profile, engine, false, true).await
    }

    async fn connect_profile(
        profile: &Path,
        engine: Option<&Path>,
        require_live: bool,
        require_demo: bool,
    ) -> io::Result<Self> {
        let profile = private_profile(profile)?;
        let until = tokio::time::Instant::now() + STARTUP_TIMEOUT;
        let mut child: Option<tokio::process::Child> = None;
        let mut child_pid = None;
        let mut next_launch = tokio::time::Instant::now();
        let mut retry_delay = Duration::from_secs(1);
        let mut launches = 0;
        let mut last_connection_failure = String::from("discovery unavailable");
        let mut last_exit = None;
        loop {
            if tokio::time::Instant::now() >= until {
                return Err(io::Error::new(io::ErrorKind::TimedOut, format!(
                    "engine did not become ready after {launches} launch attempts; last connection: {last_connection_failure}; last engine exit: {last_exit:?}; check profile permissions and compatible binaries"
                )));
            }
            match read_discovery(&profile) {
                Ok(discovery) => match tokio::time::timeout_at(until, connect(&discovery)).await? {
                    Ok(stream) => {
                        let mut client = Self {
                            stream: BufReader::new(stream),
                            next_id: 0,
                        };
                        let hello = match tokio::time::timeout_at(
                            until,
                            client.call(
                                "hello",
                                None,
                                json!({"token":discovery.token,"client":"cli"}),
                            ),
                        )
                        .await?
                        {
                            Ok(hello) => hello,
                            Err(e)
                                if matches!(
                                    e.kind(),
                                    io::ErrorKind::UnexpectedEof
                                        | io::ErrorKind::ConnectionReset
                                        | io::ErrorKind::BrokenPipe
                                        | io::ErrorKind::ConnectionAborted
                                        | io::ErrorKind::NotConnected
                                ) =>
                            {
                                // The previous writer may accept a socket just
                                // before shutdown. Retry transport loss only;
                                // malformed/authentication responses stay fatal.
                                last_connection_failure =
                                    format!("pre-authentication transport {:?}", e.kind());
                                tokio::time::sleep_until(
                                    (tokio::time::Instant::now() + Duration::from_millis(70))
                                        .min(until),
                                )
                                .await;
                                continue;
                            }
                            Err(e) => return Err(e),
                        };
                        if hello.get("ok") == Some(&Value::Bool(true)) {
                            if require_live && hello["result"]["mode"] != "live" {
                                return Err(io::Error::new(io::ErrorKind::InvalidInput, "The selected profile is synthetic; choose a separate live profile path"));
                            }
                            if require_demo && hello["result"]["mode"] != "synthetic" {
                                return Err(io::Error::new(io::ErrorKind::InvalidInput, "The selected profile is live; choose a separate demo profile path"));
                            }
                            // Reap our child asynchronously while this client process exists.
                            if let Some(mut process) = child {
                                tokio::spawn(async move {
                                    let _ = process.wait().await;
                                });
                            }
                            return Ok(client);
                        }
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "engine rejected handshake",
                        ));
                    }
                    Err(e) => {
                        // A wrong peer or bad discovery will not fix itself;
                        // anything else may be an engine still starting or
                        // shutting down, so keep trying.
                        if matches!(
                            e.kind(),
                            io::ErrorKind::PermissionDenied | io::ErrorKind::InvalidData
                        ) {
                            return Err(e);
                        }
                        last_connection_failure = format!("local transport {:?}", e.kind());
                    }
                },
                Err(e) => {
                    if matches!(
                        e.kind(),
                        io::ErrorKind::PermissionDenied | io::ErrorKind::InvalidData
                    ) {
                        return Err(e);
                    }
                    last_connection_failure = format!("discovery {:?}", e.kind());
                }
            }
            if let Some(process) = child.as_mut() {
                if let Some(status) = process.try_wait()? {
                    if (require_live || require_demo)
                        && status.code() == Some(ENGINE_PROFILE_KIND_MISMATCH)
                    {
                        return Err(io::Error::new(io::ErrorKind::InvalidInput, "The selected profile has a different kind; choose a separate profile path"));
                    }
                    if let Some(refusal) =
                        child_pid.and_then(|pid| read_startup_refusal(&profile, pid))
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("{} (profile: {})", refusal.message, profile.display()),
                        ));
                    }
                    // Reap our exact child. A losing startup can exit while the
                    // old writer is still closing after discovery was removed.
                    last_exit = Some(status.to_string());
                    child = None;
                    next_launch = tokio::time::Instant::now() + retry_delay;
                    retry_delay = (retry_delay * 2).min(Duration::from_secs(4));
                }
            }
            // A child that is still running may yet publish discovery, so
            // launch another only after the previous one has exited.
            if child.is_none() && launches < 10 && tokio::time::Instant::now() >= next_launch {
                let binary = match engine {
                    Some(p) => p.to_owned(),
                    None => std::env::current_exe()?.with_file_name(if cfg!(windows) {
                        "fm-engine.exe"
                    } else {
                        "fm-engine"
                    }),
                };
                let mut command = tokio::process::Command::new(binary);
                if require_live {
                    command.arg("--live-profile");
                } else if require_demo {
                    command.arg("--demo-profile");
                }
                command
                    .arg("--profile")
                    .arg(&profile)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());
                #[cfg(windows)]
                command.creation_flags(0x08000000); // CREATE_NO_WINDOW, no hidden startup service.
                child=Some(command.spawn().map_err(|e|io::Error::new(e.kind(),format!("cannot start engine: {e}; install fm-engine beside fm or pass --engine")))?);
                child_pid = child.as_ref().and_then(|c| c.id());
                launches += 1;
            }
            tokio::time::sleep_until(
                (tokio::time::Instant::now() + Duration::from_millis(70)).min(until),
            )
            .await;
        }
    }

    /// Sends one request and waits up to [`IO_TIMEOUT`] for its reply.
    ///
    /// Returns the whole response frame, including `ok: false` errors; only
    /// transport, timeout and framing failures are `Err`. Late replies to
    /// earlier calls that timed out are skipped; any other version or ID is
    /// `InvalidData`.
    pub async fn call(
        &mut self,
        method: &str,
        account: Option<&str>,
        params: Value,
    ) -> io::Result<Value> {
        self.call_within(method, account, params, IO_TIMEOUT).await
    }

    /// `call` for work with its own longer engine limit, such as image fetches.
    ///
    /// `timeout` bounds the wait for the reply; the write still uses
    /// [`IO_TIMEOUT`]. If a call times out, its reply may still arrive later;
    /// the next call on this connection skips it.
    pub async fn call_within(
        &mut self,
        method: &str,
        account: Option<&str>,
        params: Value,
        timeout: Duration,
    ) -> io::Result<Value> {
        self.next_id += 1;
        let id = self.next_id.to_string();
        let req = json!({"version":PROTOCOL_VERSION,"id":id,"method":method,"account":account,"params":params});
        tokio::time::timeout(IO_TIMEOUT, write_frame(self.stream.get_mut(), &req)).await??;
        let current = self.next_id;
        let stream = &mut self.stream;
        let response = tokio::time::timeout(timeout, async {
            loop {
                let bytes = read_frame(stream).await?.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::UnexpectedEof, "engine disconnected")
                })?;
                let response: Value = serde_json::from_slice(&bytes)?;
                // IDs only grow, so a smaller one is the late reply of a call
                // that already timed out. Drop it and keep waiting for ours.
                let earlier = response["id"]
                    .as_str()
                    .and_then(|id| id.parse::<u64>().ok())
                    .is_some_and(|id| id < current);
                if !(earlier && response["version"] == PROTOCOL_VERSION) {
                    return Ok::<_, io::Error>(response);
                }
            }
        })
        .await??;
        if response["version"] != PROTOCOL_VERSION || response["id"] != id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid engine response version or request ID",
            ));
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ordinary_and_demo_defaults_have_separate_names() {
        let ordinary = super::default_profile().unwrap();
        let demo = super::demo_profile().unwrap();
        assert_eq!(ordinary.file_name().unwrap(), "default");
        assert_eq!(demo.file_name().unwrap(), "demo-v1");
        assert_eq!(ordinary.parent(), demo.parent());
        assert_ne!(ordinary, demo);
    }
    use super::*;
    #[cfg(unix)]
    fn fake_engine() -> (tempfile::TempDir, PathBuf, tokio::net::UnixListener) {
        let temp = tempfile::tempdir().unwrap();
        let profile = private_profile(&temp.path().join("profile")).unwrap();
        let d = Discovery {
            protocol: 1,
            pid: std::process::id(),
            endpoint: endpoint(&profile).unwrap(),
            token: random_token(),
        };
        let listener = tokio::net::UnixListener::bind(&d.endpoint).unwrap();
        write_discovery(&profile, &d).unwrap();
        (temp, profile, listener)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn startup_retries_transport_loss_after_accept_before_hello_reply() {
        let (_temp, profile, listener) = fake_engine();
        let server = tokio::spawn(async move {
            let (first, _) = listener.accept().await.unwrap();
            let mut first = BufReader::new(first);
            read_frame(&mut first).await.unwrap().unwrap();
            // The old writer accepted our socket, then closed during shutdown.
            drop(first);
            let (next, _) = listener.accept().await.unwrap();
            let mut next = BufReader::new(next);
            let hello: Value =
                serde_json::from_slice(&read_frame(&mut next).await.unwrap().unwrap()).unwrap();
            write_frame(
                next.get_mut(),
                &response(hello["id"].as_str().unwrap(), json!({"protocol":1})),
            )
            .await
            .unwrap();
            let request: Value =
                serde_json::from_slice(&read_frame(&mut next).await.unwrap().unwrap()).unwrap();
            write_frame(
                next.get_mut(),
                &response(request["id"].as_str().unwrap(), json!({"recovered":true})),
            )
            .await
            .unwrap();
        });
        // No engine executable is provided: recovery must reconnect to the
        // existing endpoint rather than needlessly launch a replacement.
        let mut client =
            tokio::time::timeout(Duration::from_secs(3), Client::connect(&profile, None))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(
            client.call("status", None, json!({})).await.unwrap()["result"]["recovered"],
            true
        );
        server.await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn late_reply_to_a_timed_out_call_does_not_fail_the_next_call() {
        let (_temp, profile, listener) = fake_engine();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut stream = BufReader::new(stream);
            let hello: Value =
                serde_json::from_slice(&read_frame(&mut stream).await.unwrap().unwrap()).unwrap();
            write_frame(
                stream.get_mut(),
                &response(hello["id"].as_str().unwrap(), json!({"protocol":1})),
            )
            .await
            .unwrap();
            // Answer the slow call only after the client has given up on it,
            // immediately followed by the reply to the next call.
            let slow: Value =
                serde_json::from_slice(&read_frame(&mut stream).await.unwrap().unwrap()).unwrap();
            let next: Value =
                serde_json::from_slice(&read_frame(&mut stream).await.unwrap().unwrap()).unwrap();
            for (request, value) in [(&slow, "late"), (&next, "current")] {
                write_frame(
                    stream.get_mut(),
                    &response(request["id"].as_str().unwrap(), json!({"reply":value})),
                )
                .await
                .unwrap();
            }
        });
        let mut client =
            tokio::time::timeout(Duration::from_secs(3), Client::connect(&profile, None))
                .await
                .unwrap()
                .unwrap();
        let timed_out = client
            .call_within("slow", None, json!({}), Duration::from_millis(50))
            .await
            .unwrap_err();
        assert_eq!(timed_out.kind(), io::ErrorKind::TimedOut);
        let reply = client.call("status", None, json!({})).await.unwrap();
        assert_eq!(reply["result"]["reply"], "current");
        server.await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn startup_does_not_retry_auth_or_protocol_rejection() {
        for wrong_version in [false, true] {
            let (_temp, profile, listener) = fake_engine();
            let server = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let mut stream = BufReader::new(stream);
                let hello: Value =
                    serde_json::from_slice(&read_frame(&mut stream).await.unwrap().unwrap())
                        .unwrap();
                let mut rejected = error(
                    hello["id"].as_str().unwrap(),
                    "unauthorized",
                    "fixture rejection",
                );
                if wrong_version {
                    rejected["version"] = json!(999);
                }
                write_frame(stream.get_mut(), &rejected).await.unwrap();
            });
            let error =
                match tokio::time::timeout(Duration::from_secs(2), Client::connect(&profile, None))
                    .await
                    .unwrap()
                {
                    Err(error) => error,
                    Ok(_) => panic!("rejected engine must not authenticate"),
                };
            assert_eq!(
                error.kind(),
                if wrong_version {
                    io::ErrorKind::InvalidData
                } else {
                    io::ErrorKind::PermissionDenied
                }
            );
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn framing_is_bounded_and_handles_fragmentation() {
        let (mut writer, reader) = tokio::io::duplex(32);
        tokio::spawn(async move {
            writer.write_all(b"{\"x\":").await.unwrap();
            writer.write_all(b"1}\nsecond\n").await.unwrap();
        });
        let mut reader = BufReader::new(reader);
        assert_eq!(
            read_frame(&mut reader).await.unwrap().unwrap(),
            b"{\"x\":1}\n"
        );
        assert_eq!(read_frame(&mut reader).await.unwrap().unwrap(), b"second\n");
        assert!(read_frame(&mut reader).await.unwrap().is_none());
        let bytes = vec![b'x'; MAX_FRAME + 1];
        assert_eq!(
            read_frame(&mut &bytes[..]).await.unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(
            read_frame(&mut &b"partial"[..]).await.unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }
    #[test]
    fn discovery_roundtrip_and_profile_permissions() {
        let temp = tempfile::tempdir().unwrap();
        let profile = private_profile(&temp.path().join("profile")).unwrap();
        let d = Discovery {
            protocol: 1,
            pid: std::process::id(),
            endpoint: endpoint(&profile).unwrap(),
            token: random_token(),
        };
        write_discovery(&profile, &d).unwrap();
        assert_eq!(read_discovery(&profile).unwrap().token, d.token);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                profile.join("engine.json"),
                std::fs::Permissions::from_mode(0o644),
            )
            .unwrap();
            assert!(read_discovery(&profile).is_err());
        }
    }
}
