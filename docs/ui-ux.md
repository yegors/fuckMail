# Visual design and interaction brief

Updated 2026-09-11. Working brief for the founder's collaboration with Claude Design. This describes the elements and behavior to design; no final screens, palette or dimensions have been selected. Nonfinal references now guide a proposed technical baseline. Confirmed direction is separated from proposed treatments.

## Supplied visual references

The founder supplied three mood/layout mockups on 2026-09-11 for technology choices. They are not final or exact designs. [Reference notes and unchanged image copies](research/design-references.md) record what each suggests.

- [Mockup 1](research/assets/2026-09-11/mockup-01.png): dark, restrained interface with strong headings, mono metadata and a multi-calendar bottom timeline.
- [Mockup 2](research/assets/2026-09-11/mockup-02.png): serif-led treatment, small caps and red marks emphasizing deferred mail.
- [Mockup 3](research/assets/2026-09-11/mockup-03.png): a denser sidebar/list/reader with visible counts and labels.

The third image's sidebar agenda does not replace the confirmed bottom strip. “Done/Finished,” keyboard hints, grouping, mail-arrival ticks, specific fonts/colors and sample sync/AI states are reference content, not newly accepted behavior. Validate text selection, long subjects, contrast and scaling with real fixtures. Do not force received HTML into the mockup's typography at the expense of content fidelity.

A full CLI and smooth installation are now required alongside these screens. The CLI exposes the same product behavior without a window; it does not need a visual copy of every screen. The [stack recommendation](research/desktop-stack.md) can be tested while detailed visual design continues.

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

Proposed pane arrangement for design exploration: conversation list on the left, reader/composer on the right. The resizable split is confirmed; its orientation, default ratio and dimensions are not yet selected.

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

Restore each account's selected conversation/member, filters and reading position when returning from another account or the full calendar. Switching accounts must not silently retarget an existing draft's From address. Full HTML reading, a secondary plain-text fallback, attachments and restrained human composition remain required. Optional local summaries belong with their source conversation; the composer stays human-authored.

## Make read and star states legible

The following treatments and wording are proposals to test with the design:

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

Counts stay honest across filters, archive and folder moves. Show incomplete sync and pending/failed changes without claiming the day is clear. When the open conversation leaves an Unread filter, let the user finish reading it before navigating away. Read-on-open timing is still undecided.

## Today's calendar strip

Keep the strip attached to the bottom of the mail workspace across Inbox, Unread, Starred, folder/category views, search, conversation reading and composition. It does not scroll away with message content or disappear during split resizing. Its presence is confirmed; it is not an optional dashboard widget.

Proposed visual treatment: a horizontal view of today with a clear current-time marker, event positions/durations, elapsed versus remaining time, and readable current/next meeting information. Useful gaps before the next event may be shown for the displayed calendars; do not imply the user is universally available merely because those calendars contain no event.

Design for overlapping meetings, all-day events and busy days. Preserve calendar/account identity, expose overflow counts and keep an event-details route. A quiet empty state must distinguish “no events in the shown calendars” from calendars that have not loaded or are stale. Show enough date/time-zone context to make the day understandable.

Provide a clear route to the full calendar and a Join action when the event has a conference link. Joining acknowledges that occurrence's reminder only; it does not change RSVP or claim attendance. Keep physical location separate from the conference link. Alerts remain governed by running-app preferences; the strip does not add a new notification policy.

Which calendars appear while switching mailboxes remains open: current-account calendars or an explicitly selected set. Always show their identities. This does not merge mail accounts, and it does not reopen whether the strip itself is visible. Timeline range, height, overflow and compact-window treatment remain design choices.

## Dedicated calendar

Keep day/week/month/agenda navigation, search, calendar visibility and event details/editing available in a dedicated workspace. Include recurring-event edits, invitations/RSVP, shared-calendar permissions and read-only subscriptions. The bottom strip and full calendar must represent the same events and occurrence state.

Use the same typography, event identity and interaction language as the strip. Returning to mail should restore the user's place. The bottom-strip requirement applies to mail screens; its treatment inside the full calendar is open.

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

## Related specifications

[North star](north-star.md) and [product scope](product-scope.md) define the product. [Mail UI](contracts/mail-ui.md) owns mailbox navigation, the split layout and mail surfaces; [Calendar](contracts/calendar.md) supplies the day strip's event behavior and the full calendar. [Triage](contracts/triage.md) defines message states and counts; [Core](contracts/core-storage.md) preserves account/view state. Implementation details stay in those contracts.

Index design artifacts here when supplied. Append accepted changes and Q&A to [decisions](decisions.md), then update this brief and affected contracts together. [Status](status.md) continues to distinguish design progress from implemented behavior.
