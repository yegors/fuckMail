# Gmail provider



## D120 — Spam and Trash actions, full mail access (Gmail30)

- Write-mode mail access requests `https://mail.google.com/` instead of `gmail.modify` (also in `GOOGLE_SCOPES`). `ScopeSet::mail_full()` is true only for that scope; `mail_write` still accepts either.
- `GmailCall::thread` now asks `fields=id,messages(id,labelIds)` and returns `GmailThreadMessage {id, label_ids}`. New `GmailCall::label(id)` (`labels.get`, 1 unit, `LabelCounts {messages_total, threads_total}`) and `GmailCall::batch_delete(ids)` (`messages.batchDelete`, 1–1,000 ids, 50 units, state-mutation class; an empty or `{}` body is `Deleted`).
- `google_spam_list` rows add `thread_id`.
- `google_bin_counts {folder: spam|trash}`: the label's totals.
- `google_not_spam {thread_id}`: needs mail write; each message of the thread labelled SPAM gets `messages.modify` add INBOX, remove SPAM. Sync brings them in.
- `google_delete_forever {folder:"spam", thread_id}` or `{folder:"trash", conversation_id}`: needs full access, else `reconnect_required`. Spam deletes the thread's SPAM messages; Trash deletes the stored conversation's trashed messages (Core67). Local copies are then forgotten. Returns `{folder, deleted}`.
- `google_empty {folder}`: needs full access; lists `in:spam`/`in:trash` (spam and trash included) 500 at a time up to 10,000, batch-deletes them, forgets local copies; returns `{folder, deleted, more}`.
- All run on the account's active sync (`sync_not_running` otherwise). GUI operation timeouts: 60 s, emptying 300 s.

Consumers: Core67, Mail UI87, CLI51.

## D118 — remote search and unsubscribe email (Gmail29)

- `GmailCall::thread` (`threads.get format=minimal fields=id,messages/id`, 10 units; 404 is `None`).
- `google_remote_search {query, folder}` lists 50 messages with the folder-scoped `q`, groups them by thread, drops threads with a stored message and reads metadata for up to 25 (eight at a time). Rows use the list row shape with `remote`, `remote_thread_id`, id `remote:<thread>` and `group: done`.
- `google_remote_thread {thread_id}` reads the thread's newest 50 messages as raw MIME under the preparation permit and returns a conversation-shaped reply (`remote: true`; messages `remote-<id>` with text, labels as unread/starred/sent, attachments `remote-att:<id>:<index>`; the newest carries `body_html`), trimmed to fit one IPC frame.
- `google_remote_attachment {id}` reads that message again and writes the part to `<profile>/cache/remote` (newest 16 kept), returning the `attachment` reply shape.
- `google_unsubscribe_mail {id, source_version}` sends one message (`messages.send`, 100 units) to a stored message's `List-Unsubscribe` mailto address and records it when Google accepts. A rejected send is `unsubscribe_failed`; no definitive response is `send_unknown` and records nothing. It needs mail write scope; the others need mail read. All need an active account sync and store no mail. Consumers: Core66, Mail UI85, CLI50.

## D116 — Spam stays at Google (Gmail28)

- Full listings use `includeSpamTrash=true` with `q` ending ` -in:spam` (Recent: `after:N -in:spam`; Inventory: `-in:spam` or `after:N -in:spam`). Trash is still acquired.
- A fetched message whose labels include SPAM is prepared as `Spam`: no source is staged, and publication removes any local copy and its coverage row. Delta label changes refetch as before, so leaving Spam restores the message.
- `GmailCall::message_metadata` (`format=metadata`, From/Subject/Date headers, `fields=id,threadId,labelIds,snippet,internalDate,payload/headers`, 5 units) serves live listings; a 404 is `None`.
- `google_spam_list` lists `in:spam` 50 per page and reads metadata eight at a time; `google_spam_message` reads one raw message under the preparation permit and returns its extracted text. Neither writes to the store. Both need an active account sync and mail-read scope. Consumers: Core64, Mail UI83, CLI49.

## D097 — first label import (Gmail27)

After a newly added account's labels worker publishes its first complete catalog, it runs the explicit name merge once. Existing Gmail user labels become or link local labels, and memberships already retained from Gmail appear locally. Nothing is created at Google and no label membership write is queued. The GUI and CLI refresh from the published revision. Without mail editing permission the import is skipped and Settings → Labels Sync with Google or CLI `labels sync` remain the path. Later Gmail labels still wait for explicit sync. Consumers: GUI Settings → Labels, CLI `labels sync`; Core54, Local assistance20.

## D093 — completed page payload release (Gmail26)

Finishing a live page clears each completed item's staging manifest JSON in the same transaction as the completion receipt. Keep manifest hash, part key, applied revision and page/coverage proofs so a repeated part commit returns its exact result after restart. Incomplete and uncertain page data stays available. This applies to newly created schema32 profiles; Core51 owns the storage boundary, Search6 the FTS representation and Quality68 the recovery checks. Fixture0/3 and protocol1 remain unchanged.

## D088 — acquired reply contributions (Gmail25)

Shared message preparation derives the cached reading/search body from new contributions, omitting recognizable reply history while retaining inline answers and explicit forwarded content. Retain exact RAW MIME and attachment blobs and their checksums; provider identity, labels, checkpoints, replay and authored draft/send handling do not change. Existing retained messages are not backfilled. Core48/Mail UI64/Quality64; CLI41, schema29, fixtures0/3/protocol1 unchanged.

## D077 — startup acquisition and send presentation (Gmail24)

The GUI resumes previously initialized connected accounts once after initial account/preferences loading, using existing sync ownership and persisted service choices. Exclude removed/disconnected accounts, never-started setup and both-disabled services. Explicit start/pause consumes pending startup work; polling must not rearm it or repeatedly request credentials. CLI lifetime and explicit ownership remain unchanged.

Send uses the existing immutable submission authority immediately after required local save/preflight. Production has no six-second Undo hold. Progress remains visible until the outcome is known. Acceptance hides only the unchanged accepted draft projection; preserve newer edits, exact receipts, conflicts and unknown recovery. Provider acceptance is not recipient delivery. Do not auto-resend or recreate a draft after acceptance.

Consumers: Gmail24/Core44/Mail UI55/CLI39/Quality55/schema29; fixtures0/3 and protocol1 unchanged.

## D073 — empty requests and draft fidelity (Gmail23)

Empty POST/PUT/PATCH submissions explicitly declare `Content-Length: 0`; this repairs Trash/Restore wire framing. Preserve the existing bounded transport, exact provider message identity, preflight/readback checks and unknown-outcome recovery. An HTTP response alone does not establish final filing success.

Draft upload/readback can produce different MIME bytes with equivalent authored content. Core43 records a conservative decoded-content comparison while retaining distinct upload and observed hashes. Later RAW changes still require conflict handling; no automatic replacement or repeated unknown create is added. Unsupported comparisons remain unknown, and content changes receive a warning rather than being hidden as serialization differences.

Consumers: Core43/Gmail23/Mail UI50/CLI38/Quality50; schema29, Calendar25, Local assistance14, fixtures0/3 and protocol1 unchanged. Whole-area readiness remains 0/9.

## D072 — account and model feedback (Gmail22)

Save the setup history choice before sync. Inventory uses the persisted UTC cutoff; the recent lane uses the later of that cutoff and its usual recent boundary. History responses older than the boundary are discarded before MIME/content storage and do not count as missing-content failures. Provider deletions still reconcile. Resync preserves the boundary. Existing schema28 accounts keep All. Meetings has no independent permission checkbox; archive deletion changes only local data.

Consumers: Core42/Gmail22/Local assistance14/Mail UI49/CLI37/Quality49, schema29; Calendar25, fixtures0/3 and protocol1 unchanged. Whole-area readiness remains 0/9.


D067 / P2-UI3 outgoing presentation. Outgoing presentation derives its durable state from the existing Send journal. The GUI-only six-second queue uses the exact saved authored version; once invoked, the frozen MIME and receipt are authoritative. Sending, prepared, unknown and accepted states remain distinct. An unknown submission never automatically retries. Exact accepted provider-message acquisition replaces the temporary presentation and cannot revive it after Trash/deletion. UI and CLI consume the same read-only queries; outgoing previews confer no provider mutation authority. Consumers: Core40/Gmail21/Mail UI45/CLI35/Quality45, schema28, fixtures0/3 and protocol1.

D065 / P2-UI3 (Gmail20) implements durable draft-discard Undo with exact revision/lifecycle checks, captured authored buffers and unfinished recipient input, shared CLI/GUI recovery and grouped six-second toast Undo. Schema28 adds the snapshot journal and recipient-input storage; fixtures0/3 and protocol1 stay unchanged. Capture/upload continues verifying retained bytes before use. Scoped Windows Core, engine, CLI and native checks, actual schema27→28 migration/refusal/reopen, strict lint and configured relocated package146 pass. The full prototype goal and provider-cleanup/Calendar-Undo/pending-Sent gaps remain open. See D065 and the prototype execution record.

D063 / P2-UI3 adds explicit From-account transfer with source revision and Google revision checks, verified retained content, destination ownership and durable exact-request replay. A move preserves the original local history and waits for the destination Google draft readback before removing the unchanged original Google draft. Unknown deletion outcomes only reconcile the same request; explicit retry starts a new deletion attempt only after a known rejection. Account lifecycle changes fence cleanup. The composer and CLI share transfer/status/retry; source provider IDs never become destination IDs. Schema 27 adds the bounded transfer journal and preserves existing receipt bytes. Core 38/Gmail 19/Mail UI 41/CLI 32/Quality 41, fixtures 0/3 and protocol 1. Scoped Windows Core/provider/native/CLI, actual migration/refusal/reopen, strict lint and relocated package115 checks pass. Full prototype alignment and other-host acceptance remain open; package94/schema26 is preserved in ignored relocation94.

D061 / P2-UI3 adds account-local label creation, descriptions and removal plus explicit Google name merging/creation. Removal clears local membership and mappings, retaining historical receipts; it does not delete Gmail labels or mail. Descriptions use the existing assistance settings and stay local. Schema 26 journals provider creation before dispatch; unknown outcomes require inventory reconciliation, with no blind retry. Unmapped local memberships wait for an explicit link/sync before provider delivery. The decision and [prototype execution record](../research/prototype-september-15.md) own the detailed scope and evidence.

P2-UI3 Google connection accepts optional `access:{mail,calendar}` with `none`, `read`, or `write` for each service; at least one service is required. Omitted access preserves the existing request. Read Mail requests gmail.readonly; write Mail requests gmail.modify. Calendar requests catalog/detail/free-busy scopes with read-only or writable event detail access. Meetings follow the Calendar permission because Google exposes creation through the Events API. The requested access changes the consent request, never invents granted capabilities; the successful step reports the actual verified grant (which can retain earlier Google permissions). Login remains explicit, PKCE/state/nonce and same-subject restore stay unchanged, and sync starts only from the user's final Start syncing action. Schema/fixture/protocol unchanged; Core 34/Mail UI 37/CLI 28 consume the same optional request.

State: Draft · Revision: 22 · Updated: 2026-09-16 · Implementation: D072 account/model feedback at schema29. Scoped evidence is recorded in the prototype execution report; whole-area readiness remains 0/9.

D055 / P2-LLM2: one shared model and visible template (`{{labels}}`, `{{messages}}`, `{{partial}}`), combined per-message output, default-on prospective summaries/automatic single-label classification, manual precedence, and summary list previews. Schema 22 fences settings and assisted delivery; fixtures 0/3/protocol 1 unchanged. This supersedes conflicting D051–D053 delivery defaults below; historical evidence remains scoped to its original version. See the D055 decision and Local assistance 8.


D054 adds explicit google_sync_refresh with empty parameters: start a session using published budgets if absent, or wake the existing account worker while retaining owners. Wake respects provider cooldowns and cancellation. Status exposes last_mail_sync_at; only completed Gmail history application advances it. Credential refusal blocks automatic repeated vault calls until explicit retry.

D050 uses built-in published Gmail 1,200,000 project / 6,000 user quota units per minute and Calendar 10,000 project / 600 user requests per minute, retaining 20% headroom, bounded retries and provider throttling. Empty sync parameters resolve centrally; complete legacy overrides remain supported. No user quota setup or saved quota preference is required. Linking a cached user label projects retained message membership immediately and repairs older missing projections on reopen without provider writes.

D040 adds an optional bounded email hint to native browser authorization for personal Gmail and Workspace alike. It is URL-encoded once as `login_hint`, never an `hd` restriction or account identity. Signed Google subject, issuer/audience/nonce, actual grants and existing reconnect/restore checks remain authoritative, including a renamed Workspace address. Keep the owned loopback, PKCE, state, cancellation and shared vault coordinator. Login uses the app registration by default; setup status exposes availability and a sanitized failure only. Sign-in does not acquire mail or Calendar or send anything. Consumers: Platform 15/Gmail 13/Mail UI 26/CLI 21/Quality 24 with Core 27/Calendar 20/Search 5/schema 20 and fixtures 0/3/protocol 1 unchanged.

Revision 12 applies D036/P2-01B lifecycle to mail work. Removed accounts retain cached conversations, authored/imported drafts, original RAW, attachments, routing, capture and provider journals. Exact cached reads/exports and receipts remain available, but archive browsing cannot mark read, edit, send, sync, reconcile through HTTP, copy/adopt or resume prepared work. New acquisition/draft/Send/deletion claims and source publication require current active lifecycle and connection generation. Preserve definitive late outcomes and existing uncertainty; removal never fabricates delivery, deletion or non-dispatch evidence. Explicit restoration keeps the same subject/account identity and retained content without starting sync or outbound work. Consumers: Core 24/Gmail 12/Calendar 17/Platform 13/Mail UI 22/CLI 17/Quality 20/schema 18; Search 5, fixtures 0/3/protocol 1 remain unchanged.

The [P2-01B evidence](../research/phase-2-auth.md#p2-01b-account-lifecycle-under-d036--windows-functional-interim) covers actual native/CLI processes using owned loopback OAuth with synthetic signed identities and in-memory provider/vault implementations. Final CLI/native account runs pass in 7.282/14.265 seconds; the native Restore proof compares fresh pre-Restore provider counters with the authoritative final drained report. Ordinary workspace tests pass 547 with five ignored; isolated engine tests pass 156 unit plus four process cases; feature CLI tests pass five unit plus 15 process cases; full native CTest passes 18/18, including its Rust build fixture. [Retained evidence](https://github.com/yegors/fuckMail/blob/8155e998d5957e60c491475df7877553db2b453b/docs/research/assets/phase-2/p2-01b-interim/logs-manifest.json) includes earlier failures and native diagnostics. These results do not establish actual-account consent, native-vault cleanup, Send/invitations, performance or package acceptance.

Revision 11 adds explicit remote-draft authoring and deletion under D033. A plain-text copy follows ordinary local autosync; adoption requires a fresh exact-target RAW check but performs no initial upload. The first edit and immediate explicit Send retain the adopted container without inventing an ACK. Same-target editing initially accepts only completely representable plain-text MIME and ordinary attachments; unsupported semantics remain inspectable or explicitly convertible. Remote-draft deletion uses a distinct journal, exact verb/path quota admission, fresh retained source check and one claimed DELETE. Definite outcomes and uncertainty persist through cancellation/disconnect; explicit reconciliation only reads and never retries. Received-mail permanent deletion remains unavailable. Consumers: Core 22/Mail UI 20/CLI 15/Quality 18/schema 16; fixtures 0/3/protocol 1 unchanged.

Revision 10 adds D032 remote Drafts reads under an existing explicit account-sync lifetime. A newly created readable-account sync job begins/resumes one durable inventory; repeated owners of that job do not restart it. Explicit refresh begins a scan after completion or coalesces an incomplete one and returns cached progress immediately. One worker performs one list or RAW/import step at a time using the shared BackfillRead quota and memory admission, releasing permits between items. There is no periodic full re-enumeration in this slice and no work from cached reads/view changes. Read grants are independent of verified From/write capability. Import/view/refresh issue no provider mutation. Consumers: Core 21/Mail UI 19/CLI 14/Quality 17/schema 15, with fixtures 0/3/protocol 1 unchanged.

Use stable draft container identity independently from its replaceable contained-message ID; a newer GET version is valid. Preserve exact RAW, routing, HTML and CID relationships even when the bounded text preview is incomplete. Missing/omitted drafts do not establish sending or deletion, and a result estimate is not inventory completeness. Read-only snapshots stay in the existing separate-account Drafts surface. Supported adoption/editing, conflict decisions and explicit permanent remote-draft deletion remain a required B3B follow-up; B3A does not claim complete Drafts.

Revision 9 adds explicit, strictly validated Google Send under D029. Frozen MIME accompanies a known nonconflicting draft target; direct submission cannot bypass pending or unknown draft work. Admit the actual submission quota before retaining queued upload bytes. Unknown acceptance is never automatically replayed; read-only reconciliation retains bounded evidence without promising deduplication or recipient delivery. Consumers: Core 20, Mail UI 18, CLI 13, Quality 16 and schema 14; fixtures 0/3 and protocol 1 remain unchanged.

Revision 8 connects local authored drafts to costed Gmail create/update and RAW readback. Local autosave remains independent of HTTP; owned synchronization coalesces current revisions and never submits synthetic captures. Separate draft container and message identities, require acknowledged-ID readback before publishing a baseline, and retain uploaded/fetched RAW separately. A changed ID/hash or missing twin stops automatic overwrite, preserving local work and fetched evidence; unknown acknowledgment never triggers automatic creation again. Explicit replacement checks the current local/conflict revision and discloses the remaining fetch-to-update race. Recipient-free drafts are valid preparation inputs; explicit Send remains a separate strict operation in P2-05B2. Use verified primary mailbox identity and actual granted capability, never retarget a saved From. Consumers: Core 19, Mail UI 17, CLI 12, Quality 14 and MIME; schema 13/fixtures 0 and 3/protocol 1, Calendar 15/Platform 12 unchanged.

An unknown update with a retained stable target may be checked with a fresh RAW read. If that exact target still exists, the retained observation permits an explicit replacement using a new request ID and the current local and Google revisions. This creates a new operation; the original unknown receipt remains unknown and does not become proof of acceptance. Unknown creation and an unknown update whose target is missing have no safe replacement target and stay blocked. Missing never means sent. Exact historical request replay is identified separately from current draft status and cannot regress the current UI projection.

Queued upload work retains only bounded identifiers until quota admission, then loads/encodes the frozen blob under bounded preparation memory. Enforce 35,000,000 MIME bytes before dispatch; create/update use their actual method costs and the existing per-account mutation lane. RAW readback uses its read quota and releases it before waiting for mutation admission. Explicit owner lifetime covers the durable late outcome. No draft byte mismatch is silently declared equivalent; remote-only discovery/import/deletion and real-provider fidelity remain subsequent required acceptance.

Revision 7 adds cached existing-label discovery and explicit local category binding. Owned synchronization refreshes the complete labels list initially and near ten minutes, with explicit refresh sharing quota/memory admission. Preparation and bounded generation chunks precede atomic catalog publication; queries never start HTTP. At most 10,000 active labels/4 MiB metadata and ordinary 64-label/128 KiB chunks bound this path. Only an explicitly selected, present user label may create or remap a local category; system/missing labels are ineligible. Rename/deletion does not rewrite configured category identity or human intent. Request receipts and catalog/prior-mapping checks prevent stale binding. Exact operation delivery counts all targets and distinguishes pending, uncertain, failed, superseded and fully confirmed outcomes. Consumers: Core 17, Calendar 15, Mail UI 15, CLI 10, Platform 11 and Quality 11; schema 11, fixtures 0/3 and protocol 1.

Revision 6 applies D023/Core 16/schema 10 to P2-05A. Explicit read/Done/Delay, mapped categories and archive/trash/restore persist current member IDs and per-field intent revisions alongside their existing local receipt. Delivery reads the versioned provider label state before claiming one mutation attempt; accepted or uncertain responses require reconciliation. A newer local field choice cannot be cleared by an older response. Label mapping changes fail affected work explicitly and preserve human intent; category assignment never creates labels or archives implicitly. Trash stays available through the explicit Trash folder and is excluded from Inbox/Sent/All; restoration removes Trash without inventing Inbox. Attempt receipts survive local source removal. The one-shot state lane accepts only message modify/trash/untrash, with actual method units; remote drafts/send remain P2-05B. Consumers: Core 16, Calendar 14, Mail UI 14, CLI 9, Platform 11, Quality 11 and Search 5/Triage 6. Fixture 0/3 and protocol 1 are unchanged.

Revision 5 applies D022/Core 15 to live acquisition. Gmail API page sizes do not bound history fan-out or MIME/index work. A live page therefore has durable bounded internal parts: each publishes source, normalized rows, FTS and retained human intent atomically with its receipt; the provider continuation advances only after all items have durable outcomes. Already committed parts may be visible while a page remains incomplete. This explicitly supersedes whole-API-page invisibility for the live path; Revision 3's synthetic path and tests remain unchanged. Independent recent/inventory/delta lanes preserve newer per-message history and never reset delta progress to an old inventory starting cursor. Oversized, malformed, missing and unindexed sources remain explicit coverage states. Consumers: Core 15, Search 5, Mail UI 14, CLI 8, Calendar 13, Platform 11 and Quality 11; schema 9 metadata, live fixture 0, synthetic fixture 3 and additive protocol 1.

Revision 4 prepares D020/Phase 2's real personal Gmail and Workspace adapter. Existing fake transport/auth/store evidence remains synthetic. The agreed method inventory drives desktop OAuth scopes, identity and service capabilities; one engine token coordinator and OS vault serve mail/calendar and GUI/CLI. Validate stable Google subject identity and actual granted scopes. Verify the existing Cloud configuration, Workspace policy, current per-method/project/user quotas, bandwidth and sending limits; desktop incremental authorization is not assumed. Dated [Phase 2 research](../research/gmail.md#phase-2-planning-inputs) separates official limits from proposed scheduling defaults.

Implement recent-first RAW/full acquisition, resumable whole-mailbox inventory and attachment retention with ongoing history deltas. Preserve exact account/thread/message/label identities and publish source/index/checkpoint changes atomically. History 404 triggers staged reconciliation, not a profile reset. Handle oversized/malformed sources explicitly without poisoning a whole page or claiming complete coverage. Separate foreground/local navigation from network latency and budget historical work fairly. Snapshot individual existing message IDs for read-on-open, Done, Delay and category/filing actions; retries cannot capture later thread members. Inbox removal, Trash/restore and permanent deletion remain distinct; permanent deletion stays unavailable in the pilot.

Local authored drafts remain durable while remote draft updates coalesce. Google draft IDs survive content-message replacement and must not be treated as received arrivals. Prove provider conditional-write behavior before promising race-free remote draft overwrites; otherwise preserve local/known fetched versions, stop on detected divergence, require explicit conflict replacement and disclose the remaining remote race. Primary From identity is verified; alias administration stays later. Live Send freezes authored MIME, account and recipients into the existing operation model before a single submission. Lost responses/restart become acceptance-unknown and never trigger automatic resend; Sent/history/Message-ID matching supplies evidence rather than an exactly-once guarantee. Calendar owns invitation delivery. Consumers: Core 14, Calendar 12, Mail UI 14, CLI 8, Platform 11, Quality 11, MIME and Search. Exact methods/types/fixtures are agreed in P2-00; schema 7/fixture 3/protocol 1 remain the implemented baseline.

Revision 3 records the next Phase 0 integration checkpoint. The fake provider now publishes complete pages and FTS changes atomically with Core checkpoints and replay receipts. Staged or failed pages stay invisible, and pending manual flags/categories win over observed labels. Shared corpus imports verify retained bytes and safe text. Consumers: Core, Search, MIME, Triage and assistance. Real authorization and HTTP remain separate gates.

Revision 2 records D012’s implemented experimental slice. The fake transport, OAuth/PKCE, token-coordination and uncertain-send experiments are implemented; [execution evidence](../research/google-feasibility-execution.md) records prerequisites and limits. No live account has been connected. Consumers: Core, MIME, Calendar and Platform.

Revision 1 applies D009: Gmail/Workspace APIs are the first mail transport, including send; shared MIME handling stays early while IMAP/SMTP are deferred. Affected consumers: Core, Mail UI, Calendar, Triage, Local assistance and CLI.

Owner: B — Gmail. Accepted scope and proposed behavior are separated below; concrete APIs, libraries and outstanding choices await the relevant feasibility work. See [contract workflow](README.md).

## Build boundary

| Boundary | Responsibility |
| --- | --- |
| Owns | Google mail authorization flow, account discovery, initial/delta sync, labels/read/stars, bodies/attachments and draft/submission mapping |
| Consumes | Core account identities, checkpoint/journal contracts, credential references, explicit message targets and category policy |
| Produces | Normalized mail changes, capabilities, label mappings, coverage and durable mutation/submission outcomes |
| Independent start | Fake transport and recorded synthetic responses for pages, quota limits, expired history and write failures |

Related: [core](core-storage.md), [triage](triage.md), [calendar invitations](calendar.md), [Gmail research](../research/gmail.md).

## Required behavior

Prefer the Gmail API for Google accounts. Preserve provider thread IDs and message-level labels; conversations and storage never cross accounts or duplicate bodies for every label. Fetch recent conversations first, interleave deltas and full history/attachment acquisition, and use bounded fair scheduling. Provider quotas determine attainable import speed; no fixed whole-mailbox estimate is promised.

Use the system-browser installed-app authorization flow and OS credential storage. Prove durable personal/Workspace access, not just one successful login. Poll adaptively while running; reconcile after wake/reconnect. Commit history checkpoints only with applied pages; expired history triggers staged reconciliation preserving drafts and pending local intent.

Map categories to configured user labels. Apply changes to explicit existing message IDs; preserve unrelated labels, UNREAD and STARRED. Inbox removal is a separate configured filing effect. A later thread member does not inherit a completed read/unstar/label command. Surface mixed conversation membership, mapping changes and pending/failed writes. Provider mutations from manual use and validated auto-apply share Core's journal; the model never accesses Gmail directly.

Submission follows Core's accepted/failed/acceptance-unknown states. Mail invitations coordinate with Calendar's event identity and attendee notifications; do not independently send duplicate scheduling email.

## Acceptance gates

- Resume paginated import with interleaved new mail and attachments; replay without duplication and recover an expired history cursor without losing user work.
- Validate multi-account identity and durable OAuth on the actual personal/Workspace configurations; measure actual project quotas and backfill behavior.
- Apply/read/reconcile labels, stars and unread state across another client. Archived unread mail remains in the account totals; new replies remain independently reviewable.
- Exercise renamed/deleted labels, manual changes during inference, offline writes and uncertain submission. No late result overrides newer intent.

## Open before affected behavior ships

Record actual Cloud project limits, account types, required scopes and sustained authentication behavior. Finalize label setup/archiving policy and provider-draft synchronization. Public verification/distribution is deferred, but its requirements must be revisited if the audience changes. Protocol and category evidence is in the linked research, not a measured implementation result.

## D061 explicit label synchronization

Google user-label names are imported and missing local names are explicitly created with `users.labels.create` (five quota units). Descriptions stay local. The loopback IPC request starts a background operation; eight jobs may run across accounts, at most one per account, with at most 64 retained results. Same-owner/account/request replay returns the existing job. Other accounts or sessions cannot read or cancel it. Shutdown cancels and joins admitted work before closing Core.

HTTP 400/409 is a known create rejection; transport interruptions and malformed successes retain uncertainty. Never automatically retry an uncertain POST. The next explicit inventory can bind an exact unambiguous name and preserve its original local descriptions. Missing linked Google labels and skipped/conflicting names are reported. Name synchronization never deletes Gmail labels or messages.
