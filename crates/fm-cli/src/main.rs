//! `fm`, the command-line client. Each command becomes one or more requests to
//! the engine over `fm-ipc`, starting the engine when none is running, so the
//! CLI shares the GUI's profile, store and command set. Output is text for
//! people or, with `--json`, the engine's JSON. `fm` never opens the mail
//! database itself.
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
    /// Profile directory (absolute path); defaults to the shared live profile.
    #[arg(long, global = true)]
    profile: Option<PathBuf>,
    /// Explicitly open/create synthetic demo content in a separate profile.
    #[arg(long, global = true)]
    demo_profile: bool,
    /// Engine binary to start if none is running; defaults to FM_ENGINE_PATH, then fm-engine beside fm.
    #[arg(long, global = true)]
    engine: Option<PathBuf>,
    /// Account id for account-scoped commands; `fm accounts` lists them.
    #[arg(long, global = true)]
    account: Option<String>,
    /// Print each reply, including errors, as one line of JSON on stdout.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Optional local-model (Ollama or OpenAI-compatible) summaries and label/sender-kind classification.
    Assist {
        #[command(subcommand)]
        command: Assist,
    },
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
        /// Cursor returned as `after` by the previous call; needs --since.
        #[arg(long, requires = "since", value_parser = clap::value_parser!(i64).range(0..))]
        after: Option<i64>,
        /// RFC 3339 time; report only mail sent since then, looking back at most five minutes.
        #[arg(long, requires = "after")]
        since: Option<String>,
        /// Treat this RFC 3339 time as the current time.
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
        /// How far back to keep this account's mail; shortening it removes older local copies.
        #[arg(long, value_parser=["1_month","3_months","90_days","1_year","2_years","5_years","all"])]
        mail_retention: Option<String>,
        /// Mail access; supply together with --calendar-access.
        #[arg(long,value_parser=["none","read","write"],requires="calendar_access")]
        mail_access: Option<String>,
        /// Calendar access; supply together with --mail-access.
        #[arg(long,value_parser=["none","read","write"],requires="mail_access")]
        calendar_access: Option<String>,
        /// Also summarize/label mail that was already in the account when it was
        /// added (true), or leave it for `assist catch-up` (false, the default).
        #[arg(long)]
        analyze_history: Option<bool>,
        /// The account's colour, one of the 16 palette entries such as #6FB5B2.
        #[arg(long)]
        color: Option<String>,
        /// Display name, 1–80 characters.
        #[arg(long)]
        name: String,
        /// File with the signature text, or `-` for stdin; at most 16 KiB.
        #[arg(long)]
        signature_file: PathBuf,
        /// The account's `metadata_revision` from `fm accounts --json`.
        #[arg(long)]
        expected_revision: u64,
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
    },
    /// List all cached calendar sources and their permissions.
    CalendarSources,
    /// Demo profile only: create or edit a nonrecurring event, or RSVP, acknowledged locally.
    /// For real accounts use `google calendar-write`.
    CalendarWrite {
        /// JSON request file, or `-` for stdin; at most 64 KiB.
        #[arg(long)]
        params_file: PathBuf,
    },
    /// Inspect the selected account's local calendar mutation journal.
    CalendarOperations {
        /// Operations per page.
        #[arg(long, default_value_t = 30)]
        limit: u32,
        /// The `continuation` JSON object from the previous page.
        #[arg(long)]
        cursor: Option<String>,
    },
    /// Demo profile only: acknowledge a previously prepared local calendar operation.
    CalendarAck {
        /// Request ID of the prepared operation.
        request_id: String,
    },
    /// Read a half-open date range from the selected account or combined calendars.
    #[command(args_conflicts_with_subcommands = true)]
    Calendar {
        /// Combine the selected calendars of every account; --account is not needed.
        #[arg(long)]
        combined: bool,
        /// First day, as YYYY-MM-DD.
        #[arg(long)]
        from: Option<String>,
        /// Day after the last day, as YYYY-MM-DD.
        #[arg(long)]
        to: Option<String>,
        #[command(subcommand)]
        command: Option<CalendarCommand>,
    },
    /// Copy a verified retained attachment to a new file, or inspect its state.
    Attachment {
        /// Attachment id, as listed by `mail read`.
        id: String,
        /// Copy the attachment to this new file; never overwrites an existing file.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// List the selected account's labels.
    Categories,
    /// Manage account labels and their local classification instructions.
    Labels {
        #[command(subcommand)]
        command: Labels,
    },
    /// Explicit foreground session; report changes until Ctrl-C or --seconds.
    Watch {
        /// Stop after this many seconds instead of waiting for Ctrl-C.
        #[arg(long)]
        seconds: Option<u64>,
    },
    /// Link hosts across accounts, and the blocklist that links and images honour.
    Links {
        #[command(subcommand)]
        command: LinkCommand,
    },
    /// The optional HTTPS proxy every remote image request uses.
    Proxy {
        #[command(subcommand)]
        command: ProxyCommand,
    },
    /// Exercise a documented protocol method with JSON read from a file or '-'.
    Call {
        /// Engine protocol method name.
        method: String,
        /// JSON params file, or `-` for stdin; without it the params are an empty object.
        #[arg(long)]
        params_file: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum CalendarCommand {
    /// Search cached permitted occurrences in a 1–42 day range, or the next time of
    /// each matching event around a day; no Google request.
    Search {
        /// Words match titles and notes as one phrase; `title:` matches titles only;
        /// `with:` or a bare address matches guests. 1–32 words in at most 512 bytes.
        query: String,
        /// First day, as YYYY-MM-DD.
        #[arg(long, required_unless_present = "around")]
        from: Option<String>,
        /// Day after the last day, as YYYY-MM-DD.
        #[arg(long, required_unless_present = "around")]
        to: Option<String>,
        /// Instead of a range: each event's next time within a year of this day
        /// (YYYY-MM-DD), then its latest time in the 60 days before.
        #[arg(long, conflicts_with_all = ["from", "to"])]
        around: Option<String>,
        /// Maximum occurrences to return.
        #[arg(long, default_value_t = 100, value_parser = clap::value_parser!(u64).range(1..=100))]
        limit: u64,
    },
    /// List current selected, permitted Soon/Now reminders without acknowledging them.
    Reminders {
        /// Treat this RFC 3339 time as the current time.
        #[arg(long)]
        now: Option<String>,
    },
    /// Record display/dismissal of an exact currently permitted occurrence; no provider write.
    ReminderAck {
        /// Calendar id, as listed by `calendar reminders`.
        #[arg(long)]
        calendar_id: String,
        /// Occurrence `id`, as listed by `calendar reminders`.
        #[arg(long)]
        occurrence_id: String,
        /// The occurrence's `start`, exactly as listed.
        #[arg(long)]
        start: String,
        /// Whether the reminder was shown or dismissed.
        #[arg(long, value_parser = ["shown", "dismissed"])]
        state: String,
        /// Treat this RFC 3339 time as the current time.
        #[arg(long)]
        now: Option<String>,
    },
}

#[derive(Subcommand)]
enum AccountCommand {
    /// Permanently delete a removed local archive; Google data is unchanged.
    DeleteArchive {
        /// The archive's current lifecycle revision.
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        expected_revision: i64,
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
    },
    /// Inspect retained work and the current lifecycle revision before removal.
    RemovePreview,
    /// Remove from active views and clear local credentials; keep a read-only archive.
    Remove {
        /// Lifecycle revision from `account remove-preview`.
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        expected_revision: i64,
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
        /// What stays on this machine; only a read-only `archive` is supported.
        #[arg(long, default_value = "archive", value_parser = ["archive"])]
        retention: String,
    },
    /// Inspect a retained removal receipt and current credential-cleanup state.
    RemovalStatus {
        /// Request ID used for the removal.
        #[arg(long)]
        request_id: String,
    },
}

#[derive(Subcommand)]
enum Google {
    /// Check whether this build is configured for browser Google sign-in.
    SetupStatus,
    /// Open your browser and keep this CLI session until login finishes or Ctrl-C.
    /// Add --account for an identity-checked reconnect.
    Connect {
        /// Gmail access to request.
        #[arg(long, default_value="write", value_parser=["none","read","write"])]
        mail_access: String,
        /// Google Calendar access to request.
        #[arg(long, default_value="write", value_parser=["none","read","write"])]
        calendar_access: String,
        /// Gmail or Workspace email to prefill in Google's browser sign-in.
        #[arg(long)]
        email: Option<String>,
        /// Developer override; configured builds do not need this file.
        #[arg(long)]
        client_config: Option<PathBuf>,
        /// Ask Google to show its consent screen again.
        #[arg(long)]
        reauthorize: bool,
    },
    /// Restore the selected archive by signing in as the same Google identity.
    /// Retained work and sync do not start automatically.
    Restore {
        /// Gmail access to request.
        #[arg(long, default_value="write", value_parser=["none","read","write"])]
        mail_access: String,
        /// Google Calendar access to request.
        #[arg(long, default_value="write", value_parser=["none","read","write"])]
        calendar_access: String,
        /// Gmail or Workspace email to prefill in Google's browser sign-in.
        #[arg(long)]
        email: Option<String>,
        /// Developer override; configured builds do not need this file.
        #[arg(long)]
        client_config: Option<PathBuf>,
        /// The archive's current lifecycle revision.
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        expected_revision: i64,
        /// Ask Google to show its consent screen again.
        #[arg(long)]
        reauthorize: bool,
    },
    /// Stop Google access while retaining downloaded content, drafts and credentials.
    Disconnect,
    /// Download mail and Calendar while this explicit CLI session remains open.
    /// Uses built-in Google API limits. Optional developer overrides must supply all four.
    Sync {
        /// Fetch new mail immediately and retry denied credential access.
        #[arg(long)]
        refresh: bool,
        /// Developer override: Gmail quota units per minute for the whole project.
        #[arg(long, value_parser=clap::value_parser!(u32).range(100..=100_000_000))]
        gmail_project_units_per_minute: Option<u32>,
        /// Developer override: Gmail quota units per minute for each account.
        #[arg(long, value_parser=clap::value_parser!(u32).range(100..=100_000_000))]
        gmail_account_units_per_minute: Option<u32>,
        /// Developer override: Calendar quota units per minute for the whole project.
        #[arg(long, value_parser=clap::value_parser!(u32).range(100..=100_000_000))]
        calendar_project_units_per_minute: Option<u32>,
        /// Developer override: Calendar quota units per minute for each account.
        #[arg(long, value_parser=clap::value_parser!(u32).range(100..=100_000_000))]
        calendar_account_units_per_minute: Option<u32>,
        /// Stop after this many seconds instead of waiting for Ctrl-C.
        #[arg(long)]
        seconds: Option<u64>,
    },
    /// Inspect active acquisition; does not start provider requests.
    SyncStatus,
    /// Stop this account's acquisition and retain all downloaded and unsent work.
    SyncStop,
    /// Execute one explicit conditional Calendar operation from JSON; requires an active sync session.
    CalendarWrite {
        /// JSON operation file, or `-` for stdin; at most 512 KiB.
        #[arg(long)]
        params_file: PathBuf,
    },
    /// Read back an uncertain Calendar operation without submitting it again.
    CalendarReconcile {
        /// Request ID of the Calendar operation.
        request_id: String,
    },
    /// Resolve one recurring occurrence for editing; requires an active owned Google session.
    /// Reads Google without creating a mutation or starting acquisition.
    CalendarOccurrence {
        /// JSON lookup file, or `-` for stdin; at most 16 KiB.
        #[arg(long)]
        params_file: PathBuf,
    },
    /// Inspect one retained Calendar outcome without provider access.
    CalendarOperation {
        /// Request ID of the Calendar operation.
        request_id: String,
    },
    /// Resume an operation that has never dispatched; requires an active sync session.
    CalendarResume {
        /// Request ID of the Calendar operation.
        request_id: String,
    },
    /// Abandon retained local intent; does not undo a Google change or contact Google.
    CalendarAbandon {
        /// Request ID of the Calendar operation.
        request_id: String,
        /// The operation's current revision.
        #[arg(long)]
        expected_revision: u64,
    },
    /// Synchronize one saved draft through an already active owned Google session.
    /// Unknown outcomes are inspected without automatically repeating creation.
    DraftSync {
        /// Draft id, as listed by `draft list`.
        #[arg(long)]
        draft_id: String,
        /// Saved local draft revision to upload.
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        expected_revision: i64,
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
    },
    /// Inspect the retained Google draft state without contacting Google.
    DraftStatus {
        /// Draft id.
        #[arg(long)]
        draft_id: String,
    },
    /// Read the current Google draft without repeating a create or update.
    DraftReconcile {
        /// Draft id.
        #[arg(long)]
        draft_id: String,
    },
    /// Adopt a fully supported retained Google draft after a fresh source check.
    /// Opening the adopted editor alone does not upload it.
    DraftAdopt {
        /// Draft id.
        #[arg(long)]
        draft_id: String,
        /// Retained source revision, as listed by `draft sources`.
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        source_revision: i64,
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
    },
    /// Permanently delete the exact observed Google draft; retained local sources remain.
    /// Requires an owned sync session. Unknown results are never automatically retried.
    DraftDelete {
        /// Draft id.
        #[arg(long)]
        draft_id: String,
        /// Retained source revision, as listed by `draft sources`.
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        source_revision: i64,
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
    },
    /// Inspect one retained deletion receipt without contacting Google.
    DraftDeleteStatus {
        /// Request ID used for the deletion.
        #[arg(long)]
        request_id: String,
    },
    /// Check an uncertain deletion without repeating it. Absence alone is not success.
    DraftDeleteReconcile {
        /// Request ID used for the deletion.
        #[arg(long)]
        request_id: String,
    },
    /// Abandon only a proven-undispatched deletion at its current receipt revision.
    DraftDeleteAbandon {
        /// Request ID used for the deletion.
        #[arg(long)]
        request_id: String,
        /// The deletion receipt's current revision.
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        expected_revision: i64,
    },
    /// Explicitly replace a known remote conflict with the exact saved local revision.
    /// Another client can still edit between the provider read and replacement.
    DraftReplace {
        /// Draft id.
        #[arg(long)]
        draft_id: String,
        /// Saved local draft revision to upload.
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        expected_revision: i64,
        /// Revision of the conflicting Google copy, from `google draft-status`.
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        expected_google_revision: i64,
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
    },
    /// Explicitly submit one exact saved revision through an active Google session.
    /// Unknown outcomes never trigger another automatic submission.
    Send {
        /// Draft id.
        #[arg(long)]
        draft_id: String,
        /// Saved local draft revision to send.
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        expected_revision: i64,
        /// Google draft revision, from `google draft-status`.
        #[arg(long, value_parser=clap::value_parser!(i64).range(0..))]
        expected_google_revision: i64,
        /// Unique ID for this submission; reuse it only to retry the same send.
        #[arg(long)]
        request_id: String,
        /// Wait six seconds before connecting; Ctrl+C keeps the draft unsent.
        #[arg(long)]
        undo: bool,
    },
    /// Inspect one retained submission outcome without contacting Google.
    SendStatus {
        /// Request ID used for the send.
        #[arg(long)]
        request_id: String,
    },
    /// List retained outgoing messages awaiting acquisition, without contacting Google.
    Sent {
        /// Messages to skip.
        #[arg(long, default_value_t = 0)]
        offset: u64,
        /// Messages per page.
        #[arg(long, default_value_t = 50, value_parser=clap::value_parser!(u64).range(1..=100))]
        limit: u64,
        /// Only messages in this conversation.
        #[arg(long)]
        conversation_id: Option<String>,
    },
    /// Read the exact retained submission content, without contacting Google.
    SentMessage {
        /// Request ID used for the send.
        #[arg(long)]
        request_id: String,
    },
    /// Inspect bounded Sent evidence without submitting the message again.
    SendReconcile {
        /// Request ID used for the send.
        #[arg(long)]
        request_id: String,
    },
    /// Abandon only a proven-undispatched prepared submission at its current revision.
    SendAbandon {
        /// Request ID used for the send.
        #[arg(long)]
        request_id: String,
        /// The submission's current revision.
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        expected_revision: i64,
    },
    /// Browse cached account labels; pagination is bound to the catalog revision.
    Labels {
        /// Labels per page.
        #[arg(long,default_value_t=100,value_parser=clap::value_parser!(u32).range(1..=100))]
        limit: u32,
        /// Continue after this label id (`next_after` from the previous page).
        #[arg(long, requires = "expected_catalog_revision")]
        after: Option<String>,
        /// Catalog revision the pages must belong to; required with --after.
        #[arg(long)]
        expected_catalog_revision: Option<u64>,
    },
    /// Refresh the label catalog through the currently active owned sync runtime.
    LabelsRefresh,
    /// List Gmail Spam live, 50 at a time; spam is never downloaded or stored.
    Spam {
        /// Page token from the previous page.
        #[arg(long)]
        page_token: Option<String>,
    },
    /// Read one Spam message's text live from Gmail without storing it.
    SpamShow {
        /// Gmail message id, as listed by `google spam`.
        id: String,
    },
    /// Search the account on Gmail; lists threads with nothing on this machine, read live.
    Search {
        /// Search text; it is translated to a Gmail search.
        query: String,
        /// Folder to search.
        #[arg(long,default_value="all",value_parser=["inbox","sent","all","trash","drafts"])]
        folder: String,
    },
    /// Read a remote search result's thread live from Gmail without storing it.
    Thread {
        /// Gmail thread id, as listed by `google search`.
        thread_id: String,
    },
    /// Fetch one attachment of a remote thread into the view cache and print its path.
    Attachment {
        /// Attachment id, as listed by `google thread`.
        id: String,
    },
    /// Gmail's message and conversation totals for Spam or Trash.
    BinCount {
        /// Folder to count.
        #[arg(value_parser=["spam","trash"])]
        folder: String,
    },
    /// Move a Spam thread to the Inbox, marking it not spam.
    NotSpam {
        /// Gmail thread id, as listed by `google spam`.
        thread_id: String,
    },
    /// Delete one conversation's Spam or Trash messages for good (Spam takes a
    /// Gmail thread id, Trash a conversation id). Needs full mail access.
    DeleteForever {
        /// Folder the messages are in.
        #[arg(value_parser=["spam","trash"])]
        folder: String,
        /// Gmail thread id for Spam, conversation id for Trash.
        id: String,
        /// Confirm: this cannot be undone.
        #[arg(long)]
        yes: bool,
    },
    /// Delete everything in Spam or Trash for good. Needs full mail access.
    Empty {
        /// Folder to empty.
        #[arg(value_parser=["spam","trash"])]
        folder: String,
        /// Confirm: this cannot be undone.
        #[arg(long)]
        yes: bool,
    },
    /// Begin or resume remote Drafts inventory through an already owned sync session.
    DraftsRefresh,
    /// Read cached remote Drafts coverage without contacting Google.
    DraftsStatus,
    /// Bind an existing Google label to a local category; never creates provider labels.
    CategoryBind {
        /// Local label id.
        category_id: String,
        /// Gmail label id to link it to, as listed by `google labels`.
        provider_label_id: String,
        /// Label catalog revision, from `google labels`.
        #[arg(long)]
        expected_catalog_revision: u64,
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
        /// Create the local label with this name; it must not exist yet.
        #[arg(long,conflicts_with_all=["expected_provider_label_id","unmapped"],required_unless_present_any=["expected_provider_label_id","unmapped"])]
        create_name: Option<String>,
        /// Gmail label id the existing local label is linked to now.
        #[arg(long,conflicts_with_all=["create_name","unmapped"],required_unless_present_any=["create_name","unmapped"])]
        expected_provider_label_id: Option<String>,
        /// The existing local label is not linked to a Gmail label yet.
        #[arg(long,conflicts_with_all=["create_name","expected_provider_label_id"],required_unless_present_any=["create_name","expected_provider_label_id"])]
        unmapped: bool,
    },
}

// Parsed once per invocation, so List's many flags cost nothing to keep inline.
#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
enum Mail {
    /// Restore the exact retained state before a mail action (fails after newer edits).
    Undo {
        /// Conversation id.
        id: String,
        /// Request ID of the action to undo.
        #[arg(long)]
        undo_request_id: String,
        /// Conversation revision you last read.
        #[arg(long)]
        expected_revision: u64,
        /// Unique ID for this undo; generated when omitted.
        #[arg(long)]
        request_id: Option<String>,
    },
    /// Archive, move to Trash or restore the currently loaded conversation members.
    Filing {
        /// Conversation id.
        id: String,
        /// What to do with the conversation.
        #[arg(long,value_parser=["archive","trash","restore"])]
        action: String,
        /// Conversation revision you last read; defaults to the one just loaded.
        #[arg(long)]
        expected_revision: Option<u64>,
        /// Unique ID for this change; generated when omitted.
        #[arg(long)]
        request_id: Option<String>,
    },
    /// Inspect pending, uncertain and failed Google state changes without provider access.
    Delivery {
        /// Show only the change made with this request ID.
        #[arg(long)]
        request_id: Option<String>,
    },
    /// List the selected account's conversations, filtered and paged.
    List {
        /// Search text: bare words match subject or message text; a bare
        /// address matches the sender or a recipient; subject:, body:, from:
        /// (sender) and to: (To or Cc) narrow one part; label:,
        /// has:attachment and is:review|delayed|done also parse.
        #[arg(long)]
        query: Option<String>,
        /// Column for bare words; `all` means subject or message text.
        #[arg(long,default_value="all",value_parser=["all","sender","subject","body"])]
        field: String,
        /// Folder to list; `received` also includes archived incoming mail.
        #[arg(long,default_value="inbox",value_parser=["inbox","received","sent","all","trash"])]
        folder: String,
        /// Only mail with any of these label ids (comma-separated).
        #[arg(long)]
        category: Option<String>,
        /// Only mail whose sender name or address contains this text.
        #[arg(long)]
        sender: Option<String>,
        /// Only mail from exactly this address.
        #[arg(long)]
        exact_sender: Option<String>,
        /// Only mail from this kind of sender; `unknown` means not yet classified.
        #[arg(long,value_parser=["all","people","automated","unknown"])]
        kind: Option<String>,
        /// Only conversations to review (unread), held (Delayed) or done.
        #[arg(long,value_parser=["all","review","held","done"])]
        status: Option<String>,
        /// Only mail with (true) or without (false) attachments.
        #[arg(long)]
        has_attachment: Option<bool>,
        /// Only mail sent on or after this UTC date (YYYY-MM-DD).
        #[arg(long)]
        after: Option<String>,
        /// Only mail sent before this UTC date (YYYY-MM-DD).
        #[arg(long)]
        before: Option<String>,
        /// Group conversations into Review, Delayed and Done.
        #[arg(long)]
        grouped: bool,
        /// IANA zone for counting Done conversations by month (grouped lists).
        #[arg(long)]
        month_zone: Option<String>,
        /// Leave a Done month (YYYY-MM) out of the page; needs --month-zone.
        #[arg(long)]
        collapsed_month: Vec<String>,
        /// Continue from the previous page's `next_cursor` instead of using --offset.
        #[arg(long)]
        cursor: Option<String>,
        /// Conversations per page.
        #[arg(long,default_value_t=30,value_parser=clap::value_parser!(u32).range(1..=100))]
        limit: u32,
        /// Conversations to skip.
        #[arg(long, default_value_t = 0)]
        offset: u64,
    },
    /// People a search's from: or to: can name: recent senders, or the
    /// people you wrote to with --role recipient (the default in --folder
    /// sent|drafts). Querying never marks mail reviewed.
    Senders {
        /// Text to match against names and addresses.
        #[arg(default_value = "")]
        query: String,
        /// Folder to look in.
        #[arg(long,default_value="inbox",value_parser=["inbox","all","sent","drafts","trash"])]
        folder: String,
        /// List senders, or the people you wrote to.
        #[arg(long,value_parser=["sender","recipient"])]
        role: Option<String>,
        /// Maximum people to return.
        #[arg(long,default_value_t=5,value_parser=clap::value_parser!(u8).range(1..=10))]
        limit: u8,
    },
    /// Query only; this does not mark mail reviewed.
    Read {
        /// Conversation id, as listed by `mail list`.
        id: String,
    },
    /// Inspect retained message headers, HTML and source without marking reviewed.
    Source {
        /// Message id, as listed by `mail read`.
        id: String,
        /// The message's `source_version`, from `mail read`.
        #[arg(long)]
        source_version: String,
    },
    /// Show a received invitation's calendar details and its cached event, read-only.
    Invite {
        /// Message id, as listed by `mail read`.
        id: String,
        /// The message's `source_version`, from `mail read`.
        #[arg(long)]
        source_version: String,
    },
    /// Unsubscribe from a received message's mailing list: one-click in the
    /// background, the list's unsubscribe email through Google, or print the
    /// unsubscribe page to open.
    Unsubscribe {
        /// Message id, as listed by `mail read`.
        id: String,
        /// The message's `source_version`, from `mail read`.
        #[arg(long)]
        source_version: String,
    },
    /// The sanitised HTML the reader renders, with its image and link lists.
    Html {
        /// Message id, as listed by `mail read`.
        id: String,
        /// The message's `source_version`, from `mail read`.
        #[arg(long)]
        source_version: String,
    },
    /// A message's images: cached and inline ones by default. --load fetches
    /// the remote ones through the engine, over https only, and through the
    /// HTTPS proxy when one is set. Blocked hosts never load.
    Images {
        /// Message id, as listed by `mail read`.
        id: String,
        /// The message's `source_version`, from `mail read`.
        #[arg(long)]
        source_version: String,
        /// Also fetch remote images.
        #[arg(long)]
        load: bool,
    },
    /// Exact sender history in this account; querying never marks mail reviewed.
    Sender {
        /// Sender email address.
        address: String,
    },
    /// Suggest addresses from recent cached mail and drafts; never changes routing.
    Recipients {
        /// Text to match against names and addresses.
        query: String,
        /// Address to leave out, such as one already on the draft; repeatable.
        #[arg(long)]
        exclude: Vec<String>,
        /// Maximum suggestions to return.
        #[arg(long, default_value_t = 6)]
        limit: u8,
    },
    /// Mark the loaded snapshot read.
    Review {
        /// Conversation id.
        id: String,
        /// Conversation revision you last read; defaults to the one just loaded.
        #[arg(long)]
        expected_revision: Option<u64>,
        /// Unique ID for this change; generated when omitted.
        #[arg(long)]
        request_id: Option<String>,
    },
    /// Atomically mark the loaded snapshot read and clear all its stars.
    Done {
        /// Conversation id.
        id: String,
        /// Conversation revision you last read; defaults to the one just loaded.
        #[arg(long)]
        expected_revision: Option<u64>,
        /// Unique ID for this change; generated when omitted.
        #[arg(long)]
        request_id: Option<String>,
    },
    /// Place the loaded snapshot on untimed hold without changing read state.
    Delay {
        /// Conversation id.
        id: String,
        /// Conversation revision you last read; defaults to the one just loaded.
        #[arg(long)]
        expected_revision: Option<u64>,
        /// Unique ID for this change; generated when omitted.
        #[arg(long)]
        request_id: Option<String>,
    },
    /// Clear snapshot stars without changing read state.
    Undelay {
        /// Conversation id.
        id: String,
        /// Conversation revision you last read; defaults to the one just loaded.
        #[arg(long)]
        expected_revision: Option<u64>,
        /// Unique ID for this change; generated when omitted.
        #[arg(long)]
        request_id: Option<String>,
    },
    /// Set the read and starred state of the loaded snapshot directly.
    Flag {
        /// Conversation id.
        id: String,
        /// Mark read (true) or unread (false).
        #[arg(long)]
        read: Option<bool>,
        /// Star (true) or unstar (false).
        #[arg(long)]
        starred: Option<bool>,
        /// Conversation revision you last read; defaults to the one just loaded.
        #[arg(long)]
        expected_revision: Option<u64>,
        /// Unique ID for this change; generated when omitted.
        #[arg(long)]
        request_id: Option<String>,
    },
    /// Add a label to the loaded snapshot, or remove it.
    Category {
        /// Conversation id.
        id: String,
        /// Label id.
        category_id: String,
        /// Remove the label instead of adding it.
        #[arg(long)]
        remove: bool,
        /// Conversation revision you last read; defaults to the one just loaded.
        #[arg(long)]
        expected_revision: Option<u64>,
        /// Unique ID for this change; generated when omitted.
        #[arg(long)]
        request_id: Option<String>,
    },
}

#[derive(Subcommand)]
enum Labels {
    /// List the account's labels with their instructions and Auto-read and Spam settings.
    List,
    /// Create a local label in this account.
    Create {
        /// Label name, 1–256 bytes, unique in the account.
        name: String,
        /// Id for the new label.
        #[arg(long)]
        id: String,
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
    },
    /// Set the instructions the local model follows for a label.
    Describe {
        /// Label id.
        id: String,
        /// New instructions, at most 512 bytes; empty clears them.
        description: String,
        /// The current instructions, so a concurrent edit is refused.
        #[arg(long)]
        expected_description: String,
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
    },
    /// Remove a local label and its memberships; Gmail labels and mail are unchanged.
    Remove {
        /// Label id.
        id: String,
        /// The label's current name.
        #[arg(long)]
        expected_name: String,
        /// The Gmail label id it is linked to now; omit when it has none.
        #[arg(long)]
        expected_provider_label_id: Option<String>,
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
    },
    /// When the local model applies this label, mark arriving mail read.
    AutoRead {
        /// Label id.
        id: String,
        /// New setting.
        #[arg(value_parser = ["on", "off"])]
        state: String,
        /// Current setting, so a concurrent change is refused.
        #[arg(long, value_parser = ["on", "off"])]
        expected: String,
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
    },
    /// When the local model applies this label, send the message to Spam.
    Spam {
        /// Label id.
        id: String,
        /// New setting.
        #[arg(value_parser = ["on", "off"])]
        state: String,
        /// Current setting, so a concurrent change is refused.
        #[arg(long, value_parser = ["on", "off"])]
        expected: String,
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
    },
    /// Match local and Gmail labels by name, creating missing ones; Ctrl-C cancels.
    Sync {
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
    },
}

#[derive(Subcommand)]
enum Assist {
    /// How many unprocessed messages each Catch up range would add, for this account.
    Backlog,
    /// Per-account counts: processed, queued, failed and unprocessed.
    Progress,
    /// Queue older mail in this account after new mail, newest first. The range
    /// only widens; `stop` drops older mail from the queue again.
    CatchUp {
        /// How far back to queue older mail, or `stop`.
        #[arg(long, value_parser = ["week", "month", "year", "all", "stop"])]
        range: String,
    },
    /// Pause the local model for this account; downloading continues.
    Pause,
    /// Resume the local model for this account.
    Resume,
    /// Discover models from the configured same-machine service.
    Discover {
        /// Service URL to query instead of the saved one.
        #[arg(long)]
        endpoint: Option<String>,
        /// Service type to query instead of the saved one.
        #[arg(long, value_parser = ["ollama", "openai_compatible"])]
        provider: Option<String>,
    },
    /// Show saved settings, defaults and their revision.
    Settings,
    /// Save a settings JSON object with compare-and-swap revision checking.
    Configure {
        /// Settings JSON file, or `-` for stdin; at most 48 KiB.
        #[arg(long)]
        settings_file: PathBuf,
        /// Settings revision, from `assist settings`.
        #[arg(long)]
        expected_revision: u64,
    },
    /// Run built-in synthetic mail through the model without changing settings.
    Test {
        /// Built-in sample to run, such as personal or receipt.
        #[arg(long, default_value = "personal")]
        sample_case: String,
        /// Try these settings (JSON file, or `-` for stdin) instead of the saved ones.
        #[arg(long)]
        settings_file: Option<PathBuf>,
    },
    /// Summarize, label and tag one received message now, replacing saved results.
    Analyze {
        /// Conversation id.
        conversation_id: String,
        /// Received message to analyze.
        #[arg(long)]
        message_id: String,
    },
    /// Summarize a whole conversation, your own replies included, in one
    /// short paragraph, replacing its saved summary.
    Thread {
        /// Conversation id.
        conversation_id: String,
    },
    /// Show the local model's saved results for a conversation.
    Results {
        /// Conversation id.
        conversation_id: String,
    },
    /// Set a message's sender kind by hand, or clear it with `reset`.
    Correct {
        /// Conversation id.
        conversation_id: String,
        /// Message id.
        message_id: String,
        /// Sender kind to record, or `reset`.
        #[arg(value_parser=["human","automated","unknown","reset"])]
        kind: String,
        /// Message revision you last read.
        #[arg(long)]
        expected_revision: u64,
    },
    /// Apply a label that the local model currently suggests for a message.
    Accept {
        /// Conversation id.
        conversation_id: String,
        /// Message id.
        message_id: String,
        /// Suggested label id.
        category_id: String,
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
    },
    /// Work through the queue until Ctrl-C: new mail first, then Catch up.
    #[command(alias = "watch")]
    Run,
}

#[derive(Subcommand)]
enum LinkCommand {
    /// Messages in every account that link to a host, outside one conversation.
    Summary {
        /// Host to look up, such as example.com.
        host: String,
        /// Conversation in the selected account to leave out of the count.
        #[arg(long)]
        conversation: Option<String>,
    },
    /// Blocked patterns, with how many messages link to each.
    Blocked,
    /// Block a host (tracker.io) or a domain and its subdomains (*.tracker.io).
    Block {
        /// Host or `*.domain`; a URL is reduced to its host.
        pattern: String,
    },
    /// Remove a pattern from the blocklist.
    Unblock {
        /// Pattern to unblock, as listed by `links blocked`.
        pattern: String,
    },
}

#[derive(Subcommand)]
enum ProxyCommand {
    /// Proxy settings and whether a password is saved; it is never shown.
    Show,
    /// Send every remote image request through host[:port] (443 by default).
    /// Nothing goes direct while the proxy is on, even when it is unavailable.
    Set {
        /// Proxy host, with an optional :port.
        host: String,
        /// User name for the proxy, if it needs one.
        #[arg(long, default_value = "")]
        username: String,
        /// Read the password from standard input into the system keychain.
        #[arg(long)]
        password_stdin: bool,
    },
    /// Stop using the proxy and forget its saved password.
    Clear,
    /// One request through the proxy to check that it works.
    Test,
    /// The image cache's size on disk; --clear empties it.
    Cache {
        /// Empty the image cache.
        #[arg(long)]
        clear: bool,
    },
}

#[derive(Subcommand)]
enum Preferences {
    /// Show the saved preferences and their revision.
    Show,
    /// Apply a bounded preferences JSON patch from a file or stdin (-).
    Save {
        /// JSON patch file, or `-` for stdin; at most 16 KiB.
        #[arg(long)]
        preferences_file: PathBuf,
        /// Preferences revision, from `preferences show`.
        #[arg(long)]
        expected_revision: u64,
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
    },
}

#[derive(Subcommand)]
enum Draft {
    /// Change the owning From account while preserving this saved composition.
    Transfer {
        /// Draft id.
        #[arg(long)]
        draft_id: String,
        /// Account to move the draft to.
        #[arg(long)]
        target_account: String,
        /// Saved local draft revision.
        #[arg(long)]
        expected_revision: i64,
        /// Google draft revision, from `google draft-status`.
        #[arg(long)]
        expected_google_revision: i64,
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
        /// Keep the original draft in its account instead of moving it.
        #[arg(long)]
        keep_original: bool,
    },
    /// Resume a blocked account change so its cleanup continues.
    TransferRetry {
        /// Request ID of the interrupted account change.
        #[arg(long)]
        transfer_request_id: String,
        /// Unique ID for this retry; reuse it only to repeat the same request.
        #[arg(long)]
        request_id: String,
    },
    /// Inspect a recorded account change without repeating provider work
    TransferStatus {
        /// Request ID of the account change.
        #[arg(long)]
        request_id: String,
    },
    /// Inspect exact retained-source conversion and editing eligibility without Google access.
    Authoring {
        /// Draft id.
        #[arg(long)]
        draft_id: String,
        /// Retained source revision, as listed by `draft sources`.
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        source_revision: i64,
    },
    /// Create a separate plain-text draft preserving validated recipients and attachment files.
    /// Uses primary From, omits HTML/inline placement/reply linkage and adds no signature.
    /// The copy may sync automatically while account sync is running; the original is retained.
    Copy {
        /// Draft id.
        #[arg(long)]
        draft_id: String,
        /// Retained source revision, as listed by `draft sources`.
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        source_revision: i64,
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
        /// From address for the copy; only the account's `primary` address is supported.
        #[arg(long, value_parser = ["primary"])]
        from_policy: String,
        /// Body format for the copy; only `plain_text` is supported.
        #[arg(long, value_parser = ["plain_text"])]
        content_policy: String,
        /// Where inline images go; only `attachments` is supported.
        #[arg(long, value_parser = ["attachments"])]
        inline_policy: String,
        /// Reply threading for the copy; only `omit` is supported.
        #[arg(long, value_parser = ["omit"])]
        thread_policy: String,
    },
    /// Browse retained Google source versions without contacting Google.
    Sources {
        /// Draft id.
        #[arg(long)]
        draft_id: String,
        /// Source versions per page.
        #[arg(long, default_value_t=30, value_parser=clap::value_parser!(u32).range(1..=100))]
        limit: u32,
        /// Source versions to skip.
        #[arg(long, default_value_t=0, value_parser=clap::value_parser!(u64).range(0..=10_000_000))]
        offset: u64,
    },
    /// Export exact retained Google RAW or an attachment at the specified source revision.
    /// Requires an explicit profile and account, but no active Google sync session.
    Source {
        /// Draft id.
        #[arg(long)]
        draft_id: String,
        /// Retained source revision, as listed by `draft sources`.
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        source_revision: i64,
        /// Export this attachment instead of the RAW message.
        #[arg(long)]
        attachment_id: Option<String>,
        /// Copy the verified bytes to a new file; never overwrites an existing file.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Browse durable drafts in the selected account.
    List {
        /// Text to search the drafts for.
        #[arg(long)]
        query: Option<String>,
        /// Drafts per page.
        #[arg(long,default_value_t=30,value_parser=clap::value_parser!(u32).range(1..=100))]
        limit: u32,
        /// Drafts to skip.
        #[arg(long, default_value_t = 0)]
        offset: u64,
    },
    /// Create local human composition; Google submission is a separate explicit action.
    Create {
        /// Kind of draft to create.
        #[arg(long, default_value="new", value_parser=["new","reply","reply_all","forward"])]
        mode: String,
        /// Conversation to reply to or forward.
        #[arg(long)]
        conversation_id: Option<String>,
        /// Message to reply to or forward.
        #[arg(long)]
        source_message_id: Option<String>,
        /// Unique ID for this draft; generated when omitted.
        #[arg(long)]
        request_id: Option<String>,
    },
    /// Inspect bounded local capture history; never claims provider delivery.
    History {
        /// Only captures from this conversation.
        #[arg(long)]
        conversation_id: Option<String>,
        /// Only captures of this draft.
        #[arg(long, conflicts_with = "conversation_id")]
        draft_id: Option<String>,
        /// Only captures still prepared, in flight or of unknown outcome.
        #[arg(long)]
        unresolved_only: bool,
        /// Captures per page.
        #[arg(long, default_value_t = 30)]
        limit: u32,
        /// Captures to skip.
        #[arg(long, default_value_t = 0)]
        offset: u64,
    },
    /// Discard a local draft, retaining its exact state for revision-bound Undo.
    Discard {
        #[command(flatten)]
        target: DraftTarget,
        /// Draft revision you last read.
        #[arg(long)]
        expected_revision: u64,
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
        /// Optional authored editor fields and unfinished recipients to preserve for Undo.
        #[arg(long)]
        buffer_file: Option<PathBuf>,
    },
    /// Inspect the original discard receipt and whether its exact revision can be restored.
    DiscardStatus {
        /// Request ID used for the discard.
        #[arg(long)]
        request_id: String,
    },
    /// Restore the exact draft snapshot from a prior discard; never overwrites newer edits.
    UndoDiscard {
        /// Id of the discarded draft.
        #[arg(long)]
        draft_id: String,
        /// Request ID used for the discard.
        #[arg(long)]
        discard_request_id: String,
        /// The revision the draft had when it was discarded.
        #[arg(long, value_parser=clap::value_parser!(i64).range(1..))]
        expected_revision: i64,
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
    },
    /// Retain a local file and select it on this saved draft. Never sends or uploads.
    Attach {
        /// Draft id.
        #[arg(long)]
        draft_id: String,
        /// Local file path; matching replay works after the original file is removed.
        #[arg(long)]
        file: PathBuf,
        /// Draft revision you last read.
        #[arg(long, value_parser=clap::value_parser!(u64).range(1..=i64::MAX as u64))]
        expected_revision: u64,
        /// Unique ID for this change; reuse it only to retry the same change.
        #[arg(long)]
        request_id: String,
    },
    /// Serialize a saved draft into a local .eml capture. Never uses the network.
    Capture {
        #[command(flatten)]
        target: DraftTarget,
        /// Saved draft revision to capture.
        #[arg(long)]
        expected_revision: u64,
        /// Unique ID for this capture; reuse it only to retry the same capture.
        #[arg(long)]
        request_id: String,
        /// Outcome to record; `unknown` simulates an unconfirmed acceptance.
        #[arg(long, default_value="accepted", value_parser=["accepted","unknown"])]
        outcome: String,
    },
    /// Show a saved draft.
    Show {
        #[command(flatten)]
        target: DraftTarget,
    },
    /// Save a draft's body and, optionally, its subject, recipients and attachments.
    Save {
        #[command(flatten)]
        target: DraftTarget,
        /// New subject; omit to keep the saved one.
        #[arg(long)]
        subject: Option<String>,
        /// JSON object with optional to_recipients/cc_recipients/bcc_recipients arrays.
        #[arg(long)]
        recipients_file: Option<PathBuf>,
        /// Plain-text body file, or `-` for stdin; at most 256 KiB.
        #[arg(long)]
        body_file: PathBuf,
        /// HTML body file; when omitted, saved HTML is kept only if the text body is unchanged.
        #[arg(long)]
        html_file: Option<PathBuf>,
        /// Signature text file for this draft; at most 16 KiB.
        #[arg(long)]
        signature_file: Option<PathBuf>,
        /// Retained attachment id to select; repeatable, and replaces the saved selection.
        #[arg(long)]
        attachment: Vec<String>,
        /// Remove every attachment from the draft.
        #[arg(long)]
        clear_attachments: bool,
        /// Draft revision you last read; defaults to the current one.
        #[arg(long)]
        expected_revision: Option<u64>,
        /// Unique ID for this save; generated when omitted.
        #[arg(long)]
        request_id: Option<String>,
    },
}

// Selects a draft by its id or, for a legacy reply, by conversation; exactly one
// of the two is required.
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
    /// The engine's draft selector: `draft_id` when given, otherwise `conversation_id`.
    fn params(&self) -> Value {
        match &self.draft_id {
            Some(id) => json!({"draft_id":id}),
            None => json!({"conversation_id":self.conversation_id}),
        }
    }
}

/// Why a command failed. `code` is the engine's or the CLI's stable error code
/// and picks the exit status in `main`; `message` is for people.
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
    /// The error envelope printed under `--json`: the engine's error shape with
    /// id `cli`, and only the affirmative recovery proofs as `details`.
    fn json_reply(&self) -> Value {
        let mut reply = fm_ipc::error("cli", &self.code, &self.message);
        if let Some(details) = self.recovery.details() {
            reply["error"]["details"] = details;
        }
        reply
    }
}
// A broken pipe gets its own code so `main` can exit quietly when whoever reads
// stdout goes away (for example `fm mail list | head`).
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
/// An `invalid_request` failure, which exits with status 2.
fn invalid(msg: &str) -> Failure {
    Failure {
        code: "invalid_request".into(),
        message: msg.into(),
        recovery: RecoveryProofs::default(),
    }
}
/// Unwraps an engine reply envelope: its `result` when `ok` is true, otherwise
/// a `Failure` with the engine's error code, message and recovery proofs.
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
/// Reads a whole input file, or stdin when `path` is `-`. Input longer than
/// `limit` bytes is refused as `invalid_request`.
fn read_input(path: &Path, limit: u64) -> Result<Vec<u8>, Failure> {
    let reader: Box<dyn Read> = if path == Path::new("-") {
        Box::new(io::stdin())
    } else {
        Box::new(std::fs::File::open(path)?)
    };
    let mut data = Vec::new();
    // Read one byte past the limit to detect oversized input without reading it all.
    reader.take(limit + 1).read_to_end(&mut data)?;
    if data.len() as u64 > limit {
        return Err(invalid("input file is too large"));
    }
    Ok(data)
}
/// The `--account` value, or an `invalid_request` failure when it is missing.
fn account(args: &Args) -> Result<&str, Failure> {
    args.account.as_deref().ok_or_else(|| {
        invalid("this command requires --account; use 'fm accounts' to list accounts")
    })
}

/// Builds the `draft_attachment_import` params for `draft attach`, or `None`
/// for any other command. The file path is made absolute but not opened.
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

/// Maps a Google draft or Send command to its engine method and params, or
/// returns `None` for any other command.
///
/// Called before connecting: an empty draft id or request id, one containing
/// control characters, or one longer than 256 (draft) or 128 (request) bytes is
/// `invalid_request`. `--undo` is client-side and never reaches the params.
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
            ..
        } => (
            "google_send",
            json!({"draft_id":draft_id,"expected_revision":expected_revision,
                "expected_google_revision":expected_google_revision,"request_id":request_id}),
        ),
        Google::SendStatus { request_id } => {
            ("google_send_status", json!({"request_id":request_id}))
        }
        Google::Sent {
            offset,
            limit,
            conversation_id,
        } => {
            let mut params = json!({"offset":offset,"limit":limit});
            if let Some(id) = conversation_id {
                params["conversation_id"] = id.as_str().into();
            }
            ("google_sent", params)
        }
        Google::SentMessage { request_id } => {
            ("google_sent_message", json!({"request_id":request_id}))
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

/// Maps the retained Google draft commands (`draft authoring`, `copy`,
/// `sources` and `source`) to their engine method and params, or returns `None`.
///
/// Identities are checked before connecting, with the same bounds as
/// `google_composition_request` (attachment ids up to 256 bytes).
/// `draft source --output` is handled after the reply by `copy_export`.
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

/// Copies the retained file named by the reply's `path` to `output`.
///
/// An error reply, a missing `path` or an unreadable source fails before
/// `output` is created. An existing file is never overwritten, and the copy
/// is synced to disk before this returns. It keeps the engine's
/// downloaded-from-the-internet mark (the macOS quarantine flag, the Windows
/// Internet zone) where the destination can hold it.
fn copy_export(reply: &Value, output: &Path) -> Result<(), Failure> {
    let data = result(reply.clone())?;
    let source = data["path"].as_str().ok_or_else(|| Failure {
        code: "content_unavailable".into(),
        message: "Retained content is not fully available offline".into(),
        recovery: RecoveryProofs::default(),
    })?;
    let mut opened = std::fs::File::open(source)?;
    // The system copy brings the mark only to a new file, so copy to a fresh
    // name beside `output` and link it into place, which fails if `output`
    // exists. Without hard links, fall back to an exclusive plain copy.
    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.subsec_nanos());
    let staged = parent.join(format!(".fm-export-{}-{nanos}.tmp", std::process::id()));
    let linked = std::fs::copy(source, &staged).and_then(|_| std::fs::hard_link(&staged, output));
    let _ = std::fs::remove_file(&staged);
    match linked {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists || output.exists() => {
            return Err(error.into())
        }
        Err(_) => {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)?;
            io::copy(&mut opened, &mut file)?;
        }
    }
    std::fs::OpenOptions::new()
        .write(true)
        .open(output)?
        .sync_all()?;
    Ok(())
}

/// Maps an `account` subcommand to its engine method and params, or returns
/// `None` for any other command. Any request id must be 1–128 bytes without
/// control characters.
fn account_lifecycle_request(command: &Command) -> Result<Option<(&'static str, Value)>, Failure> {
    let Command::Account { command } = command else {
        return Ok(None);
    };
    let (method, params) = match command {
        AccountCommand::RemovePreview => ("account_remove_preview", json!({})),
        AccountCommand::DeleteArchive {
            expected_revision,
            request_id,
        } => (
            "account_archive_delete",
            json!({"expected_revision":expected_revision,"request_id":request_id}),
        ),
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

/// Opens Google's sign-in page in the default browser. The launcher runs
/// detached with no stdio; the sign-in itself stays tied to this CLI's engine
/// session while `run` polls for the result.
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
        // Launch through the absolute system rundll32 path, not a PATH lookup.
        // 0x08000000 is CREATE_NO_WINDOW, so no console window flashes up.
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

/// Stack for the thread that parses arguments and runs the command. clap builds
/// the whole command tree on this thread, and in unoptimized builds that needs
/// nearly all of the 1 MiB main-thread stack Windows gives a process.
const CLI_STACK_BYTES: usize = 8 * 1024 * 1024;

fn main() {
    let cli = std::thread::Builder::new()
        .name("fm".into())
        .stack_size(CLI_STACK_BYTES)
        .spawn(cli_main)
        .expect("start the CLI thread");
    // A panic has already printed its message; exit like an unwinding main would.
    if cli.join().is_err() {
        std::process::exit(101);
    }
}

fn cli_main() {
    let args = Args::parse();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("start the async runtime");
    let result = runtime.block_on(run(&args));
    // Shut the runtime down before any exit below, as `#[tokio::main]` did on return.
    drop(runtime);
    if let Err(e) = result {
        // Nobody is reading stdout any more; end quietly with status 0.
        if e.code == "broken_pipe" {
            return;
        }
        // With --json the error goes to stdout like any reply, so scripts read
        // one stream; otherwise it is a one-line message on stderr.
        if args.json {
            let _ = writeln!(io::stdout(), "{}", e.json_reply());
        } else {
            eprintln!("fm: {}: {}", e.code, e.message);
        }
        // Stable exit codes for scripts: 2 invalid input, 3 authentication or
        // permission, 4 stale revision or cursor, 5 missing or unavailable
        // content, 1 anything else.
        std::process::exit(match e.code.as_str() {
            "invalid_request" | "invalid_params" | "request_id_conflict" => 2,
            "permission_denied" | "unauthorized" | "authentication_required" => 3,
            "stale_revision" | "stale_cursor" => 4,
            "not_found" | "unsupported" | "content_unavailable" => 5,
            _ => 1,
        });
    }
}

/// Builds the `calendar_search` params for `calendar search`, or returns `None`.
/// The query must have 1–32 words in at most 512 bytes.
fn calendar_search_request(command: &Command) -> Result<Option<Value>, Failure> {
    let Command::Calendar {
        command:
            Some(CalendarCommand::Search {
                query,
                from,
                to,
                around,
                limit,
            }),
        ..
    } = command
    else {
        return Ok(None);
    };
    let count = query.split_whitespace().count();
    if query.len() > 512 || count == 0 || count > 32 {
        return Err(invalid(
            "Calendar search needs 1–32 words in at most 512 UTF-8 bytes",
        ));
    }
    Ok(Some(match around {
        Some(around) => json!({"query":query,"around":around,"limit":limit}),
        None => json!({"query":query,"from":from,"to":to,"limit":limit}),
    }))
}

/// Maps `notifications`, `calendar reminders` and `calendar reminder-ack` to
/// their engine method and params, or returns `None`. Identities and
/// timestamps must be nonempty, bounded and free of control characters.
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

/// Runs one parsed command against the engine and prints its reply.
///
/// Connecting creates the profile directory if needed and may start an engine,
/// so account scope, profile rules, request identities and the Google Calendar
/// and draft-discard input files are checked first: a mistake there fails
/// without creating a profile, with status 2 for invalid values. Other input
/// files are read after connecting.
async fn run(args: &Args) -> Result<(), Failure> {
    let profile = match &args.profile {
        Some(p) => p.to_owned(),
        None if args.demo_profile => fm_ipc::demo_profile()?,
        None => fm_ipc::default_profile()?,
    };
    // --engine wins over FM_ENGINE_PATH. With neither, fm-ipc starts the
    // fm-engine next to this executable. Either way it is only used when no
    // engine is already running for the profile.
    let engine = args
        .engine
        .clone()
        .or_else(|| std::env::var_os("FM_ENGINE_PATH").map(PathBuf::from));
    // Validate scope and local input before starting any engine work.
    // The commands listed here skip this up-front --account check (a few check
    // it later); every other command needs it now.
    if !matches!(
        args.command,
        Command::Accounts { .. }
            | Command::Status
            | Command::Notifications { .. }
            | Command::Watch { .. }
            | Command::Call { .. }
            | Command::Preferences { .. }
            | Command::Proxy { .. }
            | Command::Links {
                command: LinkCommand::Blocked
                    | LinkCommand::Block { .. }
                    | LinkCommand::Unblock { .. }
            }
            | Command::Assist { .. }
            | Command::CalendarSources
            | Command::Calendar { combined: true, .. }
            | Command::Calendar {
                command: Some(CalendarCommand::Reminders { .. }),
                ..
            }
            | Command::Calendar {
                command: Some(CalendarCommand::Search { .. }),
                ..
            }
            | Command::ProfileInit
            | Command::Google {
                command: Google::Connect { .. } | Google::SetupStatus
            }
            | Command::Google {
                command: Google::SyncStatus
            }
    ) || matches!(
        args.command,
        Command::Assist {
            command: Assist::Thread { .. }
        }
    ) {
        account(args)?;
    }
    // profile-init, Google, account and retained Google draft commands must
    // name their profile explicitly and cannot use the demo profile.
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
    // Resolve a developer client file now: a bad path fails before connecting,
    // and the engine only ever receives an absolute path.
    let google_client_path = if let Command::Google {
        command:
            Google::Connect {
                client_config: Some(client_config),
                ..
            }
            | Google::Restore {
                client_config: Some(client_config),
                ..
            },
    } = &args.command
    {
        Some(
            std::fs::canonicalize(client_config)
                .map_err(|_| invalid("Choose an existing desktop client JSON file"))?,
        )
    } else {
        None
    };
    // Read and parse the Google Calendar and draft-discard JSON files up front,
    // each with its own size limit.
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
    let discard_buffer = if let Command::Draft {
        command: Draft::Discard {
            buffer_file: Some(path),
            ..
        },
    } = &args.command
    {
        let value: Value = serde_json::from_slice(&read_input(path, 768 * 1024)?)
            .map_err(|_| invalid("Draft buffer must be JSON"))?;
        if !value.is_object() {
            return Err(invalid("Draft buffer must be a JSON object"));
        }
        Some(value)
    } else {
        None
    };
    // Build and validate the remaining requests now; dispatch below only
    // unwraps them.
    let google_composition = google_composition_request(&args.command)?;
    let draft_source = draft_source_request(&args.command)?;
    let account_lifecycle = account_lifecycle_request(&args.command)?;
    let notification = notification_request(&args.command)?;
    let calendar_search = calendar_search_request(&args.command)?;
    // `google send --undo` waits here, before connecting, so Ctrl+C in the
    // six-second window cancels without any engine or Google work.
    if matches!(
        &args.command,
        Command::Google {
            command: Google::Send { undo: true, .. }
        }
    ) {
        if args.account.is_none() {
            return Err(invalid(
                "Choose an account before starting the Send undo interval",
            ));
        }
        #[cfg(windows)]
        let mut interrupt = {
            let unavailable = || Failure {
                code: "io".into(),
                message: "Cannot listen for Send Undo. Nothing was submitted.".into(),
                recovery: RecoveryProofs::default(),
            };
            let signal = tokio::signal::windows::ctrl_c().map_err(|_| unavailable())?;
            // Launchers can pass down the per-process attribute that ignores
            // Ctrl+C. Register the listener first, then enable delivery only
            // for this CLI process, before advertising the Undo interval.
            // SAFETY: with a null handler and FALSE, SetConsoleCtrlHandler only
            // clears this process's ignore-Ctrl+C flag; no pointer is passed.
            if unsafe { windows_sys::Win32::System::Console::SetConsoleCtrlHandler(None, 0) } == 0 {
                return Err(unavailable());
            }
            signal
        };
        eprintln!("Sending in six seconds. Press Ctrl+C to keep the draft.");
        #[cfg(windows)]
        let cancelled = tokio::select! { biased; _=interrupt.recv()=>true, _=tokio::time::sleep(Duration::from_secs(6))=>false };
        #[cfg(not(windows))]
        let cancelled = tokio::select! { biased; _=tokio::signal::ctrl_c()=>true, _=tokio::time::sleep(Duration::from_secs(6))=>false };
        if cancelled {
            return Err(Failure {
                code: "cancelled".into(),
                message: "Send undone before connecting. Your saved draft is kept.".into(),
                recovery: RecoveryProofs::default(),
            });
        }
    }
    // Connect, starting an engine for the profile if none is running. The
    // engine is shared with the GUI and exits shortly after its last client
    // disconnects, so session-bound work (sync, sign-in, `assist run`) lasts
    // only while this process stays connected. The demo and live variants
    // refuse a profile of the other kind; an explicit --profile for other
    // commands may be either.
    let mut client = if args.demo_profile {
        Client::connect_demo(&profile, engine.as_deref()).await?
    } else if live_command || args.profile.is_none() {
        Client::connect_live(&profile, engine.as_deref()).await?
    } else {
        Client::connect(&profile, engine.as_deref()).await?
    };
    let scope = args.account.as_deref();
    let reply = match &args.command {
        // Accounts and account lifecycle.
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
        // Status. For profile-init, the live connection above has already
        // created or opened the live profile; status reports it.
        Command::Status | Command::ProfileInit => client.call("status", None, json!({})).await?,
        // Calendar search, notifications and reminders. These read every
        // account; only a reminder acknowledgement is account-scoped.
        Command::Calendar {
            command: Some(CalendarCommand::Search { .. }),
            ..
        } => {
            client
                .call(
                    "calendar_search",
                    None,
                    calendar_search.expect("validated search request"),
                )
                .await?
        }
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
        // Google: setup, connection and sync-session queries.
        Command::Google {
            command: Google::SetupStatus,
        } => {
            client
                .call("google_auth_configuration", None, json!({}))
                .await?
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
        // Google drafts and Send; their requests were built before connecting.
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
                | Google::Sent { .. }
                | Google::SentMessage { .. }
                | Google::SendReconcile { .. }
                | Google::SendAbandon { .. },
        } => {
            let (method, params) = google_composition
                .as_ref()
                .expect("Google composition request validated before connection");
            client.call(method, scope, params.clone()).await?
        }
        // Google labels, live Spam and remote search, Trash and label binding.
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
            command: Google::Spam { page_token },
        } => {
            let params = match page_token {
                Some(token) => json!({"page_token":token}),
                None => json!({}),
            };
            client.call("google_spam_list", scope, params).await?
        }
        Command::Google {
            command: Google::SpamShow { id },
        } => {
            client
                .call("google_spam_message", scope, json!({"id":id}))
                .await?
        }
        Command::Google {
            command: Google::Search { query, folder },
        } => {
            client
                .call(
                    "google_remote_search",
                    scope,
                    json!({"query":query,"folder":folder}),
                )
                .await?
        }
        Command::Google {
            command: Google::Thread { thread_id },
        } => {
            client
                .call("google_remote_thread", scope, json!({"thread_id":thread_id}))
                .await?
        }
        Command::Google {
            command: Google::Attachment { id },
        } => {
            client
                .call("google_remote_attachment", scope, json!({"id":id}))
                .await?
        }
        Command::Google {
            command: Google::BinCount { folder },
        } => {
            client
                .call("google_bin_counts", scope, json!({"folder":folder}))
                .await?
        }
        Command::Google {
            command: Google::NotSpam { thread_id },
        } => {
            client
                .call("google_not_spam", scope, json!({"thread_id":thread_id}))
                .await?
        }
        Command::Google {
            command: Google::DeleteForever { folder, id, yes },
        } => {
            if !yes {
                return Err(invalid("Deleting for good cannot be undone; add --yes"));
            }
            let target = if folder == "spam" {
                json!({"folder":folder,"thread_id":id})
            } else {
                json!({"folder":folder,"conversation_id":id})
            };
            client.call("google_delete_forever", scope, target).await?
        }
        Command::Google {
            command: Google::Empty { folder, yes },
        } => {
            if !yes {
                return Err(invalid("Emptying cannot be undone; add --yes"));
            }
            client
                .call("google_empty", scope, json!({"folder":folder}))
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
            // clap requires exactly one of the three; --unmapped is sent as an
            // explicit null previous link.
            if let Some(name) = create_name {
                params["create_name"] = json!(name);
            } else if *unmapped {
                params["expected_provider_label_id"] = Value::Null;
            } else {
                params["expected_provider_label_id"] = json!(expected_provider_label_id);
            }
            client.call("google_category_bind", scope, params).await?
        }
        // Google Calendar operations.
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
        // Google sync: this process owns a share of the account's sync until
        // Ctrl-C, --seconds, or no account is active any more.
        Command::Google {
            command:
                Google::Sync {
                    refresh,
                    gmail_project_units_per_minute,
                    gmail_account_units_per_minute,
                    calendar_project_units_per_minute,
                    calendar_account_units_per_minute,
                    seconds,
                },
        } => {
            let limits = if [
                gmail_project_units_per_minute,
                gmail_account_units_per_minute,
                calendar_project_units_per_minute,
                calendar_account_units_per_minute,
            ]
            .iter()
            .all(|value| value.is_none())
            {
                json!({})
            } else {
                json!({"gmail_project_units_per_minute":gmail_project_units_per_minute,"gmail_account_units_per_minute":gmail_account_units_per_minute,"calendar_project_units_per_minute":calendar_project_units_per_minute,"calendar_account_units_per_minute":calendar_account_units_per_minute})
            };
            // No overrides means the engine's built-in limits. --refresh wakes a
            // session that already runs with shared limits, so it takes none.
            if *refresh && limits != json!({}) {
                return Err(invalid(
                    "--refresh uses the existing shared limits; omit quota overrides",
                ));
            }
            let reply = client
                .call(
                    if *refresh {
                        "google_sync_refresh"
                    } else {
                        "google_sync_start"
                    },
                    scope,
                    limits,
                )
                .await?;
            result(reply.clone())?;
            emit(args, &reply)?;
            if !args.json {
                eprintln!("Google sync is running in this CLI session. Press Ctrl-C to stop.");
            }
            // Poll status about once a second and print it only when it changes.
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
            // Release only this session's share; the engine stops the account's
            // workers when no other owner remains.
            client.call("google_sync_release", scope, json!({})).await?
        }
        // Google sign-in (connect or restore). The attempt belongs to this
        // engine session: open the browser, then poll until it finishes.
        Command::Google {
            command:
                Google::Connect {
                    reauthorize,
                    email,
                    mail_access,
                    calendar_access,
                    ..
                }
                | Google::Restore {
                    reauthorize,
                    email,
                    mail_access,
                    calendar_access,
                    ..
                },
        } => {
            let mut params = json!({"reauthorize":reauthorize,"access":{"mail":mail_access,"calendar":calendar_access}});
            if let Some(path) = &google_client_path {
                params["client_config_path"] = json!(path
                    .to_str()
                    .ok_or_else(|| invalid("Desktop client path is not valid Unicode"))?);
            }
            if let Some(email) = email {
                params["email"] = json!(email.trim());
            }
            let method = match &args.command {
                Command::Google {
                    command:
                        Google::Restore {
                            expected_revision, ..
                        },
                } => {
                    params["expected_revision"] = json!(expected_revision);
                    "google_auth_restore_begin"
                }
                _ => "google_auth_begin",
            };
            let begin = result(client.call(method, scope, params).await?)?;
            let attempt = begin["attempt_id"]
                .as_str()
                .ok_or_else(|| invalid("Engine omitted the sign-in attempt"))?
                .to_owned();
            let url = begin["authorization_url"]
                .as_str()
                .ok_or_else(|| invalid("Engine omitted Google sign-in"))?;
            // If the browser cannot be opened, cancel the attempt rather than
            // leave it waiting for a sign-in that cannot happen.
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
                // Poll every 500 ms. Give up after 330 s, a little past the
                // engine's 300 s attempt lifetime, or at Ctrl-C.
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
        // Local model.
        Command::Assist { command } => 'assist: {
            // Pause and resume edit this account's entry in the shared model
            // settings, saved against the revision just read.
            if matches!(command, Assist::Pause | Assist::Resume) {
                let account = account(args)?;
                let state = self::result(client.call("assist_settings_get", None, json!({})).await?)?;
                let mut settings = state["settings"].clone();
                let policy = settings["accounts"]
                    .as_object_mut()
                    .ok_or_else(|| invalid("Invalid local model settings"))?
                    .entry(account)
                    .or_insert_with(|| json!({"classification":true,"summaries":true}));
                policy["paused"] = json!(matches!(command, Assist::Pause));
                break 'assist client
                    .call(
                        "assist_settings_save",
                        None,
                        json!({"settings":settings,"expected_revision":state["revision"]}),
                    )
                    .await?;
            }
            let (method, params) = match command {
                Assist::Discover { endpoint, provider } => {
                    let mut params = json!({});
                    if let Some(endpoint) = endpoint {
                        params["endpoint"] = json!(endpoint);
                    }
                    if let Some(provider) = provider {
                        params["provider"] = json!(provider);
                    }
                    ("assist_discover", params)
                }
                Assist::Settings => ("assist_settings_get", json!({})),
                Assist::Backlog => {
                    account(args)?;
                    ("assist_backlog", json!({}))
                }
                // Asked once, so wait for every account's counts.
                Assist::Progress => ("assist_progress", json!({"complete":true})),
                Assist::CatchUp { range } => {
                    account(args)?;
                    ("assist_catch_up", json!({"range":range}))
                }
                Assist::Pause | Assist::Resume => unreachable!(),
                Assist::Configure {
                    settings_file,
                    expected_revision,
                } => {
                    let settings: Value =
                        serde_json::from_slice(&read_input(settings_file, 48 * 1024)?)
                            .map_err(|_| invalid("Settings must be JSON"))?;
                    (
                        "assist_settings_save",
                        json!({"settings":settings,"expected_revision":expected_revision}),
                    )
                }
                Assist::Test {
                    sample_case,
                    settings_file,
                } => {
                    let mut p = json!({"sample":true,"sample_case":sample_case});
                    if let Some(path) = settings_file {
                        p["settings"] = serde_json::from_slice(&read_input(path, 48 * 1024)?)
                            .map_err(|_| invalid("Settings must be JSON"))?;
                    }
                    ("assist_start", p)
                }
                Assist::Analyze {
                    conversation_id,
                    message_id,
                } => {
                    account(args)?;
                    (
                        "assist_start",
                        json!({"conversation_id":conversation_id,"message_id":message_id}),
                    )
                }
                Assist::Thread { conversation_id } => (
                    "assist_start",
                    json!({"task":"thread","conversation_id":conversation_id}),
                ),
                Assist::Results { conversation_id } => {
                    ("assist_results", json!({"conversation_id":conversation_id}))
                }
                Assist::Correct {
                    conversation_id,
                    message_id,
                    kind,
                    expected_revision,
                } => (
                    "assist_correct",
                    json!({"conversation_id":conversation_id,"message_id":message_id,"kind":kind,"expected_revision":expected_revision}),
                ),
                Assist::Accept {
                    conversation_id,
                    message_id,
                    category_id,
                    request_id,
                } => (
                    "assist_accept",
                    json!({"conversation_id":conversation_id,"message_id":message_id,"category_id":category_id,"request_id":request_id}),
                ),
                Assist::Run => ("assist_watch", json!({})),
            };
            let mut result = client.call(method, scope, params).await?;
            // The queue keeps working only while this session is connected, so
            // `assist run` holds it open until Ctrl-C.
            if matches!(command, Assist::Run) {
                self::result(result.clone())?;
                tokio::signal::ctrl_c().await?;
                json!({"version":1,"ok":true,"result":{"status":"stopped"}})
            } else if method == "assist_start" {
                // `test`, `analyze` and `thread` start a job: poll it every 250 ms
                // until it leaves queued/running, and cancel it on Ctrl-C.
                self::result(result.clone())?;
                let job = result["result"]["job_id"].clone();
                loop {
                    tokio::select! {_=tokio::signal::ctrl_c()=>{result=client.call("assist_cancel",None,json!({"job_id":job})).await?;break;},_=tokio::time::sleep(Duration::from_millis(250))=>{}}
                    result = client
                        .call("assist_job", None, json!({"job_id":job}))
                        .await?;
                    self::result(result.clone())?;
                    if !["queued", "running"]
                        .contains(&result["result"]["status"].as_str().unwrap_or_default())
                    {
                        break;
                    }
                }
                // A failed job is the model's failure, not bad input: keep the
                // job's own error code so the exit status isn't 2.
                if result["result"]["status"] == "failed" {
                    let error = &result["result"]["error"];
                    return Err(Failure {
                        code: error["code"].as_str().unwrap_or("assist_failed").into(),
                        message: error["message"]
                            .as_str()
                            .unwrap_or("Local inference failed")
                            .into(),
                        recovery: RecoveryProofs::default(),
                    });
                }
                // Without --json, a conversation summary prints as its text.
                let saved = &result["result"]["items"][0]["items"][0];
                if let (Assist::Thread { .. }, false, Some(summary)) =
                    (command, args.json, saved["output"]["summary"].as_str())
                {
                    let mut out = io::stdout().lock();
                    let written = writeln!(out, "{summary}").and_then(|()| {
                        if saved["partial"] == true {
                            writeln!(out, "(Partial: older messages were shortened or left out.)")
                        } else {
                            Ok(())
                        }
                    });
                    return match written {
                        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
                        other => other.map_err(Failure::from),
                    };
                }
                result
            } else {
                result
            }
        }
        // Preferences and account settings.
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
            mail_retention,
            mail_access,calendar_access,
            analyze_history,
            color,
            name,
            signature_file,
            expected_revision,
            request_id,
        } => {
            let signature = String::from_utf8(read_input(signature_file, 16 * 1024)?)
                .map_err(|_| invalid("Signature must be UTF-8"))?;
            let mut params=json!({"name":name,"signature_text":signature,"expected_revision":expected_revision,"request_id":request_id});
            if let Some(retention)=mail_retention {params["mail_retention"]=json!(retention);}
            if let Some(analyze)=analyze_history {params["analyze_history"]=json!(analyze);}
            if let Some(color)=color {params["color"]=json!(color);}
            // clap requires both access flags together. `none` turns that service
            // off; the engine still wants an access level for it, so read is sent.
            if let (Some(mail),Some(calendar))=(mail_access,calendar_access) {
                params["sync"]=json!({"mail":mail!="none","calendar":calendar!="none","mail_access":if mail=="none" {"read"} else {&mail},"calendar_access":if calendar=="none" {"read"} else {&calendar}});
            }
            client.call("account_update",scope,params).await?
        }
        // Calendar sources, local calendar writes and the operation journal.
        Command::CalendarSources => client.call("calendar_sources", None, json!({})).await?,
        Command::CalendarWrite { params_file } => {
            let params: Value = serde_json::from_slice(&read_input(params_file, 64 * 1024)?)
                .map_err(|_| invalid("Calendar write must be JSON"))?;
            client.call("calendar_write", scope, params).await?
        }
        Command::CalendarOperations { limit, cursor } => {
            let mut params = json!({"limit":limit});
            // The cursor is the previous page's `continuation` object, passed as JSON text.
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
        // Labels.
        Command::Categories => client.call("categories", scope, json!({})).await?,
        Command::Labels { command } => {
            let (method, params) = match command {
                Labels::List => ("label_catalog", json!({})),
                Labels::Create {
                    name,
                    id,
                    request_id,
                } => (
                    "label_create",
                    json!({"name":name,"id":id,"request_id":request_id}),
                ),
                Labels::Describe {
                    id,
                    description,
                    expected_description,
                    request_id,
                } => (
                    "label_describe",
                    json!({"id":id,"description":description,"expected_description":expected_description,"request_id":request_id}),
                ),
                Labels::Remove {
                    id,
                    expected_name,
                    expected_provider_label_id,
                    request_id,
                } => (
                    "label_remove",
                    json!({"id":id,"expected_name":expected_name,"expected_provider_label_id":expected_provider_label_id,"request_id":request_id}),
                ),
                Labels::AutoRead {
                    id,
                    state,
                    expected,
                    request_id,
                } => (
                    "label_auto_read",
                    json!({"id":id,"auto_read":state=="on","expected_auto_read":expected=="on","request_id":request_id}),
                ),
                Labels::Spam {
                    id,
                    state,
                    expected,
                    request_id,
                } => (
                    "label_auto_spam",
                    json!({"id":id,"auto_spam":state=="on","expected_auto_spam":expected=="on","request_id":request_id}),
                ),
                Labels::Sync { request_id } => {
                    ("google_labels_sync", json!({"request_id":request_id}))
                }
            };
            let mut reply = client.call(method, scope, params).await?;
            // Label sync runs as an engine job: poll it every 250 ms while it
            // runs, and cancel it on Ctrl-C.
            if method == "google_labels_sync" {
                let job = result(reply.clone())?["job_id"].clone();
                while result(reply.clone())?["state"] == "running" {
                    let interrupted = tokio::select! { _=tokio::signal::ctrl_c()=>true, _=tokio::time::sleep(Duration::from_millis(250))=>false };
                    reply = client
                        .call(
                            if interrupted {
                                "google_labels_sync_cancel"
                            } else {
                                "google_labels_sync_status"
                            },
                            scope,
                            json!({"job_id":job}),
                        )
                        .await?;
                    if interrupted {
                        break;
                    }
                }
            }
            reply
        }
        // Mail.
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
                month_zone,
                collapsed_month,
                cursor,
                limit,
                offset,
            } => {
                let mut params =
                    json!({"field":field,"folder":folder,"limit":limit,"offset":offset});
                if let Some(zone) = month_zone {
                    params["month_zone"] = json!(zone);
                }
                if !collapsed_month.is_empty() {
                    params["collapsed_months"] = json!(collapsed_month);
                }
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
                // The engine refuses a cursor together with an offset.
                if let Some(cursor) = cursor {
                    params.as_object_mut().unwrap().remove("offset");
                    params["cursor"] = json!(cursor);
                }
                client.call("conversations", scope, params).await?
            }
            Mail::Read { id } => client.call("conversation", scope, json!({"id":id})).await?,
            Mail::Source { id, source_version } => {
                client
                    .call(
                        "message_source",
                        scope,
                        json!({"id":id,"source_version":source_version}),
                    )
                    .await?
            }
            Mail::Invite { id, source_version } => {
                client
                    .call(
                        "message_invite",
                        scope,
                        json!({"id":id,"source_version":source_version}),
                    )
                    .await?
            }
            Mail::Unsubscribe { id, source_version } => {
                let params = json!({"id":id,"source_version":source_version});
                let source = result(client.call("message_source", scope, params.clone()).await?)?;
                match source["unsubscribe"]["method"].as_str() {
                    Some("mailto") => {
                        client.call("google_unsubscribe_mail", scope, params).await?
                    }
                    Some(_) => client.call("message_unsubscribe", scope, params).await?,
                    None => {
                        return Err(invalid("This message has no way to unsubscribe"))
                    }
                }
            }
            Mail::Html { id, source_version } => {
                client
                    .call("message_html", scope, json!({"id":id,"source_version":source_version}))
                    .await?
            }
            Mail::Images { id, source_version, load } => {
                message_images(&mut client, scope, id, source_version, *load).await?
            }
            Mail::Senders {
                query,
                folder,
                role,
                limit,
            } => {
                let mut params = json!({"query":query,"folder":folder,"limit":limit});
                if let Some(role) = role {
                    params["role"] = json!(role);
                }
                client.call("search_senders", scope, params).await?
            }
            Mail::Sender { address } => {
                client
                    .call("sender_summary", scope, json!({"email":address}))
                    .await?
            }
            Mail::Recipients {
                query,
                exclude,
                limit,
            } => {
                client
                    .call(
                        "recipient_suggestions",
                        scope,
                        json!({"query":query,"exclude":exclude,"limit":limit}),
                    )
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
                // Act on exactly the messages this read returns, so a reply that
                // arrives in the meantime is left alone. The read itself does not
                // mark anything reviewed; omitted revision and request ID default
                // to the snapshot's revision and a random token.
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
            Mail::Undo { id,undo_request_id,expected_revision,request_id } => client.call("mail_undo",scope,json!({"id":id,"undo_request_id":undo_request_id,"expected_revision":expected_revision,"request_id":request_id.clone().unwrap_or_else(fm_ipc::random_token)})).await?,
            Mail::Filing {
                id,
                action,
                expected_revision,
                request_id,
            } => {
                // Same snapshot rule as review/done above.
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
        // Drafts.
        Command::Draft { command } => match command {
            Draft::Transfer { draft_id,target_account,expected_revision,expected_google_revision,request_id,keep_original } => client.call("draft_transfer",scope,json!({"draft_id":draft_id,"target_account":target_account,"expected_revision":expected_revision,"expected_google_revision":expected_google_revision,"request_id":request_id,"keep_original":keep_original})).await?,
            Draft::TransferRetry { transfer_request_id, request_id } => client.call("draft_transfer_retry",scope,json!({"transfer_request_id":transfer_request_id,"request_id":request_id})).await?,
            Draft::TransferStatus { request_id } => client.call("draft_transfer_status",scope,json!({"request_id":request_id})).await?,
            Draft::Sources { .. }
            | Draft::Source { .. }
            | Draft::Authoring { .. }
            | Draft::Copy { .. } => {
                let (method, params) = draft_source
                    .as_ref()
                    .expect("Draft source request validated before connection");
                let reply = client.call(method, scope, params.clone()).await?;
                // With --output, copy the engine's verified export to a new file.
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
                ..
            } => {
                let mut params = target.params();
                params["expected_revision"] = json!(expected_revision);
                params["request_id"] = json!(request_id);
                if let Some(buffer) = discard_buffer { params["buffer"] = buffer; }
                client.call("draft_discard", scope, params).await?
            }
            Draft::DiscardStatus { request_id } => client.call("draft_discard_status",scope,json!({"request_id":request_id})).await?,
            Draft::UndoDiscard { draft_id,discard_request_id,expected_revision,request_id } => client.call("draft_undo_discard",scope,json!({"draft_id":draft_id,"discard_request_id":discard_request_id,"expected_revision":expected_revision,"request_id":request_id})).await?,
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
                // The current draft supplies the default --expected-revision. The
                // first save of a legacy conversation reply also binds the source
                // message that `draft_get` reported.
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
                // Only the three recipient arrays are accepted; an array left out
                // keeps the saved recipients.
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
                // Ids replace the selection and --clear-attachments sends an empty
                // list; with neither, the saved selection is kept.
                if *clear_attachments && !attachment.is_empty() {
                    return Err(invalid("Choose attachments or --clear-attachments"));
                }
                if *clear_attachments || !attachment.is_empty() {
                    params["attachment_ids"] = json!(attachment);
                }
                client.call("draft_save", scope, params).await?
            }
        },
        // Calendar range read: one account, or every account with --combined.
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
        // Attachments, links, the image proxy and raw protocol calls.
        Command::Attachment { id, output } => {
            let reply = client.call("attachment", scope, json!({"id":id})).await?;
            if let Some(output) = output {
                copy_export(&reply, output)?;
            }
            reply
        }
        Command::Links { command } => links(&mut client, scope, command).await?,
        Command::Proxy { command } => proxy(&mut client, command).await?,
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
        // Watch: an explicit session that can outlive the GUI. Poll every
        // 500 ms and print status only when the store revision changes.
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
    // An engine error reply becomes a failure here, before anything is printed.
    result(reply.clone())?;
    emit(args, &reply)
}

/// Wraps a result the CLI assembled itself in a success envelope, so `emit`
/// prints it like an engine reply.
fn ok(value: Value) -> Value {
    json!({"ok":true,"result":value})
}

/// A request ID for a change the CLI makes on its own (blocklist and proxy
/// saves), made from the kind, process id and current time in nanoseconds.
fn cli_request_id(kind: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("cli-{kind}-{}-{nanos}", std::process::id())
}

/// Saves one preferences key against the revision it was read at.
async fn save_preference(
    client: &mut Client,
    key: &str,
    value: Value,
    kind: &str,
) -> Result<Value, Failure> {
    let current = result(client.call("preferences_get", None, json!({})).await?)?;
    let mut patch = serde_json::Map::new();
    patch.insert(key.to_owned(), value);
    Ok(client
        .call(
            "preferences_save",
            None,
            json!({"preferences":patch,"expected_revision":current["revision"],"request_id":cli_request_id(kind)}),
        )
        .await?)
}

/// The form blocklist patterns are stored in: lowercase host without scheme,
/// path or `www.`; the engine checks the rest.
fn block_pattern(input: &str) -> String {
    let mut value = input.trim().to_lowercase();
    if let Some(index) = value.find("://") {
        value = value[index + 3..].to_owned();
    }
    if let Some(index) = value.find(['/', '?', '#']) {
        value.truncate(index);
    }
    let wildcard = value.starts_with("*.");
    let mut name = value.trim_start_matches("*.");
    while let Some(rest) = name.strip_prefix("www.") {
        name = rest;
    }
    if wildcard {
        format!("*.{name}")
    } else {
        name.to_owned()
    }
}

/// Runs a `links` subcommand. The blocklist is the `blocked_hosts` preference:
/// block and unblock rewrite the whole list against the revision just read,
/// and fail with `invalid_request` when there is nothing to change.
async fn links(
    client: &mut Client,
    scope: Option<&str>,
    command: &LinkCommand,
) -> Result<Value, Failure> {
    match command {
        LinkCommand::Summary { host, conversation } => {
            let mut params = json!({"host":host});
            if let Some(conversation) = conversation {
                params["conversation"] = json!(conversation);
            }
            Ok(client.call("link_host_summary", scope, params).await?)
        }
        LinkCommand::Blocked => {
            let preferences = result(client.call("preferences_get", None, json!({})).await?)?;
            let blocked = preferences["preferences"]["blocked_hosts"].clone();
            let patterns: Vec<Value> = blocked
                .as_array()
                .into_iter()
                .flatten()
                .map(|e| e["pattern"].clone())
                .collect();
            let counts = result(
                client
                    .call("link_pattern_counts", None, json!({"patterns":patterns}))
                    .await?,
            )?;
            Ok(ok(
                json!({"blocked_hosts":blocked,"counts":counts["counts"],"complete":counts["complete"]}),
            ))
        }
        LinkCommand::Block { pattern } => {
            let pattern = block_pattern(pattern);
            let preferences = result(client.call("preferences_get", None, json!({})).await?)?;
            let mut list = preferences["preferences"]["blocked_hosts"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if list.iter().any(|e| e["pattern"] == pattern.as_str()) {
                return Err(invalid("That domain is already blocked"));
            }
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or_default();
            list.insert(
                0,
                json!({"pattern":pattern,"source":"settings","added_at":now}),
            );
            save_preference(client, "blocked_hosts", Value::Array(list), "block").await
        }
        LinkCommand::Unblock { pattern } => {
            let pattern = block_pattern(pattern);
            let preferences = result(client.call("preferences_get", None, json!({})).await?)?;
            let mut list = preferences["preferences"]["blocked_hosts"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let before = list.len();
            list.retain(|e| e["pattern"] != pattern.as_str());
            if list.len() == before {
                return Err(invalid("That domain is not blocked"));
            }
            save_preference(client, "blocked_hosts", Value::Array(list), "unblock").await
        }
    }
}

/// Runs a `proxy` subcommand. Host, port and user name live in the
/// `remote_proxy` preference; the password is handed to the engine for the
/// system keychain and never printed.
async fn proxy(client: &mut Client, command: &ProxyCommand) -> Result<Value, Failure> {
    match command {
        ProxyCommand::Show => {
            let preferences = result(client.call("preferences_get", None, json!({})).await?)?;
            let saved = result(
                client
                    .call("remote_proxy_password", None, json!({"op":"status"}))
                    .await?,
            )?;
            let mut value = preferences["preferences"]["remote_proxy"].clone();
            value["password_saved"] = saved["saved"].clone();
            Ok(ok(value))
        }
        ProxyCommand::Set {
            host,
            username,
            password_stdin,
        } => {
            // A trailing all-digit `:port` sets the port; otherwise use 443.
            let (name, port) = match host.rsplit_once(':') {
                Some((name, port))
                    if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) =>
                {
                    (
                        name.to_owned(),
                        port.parse::<u16>()
                            .map_err(|_| invalid("Use a port from 1 to 65535"))?,
                    )
                }
                _ => (host.clone(), 443),
            };
            // At most 4 KiB, read before anything is saved, without the trailing
            // line ending.
            let password = if *password_stdin {
                let mut value = String::new();
                io::stdin().take(4096).read_to_string(&mut value)?;
                Some(value.trim_end_matches(['\r', '\n']).to_owned())
            } else {
                None
            };
            let reply = save_preference(
                client,
                "remote_proxy",
                json!({"enabled":true,"host":name.trim().to_lowercase(),"port":port,"username":username}),
                "proxy",
            )
            .await?;
            result(reply.clone())?;
            // An empty password leaves any saved one in place.
            if let Some(password) = password.filter(|p| !p.is_empty()) {
                result(
                    client
                        .call(
                            "remote_proxy_password",
                            None,
                            json!({"op":"set","password":password}),
                        )
                        .await?,
                )?;
            }
            Ok(reply)
        }
        ProxyCommand::Clear => {
            let reply = save_preference(
                client,
                "remote_proxy",
                json!({"enabled":false,"host":"","port":443,"username":""}),
                "proxy",
            )
            .await?;
            result(reply.clone())?;
            result(
                client
                    .call("remote_proxy_password", None, json!({"op":"clear"}))
                    .await?,
            )?;
            Ok(reply)
        }
        // A test request can take longer than the ordinary 15 s reply limit.
        ProxyCommand::Test => Ok(client
            .call_within(
                "remote_proxy_test",
                None,
                json!({}),
                Duration::from_secs(30),
            )
            .await?),
        ProxyCommand::Cache { clear } => Ok(client
            .call(
                "remote_image_cache",
                None,
                json!({"op": if *clear { "clear" } else { "stats" }}),
            )
            .await?),
    }
}

/// The reader's image states for one message. Remote images are fetched only
/// with `load`; otherwise only the disk cache and the message's own parts are used.
async fn message_images(
    client: &mut Client,
    scope: Option<&str>,
    id: &str,
    version: &str,
    load: bool,
) -> Result<Value, Failure> {
    let view = result(
        client
            .call(
                "message_html",
                scope,
                json!({"id":id,"source_version":version}),
            )
            .await?,
    )?;
    let mut images = Vec::new();
    for resource in view["resources"].as_array().into_iter().flatten() {
        let remote = resource["kind"] == "remote";
        let state = if resource["kind"] == "insecure" {
            json!({"state":"insecure"})
        } else {
            let params = json!({"id":id,"source_version":version,"resource":resource["id"],"cached_only":remote && !load});
            result(
                client
                    .call_within("message_image", scope, params, Duration::from_secs(30))
                    .await?,
            )?
        };
        images.push(json!({"id":resource["id"],"kind":resource["kind"],"host":resource["host"],"alt":resource["alt"],"result":state}));
    }
    Ok(ok(
        json!({"id":id,"source_version":version,"has_html":view["has_html"],"trackers":view["trackers"],"images":images}),
    ))
}

/// Prints a successful reply to stdout.
///
/// With `--json` this is the whole reply envelope on one line, which is the
/// stable output for scripts. Otherwise accounts, conversation lists and
/// conversations get a short text form and everything else is pretty-printed
/// JSON of the `result`. A closed stdout is either ignored or returned as
/// `broken_pipe`, which `main` treats as a quiet exit.
fn emit(args: &Args, reply: &Value) -> Result<(), Failure> {
    let mut out = io::stdout().lock();
    let result = &reply["result"];
    let write = if args.json {
        writeln!(out, "{reply}")
    } else if result["configured"].is_boolean() && result["accounts"].is_array() {
        // Sync status also has an `accounts` array; print it whole rather than
        // as an account list.
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
    #[test]
    fn send_undo_preserves_exact_submission_request() {
        let input = [
            "fm",
            "--account",
            "original-account",
            "google",
            "send",
            "--draft-id",
            "original-draft",
            "--expected-revision",
            "7",
            "--expected-google-revision",
            "11",
            "--request-id",
            "exact-send",
        ];
        let immediate = Args::try_parse_from(input).unwrap();
        let delayed = Args::try_parse_from(input.into_iter().chain(["--undo"])).unwrap();
        assert!(matches!(
            &delayed.command,
            Command::Google {
                command: Google::Send { undo: true, .. }
            }
        ));
        assert_eq!(
            google_composition_request(&immediate.command).unwrap(),
            google_composition_request(&delayed.command).unwrap()
        );
        assert_eq!(
            google_composition_request(&delayed.command)
                .unwrap()
                .unwrap(),
            (
                "google_send",
                json!({"draft_id":"original-draft", "expected_revision":7,
                "expected_google_revision":11, "request_id":"exact-send"})
            )
        );
    }

    #[test]
    fn google_sync_accepts_no_quota_setup() {
        let args = Args::try_parse_from([
            "fm",
            "--account",
            "test",
            "google",
            "sync",
            "--seconds",
            "1",
        ])
        .unwrap();
        match args.command {
            Command::Google {
                command:
                    Google::Sync {
                        gmail_project_units_per_minute,
                        gmail_account_units_per_minute,
                        calendar_project_units_per_minute,
                        calendar_account_units_per_minute,
                        ..
                    },
            } => {
                assert_eq!(
                    [
                        gmail_project_units_per_minute,
                        gmail_account_units_per_minute,
                        calendar_project_units_per_minute,
                        calendar_account_units_per_minute
                    ],
                    [None; 4]
                );
            }
            _ => panic!("expected sync"),
        }
    }

    #[test]
    fn calendar_search_routes_global_explicit_range_without_notification_or_provider_work() {
        let args = Args::try_parse_from([
            "fm",
            "calendar",
            "search",
            "Élodie Room",
            "--from",
            "2026-09-12",
            "--to",
            "2026-09-15",
            "--limit",
            "7",
        ])
        .unwrap();
        assert_eq!(
            calendar_search_request(&args.command).unwrap(),
            Some(json!({"query":"Élodie Room","from":"2026-09-12","to":"2026-09-15","limit":7}))
        );
        assert!(notification_request(&args.command).unwrap().is_none());
        assert!(args.account.is_none());
        let around = Args::try_parse_from([
            "fm",
            "calendar",
            "search",
            "with:maya",
            "--around",
            "2026-09-12",
        ])
        .unwrap();
        assert_eq!(
            calendar_search_request(&around.command).unwrap(),
            Some(json!({"query":"with:maya","around":"2026-09-12","limit":100}))
        );
        for tail in [
            vec!["calendar", "search", "term"],
            vec!["calendar", "search", "term", "--from", "2026-09-12"],
            vec![
                "calendar",
                "search",
                "term",
                "--around",
                "2026-09-12",
                "--from",
                "2026-09-12",
            ],
            vec![
                "calendar",
                "--combined",
                "search",
                "term",
                "--from",
                "2026-09-12",
                "--to",
                "2026-09-15",
            ],
        ] {
            assert!(Args::try_parse_from(std::iter::once("fm").chain(tail)).is_err());
        }
    }
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
        assert_eq!(
            std::fs::read_dir(root.path()).unwrap().count(),
            1,
            "no staged copy is left behind"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn verified_export_keeps_the_downloaded_mark() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("invoice.pkg");
        let output = root.path().join("saved.pkg");
        std::fs::write(&source, b"installer").unwrap();
        let mark = "0081;00000000;fuckMail;";
        let xattr = |args: &[&str]| {
            std::process::Command::new("/usr/bin/xattr")
                .args(args)
                .output()
                .unwrap()
        };
        assert!(
            xattr(&["-w", "com.apple.quarantine", mark, source.to_str().unwrap()])
                .status
                .success()
        );
        copy_export(&json!({"ok":true,"result":{"path":source}}), &output).unwrap();
        let copied = xattr(&["-p", "com.apple.quarantine", output.to_str().unwrap()]);
        assert_eq!(String::from_utf8_lossy(&copied.stdout).trim(), mark);
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
    #[test]
    fn assist_thread_takes_one_conversation_and_no_message() {
        let args = Args::try_parse_from([
            "fm",
            "--account",
            "windscribe",
            "--json",
            "assist",
            "thread",
            "windscribe-thread-000",
        ])
        .unwrap();
        assert!(matches!(
            &args.command,
            Command::Assist {
                command: Assist::Thread { conversation_id }
            } if conversation_id == "windscribe-thread-000"
        ));
        for tail in [
            vec!["assist", "thread"],
            vec!["assist", "thread", "c1", "--message-id", "m1"],
            vec!["assist", "thread", "c1", "c2"],
        ] {
            assert!(
                Args::try_parse_from(std::iter::once("fm").chain(tail.clone())).is_err(),
                "{tail:?}"
            );
        }
    }
}
