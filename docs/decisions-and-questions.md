# Decisions and remaining design details

Updated 2026-09-10 after both founder interview rounds and the native-UI preference. Core product scope is established. Technology selections and numeric performance budgets remain proposals pending measurement.

## Accepted product decisions

| ID | Decision | Basis |
| --- | --- | --- |
| C01 | Mail and calendar only; multiple large mailboxes | Original brief |
| C02 | Windows and macOS required; Linux nice to have | First interview |
| C03 | Separate personal/work accounts; no unified mailbox | First interview |
| C04 | Conversation-only mail; mouse-first interaction; shortcuts secondary | First interview |
| C05 | Fast local sender name/email and subject/body keyword search | First interview |
| C06 | All messages and attachments stored offline | First interview |
| C07 | Prefer Google APIs; IMAP and SMTP also required | Original brief and first interview |
| C08 | Google Calendar with day/week/month/agenda, create/edit, recurrence, invitations/RSVP, shared calendars and reminders | First interview |
| C09 | Read-only iCalendar URL subscriptions in MVP; editable CalDAV later | Second interview |
| C10 | Local processing/storage and direct provider connections; no app-operated server | First interview |
| C11 | Personal use first; possibly open source later; prefer modern tools | First interview |
| C12 | Close means quit; no hidden background sync or reminders after closing | Second interview, applied to app lifecycle |
| C13 | Plain-text/HTML mail support and per-account signatures | Second interview |
| C14 | Exclude tasks, widgets, contacts and AI from MVP | Original brief |
| C15 | Avoid the JS ecosystem; use a native cross-platform UI, with no JS/TypeScript application or npm/Node pipeline | Subsequent founder clarification; supersedes initial lack of stack restrictions |
| C16 | Visual quality matters; investigate Rust + Slint + SQLite and real UI examples before choosing | Subsequent founder request; investigation is not stack acceptance |

## Known problem and scale

The founder uses eM Client and reports that large-mailbox performance, UI complexity and unreliable Google Calendar integration are its main failings for this workflow. This records the founder's experience, not a measured diagnosis of that product.

A supplied local disk-usage snapshot totals about 25 GB, with large directory subtotals around 2.9 GB, 7.6 GB and 15 GB. Do not sum nested entries again, infer exact account count, or assume every server attachment is downloaded. Message counts, complete server sizes and non-Google provider names are not yet known. The founder instructed us to assume high-end machines.

Aliases were explained as additional From addresses associated with an account. No requirement was accepted; use each account's normal sending address and defer alias management.

## Proposed engineering decisions

| ID | Proposal | Evidence needed |
| --- | --- | --- |
| P01 | Shared local core with a small cross-platform desktop UI | Native interaction, lifecycle and rendering experiments on Windows/macOS |
| P02 | Benchmark SQLite/FTS5 first; evaluate Tantivy if needed | Sender/body/subject retrieval, conversation grouping, concurrent ingestion and recovery measurements |
| P03 | Evaluate Rust + Slint + SQLite/FTS5 next; retain Qt as a fallback | Visual quality, HTML-reader/composer integration, accessibility, dependency and performance experiments on both OSes; see [Slint investigation](slint-ui-investigation.md) |
| P04 | Recent conversations first, then full resumable history and attachment acquisition | Actual quotas, mailbox inventory, disk-space and throughput measurements |
| P05 | Proposed 100 ms warm-search core budget, 200 ms visible-search budget, million-message primary fixture | High-end reference devices and repeatable benchmark results |
| P06 | Google mail/calendar integration in parallel; IMAP/SMTP before complete MVP | Shared contracts, test fixtures and actual non-Google server compatibility |
| P07 | Short transactions, durable operations and staged reconciliation | Restart, invalid-cursor, low-disk and ambiguous-send experiments |
| P08 | Native application shell; isolate an HTML renderer to received message content as needed | Representative email fidelity, script-disabled rendering, process isolation and memory measurements; zero browser-engine dependencies are not assumed |

## Remaining design details, not another broad interview

These can be resolved during their workstream before the affected behavior ships:

- **Provider inventory and scale:** account count, message counts and non-Google IMAP/SMTP services; actual free disk and Windows/macOS reference hardware.
- **Mail UX:** reading-pane geometry, conversation ordering, thread-level action semantics, simple formatting controls, signature format, and whether drafts must synchronize to other clients.
- **Search:** representative names/addresses and keywords, supported languages, partial-match semantics, and relevance versus newest-first ordering. Global search is not assumed.
- **Calendar:** overlay defaults, common recurrence editing controls, running-app alert preferences, reminder horizon and handling of invitations appearing in multiple accounts. Editable CalDAV is already deferred.
- **Retention/privacy:** trash/spam scope, behavior after provider deletion, app-level mail-store encryption, remote images, and backup/export expectations. Complete offline mail must not silently become recent-only storage.
- **Distribution:** minimum OS versions/architectures, personal build packaging, later license choice and public-release signing/updating/verification.

Record subsequent decisions with supporting founder input or experiment results and update affected documents together. No benchmark has run and no mailbox has been accessed as part of this planning work.
