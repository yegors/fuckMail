# Current status

**P1-10 is active under D019 before any new phase:** directly measure and fix the founder's sluggish mailbox/reader navigation, resize border/divider flicker, unreadable primary-button text and reader-action placement. Mail UI 13 / Quality 10 consume the unchanged schema7/fixture3/protocol1 shared boundary. Prior search passes do not cover these navigation/rendering interactions. [The spike packet](plans/phase-1.md#p1-10--navigation-and-rendering-spike-before-the-next-phase) owns its acceptance. The founder pushed `791254d`; [final P1-09 CI](research/phase-1-execution.md#p1-09-final-source-native-ci--791254d) passes all 9 tests and relocated development-package checks on Mac, Windows and Ubuntu, with packages/logs retained. The previous publication blocker is resolved.

As of 2026-09-12. **P1-09 Windows release tests, relocated package, preserved-profile upgrades and all four packaged search targets pass at source `8743162`; final-source Mac/Windows/Ubuntu CI passes at documentation/evidence checkpoint `791254d`. Phase 1 is not complete while P1-10 is active, and Google-first readiness remains 0/9.** The latest prototype uses schema 7/fresh fixture 3/additive protocol 1. [Phase 1 evidence](research/phase-1-execution.md) preserves exact results and earlier failures. Runtime `204fa62` remains the completed Phase 0 baseline; no live Google account or model is connected.

## Established direction

[D009](decisions.md) supersedes the earlier broad protocol/HTML gates. All current accounts use Google APIs. The first usable milestone includes both a small CLI and a GUI against one engine. Grow their workflows together, and have the founder dogfood usable Google clients before beginning IMAP/SMTP or external iCalendar subscriptions. Text-first reading and safe extraction remain early; best-effort received HTML display is later work.

Done/Delayed are explicit user decisions. Local classification only distinguishes human correspondence, automated senders/lists and user-defined noise, applying configured category/scope tags. It cannot mark mail reviewed or decide action state. Summaries and classification remain optional to enable; manual first use does not wait for models.

The proposed stack remains Rust engine/CLI, C++20/Qt Quick/QML UI, private local IPC and SQLite/FTS5. QML expressions are accepted, with no Node.js or separate JS compilation/bundling pipeline. WebEngine is not a first-build prerequisite. D016 adopts the September 12 [Flight Progress v2 export](research/flight-progress-v2.md) and requires close native implementation in place of invented controls. D017 explicitly confirms New message, Reply all, Forward and account-scoped Drafts as durable local workflows using the shared MIME/capture journal. D014's read-on-open, Done clearing unread and stars, untimed Delay, combined permitted calendar details and Mail-only timeline remain unchanged. D015 authorizes the local implementation; live account/model management remains later.

## Next work

**P1-10 follows the verified P1-09 source:** D018's second September 12 UI update includes General, Graphite/Paper/Harbour, text size/startup and timeline placement plus counted groups, sender-kind filters and exact-address cards. P1-09 boundary: Core 13, CLI 7, Mail UI 12, Calendar 11, Search 5, Triage 6, Platform 10, Quality 9; schema 7/fresh fixture 3/additive protocol 1. P1-10 advances Mail UI to 13 and Quality to 10. [GUI07/native05](research/assets/phase-1/p1-09-windows/p1-09-native-05.log) exits 0 through the updated Settings and mail controls, idle convergence after independent CLI changes, sender-card hover/Open/Filter/Write, all four composition modes, source-conflict refusal, calendar edits/RSVP and restart. Four fresh GUI processes verify Mail, Calendar and both last-workspace choices with saved appearance and resulting workspace.

The [complete release01 run](research/assets/phase-1/p1-09-windows/p1-09-full-release-01.log) passes **9/9 in 196.98 seconds**: all 163 nonignored Rust tests and a 54.17-second full GUI/restart walkthrough, including actual query priority, read/write barriers, bounded polling progress and idle CLI convergence. The [dev03 concurrent-ingestion report](research/assets/phase-1/p1-09-windows/p1-09-search-ingestion-dev-03.json) passes rare-subject/body/grouped/combined-filter p95 at **67.7413 / 73.0102 / 98.6387 / 168.3562 ms**, 100 target/reset pairs per class. Independent source/index/overlap checks pass for **5,150 actual arrivals / 206 pages**, with the original seed unchanged. Dev02's 200.8104 ms failure remains preserved.

Four [GUI07 Settings captures](research/assets/phase-1/p1-09-windows/visual-02/manifest.json) and five [final-source Settings captures](research/assets/phase-1/p1-09-windows/visual-03/manifest.json) cover all six sections and corrected spacing. The earlier nine GUI04 captures retain the other Update2 layout/size observations. The clean [8743162 package](research/assets/phase-1/p1-09-windows/p1-09-package-01.json) passes SDK-free relocation/full native GUI checks. Actual preserved [schema4→7](research/assets/phase-1/p1-09-windows/p1-09-upgrade-schema4-01.json) and [schema5→7](research/assets/phase-1/p1-09-windows/p1-09-upgrade-schema5-01.json) upgrades retain authored/source data and receipts, refuse an old-engine rollback without resetting data and reopen successfully. Schema-6 Core migration coverage remains distinct from a packaged upgrade: no historical schema-6 package exists.

The [packaged concurrent measurement](research/assets/phase-1/p1-09-windows/p1-09-search-ingestion-package-01.json) passes all four classes at p95 **70.1810 / 76.9568 / 110.3438 / 189.5710 ms**, with 100 target/reset pairs each and **5,100 independently verified arrivals / 204 atomic pages**. The source seed is unchanged and SDK paths are absent. The founder's push resolved the publication blocker. [Run 34703503086](research/assets/phase-1/p1-09-ci-791254d/run.json) completes P1-09's final-source Mac/Windows/Ubuntu native CI and relocated package checks at `791254d`; all six package/log artifacts are retained. These source-specific passes do not verify P1-10 navigation or resize behavior.

Earlier release attempts included a Rust relinking failure while a screenshot process held the engine, a subsequent scoped recovery and a later source-conflict correction. Their logs, the separate native shutdown warning and their narrower source coverage remain preserved in the execution record. The final complete release01 and clean package runs now cover the integrated corrections.

The [P1-07 audit](research/phase-1-execution.md#completion-audit-follow-up--p1-07) has passing native arrival/count/selection retention and edited-calendar/RSVP restart evidence. Earlier ingestion startup and measured-budget failures remain visible alongside the passing final runs. The earlier `738601e` Mac/Ubuntu CI passes and Windows aggregate Rust-suite watchdog failure retain their historical scope. Final `791254d` CI verifies the corrected allowance and integrated P1-09 fixes on all three hosts. Phase 1 remains incomplete until the active P1-10 spike passes.

Physical mouse/keyboard, clipboard/IME, accessibility and wider display-scaling checks remain explicitly unverified; desktop input automation was paused after Escape. Native programmatic controls, exact CLI readback, actual GUI restart and screenshots establish their recorded scopes only. The initial failed search measurements are retained alongside the passing optimized results.

Before actual Google accounts, implement/test the OS credential-store adapter and settle OAuth access plus real-data retention/encryption/recovery policy. Full history, live sending/calendar operations and founder Google dogfooding remain later milestones. Legacy transports remain deferred.

The home Windows 11 PC is ready for Phase 1 development at planning checkpoint `5bed257` on `codex/phase-1-plan`, with a local Windows tool-lookup correction. Bootstrap/doctor, dev and release builds (8/8 CTests each), relocated package checks and native Direct3D 11 rendering on the RTX 5090 pass. The schema-4 package is preserved locally for Phase 1 migration checks; see [Windows handoff evidence](research/phase-0-execution.md#home-windows-handoff-before-phase-1). Run `scripts/dev.ps1 run` from PowerShell. Do not copy profiles, credentials or SDK/build directories through Git. Manual Windows clipboard/IME/accessibility and wider scaling checks, OS keyrings and physical Linux desktop checks remain unverified.

## Google-first readiness

| Required area | State | Verified |
| --- | --- | --- |
| [Core/offline storage](contracts/core-storage.md) | Schema-7 native draft/restart, Core migration and preserved package upgrades pass; whole area Draft | No |
| [Gmail/Workspace](contracts/gmail.md) | Phase 0 synthetic experiment verified; whole area Draft | No |
| [Mail UI/composition](contracts/mail-ui.md) | P1-09 native/package workflows pass on three CI hosts; P1-10 navigation/rendering spike active; whole area Draft | No |
| [Search](contracts/search.md) | Native filters/idle convergence and all four development/packaged concurrent budgets pass; whole area Draft | No |
| [Triage/categories](contracts/triage.md) | Phase 1 explicit review/Done/Delay verified locally; whole area Draft | No |
| [Google Calendar](contracts/calendar.md) | P1-09 local views/writes, header timeline, duration preference and edited-event/RSVP restart pass; live Google pending; whole area Draft | No |
| [Optional local assistance](contracts/local-ai.md) | Phase 0 synthetic experiment verified; whole area Draft | No |
| [Platform/install/lifecycle](contracts/platform.md) | P1-09 Mac/Windows/Ubuntu native tests and relocated development packages pass; Windows upgrades pass; whole area Draft | No |
| [CLI](contracts/cli.md) | Paired composition/MIME/restart readback verified locally; whole area Draft | No |

The denominator changed from ten to nine under D009: IMAP/SMTP is now a deferred area, and external subscriptions are outside the current Calendar gates. This is a reporting scope change, not progress. Local assistance remains an intended capability, optional to enable. Partial CLI or text-only GUI delivery can satisfy the first usable slice without marking its entire feature Verified. Record completed slice evidence explicitly; docs and screenshots do not count.

The Google-first release still requires its applicable quality and OS gates. It does not require pixel-perfect HTML or later protocol compatibility. First use, fuller Google dogfooding and all-platform release verification are distinct evidence: none proves the others automatically.

## Phase overview

| Phase | Outcome | State |
| --- | --- | --- |
| 0 — Paired foundation | Native builds, small fixtures, storage, private engine/CLI/GUI, text/composer and Google prerequisites | Completed on available hosts; external/physical gates explicit |
| 1 — Redesigned local client | September 12 native design and paired CLI, shared triage/search/preferences, durable new/reply/reply-all/forward drafts/capture and combined calendar workflows | P1-09 Windows acceptance and final-source three-host CI/packages pass; P1-10 navigation/rendering spike active |
| 2 — Google daily workflows | Gmail/Workspace and Google Calendar, growing CLI with GUI; controlled founder use | Planned |
| 3 — Google completion and dogfooding | Usable Google GUI and CLI, expanded designs, later best-effort HTML/local assistance, recovery/performance and delivery evidence | Planned |
| 4 — Later protocols | IMAP, SMTP and external iCalendar subscriptions, only after actual Google dogfooding | Deferred; not started |
| Later | CalDAV and possible public/open-source launch | Separate scope |

## Evidence and gates

The supplied approximately 25 GB eM Client snapshot is a disk footprint, not a verified message count. Research, published examples and mockups are not benchmark results. No account has been connected during planning.

Before legacy work begins, record which Google GUI/CLI workflows the founder used, issues found and fixes/retests. A proposed two-week trial is an evaluation suggestion, not an accepted fixed duration. Future protocol coverage has its own readiness reporting and cannot retroactively block the earlier Google milestone.

Upcoming inputs include actual build/test hosts, Google OAuth/admin access, retention/encryption/recovery policy before real storage, and later provider/settings interactions. D014 resolves the current read/Done/Delay and calendar/timeline choices. Models and later HTML dependencies have separate gates. The existing exclusions, full offline requirement and explicit GUI/CLI lifetime rules remain unchanged.
