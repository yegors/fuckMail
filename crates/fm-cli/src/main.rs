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
    after_help = "The initial profile is synthetic. No Google connection or sending is performed.\nUse --json for versioned responses. fm never opens the mail database directly."
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
    /// Read a half-open date range from the selected account's calendar.
    Calendar {
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
        #[arg(long,default_value_t=30,value_parser=clap::value_parser!(u32).range(1..=100))]
        limit: u32,
        #[arg(long, default_value_t = 0)]
        offset: u64,
    },
    Read {
        id: String,
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
enum Draft {
    /// Serialize a saved draft into a local .eml capture. Never uses the network.
    Capture {
        conversation_id: String,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        request_id: String,
        #[arg(long, default_value="accepted", value_parser=["accepted","unknown"])]
        outcome: String,
    },
    Show {
        conversation_id: String,
    },
    Save {
        conversation_id: String,
        #[arg(long)]
        body_file: PathBuf,
        #[arg(long)]
        expected_revision: Option<u64>,
        #[arg(long)]
        request_id: Option<String>,
    },
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
            "stale_revision" => 4,
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
        Command::Accounts | Command::Status | Command::Watch { .. } | Command::Call { .. }
    ) {
        account(args)?;
    }
    let mut client = Client::connect(&profile, engine.as_deref()).await?;
    let scope = args.account.as_deref();
    let reply = match &args.command {
        Command::Accounts => client.call("accounts", None, json!({})).await?,
        Command::Status => client.call("status", None, json!({})).await?,
        Command::Categories => client.call("categories", scope, json!({})).await?,
        Command::Mail { command } => match command {
            Mail::List {
                query,
                field,
                folder,
                category,
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
                client.call("conversations", scope, params).await?
            }
            Mail::Read { id } => client.call("conversation", scope, json!({"id":id})).await?,
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
            Draft::Capture { conversation_id, expected_revision, request_id, outcome } => {
                client.call("draft_capture", scope, json!({"conversation_id":conversation_id,"expected_revision":expected_revision,"request_id":request_id,"outcome":outcome})).await?
            },
            Draft::Show { conversation_id } => {
                client
                    .call(
                        "draft_get",
                        scope,
                        json!({"conversation_id":conversation_id}),
                    )
                    .await?
            }
            Draft::Save {
                conversation_id,
                body_file,
                expected_revision,
                request_id,
            } => {
                let body = String::from_utf8(read_input(body_file, 256 * 1024)?)
                    .map_err(|_| invalid("draft must be UTF-8 text"))?;
                let current = result(
                    client
                        .call(
                            "draft_get",
                            scope,
                            json!({"conversation_id":conversation_id}),
                        )
                        .await?,
                )?;
                client.call("draft_save",scope,json!({"conversation_id":conversation_id,"body":body,"expected_revision":expected_revision.map(Value::from).unwrap_or(current["revision"].clone()),"request_id":request_id.clone().unwrap_or_else(fm_ipc::random_token)})).await?
            }
        },
        Command::Calendar { from, to } => {
            let mut params = json!({});
            if let Some(from) = from {
                params["from"] = json!(from);
            }
            if let Some(to) = to {
                params["to"] = json!(to);
            }
            client.call("calendar", scope, params).await?
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
