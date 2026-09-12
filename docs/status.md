# Current status

As of 2026-09-11. **Phase 0 completed on available hosts; Phase 1 plan ready, implementation not started. Google-first MVP readiness remains 0/9 whole areas verified.** Runtime checkpoint `204fa62` is pushed on `codex/phase-0`. Native macOS 15, Windows Server 2022 and Ubuntu 24.04 CI each passed all 8 CTests, relocated packaged GUI/CLI/runtime checks and artifact retention. The Mac native desktop walkthrough and actual schema 2→4 packaged upgrade/downgrade-refusal checks also pass. See [Phase 0 execution evidence](research/phase-0-execution.md) for exact sources, commands, artifacts and limits. No live Google account or model is connected.

## Established direction

[D009](decisions.md) supersedes the earlier broad protocol/HTML gates. All current accounts use Google APIs. The first usable milestone includes both a small CLI and a GUI against one engine. Grow their workflows together, and have the founder dogfood usable Google clients before beginning IMAP/SMTP or external iCalendar subscriptions. Text-first reading and safe extraction remain early; best-effort received HTML display is later work.

Done/Delayed are explicit user decisions. Local classification only distinguishes human correspondence, automated senders/lists and user-defined noise, applying configured category/scope tags. It cannot mark mail reviewed or decide action state. Summaries and classification remain optional to enable; manual first use does not wait for models.

The proposed stack remains Rust engine/CLI, C++20/Qt Quick/QML UI, private local IPC and SQLite/FTS5. QML expressions are accepted, with no Node.js or separate JS compilation/bundling pipeline. WebEngine is not a first-build prerequisite. Under D014 the [Flight Progress v2 HTML prototype](research/flight-progress-v2.md) guides Phase 1. The founder confirmed read-on-open, Done clearing unread and stars, untimed Delay, combined full permitted calendar details, and a browsable timeline only in Mail. The expanded settings design now supports planned local preferences; live account/model management remains later. This update prepares documentation/assets only; no Phase 1 runtime work has started.

## Next work

Start from the prepared [Phase 1 packets](plans/phase-1.md). P1-00 defines the next typed boundaries/fixtures; shared Core/CLI state and native shell can then proceed in parallel, followed by independent mail and calendar work. Deliver the updated native design, real search/triage/local preferences, combined calendar views and mail timeline, durable formatted replies/local capture and bounded calendar create/edit/RSVP. The [existing fixture app and CLI](build-development.md) remain runnable; Phase 1 implementation has not started. No further product decision blocks the first packets.

Before actual Google accounts, implement/test the OS credential-store adapter and settle OAuth access plus real-data retention/encryption/recovery policy. Full history, live sending/calendar operations and founder Google dogfooding remain later milestones. Legacy transports remain deferred.

On the home Windows PC, pull `codex/phase-0`, run `scripts/dev.ps1 bootstrap`, then `doctor`, `test` and `run`. This is source/tooling handoff; do not copy profiles, credentials or SDK/build directories between machines through Git. Physical Windows and Linux desktop/input/keyring checks remain unverified; CI does not replace them.

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
| 1 — Redesigned local client | Updated native design and paired CLI, shared triage/search/preferences, replies/capture and combined calendar workflows | Plan and reference handoff ready; runtime not started |
| 2 — Google daily workflows | Gmail/Workspace and Google Calendar, growing CLI with GUI; controlled founder use | Planned |
| 3 — Google completion and dogfooding | Usable Google GUI and CLI, expanded designs, later best-effort HTML/local assistance, recovery/performance and delivery evidence | Planned |
| 4 — Later protocols | IMAP, SMTP and external iCalendar subscriptions, only after actual Google dogfooding | Deferred; not started |
| Later | CalDAV and possible public/open-source launch | Separate scope |

## Evidence and gates

The supplied approximately 25 GB eM Client snapshot is a disk footprint, not a verified message count. Research, published examples and mockups are not benchmark results. No account has been connected during planning.

Before legacy work begins, record which Google GUI/CLI workflows the founder used, issues found and fixes/retests. A proposed two-week trial is an evaluation suggestion, not an accepted fixed duration. Future protocol coverage has its own readiness reporting and cannot retroactively block the earlier Google milestone.

Upcoming inputs include actual build/test hosts, Google OAuth/admin access, retention/encryption/recovery policy before real storage, and later provider/settings interactions. D014 resolves the current read/Done/Delay and calendar/timeline choices. Models and later HTML dependencies have separate gates. The existing exclusions, full offline requirement and explicit GUI/CLI lifetime rules remain unchanged.
