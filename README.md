# fuckMail

fuckMail is an opinionated desktop mail and calendar client for large, separate accounts. The goal is fast local search, complete offline messages and attachments, a calendar you can trust, and a daily workflow that ends with zero unread and zero starred mail. A star means an action has been left for later and is still outstanding.

It is being designed for Windows, macOS and Linux, including Ubuntu, Arch and Omarchy, with the same opinionated visual design on every OS. A full CLI ships alongside the GUI for humans and third-party agents. Humans write email for humans: optional local models can classify and summarize mail, while composition stays human.

## Current readiness

**0% of the planned MVP is verified: 0 of 10 required feature areas.** This is a documentation-stage project. No application features are implemented yet, and there is no runnable build or benchmark result. Nonfinal mockups now guide a recommended Rust engine/CLI with a C++20 + Qt Quick/QML UI and SQLite/FTS5. Final versions and feasibility remain unverified; visual design continues.

| Planned feature | Build status |
| --- | --- |
| [Core and offline storage](docs/contracts/core-storage.md) | Not started |
| [Gmail](docs/contracts/gmail.md) | Not started |
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

Gmail/Google Calendar are preferred; IMAP/SMTP is also required. Closing quits the GUI and its work; separately invoked CLI commands have explicit lifetimes. Tasks, contacts, widget dashboards, unrelated integrations and AI email writing are outside the product.

## How the pieces fit together

The planned desktop UI uses a local core for records, durable operations and search. Provider adapters connect directly to mail/calendar services; optional classification and summaries use a configured same-machine model runner. There is no app-operated remote service. The [recommended stack](docs/research/desktop-stack.md) uses a Rust engine and CLI, a C++20/Qt Quick UI, SQLite/FTS5 and an isolated HTML renderer. GUI and CLI access the engine through private local IPC. It is a proposed baseline awaiting prototype and packaging evidence.

The [build contracts](docs/contracts/README.md) define responsibilities and interfaces so the areas can be implemented in parallel once the shared contract baseline is agreed. End-user installation should deliver GUI and CLI without developer tools or a required model. There are no binaries or build/install commands to publish yet.

## Documentation

- [Documentation index](docs/README.md): where current and future documentation belongs.
- [North star](docs/north-star.md) and [product scope](docs/product-scope.md): the intended experience and its limits.
- [Visual design brief](docs/ui-ux.md): UI principles, top mailbox selection, resizable mail split and persistent bottom calendar strip.
- [Current status](docs/status.md): built, next, excluded and deferred work, with readiness evidence.
- [Research by scope](docs/research/README.md): findings, sources, tradeoffs and experiments, including the [stack recommendation](docs/research/desktop-stack.md) and [installation plan](docs/research/linux-and-distribution.md).
- [Build contracts](docs/contracts/README.md): living specifications, owners, dependencies and acceptance gates.
- [Decision log](docs/decisions.md): timestamped decisions, pivots and Q&A, appended chronologically.
