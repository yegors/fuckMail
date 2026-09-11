# Current status

As of 2026-09-11. **Pre-implementation. MVP readiness: 0% (0 of 10 required feature areas verified).** This repository contains documentation only: no application code, build configuration, executable, automated test harness or measured application results.

## Built and established

The product direction and scope are documented. Existing provider/toolkit research, the exec-buddy source review, triage/local-assistance behavior and proposed performance gates have been organized by scope. Research and contract indexes, ownership boundaries and an append-only decision log are in place. This is planning progress; it does not count as a built application feature.

## Building now and next

The founder is collaborating with Claude Design using [ui-ux.md](ui-ux.md). The brief records deferred-action stars, top mailbox selection, a resizable mail split and today's calendar strip on every mail screen, while retaining the full calendar. Three nonfinal [mockups](research/design-references.md) inform the Qt Quick UI direction; the subsequent core-language review recommends a [Rust engine/CLI with a C++20/Qt Quick UI](research/desktop-stack.md), joined through the proposed private local IPC. Styling and detailed layout remain open. The review revises the implementation recommendation, not an accepted stack decision. Linux (Ubuntu, Arch/Omarchy), a full CLI and smooth installation are required. No implementation workstream is active or stack version locked. Next: test the reader/composer, headless engine/CLI, required desktops and clean installation, then agree the shared types/fixtures. Final visual design need not block those feasibility slices.

Then build a small local end-to-end slice and integrate the provider, search, mail UI, calendar and optional-assistance workstreams through their contracts. Google Calendar and IMAP/SMTP feasibility start early; a Gmail demo alone is not the MVP.

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

## Phases and exit gates

| Phase | Outcome | Exit gate |
| --- | --- | --- |
| 0 — Design, feasibility and contracts | Use the nonfinal design references to validate the recommended stack/reader/composer, headless engine/CLI, storage/providers and clean installation on the required platforms | Recorded stack/version decision and Windows/macOS/Linux evidence; agreed engine/CLI/triage/provider/inference contracts and runner/platform matrix |
| 1 — Local foundation | Shared engine and CLI, storage/migrations, durable operations, conversation/search/triage models, category mappings, view state and provider/model fakes | Browse/search during ingestion; no false all-clear from filtering; restart preserves drafts, manual overrides and operations; ending the owning session stops its jobs; the engine exits after the final client |
| 2 — Google daily workflows | Multi-account Gmail/Google Calendar, manual label-based triage, human composition, full offline pipeline, optional local suggestions/summaries and configured category auto-apply | Personal/Workspace workflows work with assistance off; source-based search remains responsive; labels/read/stars reconcile across clients; model output cannot write email or clear triage state |
| 3 — Protocol and calendar completeness | IMAP/SMTP and category-folder moves, read-only feeds, all calendar views/search/recurrence/invitations, current/next status and Join/reminders; both local runner adapters and full applicable CLI workflow coverage | Required interoperability passes, including partial moves and manual corrections during inference; feeds are resilient; attachment retention complete; runner failures do not block manual use |
| 4 — Daily-driver MVP | Integration tuning, recovery, packaging, optional-inference load tests and real use on Windows/macOS/Linux, with prebuilt GUI/CLI delivery | Search/resource budgets met or renegotiated; proposed two-week trial exercises daily zero-unread/zero-starred workflow with honest counts; no hidden work after all explicit sessions end |
| Later | Editable CalDAV; possible open-source/public launch | Separate scope and compatibility decisions; no effect on MVP completion |

Phases describe integration gates, not a requirement to implement every module serially. Calendar and IMAP/SMTP prototypes begin early. Google Calendar correctness is a central goal and must not be postponed until after the mail UI is polished. A Google-only or read-only mail demo is not the complete MVP.

## Not building

Excluded product areas: tasks, a contacts application, widget dashboards, unified mailboxes, individual-message browsing mode, broad integrations/assistant chat and app-operated remote processing. Built-in AI writing, rewriting, autocomplete, sending, cloud fallback, task generation, meeting briefings and embeddings/semantic search are excluded. External agent flows use the CLI under caller control; explicit CLI sessions can run with the GUI closed. No hidden GUI work/reminders continue after close. Separate OS-specific visual designs and an npm/Node application pipeline are not planned.

Deferred beyond MVP: editable CalDAV and broad open-source/public-launch/store work. Linux, installers, CLI discovery and a maintained update/recovery route are in MVP. Further proposed deferrals and unaccepted integrations are listed in [product scope](product-scope.md); they are not hidden MVP work.

## Evidence and outstanding gates

No mailbox was accessed during planning; exec-buddy was inspected as source only. The supplied approximately 25 GB eM Client disk snapshot is not a verified message/account inventory. Slint screenshots are published examples, and every performance/model-quality number is a proposed target, not a result.

Current gates are stack/renderer/composer feasibility against the supplied references, headless CLI/engine behavior, actual devices/provider inventory, Ubuntu/Arch/Omarchy desktop compatibility, clean installation and the shared type/fixture baseline. Track evidence in the relevant scoped research file and [quality](contracts/quality.md), update this page as work changes, and append decisions to [the log](decisions.md).
