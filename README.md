# fuckMail

A personal mail and calendar client for large, separate Google accounts. Fast local search, complete offline messages and attachments, reliable calendars, and a deliberate shared design on Windows, macOS and Linux (Ubuntu, Arch and Omarchy).

Human correspondence comes first. Unread means unreviewed; stars mean action left for later. The user alone chooses Done/Delayed. Optional local models distinguish human mail, bots/lists and user-defined noise, apply configured category/scope tags and summarize. Humans write replies.

## Current state

**Documentation only. Google-first MVP readiness: 0/9 required areas verified.** No application, build, tests, installer or measured result exists yet. [Status](docs/status.md) records the milestone and evidence rules; [decisions](docs/decisions.md) preserves the changes in scope.

| Google-first feature area | Build status |
| --- | --- |
| [Core and complete offline storage](docs/contracts/core-storage.md) | Not started |
| [Gmail / Google Workspace](docs/contracts/gmail.md) | Not started |
| [Mail UI and human composition](docs/contracts/mail-ui.md) | Not started |
| [Local search](docs/contracts/search.md) | Not started |
| [User-owned triage and categories](docs/contracts/triage.md) | Not started |
| [Google Calendar](docs/contracts/calendar.md) | Not started |
| [Optional local classification/summaries](docs/contracts/local-ai.md) | Not started |
| [Platform installation and lifecycle](docs/contracts/platform.md) | Not started |
| [CLI alongside the GUI](docs/contracts/cli.md) | Not started |

IMAP, SMTP and external iCalendar subscriptions are deferred until the founder has dogfooded working Google GUI and CLI workflows. Their retained contracts are future scope, not missing first-milestone requirements. CalDAV and a broad public launch remain separately deferred. The denominator changed from ten to nine because IMAP/SMTP left the current target, not because anything was built.

## Delivery direction

- Build a small real CLI and GUI together against one engine/store; grow command coverage with each Google workflow. Do not finish a CLI monolith before the GUI.
- Use Gmail and Google Calendar APIs for all current personal/Workspace accounts, including sending, with durable OAuth and provider reconciliation.
- Begin with readable text, human replies/signatures and attachments. Preserve source MIME and safely extract text from HTML-only messages. Add best-effort HTML display later; browser-level visual fidelity is not required.
- Follow the [initial-phase mail/calendar/settings references](docs/research/design-references.md): accounts at the top, a resizable conversation list/reader, inline reply and persistent bottom calendar strip. Settings is shell/navigation plumbing only; detailed controls await the expanded design.
- Close ends GUI-owned work/reminders. Explicit independent CLI sessions have their own lifetimes. No default always-on service.

The [proposed stack](docs/research/desktop-stack.md) is a Rust engine/CLI, C++20 + Qt Quick/QML UI, private local IPC and SQLite/FTS5 with retained blobs. QML expressions are accepted; Node.js and a separate JS/TS compilation/bundling pipeline are excluded. An HTML engine is a later component choice. No stack versions are locked.

The app operates locally and connects directly to Google; optional models run on the same machine. Tasks, a contacts app, dashboards, built-in AI writing/chat and semantic search remain out of scope.

## Next execution

When asked to implement, start [P0-00 and the small-fixture part of P0-01](docs/plans/phase-0.md), then connect the engine, small CLI and functional GUI early. Prove shared state, IPC access, account boundaries, draft recovery and useful search. Final designs and later protocols do not block these experiments.

The [proposed first deliverable](docs/plans/phase-0.md#proposed-first-deliverable-a-runnable-desktop-slice-on-mac-and-windows) is a runnable synthetic-mail desktop app and small CLI, with repeatable setup/build/test commands and native CI builds for development on the work Mac and home Windows PC. No tools have been installed or implementation started.

The [development plan](docs/development-plan.md) proceeds from the paired local slice to real Google workflows and founder dogfooding, then legacy protocols. All-platform installation and quality remain product requirements; unavailable hosts are unverified, not assumed passes.

## Documentation

- [Index](docs/README.md), [north star](docs/north-star.md), [scope](docs/product-scope.md) and [UI/UX](docs/ui-ux.md).
- [Status](docs/status.md), [development plan](docs/development-plan.md) and [Phase 0 packets](docs/plans/phase-0.md).
- [Contracts](docs/contracts/README.md), [scoped research](docs/research/README.md) and [decision history](docs/decisions.md).
- [Agent instructions](AGENTS.md).
