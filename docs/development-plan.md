# Development plan

Updated 2026-09-11 under [D009–D011](decisions.md). **Planning only; no execution started. Google-first readiness: 0/9 verified.** [Status](status.md) tracks evidence; [contracts](contracts/README.md) own behavior; [research](research/README.md) preserves sources and alternatives.

## Delivery direction

Build for the founder's actual Google accounts first. Rust engine/CLI, C++20/Qt Quick/QML UI, private local IPC and SQLite/FTS5 remain the proposed baseline. QML syntax is accepted; Node.js and a separate JavaScript compilation/bundling pipeline are excluded. No stack or version lock is implied.

The first usable milestone includes a real GUI and a useful partial CLI. Implement the same small workflows in both, then expand their coverage together. IPC access, permission checks, account isolation, concurrency and recovery need both clients early; the GUI must not wait for exhaustive CLI commands or polished automation tooling.

Prioritize human correspondence and replies. Decode MIME, preserve original HTML/attachments and safely extract text from HTML-only mail from the start. Received HTML display can arrive later with best-effort readability. Browser-level fidelity, WebEngine packaging and legacy protocol compatibility are not early gates.

The user always chooses Done/Delayed. Classification supplies user-defined human/automation/noise and category/scope tags; it never changes review/action state. The latest mail/calendar/settings references now guide the initial slice. The founder explicitly requests settings plumbing only and says to ignore its unfinished switches. Remaining production interactions use the reviewed design for their slice.

IMAP, SMTP and external iCalendar URL support start only after the founder has dogfooded working Google GUI and CLI workflows. Retain their research and future contracts without implementing transports/feeds or making them early dependencies. MIME and Google recurrence/invitation representations remain needed.

## Phases and evidence

| Phase | Deliverable | Exit evidence |
| --- | --- | --- |
| 0 — Prove the paired foundation | Native builds, shared small fixtures, one engine/store, small CLI and text/composer GUI, Google prerequisites | Shared query/action through both clients; bounded state, private access and durable recovery; available-host evidence |
| 1 — First usable local slice | Synthetic mail/calendar through the actual engine, GUI and CLI; search, attachments, explicit flags and human drafts | Both clients agree; restart/offline/permission and account-isolation checks pass; incomplete CLI and nonfinal presentation are explicit |
| 2 — Google daily workflows | Gmail/Workspace mail and Google Calendar, setup, sending, full-history acquisition and an incrementally growing CLI | Real Google flows work; controlled founder use begins; no IMAP/SMTP/feed prerequisites |
| 3 — Google completion and dogfooding | Usable Google GUI and CLI; complete Google calendar/actions, expanded designs, intended local assistance and later best-effort HTML; delivery/quality evidence | Actual founder workflows, defects and retests recorded; Google-first release gates separately verified on required platforms |
| 4 — Later protocol support | IMAP, SMTP and external iCalendar subscriptions | Start only after founder Google GUI/CLI dogfooding; then prove their own interoperability and recovery |
| Later | Editable CalDAV; possible broad public/open-source launch | Separate decisions |

Phase detail decreases with distance. Only [Phase 0](plans/phase-0.md) has detailed packets. Refine the next slice from evidence rather than writing every future API in advance. Missing provider access, final design or test hosts blocks only the affected evidence.

The [proposed first self-contained package](plans/phase-0.md#proposed-first-deliverable-a-runnable-desktop-slice-on-mac-and-windows) draws a small runnable synthetic-mail slice from Phases 0/1, with repeatable development on the founder's Mac and Windows PC. It has its own bounded walkthrough and native build evidence; it does not wait for all Phase 0 experiments or complete the entire phase.

## Phase 0: a GUI before a CLI monolith

Start P0-00 and the small-fixture part of P0-01. Join Core/CLI/storage (P0-03/P0-10) with Qt GUI/input/composition (P0-04) promptly. The first boundary needs capability discovery, a bounded account-scoped query, an explicit journaled action, a durable draft and a small event view. It does not need every final command.

Use shared fixtures for MIME/text/attachment processing (P0-05), Google OAuth/sync setup (P0-02), Google recurrence/invitations (P0-06), and user-owned state/category behavior (P0-07/P0-09). Fake assistance can validate forbidden state changes without a model (P0-08). Package the small GUI/CLI slice early (P0-11), without a mandatory HTML engine.

Entry to Phase 1 is per ready slice: proven native build on available hosts, the private client/engine boundary, persistence, basic FTS and shared consumed identities/fixtures. The GUI needs readable text and a credible human editor, not a full received-HTML renderer. Missing Linux/Mac evidence stays unverified; independent work can continue.

Do not implement legacy transports/feeds, an exhaustive CLI grammar, a model installation manager or final visual polish in this phase. Record versions and measured outcomes only after work runs.

## Phase 1: the first usable local milestone

| Slice | Shared implementation and evidence |
| --- | --- |
| Workspace and ownership | GUI plus CLI connect to one engine; version negotiation, permission checks, concurrent launch and shutdown have deliberate behavior |
| Mail and search | Ingest synthetic Gmail-shaped mail, search/open exact conversation members and access retained attachments; both interfaces agree on account/coverage |
| Human actions and drafts | Explicit read/star/unstar/category actions and durable human reply/draft content; local MIME capture and restart recovery; no classifier-owned Done/Delayed |
| Calendar and shell | One Google-shaped day/week and bottom strip from a shared occurrence view; basic CLI range queries and journaled event edits; preserve reading position |

The GUI is functional and provisional, not a throwaway app with a second store. The CLI exposes the operations this slice actually implements. Record missing command coverage; do not mark the whole CLI Verified or wait for it to be finished before shipping this slice.

Before live Google work, prove source-to-store-to-search-to-reader-to-durable-action plus a draft and calendar edit in both clients. Exercise account boundaries, stale revisions, cancellation, lost clients and interrupted blob writes with no model/network. Use a local MIME capture/API fake, not an SMTP transport prerequisite.

## Phase 2: Google mail and calendar together

Implement consumer Gmail and Workspace setup, durable desktop OAuth, recent-first access with resumable full body/attachment retention, history reconciliation, labels/flags, signatures, human reply/forward/send and uncertain-submission recovery. All send/retrieve operations use Google APIs; one shared MIME interpretation serves GUI and CLI.

Develop Google Calendar alongside mail: discovery/permissions, sync, day/week/month/agenda, bottom strip, event edits, recurrence, invitations/RSVP and reminders as their fixtures pass. Recurrence/ICS representations used inside Google are not external-feed support.

Add CLI commands as their GUI/core workflows become usable. Begin controlled founder use on the available machine when the data/recovery gates are safe; a partial command set is expected initially. Record missing functionality rather than expanding into IMAP/SMTP to satisfy the old plan.

Durable auth, full offline progress and calendar correctness are core product work. Retention, account removal with unsent work, encryption and recovery must be settled before committing affected real-data behavior. Workspace access/admin restrictions have explicit gates.

## Phase 3: complete and dogfood the Google product

Refine this phase from actual use. Finish the agreed Google GUI/CLI workflows, calendar edge cases, navigation/settings, human composition and failure/recovery screens. Full Google workflow access is the intended CLI outcome; command completeness evolves with the product, not ahead of it.

Add optional local classification/summaries through the proven manual labeling path. Classify human correspondence, bots/lists and user-defined noise/category/scope; preserve manual precedence and explicit user Done/Delayed. Assistance stays off by default and does not block initial manual use.

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
| Exact Done/Delayed effects, read triggers and counts | Corresponding production interaction; user ownership is already settled | Explicit low-level commands and functional GUI |
| Retention/deletion, encryption, unsent work and recovery | Affected real-data schema/destructive behavior | Disposable synthetic stores |
| Google project/admin access | Live personal/Workspace gate | Local fixtures and paired client work |
| Remaining design details, settings controls, overlay visibility | Corresponding production behavior | Initial mail/calendar references and settings shell/navigation plumbing |
| Best-effort HTML component/resources | Later HTML display | Text-based reading/replies |
| Local runner quality/locality/cancellation | Enabling that runner/policy | Manual labeling and fake inference |
| Non-Google providers and feed cases | Phase 4, after actual Google dogfooding | Google product improvement |

Append changes to decisions, update consumed contract revisions and retain executed evidence. No planning artifact substitutes for running the slice.
