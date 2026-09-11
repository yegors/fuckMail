# Mail UI, reader and human composer

State: Draft · Revision: 3 · Updated: 2026-09-11 · Implementation: not started.

Owner: E — Mail UI. These are living specifications of accepted requirements and proposed behavior; exact types, libraries and unresolved details are not committed. See [contract workflow](README.md).

## Build boundary

| Boundary | Responsibility |
| --- | --- |
| Owns | Top mailbox navigation, resizable mail split and placement of the persistent bottom calendar strip; conversation/reading interactions, human drafts and signatures |
| Consumes | Core paged views and operation states; Search matching-message results; triage projections; Calendar's bounded today/occurrence view and actions; optional local summaries; Platform dialogs/renderer facilities |
| Produces | Explicit account/member-scoped user commands and authored draft content; visible loading, pending, coverage and failure states |
| Independent start | Mock local API and realistic paginated data; apply the founder's supplied design after review without provider/model network calls |

Related: [core](core-storage.md), [triage](triage.md), [search](search.md), [platform](platform.md), [HTML research](../research/html-mail.md).

Revision 3 records the closer nonfinal mail/calendar direction and the final-design handoff gates. Affected consumers/partners: Core and Triage (member flags, counts and actions), Search (collapsed matching members), Calendar (shared footer), Local assistance (summary states), Platform (renderer, focus/scaling) and CLI (the same command semantics). No shared API or accepted action mapping is introduced.

## Design constraint

The same opinionated layout, components and visual identity apply on Windows, macOS and Linux. The [visual design brief](../ui-ux.md) fixes mailbox selection at the top, a resizable split mail workspace and today's calendar strip at the bottom of every mail screen. A full calendar remains available. The [closer mockups](../research/design-references.md) guide a compact shared top bar and a left conversation list/right reader. Ratio, compact adaptation, palette and detailed behavior remain open. Final mockups are expected closer to UI implementation; this direction is not a frozen screen specification. Platform input and accessibility still need to work.

Propose a conversation list beside the reader/composer, with independent scrolling, usable minimum pane sizes and remembered split position. Keep the calendar strip visible across all mail scopes, search, reading and composition, including during scrolling/resizing. Mail UI owns placement and consumes Calendar's event projection; it must not create a second event/reminder store. Preserve the existing draft's account identity when switching mailboxes.

Stars must visibly mean deferred action, with proposed explanatory copy and honest hold age from [triage](triage.md). Read state and action completion remain distinct.

The closer list groups review, delayed and done mail; its reader uses Reply/Delay/Done controls, category chips, an optional local summary and collapsed earlier messages. Treat these as presentation proposals. Delay may express the native star hold; it adds no timed snooze. Resolve Done before implementation: do not silently bundle read/unstar/archive, create another completion state or include future replies in an old action. Mixed unread/starred conversations retain both facts and truthful account totals regardless of group placement. Label conversation counts separately from underlying message totals.

A collapsed thread must preserve access to all members and reveal the exact matching search message. Expansion/selection/read triggers remain undecided; a summary or hidden member is not evidence of review. Keep optional summaries absent when disabled/unavailable, and show source, stale and failure state when applicable. Attachment availability and sync status reflect actual coverage, not the sample badge text.

## Mail workflows

The proposed minimal action set is new conversation, reply, reply-all, forward, read/unread, star/unstar, archive where supported, trash/restore, provider folder/label actions, attachment handling and bulk selection. Provide an explicit From identity and durable drafts. To/Cc/Bcc are normal composition fields. Plain-text/HTML reading and composition plus per-account signatures are required; propose a restrained formatting toolbar. Provider-synced drafts remain an implementation-scope detail. Selecting or creating provider labels/folders is part of category setup.

An alias is an additional sending address associated with an account. It is not a confirmed need, so use each account's normal sending address for MVP and defer alias management. A reply must still select the receiving account correctly.

Conversation rows open the messages inline as part of the conversation. Search results are conversation-based and reveal the specific matching message/snippet. Local storage still preserves individual messages so replies, flags, attachments and provider synchronization remain correct. This does not introduce a separate message-list mode.

Proposed threading: use Gmail thread IDs for Google accounts; use reference headers with conservative fallback for IMAP. Subject equality alone must not combine unrelated mail. Never merge conversations across accounts. Define whether a bulk action affects the entire conversation or the matching folder subset, and make that choice predictable.

## Interaction refinements from exec-buddy

Adopt the useful product patterns with the following proposed expression in our existing mail/calendar screens:

- Click a sender to apply an address-based, account-scoped filter with a visible removable chip. Configured category chips filter conversations the same way.
- Restore the selected conversation, expanded message, search/filter state and scroll position separately for each account when switching views or reopening. Missing/deleted items fall back predictably without switching account context.
- Keep read/star/category changes responsive and show pending/failure state; background refresh must not reset the user's action or discard their reading position. In an Unread filter, keep the open conversation readable until the user moves on even when its read state changes.
- Make full HTML reading the normal route when available, with a plain-text fallback in the message menu. Raw HTML/source inspection is secondary, not a permanent three-tab reading interface.
- Offer People/Automated filtering only when the user configures corresponding categories. This uses the same manual/local-model category system, with visible unclassified mail and persistent total counts.

This is product inspiration, not a port of exec-buddy's code, visuals, generic integrations, restricted history windows, or individual-message browsing model.

## Shared application behavior

UI commands use the same account-scoped core operations exposed by the [CLI](cli.md). The UI does not own provider/sync logic or require a separate agent automation surface. Built-in AI still cannot write or send email; external command callers use their own explicit workflow and authority.

## Composition boundary

Provide plain-text and HTML composition with per-account signatures. Construct standards-compliant alternatives where appropriate and retain the user's authored content in drafts. The default sender is the current account; alias management is deferred. Sanitize pasted rich content and keep the composing surface separate from untrusted received HTML.

The composer exposes no model-generated writing, rewrite, autocomplete or send path. Local summaries are clearly identified in the received-message reader and retain access to their sources; plain-text reading is a secondary display option.

## Acceptance gates

- Select mailboxes at the top and resize the split without losing draft, selection or reading position. Verify the bottom today strip on Inbox, Unread, Starred, folders/categories, search, reader and composer views. Its event identity and actions agree with Calendar.
- Show unread and starred states independently, including older deferred action and first-observed star age. Starring, reading and replying must not imply completion of another state.
- Use a paginated, million-message fixture without loading the entire mailbox into the UI. Open the exact matching message from search and preserve the account, selected member and scroll position across refresh/reopen.
- Read HTML and plain-text fixtures; copy/select text, handle links/attachments and show bounded progress for long conversations. The isolated renderer has no scripts or privileged app bridge; coordinate its resources and shutdown with Platform.
- Compose, save/reopen and send a human-written plain-text/HTML draft with signature, recipients and attachments from the correct account. Recovery follows Core's send states, including acceptance unknown. No AI writing controls or draft access exist.
- Triage and file while background sync/inference runs; keep the open conversation readable when it leaves Unread, show pending/error state and preserve honest account totals.
- Before building the affected UI slice, review its final design and resolve entry points absent from the mockups: Inbox/Unread/Starred, folders/categories, drafts/sent, new composition, Reply-all/Forward, setup and account-scoped search. Exercise mixed read/star groups and collapsed search matches without inventing new provider states. Confirm loading, offline, summary-off, failure, compact-window and focus/scaling states.

## Open before affected behavior ships

Validate the recommended toolkit/renderer/editor with the nonfinal references on every required platform. Production screens and interactions wait for the relevant founder-supplied final design; provisional functional surfaces and feasibility fixtures need not. Resolve conversation/group ordering, labeled count units, read-on-open and collapsed-member behavior, Delay/Done action targets, search scope, access to hidden navigation, restrained formatting and signature format. Decide whether drafts synchronize to other clients; local durability is required. See [Slint](../research/slint-ui.md) for retained candidate-specific UI gates, not a selected implementation.
