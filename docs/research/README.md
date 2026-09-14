# Research index

Evidence, alternatives and experiments organized by subject. This index is navigation, not a general research notebook. Initial source checks were on 2026-09-10; exec-buddy/category/local-runner work and QML references were added or checked on 2026-09-11. The reorganization does not revalidate every external source. Phase 0 execution reports below distinguish tested synthetic behavior from live-provider and platform gaps.

| Scope | Existing material | Build contract |
| --- | --- | --- |
| [Evidence storage and recovery](evidence-archive.md) | Verified local snapshot, historical Git links and rules keeping generated outputs out of source | [Quality](../contracts/quality.md) |
| [Phase 2 execution](phase-2-execution.md) | Live-profile/account boundary, integrated tests and remaining real-account gates | [Core](../contracts/core-storage.md), [Platform](../contracts/platform.md), [Quality](../contracts/quality.md) |
| [Phase 2 OAuth/vault](phase-2-auth.md) | Desktop authorization, identity/refreshed grant fixtures and isolated native vault evidence | [Platform](../contracts/platform.md), [Gmail](../contracts/gmail.md) |
| [Phase 2 transport](phase-2-transport.md) | Bounded native HTTPS, quota scheduling and cancellation/fault evidence | [Gmail](../contracts/gmail.md), [Quality](../contracts/quality.md) |
| [Phase 2 Gmail HTTP](phase-2-gmail-http.md) | Typed Google mail endpoints, method costs and single-attempt submission outcomes | [Gmail](../contracts/gmail.md) |
| [Phase 2 Calendar HTTP](phase-2-calendar.md) | Catalog/events, limited-access availability and conditional write fixtures | [Calendar](../contracts/calendar.md) |
| [Flight Progress v2 design](flight-progress-v2.md) | Exact HTML/export support files, source and browser review, confirmed changes and native Phase 1 adaptation | [Mail UI](../contracts/mail-ui.md), [Calendar](../contracts/calendar.md) |
| [Phase 1 execution](phase-1-execution.md) | Completed local phase, schema/fixture boundary, measured integration and acceptance evidence | [Core](../contracts/core-storage.md), [Mail UI](../contracts/mail-ui.md), [Calendar](../contracts/calendar.md), [Quality](../contracts/quality.md) |
| [Phase 0 execution](phase-0-execution.md) | Native experiment evidence and outstanding gates | [Quality](../contracts/quality.md) |
| [Google feasibility execution](google-feasibility-execution.md) | Fake transport/auth/journal tests and live prerequisites | [Gmail](../contracts/gmail.md) |
| [MIME feasibility execution](mime-feasibility-execution.md) | Shared parser/builder/capture and retained-content tests | [Mail UI](../contracts/mail-ui.md) |
| [Calendar Core execution](calendar-core-execution.md) | Atomic fake sync/write receipts, retained recurrence identity, per-calendar permissions and shared display-zone boundaries/order | [Calendar](../contracts/calendar.md), [Core](../contracts/core-storage.md) |
| [Durable fake-assistance execution](local-assistance-execution.md) | Core source snapshots, permitted category transactions, stale/manual precedence and replay | [Local assistance](../contracts/local-ai.md) |
| [Search feasibility execution](search-feasibility-execution.md) | Measured 100k index experiment and limits | [Search](../contracts/search.md) |
| [Desktop stack and QML](desktop-stack.md) | Rust engine/CLI + C++20/Qt Quick UI recommendation, external-analysis assessment and feasibility gates | [Platform](../contracts/platform.md) |
| [Slint UI investigation](slint-ui.md) | Published examples, toolkit capabilities, reader/composer and platform gaps | [Mail UI](../contracts/mail-ui.md) |
| [HTML mail](html-mail.md) | Text-first reading, later best-effort HTML, human composition and content isolation | [Mail UI](../contracts/mail-ui.md) |
| [Gmail](gmail.md) | Current Phase 2 OAuth/scopes/quotas, history/offline sync, drafts/send and live-test prerequisites | [Gmail](../contracts/gmail.md) |
| [IMAP/SMTP](imap-smtp.md) | Deferred: identities, capabilities, folder moves and uncertain sending after Google dogfooding | [IMAP/SMTP](../contracts/imap-smtp.md) |
| [Calendar](calendar.md) | Current Phase 2 quotas/permissions, sync/free-busy, recurrence/invitations and live-test prerequisites; feeds/CalDAV deferred | [Calendar](../contracts/calendar.md) |
| [Search/storage](search-storage.md) | SQLite/FTS5, Tantivy, Notmuch, WAL and scale experiments | [Core](../contracts/core-storage.md), [Search](../contracts/search.md) |
| [Local AI](local-ai.md) | Ollama/vLLM setup, structured results, supported platforms and evaluation | [Local assistance](../contracts/local-ai.md) |
| [exec-buddy product lessons](exec-buddy.md) | Source evidence for selected mail/calendar interactions and rejected carryovers | [Triage](../contracts/triage.md), [Calendar](../contracts/calendar.md) |
| [CLI and local engine](cli-and-local-engine.md) | Headless access, shared process/state and lifecycle tradeoffs | [CLI](../contracts/cli.md), [Core](../contracts/core-storage.md) |
| [Linux and distribution](linux-and-distribution.md) | Required distro matrix, runtime packaging and smooth GUI/CLI installation | [Platform](../contracts/platform.md) |
| [Supplied mockups](design-references.md) | Historical Phase 0 PNG trio, earlier pair and mood references; superseded visually by Flight Progress v2 | [Mail UI](../contracts/mail-ui.md) |

Near-term research was expanded on 2026-09-11 for OAuth longevity/Workspace policy, Rust mail/calendar library candidates, Qt reader/composer limits, private IPC, SQLite/blob durability and clean installation. Named libraries and test recipes remain proposals; no implementation or live integration result is implied.

## Adding findings

Add material to the matching scope and preserve source/checked dates. Distinguish observed behavior from a project-fit inference, untested proposal or measured result. Record versions and reproducible commands with future experiment reports. Add a new scoped file and index row only when new material warrants one; do not create an all-purpose research.md or a collection of empty stubs.

When research changes a product/build choice, append the outcome to [decisions](../decisions.md), update the owning [contract](../contracts/README.md) and adjust [status](../status.md) as needed. An earlier recommendation does not override a newer founder decision.

## Experiment map

D009 supersedes earlier packet scope: P0-04 is a text/composer GUI paired with a small CLI; P0-05 is MIME/text/outgoing content for Gmail, without SMTP transport; P0-06 is Google calendar semantics. Legacy IMAP/SMTP and external-feed recipes stay retained for Phase 4 after founder Google GUI/CLI dogfooding. Best-effort HTML display is later work. The current phase plan wins over historical experiment labels in the research below.

The original phase-0 questions remain attached to their subjects: P0-01 inventory/scale and P0-03 indexing in Search/storage; P0-02 auth and Gmail filing in Gmail; P0-04 design/stack in Desktop and Slint; P0-05 libraries split across HTML, IMAP/SMTP and Calendar; P0-06 interoperability in Calendar and IMAP/SMTP; P0-07 categories in provider/triage work; P0-08 runners in Local AI; P0-09 product interactions in exec-buddy. [Quality](../contracts/quality.md) holds the proposed workload, budgets and trial criteria.

Added 2026-09-11: P0-10 tests a small CLI/shared local engine alongside the GUI and P0-11 tests packaged installation on required systems. P0-00 now covers future tool/build bootstrap. The [Phase 0 backlog](../plans/phase-0.md) orders these investigations and distinguishes minimum slice gates from full compatibility/scale evidence; the [development plan](../development-plan.md) covers later phases. Linux is mandatory and the supplied images are references, not completed design or performance evidence.
