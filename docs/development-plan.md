# Development plan

**P2-06B has a Windows functional interim under D035**, from B3B `d174525`: ordinary organizer invitations and truthful observed-state recovery pass paired native/CLI checks. Workspace 529 passed/five ignored, isolated 149 unit plus three process, actual schema-16→17 upgrade/refusal/reopen, existing mail regressions and native 17/17 pass. The [280-artifact archive](research/assets/phase-2/p2-06b-interim/logs-manifest.json) preserves 340 source inputs, failures and diagnostics. Core 23/Calendar 16/Mail UI 21/CLI 16/Quality 19/schema 17; Gmail 11/Platform 12/Search 5, fixtures 0/3/protocol 1 unchanged. Continue D034 account workflows and remaining recurrence/reminders/packages/live admission. D031 still defers performance; whole readiness stays 0/9.

Updated 2026-09-13 under [D009–D035](decisions.md). **Phases 0 and 1 are complete on available hosts; Phase 2 implementation is active on `codex/phase-2`. Google-first readiness: 0/9 whole areas verified.** [Status](status.md) tracks evidence; [contracts](contracts/README.md) own behavior; [research](research/README.md) preserves sources and alternatives.

D033/P2-05B3B has a Windows functional interim for explicit source conversion, supported observed-target editing and remote-draft deletion/recovery. Core 22/Gmail 11/Mail UI 20/CLI 15/Quality 18/schema 16 define the checkpoint: 512 workspace tests, 146 isolated unit plus three process tests, paired workflows, actual upgrade/refusal and native 17/17 pass. Preserve the [337-artifact evidence](research/assets/phase-2/p2-05b3b-interim/logs-manifest.json), earlier failures and D031's performance deferral. Remaining Calendar invitations, recurrence and reminders follow, alongside account removal, current packages and actual-account acceptance.

D034 also requires account addition and removal through both interfaces. Existing OAuth addition and disconnect are separate from a complete removal workflow; settle retained local data, unsent work, uncertain receipts and credential cleanup in the shared boundary before implementation. The app must not delete Google mail, calendars or the Google account through local removal.

P2-05B2 has a [functional interim checkpoint](research/assets/phase-2/p2-05b2-interim/logs-manifest.json) under Core 20/Gmail 9/Mail UI 18/CLI 13/Quality 16/schema 14: workspace02 passes 469 tests with five ignored, isolated feature tests pass 132 unit plus three process cases, CLI03 verifies four generated Sends/nine Google operations, and native04 passes 15/15. Integration takes 150.731 seconds under the 180-second aggregate watchdog; primary 120-second and child 45-second limits are unchanged. Navigation01 fails the unchanged 100-ms p95 target at 158.3543/136.6387/177.9872/86.3356 ms for click/Tab/Shift+Tab/reader; all 400 exact paints, 1,640 sources and 176 parts pass. D031 directs “Keep working; measure later”: defer remeasurement, preserve the failure and continue B3 remote Drafts plus remaining invitations, recurrence and running-app reminders after their packet boundaries are agreed. B2 is not fully accepted; current packages, complete Calendar and actual-account gates remain open.

## Delivery direction

Build for the founder's actual Google accounts first. Rust engine/CLI, C++20/Qt Quick/QML UI, private local IPC and SQLite/FTS5 remain the proposed baseline. QML syntax is accepted; Node.js and a separate JavaScript compilation/bundling pipeline are excluded. Experimental versions are pinned and have native evidence; production qualification remains separate.

The first usable milestone includes a real GUI and a useful partial CLI. Implement the same small workflows in both, then expand their coverage together. IPC access, permission checks, account isolation, concurrency and recovery need both clients early; the GUI must not wait for exhaustive CLI commands or polished automation tooling.

Prioritize human correspondence and replies. Decode MIME, preserve original HTML/attachments and safely extract text from HTML-only mail from the start. Received HTML display can arrive later with best-effort readability. Browser-level fidelity, WebEngine packaging and legacy protocol compatibility are not early gates.

The user always chooses Done/Delayed. Classification supplies user-defined human/automation/noise and category/scope tags; it never changes review/action state. The Flight Progress v2 HTML reference guides Phase 1, including local settings preferences. D014 confirms read-on-open, Done clearing unread/stars, untimed Delay, combined full permitted calendar details and a browsable timeline only in Mail. Remaining production interactions use the reviewed design for their slice.

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

Phase detail decreases with distance. Completed [Phase 0](plans/phase-0.md)/[Phase 1](plans/phase-1.md) and approaching [Phase 2](plans/phase-2.md) have detailed packets. Refine the next slice from evidence rather than writing every future API in advance. Missing provider access, final design or test hosts blocks only the affected evidence.

The [first self-contained package](plans/phase-0.md#proposed-first-deliverable-a-runnable-desktop-slice-on-mac-and-windows) delivered a small runnable synthetic-mail slice from Phases 0/1, with repeatable development on the founder's Mac and Windows PC. Its bounded walkthrough and native build evidence are part of the now-completed Phase 0 experiments.

## Phase 0: a GUI before a CLI monolith

The packets below have been executed for the synthetic foundation; [execution evidence](research/phase-0-execution.md) records all available-host results and the separate physical/live-provider gates. Do not repeat this foundation as new work.

Start P0-00 and the small-fixture part of P0-01. Join Core/CLI/storage (P0-03/P0-10) with Qt GUI/input/composition (P0-04) promptly. The first boundary needs capability discovery, a bounded account-scoped query, an explicit journaled action, a durable draft and a small event view. It does not need every final command.

Use shared fixtures for MIME/text/attachment processing (P0-05), Google OAuth/sync setup (P0-02), Google recurrence/invitations (P0-06), and user-owned state/category behavior (P0-07/P0-09). Fake assistance can validate forbidden state changes without a model (P0-08). Package the small GUI/CLI slice early (P0-11), without a mandatory HTML engine.

Entry to Phase 1 is per ready slice: proven native build on available hosts, the private client/engine boundary, persistence, basic FTS and shared consumed identities/fixtures. The GUI needs readable text and a credible human editor, not a full received-HTML renderer. Missing Linux/Mac evidence stays unverified; independent work can continue.

Do not implement legacy transports/feeds, an exhaustive CLI grammar, a model installation manager or final visual polish in this phase. Record versions and measured outcomes only after work runs.

## Phase 1: the redesigned local milestone

The [detailed Phase 1 plan](plans/phase-1.md) is completed on available hosts, including the updated design, local workflows and navigation/rendering spike. Preserve the existing engine/GUI/CLI and source-specific [execution evidence](research/phase-1-execution.md); whole-product/physical/live-provider qualification remains separate.

P1-00 freezes the next shared types/fixtures. P1-01 adds typed state, grouped queries/counts/actions and combined calendar scope while P1-02 establishes native visual primitives/settings. P1-03 mail/search and P1-04 calendar/timeline then proceed in parallel. P1-05 finishes durable human replies/local capture while P1-06 exposes bounded nonrecurring create/edit/self-RSVP. P1-07 integrates native visual/input, cross-client, recovery/upgrade and package checks.

The deliverable is a usable synthetic local client with the updated design, truthful state and relevant CLI parity. Full provider setup, destructive account removal, models, received HTML rendering, timed snooze/follow-up and legacy protocols remain later. Core owns shared schema/protocol changes; UI owners use separate components without alternate stores. Calendar creation, capture-to-Sent projection and saved signature semantics are explicit implementation work in the plan.

No Google credentials or further product decision is needed to begin the first packets. Small missing composer/calendar forms are designed and reviewed within their slice. Real-data/OAuth policy gates still apply before Phase 2.

## Phase 2: Google mail and calendar together

D020's [detailed packets](plans/phase-2.md) start from merged `885de67` on `codex/phase-2`; D021 starts implementation after planning checkpoint `0ced122`. The founder has Cloud/OAuth/admin access and selected OS permissions plus verified disk encryption for the pilot; app-level mail-store encryption is deferred. [Execution evidence](research/phase-2-execution.md) distinguishes current tested slices from live admission and full-phase acceptance.

Implement consumer Gmail and Workspace setup, durable desktop OAuth, recent-first access with resumable full body/attachment retention, history reconciliation, labels/flags, signatures, human reply/forward/send and uncertain-submission recovery. All send/retrieve operations use Google APIs; one shared MIME interpretation serves GUI and CLI.

Develop Google Calendar alongside mail: discovery/permissions, sync, day/week/month/agenda, the Mail-only header/footer timeline, event edits, recurrence, invitations/RSVP and reminders as their fixtures pass. Recurrence/ICS representations used inside Google are not external-feed support.

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
| D014 actions and count projections | Read-on-open, Done=read+unstar and untimed hold are settled; P1-00 fixes exact typed snapshots/units | Shell primitives and separate calendar presentation |
| Retention/deletion, encryption, unsent work and recovery | Affected real-data schema/destructive behavior | Disposable synthetic stores |
| Google project/admin configuration | Access reported available; inspect actual scopes, publishing mode, Workspace policy and quotas before live gate | Shared HTTP/vault/provider fixtures |
| Remaining forms and provider settings | Current combined-calendar/timeline choices are settled; missing local forms are designed in their packet | Flight Progress v2 shell, mail/calendar presentation and local preferences |
| Best-effort HTML component/resources | Later HTML display | Text-based reading/replies |
| Local runner quality/locality/cancellation | Enabling that runner/policy | Manual labeling and fake inference |
| Non-Google providers and feed cases | Phase 4, after actual Google dogfooding | Google product improvement |

Append changes to decisions, update consumed contract revisions and retain executed evidence. No planning artifact substitutes for running the slice.
