# Current status

As of 2026-09-14 (Toronto).

## Computer handoff — start here

The founder requests a commit/push and continuation on another computer. Latest runtime source is **`618e8a0`** on `codex/phase-2`. The final mail-list rendering correction passes **nine repeated native refreshes** and normal **CTest03 22/22 in 214.33 seconds**, with all 369 inputs and binaries unchanged. It fixes the actual stale text/geometry found by large-navigation01; the unchanged packaged performance rerun has **not run** on this correction.

**Package03 was interrupted, not accepted.** Its log stops at the relocated full GUI self-test, no terminal result exists, and the handoff process inspection found no remaining package/app processes. Package02 at `61a212a` remains the last fully verified package and predates the fix. Preserve the [997-artifact evidence archive](research/assets/phase-2/p2-final/manifest.json), including failures and interrupted logs.

On the destination, pull `codex/phase-2`, run the host's [bootstrap/doctor/build workflow](build-development.md#first-setup), complete package verification, then measure the final package with the unchanged navigation/search/resize verifiers. The source, reference prototype, tests and evidence travel in Git; `.tools`, `build`, `target`, `dist`, profiles/credentials and the large generated seed do not. Extract the evidence ZIP separately if earlier local helpers/logs are needed. Continue protected actual-account and remaining Phase 2 acceptance afterward; readiness remains 0/9. No further implementation or benchmark was started after the handoff request.

## D039 Windows feedback build

**Package02 is built and verified from clean `61a212aee58592ddbd018c4f87061508e178f2ee`.** The D038/D039 Windows client includes empty live default startup, Settings account setup/removal/restoration, human mail composition and Google Drafts/Send, supported Calendar invitations/recurrence/self-RSVP, running-app reminders, partial-grant acquisition, the September 14 native design and cached Calendar search. GUI and CLI share the existing store.

Current consumed boundary: **Core 27, Gmail 12, Calendar 20, Platform 14, Mail UI 25, CLI 20, Quality 23, Search 5 and schema 20**. Live fixture 0, synthetic fixture 3 and protocol 1 are unchanged. These implemented slices do not close whole-product or actual-account gates; Google-first readiness remains **0/9**.

The [execution record](research/phase-2-execution.md) owns exact source, binary and result scopes:

- The earlier D038 workspace run passes **599 tests/five ignored**, strict lint and formatting on its 249 Rust/fixture inputs. Later corrections have focused tests and rebuilt binaries; that workspace result is not relabeled as a D039 rerun.
- D039's five Core search groups, two CLI checks, actual ordinary CLI search and strengthened recurring CLI regression pass. Release03 passes **165 isolated engine unit + five process** and **seven isolated CLI unit + 18 process** tests, with 251 Rust/fixture inputs unchanged.
- Earlier normal native CTest02 passes **22/22** in 230.81 seconds. Final CTest03 on the list correction passes **22/22** (21 native checks and its Rust build fixture) in **214.33 seconds**, with 369 scoped inputs and binaries unchanged; four delegate notices and one teardown notice remain. Focused search checks cover stale context, exact occurrence selection and all themes.
- Package02 passes in **206.140 seconds**, with 391 source inputs unchanged, SDK-free relocated GUI/CLI and empty-live startup checks. The 74,180,062-byte ZIP has SHA-256 `5f0c98edeeb37a3a7d29312455514bc1f0ae7bae2bd4aaf26123f060b336fbc0`. Package02's own executable identities apply; packaging rebuilt the ordinary engine under its Visual Studio environment.
- Generated-sync navigation01 on the earlier Package02 passes p95 at **66.1764 / 69.0417 / 67.8952 / 29.6932 ms** for mailbox click/Tab/Shift+Tab/reader. Separate large-navigation01 fails `visible_snapshot`; its rendering correction is committed and passes native regressions. **Final packaged performance remains pending.** Preserve all prior failures and unchanged targets.

## Next work and first-user limits

Complete package verification and unchanged packaged performance checks on the destination host, recording exact source/binary scope. The prior Windows package is unsigned and does not establish current Mac/Linux CI, clean-machine installation, accessibility or physical-input qualification.

Use the [feedback launch guide](build-development.md#phase-2-feedback-launch) and [Phase 2 admission gates](plans/phase-2.md#builds-the-founder-can-test) for actual personal Gmail and Workspace accounts. Settings uses **Connect Google account** and the selected OAuth desktop-client JSON must remain at its saved path. Sign-in and Restore do not start acquisition; each account needs confirmed Cloud quota values and explicit Start sync. No actual Google account, external Send or invitation has been exercised by these generated checks.

The founder reports encryption On for C:. Administrator-level native status reads were denied, so reported encryption and tool verification remain distinct. The generated native Credential Manager roundtrip/cleanup and private-profile checks pass; actual locked-vault, revocation, Workspace policy, sustained refresh and protected backup/restore still need evidence. The read-only removal archive is the conservative D036 implementation choice; it is not an agreed backup/retention policy or an answer to the earlier optional preference question.

Current mail preparation buffers RAW up to 40 MiB, extracted text up to 256 KiB and at most 50 attachments; oversized or malformed items expose incomplete coverage. Full agreed inventory/source/attachment reconciliation remains necessary before claiming complete offline mail. Calendar search is cached, selected-calendar and explicitly range-bounded (1–42 days, at most 100 returned matches). Recurrence authoring accepts the supported common rule subset; following edits reset later exceptions only after confirmation, and uncertain/partial writes require explicit recovery. No receipt proves invitation notification delivery. Closing the GUI stops its reminders and sync; independent CLI owners remain explicit.

## Established direction

All current accounts use Google APIs. Separate mail accounts, conversation-only browsing, combined permitted Calendar details and a Mail-only timeline remain product constraints. Deliberate read-on-open marks reviewed; Done clears unread and stars; Delay is untimed. Humans write messages and choose review/action state. Optional local classification/summaries remain separate and off by default; best-effort received HTML rendering is later work.

Rust, C++20/Qt Quick/QML, private local IPC and SQLite/FTS5 form the implemented foundation. The [September 14 reference](research/assets/2026-09-14/flight-progress-v2/manifest.json) is the current visual target. No Node.js or separate JavaScript build pipeline is required. IMAP/SMTP, external iCalendar subscriptions and broader public release remain deferred until Google dogfooding and their separate decisions.

## Google-first readiness

| Required area | Current scoped evidence | Verified whole area |
| --- | --- | --- |
| [Core/offline storage](contracts/core-storage.md) | Schema-20 journals, lifecycle and upgrades; complete real source coverage/backup pending | No |
| [Gmail/Workspace](contracts/gmail.md) | Acquisition, composition, Drafts, explicit Send and recovery pass generated-provider checks; actual accounts pending | No |
| [Mail UI/composition](contracts/mail-ui.md) | Current Windows native workflows and package pass; final performance and real use pending | No |
| [Search](contracts/search.md) | Mail search and bounded cached Calendar search implemented; final current performance/scale qualification pending | No |
| [Triage/categories](contracts/triage.md) | Explicit review/Done/Delay verified locally; real-account use pending | No |
| [Google Calendar](contracts/calendar.md) | Invitations, supported recurrence scopes/RSVP, reminders and search pass scoped checks; real provider/edge-case acceptance pending | No |
| [Optional local assistance](contracts/local-ai.md) | Phase 0 synthetic experiment; optional runtime qualification remains separate | No |
| [Platform/install/lifecycle](contracts/platform.md) | Current Windows package, native vault roundtrip and lifecycle checks; other-host/live/trusted-distribution gates pending | No |
| [CLI](contracts/cli.md) | Shared mail/Calendar/account workflows and current Windows CLI checks pass; full real-use qualification pending | No |

The denominator changed from ten to nine under D009 because IMAP/SMTP left the current target. This was a scope change, not implementation progress. A feedback build, partial provider fixture or screenshot cannot mark an entire area Verified.

## Historical evidence

Phases 0 and 1 are complete on available hosts. [Phase 0](research/phase-0-execution.md) and [Phase 1](research/phase-1-execution.md) retain exact CI, package, performance and physical/platform limits. Final Phase 1 three-host CI belongs to its original source and does not prove current Phase 2 support.

All earlier Phase 2 checkpoint data, failures, immutable archives and source/binary bindings remain in the [owning execution record](research/phase-2-execution.md). This includes [D037 occurrence](research/assets/phase-2/p2-06c-interim/logs-manifest.json), [D036 account lifecycle](research/assets/phase-2/p2-01b-interim/logs-manifest.json), [D035 invitations](research/assets/phase-2/p2-06b-interim/logs-manifest.json), [B3B](research/assets/phase-2/p2-05b3b-interim/logs-manifest.json), [B3A](research/assets/phase-2/p2-05b3a-interim/logs-manifest.json) and [B2](research/assets/phase-2/p2-05b2-interim/logs-manifest.json). B2's failed 100-ms measurement remains failed; D031's deferral was superseded prospectively by the founder's D038 measurement request, without rewriting that decision or its evidence.

## Phase overview

| Phase | Outcome | State |
| --- | --- | --- |
| 0 — Paired foundation | Native GUI/CLI/store and synthetic prerequisites | Complete on available hosts; physical/live limits explicit |
| 1 — Redesigned local client | Shared local workflows, themes, navigation and packages | Complete on available hosts, on its recorded source |
| 2 — Google daily workflows | Gmail/Workspace and Calendar with controlled founder use | D039 Windows feedback package verified; final performance and actual-account admission/closeout remain open |
| 3 — Google completion and dogfooding | Real-use fixes, later HTML/local assistance and broader qualification | Planned, informed by founder feedback |
| 4 — Later protocols | IMAP/SMTP and external iCalendar subscriptions | Deferred until Google dogfooding |
| Later | CalDAV and possible public launch | Separate scope |

The supplied approximately 25 GB eM Client snapshot is a disk footprint, not a verified message count. Full offline storage, sustained authorization and defect closure require actual evidence. A suggested two-week trial is not a fixed user requirement. [Phase 2](plans/phase-2.md) distinguishes first feedback, canary writes and daily-driver acceptance.
