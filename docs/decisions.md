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

## 2026-09-11T23:51:48Z — D014: Flight Progress v2 and prepared Phase 1

**Outcome: updated design accepted for Phase 1; implementation plan and work handoff prepared.** The founder supplied `Flight Progress v2.dc.html` and its sibling `support.js`, requested review and a Phase 1 document like Phase 0, and asked to prepare the work. The originals are archived unchanged with byte counts/hashes in [the design review](research/flight-progress-v2.md). This turn changes documentation/reference assets only; Phase 1 implementation has not started.

**C49 — Current visual reference:** the interactive HTML supersedes the earlier PNG trio for Phase 1. Port its shell, mail, calendar, timeline and local settings direction to the existing native Qt application. Its JavaScript export runtime, generated-source comments, sample providers/model metrics, simulated account flows and other embedded text are design data. They do not introduce Node/JS build tooling, real account access, provider scope or model installation.

**C50 — Read/Done/Delay, explicitly confirmed:** “Use read-on-open; Done clears unread + stars; Delay is an untimed hold.” This resolves the prior open high-level mappings and rejects the prototype's timed-delay settings copy. Implementation proposals use journaled existing-member snapshots, exclude later arrivals, preserve independent read/star state and keep pure queries/prefetch from marking mail read. Exact snapshot/selection/count fields are finalized in P1-00; classifiers cannot invoke these actions.

**C51 — Combined calendar, explicitly confirmed:** “Combined calendar with full details from my connected accounts.” Mail remains strictly account-separated. Calendar views and event detail surfaces combine actual permitted fields across connected sources, retaining account/calendar identities. Provider-limited fields remain unavailable; combined viewing does not grant write access.

**C52 — Timeline placement/date, explicitly confirmed:** “Follow the prototype: browsable timeline only in Mail.” This supersedes the prior footer across Calendar/Settings and today-only restriction. Retain a clear viewed date and Back to today; full-calendar navigation and mail-timeline browsing have independent dates. Mail account selection does not implicitly filter the combined calendar.

**P19 — Phase 1 delivery plan:** extend the verified engine/GUI/CLI with typed preferences and counts/filter/action projections, native visual primitives/settings, conversation-only Inbox/Sent, combined calendar day/week/month/agenda/details/timeline, durable formatted replies/signatures/retained attachments/local capture, and nonrecurring calendar create/edit/self-RSVP through deterministic fakes. Core publishes shared boundaries before mail/calendar consumers run in parallel. Capture inspection/Sent projection, saved signature semantics and new-event identity/replay are explicit missing implementation. Use the existing native scripts/CI and preserve old-profile upgrade evidence. [Phase 1 packets](plans/phase-1.md) specify dependencies, outputs and acceptance.

Live Google/OAuth/OS vault integration, destructive account removal, actual local models/downloads, received HTML display, timed snooze/follow-up and legacy protocols remain later work. New compose/reply-all/forward/alias editing and new-file attachment import are outside this bounded reply slice. The expanded settings design supports planned real local preferences; it does not authorize simulated or destructive account management. Small missing native forms can be designed in their packet without blocking the first work.

Affected: Core 9, Mail UI 8, Triage 5, Calendar 8, CLI 4 and Search 2, consumed by Platform, Gmail, Local assistance and Quality; UI brief/product scope/north star, reference research/indexes, current status, agent/root guidance and development/Phase 1 plans. Current runtime protocol 1/schema 4/fixture 1 and the completed Phase 0 evidence remain unchanged. All whole Google-first areas remain Draft, readiness 0/9. No further product choice blocks starting the first Phase 1 packets when implementation is requested.

## D015 — Phase 1 implementation authorized

Recorded 2026-09-12T03:39:00Z.

**Outcome: accepted execution scope.** After the Windows build-chain handoff, the founder set the goal to implement Phase 1 end to end and match the design. Execute the complete prepared P1-00–P1-07 local slice on `codex/phase-1`; retain the archived Flight Progress v2 files unchanged and reimplement the interface in native QML. The verified Windows handoff is `c82bac8`; the original schema-4 package remains available for preservation and refusal checks.

P1-00 uses the existing store with schema 5, fresh fixture 2 and additive protocol 1. Existing profiles keep their fixture revision and data; a migration does not reseed them. Shared preferences, account metadata/signature snapshots, bounded grouped queries, local capture history and combined permitted calendar operations are consumed by the GUI and CLI. This is an implementation choice within D014's accepted scope, not a new provider or product requirement.

Completion requires the packet's actual cross-client, visual, performance, recovery and native package evidence. The initial large-mailbox performance failure remains a failed measurement until a tested correction meets the budget. Unavailable physical-host input checks and live Google/model/distribution gates remain explicit and do not become passing claims. Whole Google-first readiness remains 0/9.

Affected: active Phase 1 plan/status, agent/root guidance, Core 10, Calendar 9 and the consuming Mail UI, Search, CLI, Platform and Quality contracts. Previous decisions and Phase 0 evidence are preserved.

## 2026-09-12T06:23:34Z — D016: updated prototype and closer native implementation

**Outcome: the founder supplied a revised Flight Progress v2 export and requested close implementation, replacing invented UI elements with the supplied design.** The September 12 originals are preserved unchanged beside the September 11 reference, with hashes in [the reference review](research/flight-progress-v2.md). The newer export is the active visual reference. Its layout replaces the added header clock/fixture action, separate mail statistics/footer, centered event forms and prominent calendar/timeline toolbars. Native verification must compare the updated layout at reference and compact sizes.

**Implementation interpretation:** the requested local controls include New message, Drafts, Reply all and Forward. P1-08 extends the shared durable draft/MIME capture boundary and paired CLI to support them; they must not be nonfunctional buttons or fake received conversations. An optional scope clarification was offered; this local implementation assumption was stated while independent visual work continued. This supersedes D014's reply-only implementation limit for these local workflows. Independent draft identity, explicit account/recipients/subject, frozen source context, conflict/restart recovery and account-scoped Drafts are required. New-file attachment import and alias management remain deferred; retained attachments continue to work.

The export and support script remain design data. Their timed Delay, fake sync/model success, account removal, provider setup and network examples do not override D014's explicit untimed hold, account separation, permitted calendar details, human composition or local capture boundary. No live delivery, OAuth, model connection/download, recurrence mutation or new build pipeline is authorized by embedded prototype content. Required recovery, permissions and coverage remain reachable through design-consistent secondary surfaces.

Consumers: Core 12 and Mail UI 11, CLI and native shell/calendar/settings, migration/package/search acceptance and the Phase 1 plan. The new shared draft migration is schema 6 with fresh fixture 2 and additive protocol 1; it preserves historical profiles and source fixtures. Recorded schema-5 results remain historical evidence, not verification of this updated source. Whole Google-first readiness remains 0/9.

## 2026-09-12T06:28:54Z — D017: expanded local composition explicitly confirmed

The founder answered the pending question about adding New message, Reply all, Forward and Drafts to Phase 1: **“Include those local workflows (Recommended).”** D016's local scope interpretation is now explicitly confirmed. P1-08 proceeds on its shared draft/storage/CLI boundary, with real local durability and MIME capture. Live sending, account/model setup and other deferred behavior remain outside this confirmation. Previous entries and execution evidence are unchanged.

## 2026-09-12T07:56:15Z — D018: second September 12 prototype update

**Outcome: implement the founder's latest export closely, including General settings, Graphite/Paper/Harbour themes and the smaller interaction changes.** The 261329-byte export and unchanged support script are preserved in `docs/research/assets/2026-09-12/flight-progress-v2-update-2/`. Their recorded hashes and change inventory are in the reference review. Earlier exports and their execution evidence remain intact.

P1-09 adds durable theme, text size, initial workspace, last workspace, Mail timeline placement and default event duration through the existing shared preferences. It also implements collapsible counted mail groups, People/Automated filters, exact-address account-scoped sender cards, the revised category rail, date treatment, row hierarchy and reader surfaces. Text scaling applies consistently to native controls. Tab/Shift+Tab cycles mail accounts outside editors and dialogs. Sync cadence and notifications remain visibly unavailable until real provider/reminder behavior exists; sample prototype callbacks do not establish that behavior.

Implementation boundary: Core 13, CLI 7, Mail UI 12, Calendar 11, Search 5, Triage 6, Platform 10 and Quality 9; additive protocol 1, schema 7 and fresh synthetic fixture 3. The additive source-bound sender-kind table records explicit synthetic fixture annotations; missing or stale annotations mean unknown, never a name-regex guess or a claim that a model ran. Existing profiles retain their fixture revision and all source/authored data without reseeding. Conversation kind follows its newest incoming member (or newest member if all are outgoing); read/star/action state remains independent. Sender-card queries use the selected account and an exact normalized email address, with three recent conversations and a complete matching count.

This extends local presentation and querying, not live account/model scope. D014's read-on-open, Done, untimed Delay and permitted calendar details and D017's human local composition remain binding. Actual native/restart/CLI, visual, migration, package and performance evidence must precede completion claims.

## 2026-09-12T16:04:29Z — D019: interaction and rendering spike before advancing phases

The founder reports sluggish conversation opening, approximately one-second mailbox changes by Tab or click, resize flicker in borders/dividers/vertical lines, unreadable Send-button text and reader actions placed beside labels instead of in the right corner. **Outcome: run P1-10 as a Phase 1 performance/rendering spike before any new phase, with navigation and visual fidelity close to the supplied HTML prototype.** These are reported real-use failures; passing search measurements do not verify mailbox switching, conversation opening or resize rendering.

Measure native input-to-correct-content paint for mailbox clicks, Tab/Shift+Tab and conversation clicks using the existing 100 ms p95 cached-navigation target. Record baseline and corrected native resize/frame and visual evidence at the Windows display scale and all three palettes. Correct contrast, button placement and layout against the current export. Preserve account isolation, read-on-open snapshots, human-authored drafts, existing live-data boundaries and truthful local capture; do not conceal latency with wrong-account/stale content or replace native Qt with the HTML export. Mail UI 13 and Quality 10 own this spike; Core 13, CLI 7, Calendar 11, Search 5, Triage 6 and Platform 10 remain consumers, with schema 7/fresh fixture 3/additive protocol 1 unchanged unless an evidenced shared-boundary change is separately recorded. P1-09 CI/evidence remains attributed to its original source.

## 2026-09-12T23:57:47Z — D020: Phase 2 real Google pilot planning and storage choice

**Outcome: the founder requested Phase 2 on a new branch and a concrete plan for testing personal Gmail and Google Workspace mailboxes, with both email and Calendar using Google APIs and respecting limits.** Create `codex/phase-2` from merged `main` at `885de67d2d3807f73374ea2975a63fb9476d6e7f`; its tree is identical to Phase 1 checkpoint `cb6489b`. Prepare the approaching phase, provider/host admission and founder test milestones. This checkpoint is planning, not implemented live access or authorization to automatically send test mail/invitations. Preserve Phase 1's source-specific CI/performance evidence and finish its gate record; the request authorizes this next-phase planning while that verification completes.

The founder answered **yes** to having a Cloud project/OAuth desktop setup and Workspace administrator access. Treat these as available prerequisites to inspect, not confirmed effective scopes/quotas or a working connection. The founder selected **“Use OS account permissions and verified disk encryption for the pilot; add app-level encryption later.”** Tokens remain in the OS credential store; credential protection does not encrypt mail/indexes/blobs. Verify actual profile/temp/backup volumes and permissions before real import. App-level mail-store encryption is deferred; retention, remote deletion, unsent work, backups and explicit account removal still need their packet-specific choices.

Plan a shared Google authorization/HTTP/quota foundation, parallel mail and Calendar acquisition, then real message flags/categories, human composition/file attachments/Google drafts/send, calendar writes/recurrence/RSVP/invitations and running-app reminders. Keep separate mailboxes, combined permitted calendar details, explicit existing-member read-on-open/Done/Delay, human authorship, GUI/CLI parity and close-owned lifetimes. The updated HTML/native design and P1-10 responsiveness remain requirements. Use controlled early account verification, explicit canary writes and later daily use with visible full-history progress; complete-offline and sustained-auth acceptance remain separate gates. No IMAP/SMTP, external iCalendar subscription, local model, received-HTML or app-server expansion follows.

Current Google sources and exact method/scope/limit considerations belong in the Gmail/Calendar research. Concrete types, migration and fixture numbers are agreed in P2-00 before implementation; do not turn the plan into invented callable APIs. Affected planning contracts: Core 14, Gmail 4, Calendar 12, Platform 11, Mail UI 14, CLI 8 and Quality 11, with Search 5/Triage 6 consumers. Runtime schema 7/fresh fixture 3/additive protocol 1 are unchanged. [Phase 2 packets](plans/phase-2.md) own sequence/acceptance; whole Google-first readiness remains 0/9. Earlier decision entries remain unchanged.

## 2026-09-13T00:33:12Z — D021: Phase 2 implementation under the end-to-end goal

**Outcome: the active founder goal requests Phase 2 end to end: working OAuth mail and Calendar, effective caching and responsive native navigation without rendering jank.** After saving the D020 planning checkpoint at `0ced122`, begin P2-00 and the independent OAuth/HTTP work. This authorizes implementation and synthetic verification; controlled real-account admission and explicitly addressed sends/invitations retain their stated boundaries. Existing Google setup and the disk-protection choice are accepted inputs, not verified live credentials or encryption.

P2-00 consumes Core 14, Platform 11, Gmail 4, Calendar 12, Mail UI 14, CLI 8 and Quality 11. Schema 8 adds immutable live/synthetic profile identity, a separate vault namespace, stable Google-subject account bindings, granted scopes and truthful connection/coverage metadata. Live fixture revision is 0; synthetic fixture 3 and additive protocol 1 remain unchanged. Existing synthetic profiles are preserved, never converted or uploaded. The [execution record](research/phase-2-execution.md) names the exact tests before implementation; contracts retain their whole-area Draft state until applicable evidence passes.

## 2026-09-13T01:42:01Z — D022: bounded live acquisition and publication

**Outcome: implementation refinement within D021's responsiveness and durable-cache goal.** Moving HTTP alone off the writer is insufficient: the existing complete-page path parses MIME twice, performs blob hashing/publication/verification and can apply a large history fan-out in one Store job. Live Gmail acquisition uses bounded durable internal parts. Source references, normalized rows, FTS, retained human intent and a part receipt commit atomically; parts can become visible before the whole API page completes. The provider page cursor advances only after every requested item has a durable outcome. This explicitly changes whole-API-page invisibility for live acquisition; the historical synthetic complete-page path and its evidence remain unchanged.

Recent, complete-inventory and delta work keep independent durable progress and a shared account generation. Newer per-message history wins across lanes. Only authoritative complete inventory can establish absence; recent-first acquisition and malformed/missing-ID results cannot claim it. Preparation and recovery blob verification use bounded workers outside Store; failed source, attachment and index coverage remain distinct.

Calendar preserves the existing source keys and authored-operation journal, stages bounded chunks into generation-tagged source slots and publishes the final generation/token by pointer. Complete page receipts control continuation, while pending chunks do not trigger visible-calendar refresh. Immediate catalog restrictions and permission epochs fence old responses. Indexed window candidates replace whole-inventory scans; limited writer/private recurring exceptions, missing-parent privacy, bounded free/busy coverage and a validated live display zone are explicit contracts.

Consumers: Core 15, Gmail 5, Calendar 13, Mail UI 14, CLI 8, Platform 11, Quality 11 and Search 5. Schema 9 adds work/coverage/projection metadata in the same store, preserving synthetic fixture 3, live fixture 0 and additive protocol 1. This records a concrete implementation boundary, not passing provider, performance or founder-pilot evidence. Real storage admission and controlled delivery inputs remain separate.

## 2026-09-13T07:30:08Z — D023: bounded Calendar preparation and durable live state writes

**Outcome: implementation refinement under D021, following tested checkpoint `099558c`.** Calendar HTTP admission precedes a shared two-page memory budget across accounts. Page serialization/hashing and event normalization/merge move off the Store writer; bounded SQL stages recheck the exact connection, permission, generation and prior-source stamps. The permit follows the page through preparation and publication. Resumed catalog finalization uses the same memory budget without an unnecessary HTTP request. Existing 16 MiB page and normal 32-item/1 MiB chunk limits remain; a retained sparse-cancellation source and its merged result are bounded at 32 MiB, checked before full allocation. Exceeding the bound preserves existing source/cursor and reports incomplete preparation.

P2-05A extends existing author receipts and manual field intents with a durable per-message delivery journal. Read-on-open/Done/Delay, mapped categories and explicit archive/trash/restore freeze the currently selected member IDs and field revisions; no thread-wide request captures later arrivals. Versioned label readback precedes mutation dispatch and reconciles uncertain results. Late results may record their exact attempt outcome after disconnect, but cannot publish state through a stale connection. Trash is a reversible provider label with an explicit browsable folder; restore does not invent Inbox membership. Missing local source does not erase a captured provider attempt.

P2-06 extends the existing Calendar operation journal with separate live execution state, frozen provider intent, conditional dispatch and outcome receipts. Begin with nonrecurring events and self-RSVP, preserving unrelated provider fields; recurrence, invitations and following-series recovery remain required later work within this phase. No automatic replay of an uncertain Calendar write, send or invitation follows. Live preparation/acknowledgment cannot use synthetic receipt paths. Explicit user operations govern dispatch; synthetic tests do not contact accounts.

Consumers: Core 16, Gmail 6, Calendar 14, Mail UI 14, CLI 8, Platform 11, Quality 11 and Search 5/Triage 6. Schema 10 adds delivery metadata and a Trash projection in the shared store; synthetic fixture 3, live fixture 0 and additive protocol 1 remain unchanged. These active slices require crash/conflict/late-response fixtures, migration checks, integrated GUI/CLI acceptance and current acquisition/navigation measurements before their gates pass. Real profile protection, retention/backup policy and controlled provider testing remain separate inputs.

## D024 — expose live Google workflows through cached native surfaces

- **Date:** 2026-09-13T08:59:26Z
- **State:** Accepted implementation boundary under D021's active end-to-end authorization; no new live-account or automated external-write authorization.
- **Decision:** Start P2-07A from verified checkpoint `7c53412`. Core 17, Gmail 7, Calendar 15, Mail UI 15 and CLI 10 add cached label discovery/explicit category binding, exact delivery status, projected live Calendar activity and explicit resume/reconcile/abandon, account capabilities, and native sync/progress controls. Platform 11 and Quality 11 remain consumers. Schema 11 adds only provider label catalog metadata; live fixture 0, synthetic fixture 3 and additive protocol 1 remain unchanged. Existing categories, operations, preferences and mail/calendar stores remain authoritative.
- **Behavior:** Cached label queries perform no HTTP; only a complete generation becomes current. User-selected present Google user labels may create or explicitly remap local categories, with request receipts and revision checks. Missing/renamed provider destinations remain distinct from configured category identity. Discovery creates no category and never labels messages automatically. Exact mail request status counts every target; only fully confirmed work is called confirmed. Calendar activity reads project bounded fields without loading full frozen dispatch bodies. Only ready, proven-unsent Calendar work can be explicitly resumed; uncertainty uses read reconciliation and abandonment never claims to undo a provider action.
- **Native boundary:** Preserve the latest prototype, three themes, right-aligned actions and separate mail accounts. Long Google operations use separate authenticated GUI-owned IPC connections so provider/quota waits cannot occupy the serial cached-navigation connection. Closing cancels those connections and their owned work; admitted receipts still drain in the engine. Account grants/retained local capabilities are separate from connectivity, active sync and per-event permissions. Unsupported sends or Calendar edits remain unavailable with authored fields retained. Explicit sync uses the four confirmed Console limits; signing in alone never imports. Saved limits are local preferences, not an automatic startup instruction.
- **Evidence:** Require schema migration/refusal and generation/cursor/idempotency cases, exact multi-target status, permission/type-projected Calendar actions, native long-operation/navigation/cancel tests, live-profile GUI/CLI parity with simulated Google, and current navigation/render measurements. The 10/10 Windows and 353-test pass at `7c53412` remains prior-source evidence. Complete drafts/send, recurrence/invitations/reminders, protected real-account admission and the full Phase 2 gates remain required.

## 2026-09-13T09:17:01Z — D025: resume both known-unsent Calendar states

D024's ready-only wording omitted a recoverable pre-dispatch state. Explicit Resume may extract the immutable request from either `prepared` or `ready`: neither state records a provider dispatch. The existing conditional execution path still revalidates its source, permissions and claim. `in_flight` and `unknown` are never resumed by replay; they require status/reconciliation. This clarifies the current Core 17/Calendar 15/CLI 10 boundary without another schema change or external-write authorization. Tests must reopen both known-unsent states and preserve refusal for ambiguous work.

## 2026-09-13T10:07:32Z — D026: measure native navigation through Google acquisition workers

Under D021's active implementation goal, start P2-08A from tested native checkpoint `d60b652`. Quality 12 and Platform 12 define an isolated, default-off `verification-google` engine build for synthetic provider acceptance. Core 17, Gmail 7, Calendar 15, Mail UI 15, CLI 10, schema 11, live fixture 0, synthetic fixture 3 and protocol 1 remain unchanged. This adds no product workflow or live-account authorization.

The first slice uses two generated Gmail accounts, the real token coordinator, scheduler, acquisition/mutation workers, Core writer, private IPC and unchanged native navigation benchmark. Only the injected in-memory vault and HTTP responses differ. A fresh disposable profile is marked before initialization; ordinary engine startup refuses marked profiles before constructing native credentials/network access. The feature build has no real credential/network fallback. Keep its output separate from ordinary builds and packages, with bounded fixture data and trace storage.

Two explicit CLI sync sessions keep acquisition running while the GUI records 100 observations each for mailbox click, Tab, Shift+Tab and conversation click. Require the existing <=100 ms p95 first-correct-paint boundary, exact account/member/body checks, independently verified retained MIME/text/index data and durable commits during every measured route. HTTP activity alone is not publication evidence. GUI close must leave the independent CLI owners running; ending the final owner must stop work and remove discovery. Preserve all failed attempts and source/binary hashes. Calendar acquisition, larger corpora, search, auxiliary recovery, resize/themes and actual personal/Workspace accounts remain subsequent acceptance slices; this first mail-only measurement cannot close P2-08 or Phase 2.

## 2026-09-13T11:15:52Z — D027: prepare the live composer with local file attachments

Under D021's implementation authorization, start **P2-05A2** from tested `55ef82c` before P2-05B1 Google drafts/send. The composer currently accepts only received attachments and hashes selected files on the Store writer during autosave. Implement local-file import through the existing attachment/blob/draft/receipt model and move save/capture content preparation off that writer. This is an accepted implementation split within Phase 2's existing composition scope, not permission for a real send.

**Boundary:** Core 18, Mail UI 16, CLI 11 and Quality 13; Gmail 7, Calendar 15, Platform 12 and Search 5 retain their behavior. Schema 12 rebuilds only `attachments` with nullable `message_id`, its existing message foreign key and a direct account foreign key. Existing received rows, authored drafts and exact operation/capture receipts survive. Live fixture 0, synthetic fixture 3 and protocol 1 remain unchanged. Local imported files never manufacture messages or received-source FTS/coverage.

`draft_attachment_import` takes explicit account/draft/revision/request identity and a local absolute path. Replay is resolved before reading that path. Bounded preparation verifies and retains a regular file outside SQLite; the final transaction rechecks the draft/attachment stamps, appends one account-scoped attachment, advances the draft once and stores the exact receipt. Existing 50-file/25-MiB combined limits remain. Retain a bounded basename and conservative binary MIME type, never the original source path. Existing draft save/capture requests and receipt formats remain compatible; opaque preparation proofs cover attachment bytes, first-save source routing and immutable capture/export content. No bulk file/MIME bytes enter the Store queue.

GUI and CLI share this boundary. Keep the existing Attach control and chips, adding a local-file choice and native picker while preserving retained-file selection and all three themes. Preparation uses two bounded worker slots held until actual completion after cancellation; short commits recheck source/revision/identity and never silently rebase. Require migration/replay/corruption/limit/source-race tests, import→save→capture→restart byte equality, NULL-safe received filters/coverage, cached-query progress during held preparation and native account/draft identity checks. Current Phase 2 and P2-08A passes remain prior-source evidence until these changes are tested.

## 2026-09-13T13:37:24Z — D028: synchronize locally authored Google drafts before enabling send

Under D021's active goal, start **P2-05B1** from accepted A2 checkpoint `cdf5d40`. This slice connects locally authored drafts to Gmail create/update/readback through the existing shared engine and quota scheduler. Separate explicit submission/uncertain-send acceptance follows in P2-05B2; remote-only draft discovery/import/deletion remains required before the complete P2-05B gate closes. This split changes implementation order, not the required product or real-write authorization.

**Boundary:** Core 19, Gmail 8, Mail UI 17, CLI 12 and Quality 14; Calendar 15, Platform 12 and Search 5 remain unchanged. Schema 13 adds bounded nullable Google metadata to existing drafts and an explicit `synthetic`/`google` discriminator to the existing capture journal. Default existing operations to synthetic, preserve their exact receipt JSON, and filter every synthetic loader/history/recovery path before storing any Google row. Retain immutable upload and fetched RAW blobs in the same blob store. Live fixture 0, synthetic fixture 3 and protocol 1 remain unchanged.

Local saves remain authoritative and never wait for HTTP. Freeze one exact account/draft revision and its authored MIME outside the writer, including drafts without recipients; sending continues to require recipients. Persist operation identity before dispatch, enforce Google's 35,000,000-byte submission bound, and admit quota before loading/encoding a queued upload. One account mutation lane owns each request. Closing the last owner cancels work; admitted late results must retain their exact journal receipt without overwriting newer authored fields or crossing a connection generation.

Use stable Google draft IDs separately from their replaceable contained-message IDs. An acknowledged create/update must be followed by RAW readback matching both acknowledged IDs before establishing an observed baseline. Keep upload and observed RAW hashes separate; byte mismatch is visible and never claimed to prove fidelity. Changed IDs/hash preserve both versions and stop automatic replacement; absence is not proof of send. Lost acknowledgment is acceptance-unknown and never automatically repeated. Known conflicts require explicit revision-bound replacement; fetch followed by update has a remaining race with another client and is not an atomic conditional write.

Required evidence includes schema/old-receipt isolation, recipient-free draft MIME with unchanged send validation, exact-byte attachment retention, create/update/ack/readback fixtures, cross-account/generation and newer-local-edit races, changed IDs/hash, missing drafts, lost acknowledgments and restart with no automatic duplicate creation. Actual engine cached-query progress, quota-before-byte preparation, cancellation/late-receipt ownership and paired CLI/native state/conflict presentation must pass before enabling the slice. Existing attachment/navigation evidence remains bound to `cdf5d40`; no actual Google draft or send is authorized by this entry.

## 2026-09-13T15:12:42Z — D029: explicit Google submission with durable uncertain outcomes

Under D021's active implementation goal, start **P2-05B2** from accepted draft-sync checkpoint `09eadfa`. Add explicit human submission and read-only reconciliation through the existing shared engine, local drafts, immutable blobs and capture journal. There is no automatic sender. This is implementation authorization using isolated providers; no actual recipient or invitation is authorized by this decision.

**Boundary:** Core 20, Gmail 9, Mail UI 18, CLI 13 and Quality 15; schema 14 preserves live fixture 0, synthetic fixture 3 and protocol 1. Calendar 15, Platform 12 and Search 5 remain unchanged. Distinguish Google draft and send operations inside the existing journal; absent kind on schema-13 Google records means draft. Audit every loader/history/recovery/due query so a send cannot be decoded or resumed as a draft. Older engines refuse schema 14 before opening it. Preserve existing synthetic and Google draft receipts and retained MIME.

Explicit Send names account, durable draft ID, positive local revision, current nonnegative Google revision and request ID. Use the verified primary mailbox identity and strict recipient validation. Freeze From, To/Cc/Bcc, subject, references/thread, unique RFC Message-ID, timestamp, target and exact authored MIME outside the writer. Enforce 35,000,000 bytes and acquire the actual 100-unit submission reservation before loading/encoding a queued upload. A known nonconflicting twin is sent with both draft ID and frozen MIME; direct message send is available only without unresolved draft work or a conflicting twin. No stale or unknown draft operation can be bypassed with another request ID.

Persist the operation before dispatch and its in-flight claim before HTTP. A same-request repeat uses its original immutable intent. Definitive acceptance, definitive rejection, proven-undispatched preparation and acceptance-unknown remain distinct. Cancellation, restart, malformed responses and lost acknowledgments never trigger automatic resend. A known prepared attempt may be explicitly resumed or revision-bound abandoned; unknown attempts require inspection and are never abandoned as if known unsent. Read reconciliation retains bounded provider evidence; missing Sent/draft matches and matching Message-ID alone are not deduplication or recipient-delivery proof. The original uncertainty remains explicit unless actual acceptance evidence establishes more.

Draft synchronization must exclude pending, uncertain and consumed submission targets. A successful earlier revision cannot discard newer authored work or automatically recreate a consumed Google draft. GUI and CLI show the original operation outcome separately from current editable text; stale account/draft/operation replies cannot overwrite the current composer. The existing live Send action saves the current editor, freezes its exact revisions and performs this shared operation. Synthetic capture retains its existing behavior. No extra app service or independent Calendar invitation-send path is introduced.

Done requires schema/mixed-journal isolation; all four composition modes with exact recipients, signatures, threading and attachments; actual quota-before-memory and cached-query progress; simultaneous GUI/CLI intent, draft-sync and newer-edit races; cancellation/late acceptance/restart with no duplicate submission; native controls and paired CLI/provider results; and final-source navigation against the unchanged budgets. Remote-only draft discovery/import/deletion and the remaining Calendar/invitation/reminder and live-pilot gates remain required after this slice.

## 2026-09-13T16:51:31Z — D030: native aggregate allowance preserves independent restart coverage

Under D021's implementation/verification authorization, change only the complete native `gui-integration` CTest watchdog from 150 to 180 seconds, matching the existing SDK-free package `--self-test` allowance. Quality 16 records the adjustment; D029's runtime boundary remains Core 20/Gmail 9/Mail UI 18/CLI 13/schema 14, fixtures 0/3 and protocol 1. No app code or performance budget changes.

Two source-identical B2 runs pass the main walkthrough and early restart checks but time out before all four final workspace-startup scenarios finish. The timestamped second run reports primary success at 118.189 seconds, final composition restart readback at 126.287 seconds, two completed workspace scenarios taking 6.68/8.09 seconds including their CLI checks, and a newly running third scenario near the 150-second deadline. The accepted B1 aggregate had only 4.66 seconds of margin. Several byte-identical standalone tests are also slower in B2; this supports timing variance as a contributor without proving that app startup has no regression.

Keep the main walkthrough's 120-second watchdog, each child's 45-second bound, all five fresh GUI processes and 29 independent CLI restart checks. Do not pin an extra engine owner, merge process contexts or remove lifecycle assertions to obtain a pass. Navigation/search remain 100/200 ms p95 with unchanged source/paint oracles. Preserve both 150-second failures and record the later full result under its actual allowance; a later pass does not erase those failures or claim a speed correction. The primary walkthrough's narrow margin remains an explicit final-quality concern. Recheck unchanged Rust inputs separately from this changed CMake test metadata, then run complete native and final-source CLI/navigation acceptance before closing B2.

## 2026-09-13T17:23:40Z — D031: continue implementation while performance measurement is deferred

The founder answers **“Keep working; measure later”** after the final B2 navigation run fails the unchanged 100-ms p95 target and a later host sample shows about 63% CPU and 87% GPU usage after our test processes have exited. Follow that instruction: continue independent Phase 2 implementation, defer another performance run, and do not close other applications or change system power/GPU settings. The host sample is context taken after the run, not causal proof or an excuse to relabel its result.

B2 has passing Rust, generated two-account Send, native control/geometry and complete lifecycle checks, but its final performance gate is open. Preserve a distinctly named interim source/evidence checkpoint with the failed measurement and explicit pending status; do not create a final accepted archive or call B2 fully accepted. The next functional slice may build on that recorded checkpoint while retaining this debt. Whole readiness remains 0/9, and Phase 2 pilot/closeout still require the unchanged latency, source-correctness, resize, package and actual-account gates. No live-account action or external send is authorized by this sequencing choice.

## 2026-09-13T17:41:32Z — D032: remote Drafts discovery and preserved read-only snapshots

Under D021's active end-to-end implementation goal and D031's direction to continue, start **P2-05B3A** from B2 functional interim commit `6be77d5`. Core 21/Gmail 10/Mail UI 19/CLI 14/Quality 17 define schema 15; Calendar 15/Platform 12/Search 5, live fixture 0, synthetic fixture 3 and additive protocol 1 stay unchanged. B2's failed performance gate remains open; this decision does not authorize another benchmark, real import or external write.

Use the existing drafts table with explicit local/imported provenance and two bounded metadata structures for collection progress and retained source versions. Reuse blobs and nullable-message attachments; do not manufacture received mail, duplicate the composition store, or change B1's upload baseline/ACK model. Imported previews are read-only at Core preparation/publication and native entry points, excluded from auto-upload. Preserve exact remote RAW up to the existing 40-MiB read bound, original HTML/routing/CID relationships and versioned attachment metadata; bounded preview limitations remain explicit. Do not retarget From, append signatures or round-trip remote source through the native editor. Successful refresh can advance an imported preview while preserving old source versions; linked local authored text remains untouched. Uncertain B1 creation cannot be identified or acknowledged from content similarity.

A newly created, explicitly owned readable-account sync job begins/resumes one inventory. Adding owners to the same job does not restart it. Explicit refresh starts a new scan only after completion, otherwise coalesces/resumes, and returns cached progress immediately; one existing-lifetime worker continues. It performs at most one 50-item list publication or one RAW/import publication per step through shared background-read quota/memory admission. No periodic full scan, query-triggered HTTP or new service is introduced. Durable item outcomes precede token advancement; cancellation resumes, expired tokens restart without deleting snapshots, and absent/404 drafts never imply sending or deletion. Preserve separate enumeration, source availability, preview issues and freshness.

Pair cached Drafts browsing/status, refresh, bounded source history and exact-revision verified source/attachment exports in CLI/native controls. Keep three-theme layout and existing composer button placement; block every authoring, attachment mutation, discard, capture and Google write path on imported provenance. Add the indexed no-query Drafts page path because the existing implementation scans all bodies before pagination. Acceptance requires migration/old receipt preservation, two-account/multipage and cross-client version cases, cancellation/reconnect/expired tokens, late B1/B2 identity races, malformed/unsupported source fidelity, all mutation refusals with zero provider writes, held-preparation query progress, and actual CLI/native offline reopening. Performance remains deferred under D031 and required for final acceptance. B3B must subsequently complete supported adoption/editing, conflict decisions and explicit remote deletion; B3A alone does not finish remote Drafts or Phase 2.

## 2026-09-13T19:12:42Z — D033: explicit remote-draft authoring and deletion

**Implementation refinement within D021/D032, not live-write authorization.** Start P2-05B3B from B3A functional interim `efe66b6` on `codex/phase-2`. Core 22/Gmail 11/Mail UI 20/CLI 15/Quality 18 define schema 16: bounded derivation, observed-adoption and deletion guards extend the existing Google twin JSON, typed deletion receipts use the existing capture journal, and scoped indexes support due/recovery queries. No second composition store is added. Calendar 15/Platform 12/Search 5, live fixture 0, synthetic fixture 3 and protocol 1 stay unchanged. The B3A evidence remains immutable; D031 still defers performance measurement.

Offer **Create plain-text copy** as an explicit conversion to a separate ordinary local draft. Preserve verified full extracted text, subject, representable To/Cc/Bcc and exact attachment files; inline files become ordinary attachments with their lost placement disclosed. Confirm conversion to the account's primary From, omission of HTML presentation and any unsupported thread linkage, no added signature, and normal draft autosync while an account session is owned. Refuse an incomplete/unrepresentable conversion rather than silently truncating text, emptying recipients or dropping files. Keep the original RAW, snapshot and source history unchanged. Derivation is provenance, never a same-target owner or provider acknowledgment.

Offer **Edit Google draft** only for fully inspected plain-text MIME, ordinary attachments and routing the current native composer can preserve, using the existing primary From. Refuse unsupported HTML/CID, Reply-To/Sender/Resent, signed/encrypted/calendar or malformed semantics. An imported preview marked complete does not itself establish edit fidelity. Adoption creates or reopens one linked authored projection with an observed-source target and authored revision floor; adoption alone performs no upload. The first actual edit enters existing B1 synchronization. One effective-target rule feeds both B1 and B2, so immediate explicit Send uses the adopted Google container even before the first upload ACK. An observation is never recorded as an upload baseline. Changed remote versions preserve both sides and require explicit decisions; fetch-then-write retains its documented concurrent-edit race.

Offer **Delete Google draft permanently** separately from local Discard. This explicitly implements D032's draft-only deletion requirement; permanent deletion of received mail remains unavailable. Freeze account, source version, container/message/hash and request identity; retain source bytes and a distinct `draft_delete` receipt. Fresh exact-target read, admitted claim and one DELETE produce definitive deleted/rejected or acceptance-unknown outcomes. Status and proven-undispatched abandonment stay local; explicit reconciliation only reads Google and never retries deletion. A 404 or later absence proves absence, not this request's success, sending, or resolution of an unknown Send. Exact HTTP verb/path admission must distinguish deletion from same-cost creation. Every claimed outcome persists through client cancellation and disconnect.

Core rechecks source/owner revisions and capabilities at preparation, publication and dispatch. Known related pending or unknown upload/Send/deletion work blocks conflicting actions; identityless unknown-create candidates remain visible uncertainty, never similarity-based ownership or blanket inferred relationships. Native confirmations retain account/source/navigation stamps and existing source/export race fences. Keep original sources and operation receipts inspectable after deletion or acceptance. GUI/CLI share the same methods, conversion disclosures and recovery states.

Functional acceptance requires schema-15 upgrade/refusal and old receipt preservation; exact copy content/attachments and unsupported-source refusals; adoption with zero initial upload plus first-edit and immediate-Send target checks; stale/cross-account/racing confirmations; typed delete admission and cross-kind replay refusal; changed target, definitive failure, lost response, cancellation/restart and read-only reconciliation; and paired native/CLI recovery and source access. Use generated providers only. Performance, resize, current packages/CI, complete Calendar, protected live-profile admission and actual-account acceptance remain required and unverified by this decision.

## 2026-09-13T20:41:49Z — D034: account addition and removal are required Phase 2 workflows

The founder updates the active end-to-end goal to explicitly include the ability to add and remove accounts, alongside OAuth, mail, Calendar, caching and smooth navigation. Treat both native and CLI account workflows as required Phase 2 acceptance. Existing disconnect behavior retains local data and does not by itself complete account removal.

Account removal means an explicit app workflow with clear local-data, credential and unresolved-work consequences. It does not authorize deleting Google mail/calendars or the Google account. Settle the shared removal/retention/recovery boundary before implementation; preserve unsent work and uncertain operation evidence unless their disposition is explicitly chosen. OAuth grant revocation and local removal remain distinguishable outcomes. This adds a required implementation packet, not permission to act on a real account now. Existing protected-profile admission and the pending pilot retention/backup choices remain required.

B3B draft verification continues because it supports the updated goal. Its D033 contract/schema boundary is unchanged. D031 still defers another performance measurement; adding account workflows does not waive the existing latency, rendering, package, live-provider or Calendar gates.

## 2026-09-13T21:21:25Z — D035: ordinary organizer invitations and truthful Calendar recovery

**Implementation refinement under D021, not authorization for live invitations.** Start P2-06B from B3B functional interim `d174525`. Core 23/Calendar 16/Mail UI 21/CLI 16/Quality 19 define schema 17; Gmail 11/Platform 12/Search 5, fixtures 0/3 and protocol 1 remain unchanged. Preserve existing journal/source/receipt bytes. Version newly prepared live Calendar intents and dispatches explicitly; the schema refusal protects them from old engines whose recovery would interpret roster changes using the earlier self-RSVP predicate. Reuse the existing Store, Calendar journal, command shapes and lifetime/HTTP infrastructure.

Support ordinary, unlocked, nonrecurring organizer create/edit/delete with effective writable permission, alongside the existing self-RSVP path. Omitted `fields.attendees` preserves the provider array. A supplied string array defines the complete removable guest-address set; an empty array removes all removable guests while protected organizer/self entries remain. Bound the resulting roster to 50 participants and validate individual addresses with the shared mail validator. Refuse duplicates, malformed or protected identities. New guests begin with `needsAction`; unchanged guests retain their supported provider metadata and RSVP. Do not expose another person's response or resource designation as writable user input.

Roster replacement requires a fresh complete uniquely identifiable source. Refuse omitted, duplicate, missing-address or asynchronously expanding rosters and any metadata the request cannot preserve. Such a roster limitation need not block a supported title/location edit that omits the roster. Preserve unchanged guest arrays by omitting them from PATCH, and preserve all unedited event fields. Existing event labels require their exact retained `eventLabelId` and `eventLabelVersion=1`; preserve legacy color behavior separately. Keep conference/attachment semantics and refuse unsupported conference solutions rather than inventing or reusing a conference. Conference creation and recurrence remain later packets.

Keep conditional PATCH, fresh GET/ETag checks, exact quota admission and durable connection/permission fences. Freeze `send_updates:all` for explicit mutations, explain that Google may notify guests, and never dispatch a second Gmail invitation. Successful responses prove provider acceptance, not notification delivery. A read that matches a versioned intent may produce a distinct terminal `reconciled` state with observed-state evidence and notification uncertainty; it never becomes an invented provider acknowledgment. Organizer roster matching checks exact membership and removals, while self-RSVP checks the authenticated participant's response. Unknown/incomplete observations or mere absence do not prove deletion or notification. A matching cancelled tombstone may establish observed cancellation only.

Do not resend a reconciled or unknown attempt automatically. Explicit recovery remains read-only; a later exact response may strengthen evidence for that same dispatched attempt without duplicating its mutation or rebasing newer authored work. Legacy journal handling stays explicit and preserves historical outcomes without adding fabricated notification evidence. Keep native/CLI status and recovery meanings identical.

Acceptance requires add/remove/all-remove/no-op rosters, unchanged metadata and all source refusals; preserved labels/conferences/attachments; exact sendUpdates/quota requests; ETag/permission/cancellation races; lost response and read convergence with truthful evidence; late outcomes/restart/no duplicate writes; actual schema-16 upgrade/old-engine refusal; and paired native/CLI generated-provider workflows. B3B's 337-artifact archive remains immutable. D031 performance deferral, D034 account workflows, recurrence/reminders, current packages/CI and protected actual-account admission remain required. Whole readiness stays 0/9.

## 2026-09-13T23:25:54Z — D036: recoverable local account removal and explicit restoration

**Implementation choice under D021/D034, not a founder answer or authorization to touch real accounts.** Start P2-01B from the clean P2-06B functional interim `7129129`. The optional archive preference question has not been answered. Proceed with the conservative, reversible default stated to the founder: remove accounts from active navigation and delete their local sign-in credentials while retaining a discoverable read-only archive of saved content, drafts, sources and operation evidence. Permanent local-data deletion and Google-side revocation/deletion are outside this packet. Keep the profile's existing desktop-client-ID restriction and configuration reference unchanged.

Core 24/Gmail 12/Calendar 17/Platform 13/Mail UI 22/CLI 17/Quality 20 define schema 18; Search 5, live fixture 0/synthetic fixture 3 and protocol 1 remain unchanged. Reuse the existing Store and immutable operation receipts. Add account lifecycle state/revision and bounded current credential-cleanup metadata. Commit removed/inactive state and a new connection generation before off-writer credential cleanup. Serialize cleanup with refresh and verified-grant persistence through the same owned account lock. A failed or interrupted cleanup remains explicit and retryable; completed historical receipts never authorize deleting a later restored credential.

Ordinary account lists and combined Calendar exclude removed accounts. Explicit archive discovery and account-scoped cached queries/exports remain available without read-on-open flags, new authored mutations, provider refresh/reconciliation or resumed outbound work. New mutation admission and publication require active lifecycle/generation. Local off-writer plans retain the monotonic lifecycle revision so remove→restore cannot revive an old save/copy proof. Preserve exact receipt replay, cross-kind/request conflicts, startup uncertainty recovery and definitive late outcomes; archived prepared replay returns retained status without dispatch.

Restoration is an explicit OAuth workflow for the same verified subject after required cleanup. Ordinary Add/reconnect cannot silently reactivate an archive. Recheck lifecycle/generation before native persistence and final binding. If a failed/cancelled restore may have stored a credential, keep the account removed and make cleanup uncertainty visible; do not leave a false current claim that its credential is absent. Successful restoration retains the same local identity and saved data, advances lifecycle/generation, and starts neither sync nor retained outbound work.

Use existing Accounts settings, mailbox/reader/composer and activity surfaces. Self-removal saves pending authored changes before a fresh revision-bound confirmation. Concurrent CLI removal preserves any unsaved native buffer with its exact original account/draft identity, disables automatic continuations and fences late callbacks; it cannot attach that text to a fallback account. Require an explicit disposition before discarding such a buffer. Distinguish this unsaved recovery state from saved read-only archive content. Handle removal of the selected or last account explicitly.

Acceptance includes shared direct-Core guards and replay ordering; remove/restore races with local plans, refresh/store, claims and late provider outcomes; cleanup failure/cancellation/restart; exact retained content/attachment/receipt hashes; same-subject restoration and wrong-subject refusal; two/last-account native navigation and unsaved buffers; actual paired CLI/native OAuth loopback and lifecycle workflows through an isolated default-off fixture using the existing synthetic auth signing material. Reports omit tokens, codes and authorization URLs. Preserve all isolation, quota and owner-lifetime gates. Actual schema-17 upgrade/old-engine refusal, full functional regressions and current source binding are required. No performance run is authorized under D031; recurrence/reminders, current packages/CI and protected real-account acceptance remain required. Whole readiness stays 0/9.


## 2026-09-14T02:12:03Z — D037: one-occurrence Calendar writes and retained cancellation identity

**Implementation choice under the continuing D021 goal.** Start P2-06C from the clean account lifecycle checkpoint `3242849`. Implement explicit organizer edit/cancel for one recurring occurrence in the existing native editor and CLI, with a provider-verified instance identity. Series/following changes and recurring self-RSVP remain separate required work. Core 25/Calendar 18/Mail UI 23/CLI 18/Quality 21 define schema 19; Gmail 12/Platform 13/Search 5, live fixture 0/synthetic fixture 3 and protocol 1 remain unchanged. Preserve the P2-01B archive and actual schema-18 Release binaries for upgrade/refusal checks.

A displayed occurrence key is not a Google instance ID or ETag. Resolve the selected account/calendar/series and typed original start before exposing an editable form. Verify any cached instance hint by an actual GET; never derive a provider ID from local expansion text. Without a verified hint, use bounded instances lookup: at most 250 entries/page, four pages, one matching identity, existing response/token bounds and no actual-time window that can hide a moved instance. Empty continuation pages are incomplete; duplicate matches or exhausted bounds refuse the operation. Timed lookup may use the documented originalStart parameter. All-day lookup uses a verified hint or bounded enumeration until actual provider serialization is established. Date and timed identities cannot alias; compare timed instants independently of display-zone spelling while retaining original provider time-zone/source fields.

Keep the existing two global Calendar preparation seats and quota-before-memory order. Each master/page/instance read owns one admitted seat; reduce it to an opaque bounded metadata proof or candidate identity and release all source/page data before another quota wait. A master read compares the fresh complete source with the bounded retained current master off the Store writer, then retains identity/version/source hash, validated parent restrictions and current source/lifecycle/connection/permission fences. Changed or unavailable retained master requires sync/review. This intentionally replaces the research proposal to carry a full master body between requests. The actual mutated instance retains its full bounded baseline in the existing operation dispatch. Two GETs never establish an atomic remote snapshot.

Use an additive occurrence-read command and the existing explicit Calendar write/status/resume/reconcile/abandon commands. New occurrence requests alone use intent/dispatch version 3 and freeze actual instance ID/ETag, parent/typed original identity, exact provider originalStartTime, expected master ETag/proof, unchanged request identity/fields, lifecycle and publication fences. Ordinary absent-version/v1 and explicit v2 journal bytes and behavior remain unchanged; reject unknown versions. Reuse the existing field planner and conditional instance PATCH/DELETE, preserving unedited guest/label/conference/attachment metadata and send_updates:all. No recurrence or identity fields enter PATCH. Exact replay precedes admission; an archived replay is status-only. Unknown mutations are never automatically resubmitted.

Read reconciliation verifies the exact instance/parent/original tuple and uses its own pre-read permission/lifecycle/source fence. Missing/inaccessible resources remain uncertainty. Matching state proves observed convergence, not provider acknowledgment or invitation delivery. Preserve exact late mutation outcomes after cancellation, abandonment or account removal without allowing stale cache publication. Sparse cancelled instances without ETag/start/end are read-only cancellation evidence and never authorize a new write. An exact DELETE acknowledgment may derive the cancellation tuple from the immutable dispatched identity, clearly distinguished from fetched source evidence.

Keep calendar_visible_sources strict. While expanding an already-selected current master, include effective published cancelled children for that exact account/calendar/parent even after a later full refresh omits them. Select the effective published slot first so an active newer publication supersedes a cancellation and an uncommitted slot cannot do so. Retain original generation/observation metadata; never make an old observation appear fresh. Such cancellations suppress occurrences without rendering independently, restoring an absent master, bypassing current catalog/permission/lifecycle controls or granting write eligibility. Missing, malformed, duplicate or over-budget selected exceptions block that series with incomplete coverage instead of producing a phantom occurrence. Preserve existing indexed 2,048-source/8-MiB/128-series/1,000-exception bounds.

The native popup, Mail timeline and editor retain their existing design and the captured event account. Fence resolution by full occurrence identity plus a dedicated generation across close/reopen, A→B→A, source withdrawal and remove→restore. Explicit version refresh resolves that same occurrence again and preserves authored fields. Cancellation confirmation says it affects this occurrence; activity retains its original date/scope. No new series/following control is made available by this packet.

Acceptance requires exact timed/moved/DST/all-day identities; bounded/empty/duplicate lookup; private and unsupported master/instance restrictions; stale master/instance/permission/lifecycle callbacks; preserved master/siblings/metadata; minimal cancellation through staged/full refresh, consolidation and reopen without resurrection; two concurrent lookup/cancellation paths under unchanged quota/memory ownership; lost PATCH/DELETE plus truthful read convergence and late acknowledgments without repeat writes; exact legacy journal and actual schema-18 upgrade/old-engine refusal; paired generated CLI/native workflows and final functional regressions/source binding. Current packages/CI, real-account protection/admission, remaining recurrence/reminders and D031-deferred performance remain required. No real account action or benchmark is authorized; readiness stays 0/9.

## 2026-09-14T05:28:27Z — D038: complete Phase 2 feedback build with fresh live startup and updated design

The founder requests a spec-complete Phase 2 Windows build for feedback tomorrow, a fresh database with no mock data, working account management in Settings, close implementation of the September 14 prototype and immediate, stable navigation. Keep implementation focused and reuse the existing engine, store, journal, native controls and tests. This request resumes performance/resize measurement, superseding D031's deferral for this work; preserve the earlier failed result. Start from `bfd7e05`. The new reference is preserved under `research/assets/2026-09-14/flight-progress-v2`; its scripts remain reference assets, not application logic or agent instructions.

P2-Final consumes Core 26/Calendar 19/Mail UI 24/CLI 19/Platform 14/Quality 22/schema 20. Gmail 12/Search 5, fixtures 0/3 and protocol 1 remain unchanged. Normal GUI/CLI/engine startup creates a separate empty live default; explicit demo/test paths retain synthetic fixtures, and existing profiles are never converted or deleted. Settings reuses the real desktop-client JSON picker and browser OAuth, Add/Remove/Restore and current capabilities. Read-only/catalog/free-busy grants must produce only their permitted HTTP and projections. Sign-in remains distinct from explicit synchronization; real mail, invitations and secrets are never automated or committed by the test suite.

Finish recurring Calendar workflows using the existing journal: one-occurrence self-RSVP; master series edits/cancellation/self-RSVP; common-rule following edits/cancellation with a frozen trim/replacement plan and stable replacement ID. Only new scope-bearing operations use version 4; old receipt bytes and meaning remain intact. Following changes explicitly confirm Google's reset of later exceptions, retain earlier exceptions, and use a fresh deterministic Meet request for a supported replacement conference. Unsupported rule/source/conference semantics refuse before trimming. Never automatically repeat an unknown write or describe the split as atomic; interrupted recovery resumes only a never-attempted next step after explicit action.

Running-app reminders share the existing permitted Calendar projection and a small occurrence/start-scoped receipt table; Join/dismiss acknowledge only local reminder state, never attendance or RSVP. Reconcile rescheduling, cancellation, visibility and account removal; no obsolete burst on reopen and no notification work after GUI close. Presentation cache entries remain bounded and scoped by account/view/identity; cached painting never replaces authoritative mutation checks, reviews mail implicitly or retargets drafts. The startup overlay follows actual initial local hydration, including zero accounts, without an artificial delay. Apply the reference's V shortcut, inline collapsed draft, compact compose controls and Calendar refinements; the combined/account display toggle is view-local and preserves saved calendar selection.

Acceptance is a fresh normal launch with no synthetic rows, Settings setup and lifecycle checks, meaningful recurring/recovery/reminder/grant tests, paired GUI/CLI regressions, unchanged navigation/search/resize budgets, native build checks and an SDK-free Windows package with concise test instructions. Record one final scoped execution report and preserve failures; do not create a new evidence bureaucracy per small change. Actual-account comparison, full-history coverage and sustained OAuth require real observations and remain explicit until exercised. No Phase 2 or whole-area completion claim follows merely from a package or simulated test pass.

## 2026-09-14T07:38:00Z — D039: close the cached Calendar search gap before final feedback packaging

The final review of D038 finds the already-required Find an event workflow missing. Complete it under the founder's active spec-complete request, using the existing Calendar projection and top search field. Do not add a second index or migration. This implementation choice consumes Core 27/Calendar 20/Mail UI 25/CLI 20/Quality 23; schema 20, Platform 14/Gmail 12/Search 5, fixtures 0/3 and protocol 1 remain unchanged.

Search an explicit displayed range of 1–42 civil days and selected calendars, with bounded case-insensitive AND substring matching over permitted title, organizer, attendees, location and description. Exclude busy-only data and deduplicate split-day segments by exact occurrence identity. Return at most 100 rows with coverage and has_more; no all-history or remote-search claim. GUI and CLI share calendar_search; native selection navigates by a fresh matching occurrence and late callbacks cannot publish across changed query/range/visibility/account/lifecycle.

Evidence is focused Core/CLI/native behavior, unchanged provider/privacy semantics, rebuilt runtime and final package/performance checks. Package01 at 513a5af has already passed its SDK-free relocated tests and remains an earlier source-bound observation. No real-account action is added. Preserve earlier failures and the single final evidence report.


## 2026-09-14T16:07:43Z — D040: email-first Google sign-in and Phase 2 completion on the Mac

The founder requests finishing Phase 2, prioritizing ordinary Gmail/Workspace account addition: enter an email and open Google OAuth, without selecting a JSON file. Resume from handoff `8155e99`/runtime `618e8a0` on macOS. P2-01C consumes Platform 15/Gmail 13/Mail UI 26/CLI 21/Quality 24; Core 27/Calendar 20/Search 5/schema 20, fixtures 0/3 and protocol 1 stay unchanged. This is authorization to implement and verify the product flow, not an instruction to send mail or invitations automatically.

Use one app-owned desktop OAuth registration, provisioned locally and included beside the engine in configured packages. Keep a developer override and legacy profile compatibility; never commit registration secrets, tokens or account data. Add an email hint to the existing bound loopback/PKCE/state/nonce flow. The verified Google subject remains identity, with the existing same-subject restore/reconnect and client binding. GUI and CLI use the same engine implementation. Sign-in still does not start acquisition; quota/protection/backup admission stays explicit. Missing registration is a clear build setup failure, never a fake or borrowed OAuth client.

Acceptance is the existing signed generated-provider GUI/CLI lifecycle walkthrough updated to enter email; focused setup/hint/relocation/refusal tests; current Rust/native regressions and relocated Mac package checks, followed by the unchanged available performance checks. Preserve the 997-artifact final-run archive and earlier failed/interrupted attempts. Real project configuration, actual personal/Workspace login, full source/attachment coverage, protected backup/restore, sustained authorization and other-host acceptance remain required for Phase 2 closure. The user has been asked for the existing client configuration path or project; do independent work while waiting.


## D041 — Prioritize an immediately usable Gmail test build (2026-09-14T16:45:06+00:00)

The founder says to forget performance work and deliver a version they can sign into Gmail and use with their real test mailbox as soon as possible. Stop the broader performance/workspace sweep and prioritize provisioning the existing Google Desktop registration into the already verified Mac package. The user will choose their account and consent in the browser. Preserve all measurements and incomplete runs; their gates are deferred for this feedback request, not passed. This instruction does not authorize automated external mail or invitations. A test-mailbox feedback build is not full Phase 2 closure.


## D042 — Provision the supplied Google Desktop registration (2026-09-14T16:52:53+00:00)

The founder supplies their Google Console Desktop OAuth client download and asks whether a callback URL must be registered. Configure that registration locally and include it in the native Mac feedback build under D041. The existing bound loopback listener uses a dynamically selected port and `/oauth2/callback`; Desktop clients require no manually registered web callback. Keep registration contents and all future account tokens outside Git and evidence. This resolves the missing app-registration input; actual user consent/provider behavior remains to be exercised. No performance or broad workspace rerun is requested.

## D043 — Repair and exercise the real test-mailbox UI (2026-09-14T17:35:52Z)

The founder reports successful sign-in, mail sending/receiving and acquisition of a Google-created event, then requests a full UI walkthrough and fixes, starting with unavailable event creation. P2-LiveUI restores the existing Core 27/Gmail 13/Calendar 20/Mail UI 26/Platform 15/CLI 21/Quality 24 boundary; Search 5/schema 20, fixtures 0/3 and protocol 1 remain unchanged unless a subsequent change explicitly requires a revision. D041 still defers performance and the broad workspace sweep.

The user explicitly authorizes test email and Calendar invitations only to the two recipient addresses supplied in this conversation. Keep that allowlist and actual mailbox data in private local working notes, outside Git. Test new, clearly labeled items and preserve unrelated user content. First repair catalog invalidation and late-arriving editor choices without weakening permission, active-sync or exact-target checks. Evidence is focused native regressions and a real UI walkthrough with observed provider outcomes; compile success alone is insufficient. Record sanitized results and remaining limitations in the existing Phase 2 execution report, preserving all earlier evidence. No blanket Phase 2 completion follows from this walkthrough.


## D044 — Keep recurring UI authority stable through unchanged permission observations (2026-09-14T17:58:00Z)

The D043 live walkthrough reproduces successful recurrence creation but an unsavable one-occurrence editor: the lookup itself refreshes Google permissions and advances the general Calendar source revision. Comparing that revision invalidates the just-completed lookup. Mail UI 27/Calendar 21/Quality 25 distinguish observed UI authority changes from bookkeeping revisions. Track a local epoch across changes in catalog membership, effective permission, writability/create eligibility or time zone; preserve account lifecycle, exact projected occurrence, scope/action and shared-engine proof/ETag checks. An unchanged permission observation must not invalidate itself, while observed revoke/restore cycles still invalidate older forms and callbacks. No schema, Google scope or engine command changes. Consumers are the native Calendar editor/details, Mail timeline and their focused regressions; Core 27/Gmail 13/Platform 15/CLI 21/Search 5/schema 20, fixtures 0/3 and protocol 1 remain unchanged. Also supply native accessibility activation for timed/all-day/timeline event blocks and names for dropdown options. Evidence requires actual UI read/edit plus the permission-cycle regressions; no performance or full Phase 2 acceptance claim.


## D045 — Dismiss search without reopening it on restored focus (2026-09-14T18:15:00Z)

The real-mail UI walkthrough reproduced Escape failing to dismiss the search popup, plus a dismissed search reopening when a later menu restored focus to its text field. Handle Escape in that field and open on deliberate mouse/Tab/back-Tab focus, typing or the existing Find action; popup/window focus restoration alone must not open it. Preserve the query, filters and independent Mail/Calendar contexts. Consumers: Mail UI 28 and Quality 26; other boundaries, schema 20 and fixtures 0/3/protocol 1 stay unchanged. Evidence is the failing native Main regression, its fixed rerun and the real UI retest. Performance and the broad workspace sweep remain deferred under D041.


## D046 — Keep generated evidence out of the source tree (2026-09-14T18:58:04Z)

The founder requests cleaning the branch before committing and merging because committed test logs and manifests add over a million lines. Move generated Phase 0–2 assets out of the tracked tree after verifying a complete local archive, including uncommitted Mac evidence and the unchanged Windows final-run ZIP. Preserve authored reports, failures, source/binary identities, supplied designs and executable test fixtures. This supersedes earlier requirements to retain raw evidence in Git; their byte-preservation and acceptance limits still apply. Historical decision entries remain unchanged.

Quality 27 adds evidence storage and repository hygiene checks for documentation contributors and native CI. Link existing raw evidence at exact historical commits and document local-only archive recovery; future raw output belongs in ignored local storage or CI artifacts. Record the snapshot checksum and measured cleanup results in the [archive index](research/evidence-archive.md). No runtime, schema, fixture or product acceptance changes. This prepares the working tree only: no commit, push, merge or history rewrite is performed. A later squash merge can omit the Phase 2 evidence from the main commit; it does not remove existing Git history.


## D047 — Require green hosted CI before resuming Phase 2 (2026-09-14T19:40:37+00:00)

The founder requires a green pipeline before continuing feedback-driven development and confirms that the initial GitHub billing block is resolved. P2-CI repairs test/build failures on the current branch under Platform 15/Quality 27; runtime contracts, schema 20, fixtures 0/3 and protocol 1 remain unchanged. Preserve all assertions and time budgets. The first macOS run exposes a test profile created with ambient directory permissions, which prevents the intended verification-marker refusal from being reached. Reuse the existing private-profile initializer in that fixture. Acceptance requires successful native/workspace tests and relocated package checks on macOS, Windows and Ubuntu for the final pushed source and PR merge tree. Keep raw logs in ignored local storage or Actions artifacts under D046; performance and whole Phase 2 acceptance remain separate.


## D048 — Allow the expanded workspace suite to finish in CI (2026-09-14T20:11:27+00:00)

Both Windows hosted attempts at `ebc3484` exhaust the 600-second Rust workspace aggregate allowance while Core tests continue progressing, without an individual test failure. The local ordinary suite also exhausts that allowance after completing Core and starting engine tests. Quality 28 changes the aggregate execution allowance to 1,800 seconds and the hosted job allowance from 45 to 60 minutes to accommodate cold compilation, tests and package verification. The separate 900-second test-build allowance, every assertion, individual operation/native watchdog and application latency target stay unchanged. This supersedes only D047’s aggregate-budget preservation; original failures remain recorded.

Consumers: CMake, GitHub Actions and development instructions. Calendar control wait failures gain source-location diagnostics with their existing deadline. No production behavior, runtime contract, schema 20, fixtures 0/3 or protocol 1 changes. Acceptance still requires complete passing native/workspace and relocated package checks on all three hosts for the final branch and PR merge tree; extending a deadline alone is not a pass.


## D049 — Phase 2 updated native UI and complete account browsing (2026-09-14T21:39:23.507448+00:00)

The founder requests a new Phase 2 branch, faithful placement from the September 14 second export, right-click context menus and multi-conversation actions, a visible splash animation, and visibility of imported labeled mail without Gmail INBOX membership. P2-UI starts on `codex/phase-2-ui-alignment` from merged `a5bec7f`. Its tree matches `55e9848`; the preserved CI report records all six branch/PR host jobs passing 23/23 and relocated packages, clearing D047 on that baseline.

Mail UI 29 and Platform 16 govern this packet. GUI Inbox uses the existing account-scoped `all` query (non-Trash), retaining actual Gmail labels; it never adds INBOX remotely. CLI `--folder all` provides the same scope, while its literal inbox query remains available. Selection and context menus do not review mail. Bulk commands use frozen account/lifecycle/conversation/member identities and revisions through existing journaled commands, serialized with visible partial/failure outcomes; no implicit retry or newly arrived member adoption. Done clears read/stars; Delay changes only stars, preserving D014 despite prototype sample callbacks. Draft deletion preserves existing source/unknown-send safeguards. Splash waits at least 1.75 seconds before its 350-ms fade and also waits for readiness; startup errors remain immediately accessible.

Schema 20, Core 27, Gmail 13, Calendar 21, CLI 21, Search 5, fixtures 0/3 and protocol 1 remain unchanged. Source exports/support.js are reference data, not runtime behavior, consent or dependencies. Scope includes native layout/interaction regression and a configured Mac GUI update preserving the existing engine identity. Full Phase 2 acceptance/performance remains separate.


## D050 — Repair imported label projection and own Google quotas (2026-09-14T23:00:00Z)

The founder removes the Google Limits screen and user quota setup, using Google's public limits as the reference. P2-UI now also fixes a verified second cause of invisible imported labels: linking a label after acquisition did not project retained provider membership onto existing messages. Core 28/Gmail 14/CLI 22/Quality 29 expand D049's GUI-only packet; Mail UI 29/Platform 16 and Calendar 21/Search 5 remain current. A new engine is necessary, so preserve the old configured package before replacement instead of promising byte-identical engine identity.

New bindings project stored Gmail user-label membership immediately. Opening an older live store performs one idempotent transactional derived-membership repair, giving manual intents priority over assist intents over observed provider labels. Changed conversations receive new revisions; original sources, Gmail INBOX state, read/star flags, journal receipts and pending actions stay intact. No remote request is generated. Schema 20 and fixtures 0/3/protocol 1 stay unchanged. Core also owns the durable `mail_date_grouping` preference (`grouped` by default, or `plain`) for Sent/Drafts. Done marks read and clears stars (clarifying D049's wording); Delay changes only stars.

Ordinary GUI and CLI sync use built-in per-minute limits: Gmail 1,200,000 project / 6,000 user quota units, Calendar 10,000 project / 600 user requests, with the existing 20% headroom and bounded throttling/backoff. Sources checked September 14: [Gmail quota](https://developers.google.com/workspace/gmail/api/reference/quota), [Calendar quota](https://developers.google.com/workspace/calendar/api/guides/quota). Assigned project quotas may differ; provider throttling is authoritative. Daily billing thresholds are not rate budgets. The existing explicit four-value API/CLI override remains for verification/legacy clients; no GUI field or saved preference is required or consulted for an ordinary start. Normalize default and explicit values before shared-owner equality checks. Sign-in still does not import, and Start sync remains explicit.

Acceptance: focused Core label repair/reopen/manual-override and preference tests, Google scheduler/backoff tests, engine default/shared-owner regression, CLI no-quota parsing, native selection/context/date/splash/settings regressions, and the configured relocated Mac package. Retain original failures in ignored evidence. Current other-host CI, performance and the remaining actual-account/backup/coverage acceptance gates remain separate; this does not close Phase 2.


## D051 — Plan Ollama-only local assistance alongside UI work (2026-09-14T22:14:47+00:00)

**Accepted requirement:** the founder requests local LLM support, Ollama only for now, using this machine's instance as a reference. Automatically discover Ollama and its models; allow model selection for summaries and classification against configured local labels; distinguish human and automated email; supply useful editable prompts in Settings. Prepare the plan/contracts before starting implementation. Existing branch use is permitted while another agent continues UI/mail work.

Local assistance 5/Triage 7 and P2-LLM advance this work from the later Phase 3 sequence. The proposed delivery defaults are separate summary/classification model selectors and editable prompts shared across local accounts, independent per-account enablement, Off by default, on-demand summaries and Suggest-first classification. Reuse the existing account category catalog and optional descriptions. Human/Automated/Unknown annotations are source-bound and user-correctable, independent of category relevance and Gmail state. Default prompts and the fixed schema/authority envelope live in Local assistance 5. Auto-apply requires subsequent labeled quality qualification; vLLM and broad historical reclassification are deferred. These delivery defaults are implementation proposals, not newly observed user preferences.

Read-only reference inspection finds Ollama 0.34.0 and 20 entries, including a cloud model. One local candidate's metadata succeeds, another returns HTTP 500, and the cloud entry returns HTTP 410. This proves limited discovery only. Model eligibility, actual local execution, structured inference, quality and server-side cancellation still need verification with synthetic inputs before enabling mail inference. No real mail or inference request was sent for this planning work, and the shared Ollama service was not changed.

Keep the existing human-authorship, manual-precedence, account isolation, no-cloud-fallback and session-lifetime constraints. No model changes unread/stars, Done/Delay, drafts, Send or Calendar. Preserve the concurrent D049/D050 code, evidence and package identity; coordinate shared type/schema and Settings integration when implementation starts. Re-read current consumer revisions then and update the owning Core/CLI/Mail UI/Platform/Quality contracts before introducing production interfaces. This planning update changes neither schema 20 nor fixtures 0/3/protocol 1, starts no runtime implementation and leaves readiness 0/9. D041's broad performance/workspace deferral remains in force.

Affected files: Local assistance, Triage, scoped local-AI research, Phase 2/development plans, product/UI scope, status, root README and contract index.


## D052 — Implement the planned Ollama packet (2026-09-14T22:21:50+00:00)

The founder says “implement it,” authorizing D051/P2-LLM implementation on the existing branch. Local assistance 6/Core 29/CLI 23/Mail UI 30/Platform 17/Quality 30 define the concrete shared commands, schema 21, bounded model adapter and native/CLI integration before consumer changes. Triage 7, Gmail 14, Calendar 21, Search 5 and fixtures 0/3/protocol 1 retain their boundaries. Preserve concurrent UI/mail edits and previous working packages. Use isolated build output and synthetic model inputs for qualification; record actual outcomes and remaining gates without claiming Phase 2 closure.


## D053 — Align summary presentation and repair assistance controls (2026-09-15T01:41:41.792587+00:00)

The founder rejects D052’s summary presentation and requests alignment with the latest supplied prototype plus related bug fixes and verification. Use the September 14 second export’s compact bordered Summary card, blue assistance accent, 11-pixel uppercase heading, quiet right-aligned model attribution and readable body. Put thread summaries below reader metadata; individual summaries belong with their source message. Keep secondary classification/correction/provenance actions out of the primary summary surface. Fix independent summary enablement, exact source navigation, pending/canceled/stale callbacks, disconnected job recovery and Settings sample/edit behavior. Summaries remain explicitly on demand; no automatic summary sweep follows from prototype sample behavior.

Mail UI 31/Local assistance 7/Quality 31 own this repair. Shared commands and schema 21/fixtures 0/3/protocol 1 remain unchanged. Evidence requires native summary-only and sent/received controls, delayed response/cancel/switch/disconnect checks, source navigation and small/large text across themes, synthetic engine/CLI regression, an installed-Ollama sample, and a separate verified Mac candidate preserving existing packages and real profiles. Report limits without whole-area closure.


## D054 — Respect Keychain denial and provide a real header sync control (2026-09-15T01:43:34+00:00)

The founder reports repeating macOS credential prompts after Deny and requests replacing the header's Local copy indicator with a sync button and last successful fetch time. Keep credentials in the native vault and respect its access controls. A vault refusal blocks further automatic credential attempts in this engine session until explicit Sync/reconnect; concurrent workers must share that refusal. Existing authorized work and account/owner cancellation fences remain. No plaintext credential fallback or automatic Keychain trust changes.

The header's account-scoped Sync action starts or wakes the existing shared sync owner with published budgets, fetches fresh Gmail history promptly, and shows the last fully applied successful history check. Persist that timestamp independently of UI refreshes; failures and partial pages cannot advance it. GUI and CLI share the command/status boundary. Consumers: Core 30/Gmail 15/Platform 18/Mail UI 32/CLI 24/Quality 32; schema 21, fixture 0/3 and protocol 1 unchanged. D053's concurrent summary UI remains independently owned. Acceptance: denied-vault concurrency/retry tests, generated-provider wake/success/failure and restart readback, native header interaction, configured Mac package. Broad performance and whole Phase 2 qualification remain separate.

## D055 — One useful local mail analysis (2026-09-15T05:02:49Z)

The founder requests one model and a fully visible editable prompt for per-message summaries, human/bot classification and one matching account label. Auto-label new mail and summaries default on once a model is selected; no per-result acceptance. Zero labels is valid when no definition fits. Summaries are one or two factual prose sentences describing the main point and any explicit recipient request, preserving conditional warnings without inventing a security incident. Show a current message summary in its list preview with the assistance icon, and remove the redundant thread-summary UI, sample buttons and default-reset buttons.

P2-LLM2 supersedes D051–D053's separate model/prompt, Suggest-first and off-by-default delivery choices. Local assistance 8/Core 31/Triage 8/Mail UI 33/CLI 25/Gmail 16/Quality 33 own the shared boundary. Schema 22 fences the new settings/result and assisted label-delivery semantics; fixtures 0/3 and protocol 1 stay unchanged. Preserve explicit saved account opt-outs, select the former classification model (or summary model as fallback), retain customized old instructions in the new prompt, and retain old result/receipt bytes. New settings do not classify historical mail. No model controls read/stars, Done/Delay, composition or Calendar. Local inference remains bounded and session-owned; missing models leave ordinary mail usable.

Acceptance: focused settings-upgrade, combined-result, template, stale/manual/provider precedence and single-label delivery tests; native Settings/message/list regressions; a configured Windows candidate and synthetic installed-model evaluation when available. Keep generated evidence ignored and report actual host/quality limits. This is implementation authorization, including prospective automatic label delivery, not permission to send mail or sweep a real mailbox.

## D056 — Show and edit the complete Ollama request (2026-09-15T05:47:42.459643+00:00)

The founder clarifies that the entire prompt means all app-supplied messages, JSON response syntax, template variables and generation settings in the same editable box. D055’s separate fixed system envelope and hidden structured-output schema do not meet that requirement. P2-LLM2B stores the full JSON chat request template; inference performs a single typed variable expansion and sends that request without appending hidden messages or replacing its schema/options. Existing application validation, output/action limits and local-only transport remain enforced outside the prompt. The model’s own Ollama chat template/tokenization is outside the app request.

Local assistance 9/Core 32/Mail UI 34/CLI 26/Quality 34 own the correction. Schema 23 preserves exact schema-22 settings in history and wraps saved text into the complete request without losing custom instructions or account choices; old engines refuse it. Fixtures 0/3 and protocol 1 are unchanged. Acceptance requires byte-semantic request equality at the fake HTTP endpoint, template substitution/escaping/bounds tests, settings migration and native editor checks, installed-model smoke and a separately packaged Windows candidate. No real-mailbox sweep, external send or broad performance work is authorized.

## D057 — Readable system and user prompt content (2026-09-15T06:14:41.119501+00:00)

The founder wants only the system and user message content in Settings, with normal newlines, not request JSON, response schema or generation options. P2-LLM2C renders each stored message content as an editable plain-text field in its existing order, labeled System or User. Content edits preserve every other request field. Local assistance 10/Mail UI 35/Quality 35 own this presentation correction; engine/CLI, schema 23, fixtures 0/3 and protocol 1 are unchanged. Verify native newline/content editing and preservation of the undisplayed request fields, then package the GUI with the existing verified engine.

## D058 — Incoming-only assistance and precise model failures (2026-09-15T06:33:19.129036+00:00)

The founder reports missing classification/labels and duplicate model errors, then clarifies that sent mail must never be summarized: assistance is for external incoming mail only. P2-LLM2D adds the omitted sent flag to reader messages, excludes sent messages from inference and visible historical results/previews, and removes their assistance controls. Received-mail classification and single matching label remain automatic when enabled; no matching label is a valid result. Preserve manual overrides and receipt history.

Separate truncated output, unfinished response, model mismatch and actual tool-call errors. Null or empty tool_calls means no tools. Raise the default generation allowance from 1024 to 4096 tokens to leave room for thinking plus final JSON, retaining 120-second and response-size bounds without automatic retry. Schema 24 archives old settings and updates only the former default token allowance; preserve user message contents, options otherwise and account choices. Core 33/Local assistance 11/Mail UI 36/CLI 27/Quality 36 own the packet; fixtures 0/3/protocol 1 stay unchanged. Verify sent/received queries, combined result application, old settings migration, HTTP failure cases, native no-sent/one-error behavior and a configured Windows candidate. Use synthetic inference, not a real-mailbox sweep.

## D059 — September 15 prototype throughout the native app (2026-09-15T07:00:55+00:00)

The founder supplies a new HTML export and requests complete GUI/UX alignment plus the backend needed by its working controls. P2-UI3 covers the reader, list actions, six Settings sections, Google connection, Calendar multi-day/custom recurrence and a shared confirmation design. The supplied source is preserved unchanged and inventoried in `docs/research/prototype-september-15.md`. The reference browser URL was rejected; source analysis and native screen checks must not be described as a rendered side-by-side comparison.

Core 34/Mail UI 37/Calendar 22/CLI 28/Quality 37 own the initial packet. Add an explicit `received` conversation scope: non-Trash, non-sent messages qualify a conversation; the reader retains all its members. Native Inbox uses that scope, superseding D049/D050's inclusion of sent-only conversations. Retain provider INBOX and all-mail query meanings. Add Terminal to shared preferences, retaining readability of legacy Harbour choices. Calendar authoring exposes supported weekday/end-date recurrence through the existing conditional Google write boundary. Keep stale authority, uncertain outcome and unsaved-buffer protections, while using the shared compact confirmation surface.

Schema 24, fixtures 0/3 and protocol 1 initially remain unchanged. Preserve the earlier uncommitted assistance work, exact historical receipts, configured packages and real profiles. Scope new persistence changes before implementing them. Evidence requires focused shared-boundary tests, real native controls/keyboard/form checks, native screen inspection and a separate configured Windows candidate. Whole readiness and other-host qualification remain open; no real sends, invitations or mailbox sweep are implied.

## D060 — Local server choices in the updated prototype (2026-09-15T09:03:43Z)

P2-UI3 includes the prototype's Ollama and local OpenAI-compatible provider choices. Local assistance 12/Core 35/Mail UI 38/CLI 29/Quality 38 add a persisted provider with Ollama as the backward-compatible default. Keep the same combined incoming-message analysis, visible System/User contents, account switches and manual precedence. Standard compatible servers expose model IDs, not verified weight digests: record that distinction accurately and send only to the configured loopback server. The server controls how its model runs; no hosted fallback, model download, API key storage or service management is added.

Map the existing expanded messages and response schema to `/v1/chat/completions`, with temperature/output limits, no tools and no streaming. Server context size is configured in that server; the application retains its input/response/time bounds. Ollama keeps its existing request and digest checks. Schema 25 adds compatible-provider classification provenance, preserves all existing rows/settings/history/receipts and fences older engines. Fixtures 0/3 and protocol 1 stay unchanged. Verify exact HTTP messages/schema, discovery, incomplete/tool/mismatched responses, cancellation/limits, unchanged old snapshots, settings migration and native provider switching before packaging. No real-mailbox inference is implied.

## D061 — Account labels and explicit Google synchronization (2026-09-15T09:24:38Z)

P2-UI3 implements the prototype's account-specific label rows, local descriptions, Enter-to-add, shared removal confirmation and Sync with Google. The prototype removes labels locally; explicit sync merges Google user-label names back in and creates missing local names in Google. An optional founder question about remote deletion remains unanswered; use this supplied behavior. This does not delete mail or Gmail labels. Google names are authoritative for linked labels; descriptions stay local in the existing assistance settings. Preserve unrelated settings, manual choices, account separation and immutable operation receipts. The Accounts retention statistic reports the oldest cached message date, not an invented retention policy.

Core 36/Gmail 18/Mail UI 39/CLI 30/Local assistance 13/Quality 39 own the boundary. Schema 26 adds a durable label-creation record so interrupted/uncertain creates are reconciled from the next explicit label inventory and never blindly resubmitted. Local label CRUD preserves historical receipts while clearing removed local membership/mapping/intents. Local unmapped labels can be assigned without Google delivery; linking flushes captured pending membership through the existing mutation journal. Existing review/star/Send/calendar authority is unchanged. Sync uses the existing account-scoped operation owner, quota/cancellation path and typed Google labels API. Duplicate names, missing access, conflicts, limits and unknown outcomes must be visible. Fixtures 0/3/protocol 1 stay unchanged.

Acceptance: focused local lifecycle/replay/description preservation and provider create/reconciliation tests, native account selection/Enter/edit/remove/shared confirmation/sync controls, paired CLI checks and the configured UI3 package. No actual mailbox/provider writes are used for these verifiers.

## D062 — Composer file input and cached recipient suggestions (2026-09-15T10:27:43+00:00)

P2-UI3 continues the supplied composer interaction: selecting multiple local files and dropping files into the editor use the existing revisioned single-file import in a serial queue. Each selection captures account/draft identity. Keep uncertain import receipts for exact retry, preserve newer authored fields, pause remaining files on errors or navigation, and expose resume/skip without retargeting a file. A pending receipt still prevents Send/discard. No automatic upload, Send or cloud download-link service is introduced.

Core 37/Mail UI 40/CLI 31/Quality 40 add a bounded, read-only cached-recipient query and native suggestions with names, addresses, account attribution, keyboard navigation and duplicate exclusion across To/Cc/Bcc. Prefer the selected account; existing active account mail/draft metadata supplies candidates, without reading source files or creating a contacts app. Suggestions never change routing until chosen. Stale replies after query, account or draft changes are discarded. Schema 26, fixtures 0/3 and protocol 1 remain unchanged.

Acceptance: focused query bounds/validation/ranking/read-state checks, native production composer tests for multi-file sequencing, uncertain retry, navigation and in-flight edits, suggestion mouse/keyboard/stale identity checks, existing draft/Send regressions and a configured package. Full prototype parity and actual-account/other-host acceptance remain open.


## D063 — Composer sender-account transfer (2026-09-15T10:58:14+00:00)

P2-UI3 implements the prototype's From-account selector as an explicit transfer of the selected local authored draft. Preserve subject, To/Cc/Bcc, text/HTML, signature, attachment bytes and frozen RFC reply context. The destination uses its primary From address/display identity and its own draft/attachment ownership; do not manufacture a received conversation or reuse the source account's Google thread/draft IDs. Preserve the original row and historical receipts. Account lifecycle, local draft revision, provider state and destination identity must still match when preparation commits. Existing pending/unknown Send or draft operations must be resolved first.

The default move policy retires the local source after the destination commits and removes an existing source Google draft only after the destination has been acknowledged by Google. Capture the exact original provider identity and content observation; changed/unknown remote state requires visible reconciliation, never blind deletion. A copy policy keeps the original; an optional founder preference question is pending. These are application behaviors, not authorization for test writes to an actual mailbox. A durable transfer record owns completion, recovery and the source/destination relationship across process restart. Preserve account-removal fences and shared CLI/GUI semantics.

Core 38/Gmail 19/Mail UI 41/CLI 32/Quality 41 own this packet. Schema 27 adds only the transfer journal; all existing settings, messages, sources, drafts and operation receipts remain unchanged during migration. Fixtures 0/3 and protocol 1 are unchanged. Work is in progress: package94/schema26 remains the tested user candidate until native account-selection, actual migration/refusal/reopen, content/replay/cancellation, provider cleanup and relocated package checks pass. The full prototype goal remains open.


## D064 — P2-UI3 Send Undo and shared action toast

2026-09-15T11:56:49+00:00

The September 15 prototype gives Send a six-second Undo interval and a common compact action toast. Implement that interval before invoking the existing exact-revision Google Send command. Save and validate recipients/content first; keep a bounded GUI-owned pending action per draft, preserve its account/lifecycle and authored revision, and allow browsing or composing elsewhere. Undo removes the action before any submission request; closing/disconnecting the GUI cancels unsubmitted actions and retains their saved drafts. After the interval, reread the same draft and current Google revision so ordinary draft synchronization does not make the saved Send stale; changed authored content, account incarnation or unsafe provider state must not be silently retargeted or retried. Existing exact Send receipts and unknown-outcome recovery remain authoritative once invocation starts. Use truthful Sending/accepted/error text rather than simulated provider success. CLI may opt into the same six-second client-side cancellation interval without changing the Core envelope.

Mail UI 42/CLI 33/Quality 42; Core38/Gmail19/schema27/fixtures0,3/protocol1 unchanged. Evidence: actual native Undo/timing/navigation/lifecycle/close tests plus unchanged Send recovery controls, CLI argument/cancellation checks, theme captures and a configured relocated package. The broader prototype pending-Sent presentation, draft-discard Undo and Calendar-delete Undo remain required under the full active goal; this decision does not substitute this slice for them. No real account send or invitation is authorized for automated verification.

## D065 — P2-UI3 durable draft-discard Undo

2026-09-15T12:44:55Z

Implement the prototype's six-second draft-discard Undo through the shared store and CLI/GUI boundary. Discard atomically retains an immutable snapshot of the saved draft and any captured editor buffer before clearing the active draft. Preserve body/HTML, signature and From identity, recipient lists and unfinished recipient input, attachment references, and frozen reply context. Undo names the original discard request, account/lifecycle and exact discarded revision; it must not overwrite newer edits, revive a transferred source, or retarget an account. Exact request replay precedes current-state validation. Keep old receipt bytes unchanged. Unknown discard/Undo results retain their original request identity for explicit status/retry. Multi-selection restores the successfully discarded drafts together from the shared toast.

Schema28 adds the discard snapshot journal and durable unfinished recipient input to the existing drafts/store. Snapshot/restore checks retained attachment ownership and limits without reading files on the writer; capture/upload still verifies bytes on the existing bounded worker. Local autosave can retain unfinished address input; Send requires that input to be resolved. The existing local discard and Google draft recovery semantics remain intact; this packet does not pretend to reverse a remote deletion or finish the separate provider-cleanup/pending-Sent/Calendar-Undo gaps.

Core39/Gmail20/Mail UI43/CLI34/Quality43, fixtures0/3 and protocol1. Evidence: exact snapshot/replay/restart, stale/lifecycle/ownership refusal, unfinished-recipient persistence and upload guard, native editor and selection Undo/recovery, actual schema27 migration/refusal/reopen, scoped regressions, strict lint and configured relocated package. Package135/schema27 stays preserved until that evidence passes. The complete prototype goal remains active; no real provider mutation is authorized for automated verification.


## D066 — P2-UI3 Calendar deletion Undo

2026-09-15

Implement the prototype's six-second Undo for single-event, occurrence, series and supported following deletion. A bounded GUI queue retains the exact confirmed account/calendar/event/ETag and recurring identity before invoking the existing shared Calendar command. Hide only matching projected events across Calendar, Mail timeline and cached search; Undo restores the current cache without rewriting historical event snapshots. Closing/disconnecting cancels unsubmitted deletions. Changed account/authority/source cancels a queued deletion, while the existing Core/provider conditional checks remain authoritative at dispatch. Once invoked, use existing durable operation status/reconciliation and never retry an unknown deletion automatically. Show later outcomes through the common toast and Calendar activity, preserving unrelated open editors.

Calendar23/Mail UI44/Quality44; Core39/Gmail20/CLI34/schema28, fixtures0/3 and protocol1 unchanged. Acceptance: production QML confirmation/queue/close, real six-second interval and Undo, exact source and recurring scope filtering, account/permission changes, no dispatch after Undo/close/disconnect, original request delivery and failure recovery, existing Calendar regression and configured package. Package146 remains preserved. Full prototype parity, provider draft cleanup, pending Sent, account disposition and remaining visual/interaction work are still open. Verification uses isolated fixtures only.

## D067 — P2-UI3 outgoing message presentation

2026-09-15

Show the prototype's pending Sent rows and reply bubbles using the exact saved version during the six-second Undo interval and the existing immutable Send journal after invocation. Add bounded, read-only shared queries for outstanding outgoing messages and their retained MIME content; do not insert fabricated provider messages into Inbox or change Send certainty. Accepted messages remain visible through restart until their exact provider identity is acquired. Acquisition, including later Trash/deletion, suppresses the duplicate preview. Unknown submissions remain visibly unresolved and use the existing explicit recovery controls. Undo removes only its own unsubmitted preview and restores its saved editor. Reader content must never follow later draft edits.

Core40/Gmail21/Mail UI45/CLI35/Quality45; schema28, fixtures0/3 and protocol1 unchanged. Reuse the Send journal and bounded local content workers; preserve all existing receipts. Verification will cover frozen content/restart, account scope, acquisition deduplication, bounded reads, native Send/Undo/navigation and recovery, and the configured relocated package. Package151 stays preserved. The complete prototype goal remains active and real provider writes remain outside automated verification.

## D068 — P2-UI3 linked Google draft discard

2026-09-15

Discard of a locally owned draft also removes its linked Google draft after the shared six-second Undo interval. Record that intent alongside the immutable discard snapshot, with an authoritative expiry returned to the GUI; show the remaining interval so a displayed Undo cannot outlive its provider gate. Undo/new edits/account incarnation changes cancel unclaimed cleanup. Reuse the existing verified-source deletion journal, owned sync lifetime, quota/memory limits and read-only reconciliation. A stopped app starts no background service; committed cleanup resumes when an explicit sync owner next runs.

Wait for earlier owned uploads, reconcile acknowledged versions, and cancel only proven-undispatched preparation. Delete only an exact linked source whose own acknowledged/adopted identity still matches; preserve external changes and unknown ownership. An unknown DELETE is read-only reconciled, never automatically repeated. Pending or blocked removal must be visible from Drafts and inspectable/retryable through the same Core/CLI boundary. Existing receipts and local snapshot bytes stay unchanged. Historical discards acquire no new provider intent during migration.

Core41/Gmail22/Mail UI46/CLI36/Quality46/schema29; fixtures0/3 and protocol1 unchanged. Required evidence: exact target/deadline/Undo/claim fences, late upload and source-change handling, unknown/rejected deletion recovery, restart/account lifecycle, native confirmation/activity/Undo, actual migration/refusal/readback and configured package. Preserve package164. The full prototype goal remains active; all verification uses isolated generated data/providers.

## D069 — P2-UI3 visual and interaction scope

2026-09-15T11:04:12-04:00

The founder narrowed the active objective to matching the latest prototype's GUI and UX, followed by commit and push. Set aside D068's unfinished provider cleanup; it is not a condition of this UI pass. Preserve its exact work in ignored `build/verification/ui3-cleanup-parked168/`, including the untracked module and this turn's patch. Remove the incomplete migration and module registration from the active build, retaining the verified schema28 behavior. D068 was never delivered or exercised against a real profile.

Mail UI46/Quality46 cover the source-based UI comparison: search quick-filter chips, selected-chip colors, Sent date filters, current theme coverage and Settings wording. Existing typed filters own the query; shortcut examples use the account's real categories/senders and real date bounds rather than simulated query syntax. Core40/Gmail21/Calendar23/CLI35/schema28, fixtures0/3 and protocol1 remain the delivered boundary. Scoped native interaction checks, theme/resize captures and a configured relocated package count as evidence. The earlier browser restriction still prevents rendered reference comparison; do not equate source inspection with visual acceptance. Account retention wording must remain truthful to the existing archive behavior.

## D070 — P2-UI3 Calendar visibility

2026-09-15

Add the prototype's Show/Hide control for other accounts' events. A typed `calendar_account_visibility` preference defaults to `all`; `current` intersects the existing individual calendar selection with the currently selected mailbox. Calendar, timeline and contextual Calendar search use the same effective selection and invalidate stale reads after account/catalog changes. Preserve explicit calendar selection, sync and reminder settings. Move individual calendar and display-zone controls into the existing expandable details so the default Settings layout follows the prototype.

Core41/Calendar24/Mail UI47/CLI36/Quality47; Gmail21/schema28/fixtures0,3/protocol1 unchanged. This additive presentation preference uses the existing extensible JSON preferences store and generic CLI patch command. Verify typed saves/replay/restart, exact old-engine round-trip retention, native account-switch and late-response guards, visible Settings controls and the configured package. No new provider workflow is required.

## D071 — P2-UI3 adjacent timeline days

2026-09-15

Render the preceding, selected and following days while dragging the Mail timeline, matching the supplied prototype. Use one bounded three-day cached Calendar query through the existing shared method. Retain only those three day projections; re-use a cached adjacent day when navigation finishes, and invalidate them when calendar selection or display zone changes. All projections respect account visibility and pending deletion Undo. Only the selected day's controls can receive focus or open details. No new provider acquisition or schema is needed.

Calendar25/Mail UI48/Quality48; Core41/Gmail21/CLI36/schema28/fixtures0,3/protocol1 unchanged. Verify three distinct day projections, cached navigation, stale-response and scope guards, native drag/click/keyboard behavior and theme captures. This completes the remaining identified interaction gap before the final UI audit and package.

## D072 — account setup and local-model feedback

2026-09-16

The founder requests permanent local archive deletion with the common confirmation, clearing obsolete removal messages after restoration, no separate Meetings permission checkbox, a setup retention choice (1 month, 3 months, 1 year default, All), working installed-model selection and summaries, and removal of the global top error bar. Preserve contextual errors, incoming-only assistance, explicit user-controlled review state and Google data. Archive deletion must be revision-bound, remove only an already removed account's local content, preserve other accounts and prevent stale work from reviving deleted content. Reconnection must remain possible. Never exercise deletion against the founder's actual profile during verification.

P2-Feedback / Core42/Gmail22/Local assistance13/Mail UI49/CLI37/Quality49; fixtures0,3/protocol1 unchanged. Inspect the existing lifecycle/content and acquisition boundaries before changing storage; record any required schema migration in the owning contract. Evidence: isolated archive deletion/reconnection and retained-other-account checks, setup/restart acquisition bounds, exact expanded-request budget regression, native model selection/setup/dialog controls and a configured package. Diagnose actual model metadata read-only; do not send real email to a model during automated verification.

D072 implementation clarification (2026-09-16): local assistance was already revision13, so this packet advances it to revision14. Schema29 now owns account archive deletion and initial history coverage; the unrelated D068 implementation stays parked. Retention chooses initial history and keeps previously saved mail, with no rolling purge. Blob cleanup runs at the next exclusive Store startup so it cannot race an import or delete another account’s shared file.

## D073 — mail filing, thread identity controls and draft readback

2026-09-16

The founder reports ineffective Archive/Trash with pending Google outcomes, incorrect reader placement/hover behavior compared with the supplied prototype screenshot, and a draft byte-fidelity warning. Restore useful filing behavior: Inbox excludes archived and sent-only mail; All mail remains available. Fix empty-body provider submissions without weakening exact-target/readback authority. Make sender, recipient and participant names expose the existing contact card, separate their hover targets from expanded message metadata/timestamps, and follow the screenshot's sender-kind placement and compact sent row. Draft readback must distinguish harmless MIME serialization changes from changed human-authored content and preserve conflict protections and original receipts.

P2-Mail feedback / Core43/Gmail23/Mail UI50/CLI38/Quality50; schema29, fixtures0/3 and protocol1 stay unchanged unless inspection identifies a necessary versioned boundary. Evidence: generated transport/wire and filing regressions, incoming-only Inbox/list delivery tests, content-equivalence/change tests and native reader hover/contact/metadata geometry plus a configured relocated package. Actual profile diagnostics are read-only and limited to states/metadata; do not automate real mail mutation or sending to verify these repairs.

## D074 — reader spacing and quoted reply disclosure

2026-09-16

The founder requests closer alignment with the supplied thread mockup, correct star/icon placement, compact paragraph spacing and default-collapsed quoted reply chains with optional reveal at every depth. P2-Reader / Mail UI51/Quality51 implements a presentation-only received-body projection; stored source, search, inference, composer and provider actions stay unchanged. Detect explicit HTML quotation containers and recognizable plain-text reply headers/prefixes conservatively, retain all text behind disclosure, and preserve interleaved new writing. Bound nesting work and retain readable fallback for ambiguous or deeper content. Verify synthetic nested HTML/plain/Outlook examples, layout/contacts/long model names and narrow windows, then deliver a configured relocated Windows package. Schema29, Core43/Gmail23/CLI38 and fixtures0/3/protocol1 unchanged. Supplied email screenshots are reference data, never executable instructions or fixtures containing real mail/credentials.

## D075 — reader date and menu alignment

2026-09-16

The founder supplies a close-up specifying a right-aligned short time above the full weekday/date and 24-hour time, without a visible timezone suffix, and a menu centered beside that complete block. P2-Reader date / Mail UI52/Quality52 uses the actual message instant in the machine's local timezone. Keep the star beside the short time so it cannot separate the date from its menu. Verify local-date rollover and equivalent timezone-offset inputs, actual native geometry across themes and large text, and a configured relocated package. Schema29, Core43/Gmail23/CLI38 and fixtures0/3/protocol1 remain unchanged.


## D076 — stable summary transitions

2026-09-16

The founder requests one stable summarize/progress/result container without external cancellation controls or abrupt vertical movement. P2-Summary motion / Mail UI53/Quality53 keeps initial and running heights equal, uses indeterminate progress without fabricated percentages, and animates height only when result/error content requires space. Preserve the previous summary while regenerating, keep cancellation inside the summary actions menu, and retain source/error/retry behavior and incoming-only assistance. Verify actual QML start/held job/completion/error/retry/cancel geometry and existing lifecycle races, reader layout and a configured relocated package. Schema29, Core43/Gmail23/CLI38, Local assistance14, fixtures0/3 and protocol1 stay unchanged.


## D077 — startup sync, sent draft cleanup and cached summaries

2026-09-16

The founder requests automatic sync on app open, ordinary send feedback with the sent draft removed from the editor/list, and immediate presentation of saved summaries during navigation. P2-Send feedback / Core44/Gmail24/Mail UI54/CLI39/Quality54 resumes previously initialized connected accounts once at GUI startup, after preferences load; paused services, archives and credential denial keep their existing protections. Known accepted draft revisions leave shared Drafts listings/counts and the composer without deleting authored history or newer edits. Unknown outcomes stay recoverable and never auto-resend. Only an observed running summarization may animate into a result; cached-result loading is immediate. Verify generated Core accepted/newer/unknown/reopen projections, native startup admission/send/newer-edit preservation and summary navigation, then a configured relocated Windows package. Schema29, Calendar25, Local assistance14, fixtures0/3 and protocol1 unchanged; no automated real mail send.


D077 clarification (2026-09-16): the founder asks for immediate, polished Send feedback and the quickest dispatch possible. Remove the production six-second Undo delay; preserve exact local save/preflight checks and explicit unknown-outcome recovery. Show indeterminate progress in the fixed overlay while sending, including attachments, then a confirmed Sent checkmark. No fabricated upload percentage or artificial minimum duration. Retain the delayed-queue regression as an isolated test of cancellation/race protections, not current user-facing behavior.


## D078 — attachment previews and saving

2026-09-16

The founder requests rendered image thumbnails, a full-image modal and a minimal download button for every attachment. P2-Attachments / Mail UI55/Quality55 reuses the account-scoped verified attachment export. Raster thumbnails reserve their geometry, open in a fit/actual-size modal, and expose Save as through the native file picker. Any available attachment can be saved, with atomic writes and digest/length verification; navigation cannot retarget a pending save. Invalid/unavailable images retain download access and contextual feedback. Verify synthetic preview/modal/byte-exact export/overwrite/failure/navigation behavior and the combined D077 configured Windows package. Core44/Gmail24/CLI39/schema29, Calendar25, Local assistance14, fixtures0/3 and protocol1 unchanged.

## D079 — final interaction review

2026-09-16

The founder requests a second review of completed flows and adjacent issues, prioritizing stable layout, clear messages and responsiveness. P2-Interaction polish / Mail UI56/Quality56 checks attachment saving/loading/modal/navigation, send status and cached summaries. Reserve attachment status geometry, preserve images while asynchronous size requests complete, honor image orientation, remove obsolete production Undo wording, and retain explicit recoverable errors. Verify native geometry/state regressions and a configured relocated Windows package. Core44/Gmail24/CLI39/schema29, Calendar25, Local assistance14, fixtures0/3 and protocol1 unchanged. No automated real send or broad performance sweep.


## D080 — local summary reliability and latency

2026-09-22T02:00:00-04:00

The founder requests testing all installed Gemma, Qwen and Glimmer models and fixing unreliable, slow summaries. P2-Summary reliability / Local assistance15 / Quality57 uses synthetic model evaluations, shared validator/engine and CLI integration, native assistance regressions and a configured Windows package. Schema29, Core44/Gmail24/CLI39/Mail UI56, live/synthetic fixtures0/3 and protocol1 remain unchanged. This authorizes scoped inference latency measurements; unrelated broad performance work stays deferred.

Default Ollama requests disable thinking, including older saved templates that omit the option; explicit boolean overrides remain supported. Preserve user prompts and settings bytes. Normalize summary whitespace; keep the 600-character bound and a 100-word hard limit, with one or two sentences and 65 words as generation guidance rather than a punctuation-based rejection rule. Preserve strict schema, source identity, label authority, stale-result and cancellation checks. Model-quality misses and failures remain in ignored evidence; do not silently substitute models or claim general accuracy from a small synthetic set.

D080 implementation clarification (2026-09-22): the installed Gemma also returns invalid labels and empty/code-fence summaries. Permit one format/completion correction request with fixed instructions, never embedding the rejected response. Reserve 500 bytes from the initial 12,000-byte budget, keep both attempts inside one 120-second deadline, and retain cancellation/model/source authority. Transport/model/tool failures do not retry. This supersedes D056's prohibition on appended messages only for that bounded correction.


## D081 — responsive mail, stable signatures and scrolling (2026-09-22T02:09:37-04:00)

The founder requests mailbox-tab sync progress, reliable signature editing, much faster thread/list navigation, automatic older-mail loading without stalled pagination, and wider always-visible scrollbars. P2-Mail responsiveness / Core45 / Mail UI57 / Quality58 covers stable editor ownership, priority reader requests, bounded rendering, sync-safe live pagination and ahead-of-scroll prefetch. Scoped synthetic navigation/acquisition/large-list measurements are authorized, superseding earlier performance deferrals for these paths. Preserve concurrent D080 model work and existing artifacts. Schema29, Gmail24/CLI39, fixtures0/3 and protocol1 remain unchanged. Verify actual native focus/selection under account refresh, held-read navigation, repeated sync changes during pagination, scrollbar interaction and the configured Windows candidate; no whole-area or other-host acceptance follows.

## D082 — session debug console (2026-09-22T06:27:53Z)

The founder requests an everyday debug console on the tilde key, except while composing or searching, with readable runtime state, events, warnings, errors, sync and rate-limit diagnostics that can be pasted for debugging. P2-Debug console / Mail UI58 / Quality59 adds a session-only bounded native diagnostics feed and copyable report. Backtick and shifted tilde toggle the panel; editable focus and an open search popup preserve normal input. Snapshot existing backend state and instrument metadata at the shared GUI IPC/operation boundaries; do not change provider scheduling, storage, protocol, or concurrent D080/D081 behavior. Exclude payloads and credentials, alias accounts and redact diagnostic strings before retention. Verify production-QML keyboard/focus, filters, pause/copy, bounded threaded capture, redaction and integrated native startup in a separate build directory. Schema29, live/synthetic fixtures0/3 and protocol1 remain unchanged; no new whole-area acceptance or package claim.

## D083 — daily mail counts on Calendar (2026-09-22T06:57:53Z)

After the debug console, the founder requests a label showing emails received and sent each day in Calendar day, week and month, styled to the existing app. P2-Calendar mail counts / Core46 / Calendar26 / Mail UI59 / CLI40 / Quality60 adds one shared read-only query over cached message metadata and a native label below each date. Follow Calendar's display zone and combined/selected-account toggle; use stored Google arrival dates where available, otherwise source dates. Counts are messages with stored sent direction, include retained archived/Trash mail, and exclude removed accounts, provider-deleted mail, drafts and submissions not yet acquired as messages. Expose cached coverage in the tooltip; do not imply complete mailbox history. Use the existing CLI call boundary and store, with no migration, provider request or changes to concurrent work. Verify civil-day/DST boundaries, account scope, read-only behavior, stale native replies, all three layouts/themes and engine/CLI agreement. Schema29, fixtures0/3 and protocol1 remain unchanged; existing packages and whole-area readiness0/9 remain separate.

## D085 — scrollbar visibility (2026-09-22T14:10:00-04:00)

The founder reports always-visible scrollbars on dialogs that do not overflow, including Connect Google and event details. Always-visible tracks stay on the conversation list and the thread reader only. Other surfaces keep the wide scrollbar and show it only when content overflows. Mail UI61. Schema29 and readiness 0/9 unchanged.

## D084 — navigation crash and stable reader presentation (2026-09-22)

The founder reports a native access violation during mail navigation and staged reader resizing, and rejects the horizontal scrollbar on mailbox tabs. P2-Reader stability / Mail UI60 / Quality61 removes that tab track while retaining scroll access and wide content scrollbars, fixes the crash against the actual dump, and prepares retained source/metadata/saved assistance before publishing a new reader. Use bounded read-only prefetch and caches; never mark prefetched mail reviewed. Preserve unrelated concurrent work, exact old crash/build evidence and authored drafts. Verify a failing-then-passing error-response regression in Debug, stale-navigation/cache fences, native final-layout/rapid-navigation checks and the updated development executable. Core46/Calendar26/CLI40/schema29, fixtures0/3 and protocol1 remain unchanged; actual-mailbox latency and other-host acceptance remain separate limits.


## D086 — September 22 mail list and explicit account backlog (2026-09-22T14:40:50-04:00)

The founder supplies an updated prototype and requests visual/engine implementation, account backlog analysis and common keyboard navigation. P2-Mail backlog / Core47 / Local assistance16 / Mail UI62 / CLI41 / Quality62 uses the unchanged supplied HTML as the current visual reference. Preserve the pending D085 scrollbar changes. Sender name/address occupy a separate column alongside subject/preview, category and date; retain a compact adaptation at narrow widths. Use one input cursor, deliberate read-on-open, range selection, arrows, Page Up/Down and Home/End, with editable fields/popups retaining their own keys.

Accounts adds real retained-message/eligible-analysis counts and an explicit Last week / Last month / Last year / Everything menu. Process only missing current enabled projections; preserve existing summaries and manual choices. Scan captured account history in bounded pages, infer one message at a time through the existing shared semaphore, yield between messages and expose actual completed/skipped/failed counts and Stop. No elapsed-time simulated percentages or fabricated Google totals. Jobs belong to the invoking session, cancel on disconnect/settings/account changes and never silently resume after restart. GUI and CLI share the operations. Schema29, live/synthetic fixtures0/3 and protocol1 remain unchanged. Verify scoped Core selection/provenance, engine ownership/cancellation/fairness, CLI and actual native layout/input/settings tests plus a configured Mac build; whole-area readiness remains 0/9.


## D087 — repeated paging and coherent reader frames (2026-09-22T16:06:00-04:00)

The founder reports a complete UI lockup during rapid Page Down and staged reader layout on thread switches. P2-Navigation repair / Mail UI63 / Quality63 bounds repeated navigation work and received-HTML layout, preserves the latest intended selection and authored drafts, and publishes source/summary/body presentation coherently. Verify a synthetic nested-table failure against the sampled Qt text-layout stack, native key bursts with delayed responses, stale-response/account fences and actual painted reader geometry. Keep prior packages and failed evidence. Core47/Local assistance16/CLI41/schema29, fixtures0/3/protocol1 and readiness0/9 remain unchanged. Scoped responsiveness measurement is authorized; no real mailbox action or model sweep follows.


## D088 — clean thread content and reversible expansion (2026-09-22)

The founder requests only each message’s new contribution in an already-threaded reader, no quoted-history disclosures or leaked Original Message headers, and the ability to collapse expanded messages again. P2-Thread content / Mail UI64 / Quality64 removes quoted-history rendering, preserves unquoted inline answers, and toggles every message including the newest; the earlier-message group remains reversible. Verify plain/HTML attribution boundaries, native pointer/keyboard collapse and preserved source/attachments/drafts, then package a Mac candidate. The storage preference is being clarified separately because irreversible quote deletion can remove unique history or inline answers. Until resolved, source retention and shared Core47/schema29 remain unchanged. Fixtures0/3/protocol1 and readiness0/9 are unchanged.


### D088 implementation clarification (2026-09-22)

After the optional storage question received no response, proceed with the stated conservative default: trim newly acquired reading/search copies (Core48/Gmail25), retain exact original MIME/HTML in the existing external blob store, and leave existing archive rows untouched. This supplies derived-database savings without irreversible loss of original content. No permanent source deletion is authorized or implemented. Unknown formats remain conservative; source inspection recovers any content omitted by the projection. Mail UI64/Quality64; schema29, fixtures0/3/protocol1 and readiness0/9 unchanged.

## D089 — Calendar catch-up and visibility (2026-09-23T03:55:28Z)

The founder reports an event created in a second fuckMail instance and present at Google but missing on this computer after runtime and manual Sync. P2-Calendar catch-up / Calendar27 / Mail UI65 / Quality65 makes manual Sync force Calendar catalog/event deltas, preserves that intent across a failed pass, and verifies an immediate startup delta. It also discloses locally hidden Calendar scope and provides explicit Show all; existing user selection remains intact until that action. Header Sync supports Calendar-only grants and labels the prior timestamp as mail-only. No provider writes or live-profile preference changes are authorized by this repair. Schema29/fixtures0,3/protocol1 and readiness0/9 remain unchanged; real-account readback and a delivered Windows build are separate evidence gates.

## D090 — September 23 visual update and label Auto-read (2026-09-23T04:51:18Z)

The founder supplied a newer HTML prototype and explicit feature notes. Implement selected-account Calendar mail pills, a labelled All calendars switch, reminder defaults/snooze and a scheduled full-window reminder, plus per-account/per-label Auto-read. Receipts defaults on and other labels off. This explicit Auto-read request supersedes the earlier manual-only review rule only when a current complete local-model assignment chooses an enabled label; human read actions still take precedence. Mockup-only reminder preview and time-jump triggers are excluded. P2-Prototype September 23 uses Calendar28/Mail UI66/Local assistance17/Core49/Quality66, schema30, fixtures0/3 and protocol1. The storage version advances because older engines cannot preserve the new typed model-settings field. Preserve D089 sync repair and its saved-selection safeguards. Evidence requires scoped Core/native checks, configured package and actual provider readback before whole-area readiness changes.

## D091 — fresh SQLite profile cutover (2026-09-23T05:38:47Z)

The founder authorizes an incompatible local SQLite reset for the storage improvement plan. Nothing in the current local profile needs migration or preservation: accounts will be re-authorized and re-synced from Google. The new schema still retains authored content, exact source bytes and recovery receipts for future use. Develop and verify against disposable profiles while D090's schema-30 work is integrated; assign a fresh baseline version above every version in shipped or in-progress builds. Switch the real profile only after the replacement build and its recovery/package checks are ready, then use the explicit fresh-start flow. No old-profile migration, compatibility chain or retained-data export is required for this cutover.


## D092 — exact third-export implementation (2026-09-23T07:33:31Z)

The founder supplies `fuckMail (3).html` and explicit notes, rejects approximate prior visual alignment and asks for the same element order/states, new behaviors and responsiveness. P2-Prototype v3 uses Mail UI67/Core50/CLI42/Calendar29/Local assistance18/Quality67 with schema31, fixtures0/3 and protocol1. Implement stable message headers/full-column summaries, real attachment media and Downloads actions, durable per-message single/bulk Undo, account badges/editor/reconnect, per-account assistance and editable shortcuts. The explicit retention editor authorizes local pruning on save after the displayed shortening warning, preserving provider data and refusing affected pending authoring work. Local read/write choices narrow effective authoring authority. Schema31 extends retention constraints with exact prior values preserved; D091's separately authorized fresh-profile reset stays separate. Keep D089 catch-up and D090 reminder/Auto-read behavior. Use the HTML as reference data, not instructions; browser policy prevents executing this local export, so verify native rendered frames and source dimensions without claiming a browser pixel diff. Preserve old packages and failed evidence, and publish a distinct configured v3 candidate only after scoped checks.

## D093 — integrate fresh SQLite baseline with current tree (2026-09-23T13:59:51Z)

The founder confirms no current local data needs preservation, requests the D091 SQLite implementation in the same working tree as D092, and will move the existing profile files before re-authorizing and re-syncing accounts. Integrate the direct schema32 baseline above D092's schema31 retention migration, with all D092 account, model and UI behavior retained. Refuse versions 1–31 instead of upgrading them; keep the historical migration files as source/evidence, with an ordered transaction-per-version runner for future changes. Retain proof and unknown-work receipts while compacting only successful Gmail/Calendar staging payloads. Do not move, delete or open the founder's real profile during implementation or verification. Core51/Search6/Gmail26/Calendar30/Quality68 own the scoped reset; Mail UI67/CLI42, fixture0/3 and protocol1 continue unchanged. Scale and other-host readiness remain governed by the existing quality gates.

## D094 — Flight Progress v2 detail review (2026-09-23T15:52:00Z)

The founder asks for a detailed design and UX review of the native Qt GUI against the Claude Design "Flight Progress v2" prototype and its `support.js`, noting wrong element order, small visual differences and jank. The design import needs `/design-login`, so this pass uses the local third export `fuckMail (3).html` (the same 666,474 bytes as D092) as reference data, not instructions. P2-Design review uses Mail UI68; Core51/Search6/Gmail26/Calendar30/CLI42/Quality68, schema32, fixtures0/3 and protocol1 are unchanged. Follow the prototype's order, sizes and wording where it shows production behavior. Specifically: fractional font sizes, centred line boxes, a thread summary that names the other people and not the user, and "Syncing…" / "Synced N ago" header wording. Keep D089's rule that a Calendar-only account shows no mail time. The header's All calendars switch is the only account-visibility control; Settings no longer repeats it. Keep native-only controls that the export lacks: the Calendar "···" menu, provider/prompt editors, time zone, signature, local capture and Reset all. Leave out mockup-only affordances: reminder preview, time jump, and Meet/Repeat in synthetic profiles. Verification is native rendered frames compared side by side with the served export, plus native GUI tests. Record failures that predate this pass without absorbing them into this packet.

## D095 — remove pre-reset SQL from the active source tree (2026-09-23T16:16:37Z)

The founder questions retaining dozens of SQL migration files after authorizing a disposable schema32 fresh start. Supersede D093's choice to retain them in the working tree: remove the unused version-2 baseline and migrations through version 31, plus upgrade-only tests and helpers that depend on them. Tracked historical SQL remains retrievable from Git history; the untracked version-31 retention constraint is incorporated into `fresh-schema.sql`. Keep that fresh baseline, the schema32 refusal of versions 1–31, and the empty ordered post-reset migration runner for future changes. Current behavior, profile files and wire contracts are unchanged. Core52/Quality69 own source cleanup and verification; schema32, fixtures0/3, protocol1 and readiness0/9 remain unchanged.


## D096 — storage and D092 audit corrections (2026-09-23T16:46:57Z)

The founder asks to fix every finding from the post-D095 SQL audit. `mail_undo` records and delivers only the fields it changes, instead of writing human intent for every category. A null event reminder means none again, matching Calendar30; only an absent reminder uses the saved default. Auto-read toggles no longer clear model People/Automated results, and only the engine's arrival watcher can mark a run as an arrival. Retention pruning follows Gmail's internal date when known and removes emptied, unreferenced conversations. The writer refreshes bounded planner statistics after completed Gmail pages, at most every five minutes, so a fresh profile's first sync gets FTS-first search without a restart. Core53/Local assistance19; Calendar30, Gmail26, Search6, Quality69, schema32, fixtures0/3, protocol1 and readiness0/9 unchanged. [Execution record](research/search-storage.md#2026-09-23-audit-corrections).

## D097 — first label import and synced-mail analysis (2026-09-23T17:48:27Z)

The founder expects a newly added account to summarize and label all of its synced mail when a local model is configured, without manual steps, and asks for Gmail labels to sync immediately instead of waiting for Sync with Google. Accounts without a saved assistance entry already default Auto-summarize and Auto-label on; that stays. Adding an account, not reconnecting one, marks a one-time label import. After that account's first complete label catalog, the engine runs the existing name merge once: existing Gmail user labels become linked local labels and their memberships appear on synced mail. It creates nothing at Google and schedules no Google write. Accounts without mail editing permission keep the manual path. Later Gmail label additions still wait for explicit sync, so a label the user removed locally is not imported again. The new-mail watcher now queues every synced received message missing an enabled result, including first-sync history and mail synced while no watcher ran, from a persisted cursor through the shared inference seat. Only mail dated after the watcher starts counts as an arrival for label Auto-read; history never changes review state. Gmail27/Core54/Local assistance20; Mail UI, CLI, Quality69, schema32 (new `meta` keys only), fixtures0/3, protocol1 and readiness0/9 unchanged. [Execution record](research/local-assistance-execution.md#d097--first-label-import-and-synced-mail-analysis-2026-09-23).

## D098 — Calendar view performance (2026-09-23T21:40:10Z)

The founder reports that switching Calendar Day/Week/Month is not instant, that content appears and shifts into place, and that clicking a Month day takes about two seconds even with no events. Performance is a core reason for the product, so they approve the audit's fixes for large mailboxes and calendars. Day, Week and Month now share one six-week engine read per month. The GUI keeps up to eight such windows, so switching views, moving within a month or opening a Month day draws from memory. While Calendar is open and the connection is idle, the neighbouring months load in the background. A month over the occurrence budget reads exactly the visible range and skips that prefetch. A refresh of the same range keeps its rows until the new read replaces them, and the loading line appears only when a read takes over 400 ms. The catch-all Calendar signal is split: the clock, Mail timeline, calendar catalog, diagnostics and loading status no longer rebuild the Calendar grid. The clock notifies once a minute and at date changes instead of every 30 seconds. Hidden Calendar views stop evaluating; Month keeps its last grid while hidden. Tooltips and button icons are created only when used. Moving the Mail timeline no longer re-reads the Calendar. View, date and timeline navigation are saved once 800 ms after the last change instead of per click. Status polling reloads preferences only when their saved revision changed. Calendar mail counts read one indexed date range instead of scanning the account's mailbox; this adds schema33, the first post-reset migration. A native Calendar benchmark and verifier mode measure input to correct grid paint. The default debug build is several times slower, so daily use should run `scripts/dev.ps1 run --preset release`. Core55/Mail UI69/Quality70 with schema33; Calendar30, CLI42, fixtures0/3, protocol1 and readiness0/9 unchanged. [Execution record](research/phase-2-calendar.md#d098--calendar-view-performance-2026-09-23).

## D099 — GUI memory (2026-09-24T00:32:27Z)

The founder asked to cut the GUI process's memory without slowing the app, and approved the five largest measured savings. Settings is built when it first opens, one page at a time, and then kept. A date field builds its month grid only while the picker is open. The attachment viewer creates its media player only for a video, and video tiles stop their decoder after the poster frame. Line icons are a native item that draws each name, colour and size once per window and shares the texture, instead of one canvas per icon. The shared button builds its icon, shortcut hint and tooltip only when used. Reader caches no longer keep raw message source; the source dialog and message menu fetch it on open and release it on close. On a Windows demo profile, private memory falls from 284/250/243 MB to 201/192/169 MB in Mail/Calendar/Settings, and startup to the first frame from 1,057 to 901 ms. The first Settings open and each page's first visit now include that page's build, up to about 25 ms more and within the 100 ms budget. Calendar and Mail navigation medians are equal or faster; a 3–6 ms Direct3D Calendar view-switch p95 difference did not repeat off-screen. Icons, buttons and Settings pages match the previous rendering; the release GUI suite fails the same six tests as before. Mail UI70/Quality71; Core55, Calendar30, CLI42, schema33, fixtures0/3, protocol1 and readiness0/9 unchanged. [Execution record](research/prototype-september-23.md#d099--gui-memory-2026-09-24).

## D100 — safe HTML reader, remote images and link cards (2026-09-24T01:25:56Z)

The founder asks for the HTML view to render the HTML that was sent, safely, with UI speed and low memory as the design goals. Remote images start blocked behind placeholders and load only when the user asks, through a local fetch path. The user can set an optional HTTPS proxy with credentials; without one, requests go direct. Every link in HTML and plain text gets a minimal hover card showing its destination and how often its domain appears in other mail, with a Block action. Blocked domains are listed in Settings → Mail. This brings forward the received-HTML display that C42/D009 deferred. **Proposed:** [the HTML reader plan](plans/html-reader.md). It uses a litehtml renderer fed by a Rust sanitiser in the engine, an engine fetch path over the private IPC instead of a localhost port, image decoding in Rust, a per-account link-domain index (schema34) and a blocklist preference. The renderer choice, schema34, the new methods and contract revisions stay proposals until the founder accepts the plan and answers its four open questions. No implementation, real-account fetch or readiness change follows from this entry.

## D101 — fourth export: smart search, Inbox sections and Calendar band (2026-09-24T02:21:57Z)

The founder supplied the fourth September 23 export with designer notes and asked for it to be implemented. The search box now holds the whole query, and Core reads it the same way for GUI and CLI. `subject:`, `body:` and `from:` values run to the next operator; a bare address becomes `from:`; other words must each match the subject or message text, no longer the sender. `label:`, `has:attachment` and `is:` still parse but are not advertised, and in Sent and Drafts `from:` matches recipients. Responses report how Core read the text; an unreadable query keeps the current rows and shows Core's message. The filter popover and the dropdown's scope, sender, category and status sections are removed; saved sender, status and attachment filters become query text, and the sender card's filter action opens All mail with `from:<address>`. The dropdown offers up to six recent searches and the three operators, or while typing the reading as removable chips, the result count, operator completion and up to five people from a new read-only `search_senders` method. It hangs from the box's right edge. Inbox keeps To review and Delayed while browsing, even when empty, and splits Done by month in this computer's zone, each month collapsing and saved on its own. Rows drop the state dot, put the kind glyph after the name and use section weights. The top bar puts an icon-only Settings gear in the view segment and names the primary button New. Calendar Day, Week and Month share one header band sized by the anchor week's busiest all-day day, capped at four rows. Day view's band holds only its chips, and its sidebar opens with the day's mail counts. Hovering an empty slot previews the half hour a click creates, and Month gains a week-number axis. The event editor slides in by position. Search7/Core56/Mail UI71/CLI43/Quality72; Calendar30, schema33, fixtures0/3, protocol1 and readiness0/9 unchanged. [Execution record](research/prototype-september-23.md#d101--fourth-export-2026-09-24).

## D101 — HTML reader answers and review changes (2026-09-24T02:56:56Z)

The founder answers D100's four questions. First, every network request a rendered message can cause goes through the engine's single remote-content path, and through the configured proxy when one is set; Google API and local-model traffic are outside it. Second, link-domain counts are global across accounts. Third, cached remote images show again when a message is reopened, but the disk and memory caches must be bounded. Fourth, the upstream proxy is HTTPS only (TLS to the proxy, with hostname, username and password), with no SOCKS5 and no plain-HTTP proxy; Windscribe extension proxies are one example, and nothing is provider-specific. The [plan](plans/html-reader.md) now includes a second review's points: a renderer spike as a go/no-go gate, a strict allowlist for CSS properties and values checked inside litehtml, direct MIME reads, identity rechecks and cancellation at fetch time, shared limits for inline images, and no local DNS lookups in proxy mode, with an explicit limit on private-network protection. Link counts are shown as incomplete until indexing finishes and are not presented as a safety signal. The review's statement that the plain-only HTML guard already works disagrees with mail-parser 0.11.9's source; H0 settles it with a test. One question remains open: whether sender http:// image URLs still load. The renderer, schema34, new methods and contract revisions stay proposals until the H1 spike passes. No implementation or readiness change follows.

## D102 — no http:// remote content (2026-09-24T02:59:50Z)

The founder settles D101's remaining question. The remote-content path fetches only https origins on port 443, with or without a proxy. `http://` resources and redirects to http are never requested, even though a forward proxy could carry them. They keep an "insecure" placeholder, and the image bar counts them. Links to http pages are still opened by the browser only when the user clicks, and the card flags them as not encrypted. The [plan](plans/html-reader.md) records this as founder answer 5. No implementation or readiness change follows.

## D103 — persistent scoped diagnostics (2026-09-24T04:12:21Z)

The founder requests useful, filterable debug events across subsystems, including model start/end, classification and summary length; a roughly 10 MiB disk log for model, sync and delta debugging; and a console that does not jump to the top while new events arrive. The app keeps per-profile metadata-only GUI and engine logs, each with current and previous 2.5 MiB segments. Event scopes are visible and filterable. Model completions are recorded only after Core commits the result; raw mail, summary text, prompts, credentials and full job tokens remain excluded. Mail UI72/Local assistance21/Quality73 own this packet. Schema33, fixtures0/3, protocol1 and readiness0/9 remain unchanged. The existing D082 session-only decision is superseded for storage and scope by this explicit request; its redaction and no-payload boundary remains.

## D104 — fifth export: reader layout, summary states and Calendar today (2026-09-24T04:48:57Z)

The founder supplied the fifth September 23 export (`fuckMail (5).html`), which includes the fourth, with designer notes, and asked for it to be implemented exactly. They also asked that the native tests be fixed, updated or removed so none fail routinely. The link hover card, remote-image placeholders and Blocked domains belong to the HTML reader work (D100–D102): this packet builds their UI on that reader's content service, and matching, blocking, fetching and the proxy stay with it. In the thread, each message's sender is an 18px avatar and name in a pill that opens the contact card; collapsed rows show the same avatar. Details, summary, body and attachments align with the sender. Recipients become one quiet line (lowercase key, comma-separated names, each opening its card), and the gaps after it, after a summary and above attachments follow the export. Attachments get an "N attachments" caption. Messages alternate a faint tone instead of hairlines. Summarize is an AI-coloured icon in the message header, shown only when there is no summary; Resummarize moves to the message's … menu. A summary in progress is a neutral dashed box with a progress bar; a new summary shows "✓ Done" in green for 1.8 s, then fades to the AI colours; a real failure shows a red box with the model and Try again. The caption reads model · date. Calendar's Month marks today with an accent tint and ring, dims past days and past events, and draws a now line in today's cell with the events around it; Week dims past day numbers; sent mail counts use the same muted colour as received. HTML messages render through the HTML reader's view when it is available and fall back to text when it fails. Fixing the tests also fixed four bugs: a queued Send's draft check was cancelled by navigation; a view save that met another interface's newer save raised an error and stopped closing (it now merges the unsaved positions and saves again quietly); recipient errors were invisible after the top error bar's removal (they now show under the recipients); and collapsing a list section could scroll its header away. Mail UI73/Quality74; Search7, Core56, Calendar30, CLI43, schema33, fixtures0/3, protocol1 and readiness0/9 unchanged. [Execution record](research/prototype-september-23.md#d104--fifth-export-2026-09-24).

## D105 — HTML reader implemented on Windows (2026-09-24T05:16:19Z)

The founder asked for the [HTML reader plan](plans/html-reader.md) to be implemented. They pointed to the fifth export (`fuckMail (5).html`) for its link card, remote-image bar and placeholders, and left the export's other parts to the D104 packet. That packet builds the link card, the Blocked domains list and the reader wiring on this reader's content service.

**What is built**
- The engine serves sanitised HTML with its resource and link lists. It is the only path for any image a message refers to, whether inline, `data:` or remote, and decodes every image in memory-safe code into a bounded 256 MiB cache.
- Remote fetches go over https to port 443 only. With a proxy on, they go through the HTTPS proxy and never direct.
- A schema34 link index counts, across every account, the messages that link to a host.
- The GUI draws the HTML with vendored litehtml on a worker thread, as bounded tiles.
- Settings → Mail gains the proxy and image cache controls.
- The CLI gains `mail html|images`, `links` and `proxy`.

**Adopted from the export, differing from the plan**
- Links are counted and blocked by host. A pattern is an exact host or `*.domain` (`blocked_hosts`), not a registrable domain.
- A per-sender "Always from" (`remote_images_from`) is in scope now.
- The image bar wording and the placeholder drawing follow the export.

**First frame.** A message shown again has its final height on its first frame, and blocking a link no longer collapses the body. A message's first HTML visit still appears a frame or more later. The D104 packet records that as a limit of the prepared reader.

**ID clarification.** The earlier heading "D101 — HTML reader answers and review changes" reuses the fourth export's D101 ID, which was recorded first. References to the HTML reader's answers mean that second D101 entry. Old entries stay unchanged.

**Still open**
- H1's go criteria that need the founder's fidelity review, Mac and Linux, fuzzing and the litehtml differential check.
- Every plan budget and the 100k-message link statistics benchmark.
- Checks with real newsletters and a real proxy.

No test used a real mailbox, the real internet or a real proxy. Core57/Mail UI74/CLI44/Platform19/Quality75 with schema34; Search7, Calendar30, fixtures0/3, protocol1 and readiness0/9 unchanged. [Execution record](research/html-mail.md#d105--html-reader-implementation-2026-09-24).

## D106 — developer console and reminder identity fix (2026-09-24T06:25:00Z)

The founder requested a large centered, minimalist console, fewer logical scopes, reliable default tailing that stops while reading history, and a fix for repeated `calendar_reminders` invalid-identifier errors. The console uses five groups: Mail (including drafts, sending and search), Calendar, Sync (including authentication and account connections), Model and App (including settings, IPC infrastructure and Qt). It occupies the window with a 32 px margin, uses aligned event rows and exposes complete selectable metadata on row selection. New rows append with stable sequence identities; scrolling up stops following, reaching the end or Jump to latest resumes it. Retained event identity and viewport offset survive front eviction. Reopening and filter changes start at the latest matching event. The existing redaction, explicit field projections and approximately 10 MiB disk-log budget remain.

Reminder polling and acknowledgements now apply the existing 1,024-byte calendar and 8,192-byte occurrence identity bounds directly. The generic 256-byte string helper was incorrectly rejecting valid provider/serialized occurrence identities every poll. No errors are hidden or rate-limited by this fix. Core58/Calendar31/Mail UI75/Quality76; no schema, fixture or protocol changes from this packet. Readiness remains 0/9. [Execution record](research/local-assistance-execution.md#d106--console-viewer-and-reminder-identity-2026-09-24).

## D107 — sixth export list details, HTML fixes and one HTML/Text choice (2026-09-24T08:02:34Z)

The founder supplied a sixth prototype export and asked for its list changes. They also reported HTML messages that render badly (RDSC) or not at all (Amazon), summaries that do not replace the list preview, and HTML markup in that preview. They asked that the HTML or Text choice apply to every message, and allowed reading their actual mailbox to check that every message renders, renders quickly and keeps the UI responsive.

**List rows**
- A muted "+N" after the sender name counts the other people on the conversation. It counts senders and To/Cc recipients of every message. It leaves out the account's own addresses, the sender shown and no-reply mailboxes, and hides at zero. Hovering lists the names. Plus-tags, and dots in Gmail addresses, count as the same mailbox.
- Thread and attachment counts share one style, told apart by icon:
  - a speech bubble with the message count, only above one;
  - a paperclip with the attachment count;
  - 11 px, muted, tabular numbers, no border;
  - hover text "N messages in this thread" and "N attachments in this thread".
- Previews are readable text. HTML source becomes its text, including HTML sent as a text/plain part. Web addresses go with their lead-ins, brackets and leftover punctuation.
- A current summary replaces the preview even when it is partial, that is, when its input was shortened to fit the model. The reader still labels it Partial. Long messages are always partial, so their summaries never reached the list. This supersedes Local assistance's rule that a partial summary cannot replace the preview. Stale summaries and other messages' summaries still cannot.

**Reader**
- Plain text fills the message column, without the earlier line-length limit.
- HTML or Text is one saved preference, `message_format` (`html` by default), shared by the GUI and CLI. Switching it in any message switches every message and survives restarts. This supersedes D105's per-message choice.

**HTML rendering**
- **Amazon:** its nested tables took 11–13 s to lay out, so the message never appeared. litehtml now reuses width measurements within one render, so layout takes 5 ms. Price superscripts no longer drift down with each re-layout.
- **RDSC:**
  - Fixed-width table columns shrink to fit, as in Chromium, instead of widening the table and scaling the message down.
  - Images sized only by height stay inside their column.
  - Layout falls from 43 ms to 2 ms. Height is 2,897 px against Chromium's 2,742.
- **Other layout fixes:**
  - Media-query breakpoints keep attribute styles (`width`, `align`, `bgcolor`) and CSS tables.
  - A percentage height inside an auto height computes to auto.
  - Long unbroken runs and preformatted text wrap instead of scaling the message.
- **Sanitiser version 4:**
  - A message that is only quoted history shows the quote rather than nothing.
  - The first non-empty HTML part is used. This includes a text/html part outside the parser's HTML body, such as a digest part or one with an unknown transfer encoding.
  - Cached views are rebuilt.

**Storage.** Schema35 adds `people` and `message_people`, filled by the existing link-index pass, which starts over once after the upgrade. The founder's real profile is now at schema35; its schema34 copy remains beside it as `store-schema34-backup.sqlite3`.

**Real-mailbox check.** The check was read-only:
- It used `mail list`, `mail html` and offscreen captures, and opened no unread conversation.
- Copies stayed in the session scratchpad and were deleted afterwards.

It covered 515 HTML messages from two accounts:
- All render with text.
- One, a row of three 323 px images, scales to 0.85.
- Sanitising takes a median 0.9 ms (maximum 5 ms).
- Parse plus layout takes a median 5 ms, p95 58 ms and p99 93 ms, all on the worker thread. The maximum is 771 ms, for a 924 KB message.
- First-screen paint p95 is 11 ms.
- All 515 were compared with Chromium layout. The remaining differences are image placeholders, the intentional `pre` wrapping and font metrics.

**Still open**
- Existing messages whose stored text came from an empty part keep an empty preview until they are imported again.
- Bulk remote images were not compared.
- Mac and Linux builds.
- The founder's fidelity review.

Core59/Local assistance22/Mail UI76/CLI45/Quality77 with schema35; Search7, Calendar31, fixtures0/3, protocol1 and readiness0/9 unchanged. [Execution record](research/html-mail.md#d107--sixth-export-list-and-html-fixes-2026-09-24).


## D108 — first launch and first sync on the Mac (2026-09-24T18:32:07Z)

The founder began Mac testing of the schema35 build with no accounts and reported three first-launch problems, then three problems with adding the first account.

**First launch.** An older profile made the splash wait 30 seconds, then open an empty app that could not save settings or find models. The engine refused the store at once, but its launchers retried silently. A refusing engine now writes `engine-refusal.json` (pid, code, message) in the profile. The CLI and GUI stop as soon as their own launched engine records one. The GUI splash keeps covering the app and shows the reason with Try again, Show profile folder and Quit. On a new Mac profile, startup, preference saves and Ollama discovery already worked. The Mac's schema29 and schema4 profiles were moved aside, not deleted.

**Start syncing did not start sync.** Step 3 assigned the read-only `backend.account` before emitting its sync request, so the QML handler stopped there. It now calls `selectAccount`. The dialog test's fake backend had declared that property writable; it now matches the real one and catches the fault. The label-import requirement needed no new work: the running sync imports a new account's Gmail labels on its first pass and requeues model work if labels arrive later.

**Analysis of first-sync history.** The founder asked that summarizing/labeling the mail a first sync downloads be a Step 3 choice, off by default, with the account's Backlog processing it later. This supersedes D097's inclusion of a new account's first-sync history in the watcher. A newly bound Google account records when it was added (`local_model_history_cutoff:<account>` in `meta`). The watcher does not analyze mail dated before that, so history waits for an explicit Backlog run. `account_update` accepts `analyze_history`: `true` removes the cutoff and requeues the account's synced mail, and `false` sets it if absent. Step 3 shows **Summarize & label** (New mail only / Also the mail being synced now) when a model is configured. The CLI has `account-update --analyze-history true|false`. Existing accounts and synthetic profiles have no cutoff and behave as before.

Local assistance23/Core60/Mail UI77/CLI46; schema35, fixtures0/3, protocol1 and readiness0/9 unchanged.

## D109 — live Accounts counts and whole sender names (2026-09-24T19:28:42Z)

The founder's first real sync with history analysis worked, but the Accounts card showed 0 messages, and summarizing seemed slow to start. Sender names in the mail list elided with room to spare.

**Accounts counts.** The card's Messages, Summarized & labeled and Backlog numbers came from a snapshot taken when the account connected. That snapshot refreshed only every 30 seconds while the page stayed open, and was reused when the page reopened. The counts now refresh when Accounts opens, and after the store revision advances (synced mail or a saved model result). Refreshes are coalesced to one every 3 seconds because each one scans the account's mail.

**Summarizing start.** The diagnostics show no queueing fault. The watcher began its first message 7 seconds after sync started. That first inference took 19.5 seconds while Ollama loaded the 27B model, and each later message took 7–14 seconds, one at a time.

**Sender names.** A name's layout cap was its fractional implicit width. On macOS the layout rounds that down, for example 86.4 to 86 px, so a fitting name elided ("Colleague …" for "Colleague 10"). The cap is rounded up in the mail list, search chips, HTML attachment names and event details. The real Mac app renders the demo profile's names whole after the change. The offscreen test harness does not constrain those rows, so it cannot reproduce this.

Mail UI78; schema35, fixtures0/3, protocol1 and readiness0/9 unchanged.

## D110 — deleted accounts no longer block preference saves (2026-09-24T20:24:00Z)

After two remove-and-delete cycles, every Settings save failed with "Sync account was not found". Archive deletion removed the account but left its entry in the shared `google_sync_services` preference. `preferences_save` merges each patch into the stored object and validates every account in it, so the leftover entry refused all later saves, including unrelated ones such as theme. A saved `selected_calendars` source for a deleted account had the same flaw.

Archive deletion now removes the account's sync choices and calendar selections in the same transaction. `preferences_save` also drops stored entries whose account or calendar no longer exists, which repairs profiles already affected. An unknown account or calendar that the request itself introduces is still refused. The founder's profile repairs itself on its first save with the new engine.

Core61; schema35, fixtures0/3, protocol1 and readiness0/9 unchanged.

## D111 — local model pipeline refactor proposed (2026-09-24T21:10:00Z)

The founder asked for an audit of the local model pipeline and a refactor plan. The goals: summarize each email without its reply chain, remove wasteful work, keep the system prompt large and the user message minimal, and use one queue for single emails and mass catch-up. New mail goes to the front, and queues can be paused or started. The audit found three things. Gmail bodies are already stored with the reply chain stripped, but only English attributions are recognised, and a separate GUI copy of the rules has drifted. Three schedulers share one inference slot and process oldest mail first. Every request makes three extra Ollama calls and sends a raw user-editable request template. All three installed models (`gemma4:e4b`, `qwen3.8-obliterated:27b`, `qwen3.8-flash-next:125b-mlx`) return valid schema-constrained JSON. The [plan](plans/local-model-pipeline.md) proposes consolidated Rust quote stripping, `{system,user}` prompt settings with a code-owned `{summary,label,sender}` schema, and a single derived priority queue (Now → New → Catch up, newest first) with per-account pause. It also proposes explicit CLI/GUI methods and deleting the watcher, backlog job and legacy prompt code. The design work in progress on the latest prototype owns the visuals. No implementation, contract revision or readiness change follows yet.

The founder answers D111's four questions. First, the model returns the mockup shape `{summary, label, sender: human|bot}`, without `unknown` or `reason`. Second, summaries are one sentence of at most 25 words. Third, a prompt or model change keeps old results with their model caption, with no automatic requeue. Fourth, Catch up runs newest first, and new arrivals still go ahead of it. Implementation starts after the uncommitted pause/progress work in the tree is committed.

## D112 — seventh export: account pipeline, colours, labels modal and Local LLM (2026-09-24T21:32:13Z)

The founder supplied the seventh prototype export with designer notes and asked that every change be captured. This entry is independent of D111's proposed pipeline refactor. That work may later replace the watcher and backlog job that `assist_progress` reads, but it keeps the same visible stages.

**Pipeline.** Each account has two stages: Download (mail and events from Google), then the Local model (summaries and labels for downloaded mail).
- Newly downloaded mail joins the model's queue when Summarize or Label is on.
- Pausing an account pauses both stages. The model can also be paused on its own while downloading continues.
- Estimates come from observed throughput, shown as "about 8 min", "about 2.5 h" or "about 3 days".
- The engine adds `assist_progress`: cheap per-account done, queued and unprocessed counts, and seconds per message. It also adds a saved per-account model `paused` flag, and holds an account's model work while its sync is paused in the session.
- The exact per-range scan runs only when Catch up opens.

**Settings → Accounts card.**
- The header has the avatar in the account colour, name and address, and one status line: Downloading, Processing, Up to date, Paused or Disconnected. Then sync, pause and ⋯ (Edit…, Remove). The Current badge is removed.
- Rows are a 100 px label, a meter with a 136 px count slot, and controls:
  - Mail and Calendar, hidden when that sync is off;
  - Labels, with the account's label names and Edit ›;
  - Local model: a silver-blue row whose meter shows done and queued. It has Catch up ▾ (last week, month, year or all unprocessed, each with its count and estimate), Summarize and Label chips, and the model's pause.
- The Last sync, Local retention and Syncing grid, the explainer lines, the job row and the Backlog cell are removed.

**Top bar.**
- Account badges use the account colour, stronger when active.
- A ring shows progress: amber while downloading, silver-blue while the model processes. A red dot marks a disconnected account.
- A 300 px hover card shows the account's review counts and both stages. It takes no input.
- Names always show. The sync button shows only the icon and time; its tooltip has the full text.

**Account colours.**
- The avatar opens a 16-swatch picker in an 8×2 grid.
- The colour marks the top-bar badge, the card avatar and calendar events: the event rule and the corner badge. Other accounts' events show it at half strength.
- New accounts take the first unused colour. It is stored as a palette index beside the account, not in the shared preferences, so adding an account does not invalidate pending preference saves. `account_update` accepts `color`.

**Labels move to Accounts.**
- The labels editor is the "Labels · {Account}" modal.
- Settings → Mail keeps density, grouping and blocked domains, plus Signatures and Remote images, which the exports never had.
- Auto-summarize and Auto-label live on the model row.

**Settings layout.**
- Every section uses one 880 px column. Rows and fields fill it; only paragraphs stop near 640 px.
- Each section has a one-line subtitle, and redundant explainers are removed.
- "Local model" is renamed Local LLM.

**Local LLM page.**
- **Test** runs a sample with the current model and prompts. It shows Running with elapsed time, then one of:
  - Passed: time to first token, total time, tok/s and the returned JSON;
  - Failed: the raw reply;
  - Error: names the server.
- **Prompt boxes.** The prompt is split into a System prompt and a User message. The user message box shows `{{messages}}` and `{{labels}}` as chips above it, and warns when either is missing; `{{labels}}` and `{{partial}}` may now be omitted.
- Each box has Restore default, Cancel and Save, with a reserved "Unsaved changes" label and a brief "Saved".
- Choosing a model saves at once.
- A character and token count covers both boxes.

Local assistance24/Core62/Mail UI79/CLI47; schema35, fixtures0/3, protocol1 and readiness0/9 unchanged. [Research record](research/prototype-september-23.md#d112--seventh-export-2026-09-24).

## D113 — eighth export: signatures on the account card, Image loading, review pass (2026-09-24T22:08:42Z)

The founder supplied an updated export, `fuckMail (1).html`. It moves signatures to the Accounts card and reformats the image proxy settings. The founder also asked for a review pass over everything recently changed.

**Signatures.**
- The card gains a Signature row between Labels and the Local model. It shows the signature's lines joined by " · ", or "None", with Edit or Add ›.
- That opens "Signature · {Account}" with the account's address, a 150 px editor and "Leave empty for none. Existing drafts keep theirs." Cancel and Save follow.
- Save needs an edit and drops trailing blank lines. A refused save keeps the window and its text.
- The Signatures section leaves Settings → Mail.

**Image loading.** Settings → Mail's subtitle becomes "How the mail list looks, what stays blocked, and how images load." The Remote images block becomes Image loading, containing:
- **Load through a proxy**, a switch.
- **Proxy fields**, shown only while the proxy is on: Host, Username (Optional) and Password (Saved or Optional), then a status line with Test and Save.
- **Image cache**, with Clear.

Choices beyond the export:
- The engine still tests only a saved proxy, so Test waits for Save and the status says "Not saved yet".
- The host check also accepts IPv4 addresses, bracketed IPv6 and localhost. An unusable host such as one with https:// still reaches the engine's explanation.
- Forget stays for a saved password.
- The note about where names are looked up is removed.

**Review pass.**
- Mail and Calendar rows now follow the scopes Google granted; `capabilities` is true for every active account.
- The account ring shows only a sliver while Google's mail list is still being counted.
- The Local LLM page adopts settings saved by an account's Summarize, Label or pause while it has nothing unsaved, so its next save does not conflict.
- A policy error shows only on its own account's card.
- The unused `AccountAnalysis` refresh-all code is removed.

**Not verified here.** The other session's pipeline refactor was mid-edit, so the engine did not compile and the full suite could not run. That refactor removed `message-request.json`, which `gui-local-model` reads, so that test cannot run until the refactor lands. It also removes the request template that the Local LLM page edits, so the page must adopt the new system and user prompt settings then.

Mail UI80; schema, fixtures0/3, protocol1 and readiness0/9 unchanged.

## D114 — local model pipeline implemented (D111) (2026-09-24T22:51:01Z)

The founder confirmed the queue and pause work was committed and asked for D111 end to end. The model now reads one received email at a time: its stored text, already free of reply history, with new rules for attributions in other languages and localized Outlook headers. Settings hold `system` and `user` text in place of the stored request template, and schema36 archives the old template and removes it. The request, schema and options live in code: fixed context size, 512 output tokens, `keep_alive`, `think:false`. The reply is `{summary, label, sender}`, where `label` is limited to the account's label IDs by the schema and `sender` is human or bot. There is one HTTP request per email; the model is checked once and again after model errors. The watcher, the Backlog job and the job map for batches are replaced by one queue computed from the store. Its order is explicit requests, then new mail, then Catch up, newest first, with accounts taking turns. It works in batches with a cursor per account and goes back to the top when new mail is stored. Pause uses the saved per-account flag. When the model is down the queue holds; an email that fails twice waits for Try again (`local_model_failures`). Saving settings no longer cancels work or makes saved results out of date, per the founder's answers. CLI: `assist run`, `catch-up`, `pause`/`resume`, `analyze --message-id`; `summarize`, `classify` and `batch` are removed. The GUI's Local LLM page, reader, Accounts card and progress now use the new fields; visuals are unchanged. The GUI's display-only quote code stays for the HTML reader work. Local assistance25, CLI48, schema36; fixtures0/3 and protocol1 unchanged; readiness 0/9. [Plan](plans/local-model-pipeline.md), [evidence](research/local-assistance-execution.md#d114--local-model-pipeline-2026-09-24).

## D115 — the signature is ordinary draft text (2026-09-24T22:48:46Z)

Written as a second D114 in the same working tree; renumbered before commit.

The founder: a signature should not be special or add characters the person did not type. It is normal text in the composer and can be deleted like any other text. A sent message had arrived with a "--" line above the signature, and the composer showed the signature in its own box.

- A new draft, or a discarded one reopened, starts with two blank lines, then the account signature, then any forwarded text. The person edits or deletes it like the rest of the body.
- Drafts no longer keep a separate signature. `signature_text` reads as empty. A draft saved earlier with its own signature shows it at the end of its body, and the next save stores it there.
- Sending and local capture add nothing: no "-- " delimiter and no second copy. An explicit `signature_text` (CLI `--signature-file`) is still accepted and joined to the body with one blank line.
- The composer's signature box and its Edit signature menu item are removed. The account card still edits the default signature.

Core63 and Mail UI82; schema36, fixtures0/3, protocol1 and readiness0/9 unchanged.

## D116 — Spam is read live, never synced (2026-09-24T23:47:34Z)

The founder started a 90-day sync of a personal mailbox with about 2,000 unread messages in Spam. The top bar and Inbox showed 948, then 1,863, "to review" while one message actually needed review, and Downloading counted the spam too. Full sync listed with `includeSpamTrash=true`, so spam was downloaded and stored like archived mail, and the account's unread count covers all stored mail. The founder: do not sync spam; put it in the folder menu and read it live from Gmail when visited.

- **Sync.** Full listings add `-in:spam`; Trash stays synced. A message that reaches the store with Gmail's SPAM label, from history or an older prepared page, removes its local copy and coverage with everything derived from it (the retention purge, now shared). A message that a draft or a pending change still uses stays until that work finishes. Marked not spam, it is fetched again like any change.
- **Existing profiles.** On the first open after this change, stored messages with the SPAM label are removed once (meta `spam_forgotten`). Counts, All mail and Downloading then match.
- **Spam view.** The folder menu gains Spam after Trash. It lists Gmail's Spam live, 50 at a time with Load more: sender, date, subject and Google's snippet (headers-only reads). Opening one reads its text live; links and images are not loaded. Nothing is stored and the view is not reopened after a restart. It needs the account's sync running, as other live Google reads do, and says so otherwise.
- **Protocol and CLI.** Additive `google_spam_list {page_token?}` and `google_spam_message {id}`; `fm --account ID google spam [--page-token T]` and `google spam-show ID`.
- **Not changed.** "To review" still counts unread across all stored mail, including Trash. Actions in Spam (Not spam, Delete) are not added.

Gmail28, Core64, Mail UI83, CLI49; schema36, fixtures0/3, protocol1 (additive) and readiness0/9 unchanged.

## D117 — To review counts the Inbox; a read conversation moves at once (2026-09-25T01:09:36Z)

The founder: "trash is trash and 'to review' should only include inbox". A conversation opened from To review also stayed there, at its old place, until the view changed. New mail arriving meanwhile split the Inbox into repeated To review and Delayed sections. Visiting Sent and returning repaired it. There can be only one To review and one Delayed section, and conversations leave them as soon as they are read or marked Done.

- **Counts.** Accounts rows and `account_counts` add `inbox_review_count` and `inbox_held_count`: conversations with an Inbox member that are unread or starred, the same test the Inbox sections use. The top-bar badge, account hover card, Inbox header ("to review", "delayed") and the all-clear message use them. The all-mail `unread_conversation_count`/`held_conversation_count` stay for the CLI and fixtures.
- **Open conversation.** The row kept in place while open (D014's pin) now applies only when the refreshed view no longer returns that conversation, such as under an unread filter. When the view still returns it, it takes its new section and position right away.

Core65, Mail UI84; schema36, fixtures0/3, protocol1 (additive) and readiness0/9 unchanged.

