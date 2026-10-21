# Local protocol

The desktop app (`fuckmail-gui`) and the `fm` CLI never open a profile's SQLite store. They send JSON requests to `fm-engine`, the one process that owns the profile, over a private pipe or socket. This page is the reference for that protocol. The reasons behind its design are recorded in [decisions](decisions.md).

The code wins over this page: routing is in [`crates/fm-engine/src/main.rs`](../crates/fm-engine/src/main.rs), store methods in `Store::handle` ([`crates/fm-core/src/lib.rs`](../crates/fm-core/src/lib.rs)), framing and discovery in [`crates/fm-ipc/src/lib.rs`](../crates/fm-ipc/src/lib.rs). `fm call METHOD --params-file FILE` (or `-` for stdin) sends any request and prints the reply.

## Overview

- **One engine per profile.** A profile is a directory. The engine holds an exclusive lock on `store.lock`; a second engine for the same profile gets `store_in_use` and exits.
- **Thin clients.** Every read and write is a request. Rust clients use [`fm_ipc::Client`](../crates/fm-ipc/src/lib.rs); the GUI speaks the same protocol from C++.
- **Engine on demand.** A client that finds no engine starts one; it exits soon after its last client leaves.

[Architecture](architecture.md) covers processes, storage and sync in more depth.

## Transport

### Endpoint

- **Windows:** named pipe `\\.\pipe\fuckmail-<32 hex>`, from SHA-256 of the lowercased profile path. Each instance has a protected user-only DACL and rejects remote clients.
- **macOS, Linux:** Unix socket `<profile>/engine.sock`, mode 0600, or `/tmp/fuckmail-<uid>/<24 hex>.sock` in a private 0700 directory when the profile path is too long for a socket address.

There is no TCP or HTTP listener. (Google sign-in separately opens a temporary `127.0.0.1` callback.)

### Discovery

Once it has opened the store and is listening, the engine writes `<profile>/engine.json`:

```json
{"protocol":1,"pid":4242,"endpoint":"\\\\.\\pipe\\fuckmail-…","token":"<64 hex characters>"}
```

- `token` is 32 random bytes, new on every start. It is a credential: never log it or put it on a command line.
- The file is private (user-only ACL on Windows, 0600 on Unix). Clients refuse a symlink, a file over 4 KiB, one not private to the user (Unix), another protocol or a token that is not 64 characters, as hard errors.
- The profile directory must be private: on Unix user-owned with mode 0700; on Windows a reparse point or foreign owner is refused and the DACL is reset to user-only.
- An engine that permanently refuses the profile writes `engine-refusal.json` `{pid, code, message}` for the client that launched it. On exit it removes `engine.json` if it still names its PID.

### Connecting and launching

Clients loop for at most 30 seconds:

1. Read `engine.json`.
2. Connect and check the peer before sending anything. Unix: its UID must be this user and, where reported, its PID must equal `pid`. Windows: the pipe server's PID must equal `pid` and its process must belong to this user.
3. Send `hello` with the token.
4. If discovery is missing or the connection fails transiently, start `fm-engine --profile <dir>` (plus `--live-profile` or `--demo-profile` when a kind is required) with null stdio and no console. At most 10 launches, backing off 1, 2, then 4 seconds. A launch that loses the race to a running engine exits quietly.

The engine is `fm-engine` beside the client; `fm` also takes `--engine PATH` or `FM_ENGINE_PATH`. Engine exit code 12 means the profile is of the other kind (live versus demo); relaunching cannot fix it.

### Handshake

The first frame must be `hello`, within 5 seconds:

```json
{"version":1,"id":"1","method":"hello","params":{"token":"<token>","client":"cli"}}
```

`client` is an informational label (`cli`, `gui`, `gui-operation`). The token is compared in constant time. A wrong `version` gets `protocol_mismatch`, a wrong method or token `unauthorized`, and the engine then closes the connection. Success:

```json
{"version":1,"id":"1","ok":true,"result":{"protocol":1,"schema_version":37,"fixture_revision":0,
 "mode":"live","capabilities":{…},"max_frame_bytes":1048576,"max_page_size":100,"methods":[…]}}
```

`mode` is `live` or `synthetic` (demo); `capabilities` matches `status`. `methods` lists the request methods this engine accepts.

### Framing and limits

- **Frames:** one UTF-8 JSON object per line, ending in `\n`. No length prefix.
- **Size:** at most 1 MiB (1,048,576 bytes) including the newline, both ways. A reply that would not fit becomes `response_too_large`; ask for a smaller page. Attachments, images, large HTML and exports come back as paths to private files in the profile, never inline bytes.
- **Timeouts:** an authenticated connection may idle indefinitely. Once a frame starts, the engine allows 15 seconds to read the rest, and 15 to write each reply. Clients wait 15 seconds for a reply by default.
- **Connections:** at most 16; extras are closed at once.
- **One request per connection.** Replies come in order. A second frame before the reply closes the connection, as a disconnect would. Use more connections for concurrency.
- **Queue:** store requests share a 32-slot writer queue: 16 for client requests (and local-content work), 16 for the engine's background calls. When the client slots are full the reply is `busy`.

### Trust boundary

The boundary is the OS user. The engine accepts only same-user peers, clients verify the engine's PID and user before sending the token, and the token proves the client can read the private profile. Other processes running as the same user are not defended against.

## Envelope

```json
{"version":1,"id":"7","method":"conversations","account":"work","params":{"limit":30,"query":"invoice"}}
{"version":1,"id":"7","ok":true,"result":{…}}
{"version":1,"id":"7","ok":false,"error":{"code":"stale_revision","message":"…"}}
```

| Field | Rule |
| --- | --- |
| `version` | `1`; anything else gets `protocol_mismatch` (the connection stays open). |
| `id` | 1–128 bytes, echoed in the reply. Correlation only, not idempotency. |
| `method` | At most 64 bytes. Unknown methods get `unsupported_method`. |
| `account` | Only for account-scoped methods; see [Account scope](#account-scope). |
| `params` | Always an object (`{}` when empty). Methods refuse fields they do not know. |

Unknown envelope fields are refused; an unparseable frame gets `invalid_request` with `id` `""`. `error.code` is stable snake_case to branch on; `message` is for people.

`error.details` appears only for four markers, each `true`, proving a call did nothing:

| Marker | Methods | Meaning |
| --- | --- | --- |
| `send_invocation_finished_before_dispatch` | `google_send` | This call sent nothing. |
| `send_request_obsolete_without_dispatch` | `google_send` | Obsolete; never sent. |
| `draft_delete_invocation_finished_before_dispatch` | `google_draft_delete` | This call deleted nothing. |
| `draft_authoring_invocation_finished_without_commit` | `google_draft_copy`, `google_draft_adopt` | This call committed nothing (an earlier lost call may still be uncertain). |

No other detail crosses the wire: a `stale_revision` does not carry the current revision.

## Errors

Common codes, grouped. Many methods add their own (the Google, draft, Calendar, remote-content `blocked`/`proxy_unavailable` and local-AI `assistance_*` families); grep `err("`, `failure("` and `error("` in `crates/fm-core`, `crates/fm-engine` and `crates/fm-google` for all of them.

| Group | Codes |
| --- | --- |
| Envelope, engine | `invalid_request`, `invalid_params` (`google_sent` bounds only), `protocol_mismatch`, `unauthorized`, `unsupported_method`, `busy`, `response_too_large`, `cancelled` (connection closed or work stopped), `engine_stopped` |
| Scope, state | `account_required`, `not_found`, `account_removed` (write to an archive), `capability_unavailable` (wrong profile kind, grant or setting), `profile_kind_mismatch`, `permission_denied`, `limit_exceeded` |
| Concurrency | `stale_revision`, `stale_cursor`, `request_id_conflict`, `stale_account_revision`, `stale_source`, `stale_google_revision`, `stale_google_work` (sync work fenced off by a reconnect) |
| Storage, profile | `storage_error`, `content_unavailable` (retained bytes missing or corrupt), `store_in_use`, `schema_mismatch` |
| Google | `sync_not_running`, `reauthorization_required`, `google_rate_limited`, `google_permission_denied`, `google_transport_interrupted`, `login_in_progress` |

### CLI exit codes

`fm` maps the final code to its exit status (`cli_main` in [`crates/fm-cli/src/main.rs`](../crates/fm-cli/src/main.rs)):

| Exit | Codes |
| --- | --- |
| 0 | Success, or stdout closed early (`fm … \| head`) |
| 2 | `invalid_request`, `invalid_params`, `request_id_conflict` |
| 3 | `permission_denied`, `unauthorized`, `authentication_required` |
| 4 | `stale_revision`, `stale_cursor` |
| 5 | `not_found`, `unsupported`, `content_unavailable` |
| 1 | Anything else, including `busy`, `capability_unavailable`, other `stale_*` codes and local `io_error` |

An untrusted profile, discovery file or engine peer is a local `permission_denied`. With `--json` errors print on stdout as an envelope with `id` `"cli"` and only the markers above as `details`; otherwise `fm: code: message` goes to stderr.

## Conventions

### Account scope

Accounts are separate; nothing merges mailboxes.

- **Profile-wide** (omit `account`): `hello`, `status`, `accounts`, `preferences_get`, `preferences_save`, `calendar_sources`, `calendar_combined`, `calendar_search`, `calendar_reminders`, `mail_notifications`, `link_pattern_counts`, `sender_avatar`, `remote_proxy_test`, `remote_proxy_password`, `remote_image_cache`, `assist_settings_get`, `assist_settings_save`, `assist_discover`, `assist_watch`, `assist_job`, `assist_jobs`, `assist_cancel`, and `google_auth_configuration`, `google_auth_status`, `google_auth_cancel` (which refuse an `account`).
- **Optional:** `google_auth_begin` (reconnects that account), `google_sync_status` and `assist_progress` (filter), `calendar_mail_activity`, `assist_start` (samples).
- **Everything else** needs `account`: missing is `account_required`, unknown is `not_found`. Archived accounts stay readable; writes get `account_removed`.

### Idempotency: `request_id`

Mail, label, draft, view, preference and account writes, and every journaled Google write, take `params.request_id` (1–256 bytes, no control characters; some Google operations cap it at 128). A retry with the same ID and params returns the recorded result without acting again, even after later edits or account removal. The same ID with different params is `request_id_conflict`.

- Receipts are keyed by (account, `request_id`) with a SHA-256 of `[method, params]`: key order does not matter, number spelling does. IDs share one namespace per account; `preferences_save` keeps profile-wide receipts.
- After a lost reply, repeat the call with the same `request_id`, or read the status method (`google_send_status`, `google_calendar_operation`, `google_mail_mutation_status`, `account_remove_status`, …). Never retry work that may have reached Google under a new ID. Unknown outcomes are never retried automatically; the `*_reconcile` methods read Google to settle them and never resend.
- No `request_id`: `assist_settings_save`, `assist_correct`, `calendar_reminder_ack`, `remote_proxy_password`, `google_disconnect`, the sync controls, and `google_not_spam`, `google_delete_forever`, `google_empty`, `google_unsubscribe_mail`.

### Concurrency: `expected_revision`

Conversations, drafts, views, account metadata (`metadata_revision`), account lifecycle (`lifecycle.revision`), preferences and model settings carry revisions. A write names the one it read as `expected_revision`; if the object moved on, nothing changes and the call fails with `stale_revision` (`stale_account_revision` for lifecycle, `assistance_stale` for model settings). Re-read, then retry with a new `request_id` if the change still applies.

- Similar fences: `expected_etag` (Calendar), `expected_google_revision` (a draft's Gmail twin), `source_version` / `source_revision` (message and Gmail draft sources, `stale_source`), `expected_catalog_revision` (Gmail labels), `expected_description` / `expected_name` / `expected_auto_read` (labels).
- **Snapshot rule:** `flag`, `set_category` and `mail_filing` name the conversation's `message_ids` as seen plus its revision. A reply arriving in between bumps the revision, so the old action fails instead of covering the new message.
- `status.revision` is a store-wide change counter, not an object revision.

### Paging

`limit` is at most 100 (`max_page_size`).

| Continuation | Methods |
| --- | --- |
| Opaque `cursor` → `next_cursor`, or `offset` → `next_offset` (not both) | `conversations` |
| `offset` → `next_offset` | `drafts`, `capture_history`, `google_sent`, `google_draft_sources` |
| `after` → `next_after`, with `expected_catalog_revision` | `google_labels` |
| `cursor` object → `continuation` | `calendar_operations` |
| `page_token` → `next_page_token` | `google_spam_list` |
| `after` + `since` → `after`, `more` | `mail_notifications` |

A `conversations` cursor fails with `stale_cursor` after an engine restart, if any other param changes (including `limit`), or if mail changed in between. With `live_paging: true` it continues after the last row's sort key instead, so changes neither repeat nor skip rows. Offsets are never checked. Totals cover all matches.

### Change detection

There is no push channel. Clients poll `status` and reload when `revision` (any write), `mail_generation`, `calendar_generation` or `preferences_revision` changes: the GUI once a second, `fm watch` every 500 ms. Progress has its own polls: `google_sync_status`, `google_mail_mutation_status`, `calendar_reminders`, `mail_notifications`, `assist_progress`, `assist_job`, `google_auth_status`.

### Long-running work and cancellation

Store requests run in order on the engine's single writer thread. Google, remote-content, local-content and model methods run on the connection's own task, which is busy until the reply, so the GUI sends slow work over separate single-request connections.

When a connection closes, its queued store requests are skipped, running reads stop and runtime work is cancelled (image fetches cache nothing, sign-ins end `cancelled`, model jobs stop). A store transaction already under way still commits. Exceptions:

- **Awaited to the end:** `account_remove`; local-content work (`draft_save`, `draft_attachment_import`, `draft_transfer` and the other methods routed in [`local_content.rs`](../crates/fm-engine/src/local_content.rs)), which stops preparing but finishes a commit under way; and the journaled Google writes (`google_send`, `google_draft_sync`, `google_draft_resolve`, `google_draft_adopt`, `google_draft_delete`, `google_calendar_write`, `google_calendar_resume` and their `*_reconcile` methods). Nothing undispatched is sent; an admitted request's outcome is journaled under its `request_id`.
- **Abandoned mid-call:** `google_not_spam`, `google_delete_forever`, `google_empty` and `google_unsubscribe_mail` change Gmail without a journal. If the connection drops during one, check Gmail before repeating it.

Sign-in attempts, model jobs, sync ownership and `assist_watch` belong to the connection and end with it.

### Ownership and lifetime

Every open connection counts as an owner, idle or not. The engine exits 350 ms after the last one closes, 10 seconds after start if none ever connects, or on Ctrl-C; queued work alone never keeps it alive. On exit it stops listening, removes `engine.json`, lets admitted Google writes record their outcome, stops sync, drains the writer and closes the store.

- The GUI starts the engine detached and never kills it.
- A plain `fm` command connects, runs and disconnects. Session commands hold their connection and can outlive the GUI: `fm google sync` (sync ownership), `fm watch`, `fm assist run` (`assist_watch`).

## Methods

**Acct:** yes = `account` required, opt = optional, — = omit. **live**: live profiles only (demo profiles get `capability_unavailable`, or `profile_kind_mismatch` from a few store methods). **demo**: demo profiles only. **session**: needs the account's running sync job, else `sync_not_running`. Params are summarised; follow the linked source for full validation.

### Status and profile

| Method | Acct | Description |
| --- | --- | --- |
| `hello` | — | Handshake; see [Handshake](#handshake). |
| `status` | — | `schema_version`, `fixture_revision`, `revision`, `mail_generation`, `calendar_generation`, `preferences_revision`, `profile_kind`, `fixture_today` (demo), `capabilities` (`mail`, `drafts`, `calendar_read`, `send`, `google_auth`, `google_sync`, `models`, …). |

### Accounts and Google connection

`google_auth_begin` returns a URL for the client to open in the browser. The engine waits up to 300 seconds on a `127.0.0.1` callback, verifies the identity, stores tokens in the OS credential store and binds the account. Sign-in does not start sync. Source: [`google_auth.rs`](../crates/fm-engine/src/google_auth.rs), [`account_lifecycle.rs`](../crates/fm-core/src/account_lifecycle.rs).

| Method | Acct | Description |
| --- | --- | --- |
| `accounts` | — | Account rows: identity, counts, coverage, retention, `lifecycle`, Google connection state, scopes and sync summary. `include_removed` adds archives. |
| `account_update` | yes | `name` and `signature_text` (required), `color`, `mail_retention`, `sync`, `analyze_history`; `expected_revision` (`metadata_revision`), `request_id`. |
| `google_auth_configuration` | — | Whether the app's OAuth registration is usable: `{available, error?}`. **live** |
| `google_auth_begin` | opt | Start sign-in, or reconnect `account`. Optional `email`, `reauthorize`, `access` (`{mail, calendar}`: `none`/`read`/`write`), `client_config_path`. Returns `attempt_id`, `authorization_url`. One at a time per engine (`login_in_progress`). **live** |
| `google_auth_restore_begin` | yes | The same for an archived account: `expected_revision` (lifecycle); the Google identity must match. **live** |
| `google_auth_status`, `google_auth_cancel` | — | Read or cancel `attempt_id`: `phase` (`waiting_for_browser` … `connected`, `cancelled`, `failed`), `account`, `error`. Starting connection only. **live** |
| `google_disconnect` | yes | Mark disconnected; stops sync, sign-in and model work. Local data and the token stay. **live** |
| `account_remove_preview` | yes | What removal keeps. |
| `account_remove` | yes | Make the account a read-only local archive, fence its sync and delete its tokens: `retention: "archive"` (only option), `expected_revision` (lifecycle), `request_id`. Touches nothing at Google. **live** |
| `account_remove_status` | yes | Removal and credential cleanup state by `request_id`. |
| `account_archive_delete` | yes | Permanently delete an archive's local data after cleanup: `expected_revision`, `request_id`. Files go at next start. |

### Sync

`google_sync_start` gives an account one job with a worker per granted service. The connections that start or refresh it own it; it ends when the last owner releases it or disconnects, on `google_sync_stop`, or when the account is disconnected, reconnected or removed. **session** methods need a job owned by any connection (the CLI can send while the GUI owns sync) and never start one. Quota limits are engine-wide, fixed by the first start (`quota_configuration_conflict` until restart). Source: [`google_sync.rs`](../crates/fm-engine/src/google_sync.rs).

| Method | Acct | Description |
| --- | --- | --- |
| `google_sync_start` | yes | Start or join the job as an owner. `{}` for default quotas, or all of `gmail_project_units_per_minute`, `gmail_account_units_per_minute`, `calendar_project_units_per_minute`, `calendar_account_units_per_minute`. Returns `{account, started, active}`. Resumes the account's model work. **live** |
| `google_sync_refresh` | yes | Start or join with current limits and wake the mail and Calendar workers now. **live** |
| `google_sync_release` | yes | Stop owning the job; it ends with no owners. Pauses the account's model work. **live** |
| `google_sync_stop` | yes | End the job for all owners. **live** |
| `google_sync_status` | opt | Per-account `active`, `owned`, per-service `progress` and errors, `credential_access_required`, `last_mail_sync_at`, and `limits`. **live** |
| `google_sync_busy` | yes | Fetch and store free/busy for `from`, `to` (≤42 days), `time_zone`. **live, session** |

### Mail lists, reading and search

Source: [`queries.rs`](../crates/fm-core/src/queries.rs), grammar in [`search_query.rs`](../crates/fm-core/src/search_query.rs).

| Method | Acct | Description |
| --- | --- | --- |
| `conversations` | yes | The list. Filters: `query`, `field`, `folder` (`inbox`, `received`, `sent`, `trash`, `all`), `category`, `sender`, `exact_sender`, `exact_person`, `kind`, `status`, `has_attachment`, `after`/`before` (UTC dates), `grouped` and group/month collapsing. Paging: `limit`, `offset` or `cursor`, `live_paging`. Rows carry `message_ids` and `revision`. |
| `conversation` | yes | Messages of `id` with text, flags, attachments, `source_version`. A pure read: opening marks nothing read. |
| `sender_summary` | yes | `email` → count and three recent conversations with that person. |
| `search_senders` | yes | People for search autocomplete: `query`, `folder`, `role`, `limit` (≤10). |
| `recipient_suggestions` | yes | Addresses for composing from all active accounts: `query`, `exclude`, `limit` (≤10). |
| `mail_notifications` | — | New-mail alerts when `notifications` is `all`. No `after` gives a baseline; then send `after` and `since`. Returns `after`, `messages` (≤20), `total`, `more`. |
| `google_remote_search` | yes | Gmail search for threads with no local copy: `query`, `folder`. Up to 25 rows marked `remote`. **live, session** |
| `google_remote_thread` | yes | Read a server-only `thread_id` (newest 50 messages). Stores nothing. **live, session** |
| `google_spam_list` | yes | Page Gmail Spam, 50 at a time. Stores nothing. **live, session** |
| `google_spam_message` | yes | Plain text of spam message `id`. **live, session** |
| `google_bin_counts` | yes | Counts for `spam` or `trash`. **live, session** |

`query` takes words (a phrase, the last word matching as a prefix) and `subject:`, `body:`, `from:`, `to:`, `label:`, `has:attachment`, `is:review|delayed|held|done`. A bare address filters by person. Contradicting a typed filter is refused.

### Review, filing and undo

A conversation is **review** if any message is unread, else **held** (Delayed) if any is starred, else **done**. The GUI maps actions to `flag`: open `{read: true}`, Done `{read: true, starred: false}`, Delay `{starred: true}`, undelay `{starred: false}`, mark unread `{read: false}`. On live profiles each change commits with a queued Gmail delivery. Source: [`mutations.rs`](../crates/fm-core/src/mutations.rs), [`google_live_mutations.rs`](../crates/fm-core/src/google_live_mutations.rs).

| Method | Acct | Description |
| --- | --- | --- |
| `flag` | yes | Set `read`/`starred` on `id` + `message_ids`, `expected_revision`, `request_id`. Returns the conversation, a `mail_undo` receipt and (live) `delivery`. Live accounts need Gmail write access (`capability_unavailable`). |
| `mail_filing` | yes | `action` `archive`, `trash` or `restore`, same snapshot params. **live** |
| `mail_undo` | yes | Revert a `flag`, `set_category` or `mail_filing`: `id`, `undo_request_id`, `expected_revision` (the receipt's, still current), `request_id`. |
| `google_mail_mutation_status` | yes | Gmail delivery: with `request_id` a `state` (`pending`, `unknown`, `confirmed`, `failed`, `superseded`); without, counts, failures and pending rows. |
| `google_not_spam` | yes | Move spam `thread_id` to the Inbox. **live, session** |
| `google_delete_forever` | yes | Permanently delete from `spam` (`thread_id`) or `trash` (`conversation_id`). Needs full Gmail access. **live, session** |
| `google_empty` | yes | Permanently delete up to 10,000 messages from `spam` or `trash`; repeat while `more`. **live, session** |

### Labels

Labels (categories in the store) are per account; a local label can link to a Gmail user label, and changes to linked labels reach Gmail. Source: [`local_labels.rs`](../crates/fm-core/src/local_labels.rs), [`google_labels.rs`](../crates/fm-core/src/google_labels.rs).

| Method | Acct | Description |
| --- | --- | --- |
| `categories` | yes | Labels with conversation counts. |
| `label_catalog` | yes | Labels with model description, `auto_read`, `auto_spam`. |
| `set_category` | yes | Add or remove `category_id` (`present`) on a snapshot, like `flag`. |
| `label_create` | yes | `id`, `name`, `request_id`. At most 64 labels. |
| `label_describe` | yes | `id`, `description`, `expected_description`, `request_id`. |
| `label_auto_read`, `label_auto_spam` | yes | `id`, new and expected bool, `request_id`. |
| `label_remove` | yes | `id`, `expected_name`, `expected_provider_label_id`, `request_id`. Never deletes the Gmail label. |
| `google_labels` | yes | Page the cached Gmail catalog: `limit`, `after`, `expected_catalog_revision`. **live** |
| `google_category_bind` | yes | Link a label to a Gmail label or create one for it (`create_name`): `category_id`, `provider_label_id`, `expected_catalog_revision`, `expected_provider_label_id`, `request_id`. **live** |
| `google_labels_refresh` | yes | Re-read the Gmail catalog now. **live, session** |
| `google_labels_sync` | yes | Background job reconciling local and Gmail labels: `request_id` → `job_id`. Ends with its connection. **live, session** |
| `google_labels_sync_status`, `google_labels_sync_cancel` | yes | Read or cancel `job_id` (starting connection only). **live** |

### Drafts

Drafts are named by `draft_id`, or a conversation's reply draft by `conversation_id` (exactly one). `draft_save` replaces the whole draft. With Gmail write access, sync uploads a draft after 30 seconds of rest. Source: [`composition.rs`](../crates/fm-core/src/composition.rs), [`draft_discard.rs`](../crates/fm-core/src/draft_discard.rs), [`draft_transfer.rs`](../crates/fm-core/src/draft_transfer.rs), [`google_drafts.rs`](../crates/fm-core/src/google_drafts.rs).

| Method | Acct | Description |
| --- | --- | --- |
| `draft_get` | yes | One draft; an unsaved reply comes back at revision 0. |
| `drafts` | yes | Open drafts: `query`, `limit`, `offset`. |
| `draft_create` | yes | `mode` (`new`, `reply`, `reply_all`, `forward`), `conversation_id` + `source_message_id` (not for `new`), `request_id`. |
| `draft_save` | yes | `body` (required), `body_html`, `signature_text`, `subject`, `to_recipients`/`cc_recipients`/`bcc_recipients`, `recipient_inputs` (unparsed text), `attachment_ids`, `expected_revision`, `request_id`. |
| `draft_attachment_import` | yes | Copy a local file in: `draft_id`, absolute `path`, `expected_revision`, `request_id`. |
| `draft_discard` | yes | `expected_revision`, `request_id`, optional unsaved `buffer`; returns a `discard_undo` receipt. |
| `draft_discard_status` | yes | Discard receipt and `can_undo` by `request_id`. |
| `draft_undo_discard` | yes | `draft_id`, `discard_request_id`, `expected_revision`, `request_id`. |
| `draft_transfer` | yes | Move (or `keep_original`) to `target_account`: `draft_id`, `expected_revision`, `expected_google_revision`, `request_id`. |
| `draft_transfer_status`, `draft_transfer_retry` | yes | Transfer state by `request_id`; resume a `blocked` one. |
| `draft_capture`, `capture_history` | yes | Write a draft's MIME to a local `.eml` without sending (**demo**); list those runs. |

Gmail drafts made elsewhere are mirrored as read-only versions; local drafts get Gmail twins.

| Method | Acct | Description |
| --- | --- | --- |
| `google_drafts_status` | yes | Inventory scan state. |
| `google_drafts_refresh` | yes | Wake the inventory worker. **live, session** |
| `google_draft_sources` | yes | Versions of a Gmail draft and its deletes: `draft_id`, `limit`, `offset`. |
| `google_draft_source` | yes | Export a version (`draft_id`, `source_revision`) or `attachment_id` to a file. |
| `google_draft_authoring` | yes | Whether a version can be copied, adopted or deleted. **live** |
| `google_draft_copy` | yes | New local draft from a version: `draft_id`, `source_revision`, `request_id`, fixed `*_policy` values. **live** |
| `google_draft_adopt` | yes | Take over a version as an editable local draft; same params. **live, session** |
| `google_draft_delete` | yes | One `drafts.delete`, never retried; same params. **live, session** |
| `google_draft_delete_status`, `google_draft_delete_reconcile` | yes | Delete state by `request_id`; or read Gmail back for evidence (**session**). **live** |
| `google_draft_delete_abandon` | yes | Release a never-dispatched delete: `request_id`, `expected_revision`. **live** |
| `google_draft_sync` | yes | Upload now: `draft_id`, `expected_revision`, `request_id`. A copy changed at Gmail gives `state: "conflict"`. **live, session** |
| `google_draft_status` | yes | Twin state by `draft_id`. **live** |
| `google_draft_reconcile` | yes | Settle an uncertain upload by reading Gmail. **live, session** |
| `google_draft_resolve` | yes | Overwrite a conflicting Gmail copy with the local draft: sync params plus `action: "replace_local"`, `expected_google_revision`. **live, session** |

### Send

The client holds a send for its Undo delay (a GUI setting; six seconds in the CLI) before calling `google_send`, which rechecks the draft, freezes the MIME, journals the attempt and submits once (`drafts.send` for a Gmail twin, else `messages.send`). Source: [`google_send.rs`](../crates/fm-core/src/google_send.rs), [`send.rs`](../crates/fm-engine/src/google_sync/send.rs).

| Method | Acct | Description |
| --- | --- | --- |
| `google_send` | yes | `draft_id`, `expected_revision`, `expected_google_revision`, `request_id`. Returns `state` (`prepared`, `in_flight`, `accepted`, `rejected`, `acceptance_unknown`, `abandoned`), `message_id`, `thread_id`, `error`. **live, session** |
| `google_send_status` | yes | Send state by `request_id`. **live** |
| `google_send_reconcile` | yes | Settle an uncertain send by finding its Message-ID in Sent; never resends. **live, session** |
| `google_send_abandon` | yes | Abandon a never-dispatched send: `request_id`, `expected_revision`. **live** |
| `google_sent` | yes | Sends not yet seen back from Gmail: `offset`, `limit`, `conversation_id`. |
| `google_sent_message` | yes | A send's frozen message by `request_id`: headers, text, attachment names. |

### Attachments and message source

`source_version` is the message's source hash from `conversation`; a changed message gives `stale_source`.

| Method | Acct | Description |
| --- | --- | --- |
| `attachment` | yes | Metadata for `id`, plus `path` to a freshly verified copy under `exports/` and `sha256`. |
| `message_source` | yes | Raw RFC 822 text (cut at 256 KiB), HTML part, address metadata, `has_invite`: `id`, `source_version`. |
| `message_invite` | yes | A message's calendar invitation, linked to the cached event: `id`, `source_version`. |
| `google_remote_attachment` | yes | Save an attachment of a server-only message (`id` `remote-att:<message>:<index>`) to a file. **live, session** |

### Calendar

`from`/`to` are `YYYY-MM-DD`, `to` exclusive, 1–42 days, in the display zone (`calendar_display_time_zone`). Timed events use RFC 3339, all-day events dates; rows carry `id` and `etag`, and only the details each calendar's access level allows. Source: [`combined_calendar.rs`](../crates/fm-core/src/combined_calendar.rs), [`calendar_reminders.rs`](../crates/fm-core/src/calendar_reminders.rs), [`live_calendar_writes.rs`](../crates/fm-core/src/live_calendar_writes.rs).

| Method | Acct | Description |
| --- | --- | --- |
| `calendar_sources` | — | Calendars of active accounts: permission, `writable`, color, coverage. |
| `calendar_combined` | — | Events across accounts: `from`, `to`, `calendars` (default: saved selection). |
| `calendar` | yes | Events of one account's calendars: `from`, `to`. |
| `calendar_search` | — | `query` (`title:`, `with:`) over `from`/`to` or `around` a date. |
| `calendar_mail_activity` | opt | Received and sent counts per day. |
| `calendar_reminders` | — | Reminders due now; clients poll. |
| `calendar_reminder_ack` | yes | `state` (`shown`, `dismissed`, `snoozed`) for `calendar_id`, `occurrence_id`, `start`. |
| `calendar_operations` | yes | The write journal, unresolved first: `limit`, `cursor`. |
| `calendar_write`, `calendar_write_ack` | yes | Local create, edit or RSVP with `request_id`; commit a `prepare_only` write. **demo** |

Google Calendar writes are journaled by `request_id`: each re-reads the event, sends one ETag-guarded request and records accepted, conflict, rejected or unknown.

| Method | Acct | Description |
| --- | --- | --- |
| `google_calendar_write` | yes | `kind` (`create`, `edit`, `delete`, `rsvp`), `calendar_id`, `event_id`, `expected_etag`, `fields` (a patch), `response`, `occurrence`/`recurring` scope, `request_id`. Schema: [`live_calendar_writes.rs`](../crates/fm-core/src/live_calendar_writes.rs). **live, session** |
| `google_calendar_occurrence_read` | yes | Resolve a recurring occurrence for editing. **live, session** |
| `google_calendar_operation` | yes | Write state by `request_id`. **live** |
| `google_calendar_reconcile`, `google_calendar_resume` | yes | Settle an uncertain write by reading Google; continue a partial one (such as a "this and following" edit). **live, session** |
| `google_calendar_abandon` | yes | Abandon an unfinished write: `request_id`, `expected_revision`. **live** |

### Preferences and views

| Method | Acct | Description |
| --- | --- | --- |
| `preferences_get` | — | `{revision, preferences}`: saved values over defaults. |
| `preferences_save` | — | Patch: `preferences`, `expected_revision`, `request_id`. Validated as a whole. Sync settings wake the scheduler. |
| `view_get` | yes | The account's saved view state. |
| `view_save` | yes | `view` (≤16 KiB), `expected_revision`, `request_id`. |

Keys cover appearance, mail, Calendar, sync (`sync_every_minutes`, `google_sync_services`, `google_sync_limits`), notifications and remote content (`blocked_hosts`, `remote_images_from`, `remote_proxy`, `sender_pictures`); see [`preferences.rs`](../crates/fm-core/src/preferences.rs) and [`remote_prefs.rs`](../crates/fm-core/src/remote_prefs.rs).

### Remote content

Only the engine fetches remote content. Received HTML is sanitised, with images and links replaced by `fm-res:N` / `fm-link:N`. Images come over HTTPS port 443 from public addresses, without cookies, through the optional proxy, decoded to PNG. The client decides whether to load them (the GUI asks, or follows `remote_images_from`); `blocked_hosts` always applies. Source: [`remote_content.rs`](../crates/fm-engine/src/remote_content.rs), [`html_content.rs`](../crates/fm-core/src/html_content.rs).

| Method | Acct | Description |
| --- | --- | --- |
| `message_html` | yes | Sanitised HTML for `id` + `source_version` (`html` or `html_path`), `resources`, `links`. No network. |
| `message_image` | yes | `resource` N as a PNG `path` and a `state`; `cached_only` stays offline. |
| `sender_avatar` | — | Picture for `address` + `kind` (Gravatar by hashed address, or the domain icon) as a PNG `path`. |
| `message_unsubscribe` | yes | For `id` + `source_version`: an RFC 8058 one-click POST now, or a `link` for the browser. |
| `google_unsubscribe_mail` | yes | Unsubscribe by sending the list's mailto message through Gmail: `id`, `source_version`, `address` (the address the person confirmed; refused unless it is the one the message would go to), `fallback`. **live, session** |
| `link_host_summary` | yes | Messages in all accounts linking to `host`. |
| `link_pattern_counts` | — | Message counts for blocklist `patterns`. |
| `remote_proxy_test` | — | Test the proxy: `{ok, ms}` or `{ok: false, reason}`. |
| `remote_proxy_password` | — | `op` `status`, `set` (`password`) or `clear`; kept in the OS credential store, never returned. |
| `remote_image_cache` | — | `op` `stats` or `clear`. |

### Local AI

Optional summaries, sender kind and labels from Ollama or an OpenAI-compatible server on loopback. The queue runs only while a connection that called `assist_watch` stays open; explicit jobs belong to their connection. A model-picked label applies like a manual one, and per-label `auto_spam`/`auto_read` may follow; models never star, Done or Delay mail. Source: [`local_model.rs`](../crates/fm-engine/src/local_model.rs) (engine), [`local_model.rs`](../crates/fm-core/src/local_model.rs) (store).

| Method | Acct | Description |
| --- | --- | --- |
| `assist_settings_get` | — | `{revision, settings, defaults}`. |
| `assist_settings_save` | — | `settings`, `expected_revision`. Loopback endpoints only. |
| `assist_discover` | — | Models at `endpoint`/`provider`, with eligibility. |
| `assist_catalog` | yes | The labels the model sees. |
| `assist_watch` | — | Register as a watcher and start the queue. |
| `assist_progress` | opt | Per-account queue state. Optional `max_age_ms` (≤ 60000) accepts counts that recent while only mail changed. |
| `assist_backlog` | yes | Unprocessed counts per catch-up range. |
| `assist_catch_up` | yes | `range` `week`, `month`, `year`, `all` or `stop`. |
| `assist_start` | opt | Job for `message_id` (account required) or a test `sample`; returns `job_id`. |
| `assist_job`, `assist_cancel`, `assist_jobs` | — | Read or cancel `job_id`, or list this connection's jobs. |
| `assist_results` | yes | Saved results for `conversation_id`. |
| `assist_correct` | yes | Pin sender `kind` for `message_id`: `conversation_id`, `expected_revision`. |
| `assist_accept` | yes | Apply a suggested `category_id` through `set_category`: `conversation_id`, `message_id`, `request_id`. |

### Diagnostics

No dedicated methods: `status` and `google_sync_status` carry the counters. The engine logs metadata only to `<profile>/diagnostics/engine.log`.

## Versioning

- **Protocol:** version 1, carried in `engine.json` and every frame; peers refuse other values. New methods and result fields are added within version 1, so clients should ignore unknown result fields; params stay strict.
- **Schema:** the store's SQLite `user_version` must equal `SCHEMA_VERSION`, currently 37 (`RESET_BASELINE_VERSION` plus `POST_RESET_MIGRATIONS` in [`crates/fm-core/src/lib.rs`](../crates/fm-core/src/lib.rs)), reported as `schema_version`. New profiles start at that version from [`fresh-schema.sql`](../crates/fm-core/src/fresh-schema.sql); future changes add migration steps run at open. Other profiles are refused unmodified with `schema_mismatch`: older ones with "This profile was made by an older fuckMail and cannot be opened. Move its folder aside, then try again to start fresh and reconnect your accounts.", newer ones with "This profile needs a newer engine". The refusal goes to `engine-refusal.json` and the client shows it with the profile path.
- **Profile kind:** `live` (holds Google accounts; `fixture_revision` 0) or `synthetic` (demo, seeded once; `fixture_revision` 3), fixed at creation. The default `fuckMail/default` (under `%LOCALAPPDATA%`, `~/Library/Application Support` or `$XDG_DATA_HOME`, else `~/.local/share`) is live; `--demo-profile` uses `fuckMail/demo-v1`. `fm-engine --profile DIR` opens either kind, creating a live profile if none exists; `--live-profile`/`--demo-profile` require that kind. The wrong kind fails with `profile_kind_mismatch` (exit 12) and is never converted.
