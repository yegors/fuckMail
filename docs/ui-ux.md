# Visual design and interaction brief

**D014 update, 2026-09-11:** [Flight Progress v2](research/flight-progress-v2.md) is the current Phase 1 design reference. The founder confirmed read-on-open, Done clearing unread and stars, untimed Delay, a combined calendar with full permitted details, and a browsable timeline only in Mail. Mail accounts remain separate. Phase 1 implements local synthetic workflows in the native app and CLI; live Google, models, legacy protocols and received HTML rendering remain later work.

Updated 2026-09-11. Working brief for the founder's collaboration with Claude Design. The HTML prototype supersedes the earlier trio's visual direction; preserve all earlier references as history. Confirmed choices, proposed implementation details and sample prototype behavior remain distinct. No full-product design or stack freeze is implied.

## Supplied visual references

Use the archived [Flight Progress v2 HTML](<research/assets/2026-09-11/flight-progress-v2/Flight Progress v2.dc.html>) and [support.js](research/assets/2026-09-11/flight-progress-v2/support.js) with the [scoped reference audit](research/flight-progress-v2.md). They are design reference data, not application runtime or build instructions. The initial [mail](research/assets/2026-09-11/initial-mail.png), [calendar](research/assets/2026-09-11/initial-calendar.png) and [settings](research/assets/2026-09-11/initial-settings.png) trio, [closer mail](research/assets/2026-09-11/closer-mail.png) and [closer calendar](research/assets/2026-09-11/closer-calendar.png) remain historical in [reference notes](research/design-references.md).

The earlier mood/layout references remain useful history:

- [Mockup 1](research/assets/2026-09-11/mockup-01.png): dark, restrained interface with strong headings, mono metadata and a multi-calendar bottom timeline.
- [Mockup 2](research/assets/2026-09-11/mockup-02.png): serif-led treatment, small caps and red marks emphasizing deferred mail.
- [Mockup 3](research/assets/2026-09-11/mockup-03.png): a denser sidebar/list/reader with visible counts and labels.

The earlier sidebar agenda does not replace the confirmed strip under Mail. D014 changes the prior trio's footer treatment: Calendar and Settings now use their full height without a bottom strip. Counts, dates, throughput and sync/model status in the prototype are sample content. Validate text selection, long subjects, contrast and scaling with real fixtures. Do not force received HTML into the design's typography at the expense of readable content.

A CLI and smooth installation are required alongside these screens; begin with a useful partial CLI and expand both clients together. The CLI exposes the same product behavior without a window; it does not need a visual copy of every screen. The [stack recommendation](research/desktop-stack.md) can be tested while detailed visual design continues.

## Current visual direction

Use Flight Progress v2 as the Phase 1 composition, with these implementation boundaries:

| Region | Direction in the initial-phase mockups | Behavior still to settle |
| --- | --- | --- |
| Shared top bar | Mail/Calendar switch, separate account tabs with compact counts, search, date/time and sync status | Label count units and coverage; account changes and calendar search must have explicit scope |
| Mail list | Left conversation pane; compact subject-first rows with sender/member count, time, selection and unread dots; review/delayed/done groups and Inbox/Sent scopes | Proposed unread-first grouping preserves the hold marker on mixed conversations; Sent remains conversations; labels identify conversation versus message counts |
| Reader/composer | Right pane with category chips, Reply/Delay/Done, heading, earlier members, offline attachment and inline reply with To/From/subject | Read-on-open and Done use shared member snapshots; drafts survive navigation/restart; synthetic submission has real recovery states; model summaries remain off for normal Phase 1 use |
| Calendar | Day/week/month grids, day agenda, navigation, event detail popover and current-time line | Combine connected accounts' permitted details, honor restricted fields everywhere, add required all-day/overlap and local editing flows |
| Settings | Left page navigation and Done/Esc exit; local display names/signatures, density, hours, week start and selected-calendar visibility | Persist actual local state; calendar selection is independent of the mailbox; real OAuth/removal and model installation are later; omit timed Delay controls |
| Mail timeline | Duration blocks, overlap lanes, current/next/past accents, adjacent-day browsing and Back to today; only below Mail | Bound dense-day height and overflow, retain event identity/date/time-zone context, and expose event details |

Use the prototype's restrained dark canvas, Geist sans-serif typography, thin borders and sparse amber time accents as the native visual target. Its principal tokens are canvas `#0E0F11`, panels `#131417`, selected rows `#191A1E`, text `#E9EAEC`, borders `#232529`/`#2A2C31`, amber `#C9A36B`, a 56 px header and 52/44 px row densities. The source's 1160 px minimum width is a reference size, not a verified native limit. Package any selected font locally with its license; do not depend on the prototype's CDN. Quiet styling still needs legible secondary text, visible focus and state markers that work without color alone. Test normal/scaled windows on both development hosts.

The two-pane layout pictures Inbox/Sent, search filters, category chips, inline reply and an account-settings entry. The [Phase 1 plan](plans/phase-1.md) builds replies to existing conversations with authored plain text/formatting, signatures, retained attachment selection, durable drafts, explicit discard and synthetic submission capture. New composition, reply-all/forward, recipient/alias editing and new-file import follow later. The prototype's temporary draft and sending records do not define recovery. Live sending is later.

## What the interface should make real

Unread mail is unreviewed attention. Starring an email means leaving an action for later: the work is still outstanding, even if it has begun. Read mail is not necessarily resolved mail. The desired end-of-day state is zero unread and zero starred messages through actual review and completion.

Time is precious. The calendar timeline stays visible while working through mail, starting on today and allowing deliberate browsing of another day. A dedicated calendar remains part of the product.

Use one opinionated design across Windows, macOS and Linux (Ubuntu, Arch/Omarchy), shaped around the founder's workflow. Prioritize mouse interaction, readable content, clear state and responsive feedback. Platform input, accessibility and scaling still need to work; visual styling is shared.

## Confirmed layout

| Element | Requirement |
| --- | --- |
| Mailbox selection | At the top of the application, with the active mailbox clear |
| Main mail workspace | A resizable split screen for conversation browsing and reading |
| Mail calendar timeline | A strip fixed at the bottom of every mail screen, initially today with adjacent-day browsing and Back to today |
| Dedicated calendar | A normal calendar workspace with day/week/month/agenda views |
| Mail state | Make unread attention and deferred starred action understandable through the UI |

For this brief, “mailbox” means a connected account (personal/work), consistent with separate accounts and no unified inbox. Folders, labels and categories remain scopes within that account. Use the reference's top account tabs; adapt overflow for additional accounts without changing that scope.

The current reference keeps the conversation list on the left and reader on the right, with the reply composer inline beneath received content/attachments. Retain a resizable divider and remembered position; validate minimum pane sizes and compact-window adaptation rather than hardcoding the reference window size.

```text
+-----------------------------------------------------------+
| Mailbox selection at the top                              |
+--------------------------+--------------------------------+
| Mail scopes / search     | Selected conversation          |
| Conversation list        | Reader / human composer        |
|                          |                                |
|             draggable split divider                       |
+--------------------------+--------------------------------+
| Browsable calendar timeline                               |
+-----------------------------------------------------------+
```

This sketch identifies layout regions. Detailed controls and their placement belong in the design iterations.

## Mail workspace

Keep the selected account and its unread/starred totals visible while browsing Inbox, Unread, Starred, folders/categories and search results. Use conversations as the only list unit; show the relevant message within the reader. Folder/category/sender filters should be identifiable and removable.

Proposed resize behavior: a discoverable drag handle, independent pane scrolling, usable minimum pane sizes and remembered split position. Resizing must preserve the selected conversation, reading position, draft and bottom calendar strip. Explore compact widths without squeezing the reader into unusable space or removing the strip.

Restore each account's selected conversation/member, filters and reading position when returning from another account or the full calendar. Switching accounts must not silently retarget an existing draft's From address. Readable text, safe extraction for HTML-only messages, attachments and restrained human composition are the early requirements. Best-effort HTML display is a later enhancement, with sources preserved throughout. Optional local summaries belong with their source conversation; the composer stays human-authored.

## Make read and star states legible

D014 confirms **Delay** as an untimed star hold and **Done** as clearing unread and stars on the targeted existing members. Done does not archive, and neither command changes future replies. Omit the HTML's timed Delay preference and follow-up schedule. Read-on-open is also confirmed; its proposed trigger is a successfully loaded user-activated conversation, using the shared read command and that loaded member snapshot. Prefetch, background refresh, automatic selection/restore, summaries and CLI read queries do not trigger it. No separate synchronized Done state is needed. Read and unstarred is not proof that work outside the app was completed.

Proposed Phase 1 group precedence is To review when any member is unread, otherwise Delayed when any member is starred, otherwise Done. A mixed conversation appears once and retains both the unread and hold facts. Keep the open conversation in place while read-on-open changes its group. Account-tab badges and list/group totals count conversations and label that unit in visible detail/tooltips; account unread/starred totals count underlying messages. Filter totals never replace the account totals, and incomplete coverage or pending changes qualify any all-clear. [Triage](contracts/triage.md) owns these proposed count and command details.

| State or action | Meaning to communicate | Proposed expression |
| --- | --- | --- |
| Unread | “I have not reviewed this” | Clear row marker plus readable emphasis; an Unread view and persistent account total |
| Read | “Opened or explicitly marked reviewed” | Remove unread emphasis while retaining any outstanding star |
| Star | “I am leaving this action for later” | Keep the familiar star; explain its meaning in tooltip/action wording |
| Starred | “Action is still waiting” | A Starred view with a short “Action deferred” description and visible hold age |
| Remove star | “Clear this action hold” | Explicit unstar after completion; sending a reply does not clear it automatically |
| Done | “Clear unread attention and held action” | Explicitly clear unread and stars for the existing targeted members; keep later replies untouched |
| Read and unstarred | “No unread attention or held action on this message” | Calm normal state; no claim that the whole mailbox is complete |

Use **Delay / Undelay** and **Done** from the current reference, with tooltips that explain the actual flag effects and target. Preserve native star/read state; this does not introduce a task, due date, reminder or scheduled snooze.

Make old holds easy to notice and revisit. Show elapsed age when known and explore oldest-first sorting. A star imported from another client may only have a first-observed date; label that honestly rather than inventing when the user deferred the action. Use age and counts to reveal unfinished work without fabricated urgency, punitive scores or repeated nags.

Unread and starred are independent: starring unread mail does not mark it read; reading starred mail does not remove its hold. A conversation can be in both views. The design must explain when a command targets one message versus existing members of the conversation.

Counts stay honest across filters, archive and folder moves. Show incomplete sync and pending/failed changes without claiming the day is clear. Let the user finish reading an open conversation after it leaves Unread. The proposed read-on-open snapshot includes existing collapsed members, while every member remains accessible and a search match is revealed. This is the chosen open/read convention, not evidence that every hidden member was inspected. Later replies remain unread, and summaries never trigger the command.

## Mail calendar timeline

Keep the strip attached to the bottom of the mail workspace across Inbox, Unread, Starred, folder/category views, search, conversation reading and composition. It does not scroll away with message content or disappear during split resizing. Its presence is confirmed; it is not an optional dashboard widget.

Start on today with a current-time marker, duration blocks, overlap lanes and distinct current/next/past events. Allow adjacent-day browsing by drag plus an accessible navigation route, with the displayed date and Back to today made clear. The current-time marker belongs only to today. Useful gaps refer to the displayed calendars and their coverage; they do not establish universal availability.

Design for overlapping meetings, all-day events and busy days. Preserve calendar/account identity, expose overflow counts and keep an event-details route. A quiet empty state must distinguish “no events in the shown calendars” from calendars that have not loaded or are stale. Show enough date/time-zone context to make the day understandable.

Provide a clear route to the full calendar and an explicit meeting-link action when a permitted conference URL exists. It does not change RSVP or claim attendance. Keep physical location separate from the conference link. Phase 1 does not implement reminders or their acknowledgements. When reminders arrive in the later phase, Join will acknowledge that occurrence's reminder only, under the running-app alert preferences.

The founder chose a combined calendar with full permitted details from connected accounts; mail stays separate. The timeline and full Calendar share an explicit selected-calendar visibility preference and clear account identity in details. Calendar selection is independent of the selected mailbox: switching mail accounts does not change the displayed calendar sources. Provider permissions still govern every supplied field, including labels, tooltips, conflict text, search and CLI output. An account with free/busy access supplies Busy information only; combining calendars grants no additional access. Hiding a selected calendar is a display preference, not removal or a sync change.

Persist timeline hours separately from the full calendar's grid hours. An event outside the chosen display span still exists and needs an overflow/access route. Bound lane growth on dense days so the timeline does not consume the reader. Exact height/overflow and compact-window treatment need native review. Any optional time/meeting statistics require defined units and coverage; sample values are not product state.

## Dedicated calendar

Keep day/week/month/agenda navigation, search, calendar visibility and event details/editing available in a dedicated workspace. Include Google recurring-event edits, invitations/RSVP and shared-calendar permissions; external read-only subscriptions are later work after Google dogfooding. The bottom strip and full calendar must represent the same events and occurrence state.

Use the same typography, event identity and interaction language as the Mail timeline. Returning to Mail restores its reading position, draft and timeline date. The full Calendar and Settings have no footer under D014. Calendar date navigation and Mail timeline navigation retain their own explicit dates.

The current reference uses understated event blocks, account details and a current-day/current-time accent across day/week/month. It does not define create/reschedule or recurrence-edit gestures. Its day agenda supplements the required agenda/search workflows. Calendar search must search events within its permitted selected calendar scope; the pictured mail search placeholder is not calendar search behavior.

## Phase 1 settings

The expanded HTML replaces the earlier incomplete screen. Implement its page layout and real local account display names/signatures, row density, grid hours, timeline hours, week start and selected-calendar visibility. Adapt the prototype's mailbox-relative Other accounts switch into explicit calendar source selection that does not change when switching mail accounts. Persist local settings and account metadata through the shared Core/CLI boundary so restart and the other client see the same values. Enter/leave Settings without losing account, view, selection or draft; the timeline appears again when returning to Mail.

Omit timed Delay and timed Follow up controls. Account OAuth, real removal/cache deletion and local-model download/setup remain later work; show truthful unavailable state if those future pages are retained. The settings Done/Esc exit is distinct from mail's action-state Done. Show actual local version, freshness, indexing and offline coverage rather than sample percentages, throughput or simulated sign-in success. No additional JS runtime or general settings framework is required by the exported prototype.

## States to include in design iterations

- Normal mail: several accounts at the top, a populated split view and the calendar timeline on today or an explicitly browsed date.
- Outstanding attention: unread-only, starred-only and combined states; a recent hold and an older hold with an honest age.
- Filtered mail: sender/category chips, search matching a particular message, and archived unread mail still reflected in totals.
- Human composition: a reply with a signature and attachments while the calendar remains visible.
- Time: an empty day, overlapping meetings, a dense day, all-day overflow and an available Join action.
- Window changes: resized panes and compact layouts that preserve reading and the bottom strip.
- Reliability: loading/offline/stale calendars, incomplete mail coverage, pending/failed actions and a truthful zero-work state.
- Optional assistance: absent/disabled, category suggestion and generated summary with source access; normal reading/composition still works.

These are review scenarios for the existing product, not additional app areas. Visual direction, colors, typography, density, final copy, selector treatment, pane orientation and compact behavior should emerge from the founder/Claude design iterations. Use the reader/composer and calendar slice to validate the recommended stack on all required desktops; final styling can continue evolving.

## Handoff before UI implementation

The Phase 1 design and the key action/calendar choices are supplied. Use the [Phase 1 plan](plans/phase-1.md) for bounded native implementation and acceptance. Review missing editor and error surfaces as their packets approach; real-provider and model setup remain outside this local phase.

The focused handoff should resolve these gaps for the slice being built, rather than freezing every later screen now:

- **Mail:** account and folder/category navigation, reply/draft entry points, real search results, labeled counts and mixed rows; verify the proposed snapshot targets and read-on-open trigger against the confirmed meanings. New/Reply-all/Forward and recipient editing belong to later slices.
- **Reader/composer:** earlier-message expansion, matching-message navigation, text reading and HTML-only extraction; later best-effort HTML and blocked/allowed remote content states, summary absent/stale/error states, attachments, plain-text/HTML composition, signatures and send/recovery states.
- **Calendar:** the combined permitted projection, day/week/month/agenda plus search, event details/edit/RSVP, recurrence choices, all-day/overlap overflow, time zones and the independently browsable Mail timeline.
- **Shared shell:** minimum supported window and scaling, split resizing, mouse/keyboard focus, loading/offline/empty/error states and persisted local settings that preserve account/view/draft state. Distinguish sync freshness from incomplete history or pending writes.

Record only the selected slice's settled interactions in its living contract, with affected consumers and fixtures. The visual mockups do not resolve provider behavior, renderer safety or data consistency.

## Related specifications

[North star](north-star.md) and [product scope](product-scope.md) define the product. [Mail UI](contracts/mail-ui.md) owns mailbox navigation, the split layout and mail surfaces; [Calendar](contracts/calendar.md) supplies the day strip's event behavior and the full calendar. [Triage](contracts/triage.md) defines message states and counts; [Core](contracts/core-storage.md) preserves account/view state. Implementation details stay in those contracts.

Index design artifacts here when supplied. Append accepted changes and Q&A to [decisions](decisions.md), then update this brief and affected contracts together. [Status](status.md) continues to distinguish design progress from implemented behavior.
