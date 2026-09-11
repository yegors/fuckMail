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

## Phase 0 investigation: authentication and a resumable provider adapter

Checked 2026-09-11. These are inputs to P0-02 and P0-05, not executed tests or a dependency lock.

The mixed personal-Gmail/Workspace audience needs an External OAuth configuration unless separate projects are deliberately chosen. Internal is restricted to one organization and cannot cover the founder's consumer account. Publishing state, verification and Workspace trust are separate controls: External Testing normally limits users and token lifetime; administrators can block even verified apps. Google's current overview documents exceptions for Workspace apps marked Trusted, which must be confirmed with each actual organization. [OAuth app states](https://developers.google.com/identity/protocols/oauth2/production-readiness/overview)

Personal use has a verification exception, but may show an unverified-app warning and is subject to user limits. Planning a durable personal configuration does not require turning public verification into MVP work. Before the daily-driver trial, record the project audience/publishing state, applicable exception, granted scopes and each organization's access policy. Tokens may still expire or be revoked; reconnect is a required recovery path. [Personal-use exception](https://developers.google.com/identity/protocols/oauth2/production-readiness/restricted-scope-verification), [token expiration](https://developers.google.com/identity/protocols/oauth2#expiration)

Start evaluation with Rust `reqwest` for explicit HTTP requests and `oauth2` for authorization-code/PKCE and refresh handling. Generated Google bindings are an alternative if they preserve cancellation, raw error details and pagination control; neither approach supplies our sync state machine. Keep authorization in the engine with the system browser, short-lived loopback callback and OS credential references. The CLI must initiate the same flow without starting the GUI. [reqwest](https://docs.rs/reqwest/latest/reqwest/), [oauth2](https://docs.rs/oauth2/latest/oauth2/), [installed-app flow](https://developers.google.com/identity/protocols/oauth2/native-app)

| Investigation | Inputs and output | Acceptance and fallback |
| --- | --- | --- |
| P0-02A: Is personal installation sustainable? | First prepare a scope/method map and setup checklist without credentials. Later, with account access authorized, use one consumer and one Workspace account, each connected separately. Record consent, refresh after restart, revocation/reconnect and admin restrictions; never store token values in research. | A one-time login does not pass. Record the token-lifetime policy and later sustained refresh evidence beyond the Testing lifetime where applicable. If admin access is unavailable, keep that row blocked while mock-based work proceeds; do not bypass organization policy or call Google support complete. |
| P0-02B: Can import resume without harming local intent? | Synthetic paginated Gmail responses for recent/history retrieval, attachments, quota responses, invalid history and interleaved local changes. Output a checkpoint/reconciliation trace and counts against the input manifest. | Restart at page and blob boundaries without missing or duplicating content; expiry rebuild preserves drafts and pending mutations. If strategy exceeds observed quota, change scheduling/retrieval order, not full-history scope. Actual throughput remains a later live measurement. |
| P0-05: Does the HTTP/OAuth candidate fit the engine? | Build-only evaluation later using mocked token/API endpoints, cancellation and sanitized errors. Record selected Cargo features, TLS roots, license inventory and package dependencies. | Same behavior through GUI and CLI sessions; refresh is coordinated, canceled sessions stop their work, secrets stay out of output. Change the adapter/client candidate if it cannot expose needed control; do not move provider logic into QML. |

Mail's candidate `gmail.modify` scope must be checked against the method inventory; calendar scopes belong to the [calendar investigation](calendar.md). Shared Google authorization can request the agreed scopes without requesting unrelated Google products. A UI demo and local fixtures can progress before Cloud project or Workspace administrator access, but they cannot satisfy live interoperability acceptance.
