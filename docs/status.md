# Current status

As of 2026-09-11. **Pre-implementation. Google-first MVP readiness: 0% (0 of 9 required areas verified).** Documentation and mockups exist; application code, build configuration, executables, tests, installers and measured results do not.

## Established direction

[D009](decisions.md) supersedes the earlier broad protocol/HTML gates. All current accounts use Google APIs. The first usable milestone includes both a small CLI and a GUI against one engine. Grow their workflows together, and have the founder dogfood usable Google clients before beginning IMAP/SMTP or external iCalendar subscriptions. Text-first reading and safe extraction remain early; best-effort received HTML display is later work.

Done/Delayed are explicit user decisions. Local classification only distinguishes human correspondence, automated senders/lists and user-defined noise, applying configured category/scope tags. It cannot mark mail reviewed or decide action state. Summaries and classification remain optional to enable; manual first use does not wait for models.

The proposed stack remains Rust engine/CLI, C++20/Qt Quick/QML UI, private local IPC and SQLite/FTS5. QML expressions are accepted, with no Node.js or separate JS compilation/bundling pipeline. WebEngine is not a first-build prerequisite. Under D011 the [updated mail/calendar/settings originals](research/design-references.md) now guide the initial phase. Settings is explicitly incomplete: general shell/navigation plumbing only, ignoring the pictured switches. No application code, setup or model/provider connection was added during this reference update.

## Next work

No implementation workstream is active. When authorized, start P0-00 plus the small-fixture part of P0-01, then join P0-03/P0-10 storage/engine/CLI with P0-04's functional text/composer GUI. Exercise private IPC permissions, account scope, concurrency and recovery while the interface is still small. Google OAuth, MIME and Google calendar work proceed in their scoped packets. See the [plan](development-plan.md) and [Phase 0](plans/phase-0.md).

The [proposed first deliverable](plans/phase-0.md#proposed-first-deliverable-a-runnable-desktop-slice-on-mac-and-windows) is a runnable synthetic-mail GUI/CLI slice plus repeatable Mac/Windows development and native CI builds. Under D010, both the work Mac and home Windows PC are development hosts. This request defines scope; implementation and tool installation have not begun.

Read-only inspection of this Apple Silicon Mac found Git, Homebrew Rust/Cargo and Apple compiler/SDK tools; CMake, Ninja, rustup and Qt commands were not on PATH. The earlier Windows observation found Git and not its listed build commands. These are limited observations, not complete installation audits or selected tool versions; see [host evidence and proposed setup](research/linux-and-distribution.md#development-on-the-work-mac-and-home-windows-pc). Windows details and repository CI access remain to inspect. Missing access blocks its evidence row, not unrelated work.

## Google-first readiness

| Required area | State | Verified |
| --- | --- | --- |
| [Core/offline storage](contracts/core-storage.md) | Not started; Draft | No |
| [Gmail/Workspace](contracts/gmail.md) | Not started; Draft | No |
| [Mail UI/composition](contracts/mail-ui.md) | Not started; Draft | No |
| [Search](contracts/search.md) | Not started; Draft | No |
| [Triage/categories](contracts/triage.md) | Not started; Draft | No |
| [Google Calendar](contracts/calendar.md) | Not started; Draft | No |
| [Optional local assistance](contracts/local-ai.md) | Not started; Draft | No |
| [Platform/install/lifecycle](contracts/platform.md) | Not started; Draft | No |
| [CLI](contracts/cli.md) | Not started; Draft | No |

The denominator changed from ten to nine under D009: IMAP/SMTP is now a deferred area, and external subscriptions are outside the current Calendar gates. This is a reporting scope change, not progress. Local assistance remains an intended capability, optional to enable. Partial CLI or text-only GUI delivery can satisfy the first usable slice without marking its entire feature Verified. Record completed slice evidence explicitly; docs and screenshots do not count.

The Google-first release still requires its applicable quality and OS gates. It does not require pixel-perfect HTML or later protocol compatibility. First use, fuller Google dogfooding and all-platform release verification are distinct evidence: none proves the others automatically.

## Phase overview

| Phase | Outcome | State |
| --- | --- | --- |
| 0 — Paired foundation | Native builds, small fixtures, storage, private engine/CLI/GUI, text/composer and Google prerequisites | Prepared; not started |
| 1 — First usable local slice | Both interfaces use real shared state for synthetic mail/calendar, drafts, search and explicit actions | Planned |
| 2 — Google daily workflows | Gmail/Workspace and Google Calendar, growing CLI with GUI; controlled founder use | Planned |
| 3 — Google completion and dogfooding | Usable Google GUI and CLI, expanded designs, later best-effort HTML/local assistance, recovery/performance and delivery evidence | Planned |
| 4 — Later protocols | IMAP, SMTP and external iCalendar subscriptions, only after actual Google dogfooding | Deferred; not started |
| Later | CalDAV and possible public/open-source launch | Separate scope |

## Evidence and gates

The supplied approximately 25 GB eM Client snapshot is a disk footprint, not a verified message count. Research, published examples and mockups are not benchmark results. No account has been connected during planning.

Before legacy work begins, record which Google GUI/CLI workflows the founder used, issues found and fixes/retests. A proposed two-week trial is an evaluation suggestion, not an accepted fixed duration. Future protocol coverage has its own readiness reporting and cannot retroactively block the earlier Google milestone.

Upcoming inputs include actual build/test hosts, Google OAuth/admin access, retention/encryption/recovery policy before real storage, and reviewed UI actions/overlays. Models and later HTML dependencies have separate gates. The existing exclusions, full offline requirement and explicit GUI/CLI lifetime rules remain unchanged.
