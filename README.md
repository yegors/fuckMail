# fuckMail

**D046 repository cleanup:** raw execution evidence is preserved outside the tracked tree; reports and design references remain in Git. See [archive recovery and storage rules](docs/research/evidence-archive.md). Application behavior and Phase 2 acceptance are unchanged.

**D043–D045 — real-mailbox UI repairs:** the founder has signed into the configured Mac app. Real UI testing found and fixed Calendar creation after first sync, recurring editor invalidation, accessible event activation and search-popup focus/dismissal. Mail and Calendar walkthrough results are recorded below. Performance and the broad workspace sweep remain deferred under D041; this is not full Phase 2 acceptance.

A personal mail and calendar client for separate Google accounts, with a native GUI and CLI sharing one local store. Humans write messages and choose Done/Delayed.

## Current Mac feedback build

**D040 / P2-01C implements email-first Google sign-in.** Open **Settings → Accounts → Connect Google account**, enter a Gmail or Workspace email, and choose **Continue with Google**. Reconnect and Restore share the same browser flow. The app registration is configured once by the developer; individual account users do not select JSON.

The Mac Release build, **22/22 native checks**, final focused search check, relocated UI Package03, and generated-provider CLI/native account walkthroughs pass. The package is `dist/fuckmail-darwin-arm64.tar.gz`; launch `dist/darwin-arm64/fuckmail-gui.app`. D042 bundles the supplied Desktop registration, and relocated setup reports Google sign-in available. Follow the [one-time setup and launch guide](docs/build-development.md#email-first-google-setup-d040).

The current boundary is **Core 27 / Gmail 13 / Calendar 21 / Platform 15 / Mail UI 28 / CLI 21 / Quality 26 / Search 5 / schema 20**; fixtures 0/3 and protocol 1 are unchanged. Mail composition, Google Drafts/Send, supported Calendar recurrence/invitations/RSVP, reminders, partial grants and cached search retain their existing implementation.

The real test-account walkthrough verifies Calendar create/edit, invitations, all-day and recurring operations, plus mail sending, Reply all, forwarding, attachments, search and triage. All outbound test recipients were explicitly authorized. The Calendar and search repairs have focused native regressions; see [status](docs/status.md) and the [execution record](docs/research/phase-2-execution.md#d043d045--real-test-mailbox-ui-walkthrough).

**Phase 2 remains open; whole-area readiness stays 0/9.** Sustained authorization, actual Workspace policy, complete agreed offline coverage, protected backup/restore and current other-host acceptance remain required. Performance and the broad workspace sweep are deferred under D041.

Normal startup is empty. Sign-in alone does not import: confirm actual Cloud quota limits and protected-profile/backup admission, then choose **Start sync** for each account. Keep real data outside Git and cloud-sync folders. Removing an account archives local data and removes credentials without deleting anything at Google. Reminders and GUI-owned sync stop when the GUI closes; an explicit CLI session owns its own work.

Calendar search covers selected calendars in an explicit **1–42-day range**, with at most 100 matches and visible coverage limits. Following recurrence edits require confirmation and can need explicit partial/unknown recovery. Incomplete mail inventory is not a complete offline copy or a backup.

The historical Windows Package02 at `61a212a`, the `618e8a0` rendering correction and interrupted Package03 retain their [handoff evidence](docs/status.md#computer-handoff--start-here). The immutable 997-artifact Windows archive is preserved.

## Delivery direction

- Separate mail accounts and conversation-only browsing; combined Calendar shows only permitted details.
- Read-on-open follows a deliberate user action. Done clears unread and stars; Delay is untimed.
- Preserve MIME, original HTML and attachments while prioritizing readable text and human composition. Best-effort received HTML display is later work.
- Use Google APIs for current Gmail/Workspace mail and Calendar, including writes. GUI and CLI use the same commands and store.
- Follow the [September 14 reference](docs/research/assets/2026-09-14/flight-progress-v2/manifest.json), including the three themes and Mail-only timeline.
- Optional local models may classify or summarize; they never write replies or choose review/action state. Tasks, contacts, dashboards, assistant chat and semantic search are outside scope.

The implementation uses Rust, C++20/Qt Quick/QML, private local IPC and SQLite/FTS5 with retained blobs. Tools are pinned in `rust-toolchain.toml`, `Cargo.lock` and `scripts/toolchain.json`; pinned versions do not establish production readiness. No Node.js or separate JavaScript build pipeline is required.

## Next work

Use the configured Mac package for the founder’s immediate test-mailbox sign-in and fix observed blockers. Performance remains deferred under D041. Full Phase 2 acceptance later follows the [Phase 2 admission and comparison gates](docs/plans/phase-2.md#builds-the-founder-can-test) for controlled personal Gmail and Workspace testing, actual provider reconciliation, sustained refresh and complete agreed source coverage. Verify backup/restore and protection on the real profile volume. Current Windows/Linux CI, physical input, sustained vault behavior and distribution qualification remain separate.

IMAP, SMTP and external iCalendar subscriptions stay deferred until the founder has dogfooded the Google GUI and CLI. CalDAV and a broad public launch have separate scope.

## Historical evidence

Phases 0 and 1 are complete on available hosts. Their [foundation](docs/research/phase-0-execution.md) and [Phase 1 execution](docs/research/phase-1-execution.md) records retain source-specific CI, packages, performance and limitations. Those passes are not current Phase 2 other-host acceptance.

Earlier Phase 2 checkpoints, immutable source/artifact manifests, failed attempts and native diagnostics remain in the [Phase 2 execution record](docs/research/phase-2-execution.md). In particular, [D037 occurrence](https://github.com/yegors/fuckMail/blob/8155e998d5957e60c491475df7877553db2b453b/docs/research/assets/phase-2/p2-06c-interim/logs-manifest.json), [D036 account lifecycle](https://github.com/yegors/fuckMail/blob/8155e998d5957e60c491475df7877553db2b453b/docs/research/assets/phase-2/p2-01b-interim/logs-manifest.json), [D035 invitations](https://github.com/yegors/fuckMail/blob/8155e998d5957e60c491475df7877553db2b453b/docs/research/assets/phase-2/p2-06b-interim/logs-manifest.json), [B3B Drafts](https://github.com/yegors/fuckMail/blob/8155e998d5957e60c491475df7877553db2b453b/docs/research/assets/phase-2/p2-05b3b-interim/logs-manifest.json) and [B2's failed performance checkpoint](https://github.com/yegors/fuckMail/blob/8155e998d5957e60c491475df7877553db2b453b/docs/research/assets/phase-2/p2-05b2-interim/logs-manifest.json) are preserved. D038 resumed measurement; it did not rewrite D031 or its failed result.

## Documentation

- [Status](docs/status.md), [development plan](docs/development-plan.md), [Phase 2 packets](docs/plans/phase-2.md) and [build/launch guide](docs/build-development.md).
- [Index](docs/README.md), [north star](docs/north-star.md), [scope](docs/product-scope.md), [UI/UX](docs/ui-ux.md) and [decisions](docs/decisions.md).
- [Contracts](docs/contracts/README.md) own shared behavior; [research](docs/research/README.md) owns evidence and comparisons.
