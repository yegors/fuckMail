# Development plan

Updated 2026-09-11. **Plan prepared; execution has not started.** This is the route for Codex to build the product when implementation is requested. No application code, build files, test harness or installer is being added in this planning pass. MVP readiness remains **0/10 verified**.

[Status](status.md) records progress; [contracts](contracts/README.md) own behavior; [research](research/README.md) owns evidence; [decisions](decisions.md) records changes. This plan owns sequence, dependencies and the next execution slice. [Phase 0](plans/phase-0.md) contains the detailed starting backlog. Do not copy implementation specifications into competing plans.

## Direction and planning horizon

Use the proposed Rust engine/CLI, C++20/Qt Quick/QML UI, private local IPC and SQLite/FTS5 baseline for feasibility work. Keep one engine, one schema and one operation journal. The [stack assessment](research/desktop-stack.md) is a recommendation, not proof or a version lock.

The [closer mail/calendar references](research/design-references.md) guide structure. Final mockups will arrive closer to UI implementation; they are not a prerequisite for headless work or disposable rendering/input experiments. Review the relevant final screen/state set before implementing its production screens and interactions. Do not infer Done/Delay effects, count definitions or calendar permissions from sample pixels.

Planning detail deliberately decreases with distance:

- **Phase 0:** concrete investigation packets, prerequisites, outputs, pass/fail criteria and pivot triggers.
- **Phase 1:** small implementation slices with integration gates; refine exact tasks after Phase 0.
- **Phases 2–4:** product outcomes, dependencies and release evidence; expand only the next phase when it approaches.
- **Later:** scope labels only. No speculative APIs, dates, staffing estimates or comprehensive ticket inventory.

A phase is an integration milestone, not a rule that every stream must wait for the previous stream. Research, platform work, protocol fixtures and calendar work start early. Within an authorized phase, continue independent work when an external dependency blocks one packet; do not silently count the blocked evidence as passed.

## Phases

| Phase | Deliverable | Exit evidence |
| --- | --- | --- |
| 0 — Prove the foundation | Reproducible builds, a private Rust engine/CLI and Qt adapter experiment, safe local persistence, early mail/calendar feasibility and installation evidence | A recorded baseline for the next slice, shared contract/fixture revision and clear open gates; see the narrower entry rule below |
| 1 — Local product slice | Synthetic mail and calendar flow through the real store, engine, CLI and minimal GUI; durable human drafts and honest triage | Both interfaces agree; restart, offline content, account separation, cancellation and coverage work without any provider/model |
| 2 — Google daily workflows | Consumer Gmail and Google Workspace mail/calendar, account setup, human composition, provider-native labels and full-history acquisition | Controlled real-account interoperability, durable sign-in, correct sync/operations and useful daily mail/calendar behavior |
| 3 — Required completeness | IMAP/SMTP, complete calendar/feed/invitation behavior, optional Ollama/vLLM classification/summaries and remaining CLI/GUI coverage | Every required capability has integrated behavior and domain tests; no Google-only or read-only MVP shortcut |
| 4 — Daily-driver release | Verified performance/recovery, reviewed final design, smooth packages/updates and founder trial on required platforms | All ten area contracts and applicable platform/quality gates pass; honest readiness update |
| Later | Editable CalDAV, possible open-source/public launch | Separate requirements and decisions; no effect on current MVP completion |

## Phase 0: remove expensive uncertainty first

Execute the [Phase 0 work packets](plans/phase-0.md). Start with tool/access inventory and a small deterministic fixture baseline. Then run these independent tracks against shared identities and scenarios:

1. **Core/CLI/storage:** Qt-free Rust build, private local IPC, one engine per profile, SQLite/FTS5, durable draft/action/blob recovery and bounded queries.
2. **Qt/platform:** list and calendar surfaces, received HTML and human composer, required desktop input, and first installable fixture packages.
3. **Providers/calendar:** sustainable Google OAuth setup, synthetic Gmail deltas, MIME/IMAP/SMTP capability and outcome tests, recurrence/time-zone and feed fixtures.
4. **Triage/assistance:** independent unread/star state, mapped categories and late-result rejection with fakes; optional runner capability/cancellation checks when a supported local setup is available.

Do not spend this phase polishing the entire app, downloading real historical mail, selecting a permanent model, or implementing every command.

**Entry to Phase 1 is per ready slice.** Require a recorded provisional stack/build choice on available target systems, a passing core/CLI/Qt boundary slice, draft/operation/blob persistence evidence, correct basic FTS queries and the consumed shared fixture/interface revision. The Qt UI slice additionally needs reader/composer feasibility; a provider slice needs its own library/identity prerequisites.

A missing Mac or Omarchy test host does not stop independent local-core work, but the platform row stays unverified and no all-platform stack or release claim follows. Run cross-platform smoke packages early; do not postpone discovering a fundamental renderer/IPC/credential issue until release. Million-message benchmarks, sustained real-account refresh, complete IMAP coverage, model quality and polished signing remain explicit gates before their affected behavior ships.

At the gate, record chosen versions/features and evidence in the owning research/contracts, with a dated decision. Keep alternatives only where a specific test failed. If a pivot would reduce accepted behavior or change the product experience, bring the concrete tradeoff to the founder; otherwise resolve routine implementation choices within the authorized work.

## Phase 1: a small real local product

Refine these slices into executable tasks at Phase 0's gate. They use synthetic accounts/providers and the actual shared engine; fixtures are not a separate mock application that later gets discarded.

| Slice | Owners and prerequisites | Integrated behavior and completion evidence |
| --- | --- | --- |
| 1A — Workspace and ownership | A/G/I; proven build, IPC and storage approach | Create only the needed modules, migrations, protocol/client code and test entry points. Start one engine, connect CLI and GUI, inspect capabilities, and shut down correctly. Concurrent launches and version errors have deliberate outcomes. |
| 1B — Local mail and search | A/D/E/I; agreed message/conversation identity and source fixtures | Ingest/replay messages, index sender/subject/body, page/search/open a matching conversation member, and save/open retained attachments. GUI/CLI agree on account scope and incomplete coverage while ingestion continues. |
| 1C — Triage and human drafts | A/E/I plus B/C fake adapters; agreed read/star targets and mutation states | Review, hold, categorize and explicitly clear a hold; preserve mixed flags, new arrivals and manual overrides. Save/reopen a human-written plain/HTML draft and attachment. Capture outgoing MIME in a local sink; uncertainty never causes blind resubmission. |
| 1D — Calendar and shared shell | F/E/G/I; bounded occurrence/permission fixtures | Render one day/week and today's strip from the same occurrence view, edit a synthetic event through the journal, exercise stale/overlap/all-day cases, and preserve mail reading position. No separate calendar store or UI-only permission logic. |

The minimal GUI in this phase is provisional functional scaffolding with explicit low-level actions and replaceable presentation. It does not freeze production screens or unresolved interaction choices; the relevant final mockups govern their later implementation.

These are integration slices, not four serial teams. Search, Calendar, provider fakes and the Qt adapter can proceed independently after A publishes the relevant shared boundary. No stream invents an alternate schema to unblock itself.

Before Phase 2, demonstrate the source-to-store-to-search-to-reader-to-durable-action path and its CLI equivalent, plus one calendar occurrence/edit path. Interrupt transactions, lose a client, restart during a draft/blob write and run with no model or network. First packages launch the same binaries used in those tests. Expand the 100k query experiment toward the primary scale tier while the functional slice grows.

## Phase 2: Google mail and calendar together

Connect consumer Gmail and Workspace accounts separately through the shared engine. Implement account setup, durable OAuth/credential references, recent-first useful content, resumable full inventory/body/attachment acquisition, adaptive sync, labels and read/star reconciliation, human compose/reply/forward/signatures, and recovery for uncertain submission.

Build Google Calendar alongside mail: discovery and permissions, sync, bounded views and the strip, ordinary event edits, then recurring edits and invitation ownership as their fixtures pass. A successful Gmail demo must not defer Calendar's core correctness until the end.

Use the [Google](research/gmail.md) and [calendar](research/calendar.md) investigations for live checks. Workspace admin policy, project access and controlled recipients are explicit prerequisites when needed. C36 keeps public verification deferred for the current personal audience; sustainable OAuth still needs evidence. Do not call an expiring test login a daily-driver configuration.

Bring the relevant final mockups into production UI slices as they arrive. Initial real-account use may be deliberately limited while import, recovery and UI states mature; report those limits honestly. Provider mutations require the established journal and account/target rules, with controlled test data before primary daily use.

Exit on coherent mail/calendar workflows with assistance off, useful local search during backfill, offline access to completed content, cross-client reconciliation and recoverable failures. Full initial download coverage remains measurable; a recent-only cache cannot satisfy the final offline gate. IMAP/SMTP and full calendar fixtures continue in parallel.

## Phase 3: complete required behavior

Replan from Phase 2 evidence instead of freezing a long implementation list today. Deliver the remaining required capabilities through the same interfaces:

- Required IMAP/SMTP servers: capability negotiation, reconnect/reconciliation, durable local identity through folder moves, complete offline content, human submission and Sent-copy/uncertain-outcome handling.
- Calendar completeness: day/week/month/agenda, search, recurrence and exceptions, invitations/RSVP, shared/limited permissions, read-only feeds, current/next/Join and running-session reminders.
- Optional local assistance: Ollama and supported same-machine vLLM, independent classification/summary settings, manual/Suggest/configured Auto-apply, correction precedence, source-linked stale/partial summaries and tested cancellation. No AI authorship or cloud fallback.
- Complete CLI coverage alongside each capability, including configuration, structured results, bulk/partial operations, bounded watch and actionable headless failures. GUI-only presentation controls need no artificial CLI equivalent.
- Remaining setup/settings, navigation, composition/recovery and dense/empty/error UI states from reviewed design slices.

Dependencies remain explicit: classification applies through proven manual filing, summaries consume preserved source content, recurrence precedes recurring edit UI, provider access controls apply before displaying shared data, and notifications use the agreed session lifetime.

Exit with no missing required area hidden behind a polished Google path. Library/server/model/platform limits are documented and resolved or brought back as explicit scope decisions; they cannot quietly shrink the MVP.

## Phase 4: earn daily-driver readiness

Use the [quality contract](contracts/quality.md) as the release authority. Test the primary million-message and meaningful offline-content scenarios, then growth/stress cases; measure grouping/snippets, UI paint, import contention, renderer processes and local inference separately. Tune evidence-backed bottlenecks before replacing a stack component.

Exercise interrupted imports/migrations/downloads, low disk, cursor expiry, concurrent edits, missing/locked credentials, uncertain sends, revoked calendar access, time-zone transitions and runner shutdown. Complete installation, CLI discovery, update/recovery and uninstall on Windows, macOS, Ubuntu, Arch and Omarchy. Resolve signing, dependency notices and the maintained update route for the actual personal distribution before treating delivery as smooth.

Run the proposed two-week founder trial only after core reliability and real-account behavior are fit for daily use. Observe whether the app replaces eM Client, finds old mail and supports zero unread/zero starred through real review/action. Record unresolved holds without auto-clearing them. A trial on one machine does not substitute for the platform matrix.

Each of the ten required areas becomes Verified only with its contract and applicable shared gates satisfied. Failed budgets or changed scope need a recorded decision; neither screenshots nor unrun tests raise the percentage.

## Work ownership and future repository shape

The [contract index](contracts/README.md) defines A–I. A owns shared schema/protocol changes; G coordinates packaging/quality; other owners produce compatible adapters and evidence. Parallel work uses a named contract revision and shared fixture revision, then integrates small end-to-end changes.

Proposed locations below are for implementation later; they are not directories or APIs being scaffolded now:

| Location | Ownership and intended purpose |
| --- | --- |
| Rust workspace: engine/core, protocol/client, CLI and only then justified adapters | A owns persistence/domain/protocol; I owns command surface; B/C/F/H own provider/assistance modules |
| Qt desktop application directory | E/G own C++ presentation adapters and QML; F supplies calendar view behavior |
| Shared small fixtures and integration tests | A coordinates identity/revision manifests; every stream contributes domain cases |
| Build/package/CI definitions | G owns reproducible Cargo/CMake builds, artifact staging and platform jobs |
| Generated corpora and local profiles outside tracked source | No real mail/tokens, huge fixture blobs or mutable user state in the repository |

Choose physical directory/crate names during the first authorized bootstrap. Keep dependencies one-way: protocol types do not import storage or Qt; UI/CLI call the engine; providers return normalized data and operation outcomes; the engine alone persists intent; inference returns data without tools. Start with coarse modules and split where an independent boundary actually exists.

## Decisions at the point they become necessary

| Decision or missing input | Latest responsible point | Work that can proceed before it |
| --- | --- | --- |
| Actual toolchains, Qt artifacts, supported machines/OS floors and license/dependency route | Phase 0 build/packaging choice; every advertised platform before release | Documentation, synthetic fixture design and available-host experiments |
| ID/revision/cursor/operation/occurrence representation and protocol framing | Before consumers implement the relevant shared boundary | Isolated library and renderer experiments |
| Done/Delay targets, read trigger, mixed-state grouping and account count scope | Before production triage interaction; new mail must remain independent | Explicit low-level read/star commands and nonfinal visual experiments |
| Trash/spam scope, remote deletion, removal of accounts with unsent work, encryption at rest and recovery/export scope | Before real-store/migration or destructive-retention behavior is committed | Disposable synthetic storage/recovery experiments |
| Google project/admin access, sustained personal/Workspace auth, actual non-Google providers | Before the corresponding real-account gate | Mock sync, library evaluation, local sink and UI work |
| Final screen/state designs, calendar overlays, small-window behavior and remote-content presentation | Before production implementation of that UI slice | Headless work, renderer/input feasibility and fake data views |
| Runner/model availability and local-only/cancellation support | Before enabling that runner; quality thresholds before Auto-apply trial | Manual flows, fake inference and stale-result tests |
| Signing identities and update distribution | Before trusted personal delivery; investigate during Phase 0 | Explicitly identified local smoke packages |

This is a just-in-time decision list, not a request for another broad interview. At each phase boundary, preserve completed evidence, append pivots, update affected contracts and expand only the next executable slice. Keep [status](status.md) short and current; use scoped research for the detailed outcomes.

