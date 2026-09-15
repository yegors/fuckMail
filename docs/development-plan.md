# Development plan

**D059–D064 / P2-UI3 active:** the configured Windows candidate at `dist/ui3-windows-amd64/bin/fuckmail-gui.exe` now includes six-second Send Undo and the shared action toast, alongside the composer From selector, preserved transfers, recipient/file input, reader/list, Settings and Calendar changes. Scoped native/CLI, Windows cancellation, strict lint and relocated package135 checks pass. Schema27/fixtures0,3/protocol1 and readiness0/9 are unchanged. Draft-discard and Calendar-delete Undo, pending Sent presentation, account removal disposition and remaining interaction/visual gaps stay open. [Coverage and evidence](research/prototype-september-15.md). Preserve package115 and package94/schema26 in their ignored relocation directories.

**D056 / P2-LLM2B:** the Local model editor now contains the complete Ollama JSON request, including system instructions, response schema, options and all variables. Inference sends its expansion without hidden additions. Schema 23 preserves saved instructions and account choices. Scoped Rust/native tests, strict lint, 15 engine/CLI checks including exact HTTP request comparison, actual settings upgrade/refusal and relocated Windows launch pass. The configured app is `dist/llm2b-windows-amd64/bin/fuckmail-gui.exe`; close the older app first. The installed model has a recorded label-quality miss. Readiness remains 0/9.

D055 / P2-LLM2 is the current local-assistance packet: one model/prompt, default-on prospective summaries and automatic single-label classification, per-message cards and summary list previews. Remove the sample/default-reset buttons and top thread summary. Complete the scoped Windows, migration, delivery and installed-model checks before handing back the candidate; broad performance remains deferred. The D055 decision and Local assistance 8 supersede older delivery defaults below.

**D054:** finish the founder-reported credential denial loop and header Sync/time controls, with scoped native/engine checks and a configured Mac feedback package. Schema 21 is unchanged; D053 summary presentation proceeds independently. [Current execution](research/phase-2-execution.md#d054--keychain-denial-and-header-sync-2026-09-15-utc).

**D052 / P2-LLM:** Ollama discovery/settings, bounded summaries, per-message classification/corrections and native/CLI controls are implemented under Local assistance 6/Core 29/schema 21. [Scoped Mac evidence](research/local-assistance-execution.md#d052--p2-llm-mac-implementation-2026-09-14) records passing checks and model-quality misses. vLLM, broad historical inference and automatic label application remain deferred; remaining acceptance and readiness 0/9 stay explicit.

D046 moves raw evidence to [ignored local/CI artifact storage](research/evidence-archive.md) before commit/merge preparation. Quality 27 adds repository hygiene checks; product sequence and acceptance gates are unchanged.

**D043–D045 — real-mailbox UI repairs:** the founder has signed into the configured Mac app. Real UI testing found and fixed Calendar creation after first sync, recurring editor invalidation, accessible event activation and search-popup focus/dismissal. Mail and Calendar walkthrough results are recorded below. Performance and the broad workspace sweep remain deferred under D041; this is not full Phase 2 acceptance.

**D040 / P2-01C on macOS:** email-first Google account addition, app-owned registration, shared CLI/reconnect/Restore and packaging are implemented. Native **22/22**, relocated Mac package and generated-provider account walkthroughs pass. Current boundary: **Core 27/Gmail 13/Calendar 21/Platform 15/Mail UI 28/CLI 21/Quality 26/Search 5/schema 20**; fixtures 0/3 and protocol 1 unchanged. The ordinary workspace rerun was stopped under D041 after the earlier timeout; the two performance attempts lost window focus. D042 supplies the app registration; the founder completed Gmail sign-in and reported sending/receiving. D043–D045 repairs and exercises the real UI.

**Current packet delivered:** configured UI Package03 passes relocated verification and final real-account readback; the connected app is running for founder testing. The walkthrough uses only newly labeled test items and the two explicitly authorized recipient addresses, held outside Git. Calendar creation/editing/invitation/recurrence/cancellation and mail submission/attachments/Reply all/forward/search/triage have observed provider or UI outcomes; detailed results and remaining gaps belong to the execution record. Sustained authorization, Workspace policy, full agreed offline coverage, backup/restore and other-host acceptance remain separate. Performance and the broader workspace sweep stay deferred; readiness stays **0/9**.

D039's Windows Package02 and the `618e8a0` rendering correction remain historical checkpoints. Preserve the 997-artifact handoff archive and all failed attempts. Current results and exact source/binary scope belong to [status](status.md) and the [execution record](research/phase-2-execution.md).

## Historical checkpoints

Completed source-specific results are maintained in [Phase 0 execution](research/phase-0-execution.md), [Phase 1 execution](research/phase-1-execution.md) and [Phase 2 execution](research/phase-2-execution.md), including their immutable manifests, failed runs and retained diagnostics. The earlier packet descriptions below preserve sequence and scope; they are not a list of currently missing D039 implementation. D031's failed performance result remains preserved after D038 resumed measurement.

## Delivery direction

Build for the founder's actual Google accounts first. Rust engine/CLI, C++20/Qt Quick/QML UI, private local IPC and SQLite/FTS5 remain the proposed baseline. QML syntax is accepted; Node.js and a separate JavaScript compilation/bundling pipeline are excluded. Experimental versions are pinned and have native evidence; production qualification remains separate.

The first usable milestone includes a real GUI and a useful partial CLI. Implement the same small workflows in both, then expand their coverage together. IPC access, permission checks, account isolation, concurrency and recovery need both clients early; the GUI must not wait for exhaustive CLI commands or polished automation tooling.

Prioritize human correspondence and replies. Decode MIME, preserve original HTML/attachments and safely extract text from HTML-only mail from the start. Received HTML display can arrive later with best-effort readability. Browser-level fidelity, WebEngine packaging and legacy protocol compatibility are not early gates.

The user always chooses Done/Delayed. Classification supplies user-defined human/automation/noise and category/scope tags; it never changes review/action state. The September 14 Flight Progress v2 reference guides the current native client; earlier September 12 references retain their Phase 1 scope. D014 confirms read-on-open, Done clearing unread/stars, untimed Delay, combined full permitted calendar details and a browsable timeline only in Mail. Remaining production interactions use the reviewed design for their slice.

IMAP, SMTP and external iCalendar URL support start only after the founder has dogfooded working Google GUI and CLI workflows. Retain their research and future contracts without implementing transports/feeds or making them early dependencies. MIME and Google recurrence/invitation representations remain needed.

## Phases and evidence

P1-10's [Windows acceptance and final CI](research/phase-1-execution.md#p1-10-final-native-ci-and-phase-1-closure) close Phase 1 on available hosts. The [Phase 2 plan](plans/phase-2.md) now details real Google setup, shared provider scheduling, parallel mail/Calendar acquisition, durable drafts/send and calendar invitations/reminders, followed by staged founder testing. Actual credentials, disk/vault qualification and policy choices retain their pre-import gates.

| Phase | Deliverable | Exit evidence |
| --- | --- | --- |
| 0 — Prove the paired foundation | Native builds, shared small fixtures, one engine/store, small CLI and text/composer GUI, Google prerequisites | Shared query/action through both clients; bounded state, private access and durable recovery; available-host evidence |
| 1 — Redesigned local client | Latest Flight Progress v2 native shell/mail/calendar/settings and themes, paired CLI, shared search/actions, durable new/reply/reply-all/forward drafts/capture and local calendar edits | [Phase 1 walkthrough](plans/phase-1.md) passes; native visual/input, performance and package evidence recorded; live-provider/full-product limits explicit |
| 2 — Google daily workflows | Gmail/Workspace mail and Google Calendar, setup, sending, full-history acquisition and an incrementally growing CLI | Real Google flows work; controlled founder use begins; no IMAP/SMTP/feed prerequisites |
| 3 — Google completion and dogfooding | Usable Google GUI and CLI; complete Google calendar/actions, expanded designs, intended local assistance and later best-effort HTML; delivery/quality evidence | Actual founder workflows, defects and retests recorded; Google-first release gates separately verified on required platforms |
| 4 — Later protocol support | IMAP, SMTP and external iCalendar subscriptions | Start only after founder Google GUI/CLI dogfooding; then prove their own interoperability and recovery |
| Later | Editable CalDAV; possible broad public/open-source launch | Separate decisions |

Phase detail decreases with distance. Completed [Phase 0](plans/phase-0.md)/[Phase 1](plans/phase-1.md) and active [Phase 2](plans/phase-2.md) have detailed packets. Refine the next slice from evidence rather than writing every future API in advance. Missing provider access, final design or test hosts blocks only the affected evidence.

The [first self-contained package](plans/phase-0.md#proposed-first-deliverable-a-runnable-desktop-slice-on-mac-and-windows) delivered a small runnable synthetic-mail slice from Phases 0/1, with repeatable development on the founder's Mac and Windows PC. Its bounded walkthrough and native build evidence are part of the now-completed Phase 0 experiments.

## Historical Phase 0: a GUI before a CLI monolith

The packets below have been executed for the synthetic foundation; [execution evidence](research/phase-0-execution.md) records all available-host results and the separate physical/live-provider gates. Do not repeat this foundation as new work.

Start P0-00 and the small-fixture part of P0-01. Join Core/CLI/storage (P0-03/P0-10) with Qt GUI/input/composition (P0-04) promptly. The first boundary needs capability discovery, a bounded account-scoped query, an explicit journaled action, a durable draft and a small event view. It does not need every final command.

Use shared fixtures for MIME/text/attachment processing (P0-05), Google OAuth/sync setup (P0-02), Google recurrence/invitations (P0-06), and user-owned state/category behavior (P0-07/P0-09). Fake assistance can validate forbidden state changes without a model (P0-08). Package the small GUI/CLI slice early (P0-11), without a mandatory HTML engine.

Entry to Phase 1 is per ready slice: proven native build on available hosts, the private client/engine boundary, persistence, basic FTS and shared consumed identities/fixtures. The GUI needs readable text and a credible human editor, not a full received-HTML renderer. Missing Linux/Mac evidence stays unverified; independent work can continue.

Do not implement legacy transports/feeds, an exhaustive CLI grammar, a model installation manager or final visual polish in this phase. Record versions and measured outcomes only after work runs.

## Historical Phase 1: the redesigned local milestone

The [detailed Phase 1 plan](plans/phase-1.md) is completed on available hosts, including the updated design, local workflows and navigation/rendering spike. Preserve the existing engine/GUI/CLI and source-specific [execution evidence](research/phase-1-execution.md); whole-product/physical/live-provider qualification remains separate.

P1-00 freezes the next shared types/fixtures. P1-01 adds typed state, grouped queries/counts/actions and combined calendar scope while P1-02 establishes native visual primitives/settings. P1-03 mail/search and P1-04 calendar/timeline then proceed in parallel. P1-05 finishes durable human replies/local capture while P1-06 exposes bounded nonrecurring create/edit/self-RSVP. P1-07 integrates native visual/input, cross-client, recovery/upgrade and package checks.

The deliverable is a usable synthetic local client with the updated design, truthful state and relevant CLI parity. Full provider setup, destructive account removal, models, received HTML rendering, timed snooze/follow-up and legacy protocols remain later. Core owns shared schema/protocol changes; UI owners use separate components without alternate stores. Calendar creation, capture-to-Sent projection and saved signature semantics are explicit implementation work in the plan.

No Google credentials or further product decision is needed to begin the first packets. Small missing composer/calendar forms are designed and reviewed within their slice. Real-data/OAuth policy gates still apply before Phase 2.

## Phase 2: Google mail and calendar together

The [Phase 2 plan](plans/phase-2.md) starts from merged `885de67` and now reaches D038/D039's Windows feedback package. Ordinary startup is an empty live profile, while explicit demo/test profiles retain their synthetic data. Account removal retains a read-only local archive and records credential cleanup; restoration requires the same verified identity. GUI and CLI share these workflows.

Generated-provider and local tests cover recent-first Gmail acquisition, history/label reconciliation, composition, attachments, Drafts, explicit Send and uncertain-outcome recovery. Calendar includes catalog/permission handling, combined views and timeline, supported recurrence scopes, invitations/self-RSVP, reminders and cached search. Their source-specific limits remain in the contracts and execution record; implementation is not proof of real Google compatibility or complete offline inventory.

The remaining execution is final packaged correctness/performance, protected-profile and restorable-backup admission, then bounded real personal Gmail and Workspace comparisons. Verify actual grants, quotas, admin policy, native vault failure behavior and sustained refresh. Start user-owned canary writes on disposable recipients/calendars, with explicit authorization for any automated external effects. Daily-driver closeout requires complete agreed source coverage and defect retests. App-level store encryption remains deferred; the founder's disk-encryption report is distinct from native administrator verification.

## Phase 3: complete and dogfood the Google product

Refine this phase from actual use. Finish the agreed Google GUI/CLI workflows, calendar edge cases, navigation/settings, human composition and failure/recovery screens. Full Google workflow access is the intended CLI outcome; command completeness evolves with the product, not ahead of it.

D051 advances the first Ollama classification/summary implementation into P2-LLM. Phase 3 retains broader quality/platform qualification and any later adapter expansion. Classify human correspondence, bots/lists and user-defined noise/category/scope through the proven manual labeling path; preserve manual precedence and explicit user Done/Delayed. Assistance stays off by default and does not block manual use.

Add best-effort received HTML display in this later phase. Evaluate a limited document renderer against real reading needs before adding a browser engine. Preserve original content, useful links/images and text fallback; visual fidelity is not a browser conformance exercise. Isolation and resource policy remain required. HTML composition and received-content rendering are separate components.

Use the [quality contract](contracts/quality.md) for million-message/search, import responsiveness, durable drafts, resource/lifecycle and all-platform delivery evidence. Measure all shipped processes; do not count a deferred renderer's hypothetical overhead as a current dependency.

Record founder dogfooding with both Google clients: actual tasks, failures, fixes and retests. A two-week trial remains a suggestion, not a fixed user requirement. Google-only software is now the intended first product. A launch screenshot, CLI smoke command or mock account is insufficient evidence of daily use.

## Phase 4: protocols after experience

Begin only after working Google GUI and CLI workflows have been dogfooded by the founder. Revisit the retained [IMAP/SMTP contract](contracts/imap-smtp.md) and deferred subscription sections of [Calendar](contracts/calendar.md), based on concrete non-Google needs and the proven engine boundary.

Implement capability-driven IMAP sync/folder moves, SMTP submission/Sent recovery and external read-only iCalendar subscriptions. Add their CLI/GUI affordances together. Preserve Gmail behavior; do not retrofit the early product into a lowest-common-denominator protocol model. Expand readiness reporting explicitly when this work becomes active.

## Ownership and parallel work

The [contract index](contracts/README.md) defines owners. A owns domain/schema/protocol; B owns Gmail; F owns Google Calendar; E/G own Qt presentation/platform; I owns CLI commands; D owns search; H owns optional classification/summaries. C's legacy transports remain dormant until Phase 4. Shared MIME work needed by Gmail proceeds with A/B/E and the mail-library owner, without activating SMTP/IMAP.

Use one Rust workspace and one Qt application with shared small fixtures when implementation begins. Protocol types do not import Qt/storage; GUI and CLI never write the store directly. Start coarse and split only at actual ownership boundaries. Do not build a separate CLI engine or alternate calendar schema.

## Decisions when needed

| Decision | Needed before | Independent work still possible |
| --- | --- | --- |
| Actual tools, native dependency route, host matrix | Claimed build/install support | Synthetic fixtures and available-host experiments |
| IDs, revisions, framing, request scope and access policy | Each shared client boundary | Isolated editor/library experiments |
| D014 actions and count projections | Read-on-open, Done=read+unstar and untimed hold are settled; P1-00 fixes exact typed snapshots/units | Shell primitives and separate calendar presentation |
| Retention/deletion, encryption, unsent work and recovery | Affected real-data schema/destructive behavior | Disposable synthetic stores |
| Google project/admin configuration | Access reported available; inspect actual scopes, publishing mode, Workspace policy and quotas before live gate | Shared HTTP/vault/provider fixtures |
| Remaining forms and provider settings | Current combined-calendar/timeline choices are settled; missing local forms are designed in their packet | Flight Progress v2 shell, mail/calendar presentation and local preferences |
| Best-effort HTML component/resources | Later HTML display | Text-based reading/replies |
| Local runner quality/locality/cancellation | Enabling that runner/policy | Manual labeling and fake inference |
| Non-Google providers and feed cases | Phase 4, after actual Google dogfooding | Google product improvement |

Append changes to decisions, update consumed contract revisions and retain executed evidence. No planning artifact substitutes for running the slice.
