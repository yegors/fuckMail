# Current status

As of 2026-09-11. **Pre-implementation. MVP readiness: 0% (0 of 10 required feature areas verified).** This repository contains documentation only: no application code, build configuration, executable, automated test harness or measured application results.

## Built and established

The product direction and scope are documented. Existing provider/toolkit research, the exec-buddy source review, triage/local-assistance behavior and proposed performance gates have been organized by scope. Research and contract indexes, ownership boundaries and an append-only decision log are in place. This is planning progress; it does not count as a built application feature.

## Building now and next

**Planning complete for the next execution stage; implementation not started.** The closer mail/calendar [mockups](research/design-references.md) are documented alongside three earlier references. Final mockups will arrive closer to production UI implementation. [UI/UX](ui-ux.md) preserves open action/state/overlay decisions rather than adopting them from sample screens.

The [development plan](development-plan.md) covers all phases; [Phase 0](plans/phase-0.md) specifies immediate packets, owners, dependencies, evidence and pivot triggers. Start P0-00 plus the small-fixture portion of P0-01 when asked to implement: inspect tools/test-host access, establish reproducible builds and shared synthetic inputs. Then prove private engine/CLI/UI coordination, durable storage and reader/composer feasibility while Google/protocol/calendar investigations proceed.

The stack remains a [proposal](research/desktop-stack.md): Rust engine/CLI, C++20/Qt Quick UI, SQLite/FTS5 and private local IPC. No versions are locked, no feature workstream is active and all area contracts remain Draft. Scoped readiness is recorded only after the consumed interface/fixture choices are resolved. Planning readiness does not mean the build environment is installed or the product can run.

Current environment evidence is only a Windows PowerShell PATH lookup finding Git and not the listed Rust/Qt/build commands; existing installations outside that PATH were not audited. Other test hosts, Google project/admin access, actual non-Google providers, signing identities and local runners are unverified dependencies. Their missing evidence blocks the relevant gate, not independent work. See the plan's just-in-time decision list.

## Feature readiness

| Required feature area | Build state | Verified for daily use |
| --- | --- | --- |
| [Core and offline storage](contracts/core-storage.md) | Not started; contract draft | No |
| [Gmail](contracts/gmail.md) | Not started; contract draft | No |
| [IMAP/SMTP](contracts/imap-smtp.md) | Not started; contract draft | No |
| [Mail UI and human composition](contracts/mail-ui.md) | Not started; contract draft | No |
| [Local search](contracts/search.md) | Not started; contract draft | No |
| [Unread/star triage and categories](contracts/triage.md) | Not started; contract draft | No |
| [Calendar and read-only feeds](contracts/calendar.md) | Not started; contract draft | No |
| [Optional local classification/summaries](contracts/local-ai.md) | Not started; contract draft | No |
| [Windows/macOS/Linux installation and lifecycle](contracts/platform.md) | Not started; contract draft | No |
| [Full CLI and external agent interface](contracts/cli.md) | Not started; contract draft | No |

The headline percentage counts fully verified required areas divided by these ten areas. Partial implementation is described in the table but does not count as a delivered area until its contract and applicable [quality/release gates](contracts/quality.md) pass. It is a scope-completion measure, not an estimate of effort or time remaining. Documentation, screenshots and unrun tests do not count. All required OS/distro and GUI/CLI gates must pass; optional enabling of AI does not remove its implemented-capability gate. The denominator changed from 9 to 10 on 2026-09-11 when the full CLI became a separate required area; Linux and smooth installation expand the existing Platform area. If scope changes again, append a decision and identify the revised denominator here.

## Phase overview

Detailed sequencing lives in the [development plan](development-plan.md); avoid maintaining a second task list here.

| Phase | Intended outcome | Current state |
| --- | --- | --- |
| 0 — Foundation feasibility | Build/fixtures, engine/CLI/storage, Qt reader/composer, early provider/calendar and installation evidence | Detailed packets prepared; unstarted |
| 1 — Local product slice | Shared synthetic mail/calendar workflows, human drafts, triage and search through both interfaces | Outlined; expand after initial evidence |
| 2 — Google daily workflows | Consumer Gmail/Workspace mail and calendar with reliable setup, sync and full-history acquisition | Outcome-level plan |
| 3 — Required completeness | IMAP/SMTP, complete calendar/feeds, optional local assistance and full applicable CLI/GUI coverage | Outcome-level plan |
| 4 — Daily-driver release | Performance/recovery, final design, trusted simple installation and founder trial | Outcome-level plan |
| Later | Editable CalDAV and possible public/open-source launch | Separate future scope |

Calendar and IMAP/SMTP feasibility begin in Phase 0; installation and CLI are developed alongside the product. Later phase detail is intentionally provisional. A local fixture slice or Google-only demo does not satisfy MVP.

## Not building

Excluded product areas: tasks, a contacts application, widget dashboards, unified mailboxes, individual-message browsing mode, broad integrations/assistant chat and app-operated remote processing. Built-in AI writing, rewriting, autocomplete, sending, cloud fallback, task generation, meeting briefings and embeddings/semantic search are excluded. External agent flows use the CLI under caller control; explicit CLI sessions can run with the GUI closed. No hidden GUI work/reminders continue after close. Separate OS-specific visual designs and an npm/Node application pipeline are not planned.

Deferred beyond MVP: editable CalDAV and broad open-source/public-launch/store work. Linux, installers, CLI discovery and a maintained update/recovery route are in MVP. Further proposed deferrals and unaccepted integrations are listed in [product scope](product-scope.md); they are not hidden MVP work.

## Evidence and outstanding gates

No mailbox was accessed during planning; exec-buddy was inspected as source only. The supplied approximately 25 GB eM Client disk snapshot is not a verified message/account inventory. Slint screenshots are published examples, and every performance/model-quality number is a proposed target, not a result.

Current gates are reproducible build/shared fixtures, stack/renderer/composer feasibility against the supplied references, headless CLI/engine behavior, actual devices/provider inventory, Ubuntu/Arch/Omarchy desktop compatibility, clean installation and the shared type/fixture baseline. Track evidence in the relevant scoped research file and [quality](contracts/quality.md), update this page as work changes, and append decisions to [the log](decisions.md).
