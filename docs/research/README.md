# Research index

Evidence, alternatives and experiments organized by subject. This index is navigation, not a general research notebook. Initial source checks were on 2026-09-10; exec-buddy/category/local-runner work and QML references were added or checked on 2026-09-11. The reorganization does not revalidate every external source. No application prototype or benchmark has run.

| Scope | Existing material | Build contract |
| --- | --- | --- |
| [Desktop stack and QML](desktop-stack.md) | Rust engine/CLI + C++20/Qt Quick UI recommendation, external-analysis assessment and feasibility gates | [Platform](../contracts/platform.md) |
| [Slint UI investigation](slint-ui.md) | Published examples, toolkit capabilities, reader/composer and platform gaps | [Mail UI](../contracts/mail-ui.md) |
| [HTML mail](html-mail.md) | Received HTML fidelity, human composition, isolation and renderer costs | [Mail UI](../contracts/mail-ui.md) |
| [Gmail](gmail.md) | History sync, quotas, OAuth, polling and labels | [Gmail](../contracts/gmail.md) |
| [IMAP/SMTP](imap-smtp.md) | Identities, capabilities, folder moves and uncertain sending | [IMAP/SMTP](../contracts/imap-smtp.md) |
| [Calendar](calendar.md) | Google synchronization, recurrence, feeds and deferred CalDAV | [Calendar](../contracts/calendar.md) |
| [Search/storage](search-storage.md) | SQLite/FTS5, Tantivy, Notmuch, WAL and scale experiments | [Core](../contracts/core-storage.md), [Search](../contracts/search.md) |
| [Local AI](local-ai.md) | Ollama/vLLM setup, structured results, supported platforms and evaluation | [Local assistance](../contracts/local-ai.md) |
| [exec-buddy product lessons](exec-buddy.md) | Source evidence for selected mail/calendar interactions and rejected carryovers | [Triage](../contracts/triage.md), [Calendar](../contracts/calendar.md) |
| [CLI and local engine](cli-and-local-engine.md) | Headless access, shared process/state and lifecycle tradeoffs | [CLI](../contracts/cli.md), [Core](../contracts/core-storage.md) |
| [Linux and distribution](linux-and-distribution.md) | Required distro matrix, runtime packaging and smooth GUI/CLI installation | [Platform](../contracts/platform.md) |
| [Supplied mockups](design-references.md) | Closer mail/calendar pair plus three earlier references; nonfinal structure and open handoff decisions | [Mail UI](../contracts/mail-ui.md) |

Near-term research was expanded on 2026-09-11 for OAuth longevity/Workspace policy, Rust mail/calendar library candidates, Qt reader/composer limits, private IPC, SQLite/blob durability and clean installation. Named libraries and test recipes remain proposals; no implementation or live integration result is implied.

## Adding findings

Add material to the matching scope and preserve source/checked dates. Distinguish observed behavior from a project-fit inference, untested proposal or measured result. Record versions and reproducible commands with future experiment reports. Add a new scoped file and index row only when new material warrants one; do not create an all-purpose research.md or a collection of empty stubs.

When research changes a product/build choice, append the outcome to [decisions](../decisions.md), update the owning [contract](../contracts/README.md) and adjust [status](../status.md) as needed. An earlier recommendation does not override a newer founder decision.

## Experiment map

The original phase-0 questions remain attached to their subjects: P0-01 inventory/scale and P0-03 indexing in Search/storage; P0-02 auth and Gmail filing in Gmail; P0-04 design/stack in Desktop and Slint; P0-05 libraries split across HTML, IMAP/SMTP and Calendar; P0-06 interoperability in Calendar and IMAP/SMTP; P0-07 categories in provider/triage work; P0-08 runners in Local AI; P0-09 product interactions in exec-buddy. [Quality](../contracts/quality.md) holds the proposed workload, budgets and trial criteria.

Added 2026-09-11: P0-10 tests the full CLI/shared local engine and P0-11 tests packaged installation on required systems. P0-00 now covers future tool/build bootstrap. The [Phase 0 backlog](../plans/phase-0.md) orders these investigations and distinguishes minimum slice gates from full compatibility/scale evidence; the [development plan](../development-plan.md) covers later phases. Linux is mandatory and the supplied images are references, not completed design or performance evidence.
