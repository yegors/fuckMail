# Visual design and interaction brief

**D088 / clean thread content:** each message shows its new contribution without quoted-history controls or attribution headers. Inline answers and intentional forwards remain content. Click a header or its chevron to expand/collapse any message, including the newest; focused headers support keyboard activation. The earlier-message group also toggles both ways. Original source stays available separately.

**D087 / reader stability:** repeated navigation keeps one cursor and opens its final target once per frame. Source/body/summary geometry stays stable on display; late summary responses cannot replace the prepared result, and delivery feedback occupies a fixed line. Nested newsletter layout tables become readable linear content while retaining text/links and the original stored source.

**D086 / September 22 reference:** the [updated prototype](research/prototype-september-22.md) owns the current mail-list and Accounts presentation. Incoming rows separate sender identity from subject/summary preview; narrow panes retain readable compact rows, and Sent/Drafts show recipients. Accounts adds cached/processed counts, a missing-only date-range menu, progress and Stop. Arrows, Page Up/Down, Home/End and Shift ranges share a single pointer/keyboard cursor. Real counts and bounded engine jobs replace prototype simulation; Google mailbox total remains unknown. Preserve D085 scrollbar behavior and established human-owned mail actions.

**D059–D064 / P2-UI3 active:** implement the September 15 prototype across reader, list actions, Settings, event editing, shared confirmations and composer account/recipient/file controls, plus six-second Send Undo and a shared action toast. [Coverage and verification](research/prototype-september-15.md). Prior D058 package and evidence remain intact; scoped UI3 checks pass; complete prototype alignment remains open.

**D058 / P2-LLM2D:** incoming-only assistance fixes the missing reader sent flag, hides sent summaries and removes duplicate errors. Model response failures are distinguished and the default response allowance grows to 4,096 tokens. Schema 24 retains prior settings/history. Scoped Rust/native checks, strict lint, 16 engine/CLI checks, actual settings migration/refusal and relocated configured package pass. Use `dist/llm2d-windows-amd64/bin/fuckmail-gui.exe`; close the old app first. Readiness remains 0/9.

**D057 / P2-LLM2C:** Settings now shows only editable System/User message content with normal line breaks. Native editing/preservation and relocated package checks pass. Use `dist/llm2c-windows-amd64/bin/fuckmail-gui.exe`; schema 23 and the verified engine/CLI are unchanged.

**D056 / P2-LLM2B:** the Local model editor now contains the complete Ollama JSON request, including system instructions, response schema, options and all variables. Inference sends its expansion without hidden additions. Schema 23 preserves saved instructions and account choices. Scoped Rust/native tests, strict lint, 15 engine/CLI checks including exact HTTP request comparison, actual settings upgrade/refusal and relocated Windows launch pass. The configured app is `dist/llm2b-windows-amd64/bin/fuckmail-gui.exe`; close the older app first. The installed model has a recorded label-quality miss. Readiness remains 0/9.

D055 / P2-LLM2 is the current local-assistance packet: one model/prompt, default-on prospective summaries and automatic single-label classification, per-message cards and summary list previews. Remove the sample/default-reset buttons and top thread summary. Complete the scoped Windows, migration, delivery and installed-model checks before handing back the candidate; broad performance remains deferred. The D055 decision and Local assistance 8 supersede older delivery defaults below.

D054: the header beside search is a real account Sync button with last successful Gmail history check time. Show Syncing during the request and Retry sync after credential refusal; keep the prior successful time on failure. The OS keychain password is never entered into the app.

**D051 planned Local model page:** automatic Ollama/model discovery, independent summary/classification selectors, editable prompt buffers with Save/Cancel/Restore default and a synthetic sample test. Account enablement is independent; labels remain in the existing account label settings. Reader summaries retain originals and source/partial/stale context; sender-kind annotations are correctable. [Local assistance 5](contracts/local-ai.md) owns target behavior. No controls are implemented by this planning update.

D049/D050 applies the September 14 second updated reference. Native Inbox browses all retained non-Trash conversations in the selected account regardless of Gmail INBOX membership, displaying linked user labels. Right-click opens actions without reviewing; modifier-click/range selection supports serial frozen-target bulk actions with visible partial outcomes. Sent/Drafts have collapsible date groups and a shared Grouped/Plain list preference. Splash displays its full animation for at least 1.75 seconds plus fade, waiting for readiness while exposing failures immediately. Google Limits setup is removed; Start sync uses built-in published defaults.

**D020 Phase 2 planning:** preserve the implemented Update 2 design and P1-10 responsiveness while connecting actual Google capabilities. The [Phase 2 plan](plans/phase-2.md) owns live-account admission, provider progress, real human send/calendar writes and running-app reminders. Existing Settings and composer surfaces gain only verified capabilities; no prototype callback or planning text is evidence of live behavior. The source exports remain reference assets.

**D018 Update 2, 2026-09-12:** the second same-day [Flight Progress v2 export](research/flight-progress-v2.md) is the active Phase 1 design reference. Implement it closely, replacing invented elements. It adds General settings, three themes, text/startup choices, Mail timeline placement and mail-list/sender refinements under P1-09. D016/D017's explicitly confirmed New message, Reply all, Forward and account-scoped Drafts remain durable local workflows alongside Reply. D014's read-on-open, Done clearing unread and stars, untimed Delay, combined permitted calendar details and browsable Mail-only timeline remain. Live Google, models, legacy protocols and received HTML rendering are later work.

Updated 2026-09-12. Working brief for the founder's collaboration with Claude Design. Update 2 supersedes the earlier same-day export, September 11 HTML and earlier trio's visual direction; preserve those references as history. Confirmed choices, implementation details and sample prototype behavior remain distinct. No full-product design or stack freeze is implied.

## Supplied visual references

Use the archived September 12 Update 2 [Flight Progress v2 HTML](<research/assets/2026-09-12/flight-progress-v2-update-2/Flight Progress v2.dc.html>) (261329 bytes) and [support.js](research/assets/2026-09-12/flight-progress-v2-update-2/support.js), with SHA-256 provenance and exact inventory in the [scoped reference audit](research/flight-progress-v2.md). They are design reference data, not application runtime or build instructions. The [earlier September 12 export](<research/assets/2026-09-12/flight-progress-v2/Flight Progress v2.dc.html>) (222401 bytes), [September 11 export](<research/assets/2026-09-11/flight-progress-v2/Flight Progress v2.dc.html>), initial [mail](research/assets/2026-09-11/initial-mail.png), [calendar](research/assets/2026-09-11/initial-calendar.png) and [settings](research/assets/2026-09-11/initial-settings.png) trio, [closer mail](research/assets/2026-09-11/closer-mail.png) and [closer calendar](research/assets/2026-09-11/closer-calendar.png) remain historical in [reference notes](research/design-references.md).

The earlier mood/layout references remain useful history:

- [Mockup 1](research/assets/2026-09-11/mockup-01.png): dark, restrained interface with strong headings, mono metadata and a multi-calendar bottom timeline.
- [Mockup 2](research/assets/2026-09-11/mockup-02.png): serif-led treatment, small caps and red marks emphasizing deferred mail.
- [Mockup 3](research/assets/2026-09-11/mockup-03.png): a denser sidebar/list/reader with visible counts and labels.

The earlier sidebar agenda does not replace the confirmed Mail strip. D014 removed the strip from Calendar and Settings; D018 adds a saved footer/header placement within Mail. Calendar and Settings use their full height without reserving timeline space. Counts, dates, throughput and sync/model status in the prototype are sample content. Validate text selection, long subjects, contrast and scaling with real fixtures. Do not force received HTML into the design's typography at the expense of readable content.

A CLI and smooth installation are required alongside these screens; begin with a useful partial CLI and expand both clients together. The CLI exposes the same product behavior without a window; it does not need a visual copy of every screen. The [stack recommendation](research/desktop-stack.md) can be tested while detailed visual design continues.

## Current visual direction

Use Flight Progress v2 as the Phase 1 composition, with these implementation boundaries:

| Region | September 12 Update 2 visual target | Native behavior |
| --- | --- | --- |
| Shared top bar | 56 px bar with Mail/Calendar segment, separate account tabs/badges, quiet status, contextual search, New message/event and gear | Use actual coverage/status and account scope; omit the added clock and fixture action; New controls open working local workflows |
| Mail list | Active 22 px/inactive 15 px scopes, People/Automated rail, counted 34 px collapse headers and two-line rows including Done | Complete filtered conversation counts precede pagination; collapse retains reader/draft; explicit current kinds and unknown state replace prototype name guessing |
| Reader/composer | Category chips alongside compact Reply/Reply all/Forward and contextual Delay/Done; sender-kind/date metadata, sender card and inline To/Cc/Bcc/From/subject/body | Exact-address current-account history/count/filter and durable Write; snapshot actions exclude later arrivals; draft identity/source and one capture journal preserve authored work |
| Calendar | Quiet day/week/month navigation, day agenda, event popovers and aligned editor sheet | Combine permitted details, retain all-day/overlap access and local create/edit/self-RSVP; do not claim recurrence writes or provider access |
| Settings | 220 px navigation in General, Accounts, Local model, Mail, Calendar, Shortcuts order; 640 px content area, Done/Esc exit and segmented choices | Persist appearance/startup/duration/placement plus existing metadata/preferences; retain compact secondary controls and truthful provider/model/notification unavailability |
| Mail timeline | Quiet duration blocks and overlap lanes, adjacent-day browsing and Back to today; footer or directly beneath the shared header | Mail only; bound dense-day height/overflow and retain event identity/date/time-zone context; no empty footer reservation in header mode |

Use the exact shared Graphite (default), Paper and Harbour palettes from the [token inventory](research/flight-progress-v2.md#exact-shared-palette-and-typography-tokens). Graphite is near-black with amber; Paper is warm off-white with dark ink and oxblood; Harbour is deep blue-black with sea-glass accent. Derive secondary text, shadows, overlays, selection, borders and timeline states from the selected palette. Retain Geist, thin borders, a 56 px header and 52/44 px row densities. Small/Default/Large text scales native labels, lists, inputs and messages using rounded 13/14, 1 and 15.5/14 factors without rewriting authored formatting. The source's 1160 px minimum width is a reference size, not a verified native limit. Package the font locally with its license; do not depend on the prototype's CDN. Quiet styling still needs legible secondary text, visible focus and state markers that work without color alone. Test normal/scaled windows and all palettes on the required hosts.

The two-pane layout supplies Inbox/Sent/Drafts, search filters, category chips, a human composer and account settings. [P1-08](plans/phase-1.md#p1-08--september-12-prototype-replacement) includes new/reply/reply-all/forward with editable bounded recipients and subject, authored plain text/formatting, signatures, retained attachments, explicit discard and local MIME capture. Each draft keeps its original account and frozen source context through navigation/restart; new arrivals cannot retarget it. Drafts is a list of authored work, not fabricated incoming conversations. Alias management, new-file import and live sending remain later. The prototype's temporary draft and sending records do not define recovery.

## What the interface should make real

Unread mail is unreviewed attention. Starring an email means leaving an action for later: the work is still outstanding, even if it has begun. Read mail is not necessarily resolved mail. The desired end-of-day state is zero unread and zero starred messages through actual review and completion.

Time is precious. The calendar timeline stays visible while working through mail, starting on today and allowing deliberate browsing of another day. A dedicated calendar remains part of the product.

Use one opinionated design across Windows, macOS and Linux (Ubuntu, Arch/Omarchy), shaped around the founder's workflow. Prioritize mouse interaction, readable content, clear state and responsive feedback. Platform input, accessibility and scaling still need to work; visual styling is shared.

## Confirmed layout

| Element | Requirement |
| --- | --- |
| Mailbox selection | At the top of the application, with the active mailbox clear |
| Main mail workspace | A resizable split screen for conversation browsing and reading |
| Mail calendar timeline | A strip in the selected footer/header position of every Mail screen, initially today with adjacent-day browsing and Back to today |
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

This sketch shows the default footer arrangement. Header placement moves the same timeline directly below the top bar. Detailed controls and their placement belong in the design iterations.

## Mail workspace

Keep the selected account and its unread/starred totals available while browsing Inbox, Sent, review/held filters, categories and search results. Received mail uses conversations as its only list unit; show the relevant message within the reader. Drafts separately lists saved authored drafts in the current account and permits text search without received-message sender/category/state/date/attachment filters. Active filters should be identifiable and removable.

Proposed resize behavior: a discoverable drag handle, independent pane scrolling, usable minimum pane sizes and remembered split position. Resizing must preserve the selected conversation, reading position, draft and the calendar strip in its chosen position. Explore compact widths without squeezing the reader into unusable space or removing the strip.

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

Use the reference's contextual **Delay** or **Done** control, with **Remove Delay** and explicit read/unread actions reachable through the conversation menu. Tooltips explain the actual flag effects and snapshot target. Preserve native star/read state; this does not introduce a task, due date, reminder or scheduled snooze.

Make old holds easy to notice and revisit. Show elapsed age when known and explore oldest-first sorting. A star imported from another client may only have a first-observed date; label that honestly rather than inventing when the user deferred the action. Use age and counts to reveal unfinished work without fabricated urgency, punitive scores or repeated nags.

Unread and starred are independent: starring unread mail does not mark it read; reading starred mail does not remove its hold. A conversation can be in both views. The design must explain when a command targets one message versus existing members of the conversation.

Counts stay honest across filters, archive and folder moves. Show incomplete sync and pending/failed changes without claiming the day is clear. Let the user finish reading an open conversation after it leaves Unread. The proposed read-on-open snapshot includes existing collapsed members, while every member remains accessible and a search match is revealed. This is the chosen open/read convention, not evidence that every hidden member was inspected. Later replies remain unread, and summaries never trigger the command.

Update 2 adds collapsible review/delayed/done headers whose counts cover the complete filtered conversation set, including unloaded pages. Collapsing rows must not discard the active reader or composer or strand paging. Preserve two-line Done rows and compact relative dates with the full timestamp available on hover. People/Automated filters use explicit current source classification; absent/stale classifications remain unknown. The prototype's display-name heuristic is not product logic. Keep category scope separate, limit the suggested rail to seven and retain access to all available account categories.

The sender card shows the exact email, full account-scoped conversation count and at most three recent conversations. Its filter uses that address, never a display-name match across accounts; Write creates an ordinary durable draft in the active account. Opening the card is read-only; opening a listed conversation is a deliberate read action. Active exact-sender/kind constraints remain visible and removable in search and survive corrections in More filters. Mailbox Tab/ShiftTab cycling must leave editable/modal focus traversal intact. A minimized draft uses the compact floating bar with explicit Resume and confirmed Discard.

## Mail calendar timeline

Keep the strip attached to the selected footer or header position of the Mail workspace across Inbox, Unread, Starred, folder/category views, search, conversation reading and composition. Header mode sits directly below the 56 px top bar; it must not also reserve empty footer space. The strip does not scroll away with message content or disappear during split resizing. Its presence is confirmed; it is not an optional dashboard widget.

Start on today with a current-time marker, duration blocks, overlap lanes and distinct current/next/past events. Allow adjacent-day browsing by drag plus an accessible navigation route, with the displayed date and Back to today made clear. The current-time marker belongs only to today. Useful gaps refer to the displayed calendars and their coverage; they do not establish universal availability.

Design for overlapping meetings, all-day events and busy days. Preserve calendar/account identity, expose overflow counts and keep an event-details route. A quiet empty state must distinguish “no events in the shown calendars” from calendars that have not loaded or are stale. Show enough date/time-zone context to make the day understandable.

Provide a clear route to the full calendar and an explicit meeting-link action when a permitted conference URL exists. It does not change RSVP or claim attendance. Keep physical location separate from the conference link. Phase 1 does not implement reminders or their acknowledgements. When reminders arrive in the later phase, Join will acknowledge that occurrence's reminder only, under the running-app alert preferences.

The founder chose a combined calendar with full permitted details from connected accounts; mail stays separate. The timeline and full Calendar share an explicit selected-calendar visibility preference and clear account identity in details. Calendar selection is independent of the selected mailbox: switching mail accounts does not change the displayed calendar sources. Provider permissions still govern every supplied field, including labels, tooltips, conflict text, search and CLI output. An account with free/busy access supplies Busy information only; combining calendars grants no additional access. Hiding a selected calendar is a display preference, not removal or a sync change.

Persist the Mail timeline's visible hours. The September 12 full Calendar uses a fixed 00:00–24:00 day/week grid with midnight initially visible; its legacy grid-hour preference values remain stored for compatibility and have no native controls. An event outside the timeline's chosen display span still exists and needs an overflow/access route. Bound lane growth on dense days so the timeline does not consume the reader. Exact height/overflow and compact-window treatment need native review. Any optional time/meeting statistics require defined units and coverage; sample values are not product state.

## Dedicated calendar

Keep day/week/month/agenda navigation, search, calendar visibility and event details/editing available in a dedicated workspace. Include Google recurring-event edits, invitations/RSVP and shared-calendar permissions; external read-only subscriptions are later work after Google dogfooding. The Mail strip and full calendar must represent the same events and occurrence state.

Use the same typography, event identity and interaction language as the Mail timeline. Returning to Mail restores its reading position, draft and timeline date. Calendar and Settings have no timeline or reserved strip space under D014/D018. Calendar date navigation and Mail timeline navigation retain their own explicit dates.

The current reference uses understated event blocks, account details and a current-day/current-time accent across day/week/month, with New event in the shared header and an aligned editor sheet. Apply its form geometry to the actual local create/edit/self-RSVP workflow. Recurrence mutation remains later. Its day agenda supplements the required agenda/search workflows. Calendar search must search events within its permitted selected calendar scope; a mail search placeholder is not calendar search behavior.

## Phase 1 settings

Implement Update 2's six-section order: **General, Accounts, Local model, Mail, Calendar, Shortcuts**. General is the initial page; retain the visited section when reopening during a session. Theme swatches choose Graphite/Paper/Harbour. Text size offers Small/Default/Large. Open on offers Mail/Calendar/Where I left off and controls the next launch, preserving the last Mail/Calendar workspace independently of Settings. The typed defaults and shared persistence are defined in [Platform](contracts/platform.md#local-appearance-and-startup); screenshots and source alone do not verify restart behavior.

Calendar settings orders independent source visibility, New events last (30/45/60 minutes; default 60), then Timeline placement (Footer/Header; default Footer), hours and week start. Preserve existing hours/week defaults and retained values; changing duration affects newly created events, not existing ones. Keep segmented density/week controls and real account display names/signatures. Place signatures, source selection and diagnostics in compact secondary sections where the reference has no direct equivalent. Keep legacy full-calendar hour values stored without exposing ineffective controls for the fixed full-day grid. Adapt the prototype's mailbox-relative Other accounts switch into independent selected calendars. Native categories remain account scoped, without inventing global label deletion.

Persist preferences and metadata through Core/CLI; failed saves retain entered fields and a retry appropriate to that setting. Enter/leave Settings without losing account, independent scroll/focus, selection or draft. Theme/text changes and account signatures must not rewrite existing authored routing, formatting, body or attachments. Verify their visible effect with native controls and independent restart/readback evidence.

Sync every and Notifications remain visible but disabled with explicit unavailable explanations; no prototype default is presented as an active provider/OS configuration, and local IPC polling is not sync. Omit timed Delay and timed Follow up controls. Account OAuth, real removal/cache deletion and local-model download/setup remain later work; show truthful unavailable state for retained pages. The settings Done/Esc exit is distinct from mail's action-state Done. Show actual local version, freshness, indexing and offline coverage rather than sample percentages, throughput or simulated sign-in success. No additional JS runtime or general settings framework is required by the exported prototype.

## States to include in design iterations

- Normal mail: several accounts at the top, a populated split view and the calendar timeline on today or an explicitly browsed date.
- Outstanding attention: unread-only, starred-only and combined states; a recent hold and an older hold with an honest age.
- Filtered mail: sender/category chips, search matching a particular message, and archived unread mail still reflected in totals.
- Human composition: new/reply/reply-all/forward with editable recipients/subject, signature and retained attachments; Drafts navigation, conflict recovery and local capture while the calendar remains visible.
- Time: an empty day, overlapping meetings, a dense day, all-day overflow and an available Join action.
- Window changes: all three palettes/text sizes, resized panes and compact layouts that preserve reading and either timeline placement.
- Reliability: loading/offline/stale calendars, incomplete mail coverage, pending/failed actions and a truthful zero-work state.
- Optional assistance: absent/disabled, category suggestion and generated summary with source access; normal reading/composition still works.

These are review scenarios for the existing product, not additional app areas. Visual direction, colors, typography, density, final copy, selector treatment, pane orientation and compact behavior should emerge from the founder/Claude design iterations. Use the reader/composer and calendar slice to validate the recommended stack on all required desktops; final styling can continue evolving.

## Handoff before UI implementation

The Phase 1 design and the key action/calendar choices are supplied. Use the [Phase 1 plan](plans/phase-1.md) for bounded native implementation and acceptance. Review missing editor and error surfaces as their packets approach; real-provider and model setup remain outside this local phase.

The focused handoff should resolve these gaps for the slice being built, rather than freezing every later screen now:

- **Mail:** account and Inbox/Sent/Drafts navigation, new/reply/reply-all/forward entry points, bounded recipient/subject editing, real search results, labeled counts and mixed rows. Verify frozen draft source/account identity, snapshot actions and capture recovery across new arrivals and restart.
- **Reader/composer:** earlier-message expansion, matching-message navigation, text reading and HTML-only extraction; later best-effort HTML and blocked/allowed remote content states, summary absent/stale/error states, attachments, plain-text/HTML composition, signatures and send/recovery states.
- **Calendar:** the combined permitted projection, day/week/month/agenda plus search, event details/edit/RSVP, recurrence choices, all-day/overlap overflow, time zones and the independently browsable Mail timeline.
- **Shared shell:** minimum supported window and scaling, split resizing, mouse/keyboard focus, loading/offline/empty/error states and persisted local settings that preserve account/view/draft state. Distinguish sync freshness from incomplete history or pending writes.

Record only the selected slice's settled interactions in its living contract, with affected consumers and fixtures. The visual mockups do not resolve provider behavior, renderer safety or data consistency.

## Related specifications

[North star](north-star.md) and [product scope](product-scope.md) define the product. [Mail UI](contracts/mail-ui.md) owns mailbox navigation, the split layout and mail surfaces; [Calendar](contracts/calendar.md) supplies the day strip's event behavior and the full calendar. [Triage](contracts/triage.md) defines message states and counts; [Core](contracts/core-storage.md) preserves account/view state. Implementation details stay in those contracts.

Index design artifacts here when supplied. Append accepted changes and Q&A to [decisions](decisions.md), then update this brief and affected contracts together. [Status](status.md) continues to distinguish design progress from implemented behavior.


D053 aligns local summaries with Update 2’s compact outlined Summary card below reader metadata: dedicated theme assistance color, narrow left accent, uppercase heading and quiet model attribution. Message summaries stay with their source; re-summarize and source/provenance controls are secondary. Summary-only enablement exposes both thread and individual actions, independently of classification. See [Local assistance](contracts/local-ai.md#d053-summary-presentation-and-control-repair) and the [execution report](research/local-assistance-execution.md#d053-summary-ui-and-lifecycle-repair-2026-09-14).
