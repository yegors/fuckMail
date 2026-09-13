# Local protocol — protocol 1, Phase 2 additive boundary

P2-00 adds explicit empty live-profile initialization with `fm --profile <absolute-path> profile-init`. This starts the same engine with `--live-profile`, reports `status` and releases the CLI session. It requires an explicit path and refuses a synthetic profile, including an already-running demo engine. Ordinary later GUI/CLI opens use the persisted kind. `hello.mode` and `status.profile_kind` are `synthetic` or `live`; `status.synthetic` agrees. D032 advances the active integration boundary to schema 15 and preserves existing fixture revisions; new synthetic profiles remain fixture 3 and live profiles use fixture 0 with no fixture date. Status mail/drafts/calendar_read and live send indicate available interfaces; provider delivery and per-account grants are separate. An empty live profile has no sendable account. This command creates no Google account and performs no network request. The [execution record](research/phase-2-execution.md) distinguishes passing prior checkpoints from current integration and remaining live-provider gates.

Live account rows add `google.connection_state`, `granted_scopes`, actual `capabilities` and `sync` state. A scope grant alone never enables a workflow. Core accepts stable Google-subject bindings only from the internal verified OAuth path; there is no IPC method accepting a caller-supplied Google subject. Tokens and vault namespaces are absent from responses. Simulated `draft_capture`, `calendar_write_ack` and local `calendar_write` are refused in live profiles. D023 adds the separate live state/Calendar operation paths below; GUI capability integration remains a separate gate. See [Phase 2 execution](research/phase-2-execution.md) for the current tested scope.

The historical Phase 1 surface below remains supported. Schema 8 adds profile/vault identity and Google account bindings; schema 9 adds connection generations/client references and bounded Gmail/Calendar work, coverage and projection metadata in the same store. Only ordinary new profiles seed the synthetic fixture; explicit live initialization never does. The exact currently tested migration/native scope belongs in the execution record.

## Remote Drafts snapshots under D032

Core 21/Gmail 10/Mail UI 19/CLI 14/Quality 17 define the additive schema-15 boundary. Integration and tested scope are recorded in [Phase 2 execution](research/phase-2-execution.md#p2-05b3a-remote-drafts-snapshots-under-d032). The following fields are the agreed consumer boundary, not a whole-packet acceptance claim.

| Method | Exact parameters | Result/work |
| --- | --- | --- |
| `google_drafts_refresh` | `{}` with explicit account | Metadata-only begin/resume/coalescing through an existing owned readable sync job; its sole background worker continues the scan |
| `google_drafts_status` | `{}` with explicit account | Cached inventory status; no owner or HTTP required |
| `google_draft_sources` | `draft_id`, optional `limit` (1–100, default 30), `offset` (0–10,000,000, default 0) | Cached bounded source history with `sources`, `total`, `next_offset` |
| `google_draft_source` | `draft_id`, positive signed-64-bit `source_revision`, optional `attachment_id` | Off-writer verified local export at that exact retained revision; no HTTP |

CLI forms are `google drafts-refresh`, `google drafts-status`, `draft sources --draft-id ID [--limit N --offset N]`, and `draft source --draft-id ID --source-revision REV [--attachment-id ID] [--output FILE]`. All require explicit profile/account. Source/draft IDs are nonempty, at most 256 bytes and control-free; invalid CLI input fails before connection/storage. Export without `--output` returns the private verified copy's metadata. `--output` copies to a new file and refuses any existing destination. Neither command launches a mail client or submits imported contents.

Inventory status contains `account`, `generation`, `revision`, `enumeration_complete`, `paused`, `connection_generation`, `pending_items`, `estimated_total`, `observed_items`, `observed_this_scan`, `source_retained`, `source_unavailable`, `preview_incomplete`, `started_at` and `completed_at`. Cached latest-source counts stay separate from items actually observed in the current scan, the estimate and enumeration completion. `paused` describes a durable inventory pause; active network/error/lifetime state belongs to the account's `google_sync_status` runtime progress. A new owned sync job begins/resumes one scan; adding owners to that existing job does not restart it. A completed scan requires explicit refresh or a newly started sync job before another full enumeration. Cached views/status never create an owner or start a scan.

Existing draft list/detail replies add `origin`, `read_only` and nullable `remote_source`. An imported row has `origin: "google_snapshot"`, `read_only: true`, no received conversation/member, and null `google`/`google_send`; original From/routing and source are retained without an account-signature fallback. `remote_source` includes stable `provider_draft_id`, monotonic `source_revision`, contained `message_id`/`thread_id`, nullable RAW `sha256`/`size`, `source_state` (`available`, `missing`, `unavailable`), `preview_state` (`complete`, `partial`, `unavailable`), bounded `issues`, observation/linkage metadata and bounded `raw_headers`. Original RAW is authoritative when preview fields are incomplete. History exposes those same source records. Export returns `account`, `draft_id`, `source_revision`, nullable `attachment_id`, `path`, `sha256`, `size`, `name` and `mime_type`.

Core returns `draft_read_only` for new attempts to mutate imported snapshots, including save/discard/attachment import and Google upload/Send preparation. Earlier immutable receipt replay keeps its existing order; live synthetic capture retains its existing `capability_unavailable` refusal. An exact late provider identity may link a snapshot to existing authored work; content similarity never establishes that link or resolves unknown delivery. Missing sources preserve earlier cached content with explicit unavailability; they are not proof of sending/deletion. Full supported editing/adoption and explicit remote deletion remain B3B work.

Source freshness includes `seen_generation`, `last_observed_at`, `latest_scan_generation`, `seen_in_latest_scan` and `latest_scan_complete`, independently from the source version's original `observed_at`. A completed empty scan preserves cached drafts while marking them unseen in that scan. `absence_means_sent` remains false. Status counters are maintained as each item commits; cached status does not parse every retained source to recount the collection.

## Explicit Google submission under D029

Core 20/Gmail 9/Mail UI 18/CLI 13 define the additive schema-14 boundary. These account-scoped methods share the existing capture journal with a Google operation kind of `draft` or `send`; missing kind on older Google rows means `draft`. Synthetic receipts retain their separate execution mode. New request IDs never bypass unresolved or conflicting draft/submission work.

| Method | Exact parameters | Result |
| --- | --- | --- |
| `google_send` | `draft_id`, positive `expected_revision`, nonnegative `expected_google_revision`, `request_id` | Exact frozen submission and durable outcome; matching replay never automatically repeats a dispatched attempt |
| `google_send_status` | `request_id` | Cached selected operation; no HTTP or sync owner required |
| `google_send_reconcile` | `request_id` | Bounded read-only Sent/message metadata evidence; certainty is unchanged |
| `google_send_abandon` | `request_id`, positive `expected_revision` | Local abandonment of only proven-undispatched prepared work at that operation revision; no HTTP |

CLI forms are `google send --draft-id ID --expected-revision REV --expected-google-revision REV --request-id ID`, `google send-status --request-id ID`, `google send-reconcile --request-id ID`, and `google send-abandon --request-id ID --expected-revision REV`. All require explicit account/profile. IDs use the draft commands' existing 256-byte draft/128-byte request and control-character bounds; revisions fit signed 64-bit integers. Google revision zero permits a new twin. Invalid input fails before storage or connection.

Execution and reconciliation require an active account sync lifetime with a connected, mail-write-capable account and verified primary mailbox identity. Send reserves its 100-unit quota and bounded worker capacity before loading/encoding queued MIME. A known nonconflicting Google draft is sent with both its exact ID and frozen MIME; otherwise a permitted direct-message submission uses the same strict formatter. Preserve exact From/To/Cc/Bcc/subject/body/attachments, source threading, timestamp and unique RFC Message-ID. Request identity, preparation and in-flight state persist before HTTP. An explicit replay may resume proven-undispatched preparation; a dispatched or unknown request returns its retained outcome.

The bounded status contains `account`, `draft_id`, `request_id`, per-operation `revision`, monotonic `current_google_revision`, `state`, frozen `draft_revision` and input `google_revision`, `connection_generation`, `operation_is_current`, the four original `request` fields, nullable `provider_draft_id`/thread context, RFC `message_id`, `upload_sha256`, `size`, frozen From/recipient/subject metadata, `created_at`, nullable `ack: {message_id, thread_id}`, `error`, `read_evidence` and `last_accepted_draft_revision`. `draft.google_send` projects the latest submission or null. MIME and credentials never enter this projection. Historical replies remain distinguishable and cannot replace a later operation or editable text.

Reconciliation queries the first page of at most ten Sent candidates by the frozen RFC Message-ID, then reads each candidate's ID/thread/label/history metadata and at most one distinct acknowledged message. It records truncation and missing IDs and explicitly reports `draft_observation: "not_checked"`. It performs no draft RAW existence probe, upload or resend. Search matches, SENT labels, history IDs and absence are observations; they never promote acceptance-unknown into definitive acceptance or guarantee recipient delivery.

After definitive acceptance, the same authored revision cannot be sent under a new request. A strictly newer saved revision requires another explicit Send with fresh request/Message-ID and a direct-message target. The consumed Google draft is never reused or recreated by draft autosync. Pending/unknown sends block new attempts. Newer authored text survives every outcome. UI Send commits recipients before its save continuation; edits or navigation during that save cancel the continuation, and failures leave no later automatic send.

A lost local Send response before journal creation remains ambiguous: cached `not_found` is not proof that another invocation cannot still dispatch. Keep the original four request fields. After an explicit status check finds no record, the user may Resume that exact original request and ID; competing preparation finishes return the journal winner and only one claim can transition it to in-flight. Recorded unknown operations remain read-only Check. A direct completed Send error may include `details.send_invocation_finished_before_dispatch: true`, proving only that invocation has settled before dispatch. It never clears uncertainty from an earlier lost invocation of the same request.

Explicit same-ID Resume may instead return `stale_revision` or `stale_google_revision` with `details.send_request_obsolete_without_dispatch: true`. This stronger proof is emitted only after the writer finds no journal row and a current local or Google revision strictly greater than the original request's expected revision in that same call. Every pending preparation must recheck these monotonic revisions before insertion; no claim exists without a durable row. Thus no invocation of that exact obsolete request can subsequently submit. The UI may release its old local uncertainty, preserve newer text and require another explicit Send. Existing journal rows, future revisions, generic `not_found`/stale errors and connection failures never carry this proof. A local auxiliary-capacity refusal before constructing an operation carries only the narrower invocation proof.

## Locally authored Google drafts under D028

Core 19, Gmail 8, Mail UI 17 and CLI 12 add these protocol-1 methods to schema 13. All require the draft's explicit account. `google_draft_status` is a cached query; other methods require an active account sync lifetime and use its verified connection and actual primary mailbox identity. Ordinary local draft saves remain independent of provider work. Owned sync coalesces eligible persisted revisions automatically.

| Method | Exact parameters | Result |
| --- | --- | --- |
| `google_draft_sync` | `draft_id`, positive `expected_revision`, `request_id` | One exact authored revision's retained operation/current status; matching request replay never creates another attempt |
| `google_draft_status` | `draft_id` | Bounded current draft/twin/operation metadata; no HTTP |
| `google_draft_reconcile` | `draft_id` | Fresh RAW observation of an identified target; no upload |
| `google_draft_resolve` | `draft_id`, positive `expected_revision`, positive `expected_google_revision`, `request_id`, `action: "replace_local"` | Explicit new revision-bound replacement of a retained conflict |

CLI equivalents are `google draft-sync --draft-id ID --expected-revision REV --request-id ID`, `google draft-status --draft-id ID`, `google draft-reconcile --draft-id ID` and `google draft-replace --draft-id ID --expected-revision REV --expected-google-revision REV --request-id ID`. All require explicit profile and account; reconciliation accepts no request ID. Draft IDs are nonempty, at most 256 UTF-8 bytes and contain no control characters; request IDs have the same constraints with a 128-byte bound. Revisions are positive signed 64-bit integers. Invalid CLI input is rejected before profile creation or engine connection.

Status contains `account`, `draft_id`, `state`, Google `revision`, `local_revision`, nullable `operation`, `baseline` and `conflict`, plus `fidelity_warning` and `operation_is_current`. States are `local`, `prepared`, `in_flight`, `acceptance_unknown`, `acknowledged`, `synced`, `conflict` and `failed`. An operation includes its request/revision/state, frozen draft revision and connection generation, upload hash/size, nullable acknowledgment/error and nullable `provider_draft_id` update target. A baseline retains provider draft/message IDs, separate observed/upload hashes, its authored revision and observed size. A conflict retains its reason and nullable observed draft/message IDs/hash/size. No MIME, auth material or caller-provided provider identity enters these projections.

`operation_is_current: false` identifies a historical exact operation replay; its state is that original operation's outcome and must not replace the current draft projection. Read cached status for current state. A differing upload/readback hash yields a fidelity warning independently of acknowledged version identity. An unknown update can be explicitly replaced only after fresh observation of its exact existing target, with a new request and current local/Google revisions. Its original unknown receipt remains unknown. Unknown creation or a missing unknown target cannot be retried this way. A missing draft is never evidence of send. Conflict replacement discloses the remaining fetch-to-update race; it is not an atomic provider comparison-and-swap.

Nullable `preparation_failure: {draft_revision, code}` describes a deterministic local failure without inventing a provider attempt. Its unchanged revision is not automatically retried; a later saved edit is eligible. Failure during explicit conflict replacement preserves the conflict and still requires a fresh explicit confirmation after any edit. Nullable `retry: {draft_revision, code, until_ms}` defers bounded local IO failures so other eligible drafts can progress, without changing a frozen operation or acknowledgment. A known rejected upload likewise requires a newer authored revision before automatic work resumes; unknown operations never use this rule. Native recovery explains these local failures and offers provider Check only when a target, acknowledgment or baseline supplies an actual draft identity.

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

## P2-07A cached labels and recovery under D024/D025

The following additive protocol-1 boundary is implemented in source for Core 17, Gmail 7, Calendar 15 and CLI 10. Schema 11 adds provider label catalog metadata to the existing store; category mappings, manual message intent, receipts and preferences keep their existing tables. The current integrated Rust/native verification has not yet been recorded as passing.

All methods in this table require account scope. Catalog and delivery queries work from retained local state and make no provider request.

| Method | Exact parameters | Result |
| --- | --- | --- |
| `google_labels` | Optional `limit` (integer 1–100, default 100), `after` (last label ID), `expected_catalog_revision` (nonnegative integer); `after` requires the revision | `catalog_revision`, `available`, `observed_at_ms`, `connection_state`, `stale_connection`, `labels`, `next_after` |
| `google_labels_refresh` | Empty object | `catalog_revision`, numeric `labels` count, `observed_at_ms`, `available`; requires an active account sync job and holds a temporary operation owner |
| `google_category_bind` | `request_id`, `expected_catalog_revision`, `category_id`, `provider_label_id`, plus either `create_name` for a new local category or `expected_provider_label_id` (string or explicit null) for an existing category | Durable `{request_id, category: {id, name, provider_label_id, label_name}, catalog_revision, local_only: true}` receipt |
| `google_mail_mutation_status` | Empty object for the earlier bounded overview, or exactly `{request_id}` for one operation | Exact form returns `{request_id, state, counts, total}` across every captured target; an absent journal is `not_found` |
| `google_calendar_resume` | Exactly `{request_id}` | Reuses the retained immutable request only in `prepared` or `ready`, then returns the existing conditional execution outcome; requires active account sync |
| `google_calendar_abandon` | Exactly `{request_id, expected_revision}`; revision is a nonnegative integer from the current operation status | Current bounded Calendar status; eligible retained intent becomes `abandoned`, with its revision advanced |

Each cached label row is `{id, name, type, present, selectable, category_id}`. `type` is `user` or `system`; `category_id` is nullable. A mapped label absent from the current complete catalog remains visible with `name: null`, `present: false` and `selectable: false`. A provider rename updates the provider name and leaves the configured category name unchanged. `available: false` means no catalog has been published; `stale_connection` separately reports whether the observation belongs to another connection generation. Neither disconnected cached data nor a partial refresh is presented as a fresh complete catalog. Pagination checks the supplied catalog revision and returns `stale_revision` if it changed.

Binding selects a present user label in the same account. System, missing/deleted and foreign-account destinations are refused; a provider label cannot be bound to two local categories. Existing-category remaps compare the exact previous mapping, with null meaning explicitly unmapped. New-category names are nonempty strings of at most 256 bytes without control characters. A repeated identical request returns its original receipt even after a catalog refresh; changing its payload produces `request_id_conflict`. Cached binding can be performed offline with a retained modification grant. Binding changes neither message membership nor pending manual intent, read/star state or Inbox membership; it does not create a Google label. An older pending delivery retains its frozen mapping and cannot be redirected by a later remap.

Exact mail delivery state is `confirmed` only when every captured target is confirmed. Otherwise an in-flight/reconciliation target yields `unknown`, a failed target yields `failed`, remaining pending work yields `pending`, and fully superseded work yields `superseded`, in that order. `counts` preserves the per-target state totals. The original local action receipt continues to say `delivery.state: pending`; use this status query to inspect subsequent provider outcomes.

D025 permits Calendar Resume from both known-unsent states, `prepared` and `ready`. The normal execution path rechecks current source, permissions and connection before dispatch. `in_flight` and `unknown` cannot resume by replay; uncertainty uses `google_calendar_reconcile`. Abandon checks the exact operation revision and refuses `in_flight`; it may retire `prepared`, `ready`, `unknown`, `conflict` or `rejected` local intent. It performs no HTTP and does not claim to undo a change that may already have reached Google. Client close cancels a resumed request while allowing an admitted Calendar outcome receipt to finish.

Use the following CLI grammar with `fm --profile <absolute-path> --account <account-id>`. Angle-bracket values are placeholders, not literal defaults; `--json` exposes the complete structured result.

```text
google labels [--limit <1..100>] [--after <label-id> --expected-catalog-revision <revision>]
google labels-refresh
google category-bind <category-id> <provider-label-id> --expected-catalog-revision <revision> --request-id <id> --create-name <local-name>
google category-bind <category-id> <provider-label-id> --expected-catalog-revision <revision> --request-id <id> --expected-provider-label-id <previous-label-id>
google category-bind <category-id> <provider-label-id> --expected-catalog-revision <revision> --request-id <id> --unmapped
mail delivery [--request-id <id>]
google calendar-operation <request-id>
google calendar-resume <request-id>
google calendar-reconcile <request-id>
google calendar-abandon <request-id> --expected-revision <revision>
```

`google labels --expected-catalog-revision <revision>` may also check the first page explicitly. Exactly one of `--create-name`, `--expected-provider-label-id` and `--unmapped` is required for binding. The last option serializes the previous mapping as JSON null. `labels-refresh`, Calendar Resume and Calendar reconciliation use an already running sync session; these short commands do not implicitly start one.

The existing app-wide `preferences_get`/`preferences_save` boundary adds `google_sync_limits`, defaulting to null. Its only other accepted value is an object containing exactly `gmail_project_units_per_minute`, `gmail_account_units_per_minute`, `calendar_project_units_per_minute` and `calendar_account_units_per_minute`, each an integer from 100 through 100,000,000. A preferences patch replaces this whole value; partial objects, extra keys, strings and fractional values fail atomically. Use `preferences save --preferences-file <JSON-file-or-> --expected-revision <revision> --request-id <id>` without account scope, with the patch containing `{"google_sync_limits": <complete-object-or-null>}`. Saving or clearing these preferences performs no HTTP, starts no sync and does not change quotas already active in the engine. Explicit `google sync` still requires all four flags. The shared parser reserves 20% headroom by using `floor(value / 5) * 4` for each service/account budget.

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

Live `status`/`hello` capabilities advertise callable `google_auth` and `google_sync` interfaces. Current account capabilities describe available local/live paths separately from connectivity; an interface advertisement alone does not authorize an action. Mail acquisition and labels accept retained `gmail.readonly`, `gmail.modify` or full-mail grants; the mutation worker and category binding require `gmail.modify` or full-mail. An account must be connected before provider workers start. Unimplemented send remains unavailable. P2-07A's scoped native integration passes; acquisition/navigation and actual-account acceptance remain separate.

`fm --profile <absolute-path> --account <account-id> google sync` requires all four actual Console values: `--gmail-project-units-per-minute`, `--gmail-account-units-per-minute`, `--calendar-project-units-per-minute` and `--calendar-account-units-per-minute`. Each is an integer from 100 through 100,000,000; the scheduler uses 80% headroom with separate account/project/service budgets. Values are required, without guessed defaults, and all accounts in one engine must use the same configuration. Changing it requires an engine restart. `--seconds <duration>` limits this CLI owner's lifetime; Ctrl-C or exit releases only that owner. Another explicit sync owner may continue the shared account workers. `google sync-status` reads progress (optional account), while `google sync-stop` deliberately stops the selected account's acquisition for every owner.

| Method | Account / parameters | Result and lifetime |
| --- | --- | --- |
| `google_sync_start` | Required account; `gmail_project_units_per_minute`, `gmail_account_units_per_minute`, `calendar_project_units_per_minute`, `calendar_account_units_per_minute` | Account, `started`, `active`; starts or joins one account job for this authenticated session |
| `google_sync_status` | Optional account; empty parameters | `accounts: [{account, active, owned, progress}]`, `configured` and `limits`; `owned` refers to this authenticated session's active registration, while `active` may belong to another owner. `limits` is null or the four raw confirmed values used by the engine, before headroom; starts no provider request |
| `google_sync_release` | Required account; empty parameters | `session_stopped`, remaining `active`; releases only this session and cancels workers when no owners remain |
| `google_sync_stop` | Required account; empty parameters | `stopped`; cancels and awaits all acquisition workers for that account |
| `google_sync_busy` | Required account with active acquisition; `from`, `to` ISO dates and IANA `time_zone` | Refreshes permitted free/busy snapshots for at most 42 days and reports calendar count/window; no provider event/edit identity is fabricated |

Workers share pooled HTTPS, token refresh, quota admission and one Core writer. Gmail recent/inventory/delta work publishes bounded durable parts; downloaded RAW retains its memory permit through preparation. The labels worker refreshes a complete catalog at owned sync start and at a 600-second cadence, sharing the RAW/preparation memory permit; explicit refresh uses the same bounded path. Calendar publishes staged generations and uses indexed view candidates. Account disconnect/reconnect fences prior work, and final owning-session exit ends acquisition. D023 supplies shared decoded Calendar preparation bounds and moves whole-page hashing off the writer; the execution record owns its measured evidence and remaining unrestricted-import gates.

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

## P2-05A2 local-file attachments under D027

Core 18/Mail UI 16/CLI 11 add `draft_attachment_import` on protocol 1, with schema 12 and unchanged live/synthetic fixtures 0/3. The CLI command requires an explicit account and an existing saved draft:

```sh
fm --account ACCOUNT_ID draft attach --draft-id DRAFT_ID --file 'report.pdf' --expected-revision 7 --request-id attach-1 --json
```

The request parameters are exactly `{draft_id,expected_revision,request_id,path}`. `expected_revision` is a positive signed-64-bit draft revision. The CLI resolves `--file` to an absolute local path without opening it or resolving filesystem links; relative paths use its current working directory. `--file -` names a local file, not stdin. Use the same resolved path and other parameters when replaying a request.

The engine checks the existing receipt before accessing the source path. It prepares and retains the file outside the Store writer, then atomically rechecks the saved draft, appends the selected attachment, advances the draft once and stores the exact result. A matching replay returns that original result even if the source was moved, changed or deleted; changing request parameters under the same request ID fails. Stale revisions fail without replacing current authored content. Existing limits remain 50 selected attachments and 25 MiB combined bytes.

The result is `{draft: <updated draft projection>, attachment: {account,id,message_id:null,name,mime_type,size,state,sha256}}`. Imported metadata retains a bounded basename and `application/octet-stream`; its state is `available` after verified retention. The source path contributes only to the request fingerprint and is absent from the result and retained attachment metadata. Use the returned draft revision for the next edit. This operation performs no upload or send and does not create received messages, FTS entries or received-source coverage. Actual engine/CLI/native acceptance is recorded separately in the Phase 2 execution evidence.

## Verification and limits

Framing/private-discovery unit tests and engine session integration tests live beside the Rust code. The Qt client has a native trust test and a `--self-test` path that invokes the separate CLI. These must be run on the native target; writing a test is not pass evidence. Current Phase 2 execution and native failures are tracked in [status](status.md) and the [execution record](research/phase-2-execution.md).

The Windows vault has an isolated synthetic roundtrip and engine OAuth fixtures pass; real consent, integrated provider execution/writes, long-body transfer/paged members, recurring-calendar operations and the full retention/recovery/native matrix retain their applicable [Phase 2 gates](plans/phase-2.md). Full change streams are not promised by revision polling. This document does not remove those obligations.
