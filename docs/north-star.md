# North star

**Review the mail, finish the action, clear unread and starred mail for the day, and trust my calendar—with any old conversation immediately available offline.**

This wording incorporates the founder's brief, both interview rounds, and the 2026-09-11 daily-triage clarification. The current client is eM Client; the founder reports poor large-mailbox performance, an overcomplicated UI, and unreliable Google Calendar integration. Those are the problems this product must solve.

## Product principles

1. **Everyday interactions stay fast as history grows.** Reading, switching accounts, searching, composing, and opening the calendar must remain responsive during background work.
2. **Mail and calendar are the product.** There is no tasks area, contacts application, widget dashboard or assistant chat. Optional local classification and summaries support existing mail workflows.
3. **Simple screens, correct behavior.** Hide protocol machinery from normal use, while making actionable failures understandable. Minimalism must not lose drafts, send from the wrong account, or damage recurring meetings.
4. **Local search earns trust.** Sender name/address and subject/body keywords find conversations in the current account. When indexing is incomplete, the app explains its coverage instead of presenting a misleading empty result.
5. **Work stays in its account.** Personal and separate job mailboxes are distinct contexts. There is no unified mailbox. Conversations are the only mail browsing mode; mouse interaction is primary and shortcuts are secondary.
6. **Work has an explicit lifetime.** Closing quits the GUI and its work/reminders. A separately invoked CLI command or foreground watch can run without the GUI; it stops when completed or canceled. No always-running background service is installed by default.
7. **Visual quality is part of the product.** The founder explicitly values an attractive interface. Use the same opinionated design on every OS, shaped around the founder's own workflow. Typography, spacing, hierarchy and mouse feedback need deliberate design; OS-native visual conformity and general-purpose customization are not goals. Nonfinal mockups guide the technical recommendation while the founder continues visual design.
8. **Unread is unreviewed; starred is action deferred.** Starring an email leaves action for later, even if work has begun. Make that outstanding work clear in the UI. Aim to finish each day with unread and starred at zero; older holds stay discoverable. Categorization, filtering or automation must not disguise unfinished work by clearing its state.
9. **Humans write email for humans.** A configured local model may classify incoming mail and summarize messages/conversations. Writing, rewriting, autocompleting and sending email remain outside AI scope; the full manual workflow always works.
10. **Organization belongs in the mailbox.** Use the user's configurable Gmail labels or IMAP folders, with understandable provider-specific behavior. Manual and model-assisted classification share that organization, and manual corrections take precedence.
11. **Two interfaces, one product.** A complete GUI and a feature-rich CLI use the same accounts, data and behavior. External agents can use the CLI without a GUI session. Installation delivers both smoothly; users do not assemble a developer environment. Prefer modern tools where useful, and mature technology when it serves the product better.
12. **Time stays in view.** Today's calendar is a persistent bottom strip on every mail screen, alongside a dedicated full calendar. Mailboxes are selected at the top, and the main mail view is a resizable split. See the [visual design brief](ui-ux.md).

## Proposed daily-driver acceptance story

Connect several accounts. Recent conversations appear while history and attachments download. Switch to the right account, reply using the right identity, search for a years-old message by sender or keywords, open its attachment offline once downloaded, and manage meetings without calendar inconsistencies. Disconnect the network: all fully synchronized mail and attachments remain available, and unsent work survives a restart. Reconnect: changes reconcile and any unresolved failure is visible.

Work through Unread, classify with native labels/folders, and hold ongoing action with a star. Complete the action and remove the star. Repeat with local assistance disabled and with optional local classification/summaries enabled. Return from the calendar to the same conversation and reading position; join a meeting from its reminder without repeated alerts. An empty filtered list or an incomplete import must not masquerade as a finished day.

This is the proposed daily-driver acceptance story on Windows, macOS and Linux, including Ubuntu, Arch and Omarchy. Applicable workflows must also work through the full CLI. Calendar acceptance includes the agreed views, event editing, recurrence, invitations/RSVP, shared calendars, reminders and read-only external feeds.

## How we judge success

The primary product outcome is that the founder can replace eM Client for everyday mail and calendar use and finish daily triage with zero unread and zero starred messages across the agreed scope of each account. Counts remain honest about pending changes, filters and incomplete synchronization. This is a workflow objective, not an automatic cleanup policy. Performance is measured on a representative large corpus during synchronization and optional local inference. Search quality is measured against sender and keyword retrieval tasks. Processing and storage remain local; connections go directly to required providers, subscribed calendar URLs, and the configured same-machine model runner.

See [performance](contracts/quality.md) for candidate budgets. They are targets to validate, not benchmark results.

## Scope filter

Before adding a feature, identify the recurring mail or calendar problem it solves, the smallest useful interaction, its resource cost, and how success will be observed. If it has no concrete connection to the founder's workflow, leave it out.

The only built-in AI scope is optional local classification of incoming mail and summaries of messages/conversations. User-controlled external agents access the separate CLI; this does not add an in-app writing assistant or dictate an external tool's model. No AI email authoring, assistant chat, generated tasks, meeting briefings, cloud-model fallback, embeddings or semantic search. See [triage](contracts/triage.md) and [local assistance](contracts/local-ai.md) for the permitted behavior and boundaries.
