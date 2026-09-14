# Current status

**D047 CI gate:** further Phase 2 feedback work waits for green macOS, Windows and Ubuntu native/package pipelines. [CI repairs and verification](research/phase-2-execution.md#d047--p2-ci--september-14-hosted-verification-repairs) preserve the original failures and unchanged acceptance budgets.

**D046 repository cleanup:** generated Phase 0–2 evidence is preserved in a verified local archive and historical commits, with the current tree retaining reports and supplied designs. [Storage, recovery and checks](research/evidence-archive.md) replace the old tracked-evidence convention. Quality 27 changes documentation/CI storage only; runtime results retain their original revisions and Phase 2 readiness remains unchanged.

**D043–D045 — real-mailbox UI repairs:** the founder has signed into the configured Mac app. Real UI testing found and fixed Calendar creation after first sync, recurring editor invalidation, accessible event activation and search-popup focus/dismissal. Mail and Calendar walkthrough results are recorded below. Performance and the broad workspace sweep remain deferred under D041; this is not full Phase 2 acceptance.

As of 2026-09-14 (Toronto).

## D040 / P2-01C — Mac email-first sign-in

Settings now takes a Gmail/Workspace email and opens the system browser. Configured builds carry the app's OAuth registration; users do not choose a JSON file. CLI, reconnect and same-subject Restore share this flow. The developer setup helper stores the existing registration outside Git and includes it in relocated packages.

**Verified:** Release build, strict Rust lint, isolated engine **169 unit + five process** tests, isolated CLI **seven unit + 18 process** tests, full native **22/22 in 141.10 seconds**, SDK-free relocated Mac package (D042 Package02 includes the supplied app registration), CLI account lifecycle **6.696 seconds**, and packaged native email-entry/account lifecycle **9.610 seconds**. FileVault reports On; random-namespace Keychain roundtrip/cleanup and public signing-key HTTPS pass. The ordinary full workspace rerun was stopped at the founder’s D041 request after its first 600-second timeout; native callback and test-precondition failures were fixed and their original results retained.

**Current UI build:** native03 **22/22 in 90.06 seconds**, followed by the final one-line filter-padding correction and focused search **3.65 seconds**. Configured UI Package03 passes its full relocated GUI/CLI/restart/shutdown checks, with all **359** archive entries verified and **423** final source inputs unchanged. The final real UI readback confirms active sync, usable new-event controls, readable filters and Agenda details. The app is left running on the existing test account; see the execution record for exact source limits and unsigned-package diagnostics.

**Real UI evidence under D043–D045:** new mail, Reply all and forward with To/Cc/Bcc were accepted by Google, with a downloaded attachment matching the original bytes. Draft text, Unicode, routing and attachment selection survived restart. Body search, unread/read-on-open, Delay/Done and Trash/Restore passed on test mail. Timed/all-day creation, organizer edits and invitations, one-occurrence and following edits, series/occurrence cancellations, cached event search and event activation passed. All test Calendar occurrences were cancelled; unrelated content was preserved. Recipient delivery is not inferred from provider acceptance. The native search regression reproduced the Escape/focus defect and passes after repair.

**Remaining:** sustained refresh, real Workspace policy/multiple-account comparison, complete agreed source/attachment inventory, protected backup/restore and current other-host acceptance. Incoming invitation RSVP and destructive account lifecycle were exercised by the isolated native suite, not against the connected real account. Performance and the broad workspace sweep remain deferred. Phase 2 and whole-area readiness stay **0/9**.

Launch `dist/darwin-arm64/fuckmail-gui.app` from the Mac package after [one-time registration setup](build-development.md#email-first-google-setup-d040). Current boundary: **Core 27/Gmail 13/Calendar 21/Platform 15/Mail UI 28/CLI 21/Quality 26/Search 5/schema 20**; fixtures 0/3 and protocol 1 unchanged. The [execution record](research/phase-2-execution.md) and one Mac evidence directory own exact inputs/binaries and all results. Historical Windows results below retain their source scope.

## Computer handoff — start here

Historical departing-host handoff: the founder requested commit/push and continuation on another computer. Its runtime source was **`618e8a0`** on `codex/phase-2`. The final mail-list rendering correction passes **nine repeated native refreshes** and normal **CTest03 22/22 in 214.33 seconds**, with all 369 inputs and binaries unchanged. It fixes the actual stale text/geometry found by large-navigation01; the unchanged packaged performance rerun has **not run** on this correction.

**Package03 was interrupted, not accepted.** Its log stops at the relocated full GUI self-test, no terminal result exists, and the handoff process inspection found no remaining package/app processes. Package02 at `61a212a` remains the last fully verified package and predates the fix. Preserve the [997-artifact evidence archive](https://github.com/yegors/fuckMail/blob/8155e998d5957e60c491475df7877553db2b453b/docs/research/assets/phase-2/p2-final/manifest.json), including failures and interrupted logs.

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

Diagnose the Mac focus interruption and complete packaged performance checks and the real-registration setup, recording exact source/binary scope. The D040 Mac package now passes relocation checks. The prior Windows package is unsigned and does not establish current Mac/Linux CI, clean-machine installation, accessibility or physical-input qualification.

Use the [feedback launch guide](build-development.md#phase-2-feedback-launch) and [Phase 2 admission gates](plans/phase-2.md#builds-the-founder-can-test) for actual personal Gmail and Workspace accounts. Configured D040 builds use **Connect Google account → email → Continue with Google**; developer registration setup belongs to the app build. Sign-in and Restore do not start acquisition; each account needs confirmed Cloud quota values and explicit Start sync. No actual Google account, external Send or invitation has been exercised by these generated checks.

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
| [Mail UI/composition](contracts/mail-ui.md) | Windows checkpoints and current Mac native/package checks pass; final performance and real use pending | No |
| [Search](contracts/search.md) | Mail search and bounded cached Calendar search implemented; final current performance/scale qualification pending | No |
| [Triage/categories](contracts/triage.md) | Explicit review/Done/Delay verified locally; real-account use pending | No |
| [Google Calendar](contracts/calendar.md) | Invitations, supported recurrence scopes/RSVP, reminders and search pass scoped checks; real provider/edge-case acceptance pending | No |
| [Optional local assistance](contracts/local-ai.md) | Phase 0 synthetic experiment; optional runtime qualification remains separate | No |
| [Platform/install/lifecycle](contracts/platform.md) | Mac/Windows package checkpoints, native vault roundtrip and lifecycle checks; current other-host/live/distribution gates pending | No |
| [CLI](contracts/cli.md) | Shared mail/Calendar/account workflows and Mac email-first checks pass; full real-use qualification pending | No |

The denominator changed from ten to nine under D009 because IMAP/SMTP left the current target. This was a scope change, not implementation progress. A feedback build, partial provider fixture or screenshot cannot mark an entire area Verified.

## Historical evidence

Phases 0 and 1 are complete on available hosts. [Phase 0](research/phase-0-execution.md) and [Phase 1](research/phase-1-execution.md) retain exact CI, package, performance and physical/platform limits. Final Phase 1 three-host CI belongs to its original source and does not prove current Phase 2 support.

All earlier Phase 2 checkpoint data, failures, immutable archives and source/binary bindings remain in the [owning execution record](research/phase-2-execution.md). This includes [D037 occurrence](https://github.com/yegors/fuckMail/blob/8155e998d5957e60c491475df7877553db2b453b/docs/research/assets/phase-2/p2-06c-interim/logs-manifest.json), [D036 account lifecycle](https://github.com/yegors/fuckMail/blob/8155e998d5957e60c491475df7877553db2b453b/docs/research/assets/phase-2/p2-01b-interim/logs-manifest.json), [D035 invitations](https://github.com/yegors/fuckMail/blob/8155e998d5957e60c491475df7877553db2b453b/docs/research/assets/phase-2/p2-06b-interim/logs-manifest.json), [B3B](https://github.com/yegors/fuckMail/blob/8155e998d5957e60c491475df7877553db2b453b/docs/research/assets/phase-2/p2-05b3b-interim/logs-manifest.json), [B3A](https://github.com/yegors/fuckMail/blob/8155e998d5957e60c491475df7877553db2b453b/docs/research/assets/phase-2/p2-05b3a-interim/logs-manifest.json) and [B2](https://github.com/yegors/fuckMail/blob/8155e998d5957e60c491475df7877553db2b453b/docs/research/assets/phase-2/p2-05b2-interim/logs-manifest.json). B2's failed 100-ms measurement remains failed; D031's deferral was superseded prospectively by the founder's D038 measurement request, without rewriting that decision or its evidence.

## Phase overview

| Phase | Outcome | State |
| --- | --- | --- |
| 0 — Paired foundation | Native GUI/CLI/store and synthetic prerequisites | Complete on available hosts; physical/live limits explicit |
| 1 — Redesigned local client | Shared local workflows, themes, navigation and packages | Complete on available hosts, on its recorded source |
| 2 — Google daily workflows | Gmail/Workspace and Calendar with controlled founder use | D040 Mac email-first package verified; final performance and actual-account admission/closeout remain open |
| 3 — Google completion and dogfooding | Real-use fixes, later HTML/local assistance and broader qualification | Planned, informed by founder feedback |
| 4 — Later protocols | IMAP/SMTP and external iCalendar subscriptions | Deferred until Google dogfooding |
| Later | CalDAV and possible public launch | Separate scope |

The supplied approximately 25 GB eM Client snapshot is a disk footprint, not a verified message count. Full offline storage, sustained authorization and defect closure require actual evidence. A suggested two-week trial is not a fixed user requirement. [Phase 2](plans/phase-2.md) distinguishes first feedback, canary writes and daily-driver acceptance.
