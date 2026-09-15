# Product scope

Updated 2026-09-14. This is the high-level scope established by the brief, interviews and subsequent founder decisions. [North star](north-star.md) explains why; [contracts](contracts/README.md) hold the moving behavior and engineering specifications. [Status](status.md) distinguishes planned from built.

## Required Google-first MVP

| Area | Confirmed requirement |
| --- | --- |
| Platforms | Windows, macOS and Linux required; Ubuntu, Arch and Omarchy included |
| UI technology | Native cross-platform toolkit; QML expressions accepted; no Node.js or separate JavaScript/TypeScript compilation/bundling pipeline |
| Visual design | One attractive, opinionated design shared across all OSes; founder-supplied design precedes stack selection; no OS-native visual matching |
| Account model | Multiple separate personal Gmail and Google Workspace (corp) accounts; mailbox selection at the top; no unified mailbox |
| Mail presentation | Conversation-only browsing in a resizable split mail view; no separate individual-message mode |
| Interaction | Mouse-first GUI and a real CLI from the first usable milestone; grow both together to cover Google workflows before legacy protocols |
| Daily triage | Read-on-open; user-selected Done clears unread and stars; Delay is an untimed hold. Aim for zero unread/starred through actual review and work |
| Organization | User-defined Gmail category/scope labels for human correspondence, bots, lists and personal definitions of noise; IMAP folder mappings later |
| Local AI | Optional Ollama classification of incoming mail and summaries of messages/conversations, with discovered models and editable Settings prompts; vLLM deferred under D051; no AI email writing, rewriting, autocompletion or sending |
| Integration | Gmail API and Google Calendar API for all current consumer/Workspace accounts; durable OAuth and Workspace restrictions are required setup. IMAP/SMTP follow founder dogfooding |
| Search | Lightning-fast local search by sender name/address and keywords in subject/body |
| Offline | All messages and attachments, not just a recent window |
| Calendar views | Combined full permitted details across connected accounts; a browsable bottom timeline only in Mail, plus a dedicated day/week/month/agenda calendar |
| Calendar operations | Create/edit events, recurring events, invitations and RSVP, shared calendars and reminders |
| External calendars | Read-only iCalendar subscriptions deferred until after Google GUI/CLI dogfooding |
| Reading/composition | Human correspondence and replies first, with signatures and attachments; limited formatted composition remains intended. Best-effort received HTML display comes later; source content is preserved |
| Lifecycle | GUI close ends its session/work/reminders; separately invoked CLI commands have explicit lifetimes; no default always-on service |
| Deployment | Local GUI and headless CLI, shared core/store; smooth prebuilt installation without developer tools; no app-operated remote processing service |
| Audience | Personal workflow first; may become open source later; external agent tools can use the CLI |
| Technology judgment | Prefer modern tools when useful; demonstrably better mature technology takes precedence over novelty |

All current mailboxes and calendars use Google APIs. IMAP, SMTP and external read-only iCalendar subscriptions start only after the founder has dogfooded working Google GUI and CLI workflows. They remain planned later capabilities. CalDAV remains separately deferred. MIME parsing and recurrence/invitation handling needed by Google remain in the first product.

## The product experience

Human correspondence is the primary reading/reply workflow. Classifiers distinguish people, bots/lists and user-defined noise, using configured account/category/scope tags. Those tags may help the user choose what to look at; they never choose Done/Delayed or change read/star state. No automatic Gmail system Spam/Trash routing follows from a personal noise tag.

The first usable milestone has both a small CLI and a GUI. Later iterations add command coverage alongside GUI workflows. Received HTML readability is added later as a best-effort feature; complex newsletter layout fidelity is not an early delivery gate. Safe text extraction must still make HTML-only messages accessible.

Work in separate accounts and browse conversations. Review unread mail, use a star when leaving action for later, then remove it when that action is complete. The UI should make the deferral and remaining work clear. The desired end-of-day state is zero unread and zero starred across the agreed scope of each account. Filing, filtering and summaries cannot disguise outstanding work or incomplete synchronization.

Configure categories using Gmail labels first; IMAP folder mappings are later work. Apply them manually, accept a local model's suggestions or enable a qualified auto-apply policy. D052's first delivery uses Ollama with independent summary/classification models and editable prompts, existing account labels and Human/Automated/Unknown classification; vLLM remains deferred. Optional summaries support reading; humans write email for humans. All manual work remains available with assistance absent or disabled.

Find old conversations by sender or source-text keywords and open fully synchronized messages and attachments offline. Google Calendar correctness includes ordinary meetings, recurrence, invitations and shared permissions. Read-only URL feeds are later work. Closing quits the GUI and stops its jobs/reminders; explicitly invoked CLI sessions can operate independently. Full offline storage is not automatically an independent archival backup.

## Product refinements already carried forward

Useful exec-buddy patterns now have proposed expressions in the mail/calendar contracts: sender/category filters, restoring each account's reading position, temporary action holds, compact current/next events, expandable all-day rows, calendar search, and a Join action that acknowledges the occurrence reminder. Preserve conference links separately from physical locations. These ideas fit the existing screens; they do not add another integration dashboard.

## Design and stack

This is a personal, opinionated app with one visual design across operating systems. The founder is working with Claude Design using the [visual design brief](ui-ux.md). Top mailbox selection and a resizable mail split are confirmed. The [Flight Progress v2 HTML prototype](research/flight-progress-v2.md) now guides Phase 1, with compact sans-serif treatment, inline replies, combined calendar details, a browsable timeline only in Mail and expanded settings. Phase 1 adds real local preferences; provider/model management and destructive account removal remain later. Earlier PNGs and their settings-plumbing limit remain Phase 0 history. The core-language assessment recommends a Rust engine/CLI and C++20/Qt UI over private local IPC, with SQLite/FTS5. This remains a technical proposal. Validate the reader/composer, CLI, all required platforms and installation before claiming the baseline proven. The [development plan](development-plan.md) sequences those gates; it does not turn mockup action labels, Fastmail examples or model status into new initial-phase requirements. Earlier Slint-first/Qt-fallback ordering is historical. Windows/macOS/Linux input, accessibility, scaling and lifecycle correctness remain required. See [desktop research](research/desktop-stack.md) for the options, including the QML discussion.

## CLI and external workflows

A full CLI is an MVP interface, including accounts/configuration, mail search/read/organization, human-authored drafts and explicit send, calendar operations, sync/progress, optional local assistance and structured change consumption. It works without the GUI, shares the same store/operations, and exposes stable machine-readable results for external callers. See [CLI](contracts/cli.md).

Built-in AI remains local classification/summarization only. Third-party agents are user-controlled callers, with their own model and workflow policy; the app does not silently send data to them or supply an AI writing tool. Human authorship remains the outgoing-mail preference, while the CLI cannot infer where supplied text was written. No in-app agent host, MCP server or general automation engine is required.

## Exclusions

Confirmed: tasks, widget dashboards, a contacts application, unified mailboxes, an individual-message browsing mode, app-operated remote services, and hidden GUI-owned work after closing. Explicit independent CLI operation is in scope. IMAP/SMTP and external iCalendar subscriptions follow Google dogfooding; editable CalDAV is separately deferred. Built-in AI is limited to optional local classification and summarization: no AI writing/rewriting/autocomplete/send, assistant chat, task generation, meeting briefings, hosted-model fallback, embeddings or semantic search. The earlier blanket AI exclusion is superseded by the founder's 2026-09-11 instruction.

Proposed deferrals: mobile/web clients, plugins, broad theming, tracking/read receipts, campaigns, mail snooze, scheduled send, general-purpose rule builders, resource/booking administration, settings sync between machines, attachment-content search, and broad public-launch/store work. Required installers, runtime dependencies, CLI discovery, update/recovery and applicable signing/licensing are part of MVP delivery. Configurable category mappings and their limited auto-apply policy are in scope; an arbitrary automation engine is not. Existing server rules continue to operate without an in-app rule editor.

Exchange-specific APIs, JMAP, POP3, archive import/export and message encryption/signing require a concrete need before entering the plan. The founder subsequently rejected the JavaScript ecosystem and requested a native cross-platform approach. Use maintained native desktop tools without Node.js or a separate JS/TypeScript compilation pipeline; QML syntax and native Qt build tools are acceptable. HTML email rendering is a separate component decision and must not turn the application UI into a web app.

## Where behavior is specified

- [Core/storage](contracts/core-storage.md): identities, complete offline retention, durable drafts/operations and recovery.
- [Mail UI](contracts/mail-ui.md), [search](contracts/search.md) and [triage](contracts/triage.md): reading/composition, retrieval, unread/stars and provider categories.
- [Gmail](contracts/gmail.md) and [IMAP/SMTP](contracts/imap-smtp.md): provider capabilities and synchronization.
- [Calendar](contracts/calendar.md): event views, edits, invitations, feeds and reminders.
- [Local assistance](contracts/local-ai.md): optional classification/summaries, manual precedence and local-only processing.
- [CLI](contracts/cli.md): full command access, machine-readable output and external callers.
- [Platform](contracts/platform.md) and [quality](contracts/quality.md): required OS support, privacy, lifecycle, smooth installation and measured acceptance.

Exact implementation types, remaining UX details, provider inventory, retention/privacy settings and numeric budgets remain unresolved in their owning contracts. They are not new accepted requirements merely because a proposal is documented.
