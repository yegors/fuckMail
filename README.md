# fuckMail

A personal mail and calendar client for large, separate Google accounts. Fast local search, complete offline messages and attachments, reliable calendars, and a deliberate shared design on Windows, macOS and Linux (Ubuntu, Arch and Omarchy).

Human correspondence comes first. Unread means unreviewed; stars mean action left for later. The user alone chooses Done/Delayed. Optional local models distinguish human mail, bots/lists and user-defined noise, apply configured category/scope tags and summarize. Humans write replies.

## Current state

**Phase 1 is complete on available hosts; Phase 2 implementation is active on `codex/phase-2` under D021–D023.** The integrated schema-10 Windows development run passes 10/10 CTests, including 353 executed Rust workspace tests and the native GUI/CLI/restart walkthrough. Bounded Calendar preparation, durable mail state delivery and initial conditional Calendar writes have synthetic-provider coverage; a separate 306-test run and strict lint also pass. Live fixture 0, synthetic fixture 3 and protocol 1 remain unchanged. Live UI integration, acquisition/navigation measurements, packages and complete drafts/send/Calendar workflows remain pending. No real account is connected; Google-first readiness remains 0/9. The [execution record](docs/research/phase-2-execution.md#d023-integrated-verification) preserves exact results and remaining gates.

The [Phase 2 plan](docs/plans/phase-2.md) still requires both personal Gmail and Workspace mail/Calendar, real drafts/send, calendar writes/invitations, reminders, recovery and measured responsiveness. The founder reports existing Google project/admin access and chose OS permissions plus verified disk encryption for the pilot. Actual disk/profile/backup admission remains outstanding; app-level mail-store encryption is deferred.

P1-10 [final CI](docs/research/phase-1-execution.md#p1-10-final-native-ci-and-phase-1-closure) passes **9/9 tests on Mac, Windows and Ubuntu**, with all relocated development packages and six artifacts retained. The generated PR merge, pushed `cb6489b` and merged `main` at `885de67` have the same Git tree. Windows mailbox switching remains **82–92 ms p95**, reader opening **78–92 ms p95**, with all 900 source checks and three-theme resize/visual acceptance passing at runtime `25b8b1e`. Physical input/vault/live-provider/trusted-distribution limits remain explicit.

The [complete Windows release run](docs/research/assets/phase-1/p1-10-windows/reports/p1-10-full-release-03.log) passes **9/9 tests in 158.15 seconds**, including native GUI/CLI/restart workflows and regressions for delayed clicks, compact keyboard navigation and restored drafts. The clean package also passes SDK-free relocation and the full native walkthrough. Physical input, broader scaling and other-host GPU behavior retain their separate unverified scope.

The [packaged concurrent-ingestion measurement](docs/research/assets/phase-1/p1-10-windows/reports/p1-10-search-ingestion-package-01.json) passes all four unchanged 200 ms targets at p95 **64.54 / 74.75 / 108.18 / 164.58 ms**, with 100 target/reset pairs per class and **5,000 independently verified arrivals**. Earlier failures remain recorded. P1-09's [General captures](docs/research/assets/phase-1/p1-09-windows/visual-02/manifest.json) and [remaining Settings pages](docs/research/assets/phase-1/p1-09-windows/visual-03/manifest.json) retain all six sections; [P1-10 visual review](docs/research/assets/phase-1/p1-10-windows/reports/p1-10-render-review-package-01.json) covers the corrected controls and rendering.

The current Windows [development package](docs/research/assets/phase-1/p1-10-windows/reports/p1-10-package-01.json) is `dist/fuckmail-windows-amd64.zip`, built from clean `25b8b1e`. Earlier packages remain preserved locally. Schema4/schema5→7 upgrade/refusal evidence belongs to `8743162`; P1-10 changes neither schema 7 nor fresh fixture 3. P1-09's [three-host CI at `791254d`](docs/research/phase-1-execution.md#p1-09-final-source-native-ci--791254d) remains green for that earlier source. Runtime `204fa62` remains the completed Phase 0 baseline. See [development commands](docs/build-development.md), [status](docs/status.md), [Phase 1 evidence](docs/research/phase-1-execution.md) and the [plan](docs/plans/phase-1.md).

| Google-first feature area | Build status |
| --- | --- |
| [Core and complete offline storage](docs/contracts/core-storage.md) | Schema-9 migration, bounded publication and Windows native checks pass; packages/complete live coverage pending; whole area Draft |
| [Gmail / Google Workspace](docs/contracts/gmail.md) | OAuth, HTTP, Core and engine worker fixtures pass; multi-owner/live-write acceptance pending; whole area Draft |
| [Mail UI and human composition](docs/contracts/mail-ui.md) | Phase 1 native/package and Windows navigation/rendering pass; Phase 2 live workflows planned; whole area Draft |
| [Local search](docs/contracts/search.md) | Native filters/idle convergence and all four development/packaged concurrent budgets pass; whole area Draft |
| [User-owned triage and categories](docs/contracts/triage.md) | Phase 1 explicit local review/Done/Delay verified; whole area Draft |
| [Google Calendar](docs/contracts/calendar.md) | HTTP, indexed storage and engine worker fixtures pass; shared page bounds, live writes/reminders pending; whole area Draft |
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

Integrate live GUI capabilities, sync ownership, delivery/Calendar status and existing Gmail label setup; then complete remote drafts/send and measure responsiveness during acquisition. Current boundaries are Core 16, Gmail 6, Calendar 14, Platform 11, Mail UI 14, CLI 9 and Quality 11. Settle retention/backup choices and verify actual Cloud quotas and disk protection before real import. The first account test build precedes complete history; sending/invitations, measured performance under acquisition, packages and sustained daily use retain their own [Phase 2 gates](docs/plans/phase-2.md). Phase 1 evidence remains attributed to its original source.

The [first deliverable](docs/plans/phase-0.md#proposed-first-deliverable-a-runnable-desktop-slice-on-mac-and-windows) is a runnable synthetic-mail desktop app and small CLI, with repeatable setup/build/test commands and native CI builds for development on the work Mac and home Windows PC. Tools live in the ignored `.tools` directory. The home Windows PC now passes bootstrap/doctor, dev and release tests (8/8 each), relocated package verification and native graphics checks; its schema-4 baseline package is preserved. See [Windows handoff evidence](docs/research/phase-0-execution.md#home-windows-handoff-before-phase-1). Run `scripts/dev.ps1 run` from PowerShell on this PC, or `scripts/dev.sh run` on Mac. Manual input/accessibility and OS keyring qualification remain separate.

The [development plan](docs/development-plan.md) proceeds from the paired local slice to real Google workflows and founder dogfooding, then legacy protocols. All-platform installation and quality remain product requirements; unavailable hosts are unverified, not assumed passes.

## Documentation

- [Index](docs/README.md), [north star](docs/north-star.md), [scope](docs/product-scope.md) and [UI/UX](docs/ui-ux.md).
- [Status](docs/status.md), [development plan](docs/development-plan.md), [Phase 1 packets](docs/plans/phase-1.md) and completed [Phase 0 packets](docs/plans/phase-0.md).
- [Contracts](docs/contracts/README.md), [scoped research](docs/research/README.md) and [decision history](docs/decisions.md).
- [Agent instructions](AGENTS.md).
