# Phase 0 — foundation and feasibility

Updated 2026-09-11. **Prepared for a future implementation start; not executed.** This backlog is detailed enough to begin controlled feasibility work when requested. It does not mark feature contracts Ready or create a runnable build. [Development plan](../development-plan.md) owns the full sequence.

## Starting position

The repo has product/research/contracts, an append-only decision log, three earlier visual references and the closer mail/calendar pair. It has no code, dependency manifests, tests, CI or installers. A 2026-09-11 read-only lookup in this Windows PowerShell environment found Git on PATH, but not Rust/Cargo, CMake/Ninja/CTest, MSVC cl or Qt command tools. That is not a machine-wide installation audit. Reinspect the environment when execution begins; do not start by blindly reinstalling tools.

Proposed baseline: Rust engine/CLI, C++20/Qt Quick/QML GUI, SQLite/FTS5 with retained blobs, private on-demand engine IPC. Versions, library features and platform floors are selected from evidence during this phase. Final visual mockups and real provider credentials are not prerequisites for synthetic work.

## How to begin

The first authorized task is **P0-00 plus the small-fixture part of P0-01**. Read current status/decisions and the affected contract revisions, inspect the existing development environment and accessible test hosts, then create the smallest reproducible build/test foundation. Record exact successful commands only after they exist and run; no command in this plan claims to be runnable today.

Give each execution packet:

- its owner and contract revision, explicit purpose and prerequisites;
- the fixture/input revision and code paths it owns once created;
- a small deliverable and observable verification result;
- unresolved cases, consumer impact and a proceed/change/blocked outcome.

A missing credential, Mac, Linux desktop, signing identity or runner is an external dependency for that evidence row. Keep the row unverified and continue independent work. Do not install a model, contact a real mailbox or send mail as a side effect of a synthetic feasibility packet.

## Order and concurrency

1. P0-00 establishes available tools/hosts and the minimum build; P0-01 establishes a small shared synthetic fixture manifest.
2. P0-10's protocol skeleton and P0-03's storage slice start together after agreeing identity/operation concepts, then combine for one durable action.
3. P0-04 Qt reader/composer and P0-05 MIME/IMAP/SMTP plus P0-06 recurrence/feed experiments can run alongside the engine with the same source fixtures.
4. P0-02 prepares Google setup immediately; real authentication waits for the account/project/admin access needed for that test.
5. P0-07/P0-09 connect semantics and interface behavior; P0-11 packages the combined fixture slice on required systems early.
6. P0-08 starts with fake inference and configuration/cancellation questions. Model quality and the full workload/compatibility matrix continue before their feature gates.

These P0 identifiers retain the existing research experiment map; **P0-00** is the added bootstrap packet. Substeps in research narrow a packet, not a competing backlog.

## P0-00 — reproducible builds and host access

**Owners:** G with A/I/E. **Contracts:** Platform, Core, CLI, Mail UI. **Input:** current repository, PATH/development-tool inventory, target availability and [distribution research](../research/linux-and-distribution.md).

Later execution will create a minimal Cargo workspace for a Qt-free engine/client and a separate CMake/Ninja Qt application, with only enough packaging/test setup to run the experiments. Inspect current Rust, compiler, Qt Quick/WebEngine and build-tool compatibility first; start with maintained matched artifacts. Record native/TLS/SQLite dependencies and relevant license/release-access conditions before pinning. Do not introduce a Rust/Qt ABI bridge simply to launch separate executables.

Use Windows x64, macOS Apple Silicon and Ubuntu/Arch/Omarchy x64 as the candidate starting matrix, then reconcile actual founder hardware and Qt support. Build/test Rust and Qt separately; stage compatible artifacts together. Add CI on available supported runners once these commands work locally; CI containers can test headless code and package construction, but cannot prove desktop/keyring/Wayland or downloaded-app behavior.

**Output/pass:** recorded toolchain/dependency versions and commands, a headless Rust executable/test entry point, a Qt surface that loads its required modules, and an available/unavailable matrix for build, desktop and clean-install hosts. Every claimed host must have execution evidence; absent hosts remain explicit. **Pivot:** replace incompatible artifact/tool choices before spreading them into feature modules. See P0-11 for package behavior. An empty window alone does not pass reader/composer feasibility.

## P0-01 — identities, small fixtures and scale inventory

**Owners:** A/D/F, with every consumer. **Input:** Core, Triage, Calendar and [Quality](../contracts/quality.md), plus [storage research](../research/search-storage.md). No private mail is required.

Define the first fixture manifest before independent consumers encode different assumptions. Include several separate accounts with deliberately colliding remote-looking IDs; provider message and local conversation IDs; independent flags; mixed labels/folders; long threads; source MIME; complete/missing/partial blobs; durable drafts/operations; recurring events/exceptions and restricted calendars. Specify expected state transitions and retrieval results as data to create during execution.

Keep the first correctness corpus small. Separately design a seeded generator for the existing 100k rapid, 1m primary and 3m stress tiers, and meaningful 25 GB/100 GB content scenarios. Retain seeds/manifests and small licensed/synthetic examples in source control; generate large output outside it. The user's 25 GB disk snapshot is not a message count or a throughput result.

**Output/pass:** agreed identity relationships and fixture revision, expected outcomes, reproducible generation design and hardware/coverage fields. P0-10 later verifies that both clients interpret those identities and flags consistently; creating this initial manifest does not depend on those clients already existing. **Defer:** actual provider/message inventory and full scale runs until needed; do not make private data acquisition a bootstrap prerequisite.

## P0-10 — one engine, two clients, a bounded protocol

**Owners:** A/I/G/E. **Prerequisites:** P0-00, small P0-01 identities; P0-03 for the durable portion. **Evidence:** [CLI/engine research](../research/cli-and-local-engine.md).

Test a Rust engine and Rust CLI with a C++ Qt presentation adapter over a private local byte stream: named pipes on Windows, Unix sockets on Unix-like platforms are the first candidates. Evaluate explicit length-framed UTF-8 JSON with bounded frames; do not assume internal framing must equal public CLI JSON/NDJSON. Choose framing/schema details in the experiment rather than adding untested APIs to contracts now.

Start with version/capability discovery, one account-scoped paginated query, one snapshot-targeted flag change, operation status, cancellation and a bounded change subscription. Define stale revisions/cursors, partial outcomes and an explicit resnapshot after a stream gap. Keep binary attachments out of JSON row pages. Share protocol definitions and golden fixtures, with both Rust and C++ tested against them.

Prove profile exclusivity separately from endpoint naming and authentication. Check OS-specific access, concurrent launches, stale discovery, client crash, slow consumer, engine restart and incompatible versions. GUI and CLI needing the same work have separate interests: closing one cannot cancel the other's valid request. Engine/CLI startup must require neither display nor Qt renderer. End the engine after the last active client/job; durable queued intent alone is not a keep-alive.

**Output/pass:** cross-language fixture tests, one-writer/lifetime traces, action parity, bounded queues, stderr/stdout behavior and startup/memory measurements. The Qt model loads pages and preserves selection on GUI-thread updates. **Pivot:** fix transport/lifecycle behavior before committing interfaces; consider an in-process bridge only against a demonstrated process-boundary problem. A working loopback server is not evidence of the required private local boundary.

## P0-03 — persistence, blobs and useful search

**Owners:** A/D, consuming P0-01; integrate with I/E through P0-10. **Evidence:** [storage research](../research/search-storage.md).

Evaluate rusqlite with controlled SQLite/FTS5 features. Start with a dedicated writer, bounded reads, short transactions and explicit durability policy. No database transaction spans network/model awaits. Prove persisted intent and checkpoint ordering before tuning. Compare a simple FTS design against external-content indexing only where its consistency/size tradeoff matters.

Exercise replay, interrupted migration/transaction, draft/operation persistence and blob staging/finalization. SQLite and external files are not one atomic transaction: test interruptions before/after each commitment, including staged draft attachments. A process kill tests process-crash recovery, not power-loss durability. Design coherent preservation/restoration for migrations without quietly adding a full user-facing backup product.

Query sender names/addresses, subject and body with account filtering, conversation grouping and matching-message snippets. State partial sender and punctuation/language behavior. Deletions, edits and account removal must reach the index; summaries never become replacement source text.

**Output/pass:** correct small-fixture state/query results, recovery trace, actual SQLite features, draft/blob/operation survival and an initial 100k ingestion/query report. Record query plans and p50/p95/p99 with cache/host metadata. **Pivot:** inspect schema/query/hydration/locking first; evaluate Tantivy only if agreed retrieval or performance remains unmet. The million-message and full-content scenarios carry forward before performance readiness.

## P0-04 — received HTML and human composition in Qt

**Owners:** E/G with A/C/F fixtures. **Prerequisites:** Qt build and shared MIME/event inputs; P0-05 supplies the serializer/local sink for outgoing round trips, and P0-10 joins the integrated slice. Renderer/input exploration can start before those integrations. **Evidence:** [HTML](../research/html-mail.md), [desktop stack](../research/desktop-stack.md) and [UI brief](../ui-ux.md).

Prototype only the closer shell structure: top context bar, resizable list/reader, collapsed thread members, local-summary placeholder, compact today strip and a calendar surface. Inspect input, selection, scaling, accessibility hooks and scrolling with realistic row lengths. Fonts, exact spacing, Done behavior and production screen composition remain open.

Use the isolated WebEngine candidate with harmless synthetic mail. Validate tables, quoted content, Unicode, CID resources, very large HTML and long expanded threads. Its string-loading size limit means the experiment must evaluate controlled message-resource loading rather than concatenating an entire thread. Test blocked remote/local resource references and disabled scripts/privileged bridges without real mailbox content.

Exercise QML text/document composition: manual plain/HTML input, reply quoting, links/lists, signatures, clipboard/IME/undo and attachments. Round-trip through durable drafts and the single Rust MIME serialization path into a local capture sink. Keep the strip usable during body retrieval and editing.

**Output/pass:** fidelity/input screenshots and observations, preserved text/attachment checksums, no unexpected resource access, draft restart behavior and GUI/renderer memory/latency. Expanded members and matching search messages remain reachable. **Pivot:** fix content loading/editor integration first; change renderer/toolkit only if a material requirement remains unsupported. Production screens and interactions await their reviewed final design slice; the feasibility presentation remains provisional.

## P0-02 — sustainable Google setup and import strategy

**Owners:** B/F/G/I. **Input:** [Gmail research](../research/gmail.md), [Calendar research](../research/calendar.md), Core operation/checkpoint fixtures.

Prepare the method-to-scope map, installed-app browser/PKCE flow, OS credential-reference handling, consent/reconnect UX and actionable CLI errors with synthetic endpoints. Plan one coordinated refresh path for shared Google authorization. Consumer Gmail plus Workspace requires an audience strategy that actually covers both; a single-organization Internal configuration cannot be assumed sufficient.

Later authorized live checks need an accessible Cloud project, consumer account, Workspace account/admin policy and controlled test calendars/recipients. Record publishing/verification state, actual granted access, quota behavior, refresh after restart and sustained token policy. External Testing token lifetime and Workspace trust exceptions require explicit handling; one login is insufficient. Public verification remains deferred under C36 unless distribution changes.

Mock recent retrieval/history pages, expired cursors, rate limiting and interrupted attachments while preserving local intent. Record time-to-useful-data and full acquisition progress only once measured; never extrapolate an import promise from mailbox size alone.

**Output/pass:** viable setup/dependency record, correct synthetic checkpoint/reconciliation trace and later live evidence per account type. **Blocked rule:** absent admin/project/account access blocks that live row, not local engine/UI work. No policy bypass or silent recent-only retention workaround.

## P0-05 — mail library and submission behavior

**Owners:** C/B/A, with E/F consumers. **Input:** [IMAP/SMTP shortlist](../research/imap-smtp.md) and shared source/draft fixtures.

Evaluate incoming MIME, one outgoing MIME builder, IMAP session support and SMTP transport against required capabilities. First candidates are mail-parser, lettre builder/transport and async-imap/imap-proto; these are a test shortlist, not selected dependencies. Record maintenance, native/runtime features, licenses, cancellation and resource behavior.

Use scripted local protocol peers and a local SMTP capture sink for UIDVALIDITY changes, disconnects, optional-extension absence, destination UID mapping, partial moves, rejected submission and lost acknowledgement. Keep originals and compare decoded parts/content checksums. Share outgoing serialization with Gmail; no second composer format.

**Output/pass:** explicit baseline/optional capability matrix, preserved local identity/content/flags, bounded parser failures and distinct rejected/accepted/unknown send outcomes. Sent-copy failure cannot trigger resubmission. **Pivot:** change a library or add a contained adapter for a proven gap; do not default to inventing an entire IMAP or MIME implementation. Actual non-Google service coverage is a later live gate.

## P0-06 — calendar identity and time correctness

**Owners:** F/A, with B/C/E/I/G. **Input:** [calendar investigation](../research/calendar.md) and P0-01 calendar fixtures.

Evaluate parsing plus bounded recurrence/time-zone expansion, including all-day/floating/local times, daylight-saving gaps/overlaps, exceptions, moved/canceled instances, custom time zones and unusual rules. Compare the Rust shortlist against independent expected occurrences; investigate libical only for demonstrated gaps. Do not silently simplify an unsupported series.

Mock Google token expiry, pagination, edits/RSVP/permission changes, and read-only HTTP feeds with valid/invalid/stale snapshots. Define which path owns attendee notifications. Feed refresh failures preserve the last valid calendar. Test identical permitted event/time/coverage values in CLI, full calendar and today strip.

**Output/pass:** agreed series/occurrence identity and permission representation, expected expansion windows, recovery/edit traces and notification ownership. **Defer:** actual Google/Workspace shared-calendar behavior and controlled invitation delivery until authorized account access. **Pivot:** fix the affected adapter/recurrence component before encoding a lossy event schema.

## P0-07 and P0-09 — native state and product behavior

**Owners:** A/E/B/C/F/I, with H fake output. **Input:** [Triage](../contracts/triage.md), [UI handoff](../ui-ux.md) and shared identities.

Prove unread and starred independence, message-level provider truth with conversation views, account totals independent of visible pages/filters, mixed thread membership, late incoming replies and pending/incomplete states. Apply categories as Gmail labels or an explicit IMAP primary-folder move, preserving unrelated labels/flags and local content.

Use explicit mark-read/star/unstar commands for early tests. Do not infer that mockup Done means archive/read/unstar or that Delay schedules a snooze. Record the unresolved interaction for the relevant UI handoff. Test restoration of selection/reading position and one today/full-calendar shared view without final styling.

**Output/pass:** expected transition table exercised through both clients, no false zero after filing/filtering, no stale overwrite of manual corrections, and clear ownership of outstanding UI decisions. These fixtures become regression cases for the first real product slices. **Pivot:** clarify the product action before implementing ambiguous combined behavior; preserve accepted native-state semantics.

## P0-08 — assistance boundary before model quality

**Owners:** H/A/G/I, with B/C/E consumers. **Input:** [local-model research](../research/local-ai.md) and synthetic received-mail/category fixtures.

First use a fake runner: off means no polling/inference; invalid/late/abstaining results are data, never commands; newer manual/provider changes win. Summaries preserve source identity/coverage and exclude unpublished drafts. Reuse manual filing and the operation journal for permitted classification.

When a supported local setup is available, evaluate one small bounded job through Ollama and a supported same-machine vLLM environment. Record local-only configuration/weights, schema compatibility, cold/warm resources and whether cancellation actually stops server work. Closing a connection alone does not prove cancellation; do not terminate a runner used by other apps.

**Output/pass:** disabled/manual behavior, stale-result tests, supported-runner matrix and actual cancellation evidence before enabling that runner. Full per-category quality thresholds and summary evaluation continue before Auto-apply/daily use. **Pivot:** keep that runner integration unverified if locality or cancellation cannot meet the contract; bring a material requirement conflict back with evidence. No cloud fallback, new authoring feature or silent scope reduction.

## P0-11 — install the fixture slice early

**Owners:** G with all artifact owners. **Prerequisites:** P0-00 and a joined P0-03/P0-04/P0-10 slice. **Evidence:** [distribution research](../research/linux-and-distribution.md).

Produce explicit development smoke packages containing engine, CLI, Qt GUI, reader/composer and runtime resources. Try the proposed Windows installer, macOS app/CLI route, Ubuntu .deb and Arch/Omarchy pacman package. Test standard-user launch, fresh-terminal command discovery, shared profile, locked/unavailable credentials, rendering sandbox, scaling/input and shutdown on real desktop environments.

Keep user data separate from installed binaries; try replacement/upgrade with a fixture profile and incompatible engine/client versions. Inspect actual Linux artifact dependencies rather than trusting a generic glibc note. An Arch container build does not establish Omarchy desktop support.

**Output/pass:** per-target install/runtime checklist, package/dependency sizes and preserved fixture data, with missing hosts plainly unverified. Early unsigned/local packages may establish staging behavior, but are not trusted end-user delivery. Signing/notarization, update distribution, final OAuth and full upgrade/uninstall matrices remain release gates. **Pivot:** repair package/launcher/runtime choices while the app is still small; do not disable the renderer sandbox to make it launch.

## The first integration checkpoint

Demonstrate one synthetic message arriving, becoming searchable, opening with its attachment, and receiving a durable explicit flag action through GUI and CLI. Restart it. Add one calendar occurrence visible in both full view and today's strip, and one human draft that survives closing. Use the actual engine/process boundary, with no model installed.

Record the baseline and promote only the consumed slice of a contract after its interface/fixture/acceptance choices are settled. Whole feature contracts can remain Draft with an explicitly Ready slice; name that scope when assigning work. The [contract workflow](../contracts/README.md) defines readiness.

Before expanding Phase 1, publish evidence for available platforms and list the remaining provider, model, scale, final-design and release dependencies with their latest responsible gates. No single spike proves the complete product.

## Evidence to retain

For each experiment keep source/dependency versions, build revision, fixture seed/manifest, hardware/OS/display session, exact executed commands, expected/observed results and pass/fail/blocked status. Store small non-sensitive reports with the owning research topic and link them from status; do not pre-create empty report files or commit generated multi-gigabyte corpora.

Changes to behavior or the chosen baseline get an append-only decision and updated consumer contracts. A benchmark report must include the full measurement boundary from Quality; a screenshot, successful compile or localhost response is not a reliability or readiness result.

