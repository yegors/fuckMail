# Phase 1 — the redesigned local mail and calendar client

Prepared 2026-09-11 under [D014](../decisions.md). **Plan ready; implementation has not started.** This phase applies the [Flight Progress v2 prototype](../research/flight-progress-v2.md) to the verified native foundation. [Status](../status.md) owns progress, [contracts](../contracts/README.md) own behavior and [development plan](../development-plan.md) owns the later Google milestones.

## Deliverable

A native app you can use to review synthetic conversations, search your local fixture history, hold/finish work, compose and capture a reply, and view/edit the combined fixture calendar. The layout follows the HTML reference: compact top bar, resizable mail split, inline reply, day/week/month calendar, event popovers, mail-only browsable timeline and useful local settings. The CLI exercises the same operations and data.

Deliver Mac and Windows development packages, Ubuntu CI evidence, repeatable setup/build/test commands, a short walkthrough, reviewed native screen captures and a list of remaining product gaps. No Google credentials are required for this phase. Source changes move through Git; profiles and SDK/build output stay on each machine.

## Starting point and limits

Runtime `204fa62` and documentation `ca5fac4` are the Phase 0 baseline. Rust engine/CLI, C++20/Qt Quick/QML and SQLite/FTS5 already run together. Protocol 1, schema 4 and fixture 1 are implemented; do not increment them in planning or replace their store. [Phase 0 evidence](../research/phase-0-execution.md) records passing native Mac/Windows/Ubuntu tests, packages and the Mac upgrade/refusal check.

| Already reusable | Phase 1 delta |
| --- | --- |
| One writer, private IPC, bounded requests, receipts and client lifetime | Extend the existing versioned boundary; preserve access checks and restart behavior |
| Account-scoped Inbox/Sent/All queries, categories, source FTS and matching-member identities | Native grouped list, filter popover, actual typed filters, conversation totals and matched-member navigation |
| Snapshot read/star/category actions | User-open read action, atomic Done, explicit untimed hold and stable selection |
| Revisioned conversation replies, optional authored HTML/retained attachments and MIME capture in Core | Finished inline editor, signature, attachment selection, discard, conflict feedback and GUI capture |
| Permission-filtered calendar occurrences, week/footer and tested recurrence/layout | Combined day/week/month/agenda/details, mail-only timeline and exposed local mutation commands |
| Settings shell and allowlisted account view state | Typed local preferences and account metadata, truthful diagnostics, shortcuts help |
| Native scripts, CI, integration tests and upgrade harness | Extend meaningful checks for the new flows; package and review on both development OSes |

Current limits matter: conversation hydration caps at 200 members; calendar requests cover up to 42 days/1,000 occurrences/5,000 sources; view state is an allowlist, not arbitrary JSON. Preserve explicit limits and coverage, or change them with tests when a packet needs more. No Phase 1 result establishes million-message/25 GB complete-offline readiness.

## Agreed behavior and scope

The founder confirmed read-on-open; Done clears unread and stars; Delay is an untimed hold. Opening from a deliberate mouse/keyboard action applies review to the successfully loaded conversation snapshot. Prefetch, incidental selection restoration and CLI inspection remain queries. The operation does not sweep in later incoming members. Done applies read+unstar atomically to its snapshot. CLI exposes explicit equivalents; models never invoke these state transitions.

Mail stays account-separated and conversation-only, including Sent. Calendar combines the connected accounts' full permitted details. Keep origin account/calendar visible and apply provider permissions before details, hover, conflicts or CLI output. Mail account selection must not silently filter the combined calendar.

The timeline appears only in Mail and can browse earlier/later days. It starts on today and offers an explicit return. Its date is independent of the full calendar date; “now” only appears where it belongs. Settings and Calendar have no bottom strip. Keep date/time-zone labels clear when browsing away from today.

Phase 1 includes real local preferences: account display name, row density, selected calendars independent of the active mailbox, week start, calendar/timeline visible hours, per-account signature and shortcut reference. Preserve durable drafts and navigation on settings entry/exit. UI settings are typed and validated; account metadata/signatures have their own domain state.

### Boundaries

- Replies to existing conversations are the composer scope: plain text, authored formatting, signatures, retained attachment selection, explicit discard and local MIME capture. New compose, reply-all/forward, recipient/alias editing and arbitrary file import follow later.
- Calendar writes cover nonrecurring create/edit and self RSVP through the existing deterministic provider-fake path. Recurring items are readable; series/occurrence editing beyond this bounded slice is deferred and clearly unavailable. Calendar search and running-app reminders remain later product work.
- Search includes sender name/email and subject/body plus implemented category, status, attachment and date filters. Search uses source text; summaries have no effect on retrieval.
- Local capture is explicitly presented as a development action. It must not pretend mail was delivered. Captured outgoing content uses real operation state and conversation identity.
- Live Google/OAuth/keychain integration, account disconnect/deletion, models/downloads, received HTML display, timed snooze/follow-up and legacy protocols are outside this phase. Omit unavailable controls or explain their state; no simulated success in the app.

## Packet order and parallel work

| Packet | Owner / files | Dependencies | Concrete output |
| --- | --- | --- | --- |
| P1-00 — freeze the next shared boundary | Core integrator with Mail, Calendar, CLI | This plan and D014 | Reviewed command/query/prefs and fixture changes, exact first-slice walkthrough |
| P1-01 — shared state and projections | Core/Search + CLI; `crates/fm-core`, `fm-ipc`, `fm-cli` | P1-00 | Typed preferences, counts/filters, snapshot actions and calendar scope through one engine |
| P1-02 — native shell and local settings | UI/Platform; `gui/Main.qml`, settings/theme/components | P1-00; integrate P1-01 | Shared visual tokens, navigation, split, settings and focus behavior |
| P1-03 — mail workspace and search | Mail UI + Core/CLI consumer | P1-01, shell primitives | New Inbox/Sent/reader/filter interactions with real counts and actions |
| P1-04 — calendar presentation and timeline | Calendar UI/domain consumer | P1-01, shell primitives | Combined views, agenda, details and mail-only browsable timeline |
| P1-05 — human reply and local submission | Mail/composer + Core/CLI | P1-03, draft/prefs boundary | Durable formatted reply/signature/attachments/discard/capture |
| P1-06 — local calendar writes | Calendar/Core + CLI/UI consumer | P1-04, mutation boundary | Nonrecurring create/edit and self RSVP with durable fake acknowledgment |
| P1-07 — integrated delivery | Integrator + all owners | P1-02 through P1-06 | Passing walkthrough, native review, packages and handoff evidence |

After P1-00, Core projections and shell/components can proceed in parallel. Once the consumed boundary exists, mail and calendar presentation can run independently. Composer and calendar mutations then proceed beside each other. Each merge includes a working shared slice; do not wait for a finished CLI or assemble separate GUI-only stores.

One integrator owns migrations, protocol envelopes, common preference types, fixture manifest and `MailBackend` shared wiring. Mail and calendar owners use separate components/presenters; they agree changes to shared files before editing. CLI commands land with the domain operation they expose. Build/tooling work reuses the pinned scripts and CI caches.

## P1-00 — implementation start packet

The reference archive, source audit, user decisions and this backlog are prepared. On implementation start:

1. Read current status, D014, the relevant contract revisions and the actual checkout diff. Use a `codex/phase-1` branch from the committed planning checkpoint; preserve unrelated work.
2. Run `doctor` and the existing fixture app on the active host. Reuse the Phase 0 packages/evidence; establish only the current host baseline needed for the changed slice.
3. Core publishes the minimal typed boundary needed by the next consumer: grouped conversation query/counts/filter parsing, atomic Done/read/hold, local preferences/account metadata, combined calendar selection, draft disposition and calendar mutation requests. Include a bounded capture-state query linked to its conversation, saved signature/identity revision semantics and new-event creation identity/replay. Specify revisions, scope, pagination, errors, pending states and replay behavior. Do not mint every future API.
4. Extend source fixtures under the existing manifest discipline: three Google-shaped accounts, mixed unread/held conversation states, sent and incoming members in one thread, sender email/name collisions, saved replies, restricted calendar fields, overlaps/all-day/month overflow and deterministic dates. Seed the expanded fixture only into fresh profiles; migration must preserve old profiles without silently adding/resetting fixture accounts or authored state. Preserve an old schema-4 package/profile path for migration tests. Bump fixture/schema/protocol versions only when the implemented changes require it.
5. Record a small first acceptance path: restyled shell plus read-on-open/Done through the actual engine, with independent CLI readback and a retained draft. Then expand packet by packet.

**Pass:** consumers agree on implemented fields/error semantics and independent expected fixtures. The first GUI action and CLI readback share Core. **Do not proceed with** arbitrary prototype state blobs, hardcoded totals or a second data model to unblock a screen.

## P1-01 — shared state and projections

Extend the current `queries`, `mutations` and `view_get/view_save` surfaces rather than replacing them. Separate app-wide display preferences, per-account navigation and domain metadata/signatures. Validate values and revisions; no raw browser state dump. Preserve compatible query clients where possible and advertise new capabilities; incompatible clients fail explicitly.

Define account badges as counts of conversations containing unread members. Existing `unread_count`/`starred_count` remain message totals; add explicitly named conversation counts without reinterpreting fields consumed by old clients. Review groups take unread first, then held, then done; read and star remain independent underlying states. Label the held total so a mixed unread+held conversation is not concealed. Counts cover the declared account scope, not just a loaded page or filter; partial source coverage prevents an unqualified all-clear. Include archived mail in discoverable account totals and make its scope visible.

Use stable group/date/identity order and bounded cursors. A read action can move a row between groups without replacing the selected reader or losing its draft. Preserve the active row until navigation if needed, while counts reflect durable state. Cancel or discard stale account/query responses.

Expose combined calendar queries with explicit selected sources and account/calendar/occurrence identities. A denied source is not silently presented as an empty complete calendar. Add the same useful query/preference/action access to the CLI.

**Pass:** GUI/CLI totals, filters, action receipts and calendar selections agree; snapshot actions exclude later members; stale requests cannot change another account or overwrite a newer edit.

## P1-02 — shell, design system and settings

Port the reference into QML primitives: palette, typography, spacing, icons, density, borders, selection, focus, disabled/error and motion tokens. Use Qt rendering and existing asynchronous adapters. Verify a bundled font's license and Unicode fallback before adding it; otherwise retain a suitable native fallback. No remote fonts or export runtime in the app.

Build the top bar, resizable panes, compact navigation, contextual search and settings routing. Mail account tabs retain their context while the calendar remains combined; label its scope accurately instead of copying the prototype's misleading account-only calendar subtitle. Settings supports local names/preferences/signatures/diagnostics and shortcut help. Only implemented, truthful controls are exposed. Account connection/removal and local-model management stay outside this packet.

**Pass:** settings and account/view changes preserve selection, independent scroll positions, draft and focus. Validate at 1440×900 and 1920×1080 logical reference sizes plus a 1160×700 compact case, and Mac/Windows display scaling. These are review targets, not claims that the HTML minimum is an acceptable native clipping rule. If a size cannot fit, adapt layout/overflow and record the chosen native minimum before acceptance.

## P1-03 — mail workspace and search

Implement category rail, stable grouped rows and collapsed conversation reader against the shared models. Render a source preview when assistance is absent. Keep every member accessible within the current bound, show truncation/coverage, and reveal/scroll to the actual earlier search match. Read-on-open must not fire again from background polling.

Implement the filter popover/chips using one shared parser and typed filters; support sender name/address, category, review/held/done, attachment presence and explicit before/after dates. Define inclusive/exclusive date boundaries and the chosen time zone in the contract. Unsupported or invalid constraints must produce feedback. Sent uses conversations with outgoing members and truthful captured/pending states; omit speculative awaiting-reply/follow-up features.

Wire category correction and retained attachments to real actions. Verify availability before export/open; missing/partial data stays unavailable. Implement only useful overflow actions from the scoped contracts. The seven-day volume graphic may ship only with correctly defined received/outgoing message counts and coverage from Core; otherwise omit it from this phase's layout.

**Pass:** sender/body searches find the correct conversation/member; clearing filters restores context; paging and live fixture changes do not jump selection; empty results and incomplete coverage are distinguishable; Done/Delay/category actions survive restart with accurate counts.

## P1-04 — combined calendar and mail timeline

Reuse the tested recurrence/time-zone/permission projection and `CalendarLayout`. Build day/week/month, a reachable agenda/day list, event details, all-day rows, overlaps and overflow access. Keep account/calendar origin visible, including in conflicts. Display only fields actually available to each source; combined viewing does not grant editing rights.

Build timeline duration lanes and event popovers from the same occurrence query. Provide drag plus visible previous/next/today controls and keyboard alternatives. Show a clear date when away from today. Calendar navigation and timeline browsing are independent; mail account switching retains the combined calendar selection. Visible-hour settings must leave early/late/all-day events discoverable.

Use an injectable clock for repeatable fixtures, the real local clock for ordinary running and an explicit view time zone. Test midnight, DST and date navigation; never use the prototype's static 10:42 as app time. A meeting-link action uses permitted stored links and an explicit user action; existing reminder-acknowledgment product semantics remain later until reminders are implemented.

**Pass:** every view, popover, timeline and CLI query agrees on occurrence identity, time and permitted fields. Include month overflow, more overlaps than visible lanes, all-day/multiday events, restricted calendars and a viewed date that excludes today.

## P1-05 — reply and local submission

Keep the inline layout and exact account/From identity. Extend the editor to preserve authored plain text/HTML, a per-account signature and selected retained attachments through the existing draft/capture pipeline. Current capture reads the account's current signature outside the draft revision: add a saved per-draft signature snapshot or explicit signature/identity revision validation before promising exact captured bytes. Prefer snapshotting the signature when applied to the draft. Preserve manual edits when a signature preference changes; do not silently rewrite an existing draft. New-file imports and full recipient/alias management are later work.

Surface saving/saved/conflict/error states. Navigation and close must retain durable work; explicit discard clears all authored text, HTML and attachment selections with revision protection. Keep independent drafts when moving between conversations/accounts. GUI capture uses the same saved revision and MIME builder as the CLI, with a durable operation ID and clear local-only outcome; retrying an uncertain outcome cannot duplicate submission.

Current capture stores an operation/MIME blob but creates no Sent message and exposes no capture-history query. Implement a bounded journal-derived projection linked to the original conversation for GUI/CLI inspection and a clearly labeled local-capture section in Sent. Keep accepted, failed and unknown outcomes distinct; deduplicate by operation ID and preserve the source draft. Do not manufacture provider-sent messages or include unaccepted local captures in delivered-mail statistics.

**Pass:** Unicode, selection, IME, undo, formatting and attachments survive saves/navigation/restart; CLI reads identical draft content and capture bytes; lost acknowledgment and stale revision preserve recoverable authored work. No generated text or received-content editing is introduced.

## P1-06 — local calendar mutation slice

Expose bounded nonrecurring create/edit and self RSVP through the Core journal and deterministic fake acknowledgment. Existing prepare/edit/RSVP and acknowledgment functions require a stored event/ETag; create is new work. Add stable client-created identity, durable prepared intent, target-calendar permission checks, fake acknowledgment/source insertion and idempotent replay before exposing its form. Creating an event needs an explicit writable account/calendar target despite combined viewing. Define fields, allowed operations, validation, revision/ETag checks and recurrence restrictions before adding forms. Reuse normalized event identities; do not mutate displayed occurrence copies as if they were source events.

Add minimal native forms/popovers and CLI operations together. User-visible states distinguish locally saved/pending, fake-acknowledged, failed and stale/conflicting edits. Keep source and occurrence projections synchronized only through committed Core outcomes. Live invitations and Google event writes are Phase 2.

**Pass:** GUI-created/edited fixture event and RSVP read back through CLI and all calendar views; replay/restart does not duplicate creation; denied calendars, revoked permission and stale ETags fail without losing the user's editable content. Recurring-edit controls remain unavailable with an explanation.

## P1-07 — acceptance and packages

Use the existing tests and add scenarios that validate new behavior, not copies of implementation. Record source/contract/fixture versions, host, commands and observed results. [Quality](../contracts/quality.md) owns performance budgets; the old small Phase 0 measurements do not satisfy the full product workload.

1. Launch a disposable profile; verify three separate mail accounts and combined calendar identity.
2. Open unread mail, verify read-on-open via CLI, hold it, finish it, and introduce a new member during a snapshot action. Verify counts/selection and the new member's state.
3. Search by sender email/name and subject/body; apply real filters, page results, open an earlier match and switch accounts during a delayed response.
4. Save a formatted Unicode reply with signature/retained attachment; navigate Mail/Calendar/Settings/accounts, restart, inspect via CLI, capture it and inspect the outgoing conversation. Exercise explicit discard and lost capture acknowledgment.
5. Browse every calendar view and timeline date; inspect all-day/overlap/restricted content, then create/edit a nonrecurring event and RSVP. Compare GUI and CLI before/after restart and stale mutation failure.
6. Change local preferences/display name; verify validation and persistence. Confirm no model/network setup is needed and no fake sync/delivery status appears.
7. Close GUI with and without an independent CLI owner; verify the established engine lifetime. Upgrade a preserved schema-4 synthetic package/profile to Phase 1, retaining drafts, receipts and content; test safe old-client/engine refusal where incompatible. Adapt `verify_upgrade.py` to the actual same-schema/migration and compatible/incompatible protocol cases. Its current strictly increased-schema and exact-JSON assumptions cannot cover additive output fields unchanged. Compare stable semantic fields and exact old receipts/content, with explicit expected additions; never weaken data-preservation assertions to make a new fixture pass.
8. Run appropriate native tests and relocated package checks on Mac, Windows and Ubuntu CI. Retain packages/logs and manually review actual Mac/Windows mouse, keyboard, clipboard/IME and scaling on available physical hosts; explicitly record missing physical evidence.

For search/interaction changes, extend the existing 100k varied fixture measurement to the actual first-page/filter/grouping/paint boundary, with concurrent fixture ingestion and at least 100 observations where practical. Report failures against the quality contract; do not loosen a budget silently. Million-message and large offline-retention release tiers remain later work.

**Exit:** all included local workflows are usable through the native GUI and relevant CLI, the reference visual review passes or differences are explicitly recorded, native automated/package checks pass, and remaining physical/live-provider/full-product gates are visible. This completes the local Phase 1 slice; whole Google-first readiness is evaluated separately.

## Mac/Windows handoff and work readiness

Use the existing [development guide](../build-development.md) and pinned toolchain. On Mac: `scripts/dev.sh doctor`, `test`, `run`, `package`. On Windows: `scripts/dev.ps1 bootstrap` once, then `doctor`, `test`, `run`, `package`. Do not install Node or copy Qt SDKs/caches between machines. Native CI builds the same source and produces each host's development package.

Before switching machines, checkpoint source, docs, lockfiles and source fixtures on the same branch; record the source revision and the last tested host/commands. Pull that revision on the destination, generate disposable fixtures locally, and verify its own setup. Keep real mailbox data, credentials, drafts, `.tools`, `target`, `build` and `dist` out of Git. Cached tools accelerate runs; they do not replace native verification.

No further product decision blocks starting P1-00/P1-01/P1-02. The founder can review the running visual slices as they land. Small missing forms use the accepted style and are reviewed in their packet. Google project/admin access and real-data retention/encryption/recovery choices are needed before Phase 2, not before this synthetic work.
