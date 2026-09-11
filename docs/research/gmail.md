# Gmail: synchronization, authorization and quotas

Sources checked 2026-09-10; category sources rechecked 2026-09-11; reorganized 2026-09-11. Documentation/source evidence and proposals only; no app prototype, live account flow or benchmark has run. Recheck version-sensitive claims before implementation.

Build boundary: [Gmail contract](../contracts/gmail.md).

## Large Gmail accounts

Gmail supports an initial retrieval followed by changes obtained through a history cursor. History can expire; an invalid cursor returns HTTP 404 and requires full synchronization. Recommendation: resumable history import plus ongoing incremental updates, with a reconciliation path that preserves local drafts and pending operations. [Gmail synchronization](https://developers.google.com/workspace/gmail/api/guides/sync)

The current documentation distinguishes new quotas effective May 1, 2026 from grandfathered project quotas. It lists 6,000 units per minute per user per project, with `messages.get` costing 20 units and `messages.list` costing 5. Illustratively, 100,000 individual gets consume 2,000,000 units: about 333 minutes (5.6 hours) at that rate, before listing, other operations, retries, or network limits. This is a quota-only lower bound for that retrieval strategy, not an import estimate or universal limit. Confirm actual quotas and any billing changes in the chosen Cloud project. [Gmail usage limits](https://developers.google.com/workspace/gmail/api/reference/quota)

This motivates recent-mail priority, fair scheduling across accounts, and avoiding a full metadata fetch followed by another full fetch for every historical message. The founder requires all attachments offline, so include their retrieval and quota costs in the actual import plan. Measure message count, text size, attachment size, and label membership separately; mailbox gigabytes alone are insufficient to size the work.

Gmail labels can apply to messages within threads. The local model should retain message-level labels and provider thread IDs, deriving any conversation view without multiplying message bodies for each label. [Labels](https://developers.google.com/workspace/gmail/api/guides/labels), [threads](https://developers.google.com/workspace/gmail/api/guides/threads)

The configurable-category requirement should use Gmail user labels with explicit account mappings. Gmail documents that a label applied to a thread covers existing messages but is not automatically inherited by messages subsequently added to that thread. Recommendation: retain message-level assignments and classify new arrivals independently, preserving mixed conversation membership. Label assignment and Inbox removal are separate product actions; keep read/star state independent of either. [Label behavior](https://developers.google.com/workspace/gmail/api/guides/labels)

## Google authorization and notifications

Google's installed-app flow uses the system browser and a local redirect, with PKCE available. Credentials belong in the native core. A shipped desktop application cannot keep an embedded client secret confidential. [Desktop OAuth](https://developers.google.com/identity/protocols/oauth2/native-app)

`gmail.modify` is a restricted scope covering normal read/compose/send operations without immediate permanent deletion. Public distribution and server access affect verification/assessment requirements; personal use has a documented verification exception with limits. Recommendation: validate authorization with the founder's personal and Workspace accounts early, including administrator restrictions. A local-only design does not automatically settle public-app verification. [Gmail scopes](https://developers.google.com/workspace/gmail/api/auth/scopes), [verification and exceptions](https://developers.google.com/identity/protocols/oauth2/production-readiness/restricted-scope-verification)

An external OAuth project left in Testing normally receives seven-day refresh tokens for these scopes. A successful one-time login is therefore insufficient evidence of a viable daily-driver configuration. [OAuth token expiration](https://developers.google.com/identity/protocols/oauth2#expiration)

Gmail push uses Cloud Pub/Sub, and Google recommends polling-based synchronization for installed user devices. Calendar push requires a reachable HTTPS callback. The required local-only architecture points to adaptive polling while running, manual refresh, and immediate catch-up after wake/reconnect. Polling frequency and battery use must be measured. GUI close stops its polling/reminders. Under the new CLI scope, explicitly invoked headless sync/watch can operate separately; no polling continues after the final active session ends. [Gmail notifications](https://developers.google.com/workspace/gmail/api/guides/push), [Calendar notifications](https://developers.google.com/workspace/calendar/api/guides/push)

## Next experiment

P0-02: Validate durable Google desktop OAuth for personal and Workspace access; record actual quotas and resumable retrieval strategy. This planning work has not accessed any accounts.

## Category experiment

P0-07 (Gmail): exercise selecting/creating and remapping labels, archived unread totals, mixed conversation membership, new replies, renamed/deleted destinations and cross-client edits. Automatic category filing must preserve read/star state and respect newer manual changes. Compare the separate IMAP primary-folder behavior without treating labels and folders as equivalent.
