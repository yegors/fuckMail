# Mail UI, reader and human composer

State: Draft · Revision: 9 · Updated: 2026-09-12 · Implementation: Phase 0 synthetic experiment verified; Phase 1 integration active; whole area remains Draft.

Revision 9 records the active native implementation against Core 10/schema 5/fixture 2 and additive protocol 1. Deliberate conversation opening uses a successfully hydrated member snapshot; passive restoration remains a query. Done uses one read+unstar mutation, Delay changes only the hold, and a reviewed active row remains pinned until navigation/query changes. The composer edits authoritative plain text with a limited HTML alternative, per-draft signature snapshots, retained attachment IDs, explicit discard and local MIME capture. Formatting applies to selected text; paste inserts plain text. Conflicting authored saves retain local fields and require explicit confirmation before saving that version over the latest revision. Capture recovery queries unfinished journal entries before creating a new operation identity, including after restart. Capture history is labeled local and never claims delivery. Global typed preferences and revisioned account metadata are shared with CLI. Native integration/visual acceptance remains open in [Phase 1 evidence](../research/phase-1-execution.md). Consumers: Core, CLI, Calendar, Search, Triage, MIME and Platform.

Revision 8 applies D014 and the [Flight Progress v2 reference](../research/flight-progress-v2.md). Phase 1 adapts the native mail/calendar/settings interface, with confirmed read-on-open, Done clearing unread and stars, untimed Delay, and the browsable timeline only in Mail. Mail accounts stay separate; Calendar supplies combined permitted details. Durable drafts, conversation-only Sent and real local preferences replace prototype shortcuts. Consumers: Core, Triage, Search, Calendar, CLI, Platform and Local assistance.

Revision 7 records the next Phase 0 integration checkpoint. Explicit configured-category assignment now joins the shared snapshot actions, with actual QML mouse input and independent CLI readback. Qt-exported formatting and attachments round-trip through persisted Core/MIME capture; the interactive composer remains plain text. Consumers: Core, CLI, MIME, Triage and Platform.

Revision 6 records D012’s implemented experimental slice. The Qt reader, Unicode plain-text draft, shared account/search state, read-only week/footer and settings shell now run against the engine. The Qt formatting experiment and shared MIME builder are tested separately; no live sending is enabled. Consumers: Core, CLI, Calendar, Platform and MIME. See [GUI evidence](../../gui/README.md).

Revision 5 applies D011: the initial-phase trio guides the shell and inline reply; settings is plumbing only, with pictured switches ignored per founder clarification. D009's text-first, user-owned actions and paired-client boundaries remain. Affected consumers: Core, Calendar, Platform, Triage, Search and CLI.

Owner: E — Mail UI. These are living specifications of accepted requirements and proposed behavior; exact types, libraries and unresolved details are not committed. See [contract workflow](README.md).

## Build boundary

| Boundary | Responsibility |
| --- | --- |
| Owns | Top mailbox navigation, resizable mail split and placement of the persistent bottom calendar strip; conversation/reading interactions, human drafts and signatures |
| Consumes | Core paged views and operation states; Search matching-message results; triage projections; Calendar's bounded date/occurrence view and actions; optional local summaries; Platform dialogs/renderer facilities |
| Produces | Explicit account/member-scoped user commands and authored draft content; visible loading, pending, coverage and failure states |
| Independent start | Mock local API and realistic paginated data; apply the founder's supplied design after review without provider/model network calls |

Related: [core](core-storage.md), [triage](triage.md), [search](search.md), [platform](platform.md), [HTML research](../research/html-mail.md).

Revision 3 records the closer nonfinal mail/calendar direction and the final-design handoff gates. Affected consumers/partners: Core and Triage (member flags, counts and actions), Search (collapsed matching members), Calendar (shared footer), Local assistance (summary states), Platform (renderer, focus/scaling) and CLI (the same command semantics). No shared API or accepted action mapping is introduced.

## Design constraint

The [Flight Progress v2 HTML prototype](../research/flight-progress-v2.md) is the current Phase 1 reference, superseding the initial trio's settings and footer treatment. Build the native GUI and CLI together against the completed Phase 0 foundation. Settings now includes functional local preferences and account display names; real account connection/removal and model installation remain later work. Done/Delayed are always user choices. Done clears unread and stars on its targeted existing members; Delay is an untimed star hold. Neither action archives mail, and a model cannot issue either action.

The same opinionated layout, components and visual identity apply on Windows, macOS and Linux. The [visual design brief](../ui-ux.md) fixes mailbox selection at the top, a resizable split mail workspace and a browsable calendar timeline at the bottom of every mail screen. The full Calendar and Settings have no bottom timeline. The HTML reference guides the compact shared top bar, left conversation list/right reader, typography and inline reply. Adapt its dimensions for usable native scaling and compact windows. Platform input and accessibility must work.

Propose a conversation list beside the reader/composer, with independent scrolling, usable minimum pane sizes and remembered split position. Keep the calendar strip visible across all mail scopes, search, reading and composition, including during scrolling/resizing. Mail UI owns placement and consumes Calendar's event projection; it must not create a second event/reminder store. Preserve the existing draft's account identity when switching mailboxes.

Stars must visibly mean deferred action, with proposed explanatory copy and honest hold age from [triage](triage.md). Read state and action completion remain distinct.

The list groups review, delayed and done mail; its reader uses Reply/Delay/Done controls, category chips and collapsed earlier messages. Proposed Phase 1 grouping is unread first, otherwise starred, otherwise done; a mixed conversation retains its hold marker even while shown in review. These are projections of message flags, with no separate completion state. Done uses one shared snapshot operation to clear unread and stars on existing targeted members; new replies are excluded. Delay/Undelay uses native star state and adds no timer or scheduled resurfacing. Label list/group counts as conversations and underlying account totals as messages, with coverage and pending-state context. See [Triage](triage.md) for the proposed target and count rules.

A collapsed thread must preserve access to all members and reveal the exact matching search message. Read-on-open is confirmed. Proposed trigger: once a user-activated conversation loads successfully, submit the shared read operation against that loaded member snapshot, including its collapsed members. Prefetch, background refresh, automatic selection/restore, summaries and CLI read queries do not mark mail read; later-arriving members are excluded. This is the chosen interaction convention, not proof that every member was inspected. Keep the open conversation readable after its flags change. Optional summaries are absent in ordinary Phase 1 use; later enabled summaries show source, stale and failure state. Attachment availability and sync status reflect actual coverage.

## Mail workflows

The inline reply shows recipient, explicit From and subject beneath received content/attachments. Keep every draft tied to its original account and revision through conversation/account/settings navigation and restart. Phase 1 covers replies to existing conversations with authored plain text/formatting, signatures, retained attachment selection, explicit discard and shared synthetic submission capture; captured or pending work must not be presented as delivered. Discard is an explicit durable draft operation. New composition, reply-all/forward, recipient/alias editing and new-file import follow later. Live sending is later; the prototype's volatile draft string and permanently pending Send simulation are not the implementation.

Collapse hides the inline composer while retaining its authored fields; explicit Reply restores it. A saved draft may expand on its first load unless the restored current conversation explicitly records a collapsed reply. A stale save keeps the local text, HTML, signature and attachments visible. Confirmed **Save my version** fetches the latest revision and submits the retained fields through the ordinary optimistic save; another racing edit can conflict again. **Reload saved draft** replaces local fields only after a successful read and cancels any previously queued navigation or capture. Transient retry alone never resolves an authored conflict by overwriting the other version.

Before creating a capture ID, the adapter queries conversation-scoped `capture_history` with `unresolved_only: true` and a bounded first record. Core filters prepared, in-flight and acceptance-unknown records before pagination. Recovery reuses that record's immutable operation ID, draft revision and synthetic outcome; it does not replace or recapture a newer authored draft. The UI explains when it is checking an earlier saved revision. A failed recovery query creates no new capture. An unresolved retry does not start another transport attempt or claim delivery.

The broader product action set is new conversation, reply, reply-all, forward, read/unread, star/unstar, archive where supported, trash/restore, provider folder/label actions, attachment handling and bulk selection. Provide an explicit From identity and durable drafts. To/Cc/Bcc are normal composition fields in the later expanded composer. Readable text and human composition with per-account signatures are required early; Phase 1 supplies restrained formatting, while best-effort received HTML display comes later. Provider-synced drafts remain an implementation-scope detail. Selecting or creating provider labels/folders is part of category setup.

An alias is an additional sending address associated with an account. It is not a confirmed need, so use each account's normal sending address for MVP and defer alias management. A reply must still select the receiving account correctly.

Conversation rows open the messages inline as part of the conversation. Inbox, Sent and search results all remain conversation-based and reveal the relevant message/snippet. Adapt the prototype's individual Sent rows to this existing browsing rule. Local storage still preserves individual messages so replies, flags, attachments and provider synchronization remain correct. Timed follow-up and inferred awaiting-reply tracking are outside Phase 1.

Proposed threading: use Gmail thread IDs for Google accounts; use reference headers with conservative fallback when the later IMAP phase starts. Subject equality alone must not combine unrelated mail. Never merge conversations across accounts. Define whether a bulk action affects the entire conversation or the matching folder subset, and make that choice predictable.

## Interaction refinements from exec-buddy

Adopt the useful product patterns with the following proposed expression in our existing mail/calendar screens:

- Click a sender to apply an address-based, account-scoped filter with a visible removable chip. Configured category chips filter conversations the same way.
- Restore the selected conversation, expanded message, search/filter state and scroll position separately for each account when switching views or reopening. Missing/deleted items fall back predictably without switching account context.
- Keep read/star/category changes responsive and show pending/failure state; background refresh must not reset the user's action or discard their reading position. In an Unread filter, keep the open conversation readable until the user moves on even when its read state changes.
- Make readable human correspondence the early reading route. Prefer an available plain-text MIME part and safely extract readable text from HTML-only mail; preserve originals and attachments. Later offer best-effort HTML viewing for newsletters/automated mail without promising browser-level layout accuracy. Raw/source inspection stays secondary.
- Offer People/Automated filtering only when the user configures corresponding categories. This uses the same manual/local-model category system, with visible unclassified mail and persistent total counts.

This is product inspiration, not a port of exec-buddy's code, visuals, generic integrations, restricted history windows, or individual-message browsing model.

## Shared application behavior

Phase 1 settings supplies the updated page layout and persists local account display names/signatures, row density, calendar/grid hours, timeline hours, week start and selected-calendar visibility. Calendar defaults to combined permitted details from connected accounts. Its explicit source selection is independent of the selected mailbox; switching mail accounts does not filter the combined calendar. Hiding a selected calendar changes neither mail scope nor provider permissions. Return restores the prior view/selection/draft; the timeline is shown only in Mail. Account freshness, index and offline coverage must describe actual local state. Real OAuth, account removal and model downloads/configuration remain later work; their prototype simulations must not claim success. Omit timed Delay settings because Delay remains untimed. See the [Phase 1 plan](../plans/phase-1.md) for the bounded local delivery.

UI commands use the same account-scoped core operations exposed by the [CLI](cli.md). The UI does not own provider/sync logic or require a separate agent automation surface. Built-in AI still cannot write or send email; external command callers use their own explicit workflow and authority.

## Composition boundary

Provide plain-text and HTML composition with per-account signatures. Construct standards-compliant alternatives where appropriate and retain the user's authored content in drafts. The default sender is the current account; alias management is deferred. Sanitize pasted rich content and keep the composing surface separate from untrusted received HTML.

The composer exposes no model-generated writing, rewrite, autocomplete or send path. Local summaries are clearly identified in the received-message reader and retain access to their sources; text-first reading is the initial normal path; later HTML display is optional per message and never replaces the preserved source.

## Acceptance gates

- Enter/leave settings without changing account identity, losing the prior mail/calendar position or discarding a draft. Persist the Phase 1 local preferences and verify their visible effect after restart. Keep the timeline on Mail only; do not introduce real account/model setup side effects.
- Select mailboxes at the top and resize the split without losing draft, selection or reading position. Verify the browsable bottom timeline on Inbox, Unread, Starred, folders/categories, search, reader and composer views, including a date other than today. Its event identity and selected-calendar scope agree with Calendar and survive mailbox changes.
- Show unread and starred states independently, including older deferred action and first-observed star age. Read-on-open changes only unread; Delay changes only its star hold; replying changes neither flag. Done explicitly clears both on its existing-member snapshot. Test mixed groups, honest counts, stale requests and new arrivals through both clients.
- Mark a successfully user-opened conversation read once for the loaded snapshot, retain its selection while groups update, and keep later replies unread. Failed loads, prefetch, background refresh, automatic restore and CLI queries must leave flags unchanged.
- Use a paginated, million-message fixture without loading the entire mailbox into the UI. Open the exact matching message from search and preserve the account, selected member and scroll position across refresh/reopen.
- Initially read plain-text and safely extracted HTML-only fixtures; copy/select text, handle links/attachments and keep long conversations bounded. Later HTML display is judged on readable content, links and useful images rather than pixel fidelity; scripts, privileged bridges and unrequested resources remain prohibited.
- Compose, save/reopen and send a human-written plain-text/HTML draft with signature, recipients and attachments from the correct account. Recovery follows Core's send states, including acceptance unknown. No AI writing controls or draft access exist.
- Triage and file while background sync/inference runs; keep the open conversation readable when it leaves Unread, show pending/error state and preserve honest account totals.
- Before building the affected UI slice, review its final design and resolve entry points absent from the mockups: Inbox/Unread/Starred, folders/categories, drafts/sent, new composition, Reply-all/Forward, setup and account-scoped search. Exercise mixed read/star groups and collapsed search matches without inventing new provider states. Confirm loading, offline, summary-off, failure, compact-window and focus/scaling states.

## Open before affected behavior ships

Validate the human reply editor and updated native layout on required platforms. Phase 1 remains local and synthetic: no real Google connection, model execution, legacy protocols or received HTML renderer. The action meanings and timeline placement above are settled; the proposed snapshot trigger, group precedence, count labels and Delay target must be named in the implementation packet and tested. Refine reply/draft navigation, search scope, restrained formatting and signature surfaces in Phase 1. New/Reply-all/Forward, recipient editing and provider-synchronized drafts require their later workflow design; local durability is required now. See [Slint](../research/slint-ui.md) for retained candidate-specific history.
