# fuckMail

A personal mail and calendar client for large, separate Google accounts. Fast local search, complete offline messages and attachments, reliable calendars, and a deliberate shared design on Windows, macOS and Linux (Ubuntu, Arch and Omarchy).

Human correspondence comes first. Unread means unreviewed; stars mean action left for later. The user alone chooses Done/Delayed. Optional local models distinguish human mail, bots/lists and user-defined noise, apply configured category/scope tags and summarize. Humans write replies.

## Current state

**P1-10 Windows navigation/rendering acceptance passes at clean source `25b8b1e`; new-source Mac/Windows/Ubuntu CI is pending publication.** Packaged mailbox switching measures **82–92 ms p95**, down from **251–266 ms** in the baseline; reader clicks measure **78–92 ms p95**. All 900 fresh/large-profile source-identity checks pass. Reader actions align right, enabled/disabled button text is readable, and native resize traces and saved border samples pass in Graphite, Paper and Harbour at 125% scaling. See [final package evidence](docs/research/phase-1-execution.md#p1-10-final-windows-package--25b8b1e). Phase 1 remains incomplete until the new source's CI is recorded; no next phase has begun and Google-first readiness remains 0/9.

The [complete Windows release run](docs/research/assets/phase-1/p1-10-windows/reports/p1-10-full-release-03.log) passes **9/9 tests in 158.15 seconds**, including native GUI/CLI/restart workflows and regressions for delayed clicks, compact keyboard navigation and restored drafts. The clean package also passes SDK-free relocation and the full native walkthrough. Physical input, broader scaling and other-host GPU behavior retain their separate unverified scope.

The [packaged concurrent-ingestion measurement](docs/research/assets/phase-1/p1-10-windows/reports/p1-10-search-ingestion-package-01.json) passes all four unchanged 200 ms targets at p95 **64.54 / 74.75 / 108.18 / 164.58 ms**, with 100 target/reset pairs per class and **5,000 independently verified arrivals**. Earlier failures remain recorded. P1-09's [General captures](docs/research/assets/phase-1/p1-09-windows/visual-02/manifest.json) and [remaining Settings pages](docs/research/assets/phase-1/p1-09-windows/visual-03/manifest.json) retain all six sections; [P1-10 visual review](docs/research/assets/phase-1/p1-10-windows/reports/p1-10-render-review-package-01.json) covers the corrected controls and rendering.

The current Windows [development package](docs/research/assets/phase-1/p1-10-windows/reports/p1-10-package-01.json) is `dist/fuckmail-windows-amd64.zip`, built from clean `25b8b1e`. Earlier packages remain preserved locally. Schema4/schema5→7 upgrade/refusal evidence belongs to `8743162`; P1-10 changes neither schema 7 nor fresh fixture 3. P1-09's [three-host CI at `791254d`](docs/research/phase-1-execution.md#p1-09-final-source-native-ci--791254d) remains green for that earlier source. Runtime `204fa62` remains the completed Phase 0 baseline. See [development commands](docs/build-development.md), [status](docs/status.md), [Phase 1 evidence](docs/research/phase-1-execution.md) and the [plan](docs/plans/phase-1.md).

| Google-first feature area | Build status |
| --- | --- |
| [Core and complete offline storage](docs/contracts/core-storage.md) | Schema-7 native draft/restart, Core migration and preserved package upgrades pass; whole area Draft |
| [Gmail / Google Workspace](docs/contracts/gmail.md) | Phase 0 synthetic evidence; whole area Draft |
| [Mail UI and human composition](docs/contracts/mail-ui.md) | P1-10 Windows navigation/rendering and package workflows pass; new-source CI pending; whole area Draft |
| [Local search](docs/contracts/search.md) | Native filters/idle convergence and all four development/packaged concurrent budgets pass; whole area Draft |
| [User-owned triage and categories](docs/contracts/triage.md) | Phase 1 explicit local review/Done/Delay verified; whole area Draft |
| [Google Calendar](docs/contracts/calendar.md) | P1-09 views/writes, header timeline, duration preference and event/RSVP restart pass; live Google pending; whole area Draft |
| [Optional local classification/summaries](docs/contracts/local-ai.md) | Phase 0 synthetic evidence; whole area Draft |
| [Platform installation and lifecycle](docs/contracts/platform.md) | P1-09 Mac/Windows/Ubuntu native tests and relocated development packages pass; Windows upgrades pass; whole area Draft |
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

Record final-source Mac/Windows/Ubuntu CI for the locally committed [P1-10 spike](docs/plans/phase-1.md#p1-10--navigation-and-rendering-spike-before-the-next-phase) before closing Phase 1 or beginning another phase. Windows release/package, navigation, concurrent search and three-theme rendering acceptance pass at `25b8b1e`. New-file import, aliases, live delivery, account/model setup and timed Delay remain deferred. See the [Phase 0 evidence](docs/research/phase-0-execution.md) for the completed baseline.

The [first deliverable](docs/plans/phase-0.md#proposed-first-deliverable-a-runnable-desktop-slice-on-mac-and-windows) is a runnable synthetic-mail desktop app and small CLI, with repeatable setup/build/test commands and native CI builds for development on the work Mac and home Windows PC. Tools live in the ignored `.tools` directory. The home Windows PC now passes bootstrap/doctor, dev and release tests (8/8 each), relocated package verification and native graphics checks; its schema-4 baseline package is preserved. See [Windows handoff evidence](docs/research/phase-0-execution.md#home-windows-handoff-before-phase-1). Run `scripts/dev.ps1 run` from PowerShell on this PC, or `scripts/dev.sh run` on Mac. Manual input/accessibility and OS keyring qualification remain separate.

The [development plan](docs/development-plan.md) proceeds from the paired local slice to real Google workflows and founder dogfooding, then legacy protocols. All-platform installation and quality remain product requirements; unavailable hosts are unverified, not assumed passes.

## Documentation

- [Index](docs/README.md), [north star](docs/north-star.md), [scope](docs/product-scope.md) and [UI/UX](docs/ui-ux.md).
- [Status](docs/status.md), [development plan](docs/development-plan.md), [Phase 1 packets](docs/plans/phase-1.md) and completed [Phase 0 packets](docs/plans/phase-0.md).
- [Contracts](docs/contracts/README.md), [scoped research](docs/research/README.md) and [decision history](docs/decisions.md).
- [Agent instructions](AGENTS.md).
