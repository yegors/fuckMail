use clap::{Parser, Subcommand};
use fm_ipc::Client;
use serde_json::{json, Value};
use std::{
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Parser)]
#[command(
    name = "fm",
    version,
    about = "fuckMail: local mail and calendar, through the same engine as the GUI",
    after_help = "The default profile is synthetic. Use profile-init with an explicit path for live Google accounts.\nGoogle sign-in starts only with an explicit google connect command. fm never opens the mail database directly."
)]
struct Args {
    #[arg(long, global = true)]
    profile: Option<PathBuf>,
    #[arg(long, global = true)]
    engine: Option<PathBuf>,
    #[arg(long, global = true)]
    account: Option<String>,
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize a separate empty live profile; requires an explicit --profile.
    ProfileInit,
    /// Explicit Google sign-in and retained-account connection management.
    Google {
        #[command(subcommand)]
        command: Google,
    },
    /// List separate accounts and truthful coverage.
    Accounts,
    /// Engine/schema/fixture status (starts an explicit temporary session).
    Status,
    /// Bounded conversation reading, searching and explicit user actions.
    Mail {
        #[command(subcommand)]
        command: Mail,
    },
    /// Human-authored draft content; no send command in this phase.
    Draft {
        #[command(subcommand)]
        command: Draft,
    },
    /// Read or save typed local display preferences.
    Preferences {
        #[command(subcommand)]
        command: Preferences,
    },
    /// Update display name and signature, preserving existing draft snapshots.
    AccountUpdate {
        #[arg(long)]
        name: String,
        #[arg(long)]
        signature_file: PathBuf,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        request_id: String,
    },
    /// List all cached calendar sources and their permissions.
    CalendarSources,
    /// Nonrecurring create/edit or self RSVP through deterministic local fake acknowledgment.
    CalendarWrite {
        #[arg(long)]
        params_file: PathBuf,
    },
    /// Inspect the selected account's local calendar mutation journal.
    CalendarOperations {
        #[arg(long, default_value_t = 30)]
        limit: u32,
        #[arg(long)]
        cursor: Option<String>,
    },
    /// Acknowledge a previously prepared local calendar operation.
    CalendarAck {
        request_id: String,
    },
    /// Read a half-open date range from the selected account or combined calendars.
    Calendar {
        #[arg(long)]
        combined: bool,
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
    },
    /// Copy a verified retained attachment to a new file, or inspect its state.
    Attachment {
        id: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Categories,
    /// Explicit foreground session; report changes until Ctrl-C or --seconds.
    Watch {
        #[arg(long)]
        seconds: Option<u64>,
    },
    /// Exercise a documented protocol method with JSON read from a file or '-'.
    Call {
        method: String,
        #[arg(long)]
        params_file: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum Google {
    /// Open your browser and keep this CLI session until login finishes or Ctrl-C.
    /// Add --account for an identity-checked reconnect.
    Connect {
        #[arg(long)]
        client_config: PathBuf,
        #[arg(long)]
        reauthorize: bool,
    },
    /// Stop Google access while retaining downloaded content, drafts and credentials.
    Disconnect,
    /// Download mail and Calendar while this explicit CLI session remains open.
    /// Supply the four actual Google Console per-minute quota limits.
    Sync {
        #[arg(long, value_parser=clap::value_parser!(u32).range(100..=100_000_000))]
        gmail_project_units_per_minute: u32,
        #[arg(long, value_parser=clap::value_parser!(u32).range(100..=100_000_000))]
        gmail_account_units_per_minute: u32,
        #[arg(long, value_parser=clap::value_parser!(u32).range(100..=100_000_000))]
        calendar_project_units_per_minute: u32,
        #[arg(long, value_parser=clap::value_parser!(u32).range(100..=100_000_000))]
        calendar_account_units_per_minute: u32,
        #[arg(long)]
        seconds: Option<u64>,
    },
    /// Inspect active acquisition; does not start provider requests.
    SyncStatus,
    /// Stop this account's acquisition and retain all downloaded and unsent work.
    SyncStop,
}

#[derive(Subcommand)]
enum Mail {
    List {
        #[arg(long)]
        query: Option<String>,
        #[arg(long,default_value="all",value_parser=["all","sender","subject","body"])]
        field: String,
        #[arg(long,default_value="inbox",value_parser=["inbox","sent","all"])]
        folder: String,
        #[arg(long)]
        category: Option<String>,
        #[arg(long)]
        sender: Option<String>,
        #[arg(long)]
        exact_sender: Option<String>,
        #[arg(long,value_parser=["all","people","automated","unknown"])]
        kind: Option<String>,
        #[arg(long,value_parser=["all","review","held","done"])]
        status: Option<String>,
        #[arg(long)]
        has_attachment: Option<bool>,
        #[arg(long)]
        after: Option<String>,
        #[arg(long)]
        before: Option<String>,
        #[arg(long)]
        grouped: bool,
        #[arg(long)]
        cursor: Option<String>,
        #[arg(long,default_value_t=30,value_parser=clap::value_parser!(u32).range(1..=100))]
        limit: u32,
        #[arg(long, default_value_t = 0)]
        offset: u64,
    },
    /// Query only; this does not mark mail reviewed.
    Read { id: String },
    /// Exact sender history in this account; querying never marks mail reviewed.
    Sender { address: String },
    /// Mark the loaded snapshot read.
    Review {
        id: String,
        #[arg(long)]
        expected_revision: Option<u64>,
        #[arg(long)]
        request_id: Option<String>,
    },
    /// Atomically mark the loaded snapshot read and clear all its stars.
    Done {
        id: String,
        #[arg(long)]
        expected_revision: Option<u64>,
        #[arg(long)]
        request_id: Option<String>,
    },
    /// Place the loaded snapshot on untimed hold without changing read state.
    Delay {
        id: String,
        #[arg(long)]
        expected_revision: Option<u64>,
        #[arg(long)]
        request_id: Option<String>,
    },
    /// Clear snapshot stars without changing read state.
    Undelay {
        id: String,
        #[arg(long)]
        expected_revision: Option<u64>,
        #[arg(long)]
        request_id: Option<String>,
    },
    Flag {
        id: String,
        #[arg(long)]
        read: Option<bool>,
        #[arg(long)]
        starred: Option<bool>,
        #[arg(long)]
        expected_revision: Option<u64>,
        #[arg(long)]
        request_id: Option<String>,
    },
    Category {
        id: String,
        category_id: String,
        #[arg(long)]
        remove: bool,
        #[arg(long)]
        expected_revision: Option<u64>,
        #[arg(long)]
        request_id: Option<String>,
    },
}

#[derive(Subcommand)]
enum Preferences {
    Show,
    /// Apply a bounded preferences JSON patch from a file or stdin (-).
    Save {
        #[arg(long)]
        preferences_file: PathBuf,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        request_id: String,
    },
}

#[derive(Subcommand)]
enum Draft {
    /// Browse durable drafts in the selected account.
    List {
        #[arg(long)]
        query: Option<String>,
        #[arg(long,default_value_t=30,value_parser=clap::value_parser!(u32).range(1..=100))]
        limit: u32,
        #[arg(long, default_value_t = 0)]
        offset: u64,
    },
    /// Create local human composition; no network send is available.
    Create {
        #[arg(long, default_value="new", value_parser=["new","reply","reply_all","forward"])]
        mode: String,
        #[arg(long)]
        conversation_id: Option<String>,
        #[arg(long)]
        source_message_id: Option<String>,
        #[arg(long)]
        request_id: Option<String>,
    },
    /// Inspect bounded local capture history; never claims provider delivery.
    History {
        #[arg(long)]
        conversation_id: Option<String>,
        #[arg(long, conflicts_with = "conversation_id")]
        draft_id: Option<String>,
        #[arg(long)]
        unresolved_only: bool,
        #[arg(long, default_value_t = 30)]
        limit: u32,
        #[arg(long, default_value_t = 0)]
        offset: u64,
    },
    /// Explicitly clear saved body, HTML, signature and attachment selection.
    Discard {
        #[command(flatten)]
        target: DraftTarget,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        request_id: String,
    },
    /// Serialize a saved draft into a local .eml capture. Never uses the network.
    Capture {
        #[command(flatten)]
        target: DraftTarget,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        request_id: String,
        #[arg(long, default_value="accepted", value_parser=["accepted","unknown"])]
        outcome: String,
    },
    Show {
        #[command(flatten)]
        target: DraftTarget,
    },
    Save {
        #[command(flatten)]
        target: DraftTarget,
        #[arg(long)]
        subject: Option<String>,
        /// JSON object with optional to_recipients/cc_recipients/bcc_recipients arrays.
        #[arg(long)]
        recipients_file: Option<PathBuf>,
        #[arg(long)]
        body_file: PathBuf,
        #[arg(long)]
        html_file: Option<PathBuf>,
        #[arg(long)]
        signature_file: Option<PathBuf>,
        #[arg(long)]
        attachment: Vec<String>,
        #[arg(long)]
        clear_attachments: bool,
        #[arg(long)]
        expected_revision: Option<u64>,
        #[arg(long)]
        request_id: Option<String>,
    },
}

#[derive(clap::Args)]
struct DraftTarget {
    /// Legacy conversation reply identity.
    #[arg(required_unless_present = "draft_id", conflicts_with = "draft_id")]
    conversation_id: Option<String>,
    /// Independent draft identity returned by draft create/list.
    #[arg(
        long,
        required_unless_present = "conversation_id",
        conflicts_with = "conversation_id"
    )]
    draft_id: Option<String>,
}
impl DraftTarget {
    fn params(&self) -> Value {
        match &self.draft_id {
            Some(id) => json!({"draft_id":id}),
            None => json!({"conversation_id":self.conversation_id}),
        }
    }
}

#[derive(Debug)]
struct Failure {
    code: String,
    message: String,
}
impl From<io::Error> for Failure {
    fn from(e: io::Error) -> Self {
        Self {
            code: if e.kind() == io::ErrorKind::BrokenPipe {
                "broken_pipe"
            } else if e.kind() == io::ErrorKind::PermissionDenied {
                "permission_denied"
            } else {
                "io_error"
            }
            .into(),
            message: e.to_string(),
        }
    }
}
fn invalid(msg: &str) -> Failure {
    Failure {
        code: "invalid_request".into(),
        message: msg.into(),
    }
}
fn result(reply: Value) -> Result<Value, Failure> {
    if reply["ok"] == true {
        Ok(reply["result"].clone())
    } else {
        Err(Failure {
            code: reply["error"]["code"]
                .as_str()
                .unwrap_or("engine_error")
                .into(),
            message: reply["error"]["message"]
                .as_str()
                .unwrap_or("engine request failed")
                .into(),
        })
    }
}
fn read_input(path: &Path, limit: u64) -> Result<Vec<u8>, Failure> {
    let reader: Box<dyn Read> = if path == Path::new("-") {
        Box::new(io::stdin())
    } else {
        Box::new(std::fs::File::open(path)?)
    };
    let mut data = Vec::new();
    reader.take(limit + 1).read_to_end(&mut data)?;
    if data.len() as u64 > limit {
        return Err(invalid("input file is too large"));
    }
    Ok(data)
}
fn account(args: &Args) -> Result<&str, Failure> {
    args.account.as_deref().ok_or_else(|| {
        invalid("this command requires --account; use 'fm accounts' to list accounts")
    })
}

fn open_google_browser(url: &str) -> Result<(), Failure> {
    // The engine supplies one fixed HTTPS authorization endpoint. No shell
    // evaluates the query, and URLs/state are never copied to terminal logs.
    if !url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?")
        || url.len() > 16384
        || url.chars().any(char::is_control)
        || url.contains('#')
    {
        return Err(invalid("The engine returned an invalid Google sign-in URL"));
    }
    #[cfg(windows)]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let windows = std::env::var_os("SystemRoot")
            .ok_or_else(|| invalid("Windows browser launcher is unavailable"))?;
        let mut command =
            std::process::Command::new(PathBuf::from(windows).join("System32/rundll32.exe"));
        command
            .arg("url.dll,FileProtocolHandler")
            .arg(url)
            .creation_flags(0x08000000);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("/usr/bin/open");
        command.arg(url);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(url);
        command
    };
    let mut child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| invalid("The default browser could not be opened"))?;
    // Reap only the launcher; OAuth remains tied to this CLI's engine session.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    if let Err(e) = run(&args).await {
        if e.code == "broken_pipe" {
            return;
        }
        if args.json {
            let _ = writeln!(
                io::stdout(),
                "{}",
                fm_ipc::error("cli", &e.code, &e.message)
            );
        } else {
            eprintln!("fm: {}: {}", e.code, e.message);
        }
        std::process::exit(match e.code.as_str() {
            "invalid_request" | "invalid_params" | "request_id_conflict" => 2,
            "permission_denied" | "unauthorized" | "authentication_required" => 3,
            "stale_revision" | "stale_cursor" => 4,
            "not_found" | "unsupported" | "content_unavailable" => 5,
            _ => 1,
        });
    }
}

async fn run(args: &Args) -> Result<(), Failure> {
    let profile = match &args.profile {
        Some(p) => p.to_owned(),
        None => fm_ipc::default_profile()?,
    };
    let engine = args
        .engine
        .clone()
        .or_else(|| std::env::var_os("FM_ENGINE_PATH").map(PathBuf::from));
    // Validate scope and local input before starting any engine work.
    if !matches!(
        args.command,
        Command::Accounts
            | Command::Status
            | Command::Watch { .. }
            | Command::Call { .. }
            | Command::Preferences { .. }
            | Command::CalendarSources
            | Command::Calendar { combined: true, .. }
            | Command::ProfileInit
            | Command::Google {
                command: Google::Connect { .. }
            }
            | Command::Google {
                command: Google::SyncStatus
            }
    ) {
        account(args)?;
    }
    let live_command = matches!(args.command, Command::ProfileInit | Command::Google { .. });
    if live_command && args.profile.is_none() {
        return Err(invalid(
            "profile-init and Google commands require an explicit --profile path",
        ));
    }
    let google_client_path = if let Command::Google {
        command: Google::Connect { client_config, .. },
    } = &args.command
    {
        Some(
            std::fs::canonicalize(client_config)
                .map_err(|_| invalid("Choose an existing desktop client JSON file"))?,
        )
    } else {
        None
    };
    let mut client = if live_command {
        Client::connect_live(&profile, engine.as_deref()).await?
    } else {
        Client::connect(&profile, engine.as_deref()).await?
    };
    let scope = args.account.as_deref();
    let reply = match &args.command {
        Command::Accounts => client.call("accounts", None, json!({})).await?,
        Command::Status | Command::ProfileInit => client.call("status", None, json!({})).await?,
        Command::Google {
            command: Google::Disconnect,
        } => client.call("google_disconnect", scope, json!({})).await?,
        Command::Google {
            command: Google::SyncStatus,
        } => client.call("google_sync_status", scope, json!({})).await?,
        Command::Google {
            command: Google::SyncStop,
        } => client.call("google_sync_stop", scope, json!({})).await?,
        Command::Google {
            command:
                Google::Sync {
                    gmail_project_units_per_minute,
                    gmail_account_units_per_minute,
                    calendar_project_units_per_minute,
                    calendar_account_units_per_minute,
                    seconds,
                },
        } => {
            let reply=client.call("google_sync_start",scope,json!({"gmail_project_units_per_minute":gmail_project_units_per_minute,"gmail_account_units_per_minute":gmail_account_units_per_minute,"calendar_project_units_per_minute":calendar_project_units_per_minute,"calendar_account_units_per_minute":calendar_account_units_per_minute})).await?;
            result(reply.clone())?;
            emit(args, &reply)?;
            if !args.json {
                eprintln!("Google sync is running in this CLI session. Press Ctrl-C to stop.");
            }
            let started = tokio::time::Instant::now();
            let mut previous = Value::Null;
            loop {
                if seconds.is_some_and(|limit| started.elapsed() >= Duration::from_secs(limit)) {
                    break;
                }
                let wait = seconds
                    .map(|limit| {
                        Duration::from_secs(limit)
                            .saturating_sub(started.elapsed())
                            .min(Duration::from_secs(1))
                    })
                    .unwrap_or(Duration::from_secs(1));
                let interrupted = tokio::select! {_=tokio::signal::ctrl_c()=>true,_=tokio::time::sleep(wait)=>false};
                if interrupted {
                    break;
                }
                let status = client.call("google_sync_status", scope, json!({})).await?;
                let state = result(status.clone())?;
                if state != previous {
                    emit(args, &status)?;
                    previous = state.clone();
                }
                if state["accounts"]
                    .as_array()
                    .is_none_or(|accounts| accounts.iter().all(|a| a["active"] != true))
                {
                    break;
                }
            }
            client.call("google_sync_release", scope, json!({})).await?
        }
        Command::Google {
            command: Google::Connect { reauthorize, .. },
        } => {
            let path = google_client_path
                .as_ref()
                .ok_or_else(|| invalid("Choose an existing desktop client JSON file"))?;
            let path = path
                .to_str()
                .ok_or_else(|| invalid("Desktop client path is not valid Unicode"))?;
            let begin = result(
                client
                    .call(
                        "google_auth_begin",
                        scope,
                        json!({"client_config_path":path,"reauthorize":reauthorize}),
                    )
                    .await?,
            )?;
            let attempt = begin["attempt_id"]
                .as_str()
                .ok_or_else(|| invalid("Engine omitted the sign-in attempt"))?
                .to_owned();
            let url = begin["authorization_url"]
                .as_str()
                .ok_or_else(|| invalid("Engine omitted Google sign-in"))?;
            if let Err(error) = open_google_browser(url) {
                let _ = client
                    .call("google_auth_cancel", None, json!({"attempt_id":attempt}))
                    .await;
                return Err(error);
            }
            if !args.json {
                eprintln!("Finish Google sign-in in your browser. Press Ctrl-C to cancel.");
            }
            let start = tokio::time::Instant::now();
            loop {
                let reply = client
                    .call("google_auth_status", None, json!({"attempt_id":attempt}))
                    .await?;
                let state = result(reply.clone())?;
                match state["phase"].as_str() {
                    Some("connected") => {
                        emit(args, &reply)?;
                        return Ok(());
                    }
                    Some("cancelled") => {
                        return Err(Failure {
                            code: "cancelled".into(),
                            message: "Google sign-in was cancelled".into(),
                        })
                    }
                    Some("failed") => {
                        return Err(Failure {
                            code: state["error"]["code"]
                                .as_str()
                                .unwrap_or("google_auth_failed")
                                .into(),
                            message: state["error"]["message"]
                                .as_str()
                                .unwrap_or("Google sign-in failed")
                                .into(),
                        })
                    }
                    _ => {}
                }
                let interrupted = tokio::select! { _=tokio::signal::ctrl_c()=>true, _=tokio::time::sleep(Duration::from_millis(500))=>false };
                if interrupted || start.elapsed() >= Duration::from_secs(330) {
                    let _ = client
                        .call("google_auth_cancel", None, json!({"attempt_id":attempt}))
                        .await;
                    return Err(Failure {
                        code: "cancelled".into(),
                        message: "Google sign-in was cancelled".into(),
                    });
                }
            }
        }
        Command::Preferences { command } => match command {
            Preferences::Show => client.call("preferences_get", None, json!({})).await?,
            Preferences::Save {
                preferences_file,
                expected_revision,
                request_id,
            } => {
                let preferences: Value =
                    serde_json::from_slice(&read_input(preferences_file, 16 * 1024)?)
                        .map_err(|_| invalid("Preferences file must be JSON"))?;
                client.call("preferences_save",None,json!({"preferences":preferences,"expected_revision":expected_revision,"request_id":request_id})).await?
            }
        },
        Command::AccountUpdate {
            name,
            signature_file,
            expected_revision,
            request_id,
        } => {
            let signature = String::from_utf8(read_input(signature_file, 16 * 1024)?)
                .map_err(|_| invalid("Signature must be UTF-8"))?;
            client.call("account_update",scope,json!({"name":name,"signature_text":signature,"expected_revision":expected_revision,"request_id":request_id})).await?
        }
        Command::CalendarSources => client.call("calendar_sources", None, json!({})).await?,
        Command::CalendarWrite { params_file } => {
            let params: Value = serde_json::from_slice(&read_input(params_file, 64 * 1024)?)
                .map_err(|_| invalid("Calendar write must be JSON"))?;
            client.call("calendar_write", scope, params).await?
        }
        Command::CalendarOperations { limit, cursor } => {
            let mut params = json!({"limit":limit});
            if let Some(cursor) = cursor {
                if cursor.len() > 1024 {
                    return Err(invalid("Calendar cursor exceeds 1024 bytes"));
                }
                let cursor: Value = serde_json::from_str(cursor)
                    .map_err(|_| invalid("Calendar cursor must be JSON"))?;
                if !cursor.is_object() {
                    return Err(invalid("Calendar cursor must be an object"));
                }
                params["cursor"] = cursor;
            }
            client.call("calendar_operations", scope, params).await?
        }
        Command::CalendarAck { request_id } => {
            client
                .call(
                    "calendar_write_ack",
                    scope,
                    json!({"request_id":request_id}),
                )
                .await?
        }
        Command::Categories => client.call("categories", scope, json!({})).await?,
        Command::Mail { command } => match command {
            Mail::List {
                query,
                field,
                folder,
                category,
                sender,
                exact_sender,
                kind,
                status,
                has_attachment,
                after,
                before,
                grouped,
                cursor,
                limit,
                offset,
            } => {
                let mut params =
                    json!({"field":field,"folder":folder,"limit":limit,"offset":offset});
                if let Some(query) = query {
                    params["query"] = json!(query);
                }
                if let Some(category) = category {
                    params["category"] = json!(category);
                }
                for (key, value) in [
                    ("sender", sender),
                    ("exact_sender", exact_sender),
                    ("kind", kind),
                    ("status", status),
                    ("after", after),
                    ("before", before),
                ] {
                    if let Some(value) = value {
                        params[key] = json!(value);
                    }
                }
                params["grouped"] = json!(grouped);
                if let Some(value) = has_attachment {
                    params["has_attachment"] = json!(value);
                }
                if let Some(cursor) = cursor {
                    params.as_object_mut().unwrap().remove("offset");
                    params["cursor"] = json!(cursor);
                }
                client.call("conversations", scope, params).await?
            }
            Mail::Read { id } => client.call("conversation", scope, json!({"id":id})).await?,
            Mail::Sender { address } => {
                client
                    .call("sender_summary", scope, json!({"email":address}))
                    .await?
            }
            Mail::Review {
                id,
                expected_revision,
                request_id,
            }
            | Mail::Done {
                id,
                expected_revision,
                request_id,
            }
            | Mail::Delay {
                id,
                expected_revision,
                request_id,
            }
            | Mail::Undelay {
                id,
                expected_revision,
                request_id,
            } => {
                let snapshot = result(client.call("conversation", scope, json!({"id":id})).await?)?;
                let mut params = json!({"id":id,"message_ids":snapshot["message_ids"],"expected_revision":expected_revision.map(Value::from).unwrap_or(snapshot["revision"].clone()),"request_id":request_id.clone().unwrap_or_else(fm_ipc::random_token)});
                match command {
                    Mail::Review { .. } => params["read"] = json!(true),
                    Mail::Done { .. } => {
                        params["read"] = json!(true);
                        params["starred"] = json!(false);
                    }
                    Mail::Delay { .. } => params["starred"] = json!(true),
                    Mail::Undelay { .. } => params["starred"] = json!(false),
                    _ => unreachable!(),
                }
                client.call("flag", scope, params).await?
            }
            Mail::Flag {
                id,
                read,
                starred,
                expected_revision,
                request_id,
            } => {
                if read.is_none() && starred.is_none() {
                    return Err(invalid("provide --read true/false or --starred true/false"));
                }
                let snapshot = result(client.call("conversation", scope, json!({"id":id})).await?)?;
                let mut params = json!({"id":id,"message_ids":snapshot["message_ids"],"expected_revision":expected_revision.map(Value::from).unwrap_or(snapshot["revision"].clone()),"request_id":request_id.clone().unwrap_or_else(fm_ipc::random_token)});
                if let Some(read) = read {
                    params["read"] = json!(read);
                }
                if let Some(starred) = starred {
                    params["starred"] = json!(starred);
                }
                client.call("flag", scope, params).await?
            }
            Mail::Category {
                id,
                category_id,
                remove,
                expected_revision,
                request_id,
            } => {
                let snapshot = result(client.call("conversation", scope, json!({"id":id})).await?)?;
                client.call("set_category",scope,json!({"id":id,"message_ids":snapshot["message_ids"],"category_id":category_id,"present":!remove,"expected_revision":expected_revision.map(Value::from).unwrap_or(snapshot["revision"].clone()),"request_id":request_id.clone().unwrap_or_else(fm_ipc::random_token)})).await?
            }
        },
        Command::Draft { command } => match command {
            Draft::List {
                query,
                limit,
                offset,
            } => {
                let mut params = json!({"limit":limit,"offset":offset});
                if let Some(query) = query {
                    params["query"] = json!(query);
                }
                client.call("drafts", scope, params).await?
            }
            Draft::Create {
                mode,
                conversation_id,
                source_message_id,
                request_id,
            } => {
                let mut params = json!({"mode":mode,"request_id":request_id.clone().unwrap_or_else(fm_ipc::random_token)});
                if let Some(cid) = conversation_id {
                    params["conversation_id"] = json!(cid);
                }
                if let Some(mid) = source_message_id {
                    params["source_message_id"] = json!(mid);
                }
                client.call("draft_create", scope, params).await?
            }
            Draft::History {
                conversation_id,
                draft_id,
                limit,
                offset,
                unresolved_only,
            } => {
                let mut params =
                    json!({"limit":limit,"offset":offset,"unresolved_only":unresolved_only});
                if let Some(cid) = conversation_id {
                    params["conversation_id"] = json!(cid);
                }
                if let Some(did) = draft_id {
                    params["draft_id"] = json!(did);
                }
                client.call("capture_history", scope, params).await?
            }
            Draft::Discard {
                target,
                expected_revision,
                request_id,
            } => {
                let mut params = target.params();
                params["expected_revision"] = json!(expected_revision);
                params["request_id"] = json!(request_id);
                client.call("draft_discard", scope, params).await?
            }
            Draft::Capture {
                target,
                expected_revision,
                request_id,
                outcome,
            } => {
                let mut params = target.params();
                params["expected_revision"] = json!(expected_revision);
                params["request_id"] = json!(request_id);
                params["outcome"] = json!(outcome);
                client.call("draft_capture", scope, params).await?
            }
            Draft::Show { target } => client.call("draft_get", scope, target.params()).await?,
            Draft::Save {
                target,
                subject,
                recipients_file,
                body_file,
                html_file,
                signature_file,
                attachment,
                clear_attachments,
                expected_revision,
                request_id,
            } => {
                let body = String::from_utf8(read_input(body_file, 256 * 1024)?)
                    .map_err(|_| invalid("draft must be UTF-8 text"))?;
                let current = result(client.call("draft_get", scope, target.params()).await?)?;
                let mut params = target.params();
                params["body"] = json!(body);
                if (current["revision"] == 0 || *expected_revision == Some(0))
                    && target.conversation_id.is_some()
                    && current["source_message_id"].is_string()
                {
                    params["source_message_id"] = current["source_message_id"].clone();
                }
                params["expected_revision"] = expected_revision
                    .map(Value::from)
                    .unwrap_or(current["revision"].clone());
                params["request_id"] =
                    json!(request_id.clone().unwrap_or_else(fm_ipc::random_token));
                if let Some(subject) = subject {
                    params["subject"] = json!(subject);
                }
                if let Some(path) = recipients_file {
                    let addresses: Value = serde_json::from_slice(&read_input(path, 64 * 1024)?)
                        .map_err(|_| invalid("Recipients file must be valid JSON"))?;
                    let fields = addresses
                        .as_object()
                        .ok_or_else(|| invalid("Recipients file must contain an object"))?;
                    for (key, value) in fields {
                        if !["to_recipients", "cc_recipients", "bcc_recipients"]
                            .contains(&key.as_str())
                        {
                            return Err(invalid("Unknown recipients file field"));
                        }
                        params[key] = value.clone();
                    }
                }
                if let Some(path) = signature_file {
                    params["signature_text"] =
                        json!(String::from_utf8(read_input(path, 16 * 1024)?)
                            .map_err(|_| invalid("Signature must be UTF-8"))?);
                }
                if let Some(path) = html_file {
                    params["body_html"] = json!(String::from_utf8(read_input(path, 256 * 1024)?)
                        .map_err(|_| invalid("HTML must be UTF-8"))?);
                }
                if *clear_attachments && !attachment.is_empty() {
                    return Err(invalid("Choose attachments or --clear-attachments"));
                }
                if *clear_attachments || !attachment.is_empty() {
                    params["attachment_ids"] = json!(attachment);
                }
                client.call("draft_save", scope, params).await?
            }
        },
        Command::Calendar { from, to, combined } => {
            let mut params = json!({});
            if let Some(from) = from {
                params["from"] = json!(from);
            }
            if let Some(to) = to {
                params["to"] = json!(to);
            }
            client
                .call(
                    if *combined {
                        "calendar_combined"
                    } else {
                        "calendar"
                    },
                    if *combined { None } else { scope },
                    params,
                )
                .await?
        }
        Command::Attachment { id, output } => {
            let reply = client.call("attachment", scope, json!({"id":id})).await?;
            if let Some(output) = output {
                let data = result(reply.clone())?;
                let source = data["path"].as_str().ok_or_else(|| Failure {
                    code: "content_unavailable".into(),
                    message: "attachment is not fully available offline".into(),
                })?;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(output)?;
                io::copy(&mut std::fs::File::open(source)?, &mut file)?;
                file.sync_all()?;
            }
            reply
        }
        Command::Call {
            method,
            params_file,
        } => {
            let params = match params_file {
                Some(path) => serde_json::from_slice(&read_input(path, 512 * 1024)?)
                    .map_err(|_| invalid("params must be valid JSON"))?,
                None => json!({}),
            };
            client.call(method, scope, params).await?
        }
        Command::Watch { seconds } => {
            let start = tokio::time::Instant::now();
            let mut revision = Value::Null;
            loop {
                let reply = client.call("status", None, json!({})).await?;
                let data = result(reply.clone())?;
                if data["revision"] != revision {
                    revision = data["revision"].clone();
                    emit(args, &reply)?;
                }
                if seconds.is_some_and(|s| start.elapsed() >= Duration::from_secs(s)) {
                    break;
                }
                tokio::select! {_=tokio::signal::ctrl_c()=>break,_=tokio::time::sleep(Duration::from_millis(500))=>{}}
            }
            return Ok(());
        }
    };
    result(reply.clone())?;
    emit(args, &reply)
}

fn emit(args: &Args, reply: &Value) -> Result<(), Failure> {
    let mut out = io::stdout().lock();
    let result = &reply["result"];
    let write = if args.json {
        writeln!(out, "{reply}")
    } else if let Some(accounts) = result["accounts"].as_array() {
        for a in accounts {
            writeln!(
                out,
                "{}  {} <{}>  {} unread · {} starred",
                a["id"].as_str().unwrap_or(""),
                a["name"].as_str().unwrap_or(""),
                a["email"].as_str().unwrap_or(""),
                a["unread_count"],
                a["starred_count"]
            )?;
        }
        Ok(())
    } else if let Some(rows) = result["conversations"].as_array() {
        for row in rows {
            writeln!(
                out,
                "{}  {}{}  {} · {}",
                row["id"].as_str().unwrap_or(""),
                if row["unread"] == true { "●" } else { " " },
                if row["starred"] == true { "★" } else { " " },
                row["subject"].as_str().unwrap_or(""),
                row["sender_name"].as_str().unwrap_or("")
            )?;
        }
        if !result["next_offset"].is_null() {
            writeln!(out, "More: --offset {}", result["next_offset"])?;
        }
        Ok(())
    } else if let Some(messages) = result["messages"].as_array() {
        writeln!(out, "{}\n", result["subject"].as_str().unwrap_or(""))?;
        for msg in messages {
            writeln!(
                out,
                "{} <{}> · {}\n{}\n",
                msg["sender_name"].as_str().unwrap_or(""),
                msg["sender_email"].as_str().unwrap_or(""),
                msg["sent_at"].as_str().unwrap_or(""),
                msg["text"].as_str().unwrap_or("")
            )?;
        }
        Ok(())
    } else {
        writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(result).unwrap_or_else(|_| "{}".into())
        )
    };
    match write {
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        other => other.map_err(Failure::from),
    }
}
