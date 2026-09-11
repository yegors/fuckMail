# Decision log

Append-only history of decisions, pivots and Q&A. Append new entries at the end with a unique ID and ISO 8601 timestamp/timezone, context/question, outcome, rationale and affected documents. Corrections and superseding decisions reference prior IDs; do not rewrite old entries. Current scope belongs in [product scope](product-scope.md), implementation questions in [contracts](contracts/README.md), and progress in [status](status.md).

Timestamps below record when entries were recorded/imported. Earlier discussion timestamps were not retained; they are not reconstructed. Entries distinguish accepted requirements from proposals and unresolved questions.

## 2026-09-11T06:02:43Z — D001: Import the existing planning record

**Outcome: historical record imported.** Source: the former decisions-and-questions.md and its recorded brief/interview/clarification bases. That document was last updated 2026-09-11; initial planning/research was dated 2026-09-10. Exact original discussion timestamps are unavailable. Existing C/P IDs and their accepted/proposed status are preserved below. Relative links have been updated for the new layout.

This is a snapshot, including then-open questions. Later entries supersede specific choices; the owning contracts carry current open items. In particular, P03's evaluation order is superseded by D003. No engineering proposal below is retrospectively promoted to an accepted stack decision.

### Accepted product decisions

| ID | Decision | Basis |
| --- | --- | --- |
| C01 | Mail and calendar only; multiple large mailboxes | Original brief |
| C02 | Windows and macOS required; Linux nice to have | First interview |
| C03 | Separate personal/work accounts; no unified mailbox | First interview |
| C04 | Conversation-only mail; mouse-first interaction; shortcuts secondary | First interview |
| C05 | Fast local sender name/email and subject/body keyword search | First interview |
| C06 | All messages and attachments stored offline | First interview |
| C07 | Prefer Google APIs; IMAP and SMTP also required | Original brief and first interview |
| C08 | Google Calendar with day/week/month/agenda, create/edit, recurrence, invitations/RSVP, shared calendars and reminders | First interview |
| C09 | Read-only iCalendar URL subscriptions in MVP; editable CalDAV later | Second interview |
| C10 | Local processing/storage and direct provider connections; no app-operated remote server; optional same-machine model runner | First interview, clarified by C19 |
| C11 | Personal use first; possibly open source later; prefer modern tools | First interview |
| C12 | Close means quit; no hidden background sync or reminders after closing | Second interview, applied to app lifecycle |
| C13 | Plain-text/HTML mail support and per-account signatures | Second interview |
| C14 | Exclude tasks, widget dashboards, contacts and broad assistant features; local classification/summaries are the sole AI exception | Original brief; blanket AI exclusion superseded by C19 on 2026-09-11 |
| C15 | Avoid the JS ecosystem; use a native cross-platform UI, with no JS/TypeScript application or npm/Node pipeline | Subsequent founder clarification; supersedes initial lack of stack restrictions |
| C16 | Visual quality matters; investigate Rust + Slint + SQLite and real UI examples before choosing | Subsequent founder request; investigation is not stack acceptance |
| C17 | Daily success is zero unread and zero starred messages; stars hold mail while action is underway, not as long-term favorites | Founder clarification, 2026-09-11 |
| C18 | Use configurable categories mapped to native Gmail labels or IMAP folders, applied manually or by configured local assistance | Founder clarification, 2026-09-11 |
| C19 | Include optional local Ollama/vLLM classification of incoming mail and summaries of messages/conversations; complete manual operation without a model | Founder clarification, 2026-09-11; supersedes the original blanket MVP AI exclusion |
| C20 | Humans write email for humans: no AI drafting, rewriting, autocompletion or sending | Founder clarification, 2026-09-11 |
| C21 | Incorporate useful exec-buddy mail/calendar interactions without copying the app or its unrelated integrations | Founder instruction after product review, 2026-09-11; concrete proposed interactions are in product scope |

### Known problem and scale

The founder uses eM Client and reports that large-mailbox performance, UI complexity and ugliness, and unreliable Google Calendar integration are its main failings for this workflow. This records the founder's experience, not a measured diagnosis of that product.

A supplied local disk-usage snapshot totals about 25 GB, with large directory subtotals around 2.9 GB, 7.6 GB and 15 GB. Do not sum nested entries again, infer exact account count, or assume every server attachment is downloaded. Message counts, complete server sizes and non-Google provider names are not yet known. The founder instructed us to assume high-end machines.

Aliases were explained as additional From addresses associated with an account. No requirement was accepted; use each account's normal sending address and defer alias management.

Long-lived unread/starred mail conflicts with the founder's preferred daily routine. Keep unfinished work discoverable and support clearing it through actual review/action. Do not interpret the zero goal as authorization for automatic read/unstar, hiding old holds, or erasing pending work.

### Proposed engineering decisions

| ID | Proposal | Evidence needed |
| --- | --- | --- |
| P01 | Shared local core with a small cross-platform desktop UI | Native interaction, lifecycle and rendering experiments on Windows/macOS |
| P02 | Benchmark SQLite/FTS5 first; evaluate Tantivy if needed | Sender/body/subject retrieval, conversation grouping, concurrent ingestion and recovery measurements |
| P03 | Evaluate Rust + Slint + SQLite/FTS5 next; retain Qt as a fallback | Visual quality, HTML-reader/composer integration, accessibility, dependency and performance experiments on both OSes; see [Slint investigation](research/slint-ui.md) |
| P04 | Recent conversations first, then full resumable history and attachment acquisition | Actual quotas, mailbox inventory, disk-space and throughput measurements |
| P05 | Proposed 100 ms warm-search core budget, 200 ms visible-search budget, million-message primary fixture | High-end reference devices and repeatable benchmark results |
| P06 | Google mail/calendar integration in parallel; IMAP/SMTP before complete MVP | Shared contracts, test fixtures and actual non-Google server compatibility |
| P07 | Short transactions, durable operations and staged reconciliation | Restart, invalid-cursor, low-disk and ambiguous-send experiments |
| P08 | Native application shell; isolate an HTML renderer to received message content as needed | Representative email fidelity, script-disabled rendering, process isolation and memory measurements; zero browser-engine dependencies are not assumed |
| P09 | Manual/default, Suggest and Auto-apply category modes; independently optional summaries through bounded, versioned local inference | Ollama/vLLM compatibility, correction precedence, structured-output quality, offline/manual fallback, cancellation and resource measurements |
| P10 | Sender/category filter chips, per-account reading-position restoration, compact current/next and all-day calendar displays, Join-and-acknowledge, and calendar search | Mouse-first workflow fixtures on both platforms; unchanged account boundaries and no false triage all-clear |
| P11 | Gmail supports configured multi-label categories; generic IMAP uses a primary category folder with explicit move semantics | Cross-client persistence, UID changes, mixed conversation membership, renamed destinations and recoverable manual/automatic filing |

### Remaining design details, not another broad interview

These can be resolved during their workstream before the affected behavior ships:

- **Provider inventory and scale:** account count, message counts and non-Google IMAP/SMTP services; actual free disk and Windows/macOS reference hardware.
- **Mail UX:** reading-pane geometry, conversation ordering, read-on-open timing, thread-level action targets, old-hold presentation, simple formatting controls, signature format, and whether drafts must synchronize to other clients. Unread/star meanings and the daily zero goal are accepted.
- **Classification setup:** user-chosen category names/descriptions and per-account label/folder mappings, Gmail filing effects, IMAP primary destination, local runner/model availability and quality thresholds. These are configuration/prototype details; optional classification and summaries are accepted, and AI authorship is excluded.
- **Search:** representative names/addresses and keywords, supported languages, partial-match semantics, and relevance versus newest-first ordering. Global search is not assumed.
- **Calendar:** overlay/current-next visibility defaults, common recurrence editing controls, running-app alert preferences, reminder horizon, cached search coverage and handling of invitations appearing in multiple accounts. Editable CalDAV is already deferred.
- **Retention/privacy:** trash/spam scope, behavior after provider deletion, app-level mail-store encryption, remote images, and backup/export expectations. Complete offline mail must not silently become recent-only storage.
- **Distribution:** minimum OS versions/architectures, personal build packaging, later license choice and public-release signing/updating/verification.

Record subsequent decisions with supporting founder input or experiment results and update affected documents together. No benchmark has run and no mailbox has been accessed as part of this planning work.

## 2026-09-11T06:02:43Z — D002: Preserve the stack/QML and exec-buddy discussion

**Outcome: research and product direction recorded; no stack selected.** These questions preceded the documentation reorganization; their original timestamps are unavailable.

- **Why C++/Qt instead of Rust and alternatives?** Compare the supplied design, complete reader/composer, large-list behavior, platform integration and maintenance costs. The current [desktop research](research/desktop-stack.md) records candidates and tradeoffs. Neither language/toolkit nor database is accepted on the strength of documentation alone.
- **What does QML give us?** A declarative UI with Qt Quick visual/input/model facilities and a C++ logic boundary, potentially useful for a custom shared design. It also introduces QML/JavaScript integration and a runtime boundary. This explains an option; it does not accept QML, overturn C15 or require an npm/Node application. Resolve fit during stack selection. Evidence is linked in the desktop research.
- **Which exec-buddy elements fit?** C21 accepts selected mail/calendar product inspiration. Sender/category filters, view restoration, compact meeting/all-day presentation, Join acknowledgement and calendar search are proposed adaptations. C17–C20 govern star semantics, categories, local summaries and human authorship. The [source review](research/exec-buddy.md) is not a code-port plan or evidence that its incomplete Google Calendar path solves our requirements. Unrelated integrations remain excluded.

Affected: desktop/Slint/HTML and exec-buddy research; mail UI, triage, calendar and local-assistance contracts. No benchmark or implementation result is asserted.

## 2026-09-11T06:02:43Z — D003: Scope the documentation and wait for the shared design

**Outcome: accepted founder direction.** Request: reorganize existing material into scoped research and parallel build contracts, keep product direction/status at docs root, retain timestamped append-only decisions/Q&A, and add indexed READMEs with honest feature/readiness reporting. Create the framework and place existing knowledge; do not fill speculative future files.

- **C22 — One opinionated design across OSes.** Windows/macOS share the founder's design. OS-native visual fitting-in and a broadly customizable app for everyone are not goals. This sharpens C11/C16; required platform input, accessibility and lifecycle behavior still need verification.
- **C23 — Design review precedes stack selection.** The founder is preparing the design. Supersede P03's Slint-first/Qt-fallback order and any blanket Widgets-only recommendation. Rust/Slint and C++/Qt, including the QML option, remain research candidates. No stack is selected by this reorganization.
- **C24 — Documentation roles.** docs root holds product direction, status and this history. research/ holds topic-specific evidence; contracts/ holds living area specifications with owners, inputs/outputs, dependencies and acceptance gates. READMEs index current and future documentation; no general research.md and no empty speculative file set.
- **C25 — Honest readiness.** Report 0% implemented/verified MVP readiness while the repo contains only planning documents. Status defines the nine required feature areas and evidence rule; documentation progress is separate. Future scope changes must identify any changed readiness denominator.

Rationale: let the design inform implementation while keeping research, current specifications, delivered behavior and historical choices distinct. Parallel buildability requires an agreed shared contract/fixture revision; the present Draft contracts are the framework, not a claim that the interfaces or application are complete.

Affected: [documentation index](README.md), [north star](north-star.md), [product scope](product-scope.md), [status](status.md), [research index](research/README.md), [contracts](contracts/README.md) and [repository README](../README.md).

## 2026-09-11T06:17:12Z — D004: Make deferred action and time visible

**Outcome: accepted founder direction, with design treatments still proposed.** The founder is collaborating with Claude Design and requested a visual/interaction brief describing the elements needed.

- **C26 — Stars expose deferred action.** Refine C17: starring an email means leaving action for later, even when work has begun. The UI should make that outstanding action clear. Read/unread and star state stay independent; the daily zero goal does not authorize automatic cleanup.
- **C27 — Today's calendar stays at the bottom of mail.** A persistent strip is required on all mail screens, alongside the dedicated day/week/month/agenda calendar already required by C08. This supersedes the earlier open placement/visibility treatment of compact current/next status. Which calendars populate the strip remains open.
- **C28 — Mailbox selection belongs at the top.** The brief interprets mailbox as the connected account, consistent with C03; the selector's exact visual form remains open. Mail remains separated by account.
- **C29 — Main mail uses a resizable split.** Pane orientation, default ratio, minimum sizes and detailed styling remain design choices. A conversation list beside the reader/composer is a proposal for exploration, not an accepted mockup.

Create [ui-ux.md](ui-ux.md) as the working brief and index future supplied design artifacts there. Proposed treatments include explicit deferred-action copy, honest hold age, a horizontal today timeline and remembered split position. They do not add task management, due dates, scheduled snooze or new notification policy. Stack selection still follows design review under C23.

The brief leaves final copy/visuals, pane arrangement and calendar selection open. Mail UI owns the split and strip placement; Calendar supplies the shared day/occurrence view and event actions; Core preserves view state; Triage owns state/count semantics. Their Draft contracts advance to revision 1 without claiming implementation or agreed executable interfaces.

Affected: [visual brief](ui-ux.md), [north star](north-star.md), [product scope](product-scope.md), [mail UI](contracts/mail-ui.md), [calendar](contracts/calendar.md), [triage](contracts/triage.md), [core](contracts/core-storage.md), [contract index](contracts/README.md), [status](status.md) and both documentation indexes. MVP readiness remains 0%.

## 2026-09-11T06:34:15Z — D005: Require Linux, a full CLI and simple installation

**Outcome: new founder requirements accepted; technical baseline recommended, not locked.** The founder supplied three mockups as visual direction for technology choices, explicitly not final or exact designs.

- **C30 — Linux is mandatory.** Windows, macOS and Linux are required, including Ubuntu, Arch and Omarchy. This supersedes C02 and all earlier Linux-as-stretch/deferred treatment. Pin and test actual versions, architectures and desktop configurations.
- **C31 — Ship a full CLI.** Third-party agents and humans can use applicable mail, calendar, account, search, organization, draft/submission, sync and assistance workflows without a GUI. GUI and CLI share the core, state and reliable operation semantics. It is a separate required feature area, not a read-only export tool.
- **C32 — Installation is part of the product.** Deliver prebuilt GUI/CLI packages with runtime dependencies, clear first-run setup, command discovery and a maintained update/recovery route. Users should not need a developer environment. Required packaging/signing/dependency work can no longer be deferred wholesale to a possible public launch.
- **C33 — Prefer useful modern technology, not novelty alone.** Choose mature technology when it demonstrably serves the requirements better.
- **C34 — Mockups are references.** Preserve the supplied images and their visual implications without adopting their exact layout, copy or sample states. In particular, the sidebar agenda in image 3 does not supersede C27's bottom strip.

**P12 — Recommended stack:** C++20, Qt 6 Quick/QML, SQLite/FTS5, an isolated Qt WebEngine reader and Qt text/document facilities for the human composer, built with CMake/Ninja. Reasons: custom UI fit, one headless core for GUI/CLI, and fewer rich-mail/UI integration boundaries than the currently researched Rust/Slint route. Costs: C++ memory/thread discipline, QML bindings, Chromium size/memory/updates, dependency licensing and per-platform packaging. QML uses JavaScript expressions; building WebEngine from source can require Node/Python even though no npm application frontend or end-user SDK setup is proposed. These are explicit tradeoffs under C15, not a silent exception or a measured selection.

**P13 — Proposed lifecycle interpretation for C31:** one on-demand local engine, with explicit GUI and CLI sessions. Closing the GUI still quits its work/reminders under C12; independently invoked CLI commands/watch can run separately and end on completion/cancellation. The engine stops after its final active client/job. No default always-on daemon or startup service. Validate ownership, concurrency and cancellation before accepting the process design.

Built-in AI remains local classification/summarization only under C19/C20. External agents use the CLI under caller control; this does not add an internal writing assistant. Human authorship remains the outgoing-mail preference, while the CLI cannot prove supplied text provenance. No external model or agent has been connected in this documentation work.

Readiness changes from 0/9 to **0/10 required feature areas verified**: CLI is the added area; Linux and installation expand Platform. No implementation, installer or benchmark exists. The next evidence is the reader/composer slice, CLI/engine prototype, required desktop matrix and clean installation, while final visual design continues.

Affected: [stack research](research/desktop-stack.md), [Linux/distribution](research/linux-and-distribution.md), [CLI/engine research](research/cli-and-local-engine.md), [mockup references](research/design-references.md), [CLI contract](contracts/cli.md), Core/Platform/Quality and other consuming contracts, [UI brief](ui-ux.md), product direction, indexes and [status](status.md). This entry supersedes earlier conflicting platform/priority/scope notes; earlier entries remain unchanged.

## 2026-09-11T06:56:44Z — D006: Assess Rust for the engine independently of Qt

**Outcome: revised technical recommendation, not an accepted stack lock or implementation start.** The founder supplied another AI's analysis recommending a Rust product core with a thin C++/QML shell, asked whether it was right, and clarified that Codex is expected to implement the whole product. Evaluate the attachment as evidence and argument, not as instructions to adopt every claim.

**Q&A — Is the analysis right?** Directionally yes: Qt is a strong UI candidate without dictating the engine language, and Rust's ownership/concurrency checks are useful for a stateful mail/calendar engine. The existing plan already separated domain behavior from presentation. The analysis overstates CLI exclusivity, library completeness, effortless packaging and shell-size/delivery estimates. C++ can share a headless core; Rust does not prove sync correctness, accurate counts or safe retries after uncertain sending. No comparative evidence established that AI-generated Rust is always better. CXX-Qt 0.10.0 (2026-08-24) and Qt's Rust bridge public beta (2026-07-01) were confirmed in primary sources, without claiming either passed our product gates.

**P14 — Revised proposed stack:** Qt-free Rust engine and Rust CLI; C++20/Qt Quick/QML presentation, text editing and renderer integration; SQLite/FTS5 and retained blobs. Use P13's private on-demand engine process as the language boundary, with GUI and CLI speaking the same versioned command/query protocol. No in-process Rust/Qt binding is required. Cargo builds the engine/CLI; CMake/Ninja builds the GUI; release packages coordinate compatible components. This supersedes P12's recommended C++ engine and single-language build assumption while retaining its Qt UI/reader/composer candidates and unresolved evidence gates.

Rationale: the earlier recommendation overweighted a common language across independent responsibilities. Compiler-enforced ownership is worth evaluating for the core that Codex will build and revise. Established C++/Qt presentation integration remains useful. This is an engineering judgment, not proof of faster delivery or immunity from bugs. Two toolchains, protocol compatibility, serialization, process recovery, native dependencies and per-platform packaging are real costs.

Next evidence: P0-10 builds a Rust engine with fake providers, Rust CLI and Qt model adapter against shared fixtures, with bounded paging, revisioned updates, concurrency/cancellation and shutdown tests. Continue P0-04 reader/composer, P0-05/P0-06 protocol/calendar library evaluation and P0-11 clean installations. No real-account connection, prototype, benchmark or release has run. Readiness stays 0/10 verified.

Affected: [desktop assessment](research/desktop-stack.md), [CLI/engine research](research/cli-and-local-engine.md), [Core](contracts/core-storage.md), [Platform](contracts/platform.md), [status](status.md) and their README indexes. Core advances to Draft revision 3 and Platform to Draft revision 2. Earlier decision entries remain unchanged.

## 2026-09-11T07:14:02Z — D007: Name Google Workspace as required Google support

**Outcome: accepted founder clarification.** Corporate Workspace mail and calendar are in MVP as part of Gmail/Google support, not a separate provider. This names the account types already implied by C03/C07 and the Gmail contract's personal/Workspace OAuth gates.

- **C35 — Google Workspace is required.** Consumer Gmail and Google Workspace accounts must both work for mail and calendar. Same Gmail API and Google Calendar API path; not a second stack.
- **C36 — OAuth setup is product work.** Installed-app OAuth, durable tokens and Workspace admin consent/restrictions are required setup, not an afterthought. Public-app verification stays deferred unless the audience changes.

Does not add Exchange, a Workspace admin console, or extra Google products. Readiness remains 0/10.

Affected: [product scope](product-scope.md). Gmail contract and research already stated personal/Workspace OAuth; this makes the product-level requirement explicit.

## 2026-09-11T07:57:53Z — D008: Prepare phased development around closer design references

**Outcome: accepted planning/design direction; execution plan proposed and prepared, implementation not started.** The founder supplied closer mail and calendar mockups, explicitly still nonfinal, and requested documentation updates plus a full phased plan for Codex to execute later. This pass must contain research/planning only, with detail decreasing as uncertainty increases in later phases.

- **C37 — Closer references, final designs later.** Preserve the new pair as current structural direction and retain the three earlier references as history. Final mockups arrive closer to production UI implementation. Headless work and provisional feasibility surfaces can proceed before them once implementation is requested.
- **C38 — Plan progressively.** Prepare concrete immediate packets and a full product path; leave later phases at outcome/dependency/evidence level and refine them as they approach. Do not prewrite speculative future APIs or code to make the repo appear ready.
- **C39 — This is a documentation-only preparation.** No application source, build/CI configuration, harness, package, account connection, model setup or implementation workstream is started by this request. Development preparation and verified product readiness remain distinct.

Observed direction: compact split mail, shared top context bar, collapsed earlier thread members, review/delayed/done visual grouping, a thin today footer, and a week calendar with subdued Busy overlays. Sample Done/Delay copy, count units, read triggers, overlay selection/permissions, footer metrics and full-calendar footer behavior are not accepted commands/defaults. Missing composer, agenda, settings and error screens do not remove their required workflows. Record their handoff gaps in the UI brief and contracts.

**P15 — Execution sequence:** Phase 0 proves build/fixtures, shared private engine/CLI/UI behavior, storage, reader/composer, provider/calendar and packaging feasibility. Phase 1 integrates local synthetic mail/calendar and human drafts. Phase 2 delivers consumer Gmail/Workspace daily workflows with Calendar alongside mail. Phase 3 completes IMAP/SMTP, calendar/feeds, optional local assistance and remaining CLI/GUI coverage. Phase 4 verifies quality, final design, smooth personal delivery and the founder trial. Later CalDAV/public-launch work remains separate. This reorganizes the earlier proposed phase allocation without changing the ten required feature areas.

The detailed starting packet is P0-00 plus the small-fixture portion of P0-01 when implementation is requested. Other P0 IDs retain their existing research scopes. Minimum slice gates can unblock independent work without waiting for every provider/model/platform result; missing external access remains unverified at its own gate. Final UI decisions, retention/encryption, live provider access and signing are resolved before the affected behavior is committed or shipped. No stack/version lock follows from this plan; P14 remains the technical proposal.

Research now provides primary-source-backed near-term candidates and experiment recipes for Google OAuth longevity/Workspace restrictions, Rust mail/calendar libraries, Qt HTML/composer constraints, private IPC/lifecycle, SQLite/blob durability and clean installation. Source evidence is not a test result. The only development-environment observation is a Windows PowerShell PATH lookup finding Git and not the listed Rust/Qt/build commands; no machine-wide install audit or tool installation occurred.

Affected: [development plan](development-plan.md), [Phase 0 backlog](plans/phase-0.md), [UI brief](ui-ux.md), [design references](research/design-references.md), scoped research, [Core](contracts/core-storage.md) Draft revision 4, [Mail UI](contracts/mail-ui.md) and [Calendar](contracts/calendar.md) Draft revision 3, [Quality](contracts/quality.md) Draft revision 2, contract workflow, status/product scope, indexes and repository agent guidance. Earlier entries remain byte-for-byte unchanged. All areas remain Draft; MVP readiness remains 0/10 verified.


## 2026-09-11T17:18:54Z — D009: Google-first dogfooding, paired clients and pragmatic mail display

**Outcome: accepted founder clarifications and delivery order; no implementation start or final stack lock.** The founder answered the review questions and stated that all current mailboxes use Google APIs. This supersedes the conflicting protocol, HTML and sequencing requirements in C07/C09/C13/C15/C31 and P12–P15 only as described below; other accepted requirements remain in force.

- **C40 — QML syntax is acceptable.** “Non-JS ecosystem” means no Node.js requirement or separate JavaScript compilation/bundling pipeline. QML expressions are fine. Use normal native Rust/Qt build tools; do not make a Node-dependent upstream WebEngine source build a product prerequisite. If a later dependency cannot be supplied within this constraint, propose another component or a concrete tradeoff.
- **C41 — Done and Delayed belong to the user.** Classification never decides completion, postponement, review, read/unread or stars. The classifier distinguishes human correspondence from bots, mailing lists and user-defined noise, assigning the user's account-specific category/scope tags. “Needs my attention” is classification metadata, not evidence of review or a command to change action state. A user-defined spam/noise tag does not authorize Gmail system Spam/Trash actions. Manual correction wins. Exact button-to-provider-flag mappings and read triggers remain a focused UI handoff choice; user ownership is settled.
- **C42 — Human correspondence is the first reading path.** Received HTML display can come in a later phase with best-effort readability, not browser-level visual fidelity. It is not a stack-selection or first-usable-milestone gate. Retain source MIME, original HTML and attachments; MIME parsing, safe text extraction for HTML-only mail, human replies, signatures and durable drafts remain early work. Plain/limited formatted human composition remains intended; its editor does not require a full received-HTML renderer. Do not infer a uniform HTML/CSS subset from the relaxed fidelity requirement, or reduce content-safety requirements with visual fidelity.
- **C43 — More complete designs are coming.** The founder will supply updated screens, including settings. They guide the affected production UI slices; early functional GUI/IPC work can proceed before those screens are final.
- **C44 — Develop CLI and GUI together.** The first usable milestone includes a small real CLI and a real GUI against one shared engine/store. Use both to iterate on IPC, permissions, account isolation and recovery. Extend the CLI with each implemented workflow; a complete CLI command surface must not precede or block GUI development. Before protocol expansion, both clients must be working and usable for the agreed Google workflows and actually dogfooded by the founder.
- **C45 — Google first, legacy protocols after dogfooding.** Gmail and Google Calendar APIs cover all current accounts. Implement and dogfood the Google client before starting IMAP, SMTP or external read-only iCalendar URL support. Retain their designs as deferred work; no early transport/feed implementation or compatibility gate. MIME and Google Calendar recurrence/invitation formats are still required, not deferred merely because they use older standards. CalDAV remains separately deferred.

**P16 — Revised delivery plan:** Phase 0 proves the native build, small shared engine/CLI/GUI boundary, storage, readable text/human composition and Google prerequisites. Phase 1 joins a small usable local slice with both clients. Phase 2 grows real Google mail/calendar workflows with the CLI beside the GUI and begins controlled founder use. Phase 3 completes and dogfoods Google workflows, adds the intended optional local assistance and later best-effort HTML, and verifies the Google-first delivery gates. Phase 4 begins IMAP/SMTP/iCalendar subscription work only after the founder's actual Google CLI/GUI dogfooding. Do not turn later HTML polish or exhaustive CLI tooling into prerequisites for first use.

Readiness now reports the **Google-first MVP: 0/9 required areas verified**. IMAP/SMTP moves out of the current denominator; external feeds move out of the Calendar milestone's gates. The eventual expanded-protocol scope remains recorded separately. This denominator change is a scope change, not progress. Optional local assistance remains optional to enable and intended for the Google-first product; it does not block the first usable manual milestone. Required OS support, full offline storage, human composition, lifecycle and recovery are unchanged. All contracts remain Draft; no code or results are added.

Affected: root/agent guidance, product scope, north star, status, development/Phase 0 plans, UI brief, contract index and Core/Mail UI/Triage/Local assistance/CLI/Calendar/Gmail/IMAP-SMTP/Platform/Quality contracts, plus the corresponding scoped research. Prior entries remain unchanged.

## 2026-09-11T18:03:17Z — D010: two development hosts and the proposed first deliverable

**Outcome: two-host development requirement accepted; first work-package scope proposed. No implementation or installation requested yet.** The founder asked for a self-contained piece of work with a usable deliverable and prerequisites, and explained that development will alternate between a work Mac and home Windows PC.

**C46 — Both Mac and Windows are development hosts.** Provide repeatable environments and build/test pipelines that the founder and coding agents can use on either machine. Carry source and accurate setup/progress information between sessions. Existing required Linux product support is unchanged; it is not necessary to establish every Linux desktop before the first Mac/Windows development slice.

**P17 — First runnable desktop slice:** propose a Rust engine, small headless CLI and Qt Quick GUI using one SQLite-backed synthetic profile: two accounts, a few hundred conversation messages, retained attachments, explicit read/star actions, a plain-text reply draft that survives restart, and a simple calendar agenda/footer/CLI query. Test shared IPC state, account scoping, concurrent clients, stale requests and shutdown. Deliver pinned compatible build inputs, setup/diagnostic/build/test/run/package entry points, native Mac/Windows CI artifacts and a handoff guide. The detailed acceptance walkthrough lives in the Phase 0 plan; this package does not require all Phase 0 experiments.

The Mac can be inspected and tested locally; the Windows build can be checked on a native runner, with physical PC graphics/input/runtime checks recorded when that host is available. No Google credentials, final designs, models or signing certificates are needed for this synthetic development package. Real provider operations, calendar editing/recurrence, full CLI coverage, large-scale performance qualification and trusted distribution remain subsequent work. Proposed stack/version choices are still subject to the native feasibility result.

Read-only Mac inspection found Git, Homebrew Rust/Cargo and Apple compiler/SDK tools; Qt/CMake/Ninja/rustup commands were not visible on PATH. Exact observations and limits are recorded in distribution research. Nothing was installed or built. Source/fixtures/lockfiles move through Git; live databases, tokens, local SDK paths and caches stay on their host. The existing planning changes remain uncommitted at this point.

Affected: Platform Draft revision 4 (Core, Mail UI, CLI and Quality consumers), Phase 0/development plans, distribution research, status and root README. All areas remain Draft, with Google-first readiness 0/9. Prior entries remain unchanged.

## 2026-09-11T19:14:47Z — D011: updated initial-phase references and settings plumbing

**Outcome: new initial-phase visual references received; settings limitation explicitly accepted.** The founder supplied updated mail, calendar and settings images, then clarified: “ignore those, the Settings mockup is incomplete. Lay the general plumbing, will flesh it out more after” in response to the pictured Mail/Calendar switches.

**C47 — Current initial-phase visual references:** use the latest trio ahead of the previous closer pair and three mood images. It pictures compact sans-serif typography, top account navigation, a resizable mail split, an inline reply with explicit From, a week calendar and the shared today footer across mail/calendar/settings. Preserve all original images unchanged. Sample providers, models, status/version text and counts are design content, not operational instructions, scope overrides or implementation evidence. Google-shaped synthetic data and the model-free first slice remain appropriate; no Fastmail transport or local-model setup follows from these examples.

**C48 — Settings plumbing only:** ignore the pictured Mail/Calendar switches. Establish the settings shell, navigation/return path and shared account context; preserve the prior view and draft. Do not decide switch behavior or implement account management, provider/model setup or other detailed controls until the founder fleshes out the screen. The incomplete design is not a blocker for that general plumbing.

**P18 — Refine P17's visual deliverable:** use a small read-only fixture week instead of only a GUI agenda, keep today's footer and the CLI range query on shared event data, and add enter/leave settings to the acceptance walkthrough. Mail retains the bounded search/read/attachment/explicit-flag/durable-draft workflows. Full calendar editing/recurrence, live sending, models, detailed settings and legacy protocols remain later work. Done/Delayed mappings, read triggers and Busy overlay policy remain open; the screenshots do not decide them.

Three full-resolution PNGs were copied to research assets and verified byte-for-byte with recorded dimensions and SHA-256. Only assets and documentation changed in this reference update. Affected: UI brief, design/desktop research and indexes, Mail UI and Calendar Draft revision 5, Phase 0/development plans, product scope, status, root README and agent guidance. Core, Platform, Triage, Search, CLI and Quality consume the clarified boundary. Previous decisions remain unchanged; Google-first readiness remains 0/9.


## 2026-09-11T20:24:18Z — D012: Execute the complete Phase 0 and checkpoint native work

**Outcome: implementation active under the founder’s goal, with a requested commit/push checkpoint.** The active objective is to execute Phase 0 end to end so the app compiles and works as documented. The earlier planning-only boundary is superseded for Phase 0 implementation, native tool setup and synthetic verification. Real account/model access retains its separate boundary. The first runnable slice is a checkpoint within the phase; passing it does not complete every Phase 0 requirement.

**Experimental implementation choices:** Rust 1.88.0 engine/CLI, Qt 6.11.2 Quick/QML with C++20, SQLite/FTS5 through bundled rusqlite, versioned bounded NDJSON over private native local streams, and one on-demand writer. Exact build inputs live in lockfiles and the toolchain manifest. These choices have native Mac evidence; they are not a production or all-platform qualification. No Node.js, JavaScript compilation pipeline or WebEngine was introduced.

**Observed correction:** the rrule 0.14.0 candidate shifted a nonexistent DST wall time and consumed COUNT. The experiment retains a narrowly patched, licensed copy with independently expected gap/overlap tests; see its patch record. Search measurement also exposed repeated FTS scans during page hydration; the corrected page query has retained before/after measurements. Neither finding is concealed by the original recommendation.

**Checkpoint reporting:** preserve source, tests, source fixtures, build/CI configuration and evidence in Git. Keep SDKs, binaries, disposable profiles and credentials out. A checkpoint can be in progress; mark only observed results as passing. Continue the full goal across remaining packaging, native CI, recovery and integration gates. No full Google-first area is Verified yet.

Affected: Core revision 6, CLI 2, Mail UI 6, Calendar 6, Gmail 2, Search 1, Triage 3, Local assistance 3, Platform 5 and Quality 4, plus status, plans and development/protocol/evidence documentation. Prior entries remain unchanged.


## 2026-09-11T22:45:57Z — D013: completed synthetic foundation and next handoff

**Outcome: the founder's complete Phase 0 goal is fulfilled on available hosts.** Runtime checkpoint `204fa62` passes eight native CTests and relocated development-package checks on macOS 15 ARM64, Windows Server 2022 x64 and Ubuntu 24.04 x64. The Mac desktop walkthrough and actual old/new packaged schema 2→4 migration, preserved drafts/content/receipts and safe old-engine refusal also pass. [Execution evidence](research/phase-0-execution.md) retains exact sources, failed attempts, corrections, logs, artifact identities and limits.

The experiment now integrates provider/calendar/assistance fakes with the same durable Core store. Native CI found and drove fixes for Linux ICU packaging/selection, Windows discovery ownership, and bounded client recovery while a prior writer closes. The final application retains one writer, explicit GUI/CLI lifetimes, private IPC, literal human actions and the model-free manual workflow.

**Qualification remains scoped.** This does not lock production architecture, qualify physical other-host input/keyrings, connect a live account/model, establish complete offline storage at large scale, or complete trusted distribution. Whole Google-first readiness remains 0/9; existing decisions about designs, Done/Delayed, settings, real-data policy and deferred legacy protocols are unchanged. Phase 1 is the next local workflow refinement, not automatically authorized live-provider work.

Affected: status, root/agent guidance, Phase 0/development plan and execution evidence. Core 8, Calendar 7, Local assistance 4, Triage 4, Platform 7, Gmail 3, Mail UI 7, CLI 3, Search 1 and Quality 4 retain their scoped contracts and Draft whole-area status. Development handoff uses the same Git branch and native scripts; profiles, SDKs and credentials stay local.
