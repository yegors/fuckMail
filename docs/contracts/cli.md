# CLI and external agent interface

State: Draft · Revision: 5 · Updated: 2026-09-12 · Implementation: Phase 1 shared local commands implemented and exercised on Windows; whole area remains Draft.

Revision 5 records the implemented local CLI boundary: grouped/typed mail query, explicit review/Done/Delay/undelay, signature/formatted/retained-attachment reply save, discard/capture history, local preferences/account metadata, combined calendar and deterministic local fake create/edit/self-RSVP commands. Consumers: Core 10, Mail UI, Triage, Search 3, Calendar and Platform. Exact grammar is `fm --help` and [local protocol](../local-protocol.md); protocol 1 remains additive, schema 5/fresh fixture 2 preserve old fixture profiles. Live provider/credential/model execution remains later work.

Revision 4 applies D014 and the [Phase 1 plan](../plans/phase-1.md): query/action, reply/capture and combined-calendar parity grows alongside the updated GUI. Consumers: Core, Mail UI, Triage, Search, Calendar and Platform. Current runtime remains schema 4, protocol 1 and fixture 1; this revision does not invent executable command names or mark planned commands implemented.

Revision 3 records the next Phase 0 integration checkpoint. The draft response adds authored HTML and retained attachment IDs; structured draft_save can preserve or explicitly clear them. Existing plain-text commands clear stale HTML when changing the body. Lost-response replay survives an actual test-engine kill. Consumers: Core, Qt UI and MIME; protocol/fixture revisions remain 1.

Revision 2 records D012’s implemented experimental slice. The implemented partial `fm` CLI is documented in [local protocol](../local-protocol.md); `fm --help` is the current command reference. It shares the engine with Qt and has no Qt dependency. Consumers: Core, Mail UI and Platform. Future command descriptions below retain their proposed status.

Revision 1 applies D009: a small real CLI ships in the first usable GUI slice; coverage grows with Google workflows. Legacy transport/feed commands are deferred until founder dogfooding. Affected consumers: Core, Mail UI, Gmail, Calendar, Local assistance and Platform.

Owner: I — CLI. A full, feature-rich CLI is required alongside the desktop UI on Windows, macOS and Linux. Third-party agents use it for external workflows. Exact command names and schemas below are proposals; this is not a runnable command reference.

## Build boundary

| Boundary | Responsibility |
| --- | --- |
| Owns | Command discovery/help/completion, argument and structured-input handling, human/JSON output, exit/error semantics and compatibility tests |
| Consumes | Core's versioned account-scoped queries, commands, capabilities, revisions, operation journal and bounded event/change stream |
| Produces | Explicit user/caller commands and machine-readable results with IDs, coverage and operation outcomes |
| Independent start | Fake local engine and versioned fixtures; no desktop display, live provider, model or GUI automation needed |

The GUI and CLI are peer clients of the same local core. Providers, search, calendar and category policies have one implementation and one store. CLI is a public product interface; direct SQL, scraping the UI or changing private database files is not its API.

## Phase 1 boundary — implemented local slice

CLI coverage lands with each shared GUI workflow. P1-00 records the supported request/output shapes, capabilities, bounds and command grammar in `fm --help`, [local protocol](../local-protocol.md) and [fixture API](../../fixtures/README.md). A Windows 19-call live CLI-to-engine smoke exercised synthetic account/filter/actions/preferences, formatted attachment reply/capture replay/discard/history and local calendar creation replay. This is bounded local evidence; native UI parity and other-host delivery remain integrated gates.

- **Mail queries/actions:** typed sender/status/category/attachment/date filters, stable conversation grouping in Inbox and Sent, explicit count units and honest coverage. Reveal matching-member IDs. `mail review`, `mail done`, `mail delay` and `mail undelay` expose explicit review, atomic snapshot Done (read and clear stars), and untimed Delay/undelay using the same journal and stale-revision rules as the GUI. Fetching source content is separate from the GUI's user-open-triggered review action; an external reader cannot silently mark mail reviewed.
- **Replies:** `draft save/show` reads/saves plain or formatted authored content, per-draft editable signature snapshots and selected retained attachments; explicit discard and local MIME capture with replayable operation outcomes and bounded capture inspection linked to the original conversation. First-save From/signature snapshots and subsequent explicit signature edits participate in the saved revision and captured-content guarantee; preference changes cannot rewrite an existing reply. A local capture never claims provider delivery. The CLI accepts content through bounded files/stdin and preserves the draft on uncertain capture. `draft history --unresolved-only` filters prepared/in-flight/acceptance-unknown operations before paging, so newer accepted history cannot hide an old unresolved capture; known rejections remain distinct. New mail, forward and local-file attachment import can follow in Phase 2; live submission is outside Phase 1.
- **Calendar:** query the same combined account/calendar scope, permitted event details and bounded occurrences as day/week/month/agenda and the mail timeline. Preserve each event's owning account/calendar on output and require that target for a mutation. Add bounded nonrecurring create/edit and self-RSVP through deterministic fake acknowledgement; recurring-series edits and live writes stay deferred. `calendar-operations --limit 30 --cursor '<returned continuation JSON>'` inspects the durable account journal, with pending/failed operations before acknowledged operations. Forward the returned continuation object unchanged; each page is bounded and restarting inspection does not execute a write.
- **Settings/state:** `preferences show/save`, `account-update` and `status` expose typed local settings, metadata and diagnostics. Account names/signatures have metadata revisions; calendar/timeline selection and dates are app-wide, independent of mailbox selection. Mouse gestures, animation and layout controls need no artificial CLI equivalent. Do not add credential setup, fake OAuth completion, model downloads or legacy-provider commands to fill out the prototype's pictured settings.

Each packet verifies GUI-to-CLI and CLI-to-GUI readback where applicable, restart/replay, foreign-account rejection and stale revisions. Calendar scope/permissions apply before serialization in either interface; query filtering cannot fabricate all-clear or offline-complete status. Retain explicit client lifetimes and last-owner shutdown. Extend the small real CLI incrementally; exhaustive grammar, long-lived streaming and bulk tooling remain later product work.

## Required functional coverage

Coverage below is the intended product surface, delivered incrementally. Phase 0 provides account selection, bounded conversation list/search/read, explicit flag/category actions, durable human drafts/capture and basic calendar queries alongside the GUI. Phase 1 expands the local workflows above, including a bounded calendar edit slice; setup/sync/live submission arrive with their corresponding Google workflows. Do not finish all command grammar, streaming, bulk tooling or assistance commands before expanding the GUI. By the pre-legacy dogfood gate, both interfaces must cover the agreed everyday Google workflows and be exercised by the founder. IMAP/SMTP and external-feed commands enter only with the later adapters.

| Area | CLI capabilities |
| --- | --- |
| Accounts and setup | Discover/select accounts, inspect provider capabilities, configure accounts/categories/local assistance, initiate explicit authentication and report actionable setup state |
| Mail and search | List/search conversations, inspect specific messages/headers/bodies and source representations, fetch/save attachments, expose full offline/index coverage |
| Mail management | Read/unread, star/unstar, labels/folders, category mappings, archive/trash/restore and explicitly scoped bulk operations with provider-specific semantics |
| Human-authored mail | Create/edit/reopen drafts, recipients/signatures/attachments, reply/reply-all/forward and explicit submission; preserve uncertain-send handling |
| Calendar | Google calendar visibility/permissions, day/range/agenda/search, event edits, recurrence, RSVP and shared calendars; read-only feed commands later |
| Optional assistance | Inspect/configure mode and runner availability, request allowed local classification/summaries, accept/correct results and cancel an explicitly scoped batch |
| Operations | Explicit sync, inspect progress/errors/pending operations, retry or cancel where valid, coverage, version/capability/schema discovery and useful diagnostics |
| Change consumption | An explicitly invoked foreground watch/change stream with cursors, reconnection and bounded output for external automation |

GUI-only presentation controls need no artificial command equivalent. Every applicable core workflow is eventually reachable through CLI; a read-only exporter is not the final product. A useful partial CLI is explicitly acceptable at the first milestone and grows with the GUI. Unsupported provider capabilities return explicit errors, not invented parity. MCP servers, hosted-agent integrations and built-in scheduling are not prerequisites; external tools can wrap the CLI.

## Interface rules for agents

- Provide versioned JSON input/output and line-delimited JSON for streaming/batches; stable account/conversation/message/event IDs, bounded pagination and documented filtering/field selection. Keep human diagnostics on stderr and structured results on stdout.
- Document consistent exit codes and distinguish invalid input, authentication required, unavailable capability, stale revision, partial/batch failure and unknown provider outcome. Do not silently prompt or launch a browser in noninteractive operation; an explicit setup/auth command may do so.
- Writes require explicit account and target scope, with expected revisions and client request IDs where appropriate. Snapshot bulk targets; new arrivals must not inherit an earlier command. Offer preview/dry-run for applicable bulk/category changes.
- Report local acceptance, queued/offline, provider confirmation and acceptance-unknown distinctly. Provide bounded waiting/progress/cancellation. Retrying a request ID can deduplicate local intent; it does not promise exactly-once SMTP delivery.
- Accept bodies and structured payloads from files/stdin, so callers need not put message content or secrets in shell arguments. Do not mix binary attachment bytes into JSON; use an explicit output destination or stream mode.
- Help, discoverable capabilities/schema and examples ship with the CLI. Record compatibility/version rules before agents depend on command output. File/terminal content is data, not executable instructions.

Configured automation uses explicit commands without a mandatory GUI approval for every ordinary write. Callers remain responsible for the authority and scope of their requested actions. Keep provider permissions and the same operation validation used by the GUI.

## AI scope and authorship

Built-in AI remains local classification/summarization only; it has no drafting or sending tool. External agents can consume selected mail/calendar data and drive the CLI under the user's control. The app does not choose an external agent's model or silently route mail to one.

The existing “humans write email for humans” preference still governs outgoing content. CLI draft/send capabilities support human-authored mail and explicit submission; adding an agent interface does not introduce an internal writing assistant. Text provenance cannot be reliably inferred by the CLI. External caller policy, including whether a particular workflow needs human review, belongs to that integration; do not claim this interface can prove authorship.

## Headless execution and lifecycle

Proposed architecture: an on-demand local engine owns the store/journal and serves both GUI and CLI. CLI commands work with the GUI closed and without loading Qt Quick/WebEngine or requiring a display server. GUI and concurrent CLI clients must not run independent migrations, provider submission loops or conflicting writers.

Closing the GUI stops its sync/inference/reminder session. A separately invoked CLI command or foreground watch has its own explicit lifetime and can run without the GUI. End its work on completion/cancellation; when no client/job is active, stop the engine. Durable queued intent can remain on disk without continuing execution. Do not install an always-running daemon, startup service or scheduler by default.

Use a private authenticated local transport with OS-appropriate access checks, protocol negotiation and single-engine ownership. A local endpoint name alone is not authorization. Locked/unavailable credentials return an actionable error; headless mode must not fall back to plaintext credentials.

## Acceptance gates

- Exercise implemented Google mail, labels, human draft/submission, calendar and sync through CLI-only fixtures on every required platform, including Linux with no display session. Initial OAuth/keyring setup has its own explicit prerequisites.
- Run the same operations through GUI and CLI and compare resulting state, coverage, flags, category overrides and provider outcomes.
- Validate JSON schemas, pagination, Unicode/multiline input, binary outputs, exit codes, noninteractive failures, partial batches and cancellation.
- Race GUI edits and multiple CLI clients, restart the engine and repeat a client request ID; preserve drafts/newer intent and never blindly resubmit an uncertain send.
- Close the GUI during an independent CLI command, end all clients and verify lifecycle behavior. Ordinary installation must make the CLI discoverable from the terminal.

Related: [Core](core-storage.md), [Platform](platform.md), [quality](quality.md), [CLI research](../research/cli-and-local-engine.md). Open: executable/command naming, first schema/version, stream protocol, auth provisioning for unattended use and packaging-specific command discovery.
