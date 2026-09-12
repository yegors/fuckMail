# Current status

As of 2026-09-12. **P1-09 Windows release tests pass 9/9, including 163 Rust tests and the full GUI/restart walkthrough. All four concurrent native search classes now meet the 200 ms p95 target. Final package, upgrades and other-host CI are pending; Phase 1 is not complete and Google-first readiness remains 0/9.** The latest prototype uses schema 7/fresh fixture 3/additive protocol 1. [Phase 1 evidence](research/phase-1-execution.md) preserves exact results and earlier failures. Runtime `204fa62` remains the completed Phase 0 baseline; no live Google account or model is connected.

## Established direction

[D009](decisions.md) supersedes the earlier broad protocol/HTML gates. All current accounts use Google APIs. The first usable milestone includes both a small CLI and a GUI against one engine. Grow their workflows together, and have the founder dogfood usable Google clients before beginning IMAP/SMTP or external iCalendar subscriptions. Text-first reading and safe extraction remain early; best-effort received HTML display is later work.

Done/Delayed are explicit user decisions. Local classification only distinguishes human correspondence, automated senders/lists and user-defined noise, applying configured category/scope tags. It cannot mark mail reviewed or decide action state. Summaries and classification remain optional to enable; manual first use does not wait for models.

The proposed stack remains Rust engine/CLI, C++20/Qt Quick/QML UI, private local IPC and SQLite/FTS5. QML expressions are accepted, with no Node.js or separate JS compilation/bundling pipeline. WebEngine is not a first-build prerequisite. D016 adopts the September 12 [Flight Progress v2 export](research/flight-progress-v2.md) and requires close native implementation in place of invented controls. D017 explicitly confirms New message, Reply all, Forward and account-scoped Drafts as durable local workflows using the shared MIME/capture journal. D014's read-on-open, Done clearing unread and stars, untimed Delay, combined permitted calendar details and Mail-only timeline remain unchanged. D015 authorizes the local implementation; live account/model management remains later.

## Next work

**P1-09 is active:** D018 adopts the second September 12 UI update, including General, Graphite/Paper/Harbour, text size/startup and timeline placement plus the counted groups, sender-kind filters and exact-address cards. Boundary: Core 13, CLI 7, Mail UI 12, Calendar 11, Search 5, Triage 6, Platform 10, Quality 9; schema 7/fresh fixture 3/additive protocol 1. [GUI07/native05](research/assets/phase-1/p1-09-windows/p1-09-native-05.log) exits 0 through the updated Settings and mail controls, idle convergence after independent CLI changes, sender-card hover/Open/Filter/Write, all four composition modes, source-conflict refusal, calendar edits/RSVP and restart. Four fresh GUI processes verify Mail, Calendar and both last-workspace choices with saved appearance and resulting workspace.

The [complete release01 run](research/assets/phase-1/p1-09-windows/p1-09-full-release-01.log) passes **9/9 in 196.98 seconds**: all 163 nonignored Rust tests and a 54.17-second full GUI/restart walkthrough, including actual query priority, read/write barriers, bounded polling progress and idle CLI convergence. The [dev03 concurrent-ingestion report](research/assets/phase-1/p1-09-windows/p1-09-search-ingestion-dev-03.json) passes rare-subject/body/grouped/combined-filter p95 at **67.7413 / 73.0102 / 98.6387 / 168.3562 ms**, 100 target/reset pairs per class. Independent source/index/overlap checks pass for **5,150 actual arrivals / 206 pages**, with the original seed unchanged. Dev02's 200.8104 ms failure remains preserved.

Four [GUI07 Settings captures](research/assets/phase-1/p1-09-windows/visual-02/manifest.json) and five [final-source Settings captures](research/assets/phase-1/p1-09-windows/visual-03/manifest.json) cover all six sections and corrected spacing. The earlier nine GUI04 captures retain the other Update2 layout/size observations. Finish [P1-09](plans/phase-1.md#p1-09--second-september-12-prototype-update) with final package/relocation, preserved schema 4/5→7 package upgrade/refusal, packaged concurrent-ingestion measurement and final-source Mac/Windows/Ubuntu CI. Schema-6 Core migration coverage remains distinct from a packaged upgrade: no historical schema-6 package exists.

All nine release checks passed across the initial run and a scoped recovery on the source before the new conflict guard. The first run retained six passing GUI checks but failed Rust test relinking because a concurrently orchestrated screenshot held the engine executable; two dependent checks did not run. Once released, the remaining three checks passed in 101.93 seconds. A later review found that Save my version could combine a revision-0 reply's retained authorship with another source selected by a CLI first save. The guard and actual native confirmation regression now pass in run04; the earlier release checks do not cover it. Full failure logs and the separate native shutdown warning remain preserved.

The [P1-07 audit](research/phase-1-execution.md#completion-audit-follow-up--p1-07) now has passing native arrival/count/selection retention and edited-calendar/RSVP restart evidence. The first concurrent-ingestion GUI attempt failed its startup watchdog with zero search samples while 4,350 messages committed; it is a responsiveness failure, not an input-to-paint measurement. Background scheduling diagnosis and a rerun retain the 200 ms target and offered 25 messages every 100 ms. Final-source native CI and packages on Mac, Windows and Ubuntu remain pending. The earlier `738601e` Mac/Ubuntu passes do not cover the new source; Windows previously exhausted its old aggregate Rust-suite watchdog. The corrected allowance and later fixes still need final-source CI. Phase 1 is not complete yet.

Physical mouse/keyboard, clipboard/IME, accessibility and wider display-scaling checks remain explicitly unverified; desktop input automation was paused after Escape. Native programmatic controls, exact CLI readback, actual GUI restart and screenshots establish their recorded scopes only. The initial failed search measurements are retained alongside the passing optimized results.

Before actual Google accounts, implement/test the OS credential-store adapter and settle OAuth access plus real-data retention/encryption/recovery policy. Full history, live sending/calendar operations and founder Google dogfooding remain later milestones. Legacy transports remain deferred.

The home Windows 11 PC is ready for Phase 1 development at planning checkpoint `5bed257` on `codex/phase-1-plan`, with a local Windows tool-lookup correction. Bootstrap/doctor, dev and release builds (8/8 CTests each), relocated package checks and native Direct3D 11 rendering on the RTX 5090 pass. The schema-4 package is preserved locally for Phase 1 migration checks; see [Windows handoff evidence](research/phase-0-execution.md#home-windows-handoff-before-phase-1). Run `scripts/dev.ps1 run` from PowerShell. Do not copy profiles, credentials or SDK/build directories through Git. Manual Windows clipboard/IME/accessibility and wider scaling checks, OS keyrings and physical Linux desktop checks remain unverified.

## Google-first readiness

| Required area | State | Verified |
| --- | --- | --- |
| [Core/offline storage](contracts/core-storage.md) | Schema-7 native draft/restart and Core migration checks pass; packaged upgrade pending; whole area Draft | No |
| [Gmail/Workspace](contracts/gmail.md) | Phase 0 synthetic experiment verified; whole area Draft | No |
| [Mail UI/composition](contracts/mail-ui.md) | P1-09 release/native controls, composition/conflict and restart pass; final package acceptance pending; whole area Draft | No |
| [Search](contracts/search.md) | Native filters/idle convergence and all four concurrent development-binary budgets pass; packaged measurement pending; whole area Draft | No |
| [Triage/categories](contracts/triage.md) | Phase 1 explicit review/Done/Delay verified locally; whole area Draft | No |
| [Google Calendar](contracts/calendar.md) | P1-09 local views/writes, header timeline, duration preference and edited-event/RSVP restart pass; live Google pending; whole area Draft | No |
| [Optional local assistance](contracts/local-ai.md) | Phase 0 synthetic experiment verified; whole area Draft | No |
| [Platform/install/lifecycle](contracts/platform.md) | Windows release tests pass; P1-09 final package/upgrade/CI pending; whole area Draft | No |
| [CLI](contracts/cli.md) | Paired composition/MIME/restart readback verified locally; whole area Draft | No |

The denominator changed from ten to nine under D009: IMAP/SMTP is now a deferred area, and external subscriptions are outside the current Calendar gates. This is a reporting scope change, not progress. Local assistance remains an intended capability, optional to enable. Partial CLI or text-only GUI delivery can satisfy the first usable slice without marking its entire feature Verified. Record completed slice evidence explicitly; docs and screenshots do not count.

The Google-first release still requires its applicable quality and OS gates. It does not require pixel-perfect HTML or later protocol compatibility. First use, fuller Google dogfooding and all-platform release verification are distinct evidence: none proves the others automatically.

## Phase overview

| Phase | Outcome | State |
| --- | --- | --- |
| 0 — Paired foundation | Native builds, small fixtures, storage, private engine/CLI/GUI, text/composer and Google prerequisites | Completed on available hosts; external/physical gates explicit |
| 1 — Redesigned local client | September 12 native design and paired CLI, shared triage/search/preferences, durable new/reply/reply-all/forward drafts/capture and combined calendar workflows | P1-09 native walkthrough passes; concurrent performance and final release/package/upgrade/CI gates pending |
| 2 — Google daily workflows | Gmail/Workspace and Google Calendar, growing CLI with GUI; controlled founder use | Planned |
| 3 — Google completion and dogfooding | Usable Google GUI and CLI, expanded designs, later best-effort HTML/local assistance, recovery/performance and delivery evidence | Planned |
| 4 — Later protocols | IMAP, SMTP and external iCalendar subscriptions, only after actual Google dogfooding | Deferred; not started |
| Later | CalDAV and possible public/open-source launch | Separate scope |

## Evidence and gates

The supplied approximately 25 GB eM Client snapshot is a disk footprint, not a verified message count. Research, published examples and mockups are not benchmark results. No account has been connected during planning.

Before legacy work begins, record which Google GUI/CLI workflows the founder used, issues found and fixes/retests. A proposed two-week trial is an evaluation suggestion, not an accepted fixed duration. Future protocol coverage has its own readiness reporting and cannot retroactively block the earlier Google milestone.

Upcoming inputs include actual build/test hosts, Google OAuth/admin access, retention/encryption/recovery policy before real storage, and later provider/settings interactions. D014 resolves the current read/Done/Delay and calendar/timeline choices. Models and later HTML dependencies have separate gates. The existing exclusions, full offline requirement and explicit GUI/CLI lifetime rules remain unchanged.
