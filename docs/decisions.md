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


## D118 — ninth export: calendar headers, remote search, retention badge, message notices and unsubscribe (2026-09-25T04:34:33Z)

The founder supplied `Flight Progress v2 (offline).html` with the designer's notes and asked for them implemented, together with unsubscribe "as that's a good UX feature". The notes, as built:

- **Calendar.** Day view reads "Today · Tuesday December 3 2026" (Today, Tomorrow or Yesterday only when it applies). Week view's subtitle is "30 November 2026 – 6 December 2026", years on both ends; the title stays "Week N". The "8 events · Windscribe" summary is gone.
- **Search remote mailbox.** A session-only switch at the foot of the search dropdown. While on and text is typed, the engine searches the account on Gmail (`messages.list q=`, scoped to the folder: `in:inbox`, `in:sent`, `in:trash`, `in:drafts`, or everything for All mail; `body:` words are searched everywhere because Gmail has no such operator) 600 ms after typing pauses. Threads with any message stored here are dropped; up to 25 others become list rows marked only by a cloud icon ("Not on this machine yet · fetched from the server"). They join by date; in the Inbox they sit under Done by month. The status line reads "Searching {address}…", "N more results from the server" or "Nothing more on the server", or the failure. The word Gmail is not shown.
- **Opening a remote result** reads its thread live (`threads.get` for ids, then each message's raw MIME, newest 50) into the reader: all messages, formatted text of the newest, and attachments fetched on open or save into the view cache. Nothing is stored and it is not reopened after a restart. The thread shows no reply or Done/Delay actions and is never marked read; its status line reads "Read from the server · not on this machine". This is the D116 Spam model (read live, keep nothing) rather than importing old mail past the account's retention, which the sync pipeline would refuse and reconcile.
- **Retention badge.** Each account card shows "Keeps 1 year" or "Keeps everything" after the address, from Local Retention, with the tooltip from the notes.
- **Notices above the message text.** The mailing-list row and hidden-images row form one ruled group with no fill, 36 px rows, only the rows that apply. The old images bar is removed; its proxy-failure text and Retry move into the images row. "Always load images from {sender}" uses the existing per-account `remote_images_from`.
- **Unsubscribe.** Shown on the latest message only, never on sent mail, when it has `List-Unsubscribe` or an unsubscribe link in the body. The row reads "Mailing list · N this month, N this year" (received messages from that sender in the last 30 and 365 days). One-click (RFC 8058) is an https POST of `List-Unsubscribe=One-Click` through the remote-content client (public https only, blocked hosts refused, proxy with no direct fallback). `mailto:` sends one plain email from the account through Gmail (subject and body from the URI, else "unsubscribe"); an uncertain send says to check Sent and records nothing. A header URL without one-click, or a body link, opens in the browser. Done states follow the notes. The state is stored per account, conversation and sender (schema37 `unsubscribes`).

Choices beyond the notes: the remote switch is not remembered across restarts; remote rows are excluded from bulk actions; a remote thread cannot be replied to yet (no stored Message-ID); an unsubscribe failure shows its reason in the row and the button stays for another try; the CLI gains `google search`, `google thread`, `google attachment` and `mail unsubscribe`.

Core66, Gmail29, Mail UI85, Calendar32, Search8, CLI50; schema37; fixtures0/3, protocol1 (additive) and readiness0/9 unchanged.


## D119 — developer console redesign and loaded-images state (2026-09-25T04:58:00Z)

The founder supplied the designer's "Dev console (new)" notes with `Flight Progress v2 (offline) (2).html` and asked for them implemented, along with the image-loading notes. As built:

- **Console.** It drops from the top over a dim overlay, at 76% of the window height and at least 360 px, with a short slide-in. The existing toggle rules stay: ~ or ` outside normal text fields, and in the console's own fields. Esc, `exit`/`quit`, the close button or the overlay closes it. The design follows the notes:
  - system monospace at 12/19 px;
  - a `logs`/`state` top bar with `tail -f`, pause, clear and export;
  - a filter row with level ≤ (the eight syslog levels, default info), facility (sync, gmail, cal, llm, db, net, ui, app) and a case-insensitive grep that also searches details and JSON;
  - four-column lines with the notes' level colours;
  - a `└ key=value` details line, then a coloured, selectable JSON block with copy, on click;
  - "↓ N new" while scrolled up;
  - a command line with history and the listed commands;
  - a 22 px status bar with last-hour err/warn counts.
- **Backend.** Qt messages and existing debug events map to syslog levels and a facility. Facilities come from the IPC method family, the source or the engine scope. Lines are kept in a 5,000-entry ring, and the view shows the latest 600 matching. Pause stops capture into the console while `gui.log` keeps recording; clear hides the lines retained so far. Export writes the filtered lines to Downloads as `ISO8601 <n> level facility message {details}`, with JSON pretty-printed underneath at four spaces, then shows the path.
- **JSON payloads.** Any line can carry one, but only explicit metadata projections fill it: scrubbed like messages, bounded in depth, width and size. Engine category ids already use it. The D082 rule stands: no request or response bodies, mail content, recipients or credentials.
- **Images row.** After Load images, the button becomes a green check with "N images loaded", and the row reads "Images loaded for this message only". Always load images stays. For an allowed sender, the row reads "Images from {sender} load automatically", and the button becomes "Stop always loading from {sender}". Stopping removes the sender from `remote_images_from`; images already loaded in that message stay. The icon switches to the plain picture once images show. This replaces Mail UI85's "row hides once images load".

Choices beyond the notes:
- The prompt reads `~/fuckmail` and the export file is `fuckmail-YYYYMMDDHHMM.log`; "Flight" is the prototype's placeholder name.
- The state tab shows real account addresses and the store path live, but never writes them to a file or an export.
- Log lines keep account aliases and D082 redaction.
- Console commands, their replies and the export notice are not written to `gui.log`.
- Copy report is replaced by export.
- `sync <account>` runs Sync now for an active account, or resumes a paused one.

Mail UI86; Core, CLI, schema37, fixtures0/3, protocol1 and readiness0/9 unchanged.


## D120 — tenth export: day strip, folder tabs, Spam and Trash, delete for good, now lines; fresh schema baseline (2026-09-25T05:46:44Z)

The founder supplied `Flight Progress v2 (offline) (3).html` with the designer's list of changes (the developer console rework is D119's) and asked for a "now" line on the Day and Week calendars and in the Mail day strip, saying the Month line did not show the time of day ("It's 12:35am, the now line should be almost at the top of the day"). They also asked that the earlier changes be checked, and reported that the notices row lost its image buttons and pushed Unsubscribe off screen. As built:

- **Day strip.** The default visible range is 00:00–24:00. Each day has a full-height 1 px separator at its left edge (gold at 45% for today, ink at 20% otherwise) and a marker beside it: date number (12 px, semibold, gold on today), weekday (10.5 px, uppercase, muted), then "· Today", "· Tomorrow" or "· Yesterday" when it applies. Hour labels at the edges and any label under the marker are hidden; the right-hand date label is gone and "Back to today" stays away from today. The strip's now line keeps its time label unless the marker covers it, then shows a dot.
- **Now lines.** Day and Week draw the line in today's column as before, and now scroll it into view when a range containing today opens and the remembered position would hide it. Month orders today's rows as finished timed events, then all-day and upcoming ones, so the line sits at the top just after midnight rather than under an all-day event.
- **Folder tabs.** "Inbox · Sent · Drafts | Spam Trash": Spam and Trash at 14 px after a thin divider, dimmed until chosen. The chevron menu and All mail are gone from the header (All mail stays reachable from search and the sender card). `s` still cycles Inbox, Sent and Drafts only.
- **Spam and Trash lists.** The normal row style, no row buttons, no section headings. Above the rows: "Auto-deleted after 30 days", a rule, and "Empty spam"/"Empty trash". The header reads "N in spam"/"N in trash" with a shield or bin icon and the designer's tooltip; the delayed stat hides; the filter row shows only "All N". Empty: "No spam. Nice." / "Trash is empty.". Spam stays live (D116): the list is Gmail's Spam grouped into one row per thread, and a row opens in the normal reader through the live thread read (D118). Trash stays the synced local folder.
- **Actions.** Below the last message, Spam shows "Move to Inbox" (removes SPAM and adds INBOX on each spam message; toast "Moved to Inbox · marked not spam") and Trash shows "Restore" (toast "Restored to Inbox"); both show a red-outline "Delete permanently". Deleting and emptying always confirm with the designer's text; the confirm buttons stay on one line.
- **Deleting for good** uses Gmail `messages.batchDelete`, which needs `https://mail.google.com/`. The founder chose "Ask for full access": Write-mode mail access now requests that scope. An account connected with `gmail.modify` gets `reconnect_required`, and the app offers Reconnect, which reuses Settings → Accounts' reconnect flow. Trash's local copies are removed once Gmail confirms, as retention pruning removes them; a message an open draft replies to stays until the draft is sent or discarded.

Choices beyond the notes:
- Restore now also returns received mail to the Inbox (Gmail's untrash alone lands it in All mail), so the toast is true; this replaces the earlier rule that restore changes only Trash.
- When Gmail deletes a trashed message by itself (after 30 days) the local copy leaves Trash and stays as archived local mail, as other provider deletions already do; only the person's own delete removes local copies.
- Emptying works through at most 10,000 messages per run and says how many it removed.
- The CLI gains `google bin-count`, `google not-spam`, `google delete-forever --yes` and `google empty --yes`.
- The notices row's buttons size from their labels; the text gives way first, including in a narrow reader.
- Per the founder, databases start fresh: schema37 is now the baseline with no migrations, and older profiles are refused with the existing "start fresh" message.

Core67, Gmail30, Mail UI87, Calendar33, CLI51; schema37 (baseline reset); fixtures0/3, protocol1 (additive) and readiness0/9 unchanged.


## D121 — to: search, and stable account progress (2026-09-25T06:30:00Z)

The founder asked that search gain `to:email` alongside `from:email`, with a bare address searching both and `from:`/`to:` each limited to one side, and that the search help say so. As built (Search 9, CLI52, Mail UI88):

- `from:` is always the sender, in Sent too; this replaces Revision 7's "from: is the recipient in Sent". `to:` is a To or Cc recipient; a bare address is either. The four-person limit covers all three.
- Recipients come from the link index's existing `message_people` rows, plus the message's stored first To address for mail not yet indexed. No schema change.
- In Drafts, which are all from the user, `from:`, `to:` and bare addresses name recipients.
- The dropdown's Narrow it down lists `to:` and a line saying an address alone finds mail from or to it. `to:` offers recipients to pick.

The founder also reported that the Calendar bar in Settings → Accounts animated while mail synced, although it read Complete. A calendar delta pass during mail sync made the bar drop and re-grow. A finished first pass now stays complete, and bars animate only live progress changes, never layout.

Core, Search 9, CLI52 and Mail UI88; schema37, fixtures0/3, protocol1 and readiness0/9 unchanged.


## D122 — drafts are Gmail drafts (2026-09-25T07:48:50Z)

The founder reported that Drafts showed debug material ("Saved sources", "Actions", "Original MIME, HTML and files are retained. This preview cannot be edited or sent.", "Saved Google draft sources … Export source") and asked that drafts sync perfectly with Gmail, because the new design makes drafts first-class (D123). As built:

- **Every Gmail draft is an ordinary editable draft.** The inventory scan mirrors each readable Gmail draft as a local draft tied to it (`adopted` sync state); there are no read-only `google_snapshot` drafts, source panels, copy/adopt/delete actions or "Refresh" in the GUI. Saved source history stays in Core and the CLI.
- **Gmail's edits.** When the Gmail draft changes and the local copy has no unsent edit, the local copy takes Gmail's version. A read started before our own upload cannot overwrite it (the scan fences each read by the local draft and its sync revision). An unchanged listing reads nothing; the full scan repeats every 60 s while sync runs.
- **Deleted or sent elsewhere.** A complete listing without the draft, or Gmail answering not found, removes an unchanged local copy (nothing is deleted in Gmail); a copy with unsent edits stays and uploads as a new Gmail draft.
- **Discard.** Discarding deletes the Gmail draft once Undo's window has passed (10 s). Undo after that recreates it.
- **Conflicts resolve on their own**, with the local edit winning: a changed Gmail draft is replaced, a vanished one recreated. An unconfirmed create is left alone (retrying could make a second Gmail draft), and our own in-flight create is never mirrored as a second draft.
- **What the composer says.** Only real problems, in plain words: "Couldn't save this draft to Gmail. It's kept here.", "Couldn't save this draft to Gmail: it's larger than Gmail's 35 MB limit." and "Gmail hasn't confirmed this draft yet. It's kept here." Normal syncing is silent; Check Google and Replace Google copy are gone.
- **Empty new messages** (nothing beyond the signature) are dropped when you leave them, without Undo.
- The GUI tests and self-tests for the removed source screens are retired (`gui-google-draft-snapshot-controls`, `gui-google-draft-authoring-controls`, the `--google-drafts-self-test` and `--google-draft-actions-self-test` flags). The Core/engine commands behind them remain for the CLI.

Core68, Gmail31, Mail UI89; schema37 (the draft-source foreign key to drafts is dropped in the baseline; no migration), fixtures0/3, protocol1 and readiness0/9 unchanged.


## D123 — eleventh export: first-class drafts, header, label toggles, inline compose, event view (2026-09-25T07:48:50Z)

The founder supplied `Flight Progress v2 (offline) (4).html` with the designer's 31 notes, and reported three bugs: Spam rows flashed and could not be deleted after a fresh connect, a day-strip event card opened at the top of the window, and the strip had a "more" menu that is not in the design. As built:

- **Bugs.** Spam is no longer reloaded (and emptied) by sync's store refreshes. Deleting from Spam failed locally before reaching Gmail: the engine refused the batch delete outside its mutation allowlist; the granted scope was already full access. The strip card now reads its position when it opens. The strip's more menu is gone; "Back to today" stays.
- **Labels (Settings → Accounts).** "Auto-read" is "Read"; a new "Spam" column sends mail to Gmail Spam whenever the local model applies that label (per account and label; Spam wins over Read; each message is moved at most once, so "Not spam" sticks). Intro: "Labels for {Account} only. Descriptions help the local model sort mail." CLI: `labels spam` and `labels auto-read`.
- **Mail header.** Small raised counts after Inbox (to review) and Drafts (gold, unsent drafts), hidden at zero. Spam and Trash move behind a ▾ after Drafts that lists each with its icon and count; while in one, its name shows beside ▾. A solid "New" button follows the stats; the top-right New is removed in Mail and Calendar (events: click an empty slot or press n). Narrow panes wrap the stats and New below the tabs.
- **Label filters.** Plain-text toggles (`#` name count, gold rule when on); several can be on and mail with any of them shows; none on shows all; a `label:` search decides instead. "All" is gone; Sent and Drafts day filters are toggles; Spam and Trash have none. Core takes `category` as comma-separated label ids.
- **Composing.** A new message opens inline in the reading pane in place of the thread, at least max(420 px, half the window), starting at From; the body grows. The footer holds Send, Attach, one status line and a Discard button; the title row, minimise and top bin are gone. Leaving (another thread, folder, account, Settings) keeps it in Drafts without asking; Esc on a new message with content asks "Keep this message?" (Discard, Keep editing, Save to Drafts); drafts reopened from Drafts never ask. Under a new message, "Recent with {first name}" lists up to four conversations with the first recipient and links "All mail with {name} (N)", which searches that address.
- **Draft chips.** One row at the bottom right, above the day strip: the account's unsent drafts (up to four, then "+N" to Drafts), each a small rounded chip with a gold reply or pencil icon and its subject, first words or "Untitled"; the tooltip names the account, "Reply to …" or "To …" and the start of the text. A reply draft opens in its thread; a new message opens in Drafts. The row hides while composing and the reader gains bottom space while it shows.
- **Inbox clear.** A small gold check, "Inbox clear", "Nothing to review or delayed in {account}." and, when an event is still ahead today, "NEXT · time · title ›", which opens Calendar's Day view on today with that event's card beside its block.
- **Calendar.** The event card shows up to three lines of the notes (all on hover) and Edit · View · Delete as outline buttons. View opens the full-screen event view: a countdown to the real start (m:ss → h:mm:ss → Nd h:mm:ss → Ny Nd h:mm:ss at 96/80/64 px), "Upcoming" or "In progress · Started h:mm ago" without alarm styling, no Snooze, Close, sounds only when the one-minute or start point passes while open. Ended events (View and real reminders) read "This event is over" and "Ended" with only Close. "Open event" is gone; Join shows only with a call link. The view is 720 px wide with larger type and a Notes row that scrolls past max(360 px, 36% of the height). The editor's Notes is a labelled row at least 176 px tall. Week and Month shade Saturday and Sunday; Month day numbers are 20 px; no gold on the month axes; the working-hours arrows are gone.

Choices beyond the notes: the Drafts count and chips come from Core's draft list (written drafts only); the ▾ menu counts come from Gmail when it opens; the discard confirmation says the draft leaves Gmail too; the event view, like a reminder, turns to "STARTING NOW" in its final minute.

Core68, Gmail31, Mail UI89, Calendar34, CLI53, Local assistance26; schema37, fixtures0/3, protocol1 and readiness0/9 unchanged.


## D124 — live-mailbox acceptance fixes (2026-09-25T16:40:00Z)

The founder reviewed the build against three synced real accounts (log `fuckmail-202609251225.log`) and reported eight problems. As fixed:

1. **Reminder overlay hid and reappeared with its sound.** Sync bumped each calendar source's revision about once a minute; the reminder context included it, so every pass cleared and re-showed the overlay. The context now leaves the revision out (permission, zone, selection and account state still withdraw a reminder at once), a transient failed poll (busy, timeout, a request not sent) keeps it on screen, and the overlay never sounds again for a reminder and level it has already sounded; Snooze and Dismiss forget that. The one-minute and start sounds are unchanged.
2. **Drafts.** Drafts imported by the previous build were read-only copies; the scan skipped them because their Gmail message had not changed, so they stayed read-only without From or To and could not be discarded. The scan now reads such a copy again and turns it into an ordinary editable draft, and removes copies whose Gmail draft is gone. The Drafts count counts every draft in Drafts. A draft with nothing written (no subject, recipients, attachments or text beyond the signature) is not sent to Gmail until something is, unless it already has a Gmail draft.
3. **Deleting from Spam and Trash.** The engine's second, request-level allowlist did not include `messages/batchDelete`, so the request was refused locally and reported as "Gmail could not be reached". It now admits exactly that POST at 50 units.
4. **Missing and uncommitted events.** The calendar view loads at most 128 repeating series per calendar, in id order. The founder's Windscribe calendar has 419, 375 of them finished; the Design standup series (an invitation answered Maybe) was number 374 and never shown. Series whose single RRULE ended (UNTIL) before the shown dates, and series starting after them, no longer count against the limit. Invitations answered Maybe or not answered show hatched inside a hairline with a lighter title and "Maybe" / "Not answered" in Day view, the agenda and tooltips; declined ones are dimmed and struck through and are never the strip's "next".
5. **Event card.** Edit is offered only when the account organises the event (Google does not let guests change an event), and was also switched off for every event in a calendar whose first full import had not completed. One 2023 event from another client ended when it started; the importer refused it, so the Windscribe primary calendar never completed. Timed events may now end when they start (all-day events still need a later end date), and a calendar with failed events gets one full re-read per engine run. The View button uses the shared icon layout; notes written in Google's editor show as text (line breaks, bullets and link targets kept) instead of raw HTML, in the card and the full event view.
6. **Contact card.** The card's pointer tracker was a hover handler on the popup overlay, which takes hover from the name beneath: the card closed and reopened in a loop. It now follows the pointer over the mail view; the name keeps its hover.
7. **Embedded images as attachments.** Images the HTML shows through `cid:` are part of the body, as in Gmail, and are no longer stored or listed as attachments (Content-ID images the HTML does not use still are). Attachment ids keep their part position. Stored mail from earlier builds is tidied once in the background, in batches of 50, keeping rows a draft uses.
8. **One-click unsubscribe refused.** A redirect in answer to the one-click POST now counts as accepted. If the list still refuses and its `List-Unsubscribe` also names an address, the same Unsubscribe sends the unsubscribe email through Gmail instead.

Core69, Gmail32, Calendar35, Mail UI90; schema37 (no change), fixtures0/3, protocol1 and readiness0/9 unchanged.


## D125 — console copy, selection and inline details (2026-09-25T17:00:00Z)

The founder asked for three changes to the developer console:
- a copy button beside export;
- no drag scrolling, since it is terminal-like: click-drag selects text for the usual copy shortcut;
- one-line events showing their details directly on the line, with click-to-expand only for lines that do not fit.

As built (Mail UI91):
- **Copy.** `copy` (button and command) copies the filtered lines in the export format.
- **Scrolling.** The wheel and the scrollbar scroll.
- **Selection.** Pressing and dragging selects across rows and scrolls past an edge; Ctrl/⌘+C copies the selected text.
- **Rows.** Each row is a single line with its details inline. Only a cut-off line or one with JSON expands.

Nothing about capture, levels, filters or storage changes; schema37, fixtures0/3, protocol1 and readiness0/9 unchanged.


## D126 — twelfth export: invitation answers, sender pictures, hover timing; acceptance fixes (2026-09-25T18:55:00Z)

The founder supplied `Flight Progress v2 (offline) (5).html` with 21 designer notes and, while testing the live mailbox, reported four problems. As fixed:

1. **Old drafts still read-only, not discardable, and a crash.** D124 removed an earlier build's read-only copy before publishing its replacement, but in the live store other rows point at that copy, so the removal failed and the copy stayed read-only. Clicking between such drafts made open and save call each other until the GUI ran out of stack. Now:
   - The copy becomes an ordinary editable draft in place (same row, revision +1).
   - A copy whose Gmail draft is gone is marked discarded instead of removed.
   - The GUI no longer re-enters open or save while a draft operation is running.
2. **Send switched off and on while typing.** Each autosave queued a Gmail upload, and Send was off while it ran. Now:
   - A draft goes to Gmail once its text has been unchanged for 30 s, or when it is left or sent. Local saves stay immediate.
   - Send stays enabled. Pressed during an upload, it waits up to 60 s for Gmail to accept the draft, then sends.
   - A newer local edit arriving during an upload is not an error.
3. **Reply-To ignored, sent copy duplicated, reply closed by itself.**
   - Replies fell back to the sender even when the message named a Reply-To. The sender is now used only when the headers name nobody else.
   - After the bounce, the local preview of the sent message stayed beside Gmail's copy. The thread now rechecks for Gmail's copy and drops the preview once it arrives.
   - The reply form stayed tied to the sent draft and closed when sync caught up. It now closes on send, and Reply starts a new draft.
4. **Teams meetings had no Join, and notes could not be copied.**
   - Join now also uses meeting links in the event's location or notes: Teams, Zoom, Meet, Webex, GoTo, Whereby, Jitsi and Chime, https only.
   - Notes in the full event view can be selected and copied, and their links open.

From the export:
- **Invitations.** Answer styles in Day, Week, Month and the mail day strip:
  - Maybe: striped.
  - Not answered: dashed outline.
  - Declined: struck through, and shown only with the new "Show declined" switch (off by default; `calendar_show_declined` preference).
  - Month chips use solid, dashed or dotted edges.
  - The card has a Going? Yes/Maybe/No row. No hides the event with an Undo toast for 6 s, then sends the answer to Google.
  - The card's buttons are Edit and View on the left, and Delete (or Delete this / Delete series) on the right.
- **Mail day strip.** Each event label starts with a 14 px account avatar, at 55% for past events.
- **Sender pictures.**
  - Sizes: 28 px in list rows, 18 px in thread headers, 36 px in the contact card. The participant stack stays 22 px, as the export draws it.
  - Without a picture: initials on a colour taken from the name.
  - Pictures come from the engine's `sender_avatar`:
    - An automated sender gets the brand icon of its root domain from Google's favicon service, only when 32 px or larger and never for personal-mail domains.
    - A person gets a Gravatar photo.
    - Pictures go through the image proxy and blocklist and are cached with images. A miss is remembered for a week.
  - Settings → Image loading → Sender pictures turns them off (`sender_pictures`, on by default).
  - Google Contacts/People photos would need a new contacts permission and are not fetched.
  - Synthetic profiles ask for nothing.
- **Hover timing.**
  - The contact card opens after 200 ms, switches to another name after 350 ms, and closes 220 ms after the pointer leaves. A 12 px strip above it counts as the card.
  - Link cards open after 200 ms and stay open when their link is hovered again.

Core70, Gmail33, Calendar36, Mail UI92; schema37 (no change), fixtures0/3, protocol1 and readiness0/9 unchanged.


## D127 — thirteenth export: answer invitations from the email, guest answers, meeting link; To review holds the open item; proxy for pictures (2026-09-25T21:00:00Z)

The founder supplied `Flight Progress v2 (offline) (6).html` with 14 designer notes. They also answered D126's questions and asked for one change.

Answers:
- **Sender pictures** load by default. When the HTTPS proxy is set, pictures go through it like mail images. A found picture shows at 75% opacity so it is not too bright on the dark themes.
- **Google Contacts photos:** no new Google permission for now.
- **Participant stack:** export 6 keeps it at 22 px.
- **Proxy fix:** pictures already used the image client, but a proxy saved without a password failed ("no_password"), although username and password are optional. Credentials are now sent only when one is set. Tests now cover a proxy with no credentials, pictures through the proxy, and a down proxy never falling back to a direct fetch.

Change requested: **an opened To review item stays in To review.** It is marked read at once but keeps its place until another conversation is opened, by click or Up/Down. Filing it (Done, Delay, Archive…) still moves it at once.

From the export:
- **Invitation emails.** An invite or update email is linked to its calendar event through the calendar part's UID (Google's iCalUID), read-only (`message_invite`, Core71). The CLI reads the same data with `mail invite`. The first row of the notice group holds:
  - the event's time;
  - for an update, the earlier time from the previous invite email for the same event, crossed out;
  - "Going", "Maybe" or "Declined" once answered;
  - Yes / Maybe / No, which answer the same event as the event card. No hides the event with Undo.

  Clicking the time opens Calendar's Day view with the card next to the block, first turning on Show declined for a declined event. Google adds invitations to the calendar itself and the app creates no events. Until the event has synced, the row shows the email's own time with the answers dimmed. The mockup's sample email and event are not added.
- **Guest answers.** The event editor gets a Going? row once there is more than one person. The full event view replaces "With" with a Guests row. Both list:
  - the organiser first, then guests, then you on an invitation; on your own event you come first as "Organizer · you";
  - each person's Google answer: Accepted, Maybe, Declined (struck through) or No reply, and "Invite on save" for guests just added in the editor;
  - rooms are left out.
- **Meeting URL.** The full event view shows the meeting link without https://. It is underlined, wraps rather than being cut off, and opens in the browser. The Join button shows only with a link. Labels are 112 px wide.

Core71, Calendar37, Mail UI93, CLI54; schema37 (no change), fixtures0/3, protocol1 and readiness0/9 unchanged.

## D128 — To review transitions; invitation time opens its own day (2026-09-25T23:00:00Z)

The founder reported two problems in the D127 build:
- **To review split in two.** With two items to review, opening the first and pressing Down showed "To review 0 · All reviewed", then Delayed, then a second To review holding the kept item, then Delayed again. Delaying the last item to review also behaved oddly.
- **Invitation time opened today.** Clicking the time in an invitation email's row opened Calendar on today, not on the event's day. Yes worked and Google Calendar showed the answer.

Causes:
- The kept item went back in at its old row number, which fell inside or after Delayed once the rest of the list moved.
- Section counts came from Core, which already counted the read item in Done. The header said 0 and showed "All reviewed" next to the kept item.
- Delay, Done and Undelay did not release the kept item, so a delayed item stayed in To review.
- The event linked from an email is Core's own row, which has no layout date. The Day view then fell back to today.

Decided:
- **The open item stays in To review.** It keeps its place among the To review rows that were above it, so no section is ever split. The section headers count it there and not in Done. The top "N to review" figure stays the true unread count.
- **Explicit actions move it at once.** Done, Delay, Undelay, Archive and the other filing actions move the open item to its real section immediately, and it stays open. Up and Down then go on from where it was, not from where it went.
- **No automatic advance.** Opening the next item stays a deliberate click or Up/Down, because opening marks it read.
- **An invitation email's time opens the event's own day** in the display time zone.

Mail UI94; Core71, Calendar37, CLI54, schema37, fixtures0/3, protocol1 and readiness0/9 unchanged.

## D129 — search text matches as written (2026-09-25T23:30:00Z)

The founder searched for "c-22". Their current mail client listed only threads containing that string. Ours also listed unrelated mail, and `subject:c-22` returned invitations from 22 September.

Cause: the index splits "c-22" into the words "c" and "22". Each word was searched separately, as the start of any word, so any message with a word starting with "c" and one starting with "22" matched (for example "Calendar … Sep 22").

The founder asked that a search string be an exact match, for one word or several; partial word matches are fine.

Decided (Search 10):
- **Words typed together are one phrase.** A `subject:` or `body:` value is also one phrase. The phrase must appear in that order, starting at the beginning of a word, and its last word may be incomplete: `c-2` finds "C-22", `bill c-22` finds "Bill C-22", but `c-22 bill`, `c 22` and `ill c-22` find nothing.
- **Case, Latin accents and line breaks don't matter.** Punctuation does.
- **Separate phrases each need to appear.** A phrase ends at an operator or an address; `c-22 subject:weekly` needs both.
- The index still finds candidates quickly. Core then checks the stored text whenever a phrase has more than one word or any punctuation, so single-word searches cost the same as before.
- **Drafts search matches the same way.**
- **Remote search (Gmail) is unchanged.** It still sends the text as typed, which Gmail reads word by word.

Search 10; Core, Mail UI and CLI interfaces unchanged; schema37 (no change), fixtures0/3, protocol1 and readiness0/9 unchanged.

## D130 — single-file installers for external testers (2026-09-25T22:05:00Z)

The founder is preparing external testing by people other than themselves and asked for a build pipeline that produces one file to hand out, for Windows and macOS, with minimal dependencies and simple code. Linux stays a run script for now. After comparing Inno Setup, CPack NSIS/WiX, the existing zip and MSIX on Windows, and a disk image, a zipped app and a `.pkg` on macOS, the founder chose the recommended pair. They bought an Apple Developer membership, which takes about 48 hours to activate, so both installers stay unsigned for now. Signing will be added later, locally on a Mac and in GitHub Actions; a Windows certificate may follow but is not needed.

Decided:
- **Windows: an Inno Setup installer.** Per-user, no admin prompt, installs to `%LOCALAPPDATA%\Programs\fuckMail`, with a Start menu entry, an optional desktop icon and an uninstaller. An upgrade replaces the previous program files. The MSVC runtime DLLs ship beside the executables instead of Qt's `vc_redist`, which needs admin.
- **macOS: a disk image** made with the built-in `hdiutil`, holding `fuckMail.app` and an Applications shortcut. The bundle is renamed from `fuckmail-gui.app` in the package only; the build tree is unchanged.
- **Linux keeps its `.tar.gz`.**
- **`package` builds in its own `build/<preset>-package` tree**, so it runs while an app from `build/release` is open.
- **Inno Setup 7.1.0 is the only new tool.** It is downloaded, checked against a pinned SHA-256 and installed in portable mode inside `.tools`. Its license is permissive; commercial users are asked to buy a license.
- **Install and uninstall never touch profiles.**
- **Unsigned until later:** Windows testers see SmartScreen; macOS testers use Open Anyway, and Keychain access is asked again after each update. Notarization will first need `google-oauth-client.json` moved out of `Contents/MacOS`.

Platform 20. Engine, GUI, schema37, fixtures0/3, protocol1 and readiness0/9 unchanged.

## D130 — highlight search matches; labels in alphabetical order (2026-09-25T23:59:00Z)

With D129 working, the founder asked for two things:
- highlight the matched text in subjects and message bodies;
- list labels alphabetically. The rail showed them by count, so the order looked random from one mailbox to the next.

Decided:
- **Highlights** tint every place a search phrase appears in:
  - the list's subject and preview;
  - the reader's subject;
  - the message body, both text and HTML.

  They follow the rules that decided the match: case, accents and line breaks ignored, punctuation kept, starting at a word. The tint is the theme's accent, soft on the dark themes and a little stronger on an HTML message's white paper. The prototype has no highlight style, so this one is provisional.
- **Core reports the phrases** it matched in `search_terms`. The GUI highlights them, so it never parses the search text itself. The preview now starts near the first match of a phrase.
- **Labels** read alphabetically, ignoring case, in the rail and the All labels menu. When there are more than seven, the rail keeps the seven most used and shows them in name order.

Search 11, Mail UI95; schema37 (no change), fixtures0/3, protocol1 and readiness0/9 unchanged.

## D131 — copy from the contact card; no address tooltip (2026-09-26T00:30:00Z)

The founder reported two things:
- hovering a contact showed the contact card and also a tooltip with the email address, which is redundant;
- the card's name and address could not be selected or copied.

Decided:
- Recipient names lose their address tooltip.
- The card's name, address and status line can be selected with the mouse and copied with Ctrl+C.
- While something in the card is selected, the card stays open until Esc or a click outside, so the pointer can leave it and the copy still works.
- Its conversation rows stay clickable.

Mail UI96; schema37, fixtures0/3, protocol1 and readiness0/9 unchanged.

## D132 — app icon; native title bar follows the theme (2026-09-26T05:20:00Z)

The founder supplied a yellow character as the app icon and asked for it on the installer and the running app. They then asked about replacing the default window title bar. Of the three mocked-up options (tint the native bar, merge the app header into the title bar with native controls, or draw fully custom window buttons), they chose to keep it native and match the themes.

Decided:
- **Icon.** `gui/app-icon/app-icon.png` is the source. It is the Windows exe icon (also the window, taskbar and Start menu icon), the macOS bundle icon, the Linux window icon and the installer's icon and wizard image. `scripts/make_app_icons.py` regenerates the committed `.ico` and `.icns`; only that script needs Pillow. The source is 128 px, so the largest sizes are upscaled.
- **Title bar.** The system title bar stays, with its own buttons, snapping and dragging. Its background is the window colour (`Theme.canvas`) and its controls switch between dark and light with the theme, including when the theme changes while the app is open. Windows 11 sets the caption colour; Windows 10 only switches dark or light. macOS shows the window background through a transparent title bar. Linux keeps its window manager's bar.

Platform 21. Engine, schema37, fixtures0/3, protocol1 and readiness0/9 unchanged.


## D133 — the engine CLI in the developer console (2026-09-26T06:30:00Z)

The founder asked to make the console more powerful by exposing the engine CLI there, so the engine can be used directly.

Decided:
- The console's `fm <args>` command runs the bundled `fm` against the same profile and running engine as the app. For account commands it passes the selected account. Output appears as console lines, with `--json` replies as one expandable line.
- One command runs at a time. Ctrl+C or `stop` ends it.
- `fm` output never goes to the log file, because it can contain mail content; copy and export take it like any visible line, as a deliberate user action. D082's metadata-only rule for persisted logs is unchanged.
- The CLI remains a separate process with its own lifetime; nothing new runs in the background once a command ends.

Mail UI97, CLI55. Engine, schema37, fixtures0/3, protocol1 and readiness0/9 unchanged.

## D134 — fm pauses the live console log (2026-09-26T07:10:00Z)

Trying D133, the founder typed bare `fm` and got clap's missing-command error, with sync events burying the output. Decided: bare `fm` shows `fm --help`, and running any `fm` command pauses live events in the console (the log file keeps recording) until the user resumes. Mail UI97 updated in place; nothing else changes.

## D135 — full-window console with a quick slide (2026-09-26T07:40:00Z)

The founder asked for the developer console to fill the whole screen rather than part of it, and to slide down and up on `~` very quickly, under 300 ms. Decided: it fills the window and slides in 200 ms and out in 160 ms, without the fade. Mail UI97 updated in place.

## D136 — command output scrolls the console to the bottom (2026-09-26T08:30:00Z)

The founder reported not seeing output after running a command unless they scrolled down. Decided: any typed command or reply, including `fm` output that arrives later, brings the view to the bottom and resumes following. While fixing it: `~` now reverses a slide in progress instead of being lost, and `--json` output still pending when `fm` exits joins the JSON reply rather than printing as plain lines. Mail UI97 updated in place.

## D137 — mail download progress with a real total (2026-09-26T09:00:00Z)

The founder changed both accounts from one year of mail to two. The Accounts page then showed "13.2k of 13.2k+" with a full bar and "Counting mail…" for the whole download. They asked for the amount actually being synced and a more useful bar.

Cause: while the inventory runs, the only total was the messages Google had listed so far. The inventory lists 100 IDs and downloads them before listing the next 100, so "listed" and "downloaded" grow together, and the real size is known only at the end. Gmail's `resultSizeEstimate` is not reliable enough to show.

Decided:
- **The engine counts the inventory separately.** Each inventory step also lists one page of 500 IDs with the same query and scope (the account's window, Spam left out, Trash included). The total arrives about five times faster than the download: for 27k messages, about 55 list calls of 5 quota units each. The count is best effort: a failed page is tried again next step and never stops the download. It belongs to the account's mail generation, so a new retention window counts again, and it is not repeated once complete. The engine does not save its place, so a restart counts again from the start.
- **The Accounts mail row:**
  - while counting: what is on this machine, "Counting… · 8.5k found", and a shimmering bar;
  - once counted: "13.2k of 21.4k", the share as a bar, the percentage and the time left at the current rate;
  - 99% at most until the inventory finishes, since mail arriving meanwhile can reach the total early; then "Complete".
- No schema change: the count uses Core's existing `meta` table.

Gmail34, Core72, Mail UI98; schema37 (no change), fixtures0/3, protocol1 and readiness0/9 unchanged.

## D138 — the count finishes quickly on slow downloads (2026-09-26T09:30:00Z)

On the founder's accounts, Windscribe showed "17k found" while 27.2k were already downloaded. D137 counted one 500-ID page per inventory step, so on a slow download the count crawled too. Decided:
- Each inventory step now counts page after page for up to 2 seconds, so the total arrives in about a minute however slowly the mail downloads.
- The status reads "Counting · 26k found" ("Counting…" before the first page).

Gmail34 and Mail UI98 updated in place; nothing else changes.

## D139 — the account tab's ring shows the same share (2026-09-26T10:00:00Z)

The founder saw the account tab's ring at a sliver while its hover card said 47%. The ring stayed at 8% until the inventory finished, a leftover from before D137 gave it a total. Decided: the ring shows the download's share once the count is known, and the sliver only while counting. Mail UI98 updated in place.

## D140 — the contact card counts To and Cc (2026-09-26T11:00:00Z)

The founder hovered a person Cc'd on the open thread and the card said "0 conversations": `sender_summary` counted only conversations the person sent. Decided:
- The card counts conversations the person is on as sender, or on To or Cc. Core's `conversations` gains `exact_person` beside `exact_sender`; the two cannot be combined.
- To/Cc come from the link index's people rows, plus the first To of messages it has not read yet. On the founder's largest account the count takes about 17 ms, against 10 ms for the sender alone.
- The card's link reads "All N with …" and searches the bare address, which matches the same roles.

Core and Mail UI98 updated in place; schema37 (no change), fixtures0/3 and protocol1 unchanged.

## D141 — downloading older mail is not "Syncing" (2026-09-26T11:30:00Z)

After the founder widened retention, the header's sync button read "Syncing" for the whole download. The engine already checks for new mail between download pages, and Sync already forces a check, but the label hid both. Decided:
- The mail worker reports each step's lane as `progress.mail_backfill`: true for the recent and inventory listings, false for a history check. A refresh sets it false at once.
- The button shows "Syncing" for history checks, a requested refresh and the first sync; during a download it shows the last successful check's time. Download progress stays in Accounts.

Mail UI98 updated in place; schema37, fixtures0/3 and protocol1 unchanged.

## D142 — Inbox months hide and show at once; list reads stop rescanning during sync (2026-09-26T13:00:00Z)

The founder found collapsing and expanding Done months slow. Measured on the live engine during a two-year download: an Inbox list read took 20 ms warm but 170–540 ms, sometimes 1 s, right after a sync write, because Core rebuilt every conversation's state; the GUI then reset the whole list, since the collapsed sections were part of the model's identity, and an expand always waited for Core. Decided:
- **Core.** The conversation-state change log from the perf branch (TEMP triggers on `messages` and `message_sender_kinds`, applied per changed conversation, one scan above 512) replaces the rescans. No schema change: the covering index stays on the perf branch. `mmap_size` 1 GiB and a 16 MiB page cache.
- **GUI.** Collapsing removes that section's rows at once and keeps them; expanding puts kept rows back at once. A collapsed section's rows are read on hover of its header, so its first click is instant too. The refresh that follows reconciles with Core. The list model's scope no longer includes collapsed sections, so both diff rows instead of rebuilding.
- **GUI.** MailView reads the list through one `rows` property, one JS copy per change instead of one per read.
- **Reader.** A single-message thread's delivery status sits in its message header, so read-on-open status never moves the text; hiding the thread line had let it move the reader 32 px.

Core, Mail UI98 updated in place; schema37, fixtures0/3 and protocol1 unchanged.

## D143 — new mail comes before the download of old mail (2026-09-26T13:30:00Z)

During the two-year download one account's header read "2 h": its new-mail checks started every minute but never finished. The founder's log shows each check part at about 10 s for 10 messages, then four download parts before the next. Three causes: every Gmail label change, including the local model's, re-downloaded the whole message; one engine-wide slot served every download, so a check waited behind other accounts' backfill; and a started check yielded to the download after each part. Decided:
- A message already kept here that Gmail reports changed reads only its label state (`messages.get` with `id,threadId,labelIds,historyId`) and is prepared from the retained source. Gmail never changes a message's source. One Gmail no longer has is recorded missing without a download. This also applies to listings that name kept messages.
- New-mail checks have their own preparation slot, so they never wait behind a backfill download.
- A check that has started (a partly published page, or more history pages) runs to the end before any listing continues. When it starts is unchanged: every sync interval, or at once on Sync.

Gmail and Core updated in place; schema37, fixtures0/3 and protocol1 unchanged.

## D144 — design export 7: calendar search, list and header polish (2026-09-26T14:00:00Z)

The founder asked to implement export 7 of the Flight Progress v2 design and its 16 notes. Decided:
- **Mail list:**
  - Every other row in a group gets a 2.8% ink shade, restarting in each group. This applies to Inbox, Sent, Drafts, Spam and Trash. Selection and hover take priority.
  - A read row kept in To review folds away where it is once another conversation opens (170 ms, faded out by 70% of the way), then takes its new place. Opening another meanwhile sends the first one on at once. Inbox only.
- **Thread:** a folded message gets a 3.5% ink wash under the pointer, over 150 ms.
- **Buttons** lift on hover the way CSS `brightness()` does: main actions by 1.12, every other button by 1.3, including the selected option of a segmented control. Dropdowns, checkboxes, month cells and the video seek bar are unchanged.
- **Top bar:**
  - Settings moves out of the Mail/Calendar switch to just after the search box: a 30 px outlined icon button, radius 6, solid ink while Settings is open.
  - The export's 2 px account-colour strip along the top is left out: the founder had asked earlier that day for it to be removed.
- **Composer:** "Mentions an attachment — none added" in gold when the writer's own words say attached, attachment or enclosed and nothing is attached. It is advisory only. Forwarded and quoted lines don't count.
- **Event editor:** a gold row under the times, "Overlaps Title (14:00), Title (14:15) +N".
  - It counts timed events that day overlapping the chosen times. Declined and all-day events and the event being edited don't count.
  - It is advisory and never blocks saving.
  - It reads the Calendar window already loaded, so a day that isn't loaded shows no row.
- **Settings → Mail:** "Show drafts in the corner" (Show/Hide, on by default) replaces "Group Sent and Drafts by date". Sent and Drafts are now always grouped by date, and the old preference is dropped when read.
- **Calendar search** ("Search events") has its own query and its own Recent list: at most six, kept in preferences.
  - `date:` takes YYYY-MM-DD, MM-DD, today, tomorrow, yesterday or ±N days. It offers "Go to Fri 2 Oct"; Enter opens that day in Day view and keeps the rest of the search as the filter. A date that can't be read shows how to write one.
  - `with:` takes a name or an address and matches guests and the organizer; a bare address counts as `with:`. On its own, Enter shows this month with the filter on.
  - `title:` matches titles only. Other words, typed together, match titles and notes as one phrase, by mail search's rules.
  - The dropdown lists up to 7 events: the next time of each matching event within a year, then its latest time in the past 60 days, dimmed. A click opens Day view with the event's card. With nothing found it says "No events match".
  - Text in the box filters Day, Week, Month and Agenda as you type; × clears it.
  - Core reads the query: `calendar_search` returns its reading as `terms` and gains an `around` mode. The CLI gains `calendar search --around`.
- **Verified only:** the console's full height (done in D135) and `to:`, now described as "Recipient name or address", e.g. `to:finance@example.com`.
- **Not done:** account drag-reordering. It appears in the export but not in its notes.

Calendar38, Mail UI99, Core73 and CLI56; schema37, fixtures0/3, protocol1 and readiness0/9 unchanged.

## D145 — new mail skips a history backlog (2026-09-26T17:45:00Z)

After a restart with D143 in place, one account still showed no new mail and a stuck "Syncing". Its last sync was at 2:05 AM. Its history scan was replaying a bulk change made outside the app: Dec 2024 mail moved to Trash. Every history page named about 115 changed messages, each needing a label read, so the scan would take hours. New messages sit on the last history pages, and the sync time is recorded only when a scan finishes. Decided:
- A scan still running when the next check falls due, or when Sync is pressed, is behind. The engine then lists Gmail's newest mail directly and downloads what it doesn't hold. That counts as a successful mail sync, and the scan carries on in the background.
- Label changes to mail already held still wait for the scan.

Gmail35 and Core74; schema37, fixtures0/3, protocol1 and readiness0/9 unchanged.

## D146 — no status text on Done/Delay, no flicker in search or month toggles (2026-09-26T18:15:00Z)

The founder reported three problems:
- The Calendar search dropdown hid and reappeared while typing, unlike Mail's.
- Done and Delay showed "Saved locally · waiting for Google" and similar lines that moved the reader.
- Inbox month headers had a "Collapse" tooltip, and toggling a month briefly showed "Searching…".

Decided:
- **Calendar search** keeps its last finished answer on screen while the next search runs. With words typed, its count row always shows, blank until the first answer, so the dropdown never empties between keys.
- **Mail delivery.** Changes waiting for Google or being checked show nothing. A change Google refused shows under Reply, Forward and Delay/Done, in a line always kept for it, so it moves nothing. The read-only, cached-copy and server-only notes use the same line. Nothing shows at the top of the reader.
- **Inbox months:**
  - Section headers have no tooltip.
  - "Searching…" and "Loading…" show only on the first read of an account's folder, never on refreshes. "Inbox clear" stays up while the list refreshes.
  - Collapsing the only loaded month while older mail isn't loaded keeps every header, down to the first open month with mail, instead of an empty list.

Mail UI100 and Calendar39; schema37, fixtures0/3, protocol1 and readiness0/9 unchanged.

## D147 — Empty trash removes copies of mail Gmail already deleted (2026-09-26T18:45:00Z)

The founder moved about 110,000 messages to Trash in Gmail and emptied it there. That bulk change is the history backlog behind D145. Locally, 16,446 Trash messages (12,930 conversations) stayed, because sync had not yet replayed the deletions. Empty trash read "0 messages will be deleted for good" (Gmail's own total) and removed nothing. Decided:
- Once Gmail's Trash is empty, Empty trash also removes the Trash copies here of mail Gmail no longer lists. Copies Gmail still lists elsewhere stay for sync to move. The confirm counts what is here.
- A message deleted in Gmail, or deleted here, is remembered as deleted, so the history backlog skips it with no request.
- The mail list row tooltip is removed at the founder's request.

Gmail36, Core75 and Mail UI101; schema37, fixtures0/3, protocol1 and readiness0/9 unchanged.

## D148 — People/Automated for every account, and filters that feel instant (2026-09-26T19:45:00Z)

The founder found the Windscribe account summarized and labelled but showing "?" for every sender. Its People and Automated filters matched nothing. It had 80 classification results and no People/Automated answers: editing a label's instructions deleted them all, while the results stayed "done". The founder also asked that "?" not show at all. Only mail the model hasn't read should lack a kind. Separately, People/Automated toggles weren't instant and the list visibly rebuilt. The founder asked for every way of filtering or bulk showing/hiding mail to be fast and free of jank. Decided:
- Label edits keep People/Automated answers. Store open restores any answer a saved result has.
- No mark for an unknown kind.
- Filters, labels, day chips and folders update the list in place and show at once where rows in memory allow. That means a recent visit, a hover prefetch, or narrowing the rows on screen. Otherwise a quick fade shows the wait.
- Measured: a first page with or without People takes about 125 ms through the CLI, process start included. The visible cost was the list rebuild, not the query.

Local assistance27 and Mail UI102; schema37, fixtures0/3, protocol1 and readiness0/9 unchanged.

## D149 — sending with attachments, quick previews, a steady month toggle and faster sync (2026-09-26T21:30:00Z)

The founder sent logs and reported six problems:
- A reply with three ordinary images would not send.
- A reply sent during a long sync first flashed a red "not available" error.
- Attachment thumbnails and videos took a while to appear.
- There was no sign of progress while a large message went out.
- Toggling a month repeatedly, with many months folded, sometimes made the whole list jump.
- One account showed "May 2025, 336 items" for over an hour.

Found:
- The reply quoted a Gmail Message-ID containing `=`. The MIME builder accepted only letters, digits and a few symbols in message IDs, so it refused the reply, and the same draft could not save to Gmail.
- The reader asked for the sent copy of a reply before the engine had recorded the send, and showed the refusal.
- Previews waited for the attachment's export on the store writer, behind sync. The export hashed the file three or four times, copied it and flushed it. Sent replies had previews turned off.
- A month toggle restored the clicked header for 2 s only, and a frame later. The row drawing that header could be removed during its own click. During sync the refresh often landed after 2 s and moved the list.
- The logs show 2,701 history parts and no inventory parts. Each history part held 10 items and took 1–3 s, one Gmail read and one store write per item. A running history scan blocked the inventory completely.

Decided:
- Message IDs accept every RFC 5322 atext character. A message that still can't be built says why in plain words.
- Attachments without a stated type get one from their extension.
- The reader asks for a reply's sent copy only once the engine has recorded the send.
- **Previews:**
  - The attachment lookup runs on the store; the copy runs on a worker.
  - One pass copies and hashes the file. A copy already verified in this run is reused without reading it again.
  - A sent reply previews the files it was sent from.
  - Images still show the file scaled to the tile, and videos play the file directly. Nothing is transcoded or thumbnailed.
- **Send progress.** While a reply uploads, a 2 px line under its header fills with the bytes sent, sweeping until the size is known. When Gmail accepts it, a check shows next to the time.
- **Month toggles:**
  - The toggle applies just after the click returns.
  - The header is put back after bindings settle and before the next frame paints. It holds until the refresh lands, up to 15 s, and lets go when the list is scrolled.
  - After a diff, only rows whose data changed are rebound.
- **Sync:**
  - A history scan that fell behind takes turns with the recent and inventory listings, one part each.
  - A part asks Gmail for the labels of its kept messages all at once, within the scheduler's two reads per account.
  - Items that downloaded nothing are staged in the part's commit.
- Measured: 60,000 generated conversations over 120 months in one account take 28–38 ms for an uncached first Inbox page, whether no months, half of them or all but the newest are folded. The month jump came from late refreshes, not the query.

Gmail37, Core76, Mail UI103 and Quality78; schema37, fixtures0/3, protocol1 and readiness0/9 unchanged.

## D150 — history scans skip mail deleted for good; reply and attachment polish (2026-09-26T23:30:00Z)

The founder asked why a cleanup of mail that no longer exists was still being replayed. Gmail's history is a change log: the cleanup wrote about 110,000 "moved to Trash" records and 110,000 "deleted for good" ones. The scan asked Gmail about each trashed message and got "not found" every time. The cost is paid once. Normal syncs read only changes since the last one, and a re-added account starts from Gmail's current state.

The founder also reported:
- the send line overlapped the reply's recipients;
- the viewer's arrows showed "Preview unavailable" for the next video;
- Reply sometimes left the reader halfway up the thread, and the reply sometimes closed on its own.

Found:
- The viewer treated a file as video only when its stored type said so. A video sent as a generic file type had no poster and no playback.
- Reply after a reply that had gone out, or was still going, reopened the sent draft. When Gmail accepted it, the composer closed.
- A new reply draft clears and reloads the thread. The reader was placed on the reply for about 200 ms only, and the saved scroll position then took over.

Decided:
- **Deletions read ahead.** While a history scan has pages still ahead, the engine first reads only the scan's deletions: `history.list` with `historyTypes=messageDeleted`, 500 records per page, for up to 2 s per turn, at most 250,000 IDs, held in memory for that scan. A fetch of one of those messages is recorded missing with no request, exactly as Gmail's "not found" would be. Every other change still applies. If the read fails, the scan asks about each message as before.
- **Send line** runs along the bottom of the reply.
- **Attachments** fold and unfold from their "N ATTACHMENTS" header, like a month header. The choice is per message, for the session.
- **Viewer:** a file named as a video, sent with no type or a generic one, plays as a video. An error from the previous item no longer blocks the next.
- **Reply:**
  - Reply while the last reply is on its way or sent writes a new one.
  - The reader then shows the whole reply at the end of the thread, and keeps it there until the thread's height holds for 250 ms (3 s at most) or the reader is scrolled. A reply taller than the reader shows from its top.

Gmail38, Mail UI104 and Quality79; schema37, fixtures0/3, protocol1 and readiness0/9 unchanged.

## D151 — composing under a thread, reply after discard, video posters, account cards (2026-09-27T03:30:00Z)

The founder reported, after D150:
- Reply, Discard, then Reply again opened the reply box and shut it at once. Replying was impossible until they left the thread. Forward was fine.
- Video cards sometimes showed no picture until Reply was pressed.
- The viewer's arrows still showed "Preview unavailable" for the next video.
- In Settings → Accounts, every account's labels flickered every few seconds during sync.
- Reply, Reply all, Forward and New message should open their box at once, with a short animation and nothing above it jumping.
- Reply, Reply all and Forward should never disappear, so a reply can turn into a forward.
- Discard should go back to those buttons, and a discarded new message should not stay on screen.

Found:
- **Reply after discard.** Reply reopens the discarded reply, empty but for the signature, as intended: typing or a file revives it. But the reader and the composer closed the reply on every refresh of a draft marked discarded. During sync such refreshes are constant, so the box shut at once. Forward always starts a new draft, so it was unaffected.
- **Video posters**, two causes:
  - A card checked whether it was on screen only when the reader scrolled or changed height. A card that moved into view as the thread above it laid out never loaded. Reply scrolls, so pressing it loaded them.
  - Qt clears a video's picture when its player is released. A poster survived only when its player happened to be kept, so posters came and went.
- **Viewer arrows.** The next item was loaded from bindings that still described the previous one.
- **Account cards.** Each refresh of the account list during sync replaced it whole. Every card was rebuilt, and its labels emptied and were read again.
- **Opening a box.** Creating a draft takes the engine 4–10 ms. The lag was the GUI: a new reply or forward emptied the thread, moved the reader to the top, reloaded, then scrolled down. The buttons row also vanished as the box appeared.

Decided:
- **Reply after discard.** A discard closes the reply once, at that draft revision. A reopened discarded reply stays open through refreshes, and only a new discard closes it again.
- **On-screen check.** A card re-checks whether it is on screen on any layout change above it, not only on scrolling.
- **Posters.**
  - The poster decoder runs until the first frame has been shown, however long that takes, then pauses.
  - The GUI copies that frame from the video sink into a JPEG of at most 960 px, kept for this run in a temporary folder under the profile's `cache/`, and removed when the app closes.
  - The card shows the JPEG and releases the player. Reopening the card, or scrolling back to it, shows the poster and length without decoding.
  - If the frame can't be kept, the paused player goes on showing it.
- **Viewer.** It reads the new item directly when it loads.
- **Account cards** use a keyed model of all accounts. A refresh updates each card in place.
- **Writing under a thread.**
  - Reply, Reply all and Forward stay under the thread while a message is written. The kind being written is the filled button.
  - Pressing it again brings the box into view. Pressing another while the box is open turns the message into that kind. With the box closed, each starts its own draft and the closed one stays in Drafts.
  - A save under way finishes first; a second press while that kind is opening does nothing.
  - A new draft for the thread on screen keeps the thread, its unfolded messages and scroll position.
  - The box fades in and settles 10 px up over 200 ms. The reader glides to it over 200 ms, then holds it there while the thread lays out.
- **Switching kind.**
  - The words written above the starting text (the signature, or the forwarded message) move into the new draft, above its own starting text. Formatting is not kept.
  - The old draft is discarded, without Undo, once the new one is saved.
  - A draft with files, one being sent, or one with words anywhere but above its starting text is left as it is. Its words above still come along.
- **Discarding a new message** returns to the draft or thread it was started from. With neither, the reader is empty. Undo still reopens it.

Mail UI105 and Quality80; Gmail38, Core76, schema37, fixtures0/3, protocol1 and readiness0/9 unchanged.

## D152 — one received source and bounded attachment exports (2026-09-27T03:16:49Z)

The founder asks to reduce duplicate storage while keeping the app fast, and authorizes the proposed local extraction and bounded-cache plan. Full original messages continue to download for offline use. Network-on-demand attachments are not selected.

- Keep exact original MIME once in `blobs/`, with extracted reading text, metadata and search indexes in SQLite. A received attachment records its decoded hash/size and its original source/part position; syncing no longer writes a second decoded blob.
- Decode and verify the selected received part on a worker when a preview, export or authored operation needs it. Preview/export writes one disposable copy under `exports/`, reused on subsequent requests.
- Target 512 MiB of exports, removing least recently used copies. Copies used within five minutes are protected so outstanding replies and clients can open them; locked files are retried. Maintenance runs every minute after the initial conversion, and export requests sweep at most once a minute. Recent/locked copies can temporarily exceed the target. User-saved Downloads files are outside this cache.
- Imported files, observed Gmail draft parts and frozen outgoing MIME stay durable. A draft using a received part keeps its source even if that received message is pruned. Transferring a draft to another account retains its selected attachment bytes independently.
- Schema38 adds `attachment_sources` to the existing store and upgrades schema37 in place. Older engines refuse the newer version. Convert older received attachment references one message at a time: verify the source and decoded bytes off the writer, recheck the rows, then record the reference and queue the old blob for normal startup cleanup. Missing/mismatched sources keep their existing copies. Cleanup respects other accounts, authored work and journals.

Core77, Gmail39 and Quality81; Mail UI105 and CLI commands unchanged. Fixture revisions remain live0/demo3 and protocol1. Verification and measured results belong in `docs/research/search-storage.md`; this decision alone does not establish readiness.

## D153 — the window reopens where it was left (2026-09-27T04:30:00Z)

The founder asked for the app to remember its last screen position and size and always relaunch in the same place.

Decided:
- The GUI keeps the normal (unmaximized) geometry and the maximized state in `window.json` in the profile, saved as the window settles and on quit.
- Launch applies them before the window first shows, so it does not jump.
- A place no longer on any screen falls back to the default. An oversized one is capped to its screen.
- Minimized and full-screen reopen as the last normal window.
- Kept per profile, so disposable test profiles never touch the user's placement. Test, capture and benchmark runs ignore it.

Platform 22. Engine, CLI, schema, fixtures and protocol unchanged.

## D154 — HTML or plain text per sender, plain by default (2026-09-27T05:10:00Z)

The founder asked to make the HTML/plain-text choice per sender instead of global: everything defaults to plain text, the user can choose HTML, and all of that sender's threads use that view from then on.

Decided:
- A shared preference, `html_senders`, lists `{account, email}` pairs shown as HTML. It is per account, like "Always load images from", keeping accounts separate. Up to 2,000 entries.
- Everything else is plain text, including senders who were shown as HTML under the old global default.
- The badge and Format menu change only that message's sender.
- `message_format` is retired and a stored value is ignored.
- The CLI sees the list through `preferences`; its reading commands are unchanged.

Mail UI106, Core78, CLI57, Quality82. Schema, fixtures and protocol unchanged.

## D155 — one Mail divider width for the whole app (2026-09-27T05:40:00Z)

The founder noticed each account kept its own list/reader column width. Switching accounts therefore resized the panes, adding pointless re-rendering. They asked for one global width for snappiness, stressing that performance is paramount.

Decided:
- The divider width is GUI layout, not account data. It lives in `window.json` with the window's place.
- It is applied once at startup and saved once when a drag ends.
- Account switches never touch it.
- Existing per-account `splitWidth` values are ignored rather than migrated. The first run after this uses half the window until the divider is dragged.

Mail UI107, Platform 23. Engine, CLI, schema, fixtures and protocol unchanged.

## D156 — reuse unchanged progress counts and verify blob bytes once (2026-09-27T06:07:04Z)

The founder authorizes small, low-risk improvements from the disk I/O audit, followed by a GUI memory audit. The flat blob layout stays as it is.

- Cache the small per-account model-progress result when mail, provider state, model results/failures and account state have not changed. Read policy and history/catch-up bounds freshly; unrelated calendar publications and global revisions do not cause a recount. Keep engine session/job state fresh. Conservatively invalidate around transactions, rollback and writes missed by SQLite's row hook. Bound the cache to 256 account rows.
- Keep the existing counting SQL and exact results. Active mail changes can still require full recounts; incremental counters are a separate optimization.
- Read a requested blob once from a regular-file handle, bound the read by its declared size and verify the returned bytes. Retain checksum, size and path protections. Staged received-message recovery verifies its manifest and decoded parts using those same original bytes.
- Keep durability, offline content, cleanup and cache limits unchanged. The GUI audit measures graphics and application memory separately where possible; it does not establish a leak or authorize speculative cache reductions.

Core79, Gmail40 and Quality83. CLI/IPC shapes, schema, live0/demo3 fixtures and protocol1 are unchanged by this optimization. Results and remaining limits belong in storage research.

## D157 — share reader sources and size attachment decodes for the display (2026-09-27T06:38:21Z)

The founder authorizes the GUI memory changes with the strongest expected benefit and least regression risk, while preserving fast operation.

- A cached reader source goes straight to the prepared reader as a shared QVariant map. Do not serialize it to JSON, recreate its strings/maps or replace the cache entry on a hit. Retain account, source-version, lifecycle and navigation checks. An identical cached conversation also stays in place; changed data at the same revision still refreshes it.
- Image thumbnails and saved video posters decode for the largest card in the existing grid, at the current screen's pixel density. Keep the requested size stable during ordinary window/divider resizing, preserving the image cache and avoiding repeated decodes.
- The attachment viewer requests display-density image dimensions rounded up in 128-pixel steps. This removes the fixed 2× oversampling on lower-density screens and reduces decode requests while resizing. Keep the displayed image until its asynchronous replacement is ready.
- Preserve cache capacities, navigation coverage, source content, downloads and video playback. Do not broadly unload visited views or trim SQLite memory. Savings are workload-specific; the earlier running GUI's full memory excess remains unattributed.

Mail UI108 and Quality84. Core/Gmail/CLI contracts, schema, live0/demo3 fixtures and protocol1 unchanged. Scoped measurements and failed-attempt limits are in storage research.

## D158 — refresh only changed UI projections (2026-09-27T07:00:43Z)

The founder selected the first UI responsiveness recommendation: stop a global revision from reloading every independent area, then explain how to observe the benefit.

- Add store-open generations for accounts, categories, the reader, drafts and captures to `status`, using the existing write hook. Keep mail/calendar generations and preference revisions. Unknown dependencies, missed hooks and transaction ambiguity invalidate conservatively; a new store epoch invalidates every area.
- Scope automatic refreshes by these generations. Draft and outgoing delivery updates need not re-prepare received messages. Keep explicit refresh, fallback for older engines, dirty-text and account/navigation guards, and deliberate-open semantics.
- Preserve invalidations deferred by search typing, older-page browsing or an already loading reader until they can run. Refresh draft chips and Draft/Sent lists when drafts change.
- Track areas across the store in this slice. Per-account/per-conversation change tracking is deferred; incoming mail can still refresh the selected reader. Metadata advances account freshness because last-sync timestamps live there. Conversation writes also refresh legacy capture subjects.

Core80, Mail UI109, CLI58 and Quality85. No schema migration, new scans, cache-size change or fixture/protocol revision. Verify reduced requests and freshness; do not claim an unmeasured click-latency percentage.

## D159 — Flight Progress v2 (8), a full-length list, and every Gmail message downloads (2026-09-27T08:12:00Z)

The founder asked for the updated prototype, with the designer's notes, and a list of fixes: the "Loading…" flash when a month folds, scrollbar jumps while older mail loads, the "2 unavailable" messages, the label tooltip, the Spam/Trash menu staying open, the plain text menus, the "saved locally" line and the two-step "Inbox zero". They also asked that Inbox/Sent/Drafts switch instantly. "If it's in Gmail, it should be available to us."

- **Prototype.**
  - Account tabs are 32 px blocks without a pill. An 18 px letter tile (24% tint selected, 10% otherwise) sits before the name and count, with a 2 px underline in the account colour on the selected tab. While syncing, the underline is a progress bar (at least 4%, amber downloading, silver-blue for the local model) over a tinted track. It pulses in 80 ms steps only while the app is active; no ring. The red disconnected dot stays.
  - The hover card shows whole numbers; Settings → Accounts keeps the short form. Settings is a 32 × 28 button in a tray.
  - The composer has a title (Compose Email, Reply, Reply All, Forward) with a gold icon tile.
  - Reply, Reply All and Forward scroll the thread down only as far as the box's footer plus 24 px. New messages don't scroll.
  - Draft rows lose Discard; the list's right-click menu keeps it.
- **List length.**
  - Loading every conversation at once is rejected. The founder's accounts hold 11.6k–27.8k Inbox conversations, about 1 KB each as list rows. Core caps a page at 100 rows and IPC frames at 1 MiB, so a single load would need a new protocol and would hold 12–28 MB of rows in QML.
  - Instead, the list is its full length from the first page. Unloaded rows are a spacer sized from the engine's counts: exact per Inbox section, and estimated with header spacing elsewhere. Placeholder rows draw over the visible part.
  - Scrolling near the end prefetches the next page. Dragging past the loaded rows loads straight through to that point, at most 2,000 rows per request.
  - The Load more footer and its "Loading…" text are removed. Month folding no longer changes anything below the list.
- **Speed.** Date grouping is computed once per row set, instead of once per header per row. Switching to a long Sent went from about 230 ms to 45 ms in the paging fixture. A first frame within 300 ms of switching is a GPU test gate.
- **Interaction.**
  - Label chips have no tooltip.
  - The Spam/Trash menu closes on any outside press and on a mailbox or filter change. Its own button still toggles it.
  - Every text field, message body and the composer use one app menu (Undo, Redo, Cut, Copy, Paste, Select all, and the composer's formatting) in place of Qt's default.
  - Selection actions leave no status line unless something failed or stopped.
  - The last row leaving To review is removed locally before the reload, so the empty state draws in the same frame.
  - Focus outlines follow keyboard focus only.
  - Sender photos map their texture to the circle explicitly, because Qt maps a Shape's fill item at device-pixel scale.
- **Large messages.**
  - The 40 MiB raw bound came from early allocation caps, not from Gmail. Gmail holds messages up to its 150 MB import limit and up to 500 attachments.
  - The raw bound becomes 160 MiB, and received mail accepts 500 attachments. The blob cap follows the raw bound. The transport ceiling becomes 256 MiB; every request still sets its own lower cap.
  - Raw and draft reads get a 10-minute deadline. The 30 s gap between reads still catches a stalled connection.
  - A message is held once while it downloads: the reply is read into one buffer, the source moves to its front, and base64url decodes in place. Before this, the reply, a copy of the source and the decoded bytes were all held at once.
  - A one-time migration marks earlier `item_too_large` and `attachment_limit` rows unseen, so reconciliation fetches or re-prepares them.
  - A message Gmail still sends malformed is settled quietly. Progress says "Complete"; only the count's tooltip names it.
- **Fixes.**
  - Gmail drafts made before the local draft that mirrors them import again, because the draft-source key no longer requires that draft to exist first.
  - A flaky engine test compared live refresh generations between clients; it now compares the store epoch.

Mail UI110, Core81, Gmail41 and Quality86. Schema 40 (data-only migration 040 after 039), live0/demo3 fixtures, CLI shapes and protocol1 unchanged apart from Core81's `undownloadable_messages`. The copies of large sources made while the retained-source path re-prepares a message, and streaming raw downloads to disk, remain separate work.

## D160 — readable forwards in text mode (2026-09-27T15:50:00Z)

The founder showed a forwarded message in text mode. Its lines ran together: "Thanks,KrisOn Thursday … wrote:" was one line, and "From:CMNetworx Panel" had lost its space. Nothing showed where the forward began, which part was the forwarded text, and which part was the history it quoted. HTML mode showed all three.

- **Cause.** The message was HTML only, so text mode showed our own HTML-to-text conversion. The parser's converter broke lines only at `<br>` and `<p>`. It ignored `<div>` boundaries, which Apple Mail and Proton use for every line, and it dropped whitespace between inline elements.
- **Conversion.**
  - Reading text comes from a block-aware converter: blocks on their own lines, paragraphs a blank line apart, one space between inline runs, list items marked, and each blockquote level prefixed `> ` as plain-text mail quotes.
  - Search, previews, composer quotes and the stored reading copy of new mail use it.
  - List previews drop the `>` marks.
- **Older mail.** The reader's source reply includes the reading text as this build extracts it, so messages stored before the change display the new text without rewriting the database. Their stored copy, and so their search text, stays as it was.
- **Text-mode layout of a forward:**
  - a small "Forwarded message" caption, with a rule when a note precedes it;
  - the From/Date/To/Subject block as a compact two-column table;
  - the forwarded text at the reader's normal size;
  - an "On … wrote:" line in muted small text;
  - the history it quotes muted and indented 16 px per level, with a bar per level that runs on while the quote continues.
- **Detection.**
  - A forward whose client quotes it whole, as Apple Mail does, is read from its own level.
  - A forward inside a forward starts over.
  - A sentence that merely mentions a forwarded message stays ordinary text.
  - Replies still hide their quoted history as before.

Mail UI111, Core82, CLI59 and Quality87. No schema, fixture or protocol change.

## D161 — summarize mail that is only a subject (2026-09-27T16:40:00Z)

The founder found that an email with only a subject ("Dont forget to milk the cow") would not summarize. They asked whether the model gets the subject at all.

- **Found.**
  - Every request already sends from, to, subject, date and the stored text.
  - Two checks refused mail without text before asking the model: Summarize failed with "no text to summarize", and the background queue counted only mail with a body.
- **Change.**
  - Mail with no text is analysed from its subject: the request carries the subject and empty text. Mail with neither fails with "This message has no subject or text to summarize". Sync's "(no subject)" placeholder counts as no subject.
  - The queue and progress counts include subject-only mail, so it is summarized and labelled like other mail.
  - Leading whitespace no longer goes to the model.
- **Evidence.** The built-in prompt is unchanged. With synthetic subject-only emails, the founder's local qwen3.8 and gemma4:e4b both wrote one-line "you" summaries from the subject alone and judged the sender correctly.

Local assistance28 and Quality88. No schema, fixture or protocol change.

## D162 — widening retention keeps what is downloaded (2026-09-27T17:30:00Z)

The founder widened retention on three accounts. Two downloaded older mail within hours; the third showed no progress.

- **Found.**
  - Changing retention advanced the account's mail generation and deleted its sync lanes, so sync started over. Every kept message counted as unseen, and the listing read each one again (labels plus a re-parse of its stored source) before any older mail downloaded.
  - The stalled account had 51,630 messages kept and was re-checking 49,606 of them at about 1.4 a second. Its count stood still for the whole re-check.
  - One Gmail preparation slot served every account and was held through each download's network round trip, so accounts took turns waiting on each other's latency.
- **Change.**
  - When the account has a place in Gmail history, a retention change keeps it. The history lane carries over to the new generation. Both listings restart from that place, the recent one keeping its window and the inventory taking the new one. Mail kept whole (source, index and attachments, no issue) stays seen in the new generation.
  - A listing page first settles messages already kept whole in the current generation, with no read. History pages always read, because they report changed labels.
  - Without a history place (never synced, or history expired), a retention change starts over as before.
  - When a re-check does run (after Gmail history expires), progress shows it: "Checking N%" and "Checking downloaded mail · N to go", with the bar and the account tab's underline following it.
  - Three Gmail preparation slots are shared by all accounts, besides the slot kept for new mail. The scheduler's default in-flight limit rises from 4 to 8 so three accounts can download while label reads and new-mail slots stay free.
- **Memory.** Each slot holds at most one prepared message, so a typical download adds well under a megabyte per slot. Four of Gmail's largest messages prepared at once would need about 1.5 GiB. This bounds the worst case; it is not a measurement.
- **Not measured.** The speed-up from three slots has not been measured on the founder's accounts yet.

Gmail42, Core83, Mail UI112, CLI60 and Quality89. No schema, fixture or protocol change.

## D163 — reminders that reach you (2026-09-28T12:00:00Z)

The founder heard no alarm on macOS in the last minute before a meeting, and the reminder stayed behind other windows on macOS and Windows. Separately, a ten-minute reminder appeared while they typed a reply and the next letter, "s", snoozed it; the reply lost its cursor.

- **Found.** Only Windows played the designed tones, and quietly (about a tenth of full scale). macOS and Linux called the system beep: on macOS that is the alert sound at the alert volume, often quiet or off. Nothing brought the window forward. The reminder took focus and acted on keys at once, and never gave focus back.
- **Change.** Louder tones on every platform through Qt Multimedia (PlaySound on Windows); the window comes forward once at one minute and once at the start; a one-second key guard after it appears or comes forward; focus returns on close. See Calendar40.
- **Not verified here.** Audible output and native raising need a desktop session. The build host's agent session cannot reach the macOS audio server, so the founder checks both by hand on macOS and Windows.

Calendar40. No schema, fixture or protocol change.

## D164 — a reply that stays put (2026-09-28T13:00:00Z)

The founder reported two jolts while replying. Typing a reply sometimes lost the cursor and flashed the reader. After Send, the message being answered folded away and the reply box vanished, leaving an empty reader until the sent copy drew.

- **Found.** The typing jolt was the ten-minute meeting reminder: it took focus as it appeared, and the next letter typed ("s") snoozed it 0.28 s later. Its gui.log shows the snooze and the reminder returning five minutes later. D163 fixes it. Send made the sent preview the thread's newest message, so a folded thread hid the message answered. Send also re-applied the saved view, which moved the reader to the reply's old scroll position, past the new, shorter end.
- **Change.** The message that was newest when a reply or new mail lands after it stays shown and open. The box closes in the frame its sent message appears, fading in in its place. The reader stays within the thread and shows that message whole. Send no longer re-applies the saved view. While Send waits for Gmail, the box shows a sweeping hairline. A draft lock makes the editors read-only instead of disabled, so they keep focus.
- **Verified.** gui-reply-stability types into a real reply through autosaves, Gmail draft sync, reader and list reloads and Sent refreshes, then sends and checks every painted frame. It fails on the earlier code and passes now.

Mail UI113. No schema, fixture or protocol change.

## D165 — forwards stay in their thread (2026-09-28T14:00:00Z)

The founder forwarded a message with a note. It was sent, but the thread it came from showed no sign of it; it appeared only as a new one-message conversation.

- **Found.** A forward was built as a new message: no `In-Reply-To`, no `References`, and no Gmail `threadId` on its draft or send. Gmail therefore started a new thread. The pending preview was also kept out of the source thread. No earlier decision asked for this; tests only pinned it.
- **Change.** A forward carries the source's Message-ID as `In-Reply-To`, the source's references, and the source's Gmail thread on its Gmail draft and send, like a reply and like Gmail's own forward. Its pending preview shows in the source thread and hands over to the sent copy. Forwards created before this change keep their saved headers.
- **Not verified here.** Gmail's threading of the live message is checked by the founder on the next forward; tests check the headers and thread id sent to Gmail.

Gmail43, Mail UI114. No schema, fixture or protocol change.

## D166 — no scope banner, no dimmed account menu (2026-09-28T15:00:00Z)

The founder asked to remove Calendar's "Showing N of M calendars" banner, which the prototype does not have, and to stop the Settings → Accounts "…" menu dimming the screen.

- **Change.** Calendar shows no restriction banner. The All calendars switch in the header restores every calendar, as Show all did. The account menu is an ordinary non-modal menu that closes on Escape or a click outside.

Calendar41, Mail UI115. No schema, fixture or protocol change.


## D167 — reads that survive background revisions (2026-09-29T09:00:00Z)

During acceptance testing on a mailbox of over 100,000 messages, opening new mail after a relaunch stopped marking it read, and marking it read by hand failed with "State changed; reload before applying this command."

- **Found.** Two faults. The local model, label rules and delivery readback advance a conversation's revision without changing its members, so the revision the list or reader held went stale and every read was refused. Separately, one account's saved view named a `selectedMember` that no longer exists in any account, so every view save for it failed and repeated.
- **Change.** A mail action refused as stale is retried once when the conversation's members are exactly the snapshot the action named; later arrivals still refuse it, as the journaled member snapshots require. `view_save` names the key it cannot resolve, and the GUI drops that key and saves again.

Core84, Mail UI116. No schema, fixture or protocol version change.

## D168 — designer notes: account menu, Done by week and month, sticky bands, quiet scrollbars (2026-09-29T10:00:00Z)

The founder passed on the design notes with the Flight Progress v2 export (9), and asked that sections open, close and scroll without lag on ten years of mail at over 1,000 messages a month, and that the mail list and thread scrollbars hide when idle.

- **Accounts.** The card header keeps only ⋯ ("Sync, pause, edit or remove"). Connected: Sync now (Syncing… while running; Resume and sync now when paused), Pause/Resume syncing, Edit account, Remove account. Disconnected: Reconnect, Edit account, Remove account. The whole Labels and Signature rows open their editors; "Edit ›" is a cue only.
- **Done.** This Week (from Settings → Mail → Week starts on, Monday by default, separate from Calendar's week), This Month (the rest of the month), then one section per older month. This Week and This Month start open, older months start folded, and each choice is remembered with the account's view. Folded sections build no rows: Core leaves them out of paging.
- **Headers.** To review and Delayed head their rows with a 36px band that stays at the top while its rows scroll. To review uses a circle with an exclamation mark, also in the header's "to review" count.
- **Scrollbars.** This supersedes D085 for the conversation list and the thread reader: their bar fades in on pointer movement, scrolling, hover or drag and fades out after 0.9 s at rest. Its 14px gutter stays reserved and live.
- **Speed.** Core works out each row's Done group once per list. A hover still prefetches a folded section's rows, so opening it shows them at once.

Core84, Mail UI117. No schema or protocol version change.

## D169 — instant mailbox switches, threads open on what is new (2026-09-29T16:00:00Z)

The founder reported two problems. Switching mailboxes sometimes took seconds and showed "Inbox clear" or "Nothing open" for a mailbox full of local mail. A new reply in a thread they had answered opened with their earlier messages unfolded, so they started reading mail already read.

- **Found (switching).** The engine runs every store call on one writer thread. `assist_progress`, polled every 3 s for the account tabs, recounted every message whenever mail had changed. During sync that was every poll, about 2.2 s each on the founder's 270,000 messages, reading every body. A profile of the running engine put 70% of writer time there. The GUI's single request pipe then queued 10–20 requests behind it, so a switch waited up to 5 s. Meanwhile `loading` stayed false while the switch read the account's saved view, and an account restored from memory had no thread selected, so the reader said "Inbox clear" or "Nothing open".
- **Change (switching).** `assist_progress` takes an optional `max_age_ms` (at most 60000). Counts made that recently are reused while only mail has changed; policy, cutoff and catch-up changes still recount. The account tabs' poll accepts 15 s; after the user's own catch-up, cancel or settings changes the counts are exact. A switch to an account with nothing in memory is loading until its list arrives, and the reader stays blank while a chosen thread loads. After the first list of the session, each other account's saved view is read ahead, so every switch shows rows from memory, and a list shown from memory opens its first thread without reading it.
- **Found (threads).** A thread could appear from the reader cache before its current copy. The newest reply then arrived as a later message, and the Send-stability rule kept the previously newest message, the user's own reply, open. A remembered "last opened" message also unfolded the whole thread on each visit.
- **Change (threads).** Opening a thread shows its unread messages open, or the latest alone when none are unread; everything earlier folds under "N earlier messages". The layout follows the thread until its current copy arrives (its revision reaches the list row's) or the user unfolds or folds anything. After that, mail arriving while it is open keeps the one being read, and several new messages at once all open. The remembered message and "expanded" no longer unfold threads.
- **Not measured.** Switch times on the founder's profile after the change. Tests drive a held engine reply and the warm path. The residual cost is one 2 s recount at most every 15 s while mail changes.

Mail UI118, Local assistance29, local protocol (additive `assist_progress` field). No schema or fixture change.

## D170 — spelling in the composer (2026-09-29T17:00:00Z)

The founder asked for a spell checker and autocorrect while writing, ideally the operating system's, with no new UI.

- **Change.** The composer body uses the system checker. On macOS that is NSSpellChecker; on Windows, the Windows Spell Checking API (underlines and suggestions, no autocorrect); Linux has none. Misspelled words get a dotted red underline once finished. Right-clicking one puts the system's suggestions, Ignore and Learn at the top of the existing text menu. On macOS, "Correct spelling automatically" corrects a word when a space or punctuation follows it, and ⌘Z restores it. Quoted and forwarded text, links, addresses, words with digits and all-caps words are not checked.
- **Why this way.** Qt Quick ignores underline colour and style on highlighter formats. The highlighter only marks the layout, and the composer draws the line. Saved text and HTML are unchanged by checking. Checks run on a background queue; the GUI thread waits only for right-click suggestions, at most 200 ms.
- **Not verified here.** Typing in the real app, the GPU-rendered underline, autocorrect with the system setting on and off, non-English suggestions and the Windows build. Headless runs never use the system checker; tests use a fake checker, plus an opt-in native smoke check.

Mail UI119. No schema, fixture or protocol change.


## D171 — local mail stays open when the engine is slow or gone (2026-09-30T06:00:00Z)

The founder relaunched the app on four accounts (about 356,000 messages, a 4.4 GB store). Within a minute it showed "Connecting to your local mail…" and stayed there, although mail is local. Closing the window did nothing; only a force quit worked.

- **Found.** `assist_progress` still counted on the writer thread. It reads every message of each account, bodies included. A cold count took 13.8 s on this store, where D169 had assumed about 2 s. The GUI sends one request at a time, so everything else queued behind the count. The next poll counted again because its counts were already about 14 s old. At 15 s the GUI declared the engine dead and dropped it; the engine, with no client left, exited. Nothing started it again, and there is no Reconnect control after startup. Close then refused to quit while the view was unsaved and the engine was gone.
- **Change (engine).** Counts run on a read-only connection to the store beside the writer, one count at a time. `assist_progress` waits for a count it needs for at most 250 ms (poll) or 2 s (exact), then answers with the latest counts, keeping current settings. An account never counted is left out until its first count. `complete: true` waits for every account, for clients that ask once, such as `fm assist progress`. Engine shutdown interrupts a running count.
- **Change (GUI).** A dropped connection starts the engine again after 1 s, up to three times in five minutes. After that the mail list says it is not connected, shows the error and offers Reconnect. Closing without an engine always quits: the view and settings are let go. An unsaved reply asks once and reconnects; closing again quits without it.
- **Measured on the founder's profile.** A cold poll answers in 0.37–0.57 s including engine start, down from 13.8 s. A complete count takes 6.4 s on a fresh engine and 64 ms warm. The engine exits 1.6 s after its last client while a count runs, down from 12 s.
- **Not verified here.** The GUI on the founder's profile after the change.

Local assistance30, Mail UI120, CLI (`assist progress` asks for complete counts). No schema, fixture or protocol version change; `complete` is additive.

## D172 — local mail first at launch; replying finishes Delayed mail (2026-09-30T14:00:00Z)

The founder asked for two things. Launch should always open local mail at once, whatever the state of Gmail or the keychain, with any connection trouble shown in the account bar. Replying to a Delayed thread should mark it Done, as a Settings → Mail choice that is on by default.

- **Found (launch).** At launch the GUI started each account's Google sync on its one engine connection before reading the mail view and list. The engine's session vault held one lock across every account's keychain call. A keychain prompt left open for one account, as unsigned development builds cause after each rebuild, blocked the next account's sync start behind it. The founder's log shows a 10.3 s start in one launch; in another, the start outlasted the GUI's 15 s limit and the app showed the engine as disconnected.
- **Change (launch).** The vault serializes each account on its own lock, and its refusal list takes a short lock never held across a keychain call. Starting or refreshing sync forgets a refusal without waiting for that account's open prompt. The GUI starts Google sync only after the first mail list has shown, or 3 s after connecting when none comes, and allows sync start and refresh 2 minutes before calling the engine unresponsive. The account bar's existing states carry the connection: the red dot and the Sync button's Retry, with its keychain explanation.
- **Change (reply).** Preference `reply_marks_delayed_done` defaults to true. Pressing Send on a reply or reply-all in a thread with a starred message records the thread; once Gmail accepts that send, the thread is marked Done. On screen, this is the ordinary Done with its Undo; elsewhere, the thread's current state is read and cleared the same way. A send that is undone, cancelled, refused or abandoned changes nothing. This supersedes the triage rule that sending alone never completes a thread. The user chooses it, it is on by default, and it is limited to Delayed threads.

Mail UI121, Triage (reply-completes-Delayed), Gmail vault note. Preferences add one key; no schema, fixture or protocol change.

## D173 — month headers are list items; searches open every month (2026-09-30T19:00:00Z)

The founder reported two problems. Folding "August 2025" with every month above it folded made the whole mail list flash and redraw. A search whose matches fell in several months showed each month folded.

- **Found (flash).** A section shown by its header alone (folded, empty or not loaded) was drawn inside the next row's delegate, or in the list footer after the last row. With every month above folded, the first August 2025 row carried all their headers. Folding August removed that row, so every header above was destroyed and built again in the footer. Frame tests with folded sections spread between open ones never showed it.
- **Change (flash).** Each such section is its own list item, `section:<key>`, which the backend merges among the rows in section order in the same model update. Only the toggled section's header changes item; every other header keeps its delegate and place. Keyboard and position lookups use list positions from the model. A first attempt pushed the headers from QML after each row update. That gave the list two updates in a row, and Qt's ListView then kept a stale delegate; one update is required.
- **Change (search).** While a search runs, every section starts open: the list request omits `open_months`, so Core folds no month by default. Folds made during a search last until its text changes, are not saved to the mailbox view, and clearing the search brings back the mailbox's own.

Mail UI122. No schema, fixture or protocol change.

## D174 — folded sections keep a host row; header items withdrawn (2026-09-30T21:00:00Z)

With D173's header items the founder saw folding get slower, and month labels doubled after several folds. Cancelling a search also appeared to leave its months open, and plain-text links took a sentence's final full stop.

- **Found.** Every header item was a full row delegate. The list also showed Qt ListView keeping stale delegates across repeated structural updates (`DelegateModel::cancel` warnings), which fits the doubled labels.
- **Change.** D173's header items are withdrawn; rows again draw the headers of the sections above them, and the footer those after the last row. When a section folds, its first row stays in the list as a hidden `fold_host`, re-placed on every publish while its kept rows remain. The headers drawn in it stay in the same delegate. Opening the section replaces the host with its rows; the first of them has the host's identity. A header block, and the footer, rebuild only when their section keys change; counts update in place. Hosts are never selected, navigated or opened. A section merely read ahead on hover gets no host.
- **Limit.** A month never loaded this session, opened for the first time, still moves the headers above it once, from the footer into its first row.
- **Search.** D173's search folds stand. The reported case did not reproduce in tests that type, search on the app's timer and clear as the search box does; it waits on the founder's steps.
- **Links.** Plain-text links end before trailing `. , ; : ! ? ' "`, and before a closing bracket the link did not open.

Mail UI123. No schema, fixture or protocol change.

## D175 — every Inbox list item is one row high (2026-09-30T22:00:00Z)

With D174's hosts the founder saw the scrollbar resize at random when months folded or opened, and after a fold the list could stay blank while scrolling until another tab or a lot of scrolling redrew it. They asked for a list that is fast and predictable however its contents change.

- **Found.** Qt's ListView sizes the items it has not built from the average height of those it has. Rows drawing a stack of folded headers were many rows high, rows kept for folded sections were zero, and headed rows were almost two. Each fold or scroll moved that average, so the list's length and the scrollbar swung. The view could also sit past the list's real end, and it stayed there, blank, while the user kept scrolling. The doubled labels came from a kept (open) conversation in a folded month drawing its own header, and from kept rows placed by To review, Delayed or Done rank rather than by month.
- **Change.** D174's hosts are withdrawn. Every Inbox section is a header item, and an open section with no mail an empty item. The rows of open sections follow; a folded section's rows, the kept one included, are left out. All items are one row high, so Qt's estimate is exact. A header sits at the foot of its item, and the list's top margin hides the room above the first. Headers and empty lines get light delegates of their own, which avoids D173's cost of building every header as a row. The backend builds rows and items in one model update, which avoids the stale delegates D173 saw. It also counts what is still to load, headers included, as whole rows below the list. While older mail is still to load, headers show down to the section whose rows come next, so folding the month the loaded rows end in keeps its header. Kept rows go back by section order, within their own month.
- **Search.** Switching accounts during a search carried the search's open months into the next account; that account now starts with its own folds.
- **Saved position.** A new list for the same account starts at its top, and the saved list position now follows it even when the view did not move. Before, a cleared search left the old variable-height list 84 px down on its own, and that move saved the position. The native self-test's clear-search check passed only because of that shift.
- **Cost.** In comfortable density the room above a header grows from 10 px to 18 px (16 above To review and Delayed bands), as the row height leaves it.
- **Evidence.** MailSelectionTest `listGeometry`, software and GPU. It uses 6 To review rows, an empty Delayed, three open months of 60 and 37 folded months, paging 40 at a time. Scrolling, paging, folds while scrolled, at the end and with pages still to load, quick repeated folds, new mail and a density change all keep the list's length at its items plus what is still to load. Every item near the view sits at its exact place; each of 400 to 470 frames is fully painted and inside the list's range, and no header is painted twice. A fold of 60 rows paints in 10 to 16 ms and an opening in 16 to 37 ms. Restoring the old header rule fails the pages-still-to-load case.
- **Not verified here.** The founder's mailbox. Sent and Drafts still fold days inside their rows.

Mail UI124. No schema, fixture or protocol change.

## D176 — long message text draws whole; search result heading and Show match (2026-10-01T22:00:00Z)

The founder reported a message with a long signature and legal notice whose text stayed blank until clicked and was cut off on scrolling. In search results the reader showed a gap under the subject, and Show match did nothing.

- **Found (text).** Qt's TextEdit draws only the blocks in view when its source text passes 10,000 characters (`QQuickTextEditPrivate::largeTextSizeThreshold`, measured on the HTML a body row is given), and redraws as the view scrolls. A footer with no blank lines is one paragraph, here about 5,800 px tall. Qt counts it as drawn while drawing only what was in view, so scrolling within it never redraws. With the founder's message the reader was blank at every position tested past the first screen; with that mode off it drew fully.
- **Change (text).** Each body row turns the mode off when its text is set (`MailBackend.drawWholeText`), so the whole text is drawn. The composer sets its document directly and never enters the mode; the raw-source view keeps it, since its lines are short blocks.
- **Found (heading).** The subject's box was `lineCount` title lines tall. When a search tint arrived after the subject laid out as plain text, Qt reported two lines for a one-line rich-text subject.
- **Change (heading).** The box is the text's laid-out height rounded to title lines.
- **Found (Show match).** It scrolled to the top of the first matching message, which a single-message thread already shows.
- **Change (Show match).** It scrolls the body's first match to a third of the way down the reader, falling back to the message's top. It is hidden when only the subject matches in a single-message thread. Opening a result is unchanged.
- **Also.** A navigation test dated its rows in September without fixing the date; on October 1 September became an older month and folded, so its date is now fixed.
- **Evidence.** MailSelectionTest `longBodyPaints` (software and GPU) uses twenty short paragraphs, then a 150-line notice with no blank lines. It fails before the change with every band of the view blank after scrolling, and passes after. `searchReader` covers the tint arriving late, a wrapped subject, Show match reaching a match 40 paragraphs down, and a subject-only match hiding the button.

Mail UI125. No schema, fixture or protocol change.

## D177 — the reply stays whole in view (2026-10-02T00:40:00Z)

The founder opened a reply at the end of a thread and saw the whole form with Send and Discard. As they typed, the form grew and pushed its footer below the reader. They expect the whole form and its buttons in view at all times.

- **Found.** The reader brought the form into view only while it opened. The body then grew with its text up to 400 px, whatever the reader's height, and nothing scrolled after it. Shrinking the window also left the footer below the reader.
- **Change.** The body stops growing where the whole form would no longer fit in the reader, and scrolls inside past that. While the keyboard is in the form, the reader keeps its top and footer in view as it grows, as the window changes size and as the thread above lays out. It never pulls someone reading back up the thread down to the form. The space around the body is measured after each layout, since reading it in the body's own size loops Qt's layout.
- **Also.** Done sections refreshed whenever the clock's date key changed, including the UTC date and the calendar's display zone. Between 8 pm and midnight in Toronto that re-read the Inbox needlessly and failed a calendar test. They now refresh when the date changes in the zone Core counts them in.
- **Evidence.** ReplyStabilityTest, offscreen and in a native window: at 620 px tall the reply shows whole from the start. Over 30 typed lines every drawn frame shows the form's top and Send inside the reader. Reading back to the top of the thread stays put through a new line and a resize. Before the change it failed at the first check.

Mail UI126. No schema, fixture or protocol change.


## D178 — sync resumes after a reconnect; the store says what holds it (2026-10-02T07:00:00Z)

On the D171 build the founder's four-account profile still timed out three times in two hours, each while the local model and mail sync were busy and the machine was under heavy CPU load. The GUI restarted the engine each time (D171), but Google sync stayed off on every account until one was clicked. Two accounts then asked to be signed in again.

- **Sync after a reconnect.** The GUI resumed sync once per launch, so a restarted engine never got it back. Now the accounts whose sync this window ran are resumed on the engine that reconnects, with the startup checks (connected, a service on). Sync the user paused stays paused.
- **Queue lookup.** The model queue's "next messages" query compared a computed date, so it read every message of the account each time. Its date rules now also bound `sent_at` as text, one second wider than the exact test, so the `messages_order` index limits the read. Read-only on the founder's store: 48–63 ms down to 2–6 ms per account and class, warm, same rows.
- **Slow store steps are logged.** Any writer step over a second writes `Store slow_step` to `engine.log`, naming the request method or the code that queued the call. Mail page timings include downloads, so they could not show whether the store was the hold-up.
- **Sign-in again.** Both accounts' refreshes were rejected by Google's token endpoint (or found no stored credential); the engine does not drop tokens on a timeout. Unverified hypothesis: an OAuth client in Testing status, whose refresh tokens expire after seven days; the profile is seven days old.
- **Not found yet.** What held the writer for 15 s. The slow-step log is for the next occurrence.

Local assistance31, Mail UI127. No schema, fixture or protocol change.

## D179 — every event shows in the Mail timeline (2026-10-02T17:00:00Z)

The founder's Mail timeline left out "How to Grow Growth" (12:00–12:30), which Day view showed.

- **Found.** The strip gave each event a row by the full width of its title, not its time, and showed three rows. Long titles at 10:30 and 11:30 filled every row past 12:00, so the 12:00 event went to a hidden fourth row with nothing to say so.
- **Change.** A title that fits keeps its row as before, and a fourth row only takes events that overlap three others in time. Otherwise an event shares the row it fits by time where the title before it loses least; that title is cut short with an ellipsis where the event starts, and its tooltip keeps it whole.
- **Evidence.** CalendarSearchTest lays out the founder's six events of that day at 1280 px. Before the change "How to Grow Growth" was hidden; after it all six show, no two in a row overlap and none is hidden.
- **Open.** Four events at the same time still hide the fourth; the strip has no overflow mark yet.

Calendar42. No schema, fixture or protocol change.

## D180 — format bar always shown; bulleted and numbered lists (2026-10-02T18:30:00Z)

The founder found that the composer's B/I/U bar appeared only with a selection, which moved the form as it came and went. They asked for it to be always shown and usable, with bulleted and numbered lists.

- **Change.** The bar is always under an editable draft's body, and it adds Bulleted and Numbered list buttons. A style is shown on where the selection or caret has it. With nothing selected, Bold, Italic and Underline apply to what is typed next, joined to the typing's undo step. Lists toggle on the selected paragraphs and continue a list of the same kind just above. Return on an empty item and Backspace at an item's start end the item's list; Qt's editor would otherwise add an empty item and join paragraphs. Lists are one level, since the sanitised HTML keeps no list styles to nest by.
- **Plain text.** Qt's plain text drops list markers. The draft's plain text, which is authoritative on reload and is sent as the text part, now puts "• " or "1. " before items, as the engine writes received lists. Reload compares it the same way, so a saved list keeps its HTML.
- **Evidence.** ComposerInputTest: the bar shows with nothing selected, and selecting text leaves the form's height unchanged. Bold chosen at the caret styles the typed words and not those around them. ⌘⇧8 turns two paragraphs into "• First\n• Second"; Return adds an item, and Return on an empty one ends the list. A list saves and loads unchanged, Numbered numbers and un-numbers the chosen items in one undo step, and Backspace at an item's start ends its item.

Mail UI128. No schema, fixture or protocol change.

## D181 — one-line mail text stays on one line (2026-10-02T20:45:00Z)

The founder's Inbox drew an OpenArt row over its neighbours.

- **Found.** That message's stored subject ends with a line break before an emoji. List rows show subjects as plain text without wrapping, but a plain-text Text still breaks at a literal newline. The row's text became two lines tall, centred in a one-row delegate, and spilled over the rows around it. Summaries had the same exposure, since only snippets had their whitespace collapsed.
- **Change.** Text shown on one line reads line breaks as spaces (`Theme.oneLine`), and the list row's subject, sender and preview are capped at one line.
- **Evidence.** MailSelectionTest `oneLineRows`: a subject and a summary with line breaks each read on one line and stay inside their row. With the old list row the test fails at the subject.
- **Not changed.** The stored subject keeps its line break; the engine's header unfolding was not examined.

Mail UI129. No schema, fixture or protocol change.

## D182 — To review and Delayed headers; starred contacts (2026-10-02T21:30:00Z)

The founder supplied an October 2 offline Flight Progress export ([research](research/flight-progress-v2.md#october-2-offline-export--d182)) with notes that replace the earlier item on section headers. They also pointed out that starring a sender, shown in earlier mockups, was never built.

- **Headers.** To review and Delayed drop the full-width band, the uppercase label, the boxed count and the pinned header. They become a 40px row: icon, 13px weight-500 label in full ink, an ink-8% count chip, and the chevron. They scroll away with their rows. Date groups are unchanged. This supersedes D168's band and sticky header.
- **Starred contacts.** Starred from the contact card, kept per account and lowercase address in a new preference, `starred_senders`, shaped and validated like `html_senders`. The engine writes it, and the CLI reads and saves it like other preferences. Such a sender's list rows show an amber star in place of the kind glyph, and the reader shows the star beside the name. Your own card has no star. This sender star is separate from message stars, which stay Delay (C17); it changes no state, count or filter.
- **Evidence.** MailSelectionTest: To review's header is 40px with a count chip and scrolls with its rows, and nothing is pinned. Starring Priya from her card saves `{account, email}`, turns the card's star filled and shows a star by her name in the reader; starring again removes both. Your own card has no star. A starred sender's list row shows the star. fm-core `starred_senders_are_per_account_lowercase_and_durable` checks the default, the round trip and rejections (uppercase, repeats, no address, missing email, not a list).

Mail UI130, Core85, CLI61. No schema, fixture or protocol change.

## D183 — forwards keep their files and show in their thread at once (2026-10-02T23:00:00Z)

The founder forwarded a message. Sent had it at once, but the thread it came from showed nothing until Gmail's copy synced. They also found that a forward left out the message's attachments, and asked for them to come along as removable attachments, as when writing a reply.

- **Found.** D165 sends a forward into its source's Gmail thread, but the engine's outgoing list still linked only replies to their conversation. The GUI's own preview showed the forward in the thread until the engine's list replaced it just after Send, and then it was gone until sync. Forward drafts started with no attachments, as Core12 chose before forwarding could send anything.
- **Change.** `google_sent` lists a forward with its source conversation, like a reply. A forward draft starts with the source message's retained attachments, in message order, as many as fit 50 files and 25 MiB. They show as the draft's attachment chips, and the writer removes any or all before sending (CLI: `draft save --attachment` or `--clear-attachments`). Replies still start with none. Switching between Reply and Forward still moves the words written unless the draft has files the writer added; a forward's starting files don't count.
- **Evidence.** fm-core: a forward of a message with a file starts with it and its capture carries the file's bytes; a forward saved without it captures none; replies start with none. The Google send test lists the reply, reply-all and forward in their thread before Gmail's copies arrive, and fails on the earlier query. MailSelectionTest: a forward shows both of the message's files, removing one saves the draft without it, Reply then retires the forward and takes the words, and a new Forward starts with both files again. That check fails on the earlier switch rule.
- **Not verified here.** A live forward with attachments through Gmail; the founder checks it on the next real forward.

Core86, Mail UI131, CLI62. No schema, fixture or protocol change.

## D184 — a smaller reader subject (2026-10-02T23:30:00Z)

The founder passed on a design note: the subject at the top of the reader goes from 28px to 21px in Inbox and Sent, with slightly looser lines so long subjects still wrap cleanly ([research](research/flight-progress-v2.md#later-note-reader-subject--d184)).

- **Change.** The reader subject is 21px, weight 500, letter-spacing −0.02em, on 1.25 lines (26px at the default text size). Its box is still whole lines of its laid-out text. The label chip beside it keeps its place.
- **Evidence.** MailSelectionTest's search reader checks the subject box is exactly one line in the wide reader and exactly its wrapped lines in a narrow one, plain and tinted. Its subject was lengthened so it still wraps at 21px.
- **Not verified here.** The founder looks at it on screen.

Mail UI132. No schema, fixture or protocol change.

## D185 — Reply after a forward; a message's own Reply (2026-10-02T23:45:00Z)

The founder forwarded a message, then found every Reply went to the person they forwarded it to. The thread's Reply did, and so did Reply in the menu of the message they wanted to answer. That menu also had no Reply all, though the message was copied to someone.

- **Found.** Reply, Reply all and Forward under a thread answer its newest message. Once Gmail's copy of the forward synced, that was the forward. A reply to your own message goes to its To, so it went to the forward's recipient. The menu on each message acted on the whole thread, not the message it was opened on. Separately, the request for a message's headers could be dropped or cancelled when the thread reloaded, for example as a draft opened. That message then never showed its To and Cc, or showed "Message source is unavailable".
- **Change.** Under a thread, Reply, Reply all, Forward and their keys answer the newest message that isn't a forward you sent (your message whose subject starts with "Fwd:" or "Fw:"). A message's own "…" menu answers that message: Reply, Forward, and Reply all when it has someone besides you and the sender on To or Cc (Cc only, for your own message). Switching between Reply and Forward keeps the message being answered. A message's header request survives a reload of the same account and is matched to its message when it returns.
- **Evidence.** MailSelectionTest builds a thread where your forward follows a message Cc'd to someone. Forward and Reply under the thread both answer the message, not the forward; this check fails on the earlier rule. The message's own menu shows Reply all and starts one to it. The forward's own menu has no Reply all, and its Reply answers the forward. The same test caught the dropped header request.

Mail UI133. No schema, fixture or protocol change.

## D186 — copy the reader's header text (2026-10-02T23:59:00Z)

The founder asked for a message's subject, sender name and address, and its summary to be selectable and copyable.

- **Found.** The subject and the sender name were plain display text. The address could be selected, but never took focus, so ⌘C did nothing, and right-click had no menu. The summary selected and copied, but had no right-click menu.
- **Change.** The reader subject, an open message's sender name and address, and a summary select with the mouse, copy with ⌘C (Ctrl+C), and offer Copy on right-click through the app's text menu. A click gives them focus, as the message body has; being read-only, they leave single-key shortcuts working. A plain click on the sender name still opens the contact card. A folded message still expands from its whole row, name included. The subject keeps its fixed 26px lines and search tint as rich text. A name too long for its space is shortened with an ellipsis, so a copy then holds the shortened name; the contact card has it whole.
- **Evidence.** MailSelectionTest drags across each of the four, presses the copy key and checks the clipboard holds exactly that text, then does the same through the right-click Copy. The address failed the copy-key check before the change. The search reader check now also requires the subject's laid-out height to be its lines at 26px.

Mail UI134. No schema, fixture or protocol change.

## D187 — one blocked message no longer stops an account's mail (2026-10-03T17:30:00Z)

The founder's Windscribe mailbox stopped getting new mail for eight hours, and Sync did nothing. Only that account was affected.

- **Found.** While downloading older mail, the account fetched a phishing message. Windows Defender detected `Trojan:HTML/Phish` in its stored source and quarantined the file as it was written (04:46:13 local, and again at 12:52:31 after a restart). Staging could not verify the file it had just written and failed with `content_unavailable`. The mail worker treated that as a lasting error and returned, so the account's mail stayed paused, and Sync could not start it again until the app restarted. After each restart the same message was downloaded and blocked again. A read-only hash check found all 395,614 retained blobs intact.
- **Change.** A downloaded source that cannot be verified once written is recorded for that message alone (issue `source_blocked`, source unavailable), and the rest of the page publishes. Any mail step failure is logged as `mail_step_failed` with its code and message. An error that is not transient no longer ends the worker: it waits for Sync, or retries after five minutes, as remote drafts already did.
- **Evidence.** A Core test leaves other bytes at a message's blob name, as quarantine does: preparing that message records `source_blocked`, another message on the page publishes and the page completes. Before the change, preparation failed with `content_unavailable`. Engine and Core tests and clippy pass.
- **Not verified here.** The founder runs the new build against the live mailbox.

Core87. No schema, fixture or protocol change.

## D188 — Mail and Calendar switch at once (2026-10-03T21:00:00Z)

The founder found switching between Mail and Calendar slow, about 300 ms, and asked for it to be instant: UX speed is the first goal.

- **Found.** Each switch did three avoidable things. Remembering the open workspace went through the explicit preference save, which notified every preference binding (about 120 in QML) once immediately and again when the save returned. The Mail timeline sat beside the workspace stack, so showing or hiding it resized the stack and both Mail and Calendar laid out again. Hiding Calendar dropped its mail counts, so every return showed them loading, queried them again and laid the day headings out twice.
- **Change.** The open workspace is remembered quietly with calendar navigation (`rememberWorkspace`), 800 ms later or at shutdown, without a preference notification. The timeline header and footer belong to the Mail page, so the stack keeps its size. Hidden Calendar counts stop refreshing and drop late replies but stay published; reopening Calendar shows them at once and refreshes them immediately.
- **Evidence.** The native Calendar benchmark gains a fifth route, `workspace_switch`: the Mail and Calendar buttons alternate, timed from input to the first frame showing the other workspace (Mail with visible conversation rows, or Calendar with its unchanged grid matching). Optimized build, synthetic profile: p50 12.8 ms, p95 15.9 ms, max 18.3 ms over 100 switches; the other four routes stay within budget. CalendarMailActivityTest covers kept counts and the immediate refresh on return. The Month check had predated today's cell showing rows around the current time and now follows the cell's own row window.
- **Not verified here.** The same benchmark on the old switch path (the window lost focus on each attempt), and the founder's own large mailbox.

Mail UI135, Calendar43, Quality90. No schema, fixture or protocol change.

## D189 — the last row of a section turns into its empty line (2026-10-03T22:00:00Z)

The founder saw the Inbox list jump when the last To review or Delayed conversation was cleared: the "Inbox zero" line appeared after the row had gone, and asked for the change to be fast and clean.

- **Found.** A conversation read in To review folds away when the next one opens: its row shrinks to nothing over 170 ms, then leaves the list. When it was the section's last row, the section's empty line was then added one row high, so everything below moved up a row during the fold and snapped back. A frame check measured the Delayed header 52 px (one row) off its place across 35 frames. A conversation that is Delayed or Done moves in one change, and its section's empty line takes its place in the same frame.
- **Change.** When the folding row is the last in a section that stays shown (To review or Delayed while browsing), the model marks it `leaves_empty`. It keeps its height while its content fades out and the section's empty line fades in over it; the row is then replaced by the identical empty line. Other folding rows collapse as before.
- **Also.** A sender name in an open message's header was shortened by its last letters ("Nicole Dout…") although its pill was sized for the whole name: elision measured slightly wider than the advance the pill uses. A name whose advance fits is now shown whole. This check had failed on Windows and stopped the rest of MailSelectionTest from running.
- **Evidence.** MailSelectionTest reads the Delayed header's position at every rendered frame through the last row's fold and through Delay of the last To review row: it does not move. With the old fold the same check fails (52 px over 35 frames). The header copy check passes again, and so does the rest of MailSelectionTest. gui-mail-gpu's first-page length check fails on this host with and without these changes; it passed earlier the same day.

Mail UI136. No schema, fixture or protocol change.

## D190 — All calendars without a reload; Calendar follows today (2026-10-04T15:00:00Z)

The founder found two Calendar problems: turning All calendars on or off redrew Day, Week and Month with a visible jank, and a Calendar left on today, this week or this month stayed on the old day once the date changed.

- **Found (All calendars).** With All calendars off, the grid and the Mail timeline read only the current account's calendars. Turning the switch either way therefore changed the query. The app waited for the preference save to return, cleared the grid, showed "Loading calendars…" and drew it again from a new read. Every preference binding was also notified twice, once for the change and once when the save returned. Changing account with All calendars off read the calendar again too. The events were already filtered by account after reading, so the narrower read bought nothing.
- **Change (All calendars).** The grid and timeline read every selected calendar. All calendars off filters those rows to the current account. Turning the switch, or changing account while it is off, redraws once from what was read, with no read. The switch sends only the visibility, and clears a narrower calendar choice only when turning on. The save's own status (`preferencesStatus`, `preferencesPending`) has its own notification, so a finished save no longer re-evaluates preference bindings.
- **Found (today).** View and date were saved as a fixed date and never moved, so an open app, or a restart the next day, showed yesterday, last week or last month.
- **Change (today).** A Calendar whose day, week or month contains today saves `calendar_date` as null, which already means today. So does a Mail timeline on today with `timeline_date`. When the date changes while the app is open, such a Calendar moves to the new day, week or month; within the same week or month it stays put. A timeline on today moves to the new day. A Calendar or timeline left on another date stays there.
- **Evidence.** CalendarSearchTest: the grid and timeline request every selected calendar. Hiding other accounts, changing account and showing all again each apply at once, with one events notification for hiding and no calendar read. A controlled clock rolls over day, week and month: Day view and the timeline move to the new day, the same week stays, a new week and a new month move, and dates left elsewhere stay. Today is saved as null. All Calendar tests pass.

Calendar44, Mail UI137. Core already accepts null dates; no schema, fixture or protocol change.

## D191 — spam no longer stalls the store; a busy engine is waited for (2026-10-04T23:55:00Z)

The founder's app stopped loading any mail, and Drafts showed Inbox conversations, after leaving a draft unsent in a thread.

- **Found.** The draft was not the cause. At 23:02:48 a mail sync step held the store for 21 s. The drafts list request waited behind it, the app gave up after 15 s ("did not answer in time"), dropped the connection and so stopped the engine. The old engine took about 45 s to release the profile, and the app stopped starting a new one after 30 s, so it stayed disconnected; every later request was refused. Drafts then kept the Inbox rows on screen, drawn as drafts, because a folder that cannot load left the previous folder's rows in place. The engine logs for the day show the store held for over 5 s 12 times, all by a mail commit or a mail change. A message arriving in, or moved to, Gmail Spam is dropped by `forget_spam`, whose lookup put "this remote ID" and "everything labelled SPAM" in one `OR`. SQLite then scanned every message of the account for each spam message: 5.1 to 6.8 s on a copy of the founder's store for the 283,695-message account, against 0.2 ms by remote ID. Single-message history commits took 7 to 9.5 s, and two mail change steps took 13 s. Other commit and pending statements measured in milliseconds on the copy. `PRAGMA optimize` ran without an analysis limit when the store opened, every five minutes during sync, and when the engine closed. A full ANALYZE of the 5.4 GB copy took 4.5 s.
- **Change.** One spam message is looked up by account and remote ID; the label scan is a separate statement used only by the one-time cleanup. The store sets `analysis_limit=1000`, so statistics refreshes sample indexes. The app waits up to 60 s for a reply from a running engine (120 s for starting sync, as before) and keeps starting an engine for up to two minutes while an old one releases the profile. A folder switch with nothing saved to show at once clears the list instead of showing another folder's rows.
- **Evidence.** A Core test requires the single-message spam lookup to search `messages` by remote ID. Spam, retention and mutation tests pass; workspace tests and clippy pass. MailNavigationCacheTest switches to Drafts with no answer from the engine and finds an empty list. The query timings above are from a copy of the store, deleted afterwards.
- **Not verified here.** The founder's live store under the new build, and the full app reconnecting after a long hold.

Core88, Mail UI138. No schema, fixture or protocol change.

## D192 — read and Done show at once (2026-10-05T07:00:00Z)

The founder found that opening unread mail, which marks it read, and Done sometimes lagged, and asked that these never wait on Google, and then for an audit of the Gmail change queue so local actions are always fast.

- **Gmail queue.** Nothing local waits for Google. A mail change writes the store and a pending `google_mail_mutations` row in one transaction; a per-account worker reads each message's Gmail labels, sends one modify, trash or untrash if still needed and records the outcome, with retry deadlines that survive a restart. Mail changes have their own quota share (backfill reads take at most three quarters) and run one at a time per account. Incoming Gmail labels never undo a pending local choice: the manual intent wins until Gmail confirms it.
- **Found in the app.** It showed nothing until the engine replied, and its engine connection sends one request at a time, in order, so a mail write waited behind everything queued before it. The founder's GUI log (4 hours, release build) has `flag` at 10 ms median and 185 ms at most in the engine, dispatched behind up to 12 queued requests: calendar windows polled every 2 s while idle (7,118 in 4 hours, 120 ms median) and `accounts` (513 calls, 1.3 s median, 7.1 s at most). Done, Delay or another action taken while a change was still saving, such as the read from opening, was dropped without a word.
- **Found in the engine.** `accounts` and every mail list read rebuild an account's conversation states from all its messages once more than 512 conversations changed since the last read. On a copy of the founder's store that took 2.0 s for `accounts` and 1.8 s for an Inbox page, against 0.2 s and 0.09 s below the limit; re-reading one changed conversation costs about 0.05 ms. The engine log has 579 `accounts` calls holding the store over 1 s. What rewrote that many conversations while sync was idle is not identified.
- **Change.** Explicit queries and mail changes (read, star, category, filing, undo) pass queued polling reads, which now include calendar reads started by polling; each polling read still runs after four bypasses, and every other request keeps its place. A read or star change shows on its rows and open reader as soon as it is sent; the reply confirms it and a refusal reloads the stored state. An action taken while another saves runs right after it, for the same conversation or selection and account; a disconnect drops what is waiting. The store re-reads changed conversations up to a quarter of the account before rebuilding: on the copy, `accounts` after 1,000 changed conversations fell from 2.3 s to 0.24 s and an Inbox page from 1.8 s to 0.13 s; 12,000 take 0.9 s.
- **Evidence.** MailSelectionTest opens unread mail with the engine's reply held: the row and reader show read before it, Done pressed meanwhile is sent once the read lands, and a later Delay is sent ahead of a queued calendar read and shows at once. The GUI suite's queue-order checks (gui-integration, gui-attachments) pass. A Core test changes 700 of 3,000 threads and checks they are updated without a rebuild and match a fresh scan; it fails with the old limit.
- **Open.** Calendar polling reloads two windows on every calendar pass even when nothing visible changed. A row hook mismatch (bulk deletes, text-index writes) marks every GUI area changed. `accounts` still recounts conversations and reads Gmail coverage twice per account, about 150 ms in all on the copy.

Core89, Mail UI139. No schema, fixture or protocol change.

## D193 — reminder actions stay on screen; the alarm stops a minute in (2026-10-05T12:00:00Z)

The founder found that a reminder for a meeting with many guests pushed Join, Snooze and Dismiss off screen, and that an unattended reminder kept sounding until the meeting ended.

- **Change.** The reminder and View screen scroll their details (title, link, guests, notes) and keep the actions in a row below them that never scrolls away; a line separates the two when the details overflow. Short content and its actions stay centred together as before. Sound stops 60 s after the start: the repeating alarm ends, a playing tone is stopped, and a reminder first shown later than that comes forward without a sound.
- **Evidence.** NotificationBackendTest shows an 18-guest reminder in a 430×410 window: every action lies inside the window in all three themes at both text sizes, and the details scroll. The same test finds the alarm repeating at 59 s after the start and stopped at 61 s with no further chime. Both checks fail on the previous screen. CalendarControlsTest (View) passes unchanged.

Calendar45. No schema, fixture or protocol change.

## D194 — replies mirrored from Gmail keep bracketed Message-IDs (2026-10-05T23:30:00Z)

The founder's reply "Hey Dave, all paid." failed with "the message identity contains characters mail cannot carry".

- **Found.** The draft was a Gmail draft mirrored into an editable local draft. The mirror stores the inventory preview's routing, and the preview kept In-Reply-To and References as `mail_parser` returns them: without angle brackets. Replies composed here, and drafts adopted explicitly, store `<id>`. The composer accepts only `<id>`, so the send stopped before reaching Gmail. The Outlook Message-ID itself was valid.
- **Change.** The preview stores reply routing in angle brackets. Building mail from a draft brackets any bare ID, which repairs drafts already stored that way; the mirror rewrites a draft only when Gmail's copy changes, so it would not have repaired them.
- **Evidence.** A Core test sends a draft with bare IDs through `compose_request` and `build_mail`; the inventory test now requires a mirrored Gmail reply to carry `<parent@…>` and bracketed References. Both fail on the previous code.

Core90. No schema, fixture or protocol change.

## D195 — a forward opens in To (2026-10-08T10:00:00Z)

The founder asked that a forward put the cursor in To, so the address can be typed at once; replies keep the cursor in the body.

- **Change.** When the composer opens, a forward with no To recipient focuses the To entry. Every other draft, and a forward that already has a recipient, focuses the body as before. New messages are unchanged.
- **Evidence.** The GUI integration self-test opens reply-all, forward and reply and requires focus in the body or To; the forward check fails on the previous composer.

Mail UI140. No schema, fixture or protocol change.

## D196 — address suggestions without the wait (2026-10-08T11:00:00Z)

The founder found suggestions in To slow to appear.

- **Found.** Each lookup read the newest 2,000 messages of the current account and of the other accounts together. The other-accounts query sorted every message those accounts hold: 0.28 to 0.74 s on the founder's store (414,000 messages across three accounts), against 4 ms for the current account's indexed query. Every row's To list was then parsed whether or not it could match. The request also waited behind queued polling reads (`accounts` took up to 1.3 s in the D192 log), after a 150 ms typing pause, and the open list was cleared on each key until the new answer came.
- **Change.** Other accounts are read one at a time through their `(account, sent_at)` index and merged to the same newest 2,000: 4 to 6 ms per account on the store. A To list that does not contain the query is not parsed; stored lists are decoded text, so no match is lost. `recipient_suggestions` passes queued automatic reads like an explicit query. The pause is 50 ms, and while it runs the rows already shown narrow to those still matching the text.
- **Evidence.** MailSelectionTest sends a lookup ahead of a queued calendar read; ComposerInputTest types on and finds the matching row kept and the other dropped before the next lookup is sent. Both fail on the previous code. The existing ranking test passes. Query timings are from read-only runs on the founder's store.
- **Not measured.** The whole lookup end to end in the running app.

Mail UI141. No schema, fixture or protocol change.

## D197 — coming back to the app syncs at once (2026-10-08T12:00:00Z)

The founder asked that clicking back into the app sync straight away instead of waiting for the next timed pass.

- **Change.** When the application becomes active again after losing focus (not its first activation at launch, which already starts sync), the GUI sends `google_sync_refresh` for each account whose sync this window owns. It skips accounts it does not own (a paused account, or one synced only by the CLI), accounts waiting on keychain access (a refresh would ask the keychain again) and accounts mid start, refresh or pause. Wakes are at most every 10 s, counted from the last one that woke an account. They are quiet: a refusal shows in the account's sync state, with no error message and no busy state on the Sync button.
- **Evidence.** CalendarSearchTest raises Qt's application-state signals: staying active sends nothing; leaving and returning sends one refresh for the owned account and none for an unowned one. A second activation within 10 s sends none, an account waiting on keychain access is skipped, and an activation after 10 s sends another. The first check fails without the signal hookup.

Mail UI142. No schema, fixture or protocol change.

## D198 — the Terminal design (2026-10-09T08:30:00Z)

The founder supplied the designer's "fuckMail Terminal" export and its handoff notes, and asked for it to be built. The export is preserved as [fuckMail Terminal (offline).html](<research/assets/2026-10-09/fuckMail Terminal (offline).html>); [the research note](research/flight-progress-v2.md#october-9-terminal-export--d198) lists what was adopted.

- **Tokens.** Graphite takes the export's palette. Every font size is 94% of its old design size (never under 10.5px), weight 600 becomes 500, and no text has negative letter-spacing. Radii are 5px for controls, 8px for floating layers, 10px for dialogs and 4px for key chips and the TLDR tag. Popovers share one deeper shadow. Switches are 12px squares; the old slider stays only where a row needs it (all-day, remote content). Segmented controls have no track. Icons are drawn with square caps and mitred joins. Geist Mono (Regular and Medium, from the same upstream commit as Geist, under the same OFL licence) is bundled and is the fixed-pitch face for sources, code, search operators and key chips.
- **Shell.** The top bar is 48px: the Mail/Calendar switch, lowercase account tabs (letter tile, status dot, count), Sync, search with a ⌘K chip, and Settings. A new 24px bar along the bottom shows the current view's keys from the shortcut registry. Settings → General → Shortcut bar (`key_hints`, default on) hides it. The account hover card is a grid of inbox, mail and model rows, with "~9m left" style estimates.
- **Mail list.** Inbox, sent and drafts are lowercase tabs, with Spam and Trash behind ▾ and "N to review · N delayed" at the right. The label rail is plain `#label` toggles. Section headers are 32px uppercase rows. Rows use a 24 / sender / subject and preview / `#label` / time grid with a hairline between rows (none above a group's first) and no zebra shading. Times read "22 min ago", "3 hrs ago", "Yday 14:02", "Thu 13:22" (this week, weeks starting Monday) and "21 Sep". "45 sec ago" became "just now", because the clock publishes once a minute.
- **Reader.** The subject is 16px. Participants show as coloured 20px avatars. Every message in a thread is listed in date order; the "N earlier messages" fold is gone. A visit opens the messages since your last reply, counted from the first one still to review, or the latest alone when nothing is waiting. Each message has one 40px header row whatever its state: a caret in the gutter, a click anywhere toggles it, the preview shows when closed and the address when open, and the actions sit before the time. Hairlines replace alternating backgrounds. The cc line is indented 28px. While a reply still shows as sent from here, Gmail's copy arriving in the thread waits behind it, so the reply never shows twice. The message summary is a TLDR box: tag, `#label`, then the text. The model and time move to its tooltip, and the tag opens sources, details and Cancel.
- **Calendar.** 16px side padding and a 40px header row, with a 15px title and checkbox-style toggles.
- **Loader.** The wordmark is followed by a blinking amber block cursor.
- **Kept against the export.** The export's 1px account-colour line along the top stays out, as the founder asked under D144. The reader keeps the thread's label after the subject as a quiet `#label` that opens the label chooser. The export drops it, which would have left label changes only in the More menu. The per-message flag is not built (the founder skipped it).
- **Evidence.** 36 of the 38 GUI tests in a release `build/check` tree pass, and MailSelectionTest has also passed whole. The two failures fail the same way on the previous commit: gui-mail-gpu's first-page length check, and MailSelectionTest's frame count while a row folds (4 frames where 5 are expected), which depends on window focus on this host. Tests changed for the new layout: row separators and 24px avatars, 32px section headers, 40px message headers with the time last and the caret in the gutter, a header click that closes an open message, every message listed, closed earlier messages that stay closed through refreshes, the scope stats object, the 384px account card, the 30px Settings button, Graphite's new canvas and gold, and 12/13px button text. Preferences validate and persist `key_hints` (Core test). No screenshot of the running app on a real mailbox was taken.

Mail UI143, Core91. No schema, fixture or protocol change.

## D199 — a summary of the whole thread, on request (2026-10-09T08:30:00Z)

The Terminal export adds a thread summary button. Its text was stitched together from list snippets. The founder chose to build a real one with the local model. This reverses D111's removal of the thread summary card. The queue still works one email at a time, and nothing summarizes a thread unless asked.

- **Request.** `assist_start {task:"thread", conversation_id}`, with no `message_id`. It shares admission, the single inference seat, polling and cancel with message jobs. The model gets the subject and up to the newest 40 messages, oldest first, with your own as "You". Text shares the 24,000-character budget, newest first, and each message keeps at least 400 characters. Built-in prompts ask for one paragraph of at most 70 words that ends with who owes the next reply. The schema is summary-only, and replies are checked as one bounded paragraph.
- **Result.** One row per conversation in `local_model_results` (`task:"thread"`, `target:` the conversation), returned by `assist_results` with `partial` and a `stale` flag. `stale` turns true when any member, the subject or the account lifecycle changes. A failure writes nothing: no failure row, labels, sender kinds or per-message results.
- **GUI.** For a thread of two or more messages, a 32px AI button follows Reply, Forward and Delay. The first click writes the summary; later clicks show or hide it, and a summary the thread has moved past is rewritten when shown again. The result uses the TLDR box with a close ×. While busy, the box shows "Reading N messages…" and "Summarizing the thread…" with elapsed seconds; the engine reports no percentage, so none is shown. "Classify thread" now classifies the thread's latest received message; it used to send an empty target and always fail.
- **CLI.** `fm --account ID assist thread CONVERSATION [--json]`.
- **Evidence.**
  - fm-assist tests cover the request shape, reply validation and the text budget.
  - fm-core tests cover member order with your own replies, shortening and leaving out older mail, save and staleness, a change between prepare and finish, settings checks, and queue progress left alone.
  - An fm-engine test runs validation, a failed job, cancel during a hanging request, and a whole job through the fake OpenAI-compatible server.
  - fm-cli tests cover the arguments.
  - LocalModelTest finds a thread job named by conversation with no message and read back as the thread's result. MailSelectionTest clicks the reader button and finds the whole-conversation request.
  - Summary quality on real mail was not measured.

Local assistance32, CLI63, Mail UI143. No schema change; `local_model_results` rows with `task='thread'` are new.

## D200 — sync no longer waits on mail antivirus removed, or on linking labels (2026-10-09T08:30:00Z)

The founder's Windscribe account showed "syncing 4 items" forever and could not fetch labels.

- **Found.** Four messages were recorded `source_blocked`: their written source was removed on disk (Defender quarantine). Sync progress counted only `item_too_large` and `malformed_response` as undownloadable, so the four stayed "to go" for good. Separately, `google_labels_merge` linked every new label in one store call. Each link ran a full-account projection (three CTE scans), which held the single store worker for 23.5 s, so the GUI's requests queued up and were refused with `queue_full`.
- **Change.** Progress counts `source_blocked` as undownloadable. It still blocks offline completeness, and it is fetched again only after the next full listing. Label projection is durable and runs in parts of at most 5,000 message-state rows, one transaction each, with a cursor in `meta` (`google_category_projection\n<account>\n<category>`). Only memberships that change are written. Each merge call links at most one label or applies one part, and returns `more`; label sync and the first import repeat it, yielding between calls. Binding a label applies the first part and wakes the account's sync, whose label worker finishes the rest. Opening the store only schedules repairs.
- **Evidence.**
  - The blocked-message test now requires `undownloadable == 1`; it was 0 before the fix.
  - A 1,107-message core test links one label per call and checks each call reads at most one part and ends with exactly the labelled memberships; it fails on the previous code. Reopen and relink tests cover unfinished projections.
  - An engine test holds the store between merge calls and finds another request answered with one label linked; it fails on the previous code.
  - The fm-core, fm-engine and fm-cli suites, fmt and clippy pass.
  - Not measured on the founder's store: an estimated 25 ms per warm part, about 57 parts per label.

Core92, Gmail44. No schema, fixture or protocol change.

## D201 — no label after the subject; the TLDR tag sits on the text (2026-10-09T12:00:00Z)

The founder found two artifacts of the Terminal design.

- **Subject.** The reader heading no longer shows the thread's `#label` after the subject. The label already shows in the list row and in the summary box. The label chooser it opened is gone; labels are changed from the actions menu's Labels submenu, as for list rows.
- **TLDR tag.** The tag sat about 5 px below the first line of the summary. Its position assumed the text is centred in its 1.6 line, but Qt sets rich text at the top of the line and leaves the extra space below. The tag's capitals are now centred on the first line's capitals, from both fonts' metrics.
- **Evidence.** LocalModelTest compares the rendered ink of "TLDR" with the first word in every theme and text size; they now differ by at most 1.5 px, against 5 px before. The GUI integration self-test labels the open thread through the actions menu while a reply is open, and the CLI sees the label.

Mail UI144. No schema, fixture or protocol change.

## D202 — paste screenshots and files as attachments (2026-10-09T15:00:00Z)

The founder pastes screenshots into replies all the time and asked that a pasted image attach itself.

- **Change.** Paste in the composer body (⌘V/Ctrl+V or the menu) looks at the clipboard first. Copied local files attach. An image copied without text, such as a screenshot, is written as a PNG under the profile's `cache/pasted` and attaches as "Screenshot YYYY-MM-DD at HH.MM.SS.png", with " (2)" and so on for a second paste in the same second. An image that comes with text (a spreadsheet range, say) pastes the text, and so does everything else. Encoding runs off the UI thread. Pasted images stay a week so an uncertain import can be retried from the same file, then are removed at the next paste.
- **Queue.** A paste while an earlier file is still importing joins that queue instead of being refused. A stopped or failed import still has to be resolved first, as for chosen files. The 50-attachment limit counts waiting files.
- **Menu.** The editor menu's Paste reads "Paste as attachment" and is enabled when the clipboard holds files or a screenshot.
- **Evidence.** ComposerInputTest, on the offscreen platform's private clipboard: a pasted image starts an import of a PNG named like a screenshot with the same pixels and leaves the text alone; a second paste during that import queues and imports under its own name; copied files attach instead of pasting their names; an image with text pastes the text. The paste checks fail without the composer's handlers.

Mail UI145. No schema, fixture or protocol change.

## D203 — a long thread opens at its first unread message (2026-10-09T19:00:00Z)

The founder opened a 33-message thread for the mail that had just arrived and saw the list of older messages instead; the new one was below the fold.

- **Change.** When a thread is first shown with unread mail, the reader scrolls so the first unread message's header sits just below its top, after the thread's layout settles. It does so again if a fuller copy of the thread arrives and moves that message, until the user scrolls. It does not scroll when a position was restored after a restart, when the reply box is open, or when nothing is unread (the thread opens at its top, as before). Nothing opens or closes because of it.
- **Evidence.** MailSelectionTest opens a 31-message thread whose newest message is unread: its header ends up 12 px below the reader's top (1,216 px down, unscrolled, before). A thread with nothing unread opens at its top.

Mail UI146. No schema, fixture or protocol change.

## D204 — mail read on screen, and reviewed mail leaving To review (2026-10-09T21:00:00Z)

The founder found that a message arriving in the open thread kept it in To review after leaving it, and asked that an opened To review conversation move on after 10 s without opening another, as a Settings → Mail choice.

- **Found.** To review is unread mail. Read-on-open reads a conversation only when it is opened; a message arriving while it was open stayed unread. The open conversation, read on opening, is kept in To review until another opens.
- **Arrivals.** A message new to the open thread, unread and not sent by you, is read like an opening would read it: at once when the reader is shown with the app in front, otherwise when both become true while it is still open. A message marked unread by hand is not new and stays unread. Done still applies only to the members it was pressed on.
- **Leaving To review.** New preference `review_leave_seconds`: 0 (off), 5, 10 (default) or 30. That long after an opening from To review, the open conversation, once read, moves to its own section while it stays open, as Done or Delay move it. A read still saving is waited for; an unread one stays. It changes no mail state: the conversation was already read when opened. Settings → Mail → "Reviewed mail leaves To review" offers 5 s, 10 s, 30 s and Off.
- **Evidence.** MailSelectionTest: a message arriving in the open thread sends a read for it; with the timer at one second the open conversation leaves To review while open, and with Off it stays. Both fail without the change. Core rejects other values. The GUI integration self-test's provider arrival now expects the arrival read when the app is in front, and its collapsed-group count check no longer assumes every Done month is open.

Core93, Mail UI147. No schema, fixture or protocol change.

## D205 — motion (2026-10-10T18:00:00Z)

The founder asked for John Lasseter's 1987 animation principles to be read in full and their best ideas built into the UI, for a polished feel. [The research note](research/motion.md) maps each principle to what was built and what was left out.

- **Timing and reduced motion.** `Theme` gains one timing scale (90, 120, 170, 220, 420 ms) with arrivals easing out and departures easing in. Every one-shot transition passes through `Theme.ms()`. New preference `motion`: `system` (default), `full` or `reduced`, in Settings → General → Motion. System follows Windows' Animation effects, macOS's Reduce motion and GNOME's `enable-animations`. Reduced keeps every change and drops the movement. State never waits for motion.
- **Folding moves.** Done, Delay, Delay's undo and filing that take the open conversation out of its Inbox section, from the reader or as a one-row selection, now fold its row where it was (the D144 fold), drifting toward where it goes; so does `review_leave_seconds`. This replaces "moves at once" for those cases; multi-row selections still move at once. The list refresh waits for the fold.
- **Popups, toast, controls.** Menus, pickers and dialogs ease in from just under full size and close at once. An exit fade was tried and dropped: it kept a closed menu on screen long enough to swallow the next key. While a popup eases in, its `opened` is false, so on-screen checks read `visible` and setup runs in `aboutToShow`. The toast rises in and sinks out, and an Undo toast's line runs out with the time left. Prominent buttons sink while held, Sync's arrows turn while syncing, the pill switch stretches in flight, and tab counts roll in from the side they moved from.
- **Delight, rationed.** Reaching Inbox clear through work plays one small check, ring and staggered lines, never on launch or an account switch. Calendar slides 18 px in the direction of travel.
- **Left out.** Motion on opening a conversation, Up/Down or switching account (cached navigation's 100 ms budget, Quality), ListView add/remove transitions (the list lays out each structural batch at once), and the search dropdowns (live search starts when they open).
- **Evidence.** Release `build/check` tree on Windows 11: Rust workspace and test build pass, fmt and clippy clean; all 37 GUI tests pass. gui-mail-gpu timed out once under full-suite load, then passed alone twice. MailSelectionTest passes 6 of 6 runs in a row. Its new checks: Done folds the open row marked `done` and the list is not reloaded under the fold; Delay folds toward Delayed; archiving folds `filed`; Theme follows the preference and the system setting; the Undo line runs out evenly and hides under reduced motion; the switch knob stretches in flight and lands square. Changed checks: archive and Trash wait for the fold; the HTML badge click waits for its header row to lay out. The same suite at the previous commit fails 3 of 5 runs at the last-row fold frame check, so it was already timing-sensitive. The GUI integration self-test treats the capture popup as open once visible. Bugs the tests found and fixed: the countdown's `left` property collided with Item's, Google sign-in reset its form after the fade-in, and a label choice made during the menu's fade-in read the wrong state. Core rejects other `motion` values. No screenshot or recording of the running app on a real mailbox was taken.

Mail UI148, Core94, Platform24, Calendar46. No schema, fixture or protocol change.

## D206 — menus at the pointer, a moving workspace switch, no splash cursor (2026-10-10T20:00:00Z)

The founder reported every right-click menu opening at the left edge, asked for the splash to keep only its animation, and asked for a more animated, satisfying Mail/Calendar switch.

- **Found.** D205 clamped `FmMenu` in `aboutToShow`. Before a popup is visible Qt reads its `x` and `y` as 0, so the clamp wrote the 8 px margin over every caller's position. An isolated probe showed x = 0 at `aboutToShow` and the requested x once visible. The clamp now runs when the menu becomes visible and on size changes while it is.
- **Workspace switch.** State still changes at once. The tab highlight is one shape whose leading edge outruns its trailing edge (180 against 280 ms), stretching as it travels: Lasseter's overlapping action. The tab it lands on pops with an overshoot. The view slides 28 px in from its tab's side while fading up (220 ms). Settings and reduced motion cut.
- **Splash.** The blinking amber cursor added under D198 is removed; the wordmark animation stays.
- **Evidence.** Release `build/check` on Windows 11: 38 of 39 tests pass, two of them on rerun as described below. MailSelectionTest now checks that the row menu opens at the clicked point; that check fails at the left edge before the fix. The GUI integration self-test checks that switching to Calendar starts the slide from the right, and that the ink comes to rest exactly on the Mail tab. rust-workspace failed once in `remote_inventory_refresh_is_bounded_owned_and_does_not_restart_for_an_additional_owner`, with no engine change, then passed on rerun. gui-mail-gpu fails its first-page length check, the failure D198 records on the earlier commit; it loads MailView alone, outside this change. The first build failed to load Main.qml: the ink had a property named `left`, which QQuickItem declares FINAL. The founder then saw no ink at launch until a first switch: the ink's tab lookup ran before the Repeater built the tabs and never re-evaluated. It now depends on the Repeater's count, and the self-test checks the ink sits on Mail from launch; that check fails on the earlier version.

Mail UI149. No schema, fixture or protocol change.

## D207 — mail each way in address suggestions (2026-10-11T09:00:00Z)

The founder asked for address suggestions to show how many emails were sent to and received from each person, minimally, and to sort matches by that count.

- **Change.** Each suggestion carries `sent` and `received` in the account it names, and the busiest correspondents come first; match strength now only breaks ties. The row shows "↑ 12  ↓ 40" in small subtle type beside the address, leaving out a direction with none.
- **How it is counted.** Received reads the existing `(account, sender_email)` index. Sent walks the account's mail from its own address through the same index and joins the people index, which records From, To and Cc for every message. `message_people` has no index by person, so counting per candidate would scan it; one walk per account, kept until the people index moves on or for five minutes, avoids both that and a schema change. Mail from a send-as alias is not counted as sent.
- **Trade-off.** Sorting by total puts a busy automated sender above a person you write to rarely. Kept as asked; sorting on sent first is the obvious alternative.
- **Evidence.** fm-core recipient tests check the counts, the order (three sent beats two received beats one received, whatever the match strength), that the cache refreshes when the index moves on, and that the sent walk uses `messages_sender`. ComposerInputTest checks a row shows `1.2k` and `3` in that order and a row with no mail shows no counts. Workspace clippy and the fm-core suite pass; gui-composer-input and gui-mail-selection pass on `build/check`.
- **Not measured.** Lookup time on the founder's store: the app was running on it, so the new engine could not open it.

Mail UI150, Core95. No schema, fixture or protocol change.

## D208 — quoted history on request, with what changed highlighted (2026-10-12T12:00:00Z)

The founder showed a reply whose answers were written inline inside the quoted original. The reader hides quoted history in both HTML and text, so the answers were invisible. They asked for a minimal "···" button at the bottom of a message that reveals the quote. Then, mid-change, they asked for the quote to be diffed against the original email with the differences highlighted.

- **Reveal.** The button appears only when the reader left something out. A button that does nothing on most mail would be noise. HTML mode asks the engine for a second sanitised view with history kept (`message_html` `quotes: true`). Text mode lays out the whole reading text, which `message_source` now returns as `body_full` when it differs from `body_text`. The quotes view numbers images and links over the whole document, so its manifests, inline-image cache names and GUI cache entries are kept apart from the default view's.
- **Diff.** Done in the GUI against the earlier messages already in the conversation; no new engine call. The comparison is by words in order, not by lines, so a quote re-wrapped at 72 columns still matches. Short lines must continue where the previous line matched, so a one-word "Yes." inserted into the quote is not explained away by a "yes" later in the original. Whole lines are marked; an edit inside a line marks that line. An original that is not in this conversation (another account, or never synced) gives no highlight rather than marking everything.
- **Trade-off.** HTML mode reuses the search-highlight path, so a shown search replaces the diff tint until it is cleared, rather than showing two tints at once.
- **Evidence.** fm-mail and fm-core tests: the HTML view keeps history on request, the quotes view resolves its own images, a non-boolean `quotes` fails, and `body_full` carries `>` quotes and is empty when nothing was left out. On a Release `build/check`, these pass: ReplyDocumentTest (the whole-text layout; the diff of a re-wrapped quote with a short "Yes." and a one-line answer; no marks without an original), HtmlMessageBodyTest (reload with quotes, images asked for in that view, the added-line tint, search taking precedence) and MailSelectionTest.
- **Not checked.** The founder's real thread in the running app: it was running from `build/release`, which this change did not relink.

Mail UI151, Core96. No schema, fixture or protocol change.

## D209 — signed and notarized macOS disk image (2026-10-13T20:30:00Z)

The founder's Apple Developer membership is active. They asked for a signed, notarized Mac build they can send to people, without the keychain prompts. D130 had left both installers unsigned until then.

- **Opt-in.** `package` signs when `FM_MAC_SIGN_IDENTITY` names a Developer ID Application identity, and notarizes when `FM_MAC_NOTARY_PROFILE` names a `notarytool` keychain profile. Both credentials stay in the founder's keychain. CI sets neither, so its packages stay unsigned; signing in GitHub Actions is left for later. An identity alone signs without notarizing, for a quick local check.
- **Signing.** Inside out, as Apple advises instead of `codesign --deep`: loose libraries and plugins, the Qt frameworks, `fm` and `fm-engine`, then the app. All use the hardened runtime and a secure timestamp. The app's only entitlement is `allow-jit`, for the QML engine.
- **Keychain.** macOS lets a program back into its keychain items when it matches the same identifier and team. `fm-engine` is signed as `com.fuckmail.engine` and `fm` as `com.fuckmail.cli`, so a signed update counts as the same program. A Mac that ran an unsigned build asks once more. Changing these identifiers or the team brings the prompts back.
- **Client registration.** `google-oauth-client.json` moves to `Contents/Resources`, because a signed bundle's `Contents/MacOS` may hold only code. An engine running from `Contents/MacOS` looks there; any other engine looks beside itself. Accounts that recorded the old path fall back to the packaged file when the client id matches, as before.
- **Disk image.** It is signed, notarized and stapled. Only the image is submitted: Apple's ticket also covers the app inside it, and Gatekeeper looks that up online the first time the app opens from Applications. The trade-off is that a first launch with no network can't confirm the app; stapling the app too would need a second submission.
- **Checks.** The relocated-package checks run on the signed app. Their Qt load trace is skipped there, because the hardened runtime ignores `DYLD_*` variables and library validation refuses libraries signed by another team. After notarization, `spctl` assesses the image and the app inside it.
- **Setup found on this Mac.** The certificate went into the System keychain and showed as not valid until Apple's Developer ID G2 intermediate certificate was imported. Choosing Allow at `codesign`'s key prompt asked again for every file; `security set-key-partition-list` stopped it. `notarytool` reports a wrong Apple ID as "Your Apple ID has been locked". The build guide lists the steps.
- **Evidence.** An fm-engine test covers the client path inside and outside an app bundle, and the Google sign-in tests pass. On macOS 15.7.7 (arm64), a signed copy of the release app passes `codesign --verify --deep --strict`, the GUI smoke, a CLI query and the full self-test under the hardened runtime. `package --preset release` with both variables passes the relocated-package checks on the signed app. Apple accepted the 92 MB image, the team's first submission, in under an hour. The packaging run was stopped while it waited, so the same image was stapled by hand. `stapler validate` passes; `spctl` accepts the image and the app inside it as "Notarized Developer ID".
- **Not checked.** Opening the image on a second Mac. An update from one signed build to the next without a keychain prompt, which needs two signed builds installed in turn.

Platform25. The engine's client lookup changes; GUI, CLI, schema, fixtures and protocol unchanged.

## D210 — safer attachments, sender verdicts and unsubscribe email (2026-10-14T09:00:00Z)

The repository is public and the founder reads their own mail with the app, so they asked what a booby-trapped email could do and chose which fixes to make. They also asked for a warning left of TEXT/HTML when the header-based sender checks fail or look suspicious.

- **Found.** An attachment opened from the app reached the system without the downloaded-from-the-internet mark, so Gatekeeper and SmartScreen never checked it, and any extension opened in its default app. Image attachments went raw to Qt's decoders (a disguised SVG was parsed before its type was checked), and a video started decoding in FFmpeg for its poster frame as soon as its card was on screen. A `List-Unsubscribe` mailto could name any recipient, subject and body, so Unsubscribe could send the sender's text from the person's Gmail to someone else. "Always load images" trusted the From address alone. mail-parser's `header_raw` returns the last copy of a repeated header, the one a sender can write.
- **Downloaded marks.** Exports, fetched remote attachments, saved copies and `fm --output` copies carry `com.apple.quarantine` on macOS and a `Zone.Identifier` stream on Windows (Platform26). An existing mark is kept, so a Gatekeeper approval survives reopening.
- **Files that run.** Apps, installers, disk images, scripts, shortcuts, `.terminal`, location files, `.jar`, HTML and SVG, and their Windows and Linux equivalents are not opened from the app. A toast says why and offers Save to Downloads. Archives and Office files still open; their marks make the system check what is inside.
- **Previews.** Image thumbnails and the viewer show only a PNG the engine decoded with the memory-safe `image` crate (which now applies EXIF orientation). A video card decodes nothing; it plays only in the viewer once opened. The poster frame and the card's duration are gone.
- **Sender verdicts.** `fm_mail::sender_check` reads the topmost `Authentication-Results`, only from `mx.google.com`, dropping comments first. It reports SPF, DKIM and DMARC, whether the From domain is verified (DMARC pass, or an aligned passing DKIM signature), and warnings: DMARC failed; neither SPF nor DKIM passed; a display name showing another address; a non-ASCII or punycode From domain. Reply-To elsewhere is common in legitimate mail and is not a warning. Stored and live Gmail messages both carry the result. The reader shows a red (DMARC) or amber warning icon left of TEXT/HTML with the reasons and Gmail's checks in its tooltip.
- **Always load images** applies only when the sender is verified and the message is not in Spam. Images load when a late verdict arrives; otherwise the row says why they wait.
- **Unsubscribe email** is offered only to a domain Gmail authenticated for that message, so it can reach the list's own domain or its sending service, never a third party. The reader asks before sending and names the recipient. The automatic email after a refused one-click POST stays automatic; it is bounded by the same rule.
- **Not in the public record.** Other review findings are tracked privately until they are fixed.
- **Layout bombs.** litehtml matches selectors by backtracking, pads table grids, scans earlier rows for every cell, and reparses the sanitiser's output. Capping combinators was measured and does not bound the cost. Sanitiser version 5 instead:
  - estimates each selector's worst-case matching work on a reparse of its own output and keeps selectors within a fixed budget;
  - bounds table cells and row scans per message;
  - caps spans at 100, elements CSS turns into table parts at 200, and nesting at 128;
  - writes markup that parses back to the same tree.
  In litehtml, inline boxes are now split around their blocks in one pass, and its render list is no longer swept on every add (a local change had made that quadratic). Every attack input found now lays out within 300 ms. As a side effect, text after a nested block no longer comes out of order, and inline-blocks keep their own blocks, as browsers do. The GUI marks blocked links with one attribute and one rule.
- **Evidence.** New fm-mail tests cover the sender verdicts, including a forged lower header and comment text, mailto unsubscribe limited to authenticated domains, EXIF orientation, and the HTML bounds (listed in Quality91). fm-core tests check that exports carry the quarantine mark, keep an approved mark and gain one when reused, and that image previews are PNGs while other bytes are refused. fm-cli tests check that the output copy keeps the mark and leaves no staged file. In the GUI, gui-html-message-body checks the unsubscribe confirmation and that an unverified always-load sender's images wait until a verdict arrives. gui-mail-selection checks the warning badge's placement and tooltip, that a video card creates no player, and the engine-PNG preview. gui-html-view checks how blocked links paint, inline split order and the inline bomb. The full release suite passes, Rust and GUI. In one of three full runs, nine fm-engine `sessions` tests timed out waiting for a launched engine; they pass alone and in a full rerun.
- **Not checked.** The Windows `Zone.Identifier` paths, whose fm-core test runs only on Windows CI. Gatekeeper's prompt when a marked file is actually opened. A real mailbox: the badge's false-positive rate on real mail is unmeasured.

Core97, Gmail45, Mail UI152, Platform26, Quality91. No schema, fixture or protocol change; cached HTML views rebuild for sanitiser version 5.

## D211 — subjects fold to one line instead of failing to save (2026-10-16T22:11:00Z)

The founder reported a new draft with a short, ordinary-looking subject that would not save: every autosave failed with "Subject must be one line of at most 8192 bytes". The draft's last good save held only the first words; the rest was pasted and carried a line break or tab. Qt's text field draws control characters as spaces, so the subject looked fine and could not be corrected.

- **Composer.** `setDraftSubject` folds what the field gives it: each run of whitespace holding a control character, or a line or paragraph separator, becomes one space, or nothing at either end, and the field shows the result. Only runs holding a break change, so spaces being typed are never trimmed mid-word.
- **Received subjects.** The same rule applies in Core to reply and forward subjects built from received mail. On the founder's store about 200 conversations have subjects with line breaks or tabs. Replying to one failed with the same error, and a legacy reply silently lost its source routing.
- **Not folded on save.** `draft_save` still refuses control characters, which keeps the header-injection test as it is and gives `fm` callers an explicit error rather than a quiet rewrite. The trade-off is two copies of the rule, one in Rust and one in C++; the contracts describe it once each and point at each other.
- **Evidence.** A new fm-core test replies to and forwards a message whose encoded subject decodes to a line break and a tab, saves both drafts and a legacy reply, and checks the helper on trailing breaks, typed double spaces and U+2028. With the helper disabled it fails with the reported error. ComposerInputTest pastes the subject through the real field and checks the folded draft, the field text and the saved subject; it fails without the composer change. The fm-core suite, workspace clippy and the `build/dev` ctest suite pass, except that gui-debug-console timed out in the four-job run; it passes alone.
- **Not checked.** The founder's open draft in the running app, which still runs the old build. Whether its unsaved subject survives the restart onto the new build is untested; pasting it again fixes it.

Core98, Mail UI153. No schema, fixture or protocol change.

## D212 — hostile mail and invites: bounded parsing, confirmed unsubscribe, attachment allowlist (2026-10-21T18:00:00Z)

The founder asked for a full security review of what an outside attacker who has read the source could do by sending mail or invitations, then asked for every High and Medium finding to be fixed without regressions. D210's privately tracked findings are among them.

- **Found.** A body of about 20,000 nested elements overflowed the engine's stack in the unsubscribe link scan, on open or on prefetch, and killed every account. html5ever parses some nesting and attribute shapes in quadratic time, so a few MB of HTML stalled sync for hours each session. A recurrence rule with `FREQ=secondly`, or several times of day on a daily rule, got past the case-sensitive sub-daily check, and the vendored rrule walked every occurrence from DTSTART, freezing the store thread for seconds to hours on every reminder poll. 128 invites with low event ids pushed the user's own series out of the projection. Unsubscribe's fallback after a refused one-click POST sent the list's email without asking, and a free-mail sender's own domain (gmail.com) vouched for any address there. A quoted envelope sender could inject a `dmarc=pass` into Gmail's own `Authentication-Results`. The attachment blocklist missed types that run code (`.pyz`, `.wsb`, `.rdp`, `.xll` and others); Download kept leading dots and bidi characters; any bytes named as video reached FFmpeg. In HTML, a repeating background was scaled to its CSS size (up to gigabytes), floats laid out superlinearly with the time budget checked only after layout, and nested font sizes reached infinity. `fm` printed terminal escape sequences from mail. Linux notifications rendered markup from subjects.
- **Parsing.** A linear estimate of html5ever's work runs before every parse; HTML over it becomes a text-only `limited` view. Text extraction reads at most 1 MiB of HTML. The unsubscribe scan is iterative. Sanitiser version 6 also caps floats at 200 and background sizes (Core99).
- **Calendar.** Sub-daily is judged on the parsed rule and includes more than one time of day. The vendored rrule charges every period against a 400,000-candidate budget. Series rank by the user's own response before first-seen order (Calendar47, Core99). Refusing several times of day changes one legitimate case, such as "daily at 9:00 and 17:00", which now shows as unsupported; Google Calendar's editor cannot create it.
- **Unsubscribe and verdicts.** Every unsubscribe email goes through the confirmation, which shows address, subject and text, and the engine refuses one whose confirmed address differs. A mailto address must be on the verified From domain or a subdomain, and at shared providers must be the From address. Gmail's header is split respecting quotes, conflicting results fail, and DMARC failure is never verified (Gmail46, Mail UI154). This supersedes D210's automatic fallback and "any authenticated domain" rule.
- **Attachments.** Opening is an allowlist of documents, plain text, pictures, audio, video and zip, judged by both the export's and the sender's extension. Download names are cleaned like the engine's export names, keeping a readable stem, and never land in the home folder itself. Only MP4/QuickTime and Matroska/WebM with matching magic bytes reach the player, and only after Play (Mail UI154).
- **Reader.** Repeating backgrounds paint through a scaled brush, litehtml can be stopped at a 2 s deadline on every layout, and font sizes are clamped (Mail UI154; litehtml patch in VENDORED.md).
- **CLI and notifications.** `fm`'s human output replaces control and bidi-override characters (CLI64); Linux notification bodies are escaped.
- **Evidence.** Regression tests reproduce each attack (listed in Quality92); the stack and sender-check tests were confirmed to fail on the old code. Workspace clippy and `cargo fmt --check` pass. `cargo test --workspace` passes except `remote_inventory_refresh_is_bounded_owned_and_does_not_restart_for_an_additional_owner`, which fails the same way on the unchanged tree; on Windows the whole Rust suite passes. The Windows release build and every ctest test pass except gui-mail-gpu, which hits its 60 s limit because Windows clipboard retries stall it on this host; run directly it passes in 65 s, and on the unchanged tree it fails earlier in list geometry with the same clipboard retries.
- **Not checked.** macOS and Linux builds. A live Gmail run of the confirmed fallback email. Whether Gmail accepts a quoted envelope sender carrying `;`. How many real newsletters lose their email unsubscribe to the stricter domain rule.

Core99, Gmail46, Mail UI154, Calendar47, CLI64, Quality92. No schema, fixture or protocol-version change; `google_unsubscribe_mail` gains a required `address`, and cached HTML views rebuild for sanitiser version 6.

## D213 — audit fixes for parser escape and calendar output priority (2026-10-21T17:08:39Z)

The pre-commit audit found two gaps in D212, and the founder requested surgical fixes before committing.

- **HTML.** A `font` tag with `color`, `face` or `size` escapes SVG/MathML in html5ever. The new cost guard missed that transition, letting 50,000 following `<g/>` tags pass as cheap while the real parser nested them. Both sanitizer and source-text probes exceeded 8 s. Attribute presence now controls the transition, including empty and valueless forms; costly input uses the existing limited text view. The regression covers both namespaces and non-breakout controls.
- **Calendar.** Response rank chose the 128 admitted series, but output still iterated event IDs. A month of unanswered daily invitations filled the 1,000-row cap before an accepted weekly series. Both admission and output now use the same priority; the final rows still sort by time and incomplete coverage stays explicit. The day/week/month regression retains 1/1/5 accepted meetings; before the fix the month retained zero.
- **Test isolation.** The draft-inventory failure recorded in D212 was a fixture race: first label import wakes every worker and could advance the scan before the owner-join assertion. The test now completes that import through Core before starting sync. Production sync behavior and the strict generation, HTTP, refresh-coalescing and shutdown assertions are unchanged. Twenty consecutive runs pass.

- **Verification.** The full WSL Rust workspace suite passes (924 passed, zero failed, nine existing opt-in tests ignored), as do strict workspace Clippy, formatting, repository hygiene and diff checks. The Windows release rebuild and all four affected GUI tests pass (108.11 s). Native macOS/Linux GUI builds, packaging and live Gmail unsubscribe were not rerun.

Core100, Calendar48, Quality93. MIME fixture1, demo fixture3, schema and protocol unchanged. Sanitizer version6 remains the unreleased version introduced by D212. Measurements are in the owning HTML and Calendar research records; generated logs remain outside Git.

## D214 — Today before This Week in the mail list (2026-10-21T21:59:40Z)

The founder requested a Today section above This Week, leaving the rest of the week's mail in This Week.

- Done conversations are assigned to Today by their latest message's local calendar date. This uses the existing mail time zone and refreshes at midnight; it is not a rolling 24-hour window. To review and Delayed keep their current precedence.
- Core supplies disjoint counts and excludes folded Today rows before paging. The GUI uses the existing date-header style and stores `done:today` alongside the other fixed section folds. Today starts expanded and disappears when empty.
- Existing Core and native list regressions now exercise the split, paging, saved folds, week-start changes and daylight-saving boundaries. Measured results belong in the mail-list research and quality contract.

- **Verification.** All 33 Core demo-profile/query tests, strict workspace Clippy and formatting pass. The Windows release build, native mail-selection test and whole-app GUI integration pass; repository hygiene and diff checks pass. The test data is synthetic.

Core101, Mail UI155, Quality94. Schema, live0/demo3 fixtures and protocol1 unchanged.

## D215 — contact card on composer recipients, room above a forward (2026-10-22T18:51:00Z)

The founder asked for two composer fixes. Recipient chips showed only a name, so the address being written to was hidden; they asked for the address, or better, the contact card on hover as everywhere else. And a forward's first line was the "Forwarded message" rule, so they had to add line breaks before typing.

- **Contact card.** Chips reuse the reader's card and its hover rules rather than showing the address in the chip, which would crowd the To row; the card shows the address and recent mail. Only the name opens it, so reaching for the remove button doesn't pop a card. `RecipientField` and `ReplyComposer` only emit signals; `MailView` owns the one card, as it does for the reader's names.
- **Opening a thread from the card.** The card's recent conversations now go through `leaveComposerFor`, matching the composer's recent rows, so a new message is left cleanly instead of staying open over another thread.
- **Forward body.** The body rule already promised two empty lines for the author before the signature and forwarded text; without a signature they were left out. They are now always there before forwarded text. An empty reply or new message stays empty rather than gaining blank lines nobody asked for.
- **Opening at the top.** The founder then found the forward opened scrolled to its end. Loading a draft left the editor's cursor after the inserted text, and the body scrolled to keep it in view. A load that replaces the words now puts the cursor at the start. That also applies when a saved draft is reopened, which then opens at its top rather than its end.
- **Evidence.** A new fm-core test checks the body for each combination of signature and forwarded text, that an unsigned forward starts with two empty lines then the rule, and that an unsigned reply stays empty. ComposerInputTest hovers a real To chip, clicks it and moves away, and checks the composer asks for, shows and releases the card for that address. It also loads an 80-line forward and checks the cursor is at the start and the body at its top; it failed before the change. The GUI composition self-test now expects the two lines on an unsigned forward. The fm-core suite, workspace clippy and the `build/dev` ctest suite pass, except gui-debug-console, which timed out while the fm-core suite loaded the machine and passes on an idle one.
- **Not checked.** The card opening over the composer in the running app: no test loads `MailView` with an open composer and the card, and the founder's app runs an older build.

Mail UI156, Core102. No schema, fixture or protocol change.

## D216 — HTML forwards send the original as received (2026-10-22T21:40:00Z)

The founder found two more forward problems. The new forward opened scrolled to the end of the forwarded text (fixed under D215). And a forward always quoted the original as plain text, whichever way it was being read; they asked for it to keep that format.

- **Choice.** The composer edits a deliberately small HTML subset (bold, italic, underline, lists), with no images or CSS, so an HTML email cannot go into the editor intact. Offered either the exact original, read-only, or an editable copy that keeps only basic formatting, the founder chose the exact original, as Gmail sends it.
- **Which format.** Forward follows what the reader shows: HTML when the sender is in the account's `html_senders`, else text. Core falls back to text when the message has no HTML part, or one over 512 KiB, half the builder's 1 MiB body bound.
- **Where the original lives.** The draft records `forward_html` in its routing JSON rather than a new column, so there is no schema change, and the HTML is never copied into the draft. Sending, Gmail draft upload and capture read it from the verified source each time through one function, `compose_prepared`. The original goes out unchanged (its styles and body), outside the authored-HTML allowlist that would strip it; mail clients don't run its scripts, and Gmail sends forwards the same way. Inline images are read from the source with their own Content-IDs, because sync never stored them as attachments.
- **Composer.** The original shows read-only in the message area, under the author's words, through the reader's own HTML view and headed as a plain forward is. A first version put it under the Send bar, outside the form; the founder found the message area looked blank and confusing, so it moved inside. It sits in the editor's bottom padding rather than in a column with the editor, which keeps the editor the scroll view's own content, so it still scrolls to follow the cursor. A forward reopened away from its conversation only says the original is sent as received and where to see it.
- **Evidence.** A new fm-core test forwards a multipart message with HTML, a `<style>` block and an inline logo. It checks: the bad format is refused; text stays the default; an HTML forward's body starts empty and its flag survives a save; the built MIME has the author's words before the forwarded header, the original's style and body without its `<html>` or `<body>` wrapper, a plain part quoting the original's text, and the logo with its original Content-ID and bytes. A message without HTML forwards as text even when HTML is asked. ComposerInputTest starts a forward for an HTML sender and checks the request asks for HTML. It also checks the original shows inside the message area under the plain forward's header, with the author's words apart and room made for it below them. The fm-core, fm-mail and fm-cli suites, workspace clippy and the whole `build/dev` ctest suite pass.
- **Not checked.** A real HTML forward sent through Gmail and opened by a recipient, and the read-only view inside the running app; both need the founder's app on this build.

Core103, Mail UI157, CLI65. No schema, fixture or protocol-version change.

## D217 — OpenPGP keys, signed and encrypted mail (2026-10-23T00:41:00Z)

The founder asked for PGP support: use existing keys, generate new ones securely that can be exported to other applications, follow best practice, manage keys in a new Settings section, and show which accounts have a key, end to end. This brings message signing and encryption into scope; product scope had deferred them until there was a concrete need, and this request is that need. Evidence is in the [OpenPGP research](research/openpgp.md).

- **Library.** rpgp (`pgp` 0.21.0, MIT OR Apache-2.0, pure Rust) in a new pure crate, `fm-pgp`. Sequoia was set aside for its LGPL license and C crypto backend, GnuPG/GPGME for needing a separate installation and background agent. rpgp checks signatures but leaves validity to the application, so `fm-pgp` decides which self-signatures count (newest valid one wins), when keys expire or are revoked, which hashes are acceptable (SHA-1 only for key bindings made before 2023-02-01; MD5 and RIPEMD-160 never), that RSA, DSA and ElGamal keys under 2048 bits are weak, and that signing subkeys need a back-signature. A signature also reports whether the signer's user IDs include the From address.
- **New keys.** Version 4 Ed25519 with a Curve25519 encryption subkey, expiring in three years, matching GnuPG 2.4.8's defaults; RSA 4096 is the alternative. Not version 6: GnuPG 2.4.8 refused one, and keys must work in other applications. Imported version 6 keys still work.
- **Secret keys at rest.** Unprotected inside an AES-256-GCM envelope bound to the fingerprint, under a 256-bit key held in the OS keychain (one entry per profile) and cached in engine memory, as Thunderbird does with its password manager. No plaintext fallback: without the keychain, secret keys can't be used. So that the person never types a passphrase to read or send, the passphrase on an imported key is removed. Demo profiles keep the sealing key in an owner-only profile file.
- **Export.** Public keys, revocation certificates (armored as a public key block, which GnuPG requires), and secret keys protected with a passphrase of at least 12 characters (iterated and salted SHA-256, AES-256), written to a new owner-only file. Creating a key opens the backup at once, since a profile copied to another machine can't open its secret keys.
- **Accounts.** Each account may use one own key naming its address, with Sign and Encrypt defaults for new drafts. Keys are profile-wide; other people's public keys are imported the same way. No keyservers, WKD or Autocrypt: those would contact services or other people's domains on the founder's behalf, and keys come from files and pasted text.
- **Messages.** PGP/MIME (RFC 3156): `multipart/signed` with SHA-512, or `multipart/encrypted` signed inside, AES-256 in SEIPD version 1 (AES-128 only when a recipient lacks AES-256), never SEIPD version 2, which GnuPG doesn't read. Every message is also encrypted to the sender, so Sent stays readable; Bcc recipients get anonymous key IDs. A recipient without a usable key stops the send before anything is frozen, and nothing is ever sent unprotected in place of a failed encryption. The subject stays visible, because the Gmail API threads by subject. Attachments are now always base64, and fm-pgp converts bare LF to CRLF before signing, because the composer's 7bit text attachments broke signatures.
- **Drafts.** An encrypted draft never goes to Gmail Drafts, where it would be stored in the clear (`pgp_draft_local`). A reply to encrypted mail starts encrypted.
- **Reading.** Encrypted mail is stored as received and decrypted in memory each time it is read: the plaintext is never written to the database, the view cache, search or local models. Opening or previewing one of its attachments, or showing its inline pictures, needs a file; those go to a `decrypted` folder in the profile that the engine empties as it starts and stops, rather than the long-lived image cache. A copy the person saves elsewhere is theirs. GnuPG's OCB packet is accepted (it is authenticated, and GnuPG 2.4 uses it by default); unauthenticated encryption without an MDC is refused. Plaintext is capped at 64 MiB. Mail synced before this change is marked by a one-time background pass.
- **Evidence.** `fm-pgp` tests cover generation, expiry, sealing, export and import, revocation, signing, verification and encryption, including GnuPG-made keys and messages; GnuPG 2.4.8 imported our keys and revocation, verified our signatures and decrypted our mail, and fm-pgp read GnuPG's PGP/MIME, inline, clear-signed and OCB mail. `fm-core` covers the keyring requests, account rules, protected sending, draft upload exclusion, sync marking, repair and reading; the engine test covers the requests over IPC on a demo profile; the GUI integration self-test creates a key through Settings, checks the backup rules, sets Encrypt by default and reads the account row back through the CLI; the mail-selection test checks the list's lock and the reader's notices for a missing key and a signature that doesn't match.
- **Verification.** On macOS 15 (arm64): `cargo test --workspace` passes with no failures, as do strict workspace clippy, formatting, repository hygiene and diff checks. In the `build/dev` ctest suite 37 of 38 GUI tests passed with three running at once; gui-mail-gpu missed its 100 ms month-opening budget under that load and passes run alone.
- **Not checked.** A PGP/MIME message through real Gmail in either direction; clients other than GnuPG (Thunderbird, Proton, Mailvelope); the keychain entry on Windows and Linux and the macOS keychain prompt for it.

Core104, Mail UI158, CLI66, Gmail47, Search12, Platform27, Quality95, Local assistance33. Schema 41; fixtures live0/demo3 and protocol1 unchanged.

## D218 — Encrypted mail opens sealed; Encrypt waits for every recipient's key (2026-10-23T06:45:00Z)

The founder tried D217 between their two accounts and reported five problems: Encrypt could be pressed when a recipient had no key and then failed; their own encrypted sent mail showed in the clear; an encrypted message they received showed raw MIME with no way to decrypt it; an attached `signature.asc` looked like any other file; and Settings → Encryption's Sign and Encrypt switches seemed to do nothing and made the page jump. They asked for sent encrypted text to be redacted with a click to reveal, a distinct encrypted-message view in the spirit of the forward layout with a clear Decrypt button when there is a key, a distinct look for OpenPGP attachments, and switches that set the composer's defaults, with Encrypt staying off without a recipient key. Evidence is in the [OpenPGP research](research/openpgp.md#through-real-gmail).

- **Gmail rewrites PGP/MIME.** Read from the founder's two mailboxes: both senders' Sent copies were as sent, but the recipients' copies were `multipart/mixed`, the encrypted one with its armored message in a text part. The engine decrypted it and showed the plaintext entity as text, the raw MIME the founder saw. Decrypted inline content that is a MIME entity is now read as PGP/MIME. A rewritten signed message can't be verified (its signed bytes are gone); this is recorded, not worked around.
- **Sealed until asked.** Encrypted mail is no longer decrypted just because it is opened: `message_source` and `message_html` decrypt only with `decrypt: true`, and otherwise report `sealed` with whether one of the profile's own keys is a recipient, without touching the keychain. The reader shows redaction bars with Decrypt (received) or Reveal (sent). The choice lasts for the GUI session, in memory, with Hide to seal again. This replaces D217's decrypt-on-open; plaintext is still never stored.
- **Encrypt needs every key.** Drafts take the account's Encrypt default (and the reply-to-encrypted rule) only when every recipient has a usable key; the unsaved reply under a conversation, and a discarded draft reopened, now take the defaults too, which is why the switches seemed to do nothing. In the composer Encrypt is disabled while off and any recipient lacks a key, and a draft the person hasn't set follows the default as recipients change. Once set by the person it stays as set.
- **OpenPGP files** (public keys, signatures, encrypted data) get an accent card, and a public key imports from the message with a click; Gmail's leftover version part is hidden.
- **Settings jank.** Each switch reloaded the account rows twice and cleared every held message view, and both lists were rebuilt from fresh arrays. Rows are now keyed by id, unchanged keyring lists aren't re-emitted, account default changes no longer clear message views, and the switches update at once without dimming or a status line.
- **Verification.** On Windows 11 (x64, release): `cargo test --workspace` and strict workspace clippy pass; new `fm-pgp`, `fm-core` and GUI selection tests cover the Gmail rewrite, sealed reads, recipient-aware defaults and the sealed reader. 37 of 38 GUI tests passed in a sequential run; gui-mail-selection failed there right after a relink and passed three of three on its own.
- **Seen in the app.** On the release build with the founder's mailboxes: the sealed received message with Decrypt, its decrypted text, the sent copy's Reveal, and the Signature card. Local-model summaries are hidden on encrypted mail, as the model never sees its text and had summarized it as having no content.
- **Not checked.** A real mouse click on Decrypt or Reveal, the composer's Encrypt gating and the Settings switches in use; a public key received in real mail; whether another Gmail upload path keeps `multipart/signed` intact.

Core105, Mail UI159, Quality96. Schema 41; fixtures live0/demo3 and protocol1 unchanged.

## D219 — OpenPGP audit: keys from mail are previewed, only protected content counts (2026-10-24T22:06:02Z)

The founder asked for a security audit of the OpenPGP feature against best practice, then for every finding to be fixed, the low ones included, with tests. Evidence is in the [OpenPGP research](research/openpgp.md#audit-2026-10-24).

- **Keys from mail are previewed.** A key attached to mail imported with one click: no preview or fingerprint, secret keys accepted, any address claimed. With the newest key winning an address and signatures trusted on their user IDs, a stranger's key could take over someone's address. `pgp_key_import` now takes `preview` (what the file holds, each key's `add` or `update`, revocations and conflicts; nothing is stored and no secret key is unlocked), `public_only` (a secret key is `secret_key_not_allowed`) and `replace`. A key naming an address another stored key has, or one of the profile's account addresses when it isn't an own key, fails with `pgp_key_conflict` listing them, until `replace`. The reader's key card opens a preview with the user IDs, the fingerprint, a warning when the key isn't for the sender, and the conflicts; one that takes an address imports with **Import anyway**. Settings' Import key shows conflicts the same way, and `fm pgp import` has `--preview` and `--replace`.
- **Which key an address uses.** The one the person stored last (insertion order), not the newest by its creation time, which whoever makes a key chooses. Keys new in one import are stored oldest first.
- **Imports are cleaned and capped.** Merging keeps only signatures the primary key made and that verify: self-certifications, direct-key signatures and revocations, and subkey bindings. Third-party certifications and parts left with no signature are dropped, so a public key can't be flooded with junk, as SKS keyservers were in 2019. A stored key over 1 MiB is `limit_exceeded`.
- **Only protected content shows as protected.** Inline OpenPGP showed text before and after the armor, and the message's other files, under the signed or encrypted badge (the "Johnny, You Are Fired" partial-signing class). Text outside the block is now left out, and `pgp.unprotected` says there was other text or files; the reader says so. Signatures count only when they are binary or text document signatures: rpgp already refuses certification types over data, but hashes a standalone or timestamp signature over the first byte of the data only, so one could be moved onto any message.
- **Signed files stay listed.** Any `application/octet-stream` file inside signed mail was taken for PGP/MIME plumbing, hidden, and deleted by the repair pass. Only the detached signature (a direct child of `multipart/signed` after the first) and the version and payload parts of `multipart/encrypted` count now. A one-time pass restores the files' rows.
- **Encrypt never turns itself off.** The composer turned Encrypt off when a recipient had no key, and the reply's text then went to Gmail Drafts. Replacing part of D218: a reply (not a forward) to encrypted mail starts encrypted even when a recipient has no key, and sending waits (`pgp_recipient_key_missing`) with the draft kept off Gmail Drafts, until the key is imported or the person turns Encrypt off; then a warning says the reply is saved to Gmail Drafts and sent readable. Drafts (`replies_to_encrypted`) say whether they answer encrypted mail. Encrypt by default still turns on only once every recipient has a key.
- **Exports are written by the engine.** The GUI wrote key files with `QSaveFile`, which on Windows inherits the folder's access list, and the CLI's `0600` was Unix-only. `pgp_key_export` now takes an absolute `path` and `replace`: the engine writes the file readable only by the user (mode 0600, or on Windows a protected access list granting the current user alone), new unless `replace`, which writes a temporary file and renames it over the old one. An existing file without `replace` is `already_exists`. The GUI passes the save dialog's file with `replace`, as the dialog has already asked; `fm --output` makes new files only. A keyring plan with a path run in place (`Store::handle`) is refused, so only the engine writes key files. The demo profiles' sealing-key file uses the same helper.
- **Header protection and intended recipients.** The subject, sender, recipients and date were outside every signature. Following RFC 9788, signed mail now carries those fields inside the signed part (`hp="clear"`), and encrypted mail inside the encryption (`hp="cipher"`) with `HP-Outer` lines recording what was visible outside. The outer Subject is unchanged, as Gmail threads by it. Reading reports `pgp.headers: {subject, altered}`, the fields changed after sending, and the reader shows the sender's subject when it changed. Signatures inside encrypted mail name each recipient's primary key (RFC 9580 intended recipient fingerprints), except when there is a Bcc recipient, whom the list would reveal. A signature naming recipients that don't include any key that decrypted the message reports `forwarded`: it may be someone else's mail re-encrypted to you, so the reader says so and doesn't show it as the sender's.
- **Backups.** Exported version 4 secret keys use the iterated and salted SHA-256 S2K at its highest count (255, 65,011,712 bytes hashed) instead of rpgp's 224 (16,777,216). Version 6 keys keep rpgp's Argon2 default.
- **The engine is the boundary.** Sealed until Decrypt, and the backup passphrase rules, are the GUI's and CLI's behaviour, not a barrier between programs: any program running as the same user can connect to the engine (it can also read the profile) and ask it to decrypt mail or export a secret key under a passphrase it picks. On macOS that skips the keychain's per-app prompt, since the engine holds the sealing key. That is accepted, as OS accounts are the boundary everywhere else in the app, and [SECURITY.md](../SECURITY.md) now says so.
- **Verification.** On Windows 11 (x64, release build): `fm-pgp`, `fm-core`, `fm-ipc`, engine and CLI tests cover each finding, and `cargo test --workspace`, strict workspace clippy, formatting and repository hygiene pass. `gui-mail-selection` covers the key preview, the reader's new warnings and the encrypted reply; the GUI self-test saves a backup through the engine over an existing file. 37 of 39 GUI tests passed in one ctest run. gui-calendar-search, which checks wake timing, failed there and passed three runs of three with this change, while the committed QML failed two of three; gui-mail-gpu, which runs none of this code, timed out waiting for the Windows clipboard with the committed QML too.
- **Not checked.** A real mouse click through the key preview; how Thunderbird, Proton and Mailvelope show the protected headers and intended recipients; whether Gmail keeps `hp` parameters on delivery.

Core106, Mail UI160, CLI67, Platform28, Quality97. Schema 41; fixtures live0/demo3 and protocol1 unchanged.

## D220 — Each account has its own From name (2026-10-25T02:15:09Z)

The founder noticed that mail went out under the account's display name, the label fuckMail shows in its top bar, and asked for a From name per account that starts from Gmail's send-as name and is edited in Edit account. A new account's display name defaults to its address, so such accounts sent as `you@example.test <you@example.test>`.

- **Separate names.** The display name stays a label inside fuckMail. The From name is stored beside it, with Gmail's send-as name for the primary address kept next to it. Mail uses the person's From name, else Gmail's name, else the display name when it isn't only the address, else no name. Keeping the display name as the last fallback keeps today's From line for people who named their account after themselves and set no name in Gmail; leaving the address out avoids repeating it as a name.
- **Gmail's name.** Sign-in asks only for `openid` and `email`, not `profile`, so the Google account's own name isn't available. `settings.sendAs.list` is, under the mail scopes already granted. Gmail's primary entry often has an empty display name (Gmail then uses the Google account name), and the fallbacks cover that. The labels worker reads it every ten minutes for 1 quota unit. Writing the name back to Gmail would need `gmail.settings.basic` and change Gmail's own setting, so the choice stays local.
- **Drafts** keep the name they were made with, as they already did for the display name.
- **Verification.** On Windows 11 (x64): new tests in `fm-core` (the fallback order, saving, clearing and validation, the From header of a sent capture, the upgrade from schema 41), `fm-google` (the send-as call, its cost and validation) and the engine's labels worker (reading, keeping the saved name on a bad reply, clearing it), plus `gui-mail-selection` for the Edit account field.

Core107, Gmail48, Mail UI161, CLI68, Quality98. Schema 42; fixtures live0/demo3 and protocol1 unchanged.

## D221 — Conversation cards; every new list starts at its top (2026-10-28T19:20:20Z)

The founder supplied an updated terminal prototype (a standalone export, as the design project couldn't be read from this session) and asked for the mail list to follow it. They also reported that after reading mail and switching mailboxes or searches, the list sometimes stayed scrolled down, so older mail looked as if it were still in To review, and took it for a recent regression.

- **Cards for conversations.** Inbox and All mail show each conversation as a framed card whose top border carries the first label, message and attachment counts, and the time with an unread dot, following the prototype. Sent, Drafts, Spam and Trash keep their rows, as the prototype does. The prototype's card has no sender-kind glyph; it is kept, since the starred-contact star shares its place.
- **One row high still.** The prototype puts 10 px between a section title and its first card and 24 px above the next title. With every Inbox item one row high (D175), a card's item is its 62 px plus the 14 px gap, and a title's item is as tall, so the title is centred in it: 22 px to the first card and 36 px above the title rather than 10 and 24. Keeping the exact list length and scrollbar mattered more than those few pixels.
- **Empty sections.** On the founder's build an empty To review or Delayed took two rows, the title's item and a separate empty item, 152 px for one line, and looked unbalanced beside the cards. The empty line now sits under the title in the title's own item, so an empty section is one row like any other. The cost is D189's swap: a last row leaving a section that stays shown turned into the empty line in place so nothing moved. With no empty item left to turn into, it now folds like other rows and what's below glides up one row. Keeping the swap would have meant a row-high empty item again.
- **Why the list stayed down.** Each account keeps a saved list offset, and switching accounts restored it by design (D175, from the Phase 1 per-account view). Whether it showed depended on timing: the reset of the list to its top saved 0 over the account's offset only when that reset moved the view, and a restore while the view was still being restored skipped that save. Labels and People/Automated update the list in place (D148), which also kept the old offset. Git history shows no recent change to these paths, so the regression the founder noticed is more likely this timing showing up more often than new code.
- **The fix.** Any change of account, folder, search, labels or sender kind moves the list to its top and saves offset 0, so every later restore, including the account's view arriving after a switch, keeps it there. Section folds, refreshes, paging and new mail still leave the list where it is. Reopening the app also starts at the top, as its first account load is such a change. This replaces D175's per-account list position and D148's "a search is placed by the saved list position".
- **Delegates kept in rows.** Qt keeps 320 px of delegates past each edge of the list. That was six 52 px rows and is four cards, so a refresh moving the open conversation six rows rebuilt its delegate. The list now keeps six rows' worth (`cacheBuffer`).
- **Verification.** On Windows 11 (x64, release build), all 38 GUI tests pass. `gui-mail-selection` checks the cards, the legend, an empty section being one row, and the list returning to its top for a sender kind, a search and an account switch; without the fix, the sender-kind and account checks fail. Not checked: the Paper theme, compact density and large text on screen, and a live account.

Mail UI162, Quality99.

## D222 — The Inbox list in slices, so section titles sit 52 px apart (2026-10-30T21:23:41Z)

The founder reported that the month sections in the Inbox were too far apart after the card change: folded months stacked 76 px apart, where they had been 52 px with rows.

- **Why.** D175 makes every Inbox list item one row high, because Qt's ListView sizes the items it has not built from the average of those it has; equal items keep the list's length and scrollbar exact. D221 kept that, so a title's item became a card's 76 px. Shorter title items alone would bring back the moving scrollbar D175 fixed. Asked to choose, the founder picked keeping the list exact over a small change with an estimated scrollbar.
- **Change.** The unit is a slice, a third of a card and its gap rounded up: 26 px (23 compact). A card takes three slices and a title two, each slice an item of its own; a card and a title draw over their later slices. Titles now stack 52 px apart and sit 10 px above their first card, as the prototype has them. Cards move 78 px apart, two pixels more, to make the three slices whole.
- **Empty sections.** The empty line moves from the title's item to one more slice under it, so an empty section is still one card's place. When the last row folds out, that slice grows while the row folds, and what is below glides up two slices instead of one row.
- **Cost.** The Inbox model holds three items per loaded conversation instead of one; the extra ones are bare items. A card's first slice can sit above the view while the rest of it shows, so the list draws two slices past its top.
- **Verification.** On macOS (arm64, debug build), `gui-conversation-list-model`, `gui-mail-selection`, `gui-mail-gpu` and `gui-mail-navigation-cache` pass with `listGeometry` checking that every item is one slice and the length is exact, in both densities. Of the 38 GUI tests, 37 pass; `gui-integration` stops at its OpenPGP backup permission check, as it does on the unchanged code. Screenshots of the demo profile show folded sections 52 px apart and a card cut by the top edge drawn whole. Not checked on screen: the empty line's place, compact density, large text, Windows and Linux, and a live account.

Mail UI163, Quality100.
