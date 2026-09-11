# Phase 0 — paired foundation and Google feasibility

Updated 2026-09-11 under [D009–D011](../decisions.md). **Future execution plan; not started.** [Development plan](../development-plan.md) owns sequence; [contracts](../contracts/README.md) own behavior. All areas remain Draft.

## Starting point

The repo contains docs and nonfinal mockups only. Read-only inspection now covers this Apple Silicon Mac as well as the earlier Windows PATH lookup; see the [host observations](../research/linux-and-distribution.md#development-on-the-work-mac-and-home-windows-pc). Missing dependencies and the actual Windows configuration need inspection/setup when implementation is requested.

Proposed baseline: Rust engine and small CLI, C++20/Qt Quick/QML GUI, private local IPC, SQLite/FTS5 and retained content. QML expressions are accepted; Node.js and a separate JavaScript compilation/bundling pipeline are excluded. WebEngine is not required for this phase. No versions are selected.

## Start and order

### Proposed first deliverable: a runnable desktop slice on Mac and Windows

Under [D010](../decisions.md), development must move between the founder's work Mac and home Windows PC. The first work package combines the bounded portions of P0-00/P0-01/P0-03/P0-10/P0-04/P0-11 below. It is a proposal for the next implementation task, not authorization to start or a claim that all Phase 0 gates fit in it.

**What the founder can use:** a Qt Quick desktop app, a small headless Rust CLI and one Rust engine/SQLite store. Seed a disposable profile with two synthetic accounts, a few hundred messages in conversations, a small attachment corpus and a week's calendar events. Follow the [D011 initial-phase references](../research/design-references.md): top accounts, split reader, inline reply, persistent calendar footer and settings shell. Detailed settings controls remain excluded per founder clarification.

The acceptance walkthrough is deliberately small:

1. Launch the app, switch accounts, search by sender or subject/body keywords, and open a conversation and its retained attachment.
2. Explicitly change read/star state from either client and observe the same scoped state from the other. Use these literal actions until Done/Delayed mappings are reviewed.
3. Write and save a plain-text reply draft; close/restart and recover its content. No actual sending in this work package.
4. View a read-only fixture week using the latest calendar composition, with today's events also in the persistent footer and a CLI range query. Event editing, recurrence expansion and the other complete views are subsequent slices.
5. Exercise concurrent clients, stale/invalid requests, account scoping, draft persistence and final-client shutdown through the actual local IPC boundary.
6. Enter/leave the settings shell while preserving the selected account, prior mail/calendar position, any draft and today's footer. Ignore the pictured Mail/Calendar switches; no detailed settings, provider/model setup or generic configuration framework is needed.

**Development deliverables:** compatible pinned tool/dependency versions, repeatable Mac shell and Windows PowerShell setup, common build/test/run/package entry points, a diagnostic command, dependency caches, native macOS/Windows CI jobs with retained development artifacts, and an agent-readable setup/handoff guide. Shared configuration should own the work; OS wrappers handle installation and native tool discovery. Track source/configuration/fixtures in Git, with machine paths, build output, profiles and credentials kept local. A fixture reset must target only its disposable profile.

**Completion evidence:** launch/use the slice on this Mac, run both native CI builds and applicable automated tests, and retain exact source/tool/runner versions plus the artifacts and walkthrough. Verify the physical home Windows GUI/input/runtime when that machine becomes available; CI is not evidence of its desktop behavior. Record an unavailable runner as blocked evidence. Linux remains a product requirement; its full distro/desktop qualification belongs to later gates.

This package has no Google-account, final-design, model or signing prerequisite. Development artifacts can be unsigned; trusted distribution is later work. Full CLI coverage, real provider sync/send, calendar editing/recurrence, large-corpus performance claims, complete settings and received-HTML display are outside this bounded deliverable. The persistent store and IPC are intended to grow into the Google client. Resolve failures in the proposed stack before expanding features.

### Packet sequence

Start **P0-00 plus the small-fixture part of P0-01**. Pair P0-03/P0-10 with P0-04 early: a useful CLI and functional GUI must exercise the same engine before the CLI command surface grows large. Test IPC permissions, account scope and recovery through both clients.

P0-02 prepares Google access, P0-05 handles shared MIME/text/outgoing content, P0-06 covers Google calendar semantics, P0-07/P0-09 cover user actions/categories, and P0-11 packages the small pair. P0-08 can use a fake runner; models are not a first-use prerequisite. Legacy IMAP/SMTP and external calendar feed experiments previously attached to these IDs are deferred to Phase 4. Their research remains available.

Each packet names its purpose, owning contract revision, consumed fixture revision, small output and observed pass/fail/blocked result. Missing hosts, credentials or final designs block their own evidence row. This documentation update does not authorize implementation, real account access or model installation.

## P0-00 — native build and host inventory

**Owners:** G/A/I/E. Inspect Rust, C++ compiler, Qt Quick/text modules, Cargo, CMake/Ninja and available desktop/test hosts. Create the smallest reproducible Rust workspace and Qt application when asked to implement. No ABI bridge is required for separate executables.

Candidate targets remain Windows x64, macOS Apple Silicon, Ubuntu/Arch/Omarchy x64, subject to actual hardware and supported artifacts. Use normal native builds without a Node/JS build step. Pin versions only after they run. A later HTML dependency must not enter the bootstrap by accident.

**Pass:** actual commands/versions, a Rust test/executable, a Qt surface, and an honest available/unavailable host matrix. Missing hosts stay unverified; no all-platform claim follows from one build.

## P0-01 — small shared fixtures and scale design

**Owners:** A/D/F with consumers. Define separate Google-shaped accounts with colliding remote-looking IDs, local message/conversation identities, mixed labels/read/star flags, long threads, MIME sources, HTML-only inputs, present/missing/partial attachments, human drafts, operation states, recurring events and limited permissions.

Include classifier results for human correspondence, bots/lists, noise, ambiguity and account/category/scope tags. Expected outcomes must show that tags cannot create Done/Delayed or change read/star state. The initial fixture is small; design the seeded 100k/1m/3m and 25 GB/100 GB scenarios separately. The existing 25 GB snapshot is not a message count.

**Pass:** agreed identities, fixture revision and independent expected outcomes. Keep small synthetic fixtures in source; generate bulk content outside it. Do not implement IMAP folder moves or HTTP calendar feeds for this first manifest.

## P0-10 — one engine, a small CLI and the GUI

**Owners:** A/I/G/E. With P0-00/P0-01 and P0-03's durable portion, test a private local stream: Windows named pipes and Unix sockets are candidates. Evaluate versioned bounded framing, account-scoped paging, one journaled flag action, draft save/status, a calendar range query and cancellation. Binary attachments stay out of JSON row pages.

Use the same command/query fixtures from Rust CLI and Qt adapters. Test access/peer permissions independently of endpoint naming, concurrent launches, stale revisions, slow/disconnected clients, engine restart and incompatible versions. An account selector in the GUI is not an engine permission check.

GUI and CLI own explicit sessions; ending one must not cancel another's valid work. Stop the engine when the last owner ends; durable queued intent is not a keep-alive. CLI must run without Qt/display dependencies.

**Pass:** both clients see and change the same scoped state; one writer, bounded queues, deliberate errors and recovery; documented permission/lifetime traces. Do not implement every command, completion or watch feature before joining the GUI. Resolve the shared boundary from this evidence before expanding it.

## P0-03 — durable storage and useful search

**Owners:** A/D with I/E. Evaluate controlled SQLite/FTS5 via a Rust adapter, short transactions, bounded reads and one writer. Never hold a transaction across network/model awaits. Start with replay, draft/action/checkpoint ordering and blob staging/finalization; SQLite and files do not form one automatic atomic transaction.

Keep source MIME, HTML and attachments even when the reader shows text. Index normalized source text, not generated summaries. Test sender punctuation/partial matching, subject/body queries, account filtering, conversation grouping and matching snippets. Add a 100k report after small correctness passes; retain the larger tiers for later evidence.

**Pass:** persisted drafts/actions/content survive tested interruptions, basic queries are correct and both clients consume bounded results. Record actual cache/host/query-plan/p50/p95/p99 context. A process kill is not proof of power-loss durability. Disposable fixtures can precede real-data retention/encryption decisions.

## P0-04 — functional text reader and human composer

**Owners:** E/G with A/B/F/I. Build from the latest initial-phase trio: top account context, resizable conversation list/reader, accessible earlier members, inline reply and today's bottom calendar strip across mail/calendar/settings. Join the actual engine/CLI slice promptly. Settings is shell/navigation plumbing only; the founder said to ignore its unfinished switches. Exact fonts, compact behavior and detailed later controls still need refinement.

Read plain-text parts and safely extracted text from HTML-only fixtures. Verify quotations, links, Unicode, copy/selection, long threads, attachments and unavailable-content states. Preserve original sources. A Qt text/document composer should support human replies, signatures, undo/IME and durable drafts; try the restrained formatting already intended without introducing a web editor prerequisite.

Round-trip draft text/formatting/attachments through the shared Rust MIME path and a local Gmail API fake/capture sink. Test account switching and permission failures, updates/disconnects, draft restart and UI responsiveness. Do not implement an SMTP server/client as a prerequisite.

**Pass:** readable human mail, a recoverable authored draft, shared scoped state and usable mouse/input behavior in both clients. This gate excludes received-HTML visual fidelity and WebEngine. Best-effort HTML viewing moves to the later Google polish phase with separate resource/isolation tests. Source parsing and safe text extraction remain early.

## P0-02 — Google setup, sync and sending

**Owners:** B/F/G/I. Plan installed-app OAuth/PKCE, shared credential references and coordinated token refresh for Gmail/Google Calendar. Consumer plus Workspace access needs a viable audience/admin strategy and durable token policy. Use fakes until account/project access is authorized.

Mock retrieval/history paging, expired cursors, rate limits, interrupted attachments, label/flag edits and Gmail API sending. Preserve durable intent and ambiguous outcomes. Shared MIME generation remains needed; SMTP transport does not.

**Pass:** source-backed setup prerequisites and correct synthetic reconciliation/send states, followed later by actual personal/Workspace auth longevity and controlled live workflow evidence. Missing access blocks only the live row. Do not infer import duration or full offline coverage from a recent-mail demo.

## P0-05 — MIME and readable content, not legacy transports

**Owners:** A/B/E with the mail-library owner. Evaluate incoming MIME parsing, outgoing message building and text extraction using the shared corpus. Existing mail-parser and lettre builder candidates may be tested without enabling SMTP transport. Record maintained versions/features/licenses only when chosen for the experiment.

Test encodings, multipart/alternative, HTML-only human mail, inline resources, attachments, quoting, correct reply headers and malformed/large bounded inputs. Retain originals and compare decoded content/checksums. Generated MIME is captured locally or passed through a Gmail API fake; GUI and CLI share one serializer.

**Pass:** useful extracted content and preserved human draft/signature/attachment round trips. No browser-level HTML rendering required. IMAP session libraries, SMTP submission peers and folder-move experiments stay in the retained research until the post-dogfood phase.

## P0-06 — Google calendar identity and time

**Owners:** F/A with B/E/I/G. Test Google event/series/occurrence identity, permissions, time zones, all-day dates, recurrence exceptions, moved/canceled instances and bounded day/range projections. Compare expected occurrences independently of the selected library. Google recurrence and invitation representations may require iCalendar parsing even though external subscriptions are deferred.

Mock sync token expiry/pagination, edits, RSVP and permission changes. One path owns attendee notifications. Full calendar, footer and CLI receive the same permitted fields; styling is not access control.

**Pass:** correct fixture occurrences, shared identity, event mutation and notification ownership. Live Google/Workspace tests require later authorized access. No external feed fetching/subscription settings/cache work in this phase.

## P0-07/P0-09 — user actions and category labels

**Owners:** A/E/B/F/I with H fake output. Prove independent unread/stars, truthful account totals, new replies after a snapshot action, mixed thread labels and manual correction precedence. Use explicit read/star/unstar actions until the reviewed Done/Delayed button mappings exist.

Classification may tag user-defined human/automated/noise categories and scope in Gmail. It cannot choose completion/delay, mark mail reviewed or send it to Gmail system Spam/Trash. Test that filtering a category never silently completes its mail. Preserve account reading position and shared calendar context.

**Pass:** the same explicit actions and label outcomes through GUI/CLI, with no classifier-owned state transitions or manufactured zero counts. IMAP folder mappings are deferred, not implemented via a second category engine.

## P0-08 — fake assistance boundary

**Owners:** H/A/G/I. Start with absent/off/invalid/late/abstaining fake outputs. Manual Gmail labeling works first. Summaries retain source/coverage and exclude unpublished drafts; newer manual/provider state wins. Explicit scope and bounded cancellation apply to each job.

Later, when a supported same-machine Ollama/vLLM setup is available and authorized, verify local execution, structured results and actual server cancellation. Do not download models for bootstrap or block initial GUI/CLI use on model quality.

**Pass:** no inference when off, only validated configured category output, no Done/Delayed/read/star effect and stale-result rejection. Actual runner quality/locality/cancellation remains unverified until tested before enabling it.

## P0-11 — install the small pair early

**Owners:** G and artifact owners. Package the actual engine, useful partial CLI, Qt text/composer GUI and required runtime resources. No mandatory WebEngine/helpers in the first package. Test candidate Windows/macOS/Ubuntu/Arch/Omarchy routes on available hosts, with explicit missing evidence elsewhere.

Verify standard-user launch, fresh-terminal CLI discovery, shared profile, private IPC access, locked credentials, graphics/input, final-client shutdown and coordinated upgrades. No developer tools or model required for use. Recheck renderer dependencies separately if HTML viewing later adds them.

**Pass:** recorded install/runtime/dependency results with retained fixture data. Unsigned smoke packages can establish packaging behavior but not trusted delivery; signing/notarization/update access remains a release gate. A container build is not desktop/keyring evidence.

## First integration checkpoint

One synthetic message arrives, becomes searchable, opens with its attachment and receives an explicit user flag action through both clients. A human draft survives closing/restart. One calendar occurrence appears in full view, footer and CLI with the same permissions. Exercise concurrent clients and stale/mismatched requests. No model, SMTP/IMAP, external feed or HTML display engine is needed.

Record the consumed contract/fixture revision, build versions, exact commands, hardware/OS/session and observed outcomes. Promote only the passing slice. Final designs, Google access, larger scale tests and platform gaps remain explicit; a small successful slice does not prove the whole product. Legacy work stays deferred until later actual founder Google GUI/CLI dogfooding.
