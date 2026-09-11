# Product scope

Updated 2026-09-11. This is the high-level scope established by the brief, interviews and subsequent founder decisions. [North star](north-star.md) explains why; [contracts](contracts/README.md) hold the moving behavior and engineering specifications. [Status](status.md) distinguishes planned from built.

## Required MVP

| Area | Confirmed requirement |
| --- | --- |
| Platforms | Windows, macOS and Linux required; Ubuntu, Arch and Omarchy included |
| UI technology | Native desktop/cross-platform toolkit; avoid the JavaScript/TypeScript application ecosystem and web-app shell |
| Visual design | One attractive, opinionated design shared across all OSes; founder-supplied design precedes stack selection; no OS-native visual matching |
| Account model | Multiple separate personal Gmail and Google Workspace (corp) accounts; mailbox selection at the top; no unified mailbox |
| Mail presentation | Conversation-only browsing in a resizable split mail view; no separate individual-message mode |
| Interaction | Mouse-first GUI; full feature-rich CLI for humans and third-party agent flows |
| Daily triage | Aim for zero unread and zero starred mail at day's end; a star means deferred action, made explicit in the UI |
| Organization | Configurable categories mapped to native Gmail labels or IMAP folders; manual or optional local-model classification |
| Local AI | Optionally configured Ollama/vLLM classification of incoming mail and summaries of messages/conversations; no AI email writing, rewriting, autocompletion or sending |
| Integration | Prefer Gmail API and Google Calendar API for consumer Gmail and Google Workspace; desktop OAuth setup, including Workspace admin consent/restrictions, is required work; also support IMAP/SMTP |
| Search | Lightning-fast local search by sender name/address and keywords in subject/body |
| Offline | All messages and attachments, not just a recent window |
| Calendar views | Today's calendar as a persistent bottom strip on all mail screens, plus a dedicated day/week/month/agenda calendar |
| Calendar operations | Create/edit events, recurring events, invitations and RSVP, shared calendars and reminders |
| External calendars | Read-only subscriptions to shared iCalendar URLs |
| Composition | Plain-text/HTML email support and per-account signatures |
| Lifecycle | GUI close ends its session/work/reminders; separately invoked CLI commands have explicit lifetimes; no default always-on service |
| Deployment | Local GUI and headless CLI, shared core/store; smooth prebuilt installation without developer tools; no app-operated remote processing service |
| Audience | Personal workflow first; may become open source later; external agent tools can use the CLI |
| Technology judgment | Prefer modern tools when useful; demonstrably better mature technology takes precedence over novelty |

The founder currently uses only Google calendars and explicitly deferred editable CalDAV until after MVP. Read-only iCalendar URLs are required and must not be treated as equivalent to editable CalDAV.

## The product experience

Work in separate accounts and browse conversations. Review unread mail, use a star when leaving action for later, then remove it when that action is complete. The UI should make the deferral and remaining work clear. The desired end-of-day state is zero unread and zero starred across the agreed scope of each account. Filing, filtering and summaries cannot disguise outstanding work or incomplete synchronization.

Configure categories using native Gmail labels or IMAP folders. Apply them manually, accept a local model's suggestions or enable a defined auto-apply policy. Optional local Ollama/vLLM summaries support reading; humans write email for humans. All manual work remains available with assistance absent or disabled.

Find old conversations by sender or source-text keywords and open fully synchronized messages and attachments offline. Calendar correctness includes ordinary meetings, recurrence, invitations, shared permissions and read-only URL feeds. Closing quits the GUI and stops its jobs/reminders; explicitly invoked CLI sessions can operate independently. Full offline storage is not automatically an independent archival backup.

## Product refinements already carried forward

Useful exec-buddy patterns now have proposed expressions in the mail/calendar contracts: sender/category filters, restoring each account's reading position, temporary action holds, compact current/next events, expandable all-day rows, calendar search, and a Join action that acknowledges the occurrence reminder. Preserve conference links separately from physical locations. These ideas fit the existing screens; they do not add another integration dashboard.

## Design and stack

This is a personal, opinionated app with one visual design across operating systems. The founder is working with Claude Design using the [visual design brief](ui-ux.md). Top mailbox selection, a resizable mail split and today's persistent bottom calendar strip are confirmed; exact styling and pane arrangement remain open. Three supplied nonfinal mockups guide the Qt Quick/QML UI direction. The subsequent core-language assessment recommends a Rust engine/CLI and C++20/Qt UI over private local IPC, with SQLite/FTS5. This remains a technical proposal. Validate the reader/composer, CLI, all required platforms and installation before locking versions. Earlier Slint-first/Qt-fallback ordering is historical. Windows/macOS/Linux input, accessibility, scaling and lifecycle correctness remain required. See [desktop research](research/desktop-stack.md) for the options, including the QML discussion.

## CLI and external workflows

A full CLI is an MVP interface, including accounts/configuration, mail search/read/organization, human-authored drafts and explicit send, calendar operations, sync/progress, optional local assistance and structured change consumption. It works without the GUI, shares the same store/operations, and exposes stable machine-readable results for external callers. See [CLI](contracts/cli.md).

Built-in AI remains local classification/summarization only. Third-party agents are user-controlled callers, with their own model and workflow policy; the app does not silently send data to them or supply an AI writing tool. Human authorship remains the outgoing-mail preference, while the CLI cannot infer where supplied text was written. No in-app agent host, MCP server or general automation engine is required.

## Exclusions

Confirmed: tasks, widget dashboards, a contacts application, unified mailboxes, an individual-message browsing mode, app-operated remote services, and hidden GUI-owned work after closing. Explicit independent CLI operation is in scope. Editable CalDAV is explicitly post-MVP. Built-in AI is limited to optional local classification and summarization: no AI writing/rewriting/autocomplete/send, assistant chat, task generation, meeting briefings, hosted-model fallback, embeddings or semantic search. The earlier blanket AI exclusion is superseded by the founder's 2026-09-11 instruction.

Proposed deferrals: mobile/web clients, plugins, broad theming, tracking/read receipts, campaigns, mail snooze, scheduled send, general-purpose rule builders, resource/booking administration, settings sync between machines, attachment-content search, and broad public-launch/store work. Required installers, runtime dependencies, CLI discovery, update/recovery and applicable signing/licensing are part of MVP delivery. Configurable category mappings and their limited auto-apply policy are in scope; an arbitrary automation engine is not. Existing server rules continue to operate without an in-app rule editor.

Exchange-specific APIs, JMAP, POP3, archive import/export and message encryption/signing require a concrete need before entering the plan. The founder subsequently rejected the JavaScript ecosystem and requested a native cross-platform approach. Use maintained native desktop tools without a JS/TypeScript application or npm/Node build pipeline. HTML email rendering is a separate component decision and must not turn the application UI into a web app.

## Where behavior is specified

- [Core/storage](contracts/core-storage.md): identities, complete offline retention, durable drafts/operations and recovery.
- [Mail UI](contracts/mail-ui.md), [search](contracts/search.md) and [triage](contracts/triage.md): reading/composition, retrieval, unread/stars and provider categories.
- [Gmail](contracts/gmail.md) and [IMAP/SMTP](contracts/imap-smtp.md): provider capabilities and synchronization.
- [Calendar](contracts/calendar.md): event views, edits, invitations, feeds and reminders.
- [Local assistance](contracts/local-ai.md): optional classification/summaries, manual precedence and local-only processing.
- [CLI](contracts/cli.md): full command access, machine-readable output and external callers.
- [Platform](contracts/platform.md) and [quality](contracts/quality.md): required OS support, privacy, lifecycle, smooth installation and measured acceptance.

Exact implementation types, remaining UX details, provider inventory, retention/privacy settings and numeric budgets remain unresolved in their owning contracts. They are not new accepted requirements merely because a proposal is documented.
