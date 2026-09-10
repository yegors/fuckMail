# Research and implications

Checked 2026-09-10 against the primary sources linked below. These are documentation findings and engineering recommendations, not measurements of this app or diagnoses of other clients. API limits and publishing requirements need rechecking against the actual project before implementation/release.

## Large Gmail accounts

Gmail supports an initial retrieval followed by changes obtained through a history cursor. History can expire; an invalid cursor returns HTTP 404 and requires full synchronization. Recommendation: resumable history import plus ongoing incremental updates, with a reconciliation path that preserves local drafts and pending operations. [Gmail synchronization](https://developers.google.com/workspace/gmail/api/guides/sync)

The current documentation distinguishes new quotas effective May 1, 2026 from grandfathered project quotas. It lists 6,000 units per minute per user per project, with `messages.get` costing 20 units and `messages.list` costing 5. Illustratively, 100,000 individual gets consume 2,000,000 units: about 333 minutes (5.6 hours) at that rate, before listing, other operations, retries, or network limits. This is a quota-only lower bound for that retrieval strategy, not an import estimate or universal limit. Confirm actual quotas and any billing changes in the chosen Cloud project. [Gmail usage limits](https://developers.google.com/workspace/gmail/api/reference/quota)

This motivates recent-mail priority, fair scheduling across accounts, and avoiding a full metadata fetch followed by another full fetch for every historical message. The founder requires all attachments offline, so include their retrieval and quota costs in the actual import plan. Measure message count, text size, attachment size, and label membership separately; mailbox gigabytes alone are insufficient to size the work.

Gmail labels can apply to messages within threads. The local model should retain message-level labels and provider thread IDs, deriving any conversation view without multiplying message bodies for each label. [Labels](https://developers.google.com/workspace/gmail/api/guides/labels), [threads](https://developers.google.com/workspace/gmail/api/guides/threads)

## Google authorization and notifications

Google's installed-app flow uses the system browser and a local redirect, with PKCE available. Credentials belong in the native core. A shipped desktop application cannot keep an embedded client secret confidential. [Desktop OAuth](https://developers.google.com/identity/protocols/oauth2/native-app)

`gmail.modify` is a restricted scope covering normal read/compose/send operations without immediate permanent deletion. Public distribution and server access affect verification/assessment requirements; personal use has a documented verification exception with limits. Recommendation: validate authorization with the founder's personal and Workspace accounts early, including administrator restrictions. A local-only design does not automatically settle public-app verification. [Gmail scopes](https://developers.google.com/workspace/gmail/api/auth/scopes), [verification and exceptions](https://developers.google.com/identity/protocols/oauth2/production-readiness/restricted-scope-verification)

An external OAuth project left in Testing normally receives seven-day refresh tokens for these scopes. A successful one-time login is therefore insufficient evidence of a viable daily-driver configuration. [OAuth token expiration](https://developers.google.com/identity/protocols/oauth2#expiration)

Gmail push uses Cloud Pub/Sub, and Google recommends polling-based synchronization for installed user devices. Calendar push requires a reachable HTTPS callback. The required local-only architecture points to adaptive polling while running, manual refresh, and immediate catch-up after wake/reconnect. Polling frequency and battery use must be measured. The founder requires closing to quit, so this app performs no polling or reminder delivery while closed. [Gmail notifications](https://developers.google.com/workspace/gmail/api/guides/push), [Calendar notifications](https://developers.google.com/workspace/calendar/api/guides/push)

## IMAP and SMTP

IMAP handles mailbox access and manipulation; mail submission is separate. Message identity must include the account, mailbox, UIDVALIDITY, and UID. A sequence number or Internet Message-ID is not a durable substitute. Negotiate server capabilities and support the older IMAP4rev1 servers still encountered, rather than requiring rev2 everywhere. [IMAP4rev2](https://www.rfc-editor.org/rfc/rfc9051), [mail submission](https://www.rfc-editor.org/rfc/rfc6409)

CONDSTORE/QRESYNC can reduce work when reconnecting, but they are capability-dependent. Use IDLE where supported for change hints and provide a bounded polling/reconciliation fallback. Folder moves, deletion semantics, sent-copy behavior, and OAuth/app-password requirements need provider fixtures. [IMAP resynchronization extensions](https://www.rfc-editor.org/rfc/rfc7162)

SMTP can leave a client uncertain whether a message was accepted when a connection times out. Proposed product behavior: persist an ambiguous-send state and reconcile where possible; do not blindly retry as if exactly-once delivery were guaranteed. [SMTP timeout and duplicate-delivery discussion](https://www.rfc-editor.org/rfc/rfc5321)

## Calendar interoperability

Google Calendar incremental synchronization uses sync tokens. An invalid token returns HTTP 410 and requires a new full synchronization. Keep collection scope consistent and commit the replacement cursor only after the required pages have been applied. [Calendar synchronization](https://developers.google.com/workspace/calendar/api/guides/sync)

CalDAV provides calendar access over WebDAV; iCalendar defines the event representation. WebDAV collection sync is a separate extension. CalDAV scheduling and email-delivered invitations add further protocols. Consequently, CRUD, synchronization, subscriptions, and invitation handling need separate capability tests. [CalDAV](https://www.rfc-editor.org/rfc/rfc4791), [iCalendar](https://www.rfc-editor.org/rfc/rfc5545), [collection sync](https://www.rfc-editor.org/rfc/rfc6578), [CalDAV scheduling](https://www.rfc-editor.org/rfc/rfc6638), [iMIP](https://www.rfc-editor.org/rfc/rfc6047)

Recurring series contain rules and exceptions. Google warns against updating every occurrence individually to change a series. Recommendation: preserve series identity and exceptions, expand only a bounded view window, and test occurrence/series edits independently. [Google recurring events](https://developers.google.com/workspace/calendar/api/guides/recurringevents)

Read-only URL subscriptions are a separate, simpler integration: periodically retrieve the iCalendar feed, cache a successfully parsed snapshot, and render it locally. They provide visibility, not an editable calendar or scheduling endpoint. The founder explicitly requires these in MVP and has deferred editable CalDAV. [iCalendar format](https://www.rfc-editor.org/rfc/rfc5545)

Calendar has per-project/per-user quotas and operational limits, including bursts of writes. Measure polling and recurrence edits with the actual project; avoid rewriting large sets of occurrences to represent a series edit. [Calendar usage limits](https://developers.google.com/workspace/calendar/api/guides/quota)

## Local search and storage options

| Candidate | Evidence | Project implication |
| --- | --- | --- |
| SQLite + FTS5 | Phrase, prefix, Boolean and proximity search, ranked results, configurable tokenization; trigram substring search has specific limitations | First benchmark candidate: relational filters and text search in one store; assess query latency during writes. [FTS5](https://sqlite.org/fts5.html) |
| SQLite + Tantivy | Embedded Rust search library with ranking, incremental indexing and typed fields; fuzzy-term queries are available | Candidate if agreed search quality/performance needs exceed FTS5; adds index consistency/recovery work and a Rust bridge if the app uses C++. [Tantivy](https://github.com/quickwit-oss/tantivy), [fuzzy queries](https://docs.rs/tantivy/latest/tantivy/query/struct.FuzzyTermQuery.html) |
| Notmuch/Xapian | Separates mail indexing/search from the UI and does not receive or send mail | Useful architectural reference; not a complete mail/calendar foundation. Windows packaging and fit would need investigation before reuse. [Notmuch](https://notmuchmail.org/) |

The confirmed search need is sender name/address and subject/body keywords within an account. This strengthens the case for starting with FTS5, while benchmarking conversation grouping and partial address/name matching. FTS5 substring matching is not the same as typo correction, and tokenization is not universal language understanding. Do not infer application performance from library marketing benchmarks.

SQLite WAL permits concurrent readers and a writer, but there is still only one writer at a time; checkpoint behavior and long reads affect performance. Recommendation: short transactions, bounded indexing batches, measured checkpointing, and local-disk storage. Use the backup facilities for coherent copies rather than copying a live database file alone. [SQLite WAL](https://sqlite.org/wal.html)

## Desktop implementation choices

The founder rejected the JavaScript ecosystem and requested a native cross-platform direction. Tauri/TypeScript and Electron are excluded from the current application shortlist. This leaves several practical desktop choices; avoiding JS does not require writing two complete applications.

| Option | Documented basis | Assessment for this project |
| --- | --- | --- |
| Qt 6 Widgets + C++ | Desktop controls, model/view lists and tables, native-style drawing and CMake integration | Fallback candidate with broad desktop facilities and a native rich-text composer. Use Widgets/C++ rather than introducing QML/JavaScript bindings. Assess component licenses and packaging with the prototype. [Qt Widgets](https://doc.qt.io/qt-6/qtwidgets-index.html), [QTextEdit](https://doc.qt.io/qt-6/qtextedit.html) |
| Rust + Slint | Rust integration, a declarative `.slint` UI language, Windows/macOS/Linux desktop support | Next candidate to evaluate following the founder's request. No JS application layer is needed; prototype visual quality, complex text editing, accessibility and HTML embedding before commitment. [Slint overview](https://docs.slint.dev/latest/docs/slint/), [desktop support](https://docs.slint.dev/latest/docs/slint/guide/platforms/desktop/) |
| C# + Avalonia | .NET/XAML cross-platform UI using its own renderer | Additional non-JS option if .NET is desirable. It uses managed application code and custom-drawn controls; do not describe it as platform-native widgets. [Avalonia](https://docs.avaloniaui.net/docs/welcome) |
| Separate platform UIs with a shared core | Architectural alternative: an OS-specific UI implementation per platform | Prefer only if exact platform controls/behavior justify maintaining two frontends and a further Linux path. |

Iced is another Rust GUI candidate, but the currently served API documentation explicitly describes it as experimental. It is not the first experiment for this desktop application. That is a project-fit judgment, not a claim that it cannot work. [Iced documentation](https://docs.iced.rs/iced/)

Cross-platform native toolkits often render their own controls with platform styling. A compiled desktop UI does not automatically mean that every control is an AppKit or WinUI control. The first candidate is one shared desktop codebase with platform integration, and it must still pass native input/accessibility tests.

## HTML mail without a JS application stack

Reading HTML messages and authoring formatted replies are distinct components. Qt's rich-text editor can edit plain/rich text and serialize HTML, but its document renderer supports a defined HTML/CSS subset. It is a plausible composer, not proof of full compatibility with received email layouts. [QTextEdit](https://doc.qt.io/qt-6/qtextedit.html), [supported HTML subset](https://doc.qt.io/qt-6/richtext-html-subset.html)

For more complete received-message rendering, Qt WebEngine offers a C++ interface backed by Chromium; an OS-specific HTML view is another integration direction. Restrict the renderer to message content, disable page scripts and privileged bridges, and control external resources. This introduces browser-engine code and resource costs, even though we author no JS application or npm pipeline. Qt exposes a setting for disabling JavaScript; that setting alone is not the complete isolation policy. [Qt WebEngine overview](https://doc.qt.io/qt-6/qtwebengine-overview.html), [renderer settings](https://doc.qt.io/qt-6/qwebenginesettings.html)

Eliminating browser engines entirely would narrow the rendering choices and may reduce HTML email fidelity. Evaluate that tradeoff with representative messages rather than quietly replacing HTML with plain text. The user's objection to the JS ecosystem does not by itself establish a ban on an isolated HTML renderer.

Current recommendation after the founder's visual-quality clarification: evaluate Rust + Slint + SQLite/FTS5 next, retaining Qt as a fallback. See the [Slint UI investigation](slint-ui-investigation.md) for published visual examples, the change to Slint's default style, and the unresolved reader/composer integration. Measure realistic conversation scrolling, sender/body search, composition, calendar interaction and message rendering during indexing. No toolkit is selected or benchmarked yet.
