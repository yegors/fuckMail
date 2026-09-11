# fuckMail

fuckMail is an opinionated desktop mail and calendar client for large, separate accounts. The goal is fast local search, complete offline messages and attachments, a calendar you can trust, and a daily workflow that ends with zero unread and zero starred mail. A star means an action has been left for later and is still outstanding.

It is being designed for Windows, macOS and Linux, including Ubuntu, Arch and Omarchy, with the same opinionated visual design on every OS. A full CLI ships alongside the GUI for humans and third-party agents. Humans write email for humans: optional local models can classify and summarize mail, while composition stays human.

## Current readiness

**0% of the planned MVP is verified: 0 of 10 required feature areas.** This is a documentation-stage project. No application features are implemented yet, and there is no runnable build or benchmark result. Nonfinal mockups now guide a recommended Rust engine/CLI with a C++20 + Qt Quick/QML UI and SQLite/FTS5. Final versions and feasibility remain unverified; the closer mail/calendar mockups remain nonfinal. The [phased development plan](docs/development-plan.md) and [detailed Phase 0 backlog](docs/plans/phase-0.md) are prepared for future execution. This is readiness to begin feasibility, not a claim that tools, contracts or application features are verified.

| Planned feature | Build status |
| --- | --- |
| [Core and offline storage](docs/contracts/core-storage.md) | Not started |
| [Gmail / Google Workspace](docs/contracts/gmail.md) | Not started |
| [IMAP/SMTP](docs/contracts/imap-smtp.md) | Not started |
| [Mail UI and human composition](docs/contracts/mail-ui.md) | Not started |
| [Local search](docs/contracts/search.md) | Not started |
| [Unread/star triage and categories](docs/contracts/triage.md) | Not started |
| [Calendar and read-only feeds](docs/contracts/calendar.md) | Not started |
| [Optional local classification/summaries](docs/contracts/local-ai.md) | Not started |
| [Windows/macOS/Linux installation and lifecycle](docs/contracts/platform.md) | Not started |
| [Full CLI and external agent interface](docs/contracts/cli.md) | Not started |

Readiness counts fully verified areas, including their required OS, installation, GUI/CLI and recovery gates. It measures delivered scope, not time remaining; writing the plan does not increase it. See [current status](docs/status.md) for the definition, next work and exclusions.

## What the product is for

- **Process mail:** separate accounts, conversation-only browsing, temporary starred action holds and honest unread/starred totals.
- **Find and keep everything:** fast sender/subject/body search and complete offline messages and attachments.
- **Manage meetings:** Google Calendar, day/week/month/agenda, recurrence, invitations/RSVP, shared calendars, reminders and read-only iCalendar subscriptions.
- **Organize in the mailbox:** configurable Gmail labels or IMAP folders, applied manually or by optional local Ollama/vLLM classification. Local summaries help with reading.

Consumer Gmail and Google Workspace mail/calendar are required through the Google APIs; IMAP/SMTP is also required. Closing quits the GUI and its work; separately invoked CLI commands have explicit lifetimes. Tasks, contacts, widget dashboards, unrelated integrations and AI email writing are outside the product.

## How the pieces fit together

The planned desktop UI uses a local core for records, durable operations and search. Provider adapters connect directly to mail/calendar services; optional classification and summaries use a configured same-machine model runner. There is no app-operated remote service. The [recommended stack](docs/research/desktop-stack.md) uses a Rust engine and CLI, a C++20/Qt Quick UI, SQLite/FTS5 and an isolated HTML renderer. GUI and CLI access the engine through private local IPC. It is a proposed baseline awaiting prototype and packaging evidence.

The [build contracts](docs/contracts/README.md) define responsibilities and interfaces so the areas can be implemented in parallel once the shared contract baseline is agreed. End-user installation should deliver GUI and CLI without developer tools or a required model. There are no binaries or build/install commands to publish yet.

## Development entry point

When implementation is requested, start with [Phase 0: P0-00 and the small-fixture portion of P0-01](docs/plans/phase-0.md): inspect tools/test-host access, establish reproducible builds and shared synthetic fixtures, then prove engine/CLI/UI coordination and durable storage. Final mockups arrive closer to production UI work. Provider, model and platform evidence have their own gates so missing external access does not stall unrelated work.

The plan becomes less detailed in later phases and is revised after each evidence gate. See [AGENTS.md](AGENTS.md) for execution/documentation rules. No code, dependency installation, account connection or model download has been performed in this planning pass.

## Documentation

- [Documentation index](docs/README.md): where current and future documentation belongs.
- [North star](docs/north-star.md) and [product scope](docs/product-scope.md): the intended experience and its limits.
- [Visual design brief](docs/ui-ux.md): UI principles, top mailbox selection, resizable mail split and persistent bottom calendar strip.
- [Development plan](docs/development-plan.md) and [Phase 0 backlog](docs/plans/phase-0.md): phased outcomes, immediate work packets, dependencies and decision gates.
- [Current status](docs/status.md): built, next, excluded and deferred work, with readiness evidence.
- [Research by scope](docs/research/README.md): findings, sources, tradeoffs and experiments, including the [stack recommendation](docs/research/desktop-stack.md) and [installation plan](docs/research/linux-and-distribution.md).
- [Build contracts](docs/contracts/README.md): living specifications, owners, dependencies and acceptance gates.
- [Decision log](docs/decisions.md): timestamped decisions, pivots and Q&A, appended chronologically.
