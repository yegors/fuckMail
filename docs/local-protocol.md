# Local protocol — protocol 1, Phase 2 additive boundary

P2-00 adds explicit empty live-profile initialization with `fm --profile <absolute-path> profile-init`. This starts the same engine with `--live-profile`, reports `status` and releases the CLI session. It requires an explicit path and refuses a synthetic profile, including an already-running demo engine. Ordinary later GUI/CLI opens use the persisted kind. `hello.mode` and `status.profile_kind` are `synthetic` or `live`; `status.synthetic` agrees. Current schema 10 under D023 preserves existing fixture revisions; new synthetic profiles remain fixture 3 and live profiles use fixture 0 with no fixture date. Status capabilities remain false for unwired provider workflows. This command creates no Google account and performs no network request.

Live account rows add `google.connection_state`, `granted_scopes`, actual `capabilities` and `sync` state. A scope grant alone never enables a workflow. Core accepts stable Google-subject bindings only from the internal verified OAuth path; there is no IPC method accepting a caller-supplied Google subject. Tokens and vault namespaces are absent from responses. Simulated `draft_capture`, `calendar_write_ack` and local `calendar_write` are refused in live profiles. D023 adds the separate live state/Calendar operation paths below; GUI capability integration remains a separate gate. See [Phase 2 execution](research/phase-2-execution.md) for the current tested scope.

The historical Phase 1 surface below remains supported. Schema 8 adds profile/vault identity and Google account bindings; schema 9 adds connection generations/client references and bounded Gmail/Calendar work, coverage and projection metadata in the same store. Only ordinary new profiles seed the synthetic fixture; explicit live initialization never does. The exact currently tested migration/native scope belongs in the execution record.

## Live state operations under D023

The schema-10 integration adds the following protocol-1 methods. They use the existing store/journals and have scoped synthetic-provider, argument and native regression evidence under D023. Actual live GUI/CLI workflows remain unverified; see the execution record for the precise tested boundary.

| Method | Account / parameters | Result |
| --- | --- | --- |
| `flag`, `set_category` | Existing explicit conversation `id`, `message_ids`, `expected_revision`, `request_id` and field values | Existing local reader receipt plus live `delivery: {state: pending, request_id}`; provider confirmation is separate |
| `mail_filing` | Same member/revision/request snapshot, `action: archive\|trash\|restore` | Durable local receipt and pending delivery; never targets later arrivals |
| `google_mail_mutation_status` | Required account, empty parameters | Delivery counts plus at most 20 pending and 20 failure entries with state/error/retry time |
| `google_calendar_write` | Required account; `request_id`, `kind: create\|edit\|delete\|rsvp`, `calendar_id`; existing-event operations require `event_id` and `expected_etag`; create/edit use `fields`, RSVP uses `response` | Frozen operation identity, state and revision; replay of a dispatched/uncertain operation returns status without another mutation |
| `google_calendar_reconcile` | Required account, `request_id` | Fresh provider readback of an uncertain attempt; never resubmits |
| `google_calendar_operation` | Required account, `request_id` | Bounded retained local outcome without provider access |

Live mail authoring may remain pending offline; an explicitly running account sync session drains that journal. Existing Gmail categories require actual provider-label mappings; unmapped commands fail without losing local data. `conversations.folder` accepts `trash`; Inbox/Sent/All eligibility excludes trashed members. `view_save` preserves the selected Trash folder. Restoration removes Trash and does not add Inbox.

Calendar execution/reconciliation requires an already active account sync session and owns a temporary request lifetime independent of the client that started sync. Only writable default nonrecurring organizer copies without attendees support this first CRUD slice; self-RSVP requires a writable authenticated attendee copy. Changing attendees/Meet details, recurrence or timed/all-day kind remains unsupported in this slice. Only provided `fields` change; omitted provider fields are preserved. Provider notification policy is frozen as `all`, and no automated fixture sends invitations. Future full recurrence/invitation support remains required by Phase 2.

Ready operations are known not dispatched and may be revalidated by another explicit execution of the same immutable request. A scheduler cooldown that proves no HTTP was sent returns only that exact claim to ready with retained delay evidence. Dispatched uncertainty still requires read reconciliation and never causes automatic replay.

CLI consumers are `mail filing`, `mail delivery`, `mail list --folder trash`, `google calendar-write`, `google calendar-reconcile` and `google calendar-operation`. Existing synthetic `calendar-write`/`calendar-ack` remain distinct. Engine/client cancellation must finish admitted Calendar outcome recording; it never authorizes replay or starts a background service.

## Explicit Google authorization

The implemented CLI provides `fm --profile <absolute-path> google connect --client-config <desktop-json-path>` and account-scoped `google disconnect`. Add `--account <account-id>` for an identity-checked reconnect and `--reauthorize` for explicit renewed consent. Connect opens the system browser and keeps its session alive while polling until success, failure or Ctrl-C. The authorization URL, client secret and token values are never printed, including with `--json`. Disconnect stops account work while retaining local data, drafts and native-vault credentials; it does not revoke the Google grant or delete the account.

Engine dispatch implements the following protocol-1 additions. These are session-owned operations, not direct Store methods:

| Method | Account / parameters | Result |
| --- | --- | --- |
| `google_auth_begin` | Optional reconnect account; `client_config_path` absolute installed-desktop JSON path, optional `reauthorize` boolean | One-use `authorization_url`, `attempt_id`, `expires_in_seconds` (300) |
| `google_auth_status` | No account; `attempt_id` from this same authenticated session | `attempt_id`, `phase`, optional resolved `account`, sanitized `error` |
| `google_auth_cancel` | Same owner and parameters as status | Cancels that attempt; returns its status |
| `google_disconnect` | Required account; empty parameters | Account and `connection_state: disconnected` |

One login may be active per profile; at most 16 terminal statuses are retained. Phases are `waiting_for_browser`, `exchanging`, `persisting`, `connected`, `cancelled` and `failed`. A different session cannot inspect or cancel another session's attempt. Session exit/shutdown cancels the attempt and its loopback listener. Status never repeats the browser URL. A standalone `call google_auth_begin` followed by CLI exit cannot be used as a detached login because it releases the owning session.

Verified reconnect pauses an existing account durably before replacing its vault grant; only a successful, generation-checked Core binding publishes Connected and the actual granted scopes. Cancellation or failed binding leaves reauthorization required even if the OS accepted the credential write. GUI Accounts controls and shared CLI dispatch use this runtime. The [engine OAuth fixtures](research/phase-2-auth.md#engine-owned-login-reconnect-and-cancellation--2026-09-13-utc) pass, but actual browser consent/native paired acceptance is outstanding. Sign-in alone never imports mail or calendars.

## Explicit Google acquisition sessions

Live `status`/`hello` capabilities now advertise callable `google_auth` and `google_sync` interfaces. These do not enable ordinary live Mail browsing/read-on-open, drafts/send or Calendar writes; those capability gates remain false. Each connected account must also have its relevant granted scopes before a service worker starts.

`fm --profile <absolute-path> --account <account-id> google sync` requires all four actual Console values: `--gmail-project-units-per-minute`, `--gmail-account-units-per-minute`, `--calendar-project-units-per-minute` and `--calendar-account-units-per-minute`. Each is an integer from 100 through 100,000,000; the scheduler uses 80% headroom with separate account/project/service budgets. Values are required, without guessed defaults, and all accounts in one engine must use the same configuration. Changing it requires an engine restart. `--seconds <duration>` limits this CLI owner's lifetime; Ctrl-C or exit releases only that owner. Another explicit sync owner may continue the shared account workers. `google sync-status` reads progress (optional account), while `google sync-stop` deliberately stops the selected account's acquisition for every owner.

| Method | Account / parameters | Result and lifetime |
| --- | --- | --- |
| `google_sync_start` | Required account; `gmail_project_units_per_minute`, `gmail_account_units_per_minute`, `calendar_project_units_per_minute`, `calendar_account_units_per_minute` | Account, `started`, `active`; starts or joins one account job for this authenticated session |
| `google_sync_status` | Optional account; empty parameters | `accounts` with worker `active` and `progress`, plus whether shared quotas are `configured`; starts no provider request |
| `google_sync_release` | Required account; empty parameters | `session_stopped`, remaining `active`; releases only this session and cancels workers when no owners remain |
| `google_sync_stop` | Required account; empty parameters | `stopped`; cancels and awaits all acquisition workers for that account |
| `google_sync_busy` | Required account with active acquisition; `from`, `to` ISO dates and IANA `time_zone` | Refreshes permitted free/busy snapshots for at most 42 days and reports calendar count/window; no provider event/edit identity is fabricated |

Workers share pooled HTTPS, token refresh, quota admission and one Core writer. Gmail recent/inventory/delta work publishes bounded durable parts; downloaded RAW retains its memory permit through preparation. Calendar publishes staged generations and uses indexed view candidates. Account disconnect/reconnect fences prior work, and final owning-session exit ends acquisition. Shared RAW limits have fixture evidence; a global decoded-Calendar-page budget and removing repeated whole-page hashing from the writer remain performance work before unrestricted import.

Live account sync summaries now use incremental acquisition counters: observed messages, retained sources, indexed messages, unavailable items, inventory completion, current catch-up and offline completion. Conversation coverage also exposes `messages_with_complete_attachments`, which counts messages rather than attachment files; aggregate attachment-file counts remain null. `verified_on_acquisition` describes retained-byte verification during ingestion, not a fresh full-profile integrity scan. Progress step/page counts are activity indicators and do not replace these coverage fields. [Integrated execution evidence](research/phase-2-execution.md#shared-runtime-integration--2026-09-13) records exactly which worker and admission fixtures passed.

Implementation is in `crates/fm-ipc`, `crates/fm-engine`, `crates/fm-cli` and `gui/MailBackend.cpp`. This is the shared interface for the synthetic Phase 1 implementation, preserving the Phase 0 protocol. The domain method fields and fixture expectations are in [the fixture API](../fixtures/README.md). It is not a promise of the final CLI command catalogue or provider APIs.

## Ownership and discovery

One engine owns a profile's SQLite connection, migration and journal; the core holds an OS file lock for its lifetime. Clients never open SQLite. A newly created ordinary demo profile seeds synthetic fixture revision 3; explicit live creation seeds nothing. Existing profiles are never reset on startup; incremental migrations through schema 9 retain their actual fixture revision and original accounts. Historically, schema 6 generalized the existing drafts table without rewriting authored fields or operation/capture records. Schema 7 added explicit source-bound sender kinds; old fixture1/2 profiles stay unclassified, without rewriting sources or reseeding. The default is `fuckMail/demo-v1` beneath the user's local application-data directory (macOS `~/Library/Application Support`, Windows `%LOCALAPPDATA%`, Linux `$XDG_DATA_HOME` or `~/.local/share`). `--profile` chooses an explicit absolute path.

The engine writes `engine.json` inside the private profile with protocol version, process ID, local endpoint and a random per-launch handshake token. The token is credential material; do not log, commit or put it in command-line arguments. Unix uses a user-private directory and socket plus peer credentials. Deep Unix paths use a hashed socket in a private `/tmp/fuckmail-UID` directory. Windows uses a named pipe with a current-user DACL and remote clients rejected. Clients validate the expected engine peer before sending the token. The supported boundary is the current OS user, not mutually hostile processes already running as that user.

Concurrent launches acquire the core writer lock before replacing an old owned socket/discovery file. Each connected GUI/CLI owns an explicit session. The last disconnect gives a 350 ms reconnection grace, then the engine closes. A newly started engine allows 10 seconds for its first client. Queued durable data alone never keeps it alive. An independent `fm watch` session may outlive the GUI, and ends on Ctrl-C or its requested duration. There is no installed background service.

## Wire format

Messages are UTF-8 JSON objects terminated by one newline. Private IPC uses the OS endpoint above, with no HTTP transport or Rust/C++ ABI. Explicit OAuth separately owns its temporary loopback HTTP callback. Both IPC directions allow at most 1 MiB per frame, including the newline. Binary attachments are separate retained files; they are not embedded in list responses.

First request:

```json
{"version":1,"id":"1","method":"hello","params":{"token":"<private discovery token>","client":"cli"}}
```

Normal request and successful response:

```json
{"version":1,"id":"2","method":"conversations","account":"windscribe","params":{"limit":30,"query":"Maya infra"}}
{"version":1,"id":"2","ok":true,"result":{"conversations":[],"next_offset":null,"coverage":{}}}
```

The illustrative response above is only the envelope/field layout; actual rows and coverage come from core state. Errors preserve correlation:

```json
{"version":1,"id":"2","ok":false,"error":{"code":"stale_revision","message":"The target changed; refresh before retrying."}}
```

IDs are strings of at most 128 bytes. `params` is an object; unknown envelope fields are rejected. `hello`, `accounts`, `status`, `preferences_get`, `preferences_save`, `calendar_sources` and `calendar_combined` omit account scope. Other methods must validate the account and the target together. Request `id` correlates a response; mutation `params.request_id` deduplicates durable intent. Reusing a mutation ID with a different payload is an error. Expected revisions prevent overwriting newer state; snapshot message IDs prevent a flag action from silently including a new reply.

At most 16 clients connect and 32 core operations queue; each connection permits exactly one request in flight. The Qt adapter queues bounded asynchronous UI requests locally. Queued work is bounded independently of UI paging. Disconnect cancels work not yet begun and interrupts active SQL reads through a per-request progress callback. The callback is removed before the next owner’s request; late cancellation cannot affect that request. An already-started short mutation transaction completes durably and is recoverable with its request ID. Bounded local file work is not asynchronously interrupted. Clients allow 30 seconds for engine startup and fixture initialization, measured with monotonic clocks. If an initial launch loses to a previous writer that is still closing, clients retry with 1/2/4-second capped backoff and at most ten launch attempts. Rust reaps its own exited child before launching again; Qt keeps detached engine ownership so GUI close cannot kill an independent CLI session. Privacy, incompatible discovery and rejected authentication fail directly. Endpoint cleanup remains inside the old writer lock; absent discovery does not by itself prove the lock has been released. The handshake times out after 5 seconds, incomplete authenticated frames and blocked writes after 15 seconds. An idle authenticated GUI/watch connection is a valid owner. The small implementation uses revision polling for change observation, not a promised lossless event stream.

## CLI surface

`fm --help` and nested help describe the executable's supported commands. Global `--profile`, `--engine`, `--account` and `--json` work before or after the subcommand. `FM_ENGINE_PATH` is an optional development override; installed clients find the sibling engine.

Examples (conversation IDs come from list results):

```sh
fm accounts
fm --account windscribe mail list --query 'Maya infra' --field all
fm --account windscribe mail list --kind automated --exact-sender status@service.example
fm --account windscribe mail sender status@service.example
fm --account windscribe mail read CONVERSATION_ID
fm --account windscribe mail flag CONVERSATION_ID --starred true
fm --account windscribe draft save CONVERSATION_ID --body-file reply.txt
fm --account windscribe draft show CONVERSATION_ID --json
fm --account windscribe draft capture CONVERSATION_ID --expected-revision 1 --request-id capture-example --json
fm --account windscribe calendar --from 2026-09-07 --to 2026-09-14
fm watch --seconds 30 --json
```

`--body-file -` and `call METHOD --params-file -` accept bounded UTF-8/JSON input from stdin. Human output is for reading; `--json` emits versioned response envelopes. Core errors have stable codes; CLI exit codes are 2 for invalid input, 3 for authentication/permission failures, 4 for stale revisions/cursors, 5 for unavailable/not-found content and 1 for other failures. Closing an output pipe ends cleanly. Explicit Google authorization is described above; live sending remains unimplemented. `draft capture` is an explicit local diagnostic: it builds a saved revision into a verified `.eml` file through the shared MIME builder and a persisted synthetic Gmail capture journal. `--outcome unknown` tests ambiguous acceptance; retrying the same immutable request cannot submit it twice. Historical schema-6 migration preserves authored state; protocol remains 1 and existing profiles keep their recorded fixture revision through later migrations. `draft save --html-file`, `--signature-file`, repeated `--attachment` and `--clear-attachments` expose authored formatting/signature/retained selections through the same revisioned API. Per-draft identity/signature snapshots preserve exact capture content even when account preferences change.

## Phase 1 commands and scope

```sh
fm --account windscribe mail list --grouped --sender maya --status review --has-attachment true --after 2026-09-01 --before 2026-09-12
fm --account windscribe mail review CONVERSATION_ID
fm --account windscribe mail done CONVERSATION_ID
fm --account windscribe mail delay CONVERSATION_ID
fm --account windscribe mail undelay CONVERSATION_ID
fm preferences show --json
fm preferences save --preferences-file preferences.json --expected-revision 0 --request-id settings-1
fm --account windscribe account-update --name Windscribe --signature-file signature.txt --expected-revision 0 --request-id account-1
fm --account windscribe draft save CONVERSATION_ID --body-file reply.txt --html-file reply.html --signature-file reply-signature.txt --attachment RETAINED_ATTACHMENT_ID
fm --account windscribe draft history --conversation-id CONVERSATION_ID --json
fm --account windscribe draft discard CONVERSATION_ID --expected-revision 1 --request-id discard-1
fm calendar-sources --json
fm calendar --combined --from 2026-09-07 --to 2026-09-14 --json
fm --account windscribe calendar-write --params-file event.json --json
fm --account windscribe calendar-operations --json
```

`mail read` stays a query. Explicit review/Done/Delay/undelay commands load a snapshot and submit the same `flag` method as the GUI; their `--expected-revision` and `--request-id` options support explicit callers. For exact snapshot retry after a concurrent arrival, use `fm call flag` with the original immutable payload. Source date filters are UTC, inclusive after/exclusive before. `--grouped` selects review/held/done ordering; ordinary lists retain legacy date order. `--cursor` follows `next_cursor` and replaces offset; a changed account/query/search state or writer restart fails `stale_cursor` rather than silently drifting through pages. View/preferences/draft/calendar writes preserve paging. Treat the cursor as opaque; global response `revision` remains store-wide.

Preferences are validated patches; null calendar selection means all sources, [] means none. These settings never filter mail across accounts. `account-update` changes domain metadata, and existing authored draft snapshots remain unchanged. `draft history` describes local MIME capture, including acceptance unknown, without manufacturing delivered/sent provider mail. `draft history --conversation-id CONVERSATION_ID --unresolved-only --limit 1` checks pending/unknown captures independently of ordinary history pages. Recovery uses the history row's original `operation_id`, `draft_revision`, `conversation_id` and `synthetic_outcome` with `draft_capture`; it does not create a fresh operation ID. `calendar-write` is a deterministic local provider-fake create/edit/self-RSVP command with durable identity, permission and ETag checks; live writes remain later work. `calendar-operations --cursor '<returned continuation JSON>'` pages the account's public journal, with pending/failed operations before acknowledged receipts. Exact field names, limits and error semantics are in the [fixture API](../fixtures/README.md).

## P1-08 local composition

Core 12 and CLI 6 add `draft_create` and `drafts` on protocol 1. `draft_get`, `draft_save`, `draft_discard` and `draft_capture` accept exactly one independent `draft_id` or legacy `conversation_id` selector. `capture_history` accepts either optional selector. Each identity is validated together with its account. New, reply, reply-all and forward use one draft, MIME and immutable local capture implementation; creating composition never inserts received conversations or provider-sent mail. The [fixture API](../fixtures/README.md#p1-08-local-composition--core-12-cli-6-mail-ui-11) specifies the full fields and bounds.

```sh
fm --account windscribe draft create --mode new --request-id new-1 --json
fm --account windscribe draft list --query 'release ada' --limit 30 --json
fm --account windscribe draft show --draft-id DRAFT_ID --json
fm --account windscribe draft save --draft-id DRAFT_ID --body-file authored.txt --subject 'Release review' --recipients-file recipients.json --expected-revision 1 --request-id save-1
fm --account windscribe draft create --mode reply_all --conversation-id CONVERSATION_ID --source-message-id MESSAGE_ID --request-id reply-all-1 --json
fm --account windscribe draft create --mode forward --conversation-id CONVERSATION_ID --source-message-id MESSAGE_ID --request-id forward-1 --json
fm --account windscribe draft capture --draft-id DRAFT_ID --expected-revision 2 --request-id capture-1 --json
fm --account windscribe draft history --draft-id DRAFT_ID --unresolved-only --json
fm --account windscribe draft discard --draft-id DRAFT_ID --expected-revision 2 --request-id discard-1
```

`recipients.json` is a bounded object containing optional `to_recipients`, `cc_recipients` and `bcc_recipients` address arrays. An omitted array preserves its value and an empty array clears it. Human-authored Bcc remains private draft/submission content; the existing Gmail MIME builder retains it in the explicitly inspected local capture. Reply-all derives only visible retained source recipients, excludes the current identity case-insensitively and starts with empty Bcc. Forward includes editable quoted retained text and starts with no attachments or reply References.

The first legacy save binds the source member loaded by the client; later saves cannot retarget it. CLI replay with explicit `--expected-revision 0` includes the same frozen source. There is one historical compatibility exception to exact payload fingerprints: a first-save receipt that predates `source_message_id` can match the same request with only that additive field omitted. Its old stored receipt and fingerprint stay unchanged. New receipts reject changed source IDs. Captures always replay their original bytes/revision/subject and remain independent of later draft edits or discard.

## Verification and limits

Framing/private-discovery unit tests and engine session integration tests live beside the Rust code. The Qt client has a native trust test and a `--self-test` path that invokes the separate CLI. These must be run on the native target; writing a test is not pass evidence. Current Phase 2 execution and native failures are tracked in [status](status.md) and the [execution record](research/phase-2-execution.md).

The Windows vault has an isolated synthetic roundtrip and engine OAuth fixtures pass; real consent, integrated provider execution/writes, long-body transfer/paged members, recurring-calendar operations and the full retention/recovery/native matrix retain their applicable [Phase 2 gates](plans/phase-2.md). Full change streams are not promised by revision polling. This document does not remove those obligations.
