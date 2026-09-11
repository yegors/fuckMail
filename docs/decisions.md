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
