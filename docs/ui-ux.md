# Visual design and interaction brief

**D011 update, 2026-09-11:** updated mail, calendar and settings designs have arrived for the initial phase. Use them for the small paired GUI/CLI slice, with text-first human composition, shared IPC and durable state. D009's Google-first and user-owned Done/Delayed boundaries remain; pictured Fastmail/model features do not expand the first slice.

Updated 2026-09-11. Working brief for the founder's collaboration with Claude Design. The latest mail/calendar/settings trio guides the initial phase; the earlier pair and three mood references remain history. Confirmed product behavior, current visual direction and unresolved interactions remain separate. No full-product design or stack freeze is implied.

## Supplied visual references

The latest references are [initial mail](research/assets/2026-09-11/initial-mail.png), [initial calendar](research/assets/2026-09-11/initial-calendar.png) and [initial settings](research/assets/2026-09-11/initial-settings.png). Use these first for initial screen work. [Reference notes and unchanged image copies](research/design-references.md) distinguish observations from product decisions. The previous [closer mail](research/assets/2026-09-11/closer-mail.png) and [closer calendar](research/assets/2026-09-11/closer-calendar.png) remain historical.

The earlier mood/layout references remain useful history:

- [Mockup 1](research/assets/2026-09-11/mockup-01.png): dark, restrained interface with strong headings, mono metadata and a multi-calendar bottom timeline.
- [Mockup 2](research/assets/2026-09-11/mockup-02.png): serif-led treatment, small caps and red marks emphasizing deferred mail.
- [Mockup 3](research/assets/2026-09-11/mockup-03.png): a denser sidebar/list/reader with visible counts and labels.

The earlier third image's sidebar agenda does not replace the confirmed bottom strip; all three latest screens show that strip. Sample counts, dates, “Done,” timeline markers and sync/AI states are mock content. Validate text selection, long subjects, contrast and scaling with real fixtures. Do not force received HTML into the mockup's typography at the expense of readable content.

A CLI and smooth installation are required alongside these screens; begin with a useful partial CLI and expand both clients together. The CLI exposes the same product behavior without a window; it does not need a visual copy of every screen. The [stack recommendation](research/desktop-stack.md) can be tested while detailed visual design continues.

## Current visual direction

Use the latest trio as the initial-phase composition, while detailed styling and interactions remain open:

| Region | Direction in the initial-phase mockups | Behavior still to settle |
| --- | --- | --- |
| Shared top bar | Mail/Calendar switch, separate account tabs with compact counts, search, date/time and sync status | Label count units and coverage; account changes and calendar search must have explicit scope |
| Mail list | Left conversation pane; compact subject-first rows with sender/member count, time, selection and unread dots; delayed and done groups | Group precedence for a conversation that is both unread and starred; read triggers, ordering and access to all mail scopes |
| Reader/composer | Right pane with category chips, Reply/Delay/Done, sans-serif heading, optional summary, earlier members, offline attachment and inline reply with To/From/subject | Exact command targets, member expansion, recipients/signatures/attachments and send/recovery states; first slice saves a plain-text draft |
| Calendar | Full-width week grid, date range/account heading, Day/Week/Month/Today controls, understated event blocks and a current-time line | Agenda entry, all-day/overlap handling, calendar selection, event editing and permission states |
| Settings | Left navigation, shared top/footer and Done/Esc exit; cards and switches are illustrative | Founder says the screen is incomplete: general plumbing only; ignore the switches and await detailed controls |
| Today footer | Thin full-width timeline below mail, calendar and settings, next meeting/countdown, current-time marker and compact day context | Density/overflow, event identity, available actions and the meaning of optional statistics |

The restrained dark canvas, compact sans-serif headings/body text and sparse amber time accents are the current visual direction, replacing the earlier serif-led treatment. Exact fonts, colors, borders and dimensions are unselected. Quiet styling still needs legible secondary text, visible focus and state markers that work without color alone. Test the faint Done rows, metadata, calendar grid and footer at normal window sizes on both development hosts.

The two-pane mail layout now pictures Inbox/Sent, category chips, an inline reply and an account-settings entry. Draft navigation, reply-all, forward, new composition, editable recipients, signatures and outgoing attachments still need their affected designs. The first slice uses real local draft persistence; a pictured Send button does not add live sending to it.

## What the interface should make real

Unread mail is unreviewed attention. Starring an email means leaving an action for later: the work is still outstanding, even if it has begun. Read mail is not necessarily resolved mail. The desired end-of-day state is zero unread and zero starred messages through actual review and completion.

Time is precious. Today's calendar stays visible while working through mail so the user can see the day alongside the action they are postponing. A dedicated calendar remains part of the product.

Use one opinionated design across Windows, macOS and Linux (Ubuntu, Arch/Omarchy), shaped around the founder's workflow. Prioritize mouse interaction, readable content, clear state and responsive feedback. Platform input, accessibility and scaling still need to work; visual styling is shared.

## Confirmed layout

| Element | Requirement |
| --- | --- |
| Mailbox selection | At the top of the application, with the active mailbox clear |
| Main mail workspace | A resizable split screen for conversation browsing and reading |
| Today's calendar | A strip fixed at the bottom of every mail screen |
| Dedicated calendar | A normal calendar workspace with day/week/month/agenda views |
| Mail state | Make unread attention and deferred starred action understandable through the UI |

For this brief, “mailbox” is interpreted as a connected account (personal/work), consistent with separate accounts and no unified inbox. Folders, labels and categories remain scopes within that account. The selector's exact form—tabs, buttons or another control—is open.

The latest mail mockup keeps the conversation list on the left and reader on the right, with the reply composer inline beneath the received content/attachment. Retain a resizable divider; exact ratio, minimum sizes and compact-window adaptation remain to validate. The screenshot's apparent width is not a fixed specification.

```text
+-----------------------------------------------------------+
| Mailbox selection at the top                              |
+--------------------------+--------------------------------+
| Mail scopes / search     | Selected conversation          |
| Conversation list        | Reader / human composer        |
|                          |                                |
|             draggable split divider                       |
+--------------------------+--------------------------------+
| Today's calendar strip                                    |
+-----------------------------------------------------------+
```

This sketch identifies layout regions. Detailed controls and their placement belong in the design iterations.

## Mail workspace

Keep the selected account and its unread/starred totals visible while browsing Inbox, Unread, Starred, folders/categories and search results. Use conversations as the only list unit; show the relevant message within the reader. Folder/category/sender filters should be identifiable and removable.

Proposed resize behavior: a discoverable drag handle, independent pane scrolling, usable minimum pane sizes and remembered split position. Resizing must preserve the selected conversation, reading position, draft and bottom calendar strip. Explore compact widths without squeezing the reader into unusable space or removing the strip.

Restore each account's selected conversation/member, filters and reading position when returning from another account or the full calendar. Switching accounts must not silently retarget an existing draft's From address. Readable text, safe extraction for HTML-only messages, attachments and restrained human composition are the early requirements. Best-effort HTML display is a later enhancement, with sources preserved throughout. Optional local summaries belong with their source conversation; the composer stays human-authored.

## Make read and star states legible

The following treatments and wording are proposals to test with the design. The latest mockup's **Delay** can express the accepted star/hold meaning; it does not authorize scheduled snooze or automatic resurfacing. **Done** still needs an explicit command definition: it cannot silently bundle mark-read, unstar and archive, or clear future replies. Do not create a separate synchronized Done state merely to match the screenshot. Read and unstarred is not proof that an action outside the app was completed.

Review/delayed/done groups must handle overlapping flags. A conversation with unread and starred members keeps both facts visible even if shown only once in a combined list. Account totals and list conversation counts need distinct labels, complete scope and pending/coverage context. The latest tabs show single sample counts; their units and scope are still not specified.

| State or action | Meaning to communicate | Proposed expression |
| --- | --- | --- |
| Unread | “I have not reviewed this” | Clear row marker plus readable emphasis; an Unread view and persistent account total |
| Read | “I have reviewed this” | Remove unread emphasis while retaining any outstanding star |
| Star | “I am leaving this action for later” | Keep the familiar star; explain its meaning in tooltip/action wording |
| Starred | “Action is still waiting” | A Starred view with a short “Action deferred” description and visible hold age |
| Remove star | “Clear this action hold” | Explicit unstar after completion; sending a reply does not clear it automatically |
| Read and unstarred | “No unread attention or held action on this message” | Calm normal state; no claim that the whole mailbox is complete |

Possible copy: **Star — leave this action for later**, **Starred · action deferred**, and **Clear star — remove this hold**. Final labels remain open. Preserve the recognizable star and native provider state; this does not introduce a task, due date, reminder or scheduled snooze.

Make old holds easy to notice and revisit. Show elapsed age when known and explore oldest-first sorting. A star imported from another client may only have a first-observed date; label that honestly rather than inventing when the user deferred the action. Use age and counts to reveal unfinished work without fabricated urgency, punitive scores or repeated nags.

Unread and starred are independent: starring unread mail does not mark it read; reading starred mail does not remove its hold. A conversation can be in both views. The design must explain when a command targets one message versus existing members of the conversation.

Counts stay honest across filters, archive and folder moves. Show incomplete sync and pending/failed changes without claiming the day is clear. When the open conversation leaves an Unread filter, let the user finish reading it before navigating away. Read-on-open timing is still undecided. Collapsing earlier messages must preserve a route to every member; search reveals its matching message even when it would normally be collapsed. A local summary or an unopened collapsed member is not evidence that the user reviewed that message.

## Today's calendar strip

Keep the strip attached to the bottom of the mail workspace across Inbox, Unread, Starred, folder/category views, search, conversation reading and composition. It does not scroll away with message content or disappear during split resizing. Its presence is confirmed; it is not an optional dashboard widget.

Proposed visual treatment: a horizontal view of today with a clear current-time marker, event positions/durations, elapsed versus remaining time, and readable current/next meeting information. Useful gaps before the next event may be shown for the displayed calendars; do not imply the user is universally available merely because those calendars contain no event.

Design for overlapping meetings, all-day events and busy days. Preserve calendar/account identity, expose overflow counts and keep an event-details route. A quiet empty state must distinguish “no events in the shown calendars” from calendars that have not loaded or are stale. Show enough date/time-zone context to make the day understandable.

Provide a clear route to the full calendar and a Join action when the event has a conference link. Joining acknowledges that occurrence's reminder only; it does not change RSVP or claim attendance. Keep physical location separate from the conference link. Alerts remain governed by running-app preferences; the strip does not add a new notification policy.

Which calendars appear while switching mailboxes remains open: current-account calendars or an explicitly selected set. The latest images show named events for the active account and Busy · account placeholders for others. This is a proposal for calendar context, not permission to expose hidden titles/attendees or a decision to enable every account by default. Calendar permissions must govern the data supplied to every surface, including tooltips, search, summaries and CLI output. Distinguish a voluntary busy-only presentation from provider-limited availability.

This does not merge mail accounts or reopen whether the strip itself is visible. Timeline range, height, overflow and compact-window treatment remain design choices. The depicted 07:00–21:00 span is an example, not an event cutoff. “Time left,” “meetings ahead” and arrival markers need clear definitions and coverage before inclusion; free space on selected calendars is not guaranteed availability.

## Dedicated calendar

Keep day/week/month/agenda navigation, search, calendar visibility and event details/editing available in a dedicated workspace. Include Google recurring-event edits, invitations/RSVP and shared-calendar permissions; external read-only subscriptions are later work after Google dogfooding. The bottom strip and full calendar must represent the same events and occurrence state.

Use the same typography, event identity and interaction language as the strip. Returning to mail should restore the user's place. Carry the footer across mail, calendar and settings as shown in the latest initial-phase references. While browsing another date/week, the footer must clearly remain today rather than adopt the grid's date silently.

The week reference uses understated event blocks, subdued other-account busy markers and a current-day/current-time accent. It does not define drag-to-create, drag-to-reschedule or recurrence-edit gestures. Agenda remains required even though its tab is absent. Calendar search must search events with explicit scope; the pictured mail search placeholder is not a specification for calendar search.

## Initial settings plumbing

The founder explicitly clarified that the settings mockup is incomplete and its Mail/Calendar switches should be ignored. Implement only the shell and navigation when building the initial slice: enter/leave settings, keep the active account and footer context, restore the previous mail/calendar view and preserve any draft. Create a simple place to attach later settings pages; detailed controls and their persistence/side effects wait for the expanded design.

Do not implement switches, add/remove-account flows, provider/model setup or a generic configuration framework merely because their labels appear in the image. The settings Done/Esc exit is distinct from mail's action-state Done. The mock version, provider names and indexing percentages are not live product state. Existing sync/index/offline truthfulness requirements apply when those status controls are designed.

## States to include in design iterations

- Normal mail: several accounts at the top, a populated split view and today's strip.
- Outstanding attention: unread-only, starred-only and combined states; a recent hold and an older hold with an honest age.
- Filtered mail: sender/category chips, search matching a particular message, and archived unread mail still reflected in totals.
- Human composition: a reply with a signature and attachments while the calendar remains visible.
- Time: an empty day, overlapping meetings, a dense day, all-day overflow and an available Join action.
- Window changes: resized panes and compact layouts that preserve reading and the bottom strip.
- Reliability: loading/offline/stale calendars, incomplete mail coverage, pending/failed actions and a truthful zero-work state.
- Optional assistance: absent/disabled, category suggestion and generated summary with source access; normal reading/composition still works.

These are review scenarios for the existing product, not additional app areas. Visual direction, colors, typography, density, final copy, selector treatment, pane orientation and compact behavior should emerge from the founder/Claude design iterations. Use the reader/composer and calendar slice to validate the recommended stack on all required desktops; final styling can continue evolving.

## Handoff before UI implementation

The initial-phase mail/calendar/settings references have arrived. Use them for the approaching slice, with settings restricted to the founder's requested plumbing. Review remaining interactions when their behavior is implemented; a complete future settings design is not a blocker for the shell or headless engine.

The focused handoff should resolve these gaps for the slice being built, rather than freezing every later screen now:

- **Mail:** account and folder/category navigation, New/Reply/Reply-all/Forward entry points, search results, count labels, mixed unread/starred rows, Delay/Done targets and read-on-open behavior.
- **Reader/composer:** earlier-message expansion, matching-message navigation, text reading and HTML-only extraction; later best-effort HTML and blocked/allowed remote content states, summary absent/stale/error states, attachments, plain-text/HTML composition, signatures and send/recovery states.
- **Calendar:** visibility/Busy overlay policy, day/week/month/agenda plus search, event details/edit/RSVP, recurrence choices, all-day/overlap overflow, time zones and the footer while viewing another date.
- **Shared shell:** minimum supported window and scaling, split resizing, mouse/keyboard focus, loading/offline/empty/error states and settings navigation that preserves account/view/draft state. Detailed setup/settings controls wait for the next design; distinguish sync freshness from incomplete history or pending writes when status is implemented.

Record only the selected slice's settled interactions in its living contract, with affected consumers and fixtures. The visual mockups do not resolve provider behavior, renderer safety or data consistency.

## Related specifications

[North star](north-star.md) and [product scope](product-scope.md) define the product. [Mail UI](contracts/mail-ui.md) owns mailbox navigation, the split layout and mail surfaces; [Calendar](contracts/calendar.md) supplies the day strip's event behavior and the full calendar. [Triage](contracts/triage.md) defines message states and counts; [Core](contracts/core-storage.md) preserves account/view state. Implementation details stay in those contracts.

Index design artifacts here when supplied. Append accepted changes and Q&A to [decisions](decisions.md), then update this brief and affected contracts together. [Status](status.md) continues to distinguish design progress from implemented behavior.
