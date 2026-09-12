# Current status

As of 2026-09-12. **The Windows development chain is ready; Phase 1 final acceptance is in progress. Google-first MVP readiness remains 0/9 whole areas verified.** Branch `codex/phase-1` uses schema 5, fresh fixture 2 and additive protocol 1. Runtime `1132c44` passes all nine Windows release tests, relocated SDK-free package checks and actual schema 4→5 preservation/refusal. Warm 100k Core search passes; the packaged native rerun exceeded the combined-filter paint target and is under correction. Native Mail, Calendar, Settings and reply captures have been reviewed at the reference and compact sizes. See [Phase 1 execution evidence](research/phase-1-execution.md). Runtime `204fa62` remains the completed Phase 0 baseline; [its evidence](research/phase-0-execution.md) is unchanged. No live Google account or model is connected.

## Established direction

[D009](decisions.md) supersedes the earlier broad protocol/HTML gates. All current accounts use Google APIs. The first usable milestone includes both a small CLI and a GUI against one engine. Grow their workflows together, and have the founder dogfood usable Google clients before beginning IMAP/SMTP or external iCalendar subscriptions. Text-first reading and safe extraction remain early; best-effort received HTML display is later work.

Done/Delayed are explicit user decisions. Local classification only distinguishes human correspondence, automated senders/lists and user-defined noise, applying configured category/scope tags. It cannot mark mail reviewed or decide action state. Summaries and classification remain optional to enable; manual first use does not wait for models.

The proposed stack remains Rust engine/CLI, C++20/Qt Quick/QML UI, private local IPC and SQLite/FTS5. QML expressions are accepted, with no Node.js or separate JS compilation/bundling pipeline. WebEngine is not a first-build prerequisite. Under D014 the [Flight Progress v2 HTML prototype](research/flight-progress-v2.md) guides Phase 1. The founder confirmed read-on-open, Done clearing unread and stars, untimed Delay, combined full permitted calendar details, and a browsable timeline only in Mail. D015 authorizes end-to-end implementation of the prepared local slice. Live account/model management remains later.

## Next work

Finish [P1-07](plans/phase-1.md#p1-07--acceptance-and-packages): close the remaining native filter/discard/capture/timeline interaction checks, verify the packaged search correction, then run native CI and packages for the final source on Mac, Windows and Ubuntu. The earlier `738601e` checkpoint passed Mac/Ubuntu; Windows exhausted its old aggregate Rust-suite watchdog while individual tests continued passing. The corrected 600-second suite allowance and later runtime fixes pass locally but need fresh CI. Automatic approval review rejected the next GitHub push because publishing the source snapshot required explicit authorization; changes stay local until approved. Phase 1 is not complete yet.

Physical mouse/keyboard, clipboard/IME, accessibility and wider display-scaling checks remain explicitly unverified; desktop input automation was paused after Escape. Native programmatic controls, exact CLI readback, actual GUI restart and screenshots establish their recorded scopes only. The initial failed search measurements are retained alongside the passing optimized results.

Before actual Google accounts, implement/test the OS credential-store adapter and settle OAuth access plus real-data retention/encryption/recovery policy. Full history, live sending/calendar operations and founder Google dogfooding remain later milestones. Legacy transports remain deferred.

The home Windows 11 PC is ready for Phase 1 development at planning checkpoint `5bed257` on `codex/phase-1-plan`, with a local Windows tool-lookup correction. Bootstrap/doctor, dev and release builds (8/8 CTests each), relocated package checks and native Direct3D 11 rendering on the RTX 5090 pass. The schema-4 package is preserved locally for Phase 1 migration checks; see [Windows handoff evidence](research/phase-0-execution.md#home-windows-handoff-before-phase-1). Run `scripts/dev.ps1 run` from PowerShell. Do not copy profiles, credentials or SDK/build directories through Git. Manual Windows clipboard/IME/accessibility and wider scaling checks, OS keyrings and physical Linux desktop checks remain unverified.

## Google-first readiness

| Required area | State | Verified |
| --- | --- | --- |
| [Core/offline storage](contracts/core-storage.md) | Phase 0 synthetic experiment verified; whole area Draft | No |
| [Gmail/Workspace](contracts/gmail.md) | Phase 0 synthetic experiment verified; whole area Draft | No |
| [Mail UI/composition](contracts/mail-ui.md) | Phase 0 synthetic experiment verified; whole area Draft | No |
| [Search](contracts/search.md) | Phase 0 synthetic experiment verified; whole area Draft | No |
| [Triage/categories](contracts/triage.md) | Phase 0 synthetic experiment verified; whole area Draft | No |
| [Google Calendar](contracts/calendar.md) | Phase 0 synthetic experiment verified; whole area Draft | No |
| [Optional local assistance](contracts/local-ai.md) | Phase 0 synthetic experiment verified; whole area Draft | No |
| [Platform/install/lifecycle](contracts/platform.md) | Phase 0 synthetic experiment verified; whole area Draft | No |
| [CLI](contracts/cli.md) | Phase 0 synthetic experiment verified; whole area Draft | No |

The denominator changed from ten to nine under D009: IMAP/SMTP is now a deferred area, and external subscriptions are outside the current Calendar gates. This is a reporting scope change, not progress. Local assistance remains an intended capability, optional to enable. Partial CLI or text-only GUI delivery can satisfy the first usable slice without marking its entire feature Verified. Record completed slice evidence explicitly; docs and screenshots do not count.

The Google-first release still requires its applicable quality and OS gates. It does not require pixel-perfect HTML or later protocol compatibility. First use, fuller Google dogfooding and all-platform release verification are distinct evidence: none proves the others automatically.

## Phase overview

| Phase | Outcome | State |
| --- | --- | --- |
| 0 — Paired foundation | Native builds, small fixtures, storage, private engine/CLI/GUI, text/composer and Google prerequisites | Completed on available hosts; external/physical gates explicit |
| 1 — Redesigned local client | Updated native design and paired CLI, shared triage/search/preferences, replies/capture and combined calendar workflows | Windows package/upgrade pass; final interaction/performance checks and CI pending |
| 2 — Google daily workflows | Gmail/Workspace and Google Calendar, growing CLI with GUI; controlled founder use | Planned |
| 3 — Google completion and dogfooding | Usable Google GUI and CLI, expanded designs, later best-effort HTML/local assistance, recovery/performance and delivery evidence | Planned |
| 4 — Later protocols | IMAP, SMTP and external iCalendar subscriptions, only after actual Google dogfooding | Deferred; not started |
| Later | CalDAV and possible public/open-source launch | Separate scope |

## Evidence and gates

The supplied approximately 25 GB eM Client snapshot is a disk footprint, not a verified message count. Research, published examples and mockups are not benchmark results. No account has been connected during planning.

Before legacy work begins, record which Google GUI/CLI workflows the founder used, issues found and fixes/retests. A proposed two-week trial is an evaluation suggestion, not an accepted fixed duration. Future protocol coverage has its own readiness reporting and cannot retroactively block the earlier Google milestone.

Upcoming inputs include actual build/test hosts, Google OAuth/admin access, retention/encryption/recovery policy before real storage, and later provider/settings interactions. D014 resolves the current read/Done/Delay and calendar/timeline choices. Models and later HTML dependencies have separate gates. The existing exclusions, full offline requirement and explicit GUI/CLI lifetime rules remain unchanged.
