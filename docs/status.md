# Current status

As of 2026-09-11. **Phase 0 completed on available hosts; Phase 1 implementation is active on `codex/phase-1`. Google-first MVP readiness remains 0/9 whole areas verified.** The Windows toolchain handoff is committed at `c82bac8`. The new local slice uses schema 5, fixture 2 and additive protocol 1; Core/CLI tests and the first integrated Windows compilation pass, but the Phase 1 acceptance/package/visual gates are still open. See [Phase 1 execution evidence](research/phase-1-execution.md). Runtime checkpoint `204fa62` remains the verified Phase 0 baseline: Mac/Windows/Ubuntu native CI, relocated packages and the Mac schema 2→4 upgrade/refusal walkthrough passed. [Phase 0 execution evidence](research/phase-0-execution.md) remains unchanged. No live Google account or model is connected.

## Established direction

[D009](decisions.md) supersedes the earlier broad protocol/HTML gates. All current accounts use Google APIs. The first usable milestone includes both a small CLI and a GUI against one engine. Grow their workflows together, and have the founder dogfood usable Google clients before beginning IMAP/SMTP or external iCalendar subscriptions. Text-first reading and safe extraction remain early; best-effort received HTML display is later work.

Done/Delayed are explicit user decisions. Local classification only distinguishes human correspondence, automated senders/lists and user-defined noise, applying configured category/scope tags. It cannot mark mail reviewed or decide action state. Summaries and classification remain optional to enable; manual first use does not wait for models.

The proposed stack remains Rust engine/CLI, C++20/Qt Quick/QML UI, private local IPC and SQLite/FTS5. QML expressions are accepted, with no Node.js or separate JS compilation/bundling pipeline. WebEngine is not a first-build prerequisite. Under D014 the [Flight Progress v2 HTML prototype](research/flight-progress-v2.md) guides Phase 1. The founder confirmed read-on-open, Done clearing unread and stars, untimed Delay, combined full permitted calendar details, and a browsable timeline only in Mail. D015 authorizes end-to-end implementation of the prepared local slice. Live account/model management remains later.

## Next work

Complete and verify the active [Phase 1 packets](plans/phase-1.md): shared Core/CLI state, native mail/calendar/settings, durable formatted replies and bounded local calendar mutations. Integration is in progress. The first 100k grouped/filter benchmark exceeded the proposed response budget; query optimization is active and the failed measurements are preserved. Full GUI acceptance, design comparisons, schema 4→5 preservation/refusal and native packages remain required before Phase 1 is complete.

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
| 1 — Redesigned local client | Updated native design and paired CLI, shared triage/search/preferences, replies/capture and combined calendar workflows | Implementation and integration active; acceptance gates open |
| 2 — Google daily workflows | Gmail/Workspace and Google Calendar, growing CLI with GUI; controlled founder use | Planned |
| 3 — Google completion and dogfooding | Usable Google GUI and CLI, expanded designs, later best-effort HTML/local assistance, recovery/performance and delivery evidence | Planned |
| 4 — Later protocols | IMAP, SMTP and external iCalendar subscriptions, only after actual Google dogfooding | Deferred; not started |
| Later | CalDAV and possible public/open-source launch | Separate scope |

## Evidence and gates

The supplied approximately 25 GB eM Client snapshot is a disk footprint, not a verified message count. Research, published examples and mockups are not benchmark results. No account has been connected during planning.

Before legacy work begins, record which Google GUI/CLI workflows the founder used, issues found and fixes/retests. A proposed two-week trial is an evaluation suggestion, not an accepted fixed duration. Future protocol coverage has its own readiness reporting and cannot retroactively block the earlier Google milestone.

Upcoming inputs include actual build/test hosts, Google OAuth/admin access, retention/encryption/recovery policy before real storage, and later provider/settings interactions. D014 resolves the current read/Done/Delay and calendar/timeline choices. Models and later HTML dependencies have separate gates. The existing exclusions, full offline requirement and explicit GUI/CLI lifetime rules remain unchanged.
