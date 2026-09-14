# fuckMail

A personal mail and calendar client for separate Google accounts, with a native GUI and CLI sharing one local store. Human correspondence comes first: unread means unreviewed, stars hold work for later, and only the user chooses Done/Delayed. Humans write replies.

## Current Windows feedback build

**D038/D039 at `61a212a` has a verified Windows development package.** Normal startup opens an empty live profile. Settings supports Google account setup, local removal with a retained archive and explicit restoration. Mail includes cached reading/search, composition, attachments, Google Drafts and explicit Send with recovery. Calendar includes permitted combined views, invitations, supported recurrence scopes/self-RSVP, running-app reminders and cached search in the displayed date range. The September 14 reference guides the native design.

The current boundary is **Core 27 / Gmail 12 / Calendar 20 / Platform 14 / Mail UI 25 / CLI 20 / Quality 23 / Search 5 / schema 20**, with fixtures 0/3 and protocol 1 unchanged. Package02 passes relocated GUI/CLI and empty-live startup checks on its exact source. Final native CTest02 passes **22/22**; the [execution record](docs/research/phase-2-execution.md) distinguishes these results from earlier scoped tests and failures.

**Final performance acceptance is pending.** Generated-sync navigation01 passes the four 100 ms p95 targets at **66.1764 / 69.0417 / 67.8952 / 29.6932 ms** for mailbox click/Tab/Shift+Tab/reader. The separate large-navigation01 run fails its `visible_snapshot` correctness check with duplicate stale delegates and is under investigation. This pass does not erase that failure or earlier measurements.

**Google-first readiness remains 0/9 whole areas verified.** Generated-provider workflows and a Windows package do not establish actual Google account behavior, sustained authorization, complete offline coverage, protected backup/restore, current other-host CI or trusted distribution.

## Try the feedback package

Extract `dist/fuckmail-windows-amd64.zip` and launch `windows-amd64/bin/fuckmail-gui.exe`. In **Settings → Accounts → Connect Google account**, select the existing OAuth desktop-client JSON. Keep the file at that path. Sign-in alone does not import: confirm the four actual Cloud quota limits, then choose **Start sync** for each account. The [launch guide](docs/build-development.md#phase-2-feedback-launch) explains profile protection, CLI access and a bounded first session.

The ZIP is an unsigned Windows feedback artifact from `61a212a`; Package02's SHA-256 is `5f0c98edeeb37a3a7d29312455514bc1f0ae7bae2bd4aaf26123f060b336fbc0`. It is not a public installer. Keep real data outside Git and cloud-sync folders. Settle backup/retention admission before importing real mail; the founder's reported disk encryption remains distinct from an administrator-verified status check.

Calendar search covers selected calendars in an explicit **1–42-day range**, with at most 100 matches and visible coverage limits. Recurrence authoring supports a common rule subset; following changes require confirmation that Google resets later exceptions and may need explicit partial/unknown recovery. Reminders and GUI-owned sync stop when the GUI closes; an explicit CLI session owns its own work. Incomplete mail inventory is not a complete offline copy or a backup.

## Delivery direction

- Separate mail accounts and conversation-only browsing; combined Calendar shows only permitted details.
- Read-on-open follows a deliberate user action. Done clears unread and stars; Delay is untimed.
- Preserve MIME, original HTML and attachments while prioritizing readable text and human composition. Best-effort received HTML display is later work.
- Use Google APIs for current Gmail/Workspace mail and Calendar, including writes. GUI and CLI use the same commands and store.
- Follow the [September 14 reference](docs/research/assets/2026-09-14/flight-progress-v2/manifest.json), including the three themes and Mail-only timeline.
- Optional local models may classify or summarize; they never write replies or choose review/action state. Tasks, contacts, dashboards, assistant chat and semantic search are outside scope.

The implementation uses Rust, C++20/Qt Quick/QML, private local IPC and SQLite/FTS5 with retained blobs. Tools are pinned in `rust-toolchain.toml`, `Cargo.lock` and `scripts/toolchain.json`; pinned versions do not establish production readiness. No Node.js or separate JavaScript build pipeline is required.

## Next work

Resolve the large-navigation correctness failure and finish packaged performance checks. Then use the [Phase 2 admission and comparison gates](docs/plans/phase-2.md#builds-the-founder-can-test) for controlled personal Gmail and Workspace testing, actual provider reconciliation, sustained refresh and complete agreed source coverage. Verify backup/restore and protection on the real profile volume. Current Mac/Linux CI, native vault/input behavior and distribution qualification remain separate.

IMAP, SMTP and external iCalendar subscriptions stay deferred until the founder has dogfooded the Google GUI and CLI. CalDAV and a broad public launch have separate scope.

## Historical evidence

Phases 0 and 1 are complete on available hosts. Their [foundation](docs/research/phase-0-execution.md) and [Phase 1 execution](docs/research/phase-1-execution.md) records retain source-specific CI, packages, performance and limitations. Those passes are not current Phase 2 other-host acceptance.

Earlier Phase 2 checkpoints, immutable source/artifact manifests, failed attempts and native diagnostics remain in the [Phase 2 execution record](docs/research/phase-2-execution.md). In particular, [D037 occurrence](docs/research/assets/phase-2/p2-06c-interim/logs-manifest.json), [D036 account lifecycle](docs/research/assets/phase-2/p2-01b-interim/logs-manifest.json), [D035 invitations](docs/research/assets/phase-2/p2-06b-interim/logs-manifest.json), [B3B Drafts](docs/research/assets/phase-2/p2-05b3b-interim/logs-manifest.json) and [B2's failed performance checkpoint](docs/research/assets/phase-2/p2-05b2-interim/logs-manifest.json) are preserved. D038 resumed measurement; it did not rewrite D031 or its failed result.

## Documentation

- [Status](docs/status.md), [development plan](docs/development-plan.md), [Phase 2 packets](docs/plans/phase-2.md) and [build/launch guide](docs/build-development.md).
- [Index](docs/README.md), [north star](docs/north-star.md), [scope](docs/product-scope.md), [UI/UX](docs/ui-ux.md) and [decisions](docs/decisions.md).
- [Contracts](docs/contracts/README.md) own shared behavior; [research](docs/research/README.md) owns evidence and comparisons.
