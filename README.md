# fuckMail

A personal mail and calendar client for large, separate Google accounts. Fast local search, complete offline messages and attachments, reliable calendars, and a deliberate shared design on Windows, macOS and Linux (Ubuntu, Arch and Omarchy).

Human correspondence comes first. Unread means unreviewed; stars mean action left for later. The user alone chooses Done/Delayed. Optional local models distinguish human mail, bots/lists and user-defined noise, apply configured category/scope tags and summarize. Humans write replies.

## Current state

**P1-09 Windows release tests, relocated package, preserved-profile upgrades and all four packaged search targets pass at source `8743162`.** The [9/9 release run](docs/research/assets/phase-1/p1-09-windows/p1-09-full-release-01.log) includes 163 Rust tests and the full native GUI/restart walkthrough: all six Settings sections, three themes/text sizes, startup choices, timeline placement, counted groups, sender cards, composition/conflict recovery and calendar/RSVP. Final-source Mac/Windows/Ubuntu CI remains required, so Phase 1 is not complete; Google-first readiness remains 0/9.

The [packaged concurrent-ingestion measurement](docs/research/assets/phase-1/p1-09-windows/p1-09-search-ingestion-package-01.json) passes at p95 **70.18 / 76.96 / 110.34 / 189.57 ms**, with 100 target/reset pairs per class and independent checks for **5,100 actual source arrivals**. Earlier development results and failures remain recorded. Reviewed [General captures](docs/research/assets/phase-1/p1-09-windows/visual-02/manifest.json) and [remaining Settings pages](docs/research/assets/phase-1/p1-09-windows/visual-03/manifest.json) cover all six sections; nine earlier captures retain Mail/Calendar layout observations.

The clean Windows [development package](docs/research/assets/phase-1/p1-09-windows/p1-09-package-01.json) is `dist/fuckmail-windows-amd64.zip`; SDK-free relocation and preserved schema4/schema5→7 upgrade/refusal pass. Earlier packages remain preserved locally. Runtime `204fa62` remains the completed Phase 0 baseline. See [development commands](docs/build-development.md), [status](docs/status.md), [Phase 1 evidence](docs/research/phase-1-execution.md) and the [plan](docs/plans/phase-1.md).

| Google-first feature area | Build status |
| --- | --- |
| [Core and complete offline storage](docs/contracts/core-storage.md) | Schema-7 native draft/restart, Core migration and preserved package upgrades pass; whole area Draft |
| [Gmail / Google Workspace](docs/contracts/gmail.md) | Phase 0 synthetic evidence; whole area Draft |
| [Mail UI and human composition](docs/contracts/mail-ui.md) | P1-09 release/native and packaged controls, composition/conflict and restart pass; whole area Draft |
| [Local search](docs/contracts/search.md) | Native filters/idle convergence and all four development/packaged concurrent budgets pass; whole area Draft |
| [User-owned triage and categories](docs/contracts/triage.md) | Phase 1 explicit local review/Done/Delay verified; whole area Draft |
| [Google Calendar](docs/contracts/calendar.md) | P1-09 views/writes, header timeline, duration preference and event/RSVP restart pass; live Google pending; whole area Draft |
| [Optional local classification/summaries](docs/contracts/local-ai.md) | Phase 0 synthetic evidence; whole area Draft |
| [Platform installation and lifecycle](docs/contracts/platform.md) | Windows release/package/upgrades pass; final-source cross-platform CI pending; whole area Draft |
| [CLI alongside the GUI](docs/contracts/cli.md) | Paired composition/MIME/restart readback verified locally; whole area Draft |

IMAP, SMTP and external iCalendar subscriptions are deferred until the founder has dogfooded working Google GUI and CLI workflows. Their retained contracts are future scope, not missing first-milestone requirements. CalDAV and a broad public launch remain separately deferred. The denominator changed from ten to nine because IMAP/SMTP left the current target, not because anything was built.

## Delivery direction

- Build a small real CLI and GUI together against one engine/store; grow command coverage with each Google workflow. Do not finish a CLI monolith before the GUI.
- Use Gmail and Google Calendar APIs for all current personal/Workspace accounts, including sending, with durable OAuth and provider reconciliation.
- Begin with readable text, human replies/signatures and attachments. Preserve source MIME and safely extract text from HTML-only messages. Add best-effort HTML display later; browser-level visual fidelity is not required.
- Follow the September 12 [Flight Progress v2 reference](docs/research/flight-progress-v2.md) closely: top account tabs and contextual search, a resizable conversation list/reader, human composer and account-scoped Drafts, combined calendar, Mail-only timeline and local settings. Read-on-open is confirmed; Done clears unread and stars; Delay is untimed.
- Close ends GUI-owned work/reminders. Explicit independent CLI sessions have their own lifetimes. No default always-on service.

The [proposed stack](docs/research/desktop-stack.md) is a Rust engine/CLI, C++20 + Qt Quick/QML UI, private local IPC and SQLite/FTS5 with retained blobs. QML expressions are accepted; Node.js and a separate JS/TS compilation/bundling pipeline are excluded. An HTML engine is a later component choice. Experiment versions are pinned in `rust-toolchain.toml`, `Cargo.lock` and `scripts/toolchain.json`; this does not establish production readiness.

The app operates locally and connects directly to Google; optional models run on the same machine. Tasks, a contacts app, dashboards, built-in AI writing/chat and semantic search remain out of scope.

## Next execution

Phase 0 joined the engine, small CLI and functional GUI and verified shared state, IPC access, account boundaries, draft recovery, search and native packages. The active [P1-09 packet](docs/plans/phase-1.md#p1-09--second-september-12-prototype-update), confirmed by D018, integrates the second September 12 prototype on schema 7/fresh fixture 3/additive protocol 1. Finish final-source Mac/Windows/Ubuntu CI and retain its packages/logs; all local Windows acceptance checks now pass. New-file import, aliases, live delivery, account/model setup and timed Delay remain deferred. See the [Phase 0 evidence](docs/research/phase-0-execution.md) for the completed baseline.

The [first deliverable](docs/plans/phase-0.md#proposed-first-deliverable-a-runnable-desktop-slice-on-mac-and-windows) is a runnable synthetic-mail desktop app and small CLI, with repeatable setup/build/test commands and native CI builds for development on the work Mac and home Windows PC. Tools live in the ignored `.tools` directory. The home Windows PC now passes bootstrap/doctor, dev and release tests (8/8 each), relocated package verification and native graphics checks; its schema-4 baseline package is preserved. See [Windows handoff evidence](docs/research/phase-0-execution.md#home-windows-handoff-before-phase-1). Run `scripts/dev.ps1 run` from PowerShell on this PC, or `scripts/dev.sh run` on Mac. Manual input/accessibility and OS keyring qualification remain separate.

The [development plan](docs/development-plan.md) proceeds from the paired local slice to real Google workflows and founder dogfooding, then legacy protocols. All-platform installation and quality remain product requirements; unavailable hosts are unverified, not assumed passes.

## Documentation

- [Index](docs/README.md), [north star](docs/north-star.md), [scope](docs/product-scope.md) and [UI/UX](docs/ui-ux.md).
- [Status](docs/status.md), [development plan](docs/development-plan.md), [Phase 1 packets](docs/plans/phase-1.md) and completed [Phase 0 packets](docs/plans/phase-0.md).
- [Contracts](docs/contracts/README.md), [scoped research](docs/research/README.md) and [decision history](docs/decisions.md).
- [Agent instructions](AGENTS.md).
