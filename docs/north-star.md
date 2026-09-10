# North star

**Open the app, work in the right account, find any conversation immediately, trust my calendar, and get on with my day—even with decades of mail stored offline.**

This wording synthesizes the founder's brief and both interview rounds. The current client is eM Client; the founder reports poor large-mailbox performance, an overcomplicated UI, and unreliable Google Calendar integration. Those are the problems this product must solve.

## Product principles

1. **Everyday interactions stay fast as history grows.** Reading, switching accounts, searching, composing, and opening the calendar must remain responsive during background work.
2. **Mail and calendar are the product.** There is no tasks area, contacts application, widget dashboard, or AI surface in the MVP.
3. **Simple screens, correct behavior.** Hide protocol machinery from normal use, while making actionable failures understandable. Minimalism must not lose drafts, send from the wrong account, or damage recurring meetings.
4. **Local search earns trust.** Sender name/address and subject/body keywords find conversations in the current account. When indexing is incomplete, the app explains its coverage instead of presenting a misleading empty result.
5. **Work stays in its account.** Personal and separate job mailboxes are distinct contexts. There is no unified mailbox. Conversations are the only mail browsing mode; mouse interaction is primary and shortcuts are secondary.
6. **Closing means quitting.** There is no hidden background client. Imports and indexing resume after reopening; mail sync and calendar reminders run only while the app is running.
7. **Visual quality is part of the product.** The founder explicitly values an attractive interface. Typography, spacing, hierarchy and mouse feedback need deliberate design on both Windows and macOS; visual polish is not a post-MVP extra.

## Proposed daily-driver acceptance story

Connect several accounts. Recent conversations appear while history and attachments download. Switch to the right account, reply using the right identity, search for a years-old message by sender or keywords, open its attachment offline once downloaded, and manage meetings without calendar inconsistencies. Disconnect the network: all fully synchronized mail and attachments remain available, and unsent work survives a restart. Reconnect: changes reconcile and any unresolved failure is visible.

This is the proposed daily-driver acceptance story on both Windows and macOS. Calendar acceptance includes the agreed views, event editing, recurrence, invitations/RSVP, shared calendars, reminders and read-only external feeds.

## How we judge success

The primary product outcome is that the founder can replace eM Client for everyday mail and calendar use without returning to another client for an agreed core workflow. Performance is measured on a representative large corpus, including while synchronization runs. Search quality is measured against sender and keyword retrieval tasks, not only query speed. All application processing and storage remain local; connections go directly to the required providers and subscribed calendar URLs.

See [performance](performance.md) for candidate budgets. They are targets to validate, not benchmark results.

## Scope filter

Before adding a feature, identify the recurring mail or calendar problem it solves, the smallest useful interaction, its resource cost, and how success will be observed. If it has no concrete connection to the founder's workflow, leave it out.

AI remains outside the MVP even if an implementation library makes it easy to add. Future AI ideas require a separate product decision; no model integration, embeddings, or speculative AI infrastructure is needed now.
