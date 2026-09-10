# Product scope

Status: founder requirements from the brief and both interview rounds, with engineering recommendations explicitly identified. See [remaining decisions](decisions-and-questions.md).

## Required MVP

| Area | Confirmed requirement |
| --- | --- |
| Platforms | Windows and macOS; Linux is nice to have |
| UI technology | Native desktop/cross-platform toolkit; avoid the JavaScript/TypeScript application ecosystem and web-app shell |
| Visual quality | An attractive, deliberately designed interface is required; exact visual style remains to be chosen |
| Account model | Multiple separate personal/work accounts; no unified mailbox |
| Mail presentation | Conversations are required and the only browsing mode; no separate individual-message mode |
| Interaction | Mouse-first; keyboard shortcuts are a secondary nicety |
| Integration | Prefer Gmail API and Google Calendar API; also support IMAP/SMTP |
| Search | Lightning-fast local search by sender name/address and keywords in subject/body |
| Offline | All messages and attachments, not just a recent window |
| Calendar views | Day, week, month and agenda |
| Calendar operations | Create/edit events, recurring events, invitations and RSVP, shared calendars and reminders |
| External calendars | Read-only subscriptions to shared iCalendar URLs |
| Composition | Plain-text/HTML email support and per-account signatures |
| Lifecycle | Closing the app quits it; no hidden background process, synchronization or reminder delivery afterward |
| Deployment | Local application; no app-operated remote server or processing service |
| Audience | Personal use first; may become open source later |

The founder currently uses only Google calendars and explicitly deferred editable CalDAV until after MVP. Read-only iCalendar URLs are required and must not be treated as equivalent to editable CalDAV.

## Mail workflows

The proposed minimal action set is new conversation, reply, reply-all, forward, read/unread, archive where supported, trash/restore, provider folder/label actions, attachment handling and bulk selection. Provide an explicit From identity and durable drafts. To/Cc/Bcc are normal composition fields. Plain-text/HTML reading and composition plus per-account signatures are required; propose a restrained formatting toolbar. Provider-synced drafts and folder/label creation remain implementation-scope details.

An alias is an additional sending address associated with an account. It is not a confirmed need, so use each account's normal sending address for MVP and defer alias management. A reply must still select the receiving account correctly.

Conversation rows open the messages inline as part of the conversation. Search results are conversation-based and reveal the specific matching message/snippet. Local storage still preserves individual messages so replies, flags, attachments and provider synchronization remain correct. This does not introduce a separate message-list mode.

Proposed threading: use Gmail thread IDs for Google accounts; use reference headers with conservative fallback for IMAP. Subject equality alone must not combine unrelated mail. Never merge conversations across accounts. Define whether a bulk action affects the entire conversation or the matching folder subset, and make that choice predictable.

## Search

The MVP search bar operates within the current account. Sender name/address, subject and body are the required indexed fields. Support ordinary keyword input with simple mouse-accessible field filters; quoted phrases are a proposed useful addition. Keep sender matching robust to display-name/address variations and evaluate partial names/addresses with real examples.

Results show conversation subject, relevant sender/date and matching text. Clicking a result brings the matching message into view. Relevance versus newest-first ordering needs a simple, consistent default. Proposed advanced operators can follow if they help, but an extensive query language is not an MVP requirement.

Full offline attachment storage does not imply searching inside PDF/Office files. Attachment-content extraction, OCR, typo tolerance, transliteration and semantic search are outside the required search scope. AI remains excluded. Language coverage and precise matching semantics need representative examples rather than an assumption of English-only mail.

Show inventory, message-text indexing and attachment-download coverage separately. A partially synchronized account must never be presented as fully searchable or fully available offline.

## Complete offline mail

Eventually download and retain all messages and attachments within the account's synchronized mailbox scope. Recent conversations and user-requested content have priority, while older bodies and attachments continue in a resumable background queue. On-demand retrieval is an acceleration path during initial import, not the final retention policy.

Never silently evict old attachments or replace full history with a rolling window. Estimate and display storage needs, preserve disk headroom, and pause with an actionable explanation if space is insufficient. Store attachments and the message content required for complete offline reading and reply/forward. Do not claim complete offline availability until required content is present.

Offline draft creation is proposed as mandatory reliability behavior; durable queues for sends and supported edits are recommended. Remote deletion and permanent local archival behavior are separate: the full offline copy is not automatically an independent backup. Trash/spam inclusion and retention after remote deletion remain explicit follow-up details.

## Calendar

Calendar correctness is a dedicated MVP gate because unreliable Google integration is a primary reason for building the product. Keep Google calendar operations native to the Google API and event permissions explicit. Shared calendars must show events and permit edits only to the extent the account is allowed.

| Workflow | Proposed acceptance detail implementing the confirmed requirement |
| --- | --- |
| Views | Day/week/month/agenda use the same event state and preserve calendar identity; calendar visibility is selectable |
| Create/edit | Timed/all-day events, correct target calendar, time zone, title/location/description and attendees; deletion/cancellation handled correctly |
| Recurrence | Preserve series and exceptions; support common rules and editing one occurrence, the series, or following occurrences; unusual rules must not be silently simplified |
| Invitations | Organize, update and cancel meetings; attendee accept/tentative/decline; changed times and cancellations propagate without duplicate invitations |
| Shared calendars | Honor read-only, write and limited-visibility access; recover cleanly if permissions change |
| Reminders | Local notifications while the app is running, without duplicates after refresh/restart; none after app close |
| iCalendar URLs | Add/remove/show/hide read-only feeds, refresh and cache locally, render recurrence/time zones; no editing or RSVP through the feed |

Read-only subscriptions connect directly to the subscribed URLs, with no intermediary service. Preserve the last valid feed if refresh fails, show freshness/error state, and reconcile removals only after a successful complete refresh. Treat private subscription URLs as secrets in logs and diagnostics.

Mail invitations and calendar events must converge on the same provider-backed event state. Avoid separately emailing an invitation when the Google Calendar operation already handles attendee notifications. Shared-calendar use does not automatically require an ACL administration UI. Calendar overlay defaults and which running-app notifications are enabled remain design choices.

## App lifecycle

Closing the main app window exits the application. Save drafts and synchronization checkpoints before exit; do not continue running in a tray/menu-bar helper. Mail synchronization and calendar reminders stop when closed. Reopening resumes work and reconciles calendar changes before scheduling upcoming reminders. Explain this behavior once in the reminder settings; do not add an always-running service to compensate for it.

## Exclusions

Confirmed: tasks, widgets, a contacts application, unified mailboxes, an individual-message browsing mode, MVP AI, app-operated remote services, and background activity after closing. Editable CalDAV is explicitly post-MVP. No model integrations, embeddings or speculative AI infrastructure.

Proposed deferrals: mobile/web clients, plugins, broad theming, tracking/read receipts, campaigns, snooze, scheduled send, rule builders, resource/booking administration, settings sync between machines, attachment-content search, and public distribution work. Existing server rules continue to operate without an in-app rule editor.

Exchange-specific APIs, JMAP, POP3, archive import/export and message encryption/signing require a concrete need before entering the plan. The founder subsequently rejected the JavaScript ecosystem and requested a native cross-platform approach. Use maintained native desktop tools without a JS/TypeScript application or npm/Node build pipeline. HTML email rendering is a separate component decision and must not turn the application UI into a web app.

## Trust and recovery

Proposed baseline: credentials in the OS credential store, account-scoped records, isolated HTML rendering and user-controlled remote content. Routine diagnostics contain no message contents, credentials or private calendar URLs. Rebuilds and resyncs preserve drafts and pending operations. App-level mail-store encryption remains a separate choice from storing credentials securely.
