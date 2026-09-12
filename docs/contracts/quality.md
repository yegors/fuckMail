# Quality, performance and release acceptance

Updated 2026-09-11. Proposed budgets, not measured results. Google-first required behavior includes fast sender/subject/body search, responsive user-owned daily triage, conversation-only mail, full offline messages/attachments, reliable Google calendar views and optional local classification/summaries on Windows/macOS/Linux. The founder specified high-end machines. Exact devices, mailbox message counts and local models remain unmeasured.

State: Draft · Revision: 5 · Implementation/evidence: Phase 0 synthetic results recorded; Phase 1 measurements and native acceptance active; whole area remains Draft. Owner: G — Platform/release coordinates; each workstream supplies its own fixtures and evidence. This is a shared acceptance contract, not a separate application feature.

Revision 5 keeps the existing budgets and adds the app-consumed grouped first-page/filter queries, source-ingestion contention, native formatted document and combined calendar checks to the active [Phase 1 execution record](../research/phase-1-execution.md). The initial 100k/100-observation grouped and filtered p95 values (253 ms and 648 ms) exceed the proposed 100 ms Core budget; concurrent query/queue-response p95 was 601/602 ms. These are failed measurements under correction, not readiness. Native compilation and a passing startup/layout test do not prove the complete phase or the input-to-painted-result budget. Affected consumers: Core, Search, Mail UI, Calendar, CLI and Platform.

Revision 3 applies D009: separate first usable, Google dogfood and later-protocol evidence. Best-effort HTML comes later; full browser fidelity and legacy protocols do not gate first use. Affected consumers: all workstreams. Current Google-first readiness denominator is nine.

Consumes versioned builds, synthetic fixtures, hardware/runner metadata and workstream results. Produces reproducible measurements and pass/fail or explicitly renegotiated gates. Start independently with fixture definitions; measured experiment results now live in [Phase 0 evidence](../research/phase-0-execution.md) and the search report. Proposed product budgets are distinct from those measurements.

Home Windows evidence, 2026-09-11: the planning checkpoint `5bed257` (runtime unchanged from `204fa62`, fixture 1/schema 4/protocol 1) passed 8/8 dev CTests in 69.13 seconds and 8/8 release CTests in 95.63 seconds, including test compilation but excluding the initial application build. A scoped developer-tool correction then enabled packaging with project-local Rust. Relocated GUI/CLI, final-owner shutdown and native D3D11/RTX 5090 rendering passed. [The handoff record](../research/phase-0-execution.md#home-windows-handoff-before-phase-1) preserves the failed packaging attempt, correction, package hash and remaining manual checks. These are development-host checks, not performance-budget or whole-feature release passes; readiness remains 0/9.

Revision 2 connects [Phase 0](../plans/phase-0.md) to reusable fixtures and evidence gates. Affected consumers: every workstream, especially Core/CLI/Platform for the first shared slice. No fixtures, harness or measured results exist yet.

## Fixture and verification foundation

Current gates are scoped to the Google-first product (nine areas); IMAP/SMTP and external iCalendar subscriptions do not count until the later protocol phase. First usability requires a real GUI plus useful partial CLI, readable text including HTML-only inputs, human drafts and basic mail/calendar behavior. Expand CLI/GUI parity with each Google workflow. The founder must dogfood usable Google clients before legacy implementation begins; document actual scenarios and failures, not merely an installed build. Best-effort HTML is later work and does not block initial use or select the toolkit.

Create these artifacts during P0-01 and their consuming experiments; this table describes inputs and expected evidence, not tests that already exist.

| Fixture family | Owner and scenarios | Verification purpose |
| --- | --- | --- |
| Identity and triage | A: separate accounts, colliding provider IDs, mixed unread/star states, later thread replies, archived/folder mail, partial coverage | Correct account scope, snapshot actions and totals independent of loaded rows/filters |
| Source mail and drafts | C/E: varied MIME/encodings/quoting, inline resources, malformed inputs, large parts and authored draft/signature/attachment cases | Preserve original bytes, extracted fields and attachments; round-trip human content and control reader resources |
| Sync and durable operations | A/B/C/F: paginated deltas, expired cursors, partial moves, stale revisions, crash points, acceptance-unknown send and Sent-copy failure | No skipped checkpoint, lost authored work or automatic duplicate send; provider outcomes remain distinct |
| Protocol and lifecycle | A/I/G: version mismatch, concurrent clients, locked credentials, disconnected/slow consumers and final-client exit | Same semantics from Rust CLI and Qt adapter; bounded memory, one writer and explicit recovery |
| Calendar | F: independently expected occurrences, time-zone changes, exceptions, permissions, overlaps/all-day, RSVP and stale feeds | One correct event view for full calendar, strip and CLI; no title/field access beyond actual permission |
| Assistance | H/A: off/unavailable, invalid/abstaining/late output, changed mappings/manual overrides, new thread members | Manual use unaffected; only permitted filing; faithful source/coverage handling and cancellation |
| Scale and packaging | D/G: seeded varied corpus plus clean installation/profile fixtures | Repeatable retrieval/ingestion measurements and actual GUI/CLI/renderer behavior on required targets |

Version small fixture manifests and expected outcomes, record provenance/licenses and keep private content and generated bulk data out of source control. Shared boundary tests use the same fixture revision in Rust and C++; calendar expected results must not be generated solely by the recurrence library under test. A provider mock demonstrates our response to its input, not the real provider's behavior.

When code exists, layer fast domain/fixture tests, shared-protocol and crash/recovery integration tests, local protocol sinks, GUI interaction/rendering checks and clean-install/live-provider checks. Run the relevant layer for each slice and publish exact successful commands with the implementation. Start CI with headless/build checks on available supported runners; actual OS/desktop/keyring tests remain separate evidence. No speculative test scripts or CI configuration are added by this plan.

A result records build/dependency versions, fixture revision/seed, OS/architecture/hardware/display session, executed command, expected/observed outcome and pass/fail/blocked status. Only executed checks can pass. Blocked platform or account rows remain visible while independent work proceeds. Use the existing budgets below, not a new per-plan definition of readiness.

## Representative workload

Use synthetic mail and non-sensitive fixtures for repeatable development. Add founder-owned data only through a deliberately scoped local evaluation. Do not put private messages, attachments, addresses or credentials into the repository or benchmark reports.

The supplied `du -h` snapshot reports about 25 GB for the existing eM Client local store, with major directory totals around 2.9 GB, 7.6 GB and 15 GB. This is user-supplied disk-usage evidence, not a verified account inventory, full server size, or message count. No eM Client database was inspected.

Proposed scale tiers: 100,000 messages for rapid iteration, 1 million across several accounts as the primary large-corpus test, and 3 million as a stress tier. Include a roughly 25 GB offline-content case anchored to the current store and a 100 GB growth case. These are synthetic capacity scenarios, not claims about the founder's message count. Include both many short messages and attachment-heavy mailboxes.

Record message count, unique text bytes, binary bytes, average and tail body size, number of accounts/folders/labels, thread lengths, languages, calendar series, local retention coverage and index size. Repeated identical text alone is not a representative search corpus.

## Candidate budgets

Proposed reference: a high-end computer with 32 GB RAM and an NVMe SSD; record exact hardware before benchmarking. Report Windows, macOS, Ubuntu, Arch and Omarchy results separately; all required platform/configuration gates block MVP. Pin versions, architectures and display sessions rather than claiming every Linux configuration.

| Interaction | Proposed target | Measurement boundary |
| --- | --- | --- |
| Warm local search | p95 <= 100 ms | Core query to first 50 conversation results, including grouping, matching-message snippets and filters |
| Search as seen by user | p95 <= 200 ms | Final input event to painted first results, including debounce and any cross-thread/process communication |
| Cold local search | p95 <= 500 ms | First query after a documented cold-start/cache protocol |
| Cached mailbox switch/conversation open | p95 <= 100 ms | User action to usable visible content for normal-sized conversations |
| Cached calendar view switch | p95 <= 100 ms | User action to visible events after bounded recurrence expansion |
| Triage action feedback | p95 <= 100 ms | Read/star/category click to locally durable acceptance and visible pending state; provider confirmation is measured separately |
| App launch into cached inbox | p95 <= 2 s | Process launch to usable cached list; excludes first import and auth |
| Scrolling | 60 Hz target with no sustained visible stalls | Frame timing while backfill/indexing runs; inspect tail frames |
| Idle resource use, local assistance off | <= 300 MB combined resident memory, < 1% average CPU | All app/rendering processes over a quiet 5-minute interval while open; no model inference or polling |

Long conversations and large/complex messages get bounded asynchronous rendering and progress rather than blocking the whole UI. Fetch and render conversation members incrementally without exposing a separate message mode. The memory budget is a starting hypothesis and must include any HTML-renderer/runtime subprocesses, even with a native shell. Measure native list/calendar interaction separately from HTML reader cost. After GUI close with no independent CLI session, verify that no GUI/engine/renderer process, sync request or reminder remains active. Also test an explicitly invoked CLI command/watch without the GUI and engine exit when that final client ends. Record the peak during import separately; set its release budget after phase 0 measurements.

Measure local assistance separately in disabled, cold-model and warm-model states. Report app plus runner RAM, GPU/VRAM or unified memory, CPU/GPU utilization, model disk size, input/output size, latency, throughput and incremental load relative to the runner's baseline. Do not claim the 300 MB off-mode budget includes model weights. Keep interactive search/scrolling/triage targets during active inference; throttle or pause background jobs to protect them. Set model-specific inference budgets after measurement, and verify cancellation stops the owning GUI/CLI session's runner job without terminating a shared service.

Do not impose a fictional whole-mailbox import time independent of provider quotas and bandwidth. Measure time to useful recent conversations, messages/text and attachment bytes per minute, quota consumption, disk amplification, projected completion and resume behavior. Include disk space for full attachment retention and temporary download/maintenance overhead. Define the recent-mail target after a real authorization and provider experiment.

## Evaluation method

Run representative queries with and without simultaneous synchronization, insertion, deletion, index maintenance and optional local inference. Include common subject/body terms with huge match sets, rare terms, sender names/addresses, partial sender input, current-account/category filters, long conversations and selected languages. Measure grouping and matching-message hydration, not only raw full-text query time. Report p50/p95/p99, repetitions, fixture version, build mode, hardware, cache state, and storage growth. A process restart alone does not prove the OS page cache was cold.

Use at least 100 observations per interaction class where practical. Save reproducible commands and reports when the harness exists. If a target fails, investigate query plans, result hydration, locks, indexing batches and UI work before replacing a library.

## Search usefulness

Collect 10–20 retrieval tasks covering the founder's confirmed needs: sender display name, sender email address, subject keyword and body keyword. Include a few exact identifiers and multi-message conversations. Attachment-content search is not part of the required MVP benchmark.

For known-item tasks, propose the desired conversation appearing in the first 10 relevance results in at least 90% of cases, with the actual matching message revealed on opening. Exact identifiers and explicitly matching filters should never be missed once their fields are indexed. Evaluate quoted-reply duplication and required languages separately; typo handling is a future scope decision. Show incomplete coverage whenever the underlying data has not been acquired or indexed.

## Reliability under load

Acceptance scenarios include interruption during initial import, expired provider cursors, remote deletions during backfill, concurrent edits from another device, low disk space, index rebuild during reading, lost submission acknowledgements, revoked calendar permissions, and daylight-saving transitions. Verify that unsent drafts survive restart and that stale index entries never expose removed-account data.

Add classification during manual/provider edits, renamed category destinations, interrupted IMAP folder moves in the later protocol phase, stale summaries after new replies, unavailable runners, cancellation and account removal during inference. Preserve reading position, manual corrections and unread/star counts. A model result must never silently revert a newer user choice or complete a held action.

## Triage and local-model usefulness

During the proposed daily-driver trial, evaluate whether the founder can review new mail, complete action holds and reach zero unread/zero starred within each account's agreed scope. Record remaining holds and why they remain without automatically clearing them. Verify that archived/category-folder unread mail counts, filtered-out work is still discoverable, and partial synchronization or pending writes cannot produce an unqualified all-clear. The daily zero goal is a workflow outcome, not a benchmark guarantee that incoming mail stops or every action finishes that day.

Build a labeled synthetic/consented-local fixture set with the configured categories, ambiguous mail, human replies from automated-looking addresses, newsletters, receipts, multiple languages and new messages within old conversations. Measure per-category precision/recall, abstention, invalid-output rate, manual-correction rate and incorrect filing separately. Model-reported confidence alone is insufficient to enable auto-filing. Choose and record acceptance thresholds before enabling automatic classification in the trial; validate Ollama and vLLM adapters against their actual models and supported platforms.

Evaluate summaries for factual fidelity, attribution to the included messages, preservation of key requests/dates, appropriate length, and explicit partial/stale coverage. Test text that contains instructions without allowing it to control application behavior. Original-text search must work identically with summaries disabled, missing or incorrect. Evaluate calendar search on organizer/attendee/location/title examples and known occurrence coverage separately from mail search.


## Required Google-first release evidence

Mail: separate account identity, correct conversation grouping, fast scoped search, human-only plain-text/HTML composition and signatures, full offline messages/attachments, reliable Gmail API retrieval/submission, durable drafts and uncertain-send recovery. Add accurate Unread/Starred filters and account totals, explicit action holds, manual/provider category persistence and view restoration. Filing, generated summaries and filter changes must not create a false daily all-clear.

Calendar: all four views, search/coverage, compact all-day rows, concurrent/current-next events, create/edit, recurrence exceptions/time zones, invitations/RSVP, shared permissions and running-app reminders. External read-only subscriptions have a later protocol gate. Verify Join opens the meeting link and acknowledges only that occurrence's reminder. Test conflicting edits and changes made in Google Calendar separately from ordinary CRUD.

Local assistance: disabled/unconfigured/manual operation, Suggest and configured Auto-apply, independent summaries, both runner adapters on supported local setups, correction precedence, unknown/invalid/stale output, source-linked partial/stale summaries, explicit historical batches, unavailable-runner fallback, no cloud execution, no composer access and verified cancellation on close. Set category quality acceptance thresholds from labeled fixtures before enabling automatic filing in the founder trial.

Platforms: prebuilt GUI/engine/CLI installation on Windows, macOS, Ubuntu, Arch and Omarchy, responsive interaction during import, recovery from interrupted work/low disk/expired cursors, and no GUI-owned work/reminders after close. An independent explicit CLI session follows its documented lifetime. Numeric targets are documented in [performance](quality.md); core scope is in [product scope](../product-scope.md).

Smooth installation, trusted signing where applicable, a maintained update route and dependency/license review are now MVP work. Broad public-launch/store work remains deferred; OAuth verification and any model redistribution requirements must be resolved for the actual distribution scope before release. Tasks, contacts, widget dashboards and broad assistant integrations remain outside MVP. Optional local classification/summaries do not authorize AI email writing.

## CLI and installation gates

For the first usable slice, test its useful partial CLI alongside the GUI; grow to complete applicable Google workflow coverage before the pre-legacy dogfood gate. Test JSON/schema compatibility, pagination/streams, exit codes, explicit noninteractive authentication errors, file/stdin payloads, operation IDs and partial outcomes. Compare GUI and CLI effects against the same core fixtures. Verify concurrent clients, cancellation, protocol mismatches and uncertain-send recovery without opening a GUI or requiring a display server.

Measure cold CLI command startup, warm-engine query latency, stream backpressure and engine memory separately from rendering; set numeric CLI budgets after the first prototype. Keep existing interaction budgets during concurrent CLI work and indexing.

Run installation, first launch, terminal command discovery, update, recovery and uninstall on clean target systems without developer tools or a model. Test actual Linux desktop/keyring/portal/sandbox behavior and a CLI-only environment; a container-only build test is insufficient. Record package sizes and first-run steps rather than promising tiny or dependency-free downloads.

## Open before claiming readiness

Agree the actual reference devices, account/provider inventory, corpus and language mix, inference model/platform matrix, quality thresholds and peak-import budget. Numeric budgets above remain proposals. An unrun test is not a pass; every feature in [status](../status.md) currently has zero verified release gates. Record failed targets and any accepted revisions in [decisions](../decisions.md), and link reproducible evidence here when it exists.


Revision 4 records D012: retain native command/test results, synthetic corpus definitions, search percentile/cache context and explicit unverified platform/live-provider gates. All workstreams consume this evidence rule. No small experiment promotes a complete feature to Verified.
