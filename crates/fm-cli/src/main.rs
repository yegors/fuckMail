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
    after_help = "The default profile is an empty live profile shared with the GUI. Use --demo-profile only for synthetic examples.\nGoogle sign-in starts only with an explicit google connect or google restore command and --profile. fm never opens the mail database directly."
)]
struct Args {
    #[arg(long, global = true)]
    profile: Option<PathBuf>,
    /// Explicitly open/create synthetic demo content in a separate profile.
    #[arg(long, global = true)]
    demo_profile: bool,
    #[arg(long, global = true)]
    engine: Option<PathBuf>,
    #[arg(long, global = true)]
    account: Option<String>,
    #[arg(long, global = true)]
    json: bool,
    #[cfg(feature = "verification-google")]
    #[arg(long, global = true, hide = true)]
    verification_google_auth: bool,
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
    Accounts {
        /// Include retained read-only archives in this cached list.
        #[arg(long)]
        include_removed: bool,
    },
    /// Explicit local removal, retained archives and credential-cleanup status.
    Account {
        #[command(subcommand)]
        command: AccountCommand,
    },
    /// Engine/schema/fixture status (starts an explicit temporary session).
    Status,
    /// Read a bounded cursor of newly acquired incoming mail; starts at the current cache end.
    Notifications {
        #[arg(long, requires = "since", value_parser = clap::value_parser!(i64).range(0..))]
        after: Option<i64>,
        #[arg(long, requires = "after")]
        since: Option<String>,
        #[arg(long)]
        now: Option<String>,
    },
    /// Bounded conversation reading, searching and explicit user actions.
    Mail {
        #[command(subcommand)]
        command: Mail,
    },
    /// Local authoring and retained Google draft sources; Google Send is explicit.
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
    #[command(args_conflicts_with_subcommands = true)]
    Calendar {
        #[arg(long)]
        combined: bool,
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
        #[command(subcommand)]
        command: Option<CalendarCommand>,
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
enum CalendarCommand {
    /// List current selected, permitted Soon/Now reminders without acknowledging them.
    Reminders {
        #[arg(long)]
        now: Option<String>,
    },
    /// Record display/dismissal of an exact currently permitted occurrence; no provider write.
    ReminderAck {
        #[arg(long)]
        calendar_id: String,
        #[arg(long)]
        occurrence_id: String,
        #[arg(long)]
        start: String,
        #[arg(long, value_parser = ["shown", "dismissed"])]
        state: String,
        #[arg(long)]
        now: Option<String>,
    },
}

#[derive(Subcommand)]
enum AccountCommand {
    /// Inspect retained work and the current lifecycle revision before removal.
    RemovePreview,
    /// Remove from active views and clear local credentials; keep a read-only archive.
    Remove {
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        expected_revision: i64,
        #[arg(long)]
        request_id: String,
        #[arg(long, default_value = "archive", value_parser = ["archive"])]
        retention: String,
    },
    /// Inspect a retained removal receipt and current credential-cleanup state.
    RemovalStatus {
        #[arg(long)]
        request_id: String,
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
    /// Restore the selected archive by signing in as the same Google identity.
    /// Retained work and sync do not start automatically.
    Restore {
        #[arg(long)]
        client_config: PathBuf,
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        expected_revision: i64,
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
    /// Execute one explicit conditional Calendar operation from JSON; requires an active sync session.
    CalendarWrite {
        #[arg(long)]
        params_file: PathBuf,
    },
    /// Read back an uncertain Calendar operation without submitting it again.
    CalendarReconcile { request_id: String },
    /// Resolve one recurring occurrence for editing; requires an active owned Google session.
    /// Reads Google without creating a mutation or starting acquisition.
    CalendarOccurrence {
        #[arg(long)]
        params_file: PathBuf,
    },
    /// Inspect one retained Calendar outcome without provider access.
    CalendarOperation { request_id: String },
    /// Resume an operation that has never dispatched; requires an active sync session.
    CalendarResume { request_id: String },
    /// Abandon retained local intent; does not undo a Google change or contact Google.
    CalendarAbandon {
        request_id: String,
        #[arg(long)]
        expected_revision: u64,
    },
    /// Synchronize one saved draft through an already active owned Google session.
    /// Unknown outcomes are inspected without automatically repeating creation.
    DraftSync {
        #[arg(long)]
        draft_id: String,
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        expected_revision: i64,
        #[arg(long)]
        request_id: String,
    },
    /// Inspect the retained Google draft state without contacting Google.
    DraftStatus {
        #[arg(long)]
        draft_id: String,
    },
    /// Read the current Google draft without repeating a create or update.
    DraftReconcile {
        #[arg(long)]
        draft_id: String,
    },
    /// Adopt a fully supported retained Google draft after a fresh source check.
    /// Opening the adopted editor alone does not upload it.
    DraftAdopt {
        #[arg(long)]
        draft_id: String,
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        source_revision: i64,
        #[arg(long)]
        request_id: String,
    },
    /// Permanently delete the exact observed Google draft; retained local sources remain.
    /// Requires an owned sync session. Unknown results are never automatically retried.
    DraftDelete {
        #[arg(long)]
        draft_id: String,
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        source_revision: i64,
        #[arg(long)]
        request_id: String,
    },
    /// Inspect one retained deletion receipt without contacting Google.
    DraftDeleteStatus {
        #[arg(long)]
        request_id: String,
    },
    /// Check an uncertain deletion without repeating it. Absence alone is not success.
    DraftDeleteReconcile {
        #[arg(long)]
        request_id: String,
    },
    /// Abandon only a proven-undispatched deletion at its current receipt revision.
    DraftDeleteAbandon {
        #[arg(long)]
        request_id: String,
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        expected_revision: i64,
    },
    /// Explicitly replace a known remote conflict with the exact saved local revision.
    /// Another client can still edit between the provider read and replacement.
    DraftReplace {
        #[arg(long)]
        draft_id: String,
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        expected_revision: i64,
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        expected_google_revision: i64,
        #[arg(long)]
        request_id: String,
    },
    /// Explicitly submit one exact saved revision through an active Google session.
    /// Unknown outcomes never trigger another automatic submission.
    Send {
        #[arg(long)]
        draft_id: String,
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        expected_revision: i64,
        #[arg(long, value_parser=clap::value_parser!(i64).range(0..))]
        expected_google_revision: i64,
        #[arg(long)]
        request_id: String,
    },
    /// Inspect one retained submission outcome without contacting Google.
    SendStatus {
        #[arg(long)]
        request_id: String,
    },
    /// Inspect bounded Sent evidence without submitting the message again.
    SendReconcile {
        #[arg(long)]
        request_id: String,
    },
    /// Abandon only a proven-undispatched prepared submission at its current revision.
    SendAbandon {
        #[arg(long)]
        request_id: String,
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        expected_revision: i64,
    },
    /// Browse cached account labels; pagination is bound to the catalog revision.
    Labels {
        #[arg(long,default_value_t=100,value_parser=clap::value_parser!(u32).range(1..=100))]
        limit: u32,
        #[arg(long, requires = "expected_catalog_revision")]
        after: Option<String>,
        #[arg(long)]
        expected_catalog_revision: Option<u64>,
    },
    /// Refresh the label catalog through the currently active owned sync runtime.
    LabelsRefresh,
    /// Begin or resume remote Drafts inventory through an already owned sync session.
    DraftsRefresh,
    /// Read cached remote Drafts coverage without contacting Google.
    DraftsStatus,
    /// Bind an existing Google label to a local category; never creates provider labels.
    CategoryBind {
        category_id: String,
        provider_label_id: String,
        #[arg(long)]
        expected_catalog_revision: u64,
        #[arg(long)]
        request_id: String,
        #[arg(long,conflicts_with_all=["expected_provider_label_id","unmapped"],required_unless_present_any=["expected_provider_label_id","unmapped"])]
        create_name: Option<String>,
        #[arg(long,conflicts_with_all=["create_name","unmapped"],required_unless_present_any=["create_name","unmapped"])]
        expected_provider_label_id: Option<String>,
        #[arg(long,conflicts_with_all=["create_name","expected_provider_label_id"],required_unless_present_any=["create_name","expected_provider_label_id"])]
        unmapped: bool,
    },
}

#[derive(Subcommand)]
enum Mail {
    /// Archive, move to Trash or restore the currently loaded conversation members.
    Filing {
        id: String,
        #[arg(long,value_parser=["archive","trash","restore"])]
        action: String,
        #[arg(long)]
        expected_revision: Option<u64>,
        #[arg(long)]
        request_id: Option<String>,
    },
    /// Inspect pending, uncertain and failed Google state changes without provider access.
    Delivery {
        #[arg(long)]
        request_id: Option<String>,
    },
    List {
        #[arg(long)]
        query: Option<String>,
        #[arg(long,default_value="all",value_parser=["all","sender","subject","body"])]
        field: String,
        #[arg(long,default_value="inbox",value_parser=["inbox","sent","all","trash"])]
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
    /// Inspect exact retained-source conversion and editing eligibility without Google access.
    Authoring {
        #[arg(long)]
        draft_id: String,
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        source_revision: i64,
    },
    /// Create a separate plain-text draft preserving validated recipients and attachment files.
    /// Uses primary From, omits HTML/inline placement/reply linkage and adds no signature.
    /// The copy may sync automatically while account sync is running; the original is retained.
    Copy {
        #[arg(long)]
        draft_id: String,
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        source_revision: i64,
        #[arg(long)]
        request_id: String,
        #[arg(long, value_parser = ["primary"])]
        from_policy: String,
        #[arg(long, value_parser = ["plain_text"])]
        content_policy: String,
        #[arg(long, value_parser = ["attachments"])]
        inline_policy: String,
        #[arg(long, value_parser = ["omit"])]
        thread_policy: String,
    },
    /// Browse retained Google source versions without contacting Google.
    Sources {
        #[arg(long)]
        draft_id: String,
        #[arg(long, default_value_t=30, value_parser=clap::value_parser!(u32).range(1..=100))]
        limit: u32,
        #[arg(long, default_value_t=0, value_parser=clap::value_parser!(u64).range(0..=10_000_000))]
        offset: u64,
    },
    /// Export exact retained Google RAW or an attachment at the specified source revision.
    /// Requires an explicit profile and account, but no active Google sync session.
    Source {
        #[arg(long)]
        draft_id: String,
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        source_revision: i64,
        #[arg(long)]
        attachment_id: Option<String>,
        /// Copy the verified bytes to a new file; never overwrites an existing file.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Browse durable drafts in the selected account.
    List {
        #[arg(long)]
        query: Option<String>,
        #[arg(long,default_value_t=30,value_parser=clap::value_parser!(u32).range(1..=100))]
        limit: u32,
        #[arg(long, default_value_t = 0)]
        offset: u64,
    },
    /// Create local human composition; Google submission is a separate explicit action.
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
    /// Retain a local file and select it on this saved draft. Never sends or uploads.
    Attach {
        #[arg(long)]
        draft_id: String,
        /// Local file path; matching replay works after the original file is removed.
        #[arg(long)]
        file: PathBuf,
        #[arg(long, value_parser=clap::value_parser!(u64).range(1..=i64::MAX as u64))]
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
    recovery: RecoveryProofs,
}

/// Only these affirmative engine proofs cross the CLI error boundary. A missing
/// or false flag proves nothing; unrelated detail may contain private content.
#[derive(Debug, Default)]
struct RecoveryProofs([bool; 4]);
impl RecoveryProofs {
    const FIELDS: [&'static str; 4] = [
        "send_invocation_finished_before_dispatch",
        "send_request_obsolete_without_dispatch",
        "draft_authoring_invocation_finished_without_commit",
        "draft_delete_invocation_finished_before_dispatch",
    ];

    fn from_details(details: &Value) -> Self {
        Self(Self::FIELDS.map(|field| details.get(field).and_then(Value::as_bool) == Some(true)))
    }

    fn details(&self) -> Option<Value> {
        let fields: serde_json::Map<String, Value> = Self::FIELDS
            .into_iter()
            .zip(self.0)
            .filter(|(_, present)| *present)
            .map(|(field, _)| (field.into(), Value::Bool(true)))
            .collect();
        (!fields.is_empty()).then_some(Value::Object(fields))
    }
}
impl Failure {
    fn json_reply(&self) -> Value {
        let mut reply = fm_ipc::error("cli", &self.code, &self.message);
        if let Some(details) = self.recovery.details() {
            reply["error"]["details"] = details;
        }
        reply
    }
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
            recovery: RecoveryProofs::default(),
        }
    }
}
fn invalid(msg: &str) -> Failure {
    Failure {
        code: "invalid_request".into(),
        message: msg.into(),
        recovery: RecoveryProofs::default(),
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
            recovery: RecoveryProofs::from_details(&reply["error"]["details"]),
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

fn attachment_import_params(command: &Command) -> Result<Option<Value>, Failure> {
    let Command::Draft {
        command:
            Draft::Attach {
                draft_id,
                file,
                expected_revision,
                request_id,
            },
    } = command
    else {
        return Ok(None);
    };
    // Lexical absolute-path resolution does not open or canonicalize the file.
    // The engine must resolve a matching durable receipt before source access.
    let path =
        std::path::absolute(file).map_err(|_| invalid("Attachment path cannot be resolved"))?;
    let path = path
        .to_str()
        .ok_or_else(|| invalid("Attachment path cannot be represented as UTF-8"))?;
    Ok(Some(json!({
        "draft_id":draft_id,"expected_revision":expected_revision,
        "request_id":request_id,"path":path
    })))
}

fn google_composition_request(command: &Command) -> Result<Option<(&'static str, Value)>, Failure> {
    let Command::Google { command } = command else {
        return Ok(None);
    };
    let (method, params) = match command {
        Google::DraftSync {
            draft_id,
            expected_revision,
            request_id,
        } => (
            "google_draft_sync",
            json!({"draft_id":draft_id,"expected_revision":expected_revision,"request_id":request_id}),
        ),
        Google::DraftStatus { draft_id } => ("google_draft_status", json!({"draft_id":draft_id})),
        Google::DraftReconcile { draft_id } => {
            ("google_draft_reconcile", json!({"draft_id":draft_id}))
        }
        Google::DraftAdopt {
            draft_id,
            source_revision,
            request_id,
        } => (
            "google_draft_adopt",
            json!({"draft_id":draft_id,"source_revision":source_revision,"request_id":request_id}),
        ),
        Google::DraftDelete {
            draft_id,
            source_revision,
            request_id,
        } => (
            "google_draft_delete",
            json!({"draft_id":draft_id,"source_revision":source_revision,"request_id":request_id}),
        ),
        Google::DraftDeleteStatus { request_id } => (
            "google_draft_delete_status",
            json!({"request_id":request_id}),
        ),
        Google::DraftDeleteReconcile { request_id } => (
            "google_draft_delete_reconcile",
            json!({"request_id":request_id}),
        ),
        Google::DraftDeleteAbandon {
            request_id,
            expected_revision,
        } => (
            "google_draft_delete_abandon",
            json!({"request_id":request_id,"expected_revision":expected_revision}),
        ),
        Google::DraftReplace {
            draft_id,
            expected_revision,
            expected_google_revision,
            request_id,
        } => (
            "google_draft_resolve",
            json!({"draft_id":draft_id,"expected_revision":expected_revision,
                "expected_google_revision":expected_google_revision,"request_id":request_id,
                "action":"replace_local"}),
        ),
        Google::Send {
            draft_id,
            expected_revision,
            expected_google_revision,
            request_id,
        } => (
            "google_send",
            json!({"draft_id":draft_id,"expected_revision":expected_revision,
                "expected_google_revision":expected_google_revision,"request_id":request_id}),
        ),
        Google::SendStatus { request_id } => {
            ("google_send_status", json!({"request_id":request_id}))
        }
        Google::SendReconcile { request_id } => {
            ("google_send_reconcile", json!({"request_id":request_id}))
        }
        Google::SendAbandon {
            request_id,
            expected_revision,
        } => (
            "google_send_abandon",
            json!({"request_id":request_id,"expected_revision":expected_revision}),
        ),
        _ => return Ok(None),
    };
    for (field, limit) in [("draft_id", 256), ("request_id", 128)] {
        if let Some(value) = params.get(field).and_then(Value::as_str) {
            if value.is_empty() || value.len() > limit || value.chars().any(char::is_control) {
                return Err(invalid(
                    "Choose a nonempty bounded draft and request identity",
                ));
            }
        }
    }
    Ok(Some((method, params)))
}

fn draft_source_request(command: &Command) -> Result<Option<(&'static str, Value)>, Failure> {
    let (method, params) = match command {
        Command::Draft {
            command:
                Draft::Authoring {
                    draft_id,
                    source_revision,
                },
        } => (
            "google_draft_authoring",
            json!({"draft_id":draft_id,"source_revision":source_revision}),
        ),
        Command::Draft {
            command:
                Draft::Copy {
                    draft_id,
                    source_revision,
                    request_id,
                    from_policy,
                    content_policy,
                    inline_policy,
                    thread_policy,
                },
        } => (
            "google_draft_copy",
            json!({"draft_id":draft_id,"source_revision":source_revision,"request_id":request_id,
                "from_policy":from_policy,"content_policy":content_policy,
                "inline_policy":inline_policy,"thread_policy":thread_policy}),
        ),
        Command::Draft {
            command:
                Draft::Sources {
                    draft_id,
                    limit,
                    offset,
                },
        } => (
            "google_draft_sources",
            json!({"draft_id":draft_id,"limit":limit,"offset":offset}),
        ),
        Command::Draft {
            command:
                Draft::Source {
                    draft_id,
                    source_revision,
                    attachment_id,
                    ..
                },
        } => {
            let mut params = json!({"draft_id":draft_id,"source_revision":source_revision});
            if let Some(id) = attachment_id {
                params["attachment_id"] = json!(id);
            }
            ("google_draft_source", params)
        }
        _ => return Ok(None),
    };
    for (field, limit) in [
        ("draft_id", 256),
        ("attachment_id", 256),
        ("request_id", 128),
    ] {
        if let Some(value) = params.get(field).and_then(Value::as_str) {
            if value.is_empty() || value.len() > limit || value.chars().any(char::is_control) {
                return Err(invalid(
                    "Choose a nonempty bounded draft, attachment and request identity",
                ));
            }
        }
    }
    Ok(Some((method, params)))
}

fn copy_export(reply: &Value, output: &Path) -> Result<(), Failure> {
    let data = result(reply.clone())?;
    let source = data["path"].as_str().ok_or_else(|| Failure {
        code: "content_unavailable".into(),
        message: "Retained content is not fully available offline".into(),
        recovery: RecoveryProofs::default(),
    })?;
    let mut source = std::fs::File::open(source)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    io::copy(&mut source, &mut file)?;
    file.sync_all()?;
    Ok(())
}

fn account_lifecycle_request(command: &Command) -> Result<Option<(&'static str, Value)>, Failure> {
    let Command::Account { command } = command else {
        return Ok(None);
    };
    let (method, params) = match command {
        AccountCommand::RemovePreview => ("account_remove_preview", json!({})),
        AccountCommand::Remove {
            expected_revision,
            request_id,
            retention,
        } => (
            "account_remove",
            json!({"expected_revision":expected_revision,"request_id":request_id,"retention":retention}),
        ),
        AccountCommand::RemovalStatus { request_id } => {
            ("account_remove_status", json!({"request_id":request_id}))
        }
    };
    if params.get("request_id").is_some_and(|value| {
        value
            .as_str()
            .is_none_or(|id| id.is_empty() || id.len() > 128 || id.chars().any(char::is_control))
    }) {
        return Err(invalid(
            "Choose a nonempty bounded removal request identity",
        ));
    }
    Ok(Some((method, params)))
}

enum BrowserLaunch {
    System,
    #[cfg(feature = "verification-google")]
    VerifiedFixture,
}

fn open_google_browser(url: &str, launch: BrowserLaunch) -> Result<(), Failure> {
    // The engine supplies one fixed HTTPS authorization endpoint. No shell
    // evaluates the query, and URLs/state are never copied to terminal logs.
    if !url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?")
        || url.len() > 16384
        || url.chars().any(char::is_control)
        || url.contains('#')
    {
        return Err(invalid("The engine returned an invalid Google sign-in URL"));
    }
    match launch {
        BrowserLaunch::System => {}
        #[cfg(feature = "verification-google")]
        BrowserLaunch::VerifiedFixture => return Ok(()),
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
            let _ = writeln!(io::stdout(), "{}", e.json_reply());
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

fn notification_request(command: &Command) -> Result<Option<(&'static str, Value)>, Failure> {
    let (method, mut params, now) = match command {
        Command::Notifications { after, since, now } => {
            let mut params = json!({});
            if let Some(after) = after {
                params["after"] = json!(after);
            }
            if let Some(since) = since {
                params["since"] = json!(since);
            }
            ("mail_notifications", params, now)
        }
        Command::Calendar {
            command: Some(CalendarCommand::Reminders { now }),
            ..
        } => ("calendar_reminders", json!({}), now),
        Command::Calendar {
            command:
                Some(CalendarCommand::ReminderAck {
                    calendar_id,
                    occurrence_id,
                    start,
                    state,
                    now,
                }),
            ..
        } => (
            "calendar_reminder_ack",
            json!({"calendar_id":calendar_id,"occurrence_id":occurrence_id,"start":start,"state":state}),
            now,
        ),
        _ => return Ok(None),
    };
    if let Some(now) = now {
        params["now"] = json!(now);
    }
    for (key, max) in [
        ("calendar_id", 1024),
        ("occurrence_id", 8192),
        ("start", 64),
        ("since", 64),
        ("now", 64),
    ] {
        if params.get(key).is_some_and(|v| {
            v.as_str()
                .is_none_or(|v| v.is_empty() || v.len() > max || v.chars().any(char::is_control))
        }) {
            return Err(invalid(
                "Notification identities and timestamps must be bounded text",
            ));
        }
    }
    Ok(Some((method, params)))
}

async fn run(args: &Args) -> Result<(), Failure> {
    #[cfg(feature = "verification-google")]
    if args.verification_google_auth
        && (args.profile.is_none()
            || args.engine.is_none()
            || !matches!(
                args.command,
                Command::Google {
                    command: Google::Connect { .. } | Google::Restore { .. }
                }
            ))
    {
        return Err(invalid("Synthetic sign-in requires explicit profile/engine paths and a Google connect or restore command"));
    }
    let profile = match &args.profile {
        Some(p) => p.to_owned(),
        None if args.demo_profile => fm_ipc::demo_profile()?,
        None => fm_ipc::default_profile()?,
    };
    let engine = args
        .engine
        .clone()
        .or_else(|| std::env::var_os("FM_ENGINE_PATH").map(PathBuf::from));
    // Validate scope and local input before starting any engine work.
    if !matches!(
        args.command,
        Command::Accounts { .. }
            | Command::Status
            | Command::Notifications { .. }
            | Command::Watch { .. }
            | Command::Call { .. }
            | Command::Preferences { .. }
            | Command::CalendarSources
            | Command::Calendar { combined: true, .. }
            | Command::Calendar {
                command: Some(CalendarCommand::Reminders { .. }),
                ..
            }
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
    let live_command = matches!(
        args.command,
        Command::ProfileInit
            | Command::Google { .. }
            | Command::Account { .. }
            | Command::Draft {
                command: Draft::Source { .. }
                    | Draft::Sources { .. }
                    | Draft::Authoring { .. }
                    | Draft::Copy { .. }
            }
    );
    if live_command && args.profile.is_none() {
        return Err(invalid(
            "profile-init, account, Google and retained Google draft commands require an explicit --profile path",
        ));
    }
    if live_command && args.demo_profile {
        return Err(invalid(
            "Google and account commands cannot use --demo-profile",
        ));
    }
    let google_client_path = if let Command::Google {
        command: Google::Connect { client_config, .. } | Google::Restore { client_config, .. },
    } = &args.command
    {
        Some(
            std::fs::canonicalize(client_config)
                .map_err(|_| invalid("Choose an existing desktop client JSON file"))?,
        )
    } else {
        None
    };
    let google_calendar_input = match &args.command {
        Command::Google {
            command: Google::CalendarWrite { params_file },
        } => Some((params_file, 512 * 1024, "Calendar write")),
        Command::Google {
            command: Google::CalendarOccurrence { params_file },
        } => Some((params_file, 16 * 1024, "Calendar occurrence lookup")),
        _ => None,
    };
    let google_calendar_request = if let Some((params_file, limit, label)) = google_calendar_input {
        let value: Value = serde_json::from_slice(&read_input(params_file, limit)?)
            .map_err(|_| invalid(&format!("{label} must be JSON")))?;
        if !value.is_object() {
            return Err(invalid(&format!("{label} must be a JSON object")));
        }
        Some(value)
    } else {
        None
    };
    let attachment_import = attachment_import_params(&args.command)?;
    let google_composition = google_composition_request(&args.command)?;
    let draft_source = draft_source_request(&args.command)?;
    let account_lifecycle = account_lifecycle_request(&args.command)?;
    let notification = notification_request(&args.command)?;
    let mut client = if args.demo_profile {
        Client::connect_demo(&profile, engine.as_deref()).await?
    } else if live_command || args.profile.is_none() {
        Client::connect_live(&profile, engine.as_deref()).await?
    } else {
        Client::connect(&profile, engine.as_deref()).await?
    };
    let scope = args.account.as_deref();
    #[cfg(not(feature = "verification-google"))]
    let browser_launch = BrowserLaunch::System;
    #[cfg(feature = "verification-google")]
    let browser_launch = if args.verification_google_auth {
        let fixture = result(
            client
                .call("verification_google_auth_fixture", None, json!({}))
                .await?,
        )?;
        if fixture != json!({"mode":"accounts-v1","browser_stub_allowed":true}) {
            return Err(invalid(
                "The engine did not confirm the isolated account fixture",
            ));
        }
        BrowserLaunch::VerifiedFixture
    } else {
        BrowserLaunch::System
    };
    let reply = match &args.command {
        Command::Accounts { include_removed } => {
            let params = if *include_removed {
                json!({"include_removed":true})
            } else {
                json!({})
            };
            client.call("accounts", None, params).await?
        }
        Command::Account { .. } => {
            let (method, params) =
                account_lifecycle.ok_or_else(|| invalid("Missing account lifecycle request"))?;
            client.call(method, scope, params).await?
        }
        Command::Status | Command::ProfileInit => client.call("status", None, json!({})).await?,
        Command::Notifications { .. }
        | Command::Calendar {
            command: Some(_), ..
        } => {
            let (method, params) =
                notification.expect("notification request validated before connection");
            let account = if method == "calendar_reminder_ack" {
                scope
            } else {
                None
            };
            client.call(method, account, params).await?
        }
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
            command: Google::DraftsRefresh,
        } => {
            client
                .call("google_drafts_refresh", scope, json!({}))
                .await?
        }
        Command::Google {
            command: Google::DraftsStatus,
        } => {
            client
                .call("google_drafts_status", scope, json!({}))
                .await?
        }
        Command::Google {
            command:
                Google::DraftSync { .. }
                | Google::DraftStatus { .. }
                | Google::DraftReconcile { .. }
                | Google::DraftAdopt { .. }
                | Google::DraftDelete { .. }
                | Google::DraftDeleteStatus { .. }
                | Google::DraftDeleteReconcile { .. }
                | Google::DraftDeleteAbandon { .. }
                | Google::DraftReplace { .. }
                | Google::Send { .. }
                | Google::SendStatus { .. }
                | Google::SendReconcile { .. }
                | Google::SendAbandon { .. },
        } => {
            let (method, params) = google_composition
                .as_ref()
                .expect("Google composition request validated before connection");
            client.call(method, scope, params.clone()).await?
        }
        Command::Google {
            command:
                Google::Labels {
                    limit,
                    after,
                    expected_catalog_revision,
                },
        } => {
            let mut params = json!({"limit":limit});
            if let Some(after) = after {
                params["after"] = json!(after);
            }
            if let Some(revision) = expected_catalog_revision {
                params["expected_catalog_revision"] = json!(revision);
            }
            client.call("google_labels", scope, params).await?
        }
        Command::Google {
            command: Google::LabelsRefresh,
        } => {
            client
                .call("google_labels_refresh", scope, json!({}))
                .await?
        }
        Command::Google {
            command:
                Google::CategoryBind {
                    category_id,
                    provider_label_id,
                    expected_catalog_revision,
                    request_id,
                    create_name,
                    expected_provider_label_id,
                    unmapped,
                },
        } => {
            let mut params = json!({"category_id":category_id,"provider_label_id":provider_label_id,"expected_catalog_revision":expected_catalog_revision,"request_id":request_id});
            if let Some(name) = create_name {
                params["create_name"] = json!(name);
            } else if *unmapped {
                params["expected_provider_label_id"] = Value::Null;
            } else {
                params["expected_provider_label_id"] = json!(expected_provider_label_id);
            }
            client.call("google_category_bind", scope, params).await?
        }
        Command::Google {
            command: Google::CalendarResume { request_id },
        } => {
            client
                .call(
                    "google_calendar_resume",
                    scope,
                    json!({"request_id":request_id}),
                )
                .await?
        }
        Command::Google {
            command:
                Google::CalendarAbandon {
                    request_id,
                    expected_revision,
                },
        } => {
            client
                .call(
                    "google_calendar_abandon",
                    scope,
                    json!({"request_id":request_id,"expected_revision":expected_revision}),
                )
                .await?
        }
        Command::Google {
            command: Google::CalendarWrite { .. },
        } => {
            client
                .call(
                    "google_calendar_write",
                    scope,
                    google_calendar_request.unwrap(),
                )
                .await?
        }
        Command::Google {
            command: Google::CalendarReconcile { request_id },
        } => {
            client
                .call(
                    "google_calendar_reconcile",
                    scope,
                    json!({"request_id":request_id}),
                )
                .await?
        }
        Command::Google {
            command: Google::CalendarOccurrence { .. },
        } => {
            client
                .call(
                    "google_calendar_occurrence_read",
                    scope,
                    google_calendar_request.unwrap(),
                )
                .await?
        }
        Command::Google {
            command: Google::CalendarOperation { request_id },
        } => {
            client
                .call(
                    "google_calendar_operation",
                    scope,
                    json!({"request_id":request_id}),
                )
                .await?
        }
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
            command: Google::Connect { reauthorize, .. } | Google::Restore { reauthorize, .. },
        } => {
            let path = google_client_path
                .as_ref()
                .ok_or_else(|| invalid("Choose an existing desktop client JSON file"))?;
            let path = path
                .to_str()
                .ok_or_else(|| invalid("Desktop client path is not valid Unicode"))?;
            let (method, params) = match &args.command {
                Command::Google {
                    command:
                        Google::Restore {
                            expected_revision, ..
                        },
                } => (
                    "google_auth_restore_begin",
                    json!({"client_config_path":path,"reauthorize":reauthorize,"expected_revision":expected_revision}),
                ),
                _ => (
                    "google_auth_begin",
                    json!({"client_config_path":path,"reauthorize":reauthorize}),
                ),
            };
            let begin = result(client.call(method, scope, params).await?)?;
            let attempt = begin["attempt_id"]
                .as_str()
                .ok_or_else(|| invalid("Engine omitted the sign-in attempt"))?
                .to_owned();
            let url = begin["authorization_url"]
                .as_str()
                .ok_or_else(|| invalid("Engine omitted Google sign-in"))?;
            if let Err(error) = open_google_browser(url, browser_launch) {
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
                            recovery: RecoveryProofs::default(),
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
                            recovery: RecoveryProofs::default(),
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
                        recovery: RecoveryProofs::default(),
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
            Mail::Delivery { request_id } => {
                let params = match request_id {
                    Some(id) => json!({"request_id":id}),
                    None => json!({}),
                };
                client
                    .call("google_mail_mutation_status", scope, params)
                    .await?
            }
            Mail::Filing {
                id,
                action,
                expected_revision,
                request_id,
            } => {
                let snapshot = result(client.call("conversation", scope, json!({"id":id})).await?)?;
                client.call("mail_filing",scope,json!({"id":id,"action":action,"message_ids":snapshot["message_ids"],"expected_revision":expected_revision.map(Value::from).unwrap_or(snapshot["revision"].clone()),"request_id":request_id.clone().unwrap_or_else(fm_ipc::random_token)})).await?
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
            Draft::Sources { .. }
            | Draft::Source { .. }
            | Draft::Authoring { .. }
            | Draft::Copy { .. } => {
                let (method, params) = draft_source
                    .as_ref()
                    .expect("Draft source request validated before connection");
                let reply = client.call(method, scope, params.clone()).await?;
                if let Draft::Source {
                    output: Some(output),
                    ..
                } = command
                {
                    copy_export(&reply, output)?;
                }
                reply
            }
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
            Draft::Attach { .. } => {
                client
                    .call(
                        "draft_attachment_import",
                        scope,
                        attachment_import.expect("attachment request was prepared before connect"),
                    )
                    .await?
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
        Command::Calendar {
            from,
            to,
            combined,
            command: None,
        } => {
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
                copy_export(&reply, output)?;
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
    } else if result["configured"].is_boolean() && result["accounts"].is_array() {
        writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(result).unwrap_or_else(|_| "{}".into())
        )
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
        if let Some(delivery) = result["delivery"].as_object() {
            writeln!(
                out,
                "Google delivery: {} · request {}",
                delivery
                    .get("state")
                    .and_then(Value::as_str)
                    .unwrap_or("pending"),
                delivery
                    .get("request_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_commands_map_exact_cached_queries_and_local_receipts() {
        for (args, method, expected) in [
            (vec!["notifications"], "mail_notifications", json!({})),
            (
                vec![
                    "notifications",
                    "--after",
                    "7",
                    "--since",
                    "2026-09-14T13:00:00Z",
                ],
                "mail_notifications",
                json!({"after":7,"since":"2026-09-14T13:00:00Z"}),
            ),
            (
                vec!["calendar", "reminders"],
                "calendar_reminders",
                json!({}),
            ),
            (
                vec!["calendar", "reminders", "--now", "2026-09-14T13:00:00Z"],
                "calendar_reminders",
                json!({"now":"2026-09-14T13:00:00Z"}),
            ),
            (
                vec![
                    "calendar",
                    "reminder-ack",
                    "--calendar-id",
                    "calendar",
                    "--occurrence-id",
                    "instance",
                    "--start",
                    "2026-09-14",
                    "--state",
                    "dismissed",
                ],
                "calendar_reminder_ack",
                json!({"calendar_id":"calendar","occurrence_id":"instance","start":"2026-09-14","state":"dismissed"}),
            ),
        ] {
            let mut command = vec!["fm"];
            command.extend(args);
            let parsed = Args::try_parse_from(command).unwrap();
            assert_eq!(
                notification_request(&parsed.command).unwrap(),
                Some((method, expected))
            );
        }
        let old = Args::try_parse_from([
            "fm",
            "calendar",
            "--combined",
            "--from",
            "2026-09-14",
            "--to",
            "2026-09-15",
        ])
        .unwrap();
        assert!(notification_request(&old.command).unwrap().is_none());
        for args in [
            vec!["fm", "notifications", "--after", "1"],
            vec!["fm", "notifications", "--since", "2026-09-14T13:00:00Z"],
            vec![
                "fm",
                "notifications",
                "--after",
                "-1",
                "--since",
                "2026-09-14T13:00:00Z",
            ],
            vec!["fm", "calendar", "--combined", "reminders"],
            vec![
                "fm",
                "calendar",
                "reminder-ack",
                "--calendar-id",
                "c",
                "--occurrence-id",
                "i",
                "--start",
                "2026-09-14",
                "--state",
                "sent",
            ],
        ] {
            assert!(Args::try_parse_from(args).is_err());
        }
        let malformed =
            Args::try_parse_from(["fm", "calendar", "reminders", "--now", "bad\ntime"]).unwrap();
        assert!(notification_request(&malformed.command).is_err());
    }
    #[test]
    fn engine_recovery_proofs_survive_failure_and_cli_json_serialization() {
        for details in [
            json!({"draft_authoring_invocation_finished_without_commit":true}),
            json!({"draft_delete_invocation_finished_before_dispatch":true}),
            json!({"send_invocation_finished_before_dispatch":true}),
            json!({"send_request_obsolete_without_dispatch":true}),
            json!({"send_invocation_finished_before_dispatch":true,
                "send_request_obsolete_without_dispatch":true}),
        ] {
            let reply = json!({"version":1,"id":"engine-request","ok":false,
                "error":{"code":"stale_source_revision","message":"Saved source changed",
                    "details":details}});
            let failure = result(reply).unwrap_err();
            let encoded = serde_json::to_string(&failure.json_reply()).unwrap();
            let output: Value = serde_json::from_str(&encoded).unwrap();
            assert_eq!(
                output,
                json!({"version":1,"id":"cli","ok":false,
                "error":{"code":"stale_source_revision","message":"Saved source changed",
                    "details":details}})
            );
        }
    }

    #[test]
    fn cli_errors_keep_only_bounded_affirmative_recovery_proofs() {
        let mut details = json!({
            "draft_authoring_invocation_finished_without_commit":true,
            "draft_delete_invocation_finished_before_dispatch":false,
            "send_invocation_finished_before_dispatch":"true",
            "send_request_obsolete_without_dispatch":1,
            "nested":{"send_request_obsolete_without_dispatch":true},
            "unrelated":"private diagnostic".repeat(32768),
        });
        // Large unknown collections are neither traversed nor retained.
        for index in 0..1024 {
            details[format!("extra_{index}")] = json!([true, "unrelated"]);
        }
        let failure = result(json!({"ok":false,"error":{"code":"refused",
            "message":"Cannot complete","details":details}}))
        .unwrap_err();
        let output = failure.json_reply();
        assert_eq!(
            output["error"]["details"],
            json!({"draft_authoring_invocation_finished_without_commit":true})
        );
        assert!(serde_json::to_vec(&output).unwrap().len() < 256);

        for details in [
            Value::Null,
            json!(true),
            json!("true"),
            json!([]),
            json!({"send_invocation_finished_before_dispatch":false}),
            json!({"send_request_obsolete_without_dispatch":{"value":true}}),
        ] {
            let failure = result(json!({"ok":false,"error":{"code":"not_found",
                "message":"No recorded operation","details":details}}))
            .unwrap_err();
            assert_eq!(
                failure.json_reply(),
                fm_ipc::error("cli", "not_found", "No recorded operation")
            );
        }
        assert_eq!(
            invalid("bad input").json_reply(),
            fm_ipc::error("cli", "invalid_request", "bad input")
        );
        let failure = Failure::from(io::Error::other("disconnected"));
        assert_eq!(
            failure.json_reply(),
            fm_ipc::error("cli", "io_error", "disconnected")
        );
    }

    #[test]
    fn remote_draft_actions_keep_read_conversion_adoption_and_deletion_distinct() {
        for (args, method, expected) in [
            (
                vec![
                    "draft",
                    "authoring",
                    "--draft-id",
                    "snapshot",
                    "--source-revision",
                    "9",
                ],
                "google_draft_authoring",
                json!({"draft_id":"snapshot","source_revision":9}),
            ),
            (
                vec![
                    "draft",
                    "copy",
                    "--draft-id",
                    "snapshot",
                    "--source-revision",
                    "9",
                    "--request-id",
                    "copy-1",
                    "--from-policy",
                    "primary",
                    "--content-policy",
                    "plain_text",
                    "--inline-policy",
                    "attachments",
                    "--thread-policy",
                    "omit",
                ],
                "google_draft_copy",
                json!({"draft_id":"snapshot","source_revision":9,
                    "request_id":"copy-1","from_policy":"primary","content_policy":"plain_text",
                    "inline_policy":"attachments","thread_policy":"omit"}),
            ),
            (
                vec![
                    "google",
                    "draft-adopt",
                    "--draft-id",
                    "snapshot",
                    "--source-revision",
                    "9",
                    "--request-id",
                    "adopt-1",
                ],
                "google_draft_adopt",
                json!({"draft_id":"snapshot","source_revision":9,"request_id":"adopt-1"}),
            ),
            (
                vec![
                    "google",
                    "draft-delete",
                    "--draft-id",
                    "snapshot",
                    "--source-revision",
                    "9",
                    "--request-id",
                    "delete-1",
                ],
                "google_draft_delete",
                json!({"draft_id":"snapshot","source_revision":9,"request_id":"delete-1"}),
            ),
            (
                vec!["google", "draft-delete-status", "--request-id", "delete-1"],
                "google_draft_delete_status",
                json!({"request_id":"delete-1"}),
            ),
            (
                vec![
                    "google",
                    "draft-delete-reconcile",
                    "--request-id",
                    "delete-1",
                ],
                "google_draft_delete_reconcile",
                json!({"request_id":"delete-1"}),
            ),
            (
                vec![
                    "google",
                    "draft-delete-abandon",
                    "--request-id",
                    "delete-1",
                    "--expected-revision",
                    "2",
                ],
                "google_draft_delete_abandon",
                json!({"request_id":"delete-1","expected_revision":2}),
            ),
        ] {
            let mut input = vec!["fm", "--profile", "unused", "--account", "account"];
            input.extend(args);
            let parsed = Args::try_parse_from(input).unwrap();
            let local = draft_source_request(&parsed.command).unwrap();
            let remote = google_composition_request(&parsed.command).unwrap();
            assert_ne!(local.is_some(), remote.is_some());
            assert_eq!(local.or(remote).unwrap(), (method, expected));
        }
    }

    #[test]
    fn verified_export_preserves_exact_bytes_and_never_overwrites() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("verified.eml");
        let output = root.path().join("saved.eml");
        let bytes: Vec<u8> = (0..=255).cycle().take(4096).collect();
        std::fs::write(&source, &bytes).unwrap();
        let reply = json!({"ok":true,"result":{"path":source}});
        copy_export(&reply, &output).unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), bytes);
        std::fs::write(&source, b"new remote version").unwrap();
        assert!(copy_export(&reply, &output).is_err());
        assert_eq!(std::fs::read(&output).unwrap(), bytes);
        let refused = root.path().join("refused.eml");
        assert!(copy_export(
            &json!({"ok":false,"error":{"code":"stale_revision","message":"Changed source"}}),
            &refused
        )
        .is_err());
        assert!(!refused.exists());
        std::fs::remove_file(&source).unwrap();
        assert!(copy_export(&reply, &refused).is_err());
        assert!(!refused.exists());
    }

    #[test]
    fn attachment_request_maps_exact_fields_without_opening_relative_or_deleted_sources() {
        let root = tempfile::tempdir().unwrap();
        let deleted = root.path().join("report with spaces 日本語.bin");
        std::fs::write(&deleted, b"source is no longer needed for replay").unwrap();
        std::fs::remove_file(&deleted).unwrap();
        let relative = PathBuf::from(format!("missing-{}", fm_ipc::random_token()))
            .join("report with spaces 日本語.bin");
        let expected_relative = std::env::current_dir().unwrap().join(&relative);
        for (file, expected) in [(&relative, &expected_relative), (&deleted, &deleted)] {
            assert!(!file.exists());
            let args = Args::try_parse_from([
                "fm",
                "--account",
                "chosen-account",
                "draft",
                "attach",
                "--draft-id",
                "draft-1",
                "--file",
                file.to_str().unwrap(),
                "--expected-revision",
                "7",
                "--request-id",
                "import-1",
            ])
            .unwrap();
            assert_eq!(account(&args).unwrap(), "chosen-account");
            assert_eq!(
                attachment_import_params(&args.command).unwrap().unwrap(),
                json!({"draft_id":"draft-1","expected_revision":7,"request_id":"import-1",
                    "path":expected.to_str().unwrap()})
            );
            assert!(!file.exists());
        }
    }
}
