# Core and storage

## D107 — conversation people, readable previews and one message format (Core59, schema35)

**People.** Schema35 adds `people(email, name)` and `message_people`.
- The link-index pass records each message's From, To and Cc mailboxes, up to 100 per message, beside its link hosts.
- The upgrade restarts that pass from the first message, once. A message's people count once the pass reaches it.

`mail_list` rows gain two fields:
- `others`: how many distinct other people are on the conversation.
- `other_names`: up to 12 of their display names, or their addresses, in order of first appearance.

Others leave out:
- the account's address;
- addresses the account sent from in that conversation;
- the row's shown sender;
- no-reply mailboxes (`noreply`, `no-reply`, `donotreply`, `do-not-reply`, `mailer-daemon`).

Mailboxes compare lowercased and without a `+tag`. At gmail.com and googlemail.com they also compare without dots.

**Previews.** A row's `snippet` comes from readable text:
- A stored body that is HTML source (a doctype or a common element start) is converted to its text when read.
- New imports read HTML sent as a text/plain part as HTML, never indexing its markup.
- Web addresses are removed with their lead-ins, such as "(also on https://…)", their angle brackets, including an empty `<>` left behind, and leftover punctuation.

Search matching is unchanged.

**Summaries.** A row's `summary` is the current summary of the preview's message, now including a partial one (Local assistance22).

**Message format.** Preferences gain `message_format`: `html` (default) or `text`. It is validated and saved like the other enum preferences.

**HTML view.** The sanitiser is version 4. Its part choice:
1. The first non-empty `text/html` part in the HTML body.
2. Otherwise, any other part labelled `text/html`, such as a digest part or one with an unknown transfer encoding.

`message_html`, `message_source`, the link index and new imports all use this part. When removing quoted history leaves nothing visible, the quote stays. Cached views are named with the version, so they are rebuilt.

Consumers: engine, Mail UI76, CLI45 and Quality77. Fixtures0/3 and protocol1 unchanged.

## D106 — reminder identity bounds (Core58)

`calendar_reminders` and `calendar_reminder_ack` accept projected calendar IDs up to 1,024 UTF-8 bytes and serialized occurrence IDs up to 8,192 UTF-8 bytes. Validate these directly rather than first using the generic 256-byte local identifier helper. Empty, oversized and control-containing identifiers still fail. Exact account/calendar/occurrence/start receipt matching, permission checks, due windows, snoozing and dismissal remain unchanged. Consumers: Calendar31, GUI reminder polling, CLI reminder commands and Quality76. No schema, fixture or protocol change.

## D105 — HTML view, message images, link index and remote preferences (Core57, schema34)

**HTML view.** `message_html(account, {id, source_version})` is read-only and allowed for archived accounts. It reads the retained MIME directly, under the existing raw limit, and chooses the first real `text/html` part by content type. Plain-only mail returns `has_html: false`, and `message_source` uses the same choice.

The engine's `fm_mail::html_view` sanitiser (version 1) rebuilds the HTML from allowlists:
- tags, attributes and CSS properties and values;
- scoped `<style>` rules with prefixed classes and ids;
- `@media` only;
- links as `fm-link:N` (http, https and mailto only);
- images and CSS backgrounds as `fm-res:N`.

It removes quoted history unless the message is a forward, and applies the planned limits:
- 4 MiB HTML, 50,000 nodes, depth 256, table depth 16;
- 2,000 CSS rules, 256 KiB CSS;
- 500 resources, 2,000 links, 8 KiB URLs, 2 MiB `data:` URIs.

The result has:
- `too_large`;
- `html`, or `html_path` to a private cache file when over 512 KiB (the newest 16 files are kept);
- `resources[{id, kind: remote|cid|data|insecure, host?, alt, width?, height?, image?}]` and `links[{id, href, scheme, host}]`;
- `trackers`, `quote_removed` and `sanitizer_version`.

Images at most 3×3, zero-sized or hidden are counted in `trackers` and never listed. `image {path, width, height}` appears for a resource already in the image cache.

**Images.** `message_image(account, {id, source_version, resource, cached_only?})` is the only way to get an image a message refers to. It rechecks the account, the message, the current source version and that the resource belongs to that version's manifest; the newest 16 manifests are kept.
- It returns `state` `ready` (with `path`, `width` and `height`), `not_cached`, `blocked` (with `by`), `insecure`, `proxy_unavailable` or `failed` (with `reason`).
- `cid:` parts (up to 25 MiB), `data:` images and remote images are decoded in memory-safe Rust: PNG, JPEG, GIF first frame or WebP by magic bytes, at most 16 megapixels. They are re-encoded as PNG at most 1600 px wide. SVG is refused.
- Images are cached under `<profile>/cache/images/<account-key>/`, within 256 MiB in total with the oldest evicted first. Only pixels are stored.
- `cached_only` never reaches the network. `insecure` (`http://`) sources are never requested.
- The engine's remote-content runtime owns the network policy; see [Platform 19](platform.md).
- `remote_image_cache({op: stats|clear})` reports `{bytes, limit}` or clears the cache.

**Link index (schema34).** One post-reset migration adds three tables, mirrored in `fresh-schema.sql`:
- `link_hosts(id, rhost UNIQUE)`, where `rhost` is the host with its labels reversed;
- `message_link_hosts(host, message_rowid)`, which cascades on message delete, plus an index by message;
- `link_index_state(singleton, indexed_rowid)`.

An engine task indexes messages in rowid order, 100 per batch, off the writer. It reads each retained source's HTML and text links, at most 200 hosts per message, and falls back to reading text when no source is retained.
- `link_host_summary(account, {host, conversation?})` returns `{host, messages, complete}` for messages in every account outside that conversation.
- `link_pattern_counts({patterns})` returns `{counts, complete}` for normalised blocklist patterns; a `*.domain` pattern is one reversed-host range scan.

A schema33 profile upgrades once and starts indexing from the beginning. Older binaries refuse schema34.

**Preferences.**
- `blocked_hosts`: at most 1,000 unique `{pattern, source: settings|link, added_at}` entries. A pattern must already be normalised: a lowercase ASCII host without `www.`, or `*.domain`.
- `remote_images_from`: at most 2,000 unique `{account, email}` entries.
- `remote_proxy {enabled, host, port 1–65535, username ≤256}`. The host must be a host name.

The proxy password is never a preference. `remote_proxy_password({op: status|set|clear})` stores it in the OS vault and returns only `saved`.

Consumers: Mail UI74, CLI44, Platform19 and Quality75; Search7, Calendar30, fixtures0/3 and protocol1 unchanged. [Execution record](../research/html-mail.md#d105--html-reader-implementation-2026-09-24).

## D101 — search grammar, people lookup, Done months and view keys (Core56, schema33)

`conversations` and `drafts` parse the search text with the [Search 7 grammar](search.md#query-grammar-revision-7) and return `search_terms`. `conversations` rejects `is:` together with a different `status` field and `has:attachment` together with `has_attachment:false`; `label:` resolves to the account's category ID and ANDs with any `category` field. `from:` values bind as escaped `LIKE` patterns on the sender name and address, or on `recipient` in Sent; `label:` binds one category slot. Both join the existing numbered bindings, and `matching_message_ids` applies them to members.

`conversations` accepts `month_zone`, an IANA zone name, and `collapsed_months` (at most 256 unique `YYYY-MM` values, requiring `month_zone`). With a zone, each Done conversation belongs to the month of its latest message in that zone, and the response adds `done_months:[{month,count}]` over the whole filtered set, newest first, with `undated` last; without one it is `null`. A collapsed month's rows leave paging like a collapsed group and count toward neither `expanded_conversation_count` nor the page, while `done_months` keeps full counts. Cursor scope includes both fields.

`search_senders(account, {query, folder, limit})` is a read-only method on the allowlist of Core and the engine; see [Search 7](search.md#query-grammar-revision-7). `view_save` accepts two more account view keys: `recentSearches`, at most six non-blank strings of up to 512 bytes without line breaks, and `collapsedSections`, at most 258 unique values from `review`, `held`, `done:undated` and `done:YYYY-MM`. The 16 KiB view limit is unchanged. No table, index or migration changes. Consumers: Mail UI71, CLI43, Search7 and Quality72. [Execution record](../research/prototype-september-23.md#d101--fourth-export-2026-09-24).

## D098 — Calendar mail-count range and preferences revision (Core55, schema33)

Schema33 is the first post-reset migration. It adds the partial index `google_mail_coverage_by_date(account,internal_date_ms) WHERE deleted=0`, and fresh profiles create it directly. `calendar_mail_activity` reads provider coverage through that index, so counting a six-week window touches only its date range instead of the account's whole coverage. An engine at schema32 upgrades once in place; newer schemas are still refused. `status` adds `preferences_revision`, the saved preferences revision, so clients can skip reloading unchanged preferences on every poll. Consumers: Mail UI69 (GUI polling) and Quality70; CLI42 unchanged, since its `status` output only gains a field. [Execution record](../research/phase-2-calendar.md#d098--calendar-view-performance-2026-09-23).

## D097 — first label import and watcher cursor (Core54, schema32)

Binding a new account inserts `labels_initial_import:<account>` into `meta` in the binding transaction; reconnecting an existing account does not. `google_labels_initial_import` runs the name merge with request `initial-import` while that key exists; any completed merge deletes it, as does missing mail editing permission. When the import adds labels it sets the watcher's requeue bound to that account's first message. `local_model_arrivals` stores its cursor and consumes the requeue bound in `meta`. Retention pruning and account removal lower the requeue bound to the highest remaining message row in their transactions. No table or index changes. Consumers: GUI and CLI through the shared engine; Gmail27, Local assistance20, Quality69 unchanged.

## D096 — audit corrections (Core53, schema32)

`mail_undo` records human intent only for flags, filing labels and categories whose snapshot value differs from the current message, and live delivery sends only the intents written at that undo's new message revision. Unchanged labels keep provider/model state, so undoing a flag change neither blocks later model labels nor queues Gmail label removals. A message already matching its snapshot gets no delivery row.

Shortening retention selects older mail by Gmail's internal received date when coverage records it, else by the header date, matching acquisition's cutoff. Conversations left without messages, drafts or assistance results are deleted in the same transaction, so account conversation counts stay truthful. After a completed Gmail page, the writer refreshes bounded planner statistics at most every five minutes, so a fresh profile's first sync does not keep empty-table search plans until restart. Consumers: GUI and CLI through the shared engine; Gmail26, Search6 and Quality69 unchanged. [Execution record](../research/search-storage.md#2026-09-23-audit-corrections).

## D095 — active schema source (Core52, schema32)

`fresh-schema.sql` is the sole SQL baseline for new live and synthetic profiles. Pre-reset migrations and version-2 test scaffolding are absent from the active source tree; historical versions 1–31 are still refused without modifying their profiles. Add future schema changes to the ordered post-reset migration runner. Consumers: GUI and CLI through the shared engine, Quality69. No data or wire format changes.

## D093 — fresh SQLite baseline (Core51, schema32)

New live and synthetic profiles create schema32 directly in one transaction. All 60 ordinary tables are `STRICT`; the baseline includes D092's expanded account-retention choices. Engines refuse every pre-reset schema (1–31) with `schema_mismatch` and never rewrite that database. Future schema steps belong in the ordered post-reset migration list, each committed with its own `user_version`. Public IDs, fixture revisions 0/3 and protocol1 remain unchanged. Keep the single serialized writer, profile lock, WAL, foreign keys and `synchronous=FULL`. Run bounded SQLite planner optimization after Store recovery and on normal close.

The FTS5 table is contentless-delete and keeps the existing explicit rowid insert/delete/update paths; exact MIME and attachment bytes remain in verified external blobs. A category-first index serves `(account,category_id,message_id)` lookups. Completed Gmail page items retain hashes, revision and part receipts while releasing their staging JSON; successful published Calendar items release staging JSON while failed items retain it. Keep run/page/item proof headers and all unresolved or uncertain work. Consumers: Search6, Gmail26, Calendar30, CLI42, Mail UI67 and Quality68. [Execution record](../research/search-storage.md#2026-09-23-shared-tree-integration).

## D092 — reversible mail actions and account editing (Core50, schema31)

`mail_undo` restores the exact per-message flag, filing and category snapshot in an earlier mutation receipt. It requires the same account, original request ID and current expected conversation revision; retries replay the immutable result. Later writes make an old Undo stale. Live restores enqueue normal durable provider mutations, with Trash restoration before dependent label changes. No action is represented as provider-acknowledged until normal reconciliation completes.

`account_update` also accepts `analyze_history` (D108; see Local assistance23) and `color` (D112: one of the 16 palette entries, stored as a palette index in `meta` as `account_color:<account>`; a new account gets the first unused entry, and archive deletion removes it). `accounts` rows report `color` (palette order for accounts without one) and `calendar_events` (stored events visible for the account's calendars). Core62. It accepts 90 days / 1 / 2 / 5 years / all retention and enabled Mail/Calendar read or read/write choices. At least one service stays enabled. Local read access narrows actual authoring and worker authority without erasing granted scopes. Editing retention resets acquisition generation/cursors; shortening removes only older local derived mail, queues unreferenced blob cleanup and refuses while affected conversations have pending writes or retained drafts. Extending makes older provider mail eligible again. Successful account sync/retention edits wake active engine sessions through the existing cadence notification. Schema30→31 rebuilds the metadata constraint while preserving exact existing values, revisions and cutoffs; older engines refuse. Consumers: CLI42, Mail UI67, Calendar29, Local assistance18 and Quality67; fixtures0/3 and protocol1 unchanged.

## D090 — typed reminder preferences and model Auto-read (Core49, schema30)

Calendar reminder and snooze durations use validated saved preferences. Existing events retain their own lead; a missing lead uses the default when the running client evaluates a due occurrence. A shown receipt can encode an absolute snooze deadline and return the occurrence after that time without changing the receipt table. Account-local Auto-read choices are part of typed model settings; only the new-arrival watcher may invoke the local plus durable provider-read transition, with human pending read intent taking precedence. Schema30 rejects older engines before they can drop the new settings field. Consumers: Calendar28, Local assistance17, Mail UI66, Quality66 and CLI's shared command boundary. Fixtures0/3 and protocol1 unchanged.

## D088 — received contribution projection (Core48)

Google acquisition stores recognizable reply contributions in `messages.body` and FTS instead of repeated quoted history. Preserve unquoted inline answers, intentional forwards and unknown formats. Exact MIME/HTML and attachment bytes remain in the existing verified blob store, outside SQLite; original-source inspection remains available. Apply during new message preparation before the existing text bound, on the shared live/synthetic provider path. Do not rewrite the existing archive, authored drafts, submission receipts, read/star flags or source hashes. No schema migration or claim of reclaimed space in an existing database follows. Summaries/search for newly acquired messages consume the same derived body; existing source/text provenance checks remain authoritative.

Consumers: Core48 / Gmail25 / Mail UI64 / Quality64; CLI41 and Local assistance16 consume the existing query shape. Schema29, fixtures0/3 and protocol1 unchanged.

## D086 — mail list and account backlog

Explicit account backlog queries capture account lifecycle, settings revision, maximum retained row ID and a time ceiling. Read bounded pages; exclude sent, Trash, provider-deleted and messages without a source identity or readable cached body. Counts are local metadata, not Google inventory. Conversation list rows additionally expose the selected preview message’s `recipient`, so acquired Sent previews show who received the mail. Recheck current enabled summary/classification projections, preserve valid existing results, and use existing source/manual/provider guards before commit.

Consumers: Core47 / Local assistance16 / Mail UI62 / CLI41 / Quality62. Schema29, fixtures0/3 and protocol1 unchanged.


## D083 — daily cached mail counts (Core46)

`calendar_mail_activity` reads the existing messages and Google coverage metadata for a half-open range of 1–42 civil days in the saved Calendar display zone. Omitted account means all active accounts (at most 100); explicit account must be active. Count messages, not conversations: the stored sent flag determines sent versus received. Prefer Google's stored internal arrival timestamp; fall back to the source message timestamp only when no provider timestamp exists. Count retained Inbox, archived and Trash messages; exclude removed accounts, provider-deleted messages, local/remote drafts and submissions not yet acquired as mail. Separate account copies count separately. Return every date including zero days, scope, zone, revision and `cached_only: true`; these are retained-cache totals, never a claim of complete historical coverage. No body/recipient/MIME reads, provider work, mutation or new schema. Consumers: Calendar26, Mail UI59, CLI40, Quality60; schema29/fixtures0,3/protocol1 unchanged.

## D081 — mail responsiveness

Core45 adds optional `live_paging` to conversation queries. Default cursors retain strict generation validation. Live cursors bind account/query/engine epoch and the last ordered identity, continuing across acquisition without restarting from page one. Results remain fresh, bounded and account-scoped; clients deduplicate moved identities. Folder counts and ordinary grouped browsing reuse the same invalidated conversation projection instead of scanning all message rows on every page.

`status.mail_generation` identifies in-process mail projection changes separately from view/settings writes; clients reset comparisons on reconnect. Consumers: Core45/Mail UI57/Quality58; CLI39 retains default strict cursors. Schema29, fixtures0/3 and protocol1 are unchanged.

## D077 — accepted draft projections (Core44)

Shared Drafts queries, search and account counts omit a draft when its persisted last accepted submission revision is at least its current revision. In-flight/unknown drafts remain present. A newer saved revision appears again. Keep authored content, submission receipts and history intact; this projection never discards a draft or grants resend authority. Reopening the store preserves the result. GUI composer cleanup also requires no newer unsaved text or recipient input.

Consumers: Core44/Gmail24/Mail UI55/CLI39/Quality55; schema29, fixtures0/3 and protocol1 unchanged.

## D073 — mail filing and draft content readback (Core43)

Inbox queries and counts require a non-Trash, non-sent message with Inbox membership. Archived incoming mail remains available through All mail/received queries. Conversation membership and explicit filing targets are unchanged.

Google draft readback compares the frozen uploaded MIME with fetched RAW off the Store writer. An optional `baseline.content_matches` records a decoded-content comparison; omission means unknown, not proven equivalent. Compare From/To/Cc/Bcc/Reply-To, subject, reply/reference identities, MIME part structure/type and rendering parameters, decoded text/HTML, attachment names/content/disposition and content IDs. Ignore transport-added headers, transfer encoding, MIME boundaries and body terminal line endings. Text attachments retain their terminal newlines. Unsupported groups/nested messages or decoding errors produce no equivalence assertion. `fidelity_warning` means a proven content difference. A legacy baseline can acquire this proof through read-only reconciliation with no new upload.

Keep both original hashes, exact provider identity, revisions, RAW retention and conflict checks. Content equivalence never grants overwrite authority: a subsequent changed RAW baseline still enters the existing conflict flow. Local authored text and unsaved/newer edits remain independent. This is additive metadata in the existing schema29 journal.

Consumers: Core43/Gmail23/Mail UI50/CLI38/Quality50; schema29, Calendar25, Local assistance14, fixtures0/3 and protocol1 unchanged. Whole-area readiness remains 0/9.

## D072 — account and model feedback (Core42)

Schema29 adds a minimal archive-deletion receipt, pending shared-file cleanup and account metadata fields `mail_retention`/`mail_since`. Existing profiles retain All and their history. New Google bindings default to one calendar year; revisioned `account_update` accepts `1_month`, `3_months`, `1_year` or `all` before acquisition starts. The saved UTC cutoff survives restart/resync. Existing saved mail is not aged out automatically.

`account_archive_delete(account,{expected_revision,request_id})` permanently removes a removed account's local mail, drafts, Calendar data, account-specific model settings and history. Require the exact lifecycle revision, completed credential cleanup and no unfinished cross-account transfer. Delete content/index references atomically, preserving other accounts. Exact replay returns the saved result after deletion. A new connection uses a fresh local account identity, fencing old callbacks. The response reports `file_cleanup:"next_startup"`: under the exclusive profile lock, before workers start, recheck remaining references and remove only unused queued blobs. User-exported files remain untouched. Older engines refuse schema29.

Consumers: Core42/Gmail22/Local assistance14/Mail UI49/CLI37/Quality49, schema29; Calendar25, fixtures0/3 and protocol1 unchanged. Whole-area readiness remains 0/9.


D070 / P2-UI3 (Core41): `calendar_account_visibility` is an additive typed local preference with values `all` (default) and `current`. It shares the revisioned JSON preferences store and exact replay semantics; existing individual `selected_calendars` remain unchanged. The GUI intersects the saved selection with the current account for display. Sync and reminder settings are independent. Older schema28 engines retain this unknown stored field through unrelated known preference edits. Consumers: Calendar25/Mail UI48/CLI36/Quality48; Gmail21/schema28/fixtures0,3/protocol1 unchanged.

D067 / P2-UI3 outgoing presentation. The read-only `google_sent(account,{offset?,limit?,conversation_id?})` query returns outstanding prepared/in-flight/unknown/accepted submissions, with bounded pages (1–100 rows), immutable request/headers/source identity and a next offset. A conversation filter includes only replies whose retained source belongs to that conversation. `google_sent_message(account,{request_id})` parses verified frozen MIME on the existing bounded content worker and returns plain text, truncation state, recipients and attachment metadata. It never reads the mutable draft as sent content, changes review flags, or contacts Google. Provider acquisition suppresses the accepted preview by exact account/provider ID, including later deletion. Existing receipts and schema28 are preserved. Consumers: Core40/Gmail21/Mail UI45/CLI35/Quality45, schema28, fixtures0/3 and protocol1.

D065 / P2-UI3 (Core39) implements durable draft-discard Undo with exact revision/lifecycle checks, captured authored buffers and unfinished recipient input, shared CLI/GUI recovery and grouped six-second toast Undo. Schema28 adds the snapshot journal and recipient-input storage; fixtures0/3 and protocol1 stay unchanged. Capture/upload continues verifying retained bytes before use. Scoped Windows Core, engine, CLI and native checks, actual schema27→28 migration/refusal/reopen, strict lint and configured relocated package146 pass. The full prototype goal and provider-cleanup/Calendar-Undo/pending-Sent gaps remain open. See D065 and the prototype execution record.

`draft_discard(account, {draft_id|conversation_id, expected_revision, request_id, buffer?})` captures the current authored snapshot atomically and returns the discarded projection plus `discard_undo:{version:1,request_id,draft_id,revision,lifecycle_revision}` when a saved active draft existed. Optional buffer fields are body/HTML/signature/subject, To/Cc/Bcc, attachment IDs and `recipient_inputs` (To/Cc/Bcc field names mapped to unfinished text, each ≤16 KiB). `draft_save` also persists recipient inputs; omission preserves them, while explicitly replacing a formal recipient list clears its pending input unless a new pending map was supplied. Capture/upload rejects unfinished inputs. `draft_discard_status(account,{request_id})` returns the original receipt and current `can_undo`. `draft_undo_discard(account,{draft_id,discard_request_id,expected_revision,request_id})` restores only the captured authored fields when the discarded revision, lifecycle and original identity still match. Historical provider state is never restored. Existing exact replay precedes current-state checks.

D063 / P2-UI3 adds explicit From-account transfer with source revision and Google revision checks, verified retained content, destination ownership and durable exact-request replay. A move preserves the original local history and waits for the destination Google draft readback before removing the unchanged original Google draft. Unknown deletion outcomes only reconcile the same request; explicit retry starts a new deletion attempt only after a known rejection. Account lifecycle changes fence cleanup. The composer and CLI share transfer/status/retry; source provider IDs never become destination IDs. Schema 27 adds the bounded transfer journal and preserves existing receipt bytes. Core 38/Gmail 19/Mail UI 41/CLI 32/Quality 41, fixtures 0/3 and protocol 1. Scoped Windows Core/provider/native/CLI, actual migration/refusal/reopen, strict lint and relocated package115 checks pass. Full prototype alignment and other-host acceptance remain open; package94/schema26 is preserved in ignored relocation94.

D062 / P2-UI3 adds serial multi-file composer input and bounded cached-recipient suggestions with account attribution. Preserve exact attachment retry receipts, captured draft ownership, unsaved edits and selected routing. Suggestions are read-only until chosen; schema 26, fixtures 0/3 and protocol 1 are unchanged. The D062 decision and [execution record](../research/prototype-september-15.md) own the boundary and pending verification. `recipient_suggestions(account, {query, exclude?, limit?})` returns `{account, query, suggestions:[{name,email,account,account_name}], scope}` for active accounts. Query ≤320 bytes, exclusions ≤100, limit 1–10/default six. Search recent cached metadata only: ≤2,000 messages plus ≤100 draft routing records for the selected account, plus equal allowances across other active accounts; exclude Trash/removed accounts and invalid mailboxes. Rank prefix/name-word matches before substrings, with selected-account preference at equal match strength. Deduplicate case-insensitively, preserve selected routing until explicit choice, and never mutate review state.

D061 / P2-UI3 adds account-local label creation, descriptions and removal plus explicit Google name merging/creation. Removal clears local membership and mappings, retaining historical receipts; it does not delete Gmail labels or mail. Descriptions use the existing assistance settings and stay local. Schema 26 journals provider creation before dispatch; unknown outcomes require inventory reconciliation, with no blind retry. Unmapped local memberships wait for an explicit link/sync before provider delivery. The decision and [prototype execution record](../research/prototype-september-15.md) own the detailed scope and evidence.

D060 / P2-UI3 schema 25 adds compatible-server sender-kind provenance while retaining existing settings, history, results and receipt bytes. Missing provider means Ollama; old Ollama snapshots retain their exact representation. Provider changes fence pending results and invalidate derived annotations while preserving manual choices. See Local assistance 12.

P2-UI3 adds `message_source` with an exact account, message `id` and `source_version` obtained from the conversation projection. It snapshots only retained source identity on the writer, verifies and parses bytes in the existing two-seat local-content worker, and never marks read, starts inference or contacts Google. Removed-account archives remain readable. The reply contains bounded To/Cc/Bcc/Reply-to metadata, an actual HTML alternative, a plain-text source preview and explicit truncation flags; JSON stays below 900 KiB inside protocol 1. Retained bytes remain unchanged. A changed message/source fails instead of retargeting the read. Core 34, Mail UI 37 and CLI 28 share this read boundary; schema 24 and fixtures 0/3 are unchanged.

P2-UI3 account service switches use `google_sync_services`, a revisioned preference map keyed by existing account ID with boolean `mail` and `calendar` values. Missing entries enable both services. The map is bounded to 200 accounts. Disabling pauses subsequent automatic acquisition passes for that service (including Mail label/draft inventory); an already running bounded pass may finish. It retains cached data, sync cursors, credentials and explicit account ownership. Enabling wakes existing owners, never starts stopped accounts or retries credential denial. Explicit user writes and their recovery retain their current scheduling. GUI/CLI share this persisted map; no schema migration or new service lifetime.

P2-UI3 shared preferences add `sync_every_minutes` with values 1, 5 or 15, defaulting to the existing one-minute cadence for older settings. The existing revisioned preferences JSON and operation receipts own persistence; no second settings store or schema change is required. Engine acquisition uses the setting for completed Gmail delta and Calendar passes. A successful cadence edit wakes existing sync owners only; it does not start stopped accounts or retry denied credentials. Sends and mutation delivery keep their current immediate scheduling. GUI and CLI consume the same preference.

D059 / Native Inbox uses the new `received` query scope: a retained non-Trash, non-sent member qualifies a conversation. Whole-conversation reader membership is unchanged. Folder totals include `received`. Preferences accept `terminal` while retaining legacy `harbour`. Consumers: GUI, CLI 28 and Mail UI 37; schema 24 is unchanged.

D058 / P2-LLM2D limits all assistance to received mail. Reader queries expose the actual sent flag; sent targets fail before inference and historical sent results remain stored but hidden. Combined failures display once; null/empty tool calls are accepted as no tools, and truncation has its own error. Schema 24 preserves prompt/account choices while expanding the former default response allowance to 4,096 tokens. See Local assistance 11 and its scoped execution evidence.

D056 / P2-LLM2B exposes the complete editable Ollama JSON request: system/user messages, all five variables, full response schema and generation options. No hidden envelope or schema is injected. Schema 23 archives and wraps prior prompt text, preserving model/account choices. Application output/action validation remains independent of editable instructions. Consumers: Local assistance 9/Core 32/Mail UI 34/CLI 26/Quality 34; fixtures 0/3/protocol 1 unchanged. See [the current request contract](local-ai.md) and D056 evidence.

State: Draft · Revision: 49 · Updated: 2026-09-23 · Implementation: D090 scoped Core tests pass; native/package and schema upgrade/refusal evidence remain in progress. Schema30, fixtures0/3, protocol1 and whole-area readiness0/9 unchanged.

D055 / P2-LLM2: one shared model and visible template (`{{labels}}`, `{{messages}}`, `{{partial}}`), combined per-message output, default-on prospective summaries/automatic single-label classification, manual precedence, and summary list previews. Schema 22 fences settings and assisted delivery; fixtures 0/3/protocol 1 unchanged. This supersedes conflicting D051–D053 delivery defaults below; historical evidence remains scoped to its original version. See the D055 decision and Local assistance 8.


D054 records account-scoped last successful Gmail history completion in existing local metadata only after the final page and all its parts are applied; expose it in accounts/status without changing schema 21 or legacy receipts.

D050 repairs existing Google category projections transactionally on first live reopen and on label binding, preserving manual/assist precedence, original sources, flags and receipts. Only changed conversation revisions and derived category membership advance. Account count projections add cached `folder_conversation_counts`; conversation queries return `folder_conversation_count` for the selected folder before search/category filters, excluding Trash appropriately. This avoids a competing GUI count query. Adds durable `mail_date_grouping` (`grouped`/`plain`, default `grouped`) shared by GUI and CLI. Schema 20 and fixtures 0/3/protocol 1 are unchanged.

D039 adds read-only calendar_search through the existing permitted combined projection. It creates no index, schema, receipt or provider work. The explicit 1–42-day range and selected-calendar semantics remain unchanged. Query text is at most 512 UTF-8 bytes and 32 whitespace-separated case-insensitive AND substring terms over permitted title, organizer, attendees, location and description. Busy-only rows are excluded; split-day rows deduplicate by account/calendar/occurrence identity. Return at most 100 results with has_more plus inherited range, revision and coverage. Existing projection limits remain explicit. Consumers: Core 27/Calendar 20/Mail UI 25/CLI 20/Quality 23; schema 20, Platform 14/Gmail 12/Search 5, fixtures 0/3 and protocol 1 unchanged.

Revision 25 applies D037/P2-06C to schema 19. Preserve old journal/source bytes and version only new occurrence intents/dispatches as 3. Retain actual instance target, expected instance/master versions, typed parent/original identity, exact originalStartTime source, full instance baseline and opaque bounded master observation/fences. Master source comparison and normalization run off-writer under one admitted Calendar seat; finish/stage/start recheck current lifecycle, connection, permission, effective published source generation and Calendar revision. No full master/page source crosses a later quota wait. Reuse the existing journal and exact late-outcome/read-publication distinctions. Keep ordinary visible source selection strict and add a narrow effective-published cancellation input for an already-current parent, preserving observation metadata and supersession ordering. An unreadable or over-budget exception blocks its series. Consumers: Calendar 18/Mail UI 23/CLI 18/Quality 21; Gmail 12/Platform 13/Search 5 unchanged.

Version-three master/source qualification for read reconciliation accepts current permitted Calendar detail-read grants; editable lookup finish and stage/start separately require writable organizer eligibility. It never relaxes private-master detail, catalog, permission, connection or lifecycle checks. DELETE ACK publication retains its dispatch-derived cancellation tuple in `calendar_sources`, while `calendar_event_state.observed_json` holds `{kind: acknowledged_occurrence_cancellation, request_id, source}` provenance and the outcome names `source_origin: dispatched_occurrence_identity`. Actual fetched cancellations retain exact provider JSON and replace this envelope on subsequent observation. No new provenance column or reinterpretation of v1/v2 journals is introduced.

Revision 24 applies D036/P2-01B account lifecycle in the existing Store. Retain account identity, saved content and all old receipts. Active/removed state has a monotonic lifecycle revision; bounded current cleanup metadata is separate from immutable historical results. Removal commits inactivity and advances connection generation before vault cleanup. Exact receipt replay and cross-kind conflicts precede new-work admission. Current active lifecycle fences mutation/claim/publication, and local off-writer plans carry the admitted lifecycle revision through finish. Remove followed by restore cannot revive an old plan. Late exact provider outcomes and startup in-flight recovery remain recordable while source publication is blocked. Ordinary lists/default combined queries omit archives; explicit cached account reads and source/file exports remain available without new legacy-draft creation. Prepared archived replay never dispatches. Revision-bound remove preview/commit/status and same-subject restore reuse the existing command/journal model; reports distinguish local removal, current cleanup and historical receipts. Actual schema-17 upgrade/refusal must preserve all old data and receipt bytes. Consumers: Core 24/Gmail 12/Calendar 17/Platform 13/Mail UI 22/CLI 17/Quality 20/schema 18; Search 5, fixtures 0/3/protocol 1 remain unchanged.

Revision 23 applies D035/schema 17. Preserve every existing Calendar journal/source/receipt byte while versioning newly prepared live invitation intents and dispatches. The schema bump prevents earlier engines from reconciling guest removal with the old self-RSVP-only predicate. Reuse the existing journal; distinguish provider acceptance from observed-state reconciliation and never invent notification delivery. Exact target, ETag, connection, permission and operation stamps fence preparation, dispatch and late outcomes. Organizer roster replacement preserves protected entries and supported unchanged guest metadata; omission preserves the whole array. Observed-state evidence retains its own opaque publication fence captured before the reconciliation read; connection, permission and source revisions must still match and current catalog membership must permit access before cache publication or staging expiry. The original mutation lease remains immutable and controls any later provider acknowledgment. Consumers: Calendar 16/Mail UI 21/CLI 16/Quality 19; Gmail 11/Platform 12/Search 5, fixtures 0/3/protocol 1 unchanged.

Revision 22 uses schema 16 and the existing Store/drafts/blob/capture structures for explicit source conversion, observed-target adoption and typed remote-draft deletion. Bounded twin metadata retains derivation separately from adoption and acknowledged upload baselines; indexed due selection excludes an unchanged newly adopted revision. Both upload and Send use one effective target rule. Fresh source checks, opaque off-writer content proofs and short revision-fenced commits preserve original RAW, authored edits and old receipts. Delete guards block conflicting exact-target work; kind `draft_delete` is excluded from upload/Send/synthetic loaders and inventory owner linking. Missing provider state never proves sending or an uncertain mutation's success. Consumers: Gmail 11/Mail UI 20/CLI 15/Quality 18; Calendar 15/Platform 12/Search 5, fixtures 0/3 and protocol 1 stay unchanged.

Revision 21 adds D032/schema 15 provenance to the existing drafts table and bounded remote-inventory/source-version metadata in the same Store. Existing blobs and nullable-message attachments retain exact remote RAW and decoded parts; imported previews have no invented conversation, primary From, signature, upload baseline or operation. Local B1/B2 baselines and receipts stay unchanged. Read-only provenance is checked from persisted state at every save/attachment/discard/capture/upload/Send preparation and commit/claim entry, and auto-upload selection explicitly excludes it. Cached unfiltered Drafts paging counts/indexes the collection and loads only the requested page. Consumers: Gmail 10, Mail UI 19, CLI 14 and Quality 17; Calendar 15/Platform 12/Search 5, fixtures 0/3 and protocol 1 stay unchanged.

Inventory stores generation, connection generation, bounded pending page, per-item durable outcomes and continuation. List pages contain at most 50 identities; RAW decoding, MIME parsing, hashing and exports run on bounded workers before short source-fenced publication. Cancel resumes pending work; expired tokens restart enumeration without deleting versions. Completed enumeration, retained RAW, preview issues and freshness remain separate. Known B1/B2 targets are rechecked at publication; uncertain creation without a provider identity is never linked or acknowledged by content similarity. Imported previews may follow a newer retained remote version, with older RAW/source metadata preserved; linked authored text remains unchanged. Exact source revisions bind cached history and verified source/attachment export.

Revision 20 adds explicit Google submission under D029 using schema 14 and a draft/send discriminator in the existing capture journal. Preserve old draft and synthetic receipts; all loaders, recovery and due selection respect operation kind. Freeze exact authored identity/content outside the writer, retain known-unsent/in-flight/accepted/rejected/unknown outcomes, and exclude pending or consumed sends from automatic draft work. Newer authored revisions survive every outcome. Consumers: Gmail 9, Mail UI 18, CLI 13 and Quality 16; Calendar 15/Platform 12/Search 5, fixtures 0/3 and protocol 1 stay unchanged.

After definitive acceptance, a strictly newer authored revision may be submitted only by another explicit Send with a fresh request and RFC Message-ID. Reusing the accepted authored revision with a new request is refused. The consumed Google draft ID is never reused; later submissions use a fresh direct-message intent. Pending or unknown submissions block all new attempts. The last accepted authored revision remains distinct from the current operation revision and monotonically advancing Google twin revision. Cached `draft.google_send` reports the latest operation without rebasing editor content. Per-account `capabilities.send` requires a connected live account and the existing mail-write scope predicate; global live `status.capabilities.send` describes the implemented interface, not account admission or delivery readiness.

Two competing preparations with the same exact request must preserve one journal winner's MIME/Message-ID, with one atomic claim. A lost local response followed by `not_found` does not prove absence of an invocation still preparing. Explicit Resume may repeat only that original four-field request and ID; an existing in-flight/unknown/accepted operation returns its retained outcome without another submission. Direct predispatch error proof is scoped to the invocation that returned it and cannot erase earlier transport uncertainty.

An exact obsolete request can be conclusively refused without dispatch only if the writer first finds no journal row, then finds its current local or Google revision strictly greater than the request's expected revision. Emit the bounded `send_request_obsolete_without_dispatch` proof with the existing stale error; all pending proofs recheck the same monotonic stamps before insertion and claim requires that row. Existing journal states, future revisions, connection errors or `not_found` cannot supply this stronger proof. This lets an explicit original-ID Resume release a no-longer-possible request after newer edits while preserving both durable uncertainty and authored content.

Revision 19 adds bounded Google twin metadata on existing drafts and an execution discriminator on existing capture operations, defaulting old rows to synthetic. Synthetic replay/history/recovery must exclude Google rows and preserve old JSON receipts; request-ID collisions across modes fail explicitly. Freeze exact authored revision/identity/source/attachments using opaque off-writer proofs and the existing blob store. Google draft operations retain uploaded MIME separately from fetched provider RAW, stable draft ID, acknowledged contained-message ID, acknowledged local revision and observation/version fences. A late acknowledgment may record its original operation while a newer local draft survives; stale connection work cannot publish a new twin baseline. Consumers: Gmail 8, Mail UI 17, CLI 12, Quality 14 and MIME; Calendar 15/Platform 12/Search 5 unchanged, schema 13/fixtures 0 and 3/protocol 1.

Only readback matching both acknowledged provider IDs establishes an observed baseline. Later changed IDs or RAW hash preserve both versions and require explicit conflict resolution; missing remote drafts remain distinct from proven send. Store upload-versus-observed byte equality separately from version identity. Unknown create/update outcomes survive restart without automatic replay. Small status/progress projections contain references and hashes, never raw MIME or tokens. Preparation and fetched-RAW hashing stay outside the writer, bounded by worker memory; SQL commits recheck account, generation, operation and draft stamps without rebasing authored text.

Fresh readback of an unknown update may retain a conflict observation for its exact existing target. Only a new request ID with the current local and Google revisions may explicitly replace that observation; the original unknown operation remains immutable and unknown. Unknown creation and missing unknown targets cannot take this path. Historical replay exposes its selected operation separately through `operation_is_current: false`; current status and `draft.google` use `true`. The nullable operation `provider_draft_id` is the frozen update target, never a target inferred from an unrelated later baseline.

An unchanged draft with a deterministic local preparation failure must not block newer drafts. Retain bounded revision-bound failure metadata without inventing an upload receipt; changed authored revisions become eligible again. Retryable local IO, cancellation, quota and authentication failures retain their respective retry semantics. Due selection remains bounded and fair to other eligible drafts.

Revision 18 adds account-scoped local-file attachments to existing durable composition. Schema 12 permits a NULL received-message link in `attachments`, retains its message foreign key and adds a direct account foreign key. Imported files do not enter received messages, attachment filters, FTS or source coverage; negative attachment filters must explicitly exclude NULL links. Existing rows, drafts, fingerprints and exact receipts survive migration. Consumers: Mail UI 16, CLI 11, Quality 13, Gmail 7/MIME and Search 5; Calendar 15/Platform 12 remain unchanged.

The additive `draft_attachment_import` boundary requires `draft_id`, positive `expected_revision`, `request_id` and an absolute local `path` under an explicit account. Check replay before opening a file. Off-writer preparation retains exact bounded regular-file bytes; one short transaction rechecks current draft/attachment stamps, adds the attachment and selected ID, advances the draft revision once and records the result. The result contains `draft` plus attachment account/id/name/type/size/state/SHA-256 and nullable message identity. Preserve the 50-file and 25-MiB combined bounds. Store only a bounded basename, `application/octet-stream` and immutable blob reference; the source path contributes only to the request fingerprint, never retained metadata or receipt text.

`local_content_begin` and `local_content_finish` exchange either an exact reply or an opaque preparation plan with a bounded blocking worker. `prepare_local_content` has no SQLite access and produces an opaque verified proof. The same path handles unchanged `draft_save` and synthetic `draft_capture` requests: attachment hashing, first-save legacy source parsing, MIME construction and capture export occur outside the Store writer. Snapshot and recheck draft/source/attachment/account-identity stamps; preserve existing routing fallback and old first-save receipt compatibility. Capture replay verifies its original frozen bytes, never the current draft. A same-request capture race may redirect once to the winning frozen capture; other stale work fails without automatic rebase. No bulk attachment/MIME bytes enter the Core queue. Runtime verification remains separate from this agreed boundary.

Revision 17 adds a generation-fenced cached Google label catalog, complete-generation publication, revision-bound pagination and explicit category binding through existing categories/maps/operation receipts. Discovery changes no categories or messages. Binding validates a present user label and current catalog revision, plus explicit prior mapping on remap; missing destinations retain configured category identity. Exact mail delivery lookup counts all rows for one account/request and never infers confirmation from a truncated pending list. Live Calendar status/history project bounded target/kind/state/revision/outcome fields without copying dispatch bodies; known-unsent `prepared` or `ready` request extraction supports explicit resume under D025, while unknown work requires reconciliation. Saved confirmed quota limits use existing typed preferences. Consumers: Gmail 7, Calendar 15, Mail UI 15, CLI 10, Platform 11, Quality 11 and Search 5/Triage 6. Current implementation evidence remains separate from this boundary.

Revision 16 applies D023: extend the existing operation receipts/manual intents with per-message delivery state, frozen target revisions and generation-fenced reconciliation. Schema 10 adds a Trash projection and extends the existing Calendar write journal with explicit local/live execution modes, conditional dispatch and durable outcomes. A late provider response can record the exact attempt without publishing through a stale connection; its receipt must survive normalized source removal. Core rechecks every prepared Calendar chunk against connection, permission, run and source stamps. Two shared Calendar memory permits remain owned through off-writer hashing/normalization and bounded SQL stages; normal chunks contain at most 32 items/1 MiB and page JSON is limited to 16 MiB. Prior source byte lengths are checked in SQL before copying; merged serialization is capped at 32 MiB before normalization. These logical source bounds do not establish physical allocation/RSS limits. Resource refusal preserves prior source and continuation. Consumers: Gmail 6, Calendar 14, Mail UI 14, CLI 9, Platform 11, Quality 11 and Search 5/Triage 6. Implementation and measured acceptance remain distinct.

Revision 15 defines D022's bounded live acquisition boundary, consumed by Gmail 5, Calendar 13, Mail UI 14, CLI 8, Platform 11, Quality 11 and Search 5. Schema 9 adds provider work, coverage and projection metadata to the existing store; it does not create a second message or authored-operation store. The synthetic complete-page path remains unchanged. Live Gmail commits bounded visible parts, each with source references, normalized messages, FTS, retained human intent and an exact replay receipt in one transaction. It advances the Google page cursor only after every requested item has a durable outcome. Parsing, sanitization, hashing, blob publication and recovery verification occur on bounded workers before Store admission. Partial inventory, source availability, attachment completeness and indexed text remain separate coverage dimensions. An opaque prepared item cannot be reconstructed merely by deserializing an unverified manifest.

Independent recent, inventory and delta lanes share account generation and per-message history ordering. Only complete inventory proves absence; neither the recent cutoff nor an older source response may erase a newer observation or rewind current history. Calendar stages bounded chunks into generation-tagged slots of the existing source tables, seals complete pages and publishes by changing a committed generation/token pointer. Pending runs do not invalidate the visible model. Permission restrictions invalidate stale leases immediately; limited-writer private masters/exceptions remain busy-only. Indexed window candidates, bounded expansion and explicit incomplete coverage replace the whole-inventory source cap. Actual packet evidence belongs in the [execution record](../research/phase-2-execution.md).

D021 implements this revision's first shared boundary: immutable live/synthetic profile identity, persistent vault namespace and stable Google-subject bindings in schema 8. Live fixture 0 seeds nothing. Internal verified-auth registration preserves reconnect identity; capabilities and inventory remain unavailable/incomplete until their adapters pass. [Execution evidence](../research/phase-2-execution.md) separates targeted tests from complete packet acceptance. The following planning record describes the revision's broader scope.

Revision 14 prepares D020/Phase 2's real Google boundary; no new runtime schema is implemented. P2-00 owns exact account identity (validated Google subject mapped to a local ID), provider capability/auth references, inventory/source/blob/index coverage, checkpoint generations and durable remote-operation outcomes. GUI/CLI keep one store and writer; provider HTTP and unbounded parsing never hold the UI or store mutation path. A separate empty live profile must never seed synthetic accounts, replay fake acknowledgments or submit old captures. Preserve schema-7 profiles and authored state; assign the next schema/fixture revision only with an actual agreed migration.

The founder chose OS account permissions plus verified disk encryption for the pilot; app-level database/blob encryption is deferred. Verify every real-data volume, temporary path and backup destination before import. Tokens remain in the OS vault. Resolve Spam/Trash inventory, confirmed remote-deletion retention, backup/restore, disk-full behavior and account removal before affected real-data work. Disconnect pauses access without deleting drafts or unresolved operations; explicit cleanup/revocation is separate. The proposed retention choices are in P2-00, not silently accepted defaults.

Extend the tested atomic page/checkpoint/FTS boundary with streaming and durable staging that can handle real source sizes and dense calendars. Per-item unavailable/error states must preserve coverage and let other valid items progress without silently discarding originals. Full inventory, retained MIME, verified attachments and indexed text are distinct completion states. Gmail draft container IDs, source message IDs, local draft IDs and submission IDs stay distinct. Preserve local revisions and known remote conflicts; a fresh read followed by an unconditional Google draft update is not atomic conflict protection. Never reinterpret synthetic capture acknowledgment as live submission. Consumers: Gmail 4, Calendar 12, Mail UI 14, CLI 8, Platform 11, Quality 11 and existing Search 5/Triage 6. See the [Phase 2 plan](../plans/phase-2.md).

Revision 13 defines D018/P1-09: schema 7 adds `message_sender_kinds(account,message_id,source_hash,kind,provenance)`, with an account/member foreign key, people/automated kinds and explicit fixture provenance. A missing annotation or retained-source hash mismatch projects unknown. Conversation kind follows the newest incoming member by actual timestamp and stable member ID, falling back to the newest member when all are outgoing. Querying never changes review state. Source changes and annotation changes invalidate projections/cursors; bounded message appends update the existing conversation cache. Fresh fixture 3 carries explicit known annotations; migrated fixture 1/2 profiles retain their revision, accounts, sources, drafts and exact receipts without reseeding or inferred classifications. Consumers: Mail UI 12, Search 5, Triage 6, CLI 7, Platform and Quality.

The additive protocol-1 boundary supplies `kind` and `exact_sender` conversation filters, exact whole-filter `filtered_group_counts`, and bounded account-scoped `sender_summary` history. The [fixture API](../../fixtures/README.md#p1-09-sender-kinds-and-presentation-preferences--core-13-cli-7-search-5-triage-6) owns exact fields. Local preferences also add theme graphite/paper/harbour, text size small/default/large, startup view mail/calendar/last, last workspace mail/calendar, timeline position footer/header, and default event duration 30/45/60 minutes. Defaults are graphite/default/mail/mail/footer/60; old saved JSON and receipts remain unchanged and getters merge new defaults.

Presentation-only `collapsed_groups` removes review/held/done groups from paging while preserving full filtered totals, adding `expanded_conversation_count`. Cursor scope includes that presentation choice. `status.calendar_generation` starts at zero after each Store startup and advances on actual calendar-table writes; mail/draft/appearance changes do not advance it. Clients can retain their calendar model through appearance-only changes; a new engine starts a fresh generation scope. Account unread/starred message totals reuse the same source-derived append projection and remain independently verified against stored message sums/counts.

Windows P1-09 verification passed the full release workspace's 161 executed tests before the last collapsed-group/calendar-generation additions. The final six focused sender-kind/query/generation/migration tests pass in 6.02 seconds, including parser-supported Unicode sender identity and structured draft recipients. Strict workspace/all-target release Clippy passes, and production engine/CLI relink succeeds. Logs are retained at `build/verification/p1-09-rust-workspace-01.log`, `p1-09-core-final-additions-01.log`, `p1-09-rust-clippy-02.log` and `p1-09-production-build-02.log`. Final-source integrated/native/performance/package evidence remains separate; the preceding full run does not claim to include those last two tests.

Revision 12 defines the explicitly authorized P1-08 local New/Reply-all/Forward/Drafts boundary. Schema 6 generalizes the existing drafts table with durable account/draft identity, an optional source conversation/member and frozen routing. Protocol 1 and fixture 2 remain additive/unchanged. Existing reply selectors, authored columns, operation receipts, captured bytes and fixture revisions survive migration. A migrated historical draft freezes its currently retained source: routing that the old schema never stored cannot be reconstructed historically. Consumers: Mail UI 11, CLI 6, MIME, Platform and Quality. Exact fields are in the [P1-08 fixture API](../../fixtures/README.md#p1-08-local-composition--core-12-cli-6-mail-ui-11). Current execution evidence remains separate from the specification.

New drafts never manufacture received conversations or provider-sent records. New, reply, reply-all and forward share the existing account identity/signature snapshot, revision journal, authored text/HTML, retained-attachment selection, discard and immutable local capture path. Recipients and subject are persisted and editable; source member/hash, RFC reply context and account identity are fixed. The first legacy save may bind the source member returned by `draft_get`, so an arrival before autosave does not retarget the reply. Later saves reject a different source. Reply-all excludes the current identity case-insensitively, deduplicates visible source recipients and starts with empty Bcc. Forward starts with an editable quote of retained source text and no implicit attachment selection. Its MIME is not a reply to the original Message-ID.

Draft browsing is a bounded account projection with plain-word subject/body/recipient search, exact filtered totals, stable update-time/identity order and a byte-bounded page. Draft content never enters received-source FTS or mail counts. Explicitly created/saved empty drafts remain visible until discarded; merely querying a nonexistent legacy reply does not create a draft. Source I/O, recipient validation, stale revisions and acceptance-unknown recovery remain meaningful failure states. Real transmission, account connection/removal, aliases, models and new-file import are outside this local extension.

The scoped Windows run `cargo test -p fm-core -p fm-cli -p fm-engine -p fm-mail --locked` passed 108 executed tests: Core 71 unit and 14 integration, engine 5 unit and 6 integration, CLI 3, MIME 9. The opt-in 100k experiment and native-GUI startup case retain their separate runners. Six new composition tests cover independent draft identity, frozen source across actual provider-row ingestion, Reply-all recipient privacy, Forward without implicit retained attachments, bounded/account-scoped Drafts, stale/invalid edits, schema-5 authored-column and exact journal preservation, and capture replay with a maximum escaped subject. Scoped all-target clippy passes with warnings denied. The [21-call actual CLI report](https://github.com/yegors/fuckMail/blob/885de67d2d3807f73374ea2975a63fb9476d6e7f/docs/research/assets/phase-1/p1-08-cli-composition.json) records preserved schema-5 first-save replay through the updated CLI, exact stored fingerprint/receipt preservation, new first-save replay and local capture recovery after later edits/restart, including binary hashes. Build/test logs are retained under `build/verification/p1-08-*`; whole-phase execution belongs in the [execution record](../research/phase-1-execution.md).

Revision 11 refines Search 4 cursor invalidation to search-table changes and writer restart, so unrelated view/preferences/draft/calendar writes preserve paging. Response revisions remain store-wide. Bounded message/FTS append projections independently verify inventory and fall back on mutation, overflow or transaction rollback. Consumers: Search, Mail UI, CLI 5, Platform and Quality; schema 5/protocol 1 remain unchanged.

Revision 10 records the implemented P1-00/P1-01/P1-05 boundary: schema 5, protocol 1 and fresh fixture 2; typed local preferences, metadata/signature snapshots, grouped typed search/counts/cursors, snapshot action recipes, durable discard and capture history. Consumers: Mail UI, Triage, Search, Calendar, CLI, Platform and Quality. [Fixture API](../../fixtures/README.md) owns exact wire fields; [local protocol](../local-protocol.md) owns envelopes and CLI grammar. Existing fixture-1 profiles retain their revision/accounts/data under migration. Windows Core/CLI tests and the 19-call live CLI smoke pass; native whole-phase, other-host, large-corpus and product gates remain separate.

Revision 9 applies D014 and the [Phase 1 plan](../plans/phase-1.md): read-on-open and snapshot Done/untimed Delay, bounded typed preferences, grouped conversation queries and combined permission-aware calendar views. Consumers: Mail UI, Triage, Search, Calendar, CLI and Platform. Current runtime remains schema 4, protocol 1 and fixture 1; implementation determines the next required revisions.

Revision 8 records the integrated Phase 0 experiment. Schema 4 adds completed calendar-batch checkpoints/visibility/write receipts and validated fake-assistance policies/results/intents to the existing store. Calendar permissions filter the shared projection; assistance rechecks retained source/provider/manual/policy stamps inside its apply transaction. Consumers: Calendar, Local assistance, Gmail, Qt UI, CLI and Platform.

Revision 7 records the next Phase 0 integration checkpoint. Schema 3 joins fake Google pages, normalized source text/FTS, provider checkpoints and durable receipts in the one store; pending human actions survive reconciliation. Authored HTML and retained draft attachments round-trip through capture. Per-request SQL cancellation, abrupt engine termination/replay and schema 1/2 migration are tested. Consumers: Gmail, CLI, Qt UI, MIME, Search, Calendar and assistance.

Revision 6 records D012’s implemented experimental slice. Protocol 1 and fixture 1 now have a single SQLite writer, bounded queries, independent flags, durable drafts/views and recorded operation responses. See [local protocol](../local-protocol.md) and [fixture contract](../../fixtures/README.md). Consumers: CLI, Qt UI, Search, Gmail, Calendar and assistance. Full provider reconciliation, retention policy and migration/recovery qualification remain open.

Revision 5 applies D009: Google-first scope, paired early GUI/CLI, retained MIME/text without an initial HTML renderer, and user-owned action state. Affected consumers: Gmail, Search, Mail UI, Calendar, Triage, Local assistance, Platform and CLI; future IMAP/SMTP contracts stay deferred.

Owner: A — Core/storage. These are living specifications of accepted requirements and proposed behavior; exact types, libraries and unresolved details are not committed. See [contract workflow](README.md).

## Build boundary

| Boundary | Responsibility |
| --- | --- |
| Owns | Shared identity and schema, migrations, durable drafts/operations, offline retention, normalized change stream, bounded local API and account isolation |
| Consumes | Normalized provider changes, user commands, validated assistance results; credential references from Platform |
| Produces | Account/message/conversation/event identities and revisions, capabilities, paged views, coverage, durable operation outcomes and change notifications |
| Independent start | Synthetic records and fake providers; publish shared types and fixture revision before parallel consumers implement against them |

Related: [triage](triage.md), [search](search.md), [platform](platform.md), [storage research](../research/search-storage.md).

Revision 4 links the initial shared-boundary handoff to [Phase 0](../plans/phase-0.md). Affected consumers: Gmail, IMAP/SMTP, Search, Mail UI, Calendar, Platform, Local assistance and CLI. The area remains Draft; no wire types or schema have been implemented.

## Phase 1 boundary — implemented local slice

The existing store and journal now implement schema 7. Protocol 1 stays additive: legacy date-sorted queries and message-count fields retain their meaning. Fresh fixture 3 has three accounts and explicit sender kinds; migration never reseeds existing fixture 1/2. The exact reviewed request/response fields and bounds are recorded in the [fixture API](../../fixtures/README.md). No client opens SQLite or introduces another mail/calendar store.

- **Mail state and counts:** `conversations` supplies bounded, stably ordered conversation groups through `grouped:true`, typed source filters, filtered totals and account totals with explicit message/conversation units and coverage. Cursor paging binds the account, options, page size, writer instance and search-table generation; a changed query/search state or writer restart fails `stale_cursor`. Unrelated view, preference, draft and calendar writes preserve pagination. Sender/category/date/attachment/text constraints combine on one source member; status examines the full conversation. Dates are inclusive `after` and exclusive `before` at UTC midnight. Held count includes unread+held conversations even though group precedence is review, held, done. Existing unread_count/starred_count keep their message-count meaning; new conversation fields are distinct. A user opening a conversation in the GUI submits a journaled read action for its opened member snapshot; the underlying fetch remains a query. Done atomically marks that snapshot read and clears its stars; Delay is an untimed star hold. Later arrivals remain outside earlier actions. These use provider-backed read/star state and the existing durable request/revision rules. Filtering, paging and sending never manufacture an all-clear.
- **Preferences and view state:** `preferences_get/save` stores validated density, week start, calendar/timeline hours and calendar selection with independent revision/replay receipts. Calendar selection uses null=all, []=none or explicit scoped identities. Distinguish these app-wide preferences from account-specific mail selection, typed filters, reply-open/member state and scroll position. Calendar selection/navigation is independent of the selected mailbox. Extend the current view allowlist deliberately; do not serialize arbitrary prototype state. Account display names/signatures, provider permissions and operation intent are domain data, not presentation fields.
- **Calendar scope:** a combined view names its participating account/calendar identities, date range, display time zone, coverage and scope revision. Reuse Calendar's permission-filtered occurrences and preserve the owning account on every event. Bind continuation/cursor state, where needed, to that complete scope; changes in visibility/permissions invalidate stale results. Enforce an aggregate response bound across calendars and never truncate silently. Full permitted details may appear across connected accounts; free/busy and private-field restrictions still apply before serialization. Mail queries remain account-scoped.
- **Reply and calendar writes:** plain/formatted replies, signatures, retained attachments, explicit discard and local MIME capture share the durable path. First save snapshots signature/From display identity; body edits preserve the snapshot, and explicit per-draft `signature_text` edits participate in its revision. Account preference changes never rewrite an existing draft. Explicit discard clears authored fields but preserves a revision tombstone; composing anew snapshots current metadata. Capture appends the saved signature once. `capture_history` returns bounded journal-derived outcomes linked to the original conversation, with distinct accepted/failed/unknown states and explicit `provider_delivered:false`. Captures are not provider-sent messages. New mail, forwarding and local-file attachment import follow later. Expose nonrecurring create/edit and self-RSVP through the journal and deterministic fake acknowledgment. Create needs a new stable identity/prepared-intent/acknowledgment path because current edit/RSVP assumes an existing stored event/ETag. Recurrence-series editing remains later work.

Acceptance includes GUI-to-CLI readback, restart/replay, concurrent new mail and stale revisions, permission changes during combined calendar queries, and migration preserving existing drafts/actions/views. The [Phase 1 plan](../plans/phase-1.md) owns packet evidence. Real provider/model execution remains outside this phase.

## First shared-boundary handoff

P0-01 defines synthetic identity/state expectations; P0-03 and P0-10 prove storage and client behavior. Before a production consumer starts, publish the relevant agreed interface/fixture revision covering account/message/conversation identities, calendar series/occurrence identity when needed, revisions/cursors, snapshot action targets, operation outcomes, capabilities, coverage and session ownership. Define representation and serialization in the actual chosen implementation, with Rust/Qt contract tests; do not let each adapter invent its own IDs or error model.

The first slice needs a bounded page/query, one journaled action, source/blob availability and an explicit event view, not every future API. Keep later provider/inference capabilities open behind the same ownership rules. Mark only that resolved slice ready; full Core readiness still requires retention, migration and all applicable recovery decisions/gates. The [development plan](../development-plan.md) sets when unresolved choices become blocking.

## Shape

Current delivery scope is Gmail/Google Calendar first, with both GUI and a small CLI using this same boundary immediately. MIME decoding, original source retention, text extraction and outgoing MIME remain required. Neither a full HTML renderer nor IMAP/SMTP/external-feed adapters are initial prerequisites. Preserve extensible identities and capabilities without implementing unused adapters. User-selected Done/Delayed and classifier tags are distinct state domains; inference never supplies read/star/completion commands.

Use one local application core for the desktop GUI and a full headless CLI on Windows, macOS and Linux, including Ubuntu, Arch and Omarchy. Both clients read the same local views and submit the same validated commands. Provider synchronization, parsing, indexing, storage and optional local inference work run outside the UI's event loop. Direct provider access with no app-operated remote server is required; optional inference connects to the user's local Ollama/vLLM service. Mail navigation and search stay within the selected account and render conversations. The 2026-09-11 requirements are detailed in [triage](triage.md) and [local assistance](local-ai.md).

Use a native desktop toolkit for navigation, lists, calendar and composition. QML expressions are acceptable; no Node.js or separate JavaScript/TypeScript compilation/bundling pipeline is planned. Recommend a private, versioned local IPC boundary to one on-demand engine. Keep the domain/API implementation independent of Qt Quick and WebEngine so CLI use requires no display or renderer. No remote control server is required.

The revised recommendation is a Qt-free Rust engine and Rust CLI, with a C++20/Qt Quick UI accessing the same engine over private local IPC. The engine owns all domain behavior and persistence; the GUI owns presentation models, input, composition widgets and renderer integration. Neither client writes the database directly. No in-process Rust/Qt binding is required for this proposal. See [desktop research](../research/desktop-stack.md) and [HTML research](../research/html-mail.md). Language, transport and library choices remain subject to prototype evidence; the module boundaries below are independent of that choice.

```mermaid
flowchart TD
    UI[Mail and calendar UI] --> API[Local commands and paginated queries]
    CLI[Headless CLI and external callers] --> API
    API --> CORE[On-demand local engine]
    CORE --> DB[(Local records and durable operations)]
    CORE --> SEARCH[Local search index]
    CORE --> BLOBS[Message and attachment storage]
    CORE --> SYNC[Bounded sync scheduler]
    SYNC --> GOOGLE[Gmail and Google Calendar adapters]
    SYNC -. After Google dogfooding .-> STANDARD[IMAP and SMTP adapters]
    SYNC -. After Google dogfooding .-> FEEDS[Read-only iCalendar subscriptions]
    SYNC -. Post-MVP .-> DAV[Editable CalDAV adapter]
    GOOGLE --> PROVIDERS[User's providers]
    STANDARD --> PROVIDERS
    FEEDS --> ORIGINS[Subscribed calendar URLs]
    DAV --> PROVIDERS
    CORE --> KEYS[OS credential store]
    CORE --> ASSIST[Optional bounded classification and summary queue]
    ASSIST --> LOCALMODEL[Same-machine Ollama or vLLM]
    LOCALMODEL --> VALIDATE[Validate versioned structured results]
    VALIDATE --> CORE
```

This describes module responsibilities, not a requirement for one process per box.

## GUI/CLI ownership and runtime

A proposed engine process owns migrations, writes, provider synchronization and the operation journal for a profile. GUI and CLI share its capabilities and state; neither maintains a separate mailbox or bypasses the journal. Core owns protocol types/versioning, engine exclusivity and bounded change streams; CLI owns command grammar/output, while providers/Search/Calendar supply the same workflows to either client. Internal database schema is not a public agent API.

Start the engine on demand without an installed startup service. Closing the GUI cancels GUI-owned jobs/reminders; independent explicit CLI commands/watch sessions can continue. Stop after the final active client/job, checkpointing canceled work. Offline queued intent is durable data, not an implicit keep-alive. Verify client crashes, stale endpoint recovery, concurrent launches and mismatched protocol/schema versions without split-brain writers or duplicate submissions.

Use private authenticated local transport with OS-specific permission/peer checks. Noninteractive credential failures must be actionable, never trigger hidden browser login or plaintext fallback. See [CLI contract](cli.md) and [process research](../research/cli-and-local-engine.md).

## Data ownership

- **Accounts and identities:** provider account, authentication reference, allowed From identities, and capabilities. Never use email display text alone as the internal account key.
- **Mail records:** local ID, account-scoped remote identity, headers, message dates, flags, memberships, body availability, and provider thread reference. Use explicit mappings for IMAP mailbox identities and moves.
- **Triage and organization:** read/star provider flags, known hold-set or first-observed timestamps, configured category definitions and provider destination mappings. Manual corrections and pending category mutations survive restart; displayed counts derive from account scope rather than the active filter.
- **Local AI results:** optional cached message/conversation summaries, category suggestions, included message IDs, coverage and source-content revisions, model and prompt/schema versions, category-policy version and provenance. Generated results do not replace source bodies or manual/provider assignments; changed content invalidates affected results without forcing a historical inference sweep.
- **View state:** account-specific selected conversation/member, scroll position, search/category filters and mail split position; independently scoped combined-calendar navigation/visibility and bounded application preferences. Restore mail state without crossing accounts and handle removed items predictably. Mail UI consumes Calendar's bounded day view for its browsable bottom timeline, visible only in Mail; it is another view of the existing event state. See [visual design](../ui-ux.md).
- **Bodies and attachments:** retain every synchronized message and attachment for complete offline use; track durable download completion and retry state. Keep source material needed for rendering and re-indexing; store large binary content outside hot list queries. Content hashes may share bytes without collapsing account ownership or permission boundaries.
- **Drafts and operations:** locally authored drafts, staged attachments, queued commands, attempts and provider acknowledgements. These are durable user data, not disposable cache.
- **Calendars and events:** calendar permissions, provider identifiers, iCalendar UID where applicable, recurrence masters, exceptions, cancellations, separate conference URL/physical location and original time-zone/date semantics. Read-only subscriptions have a separate source identity, protected URL and last valid snapshot; feed event identity includes the source, UID and recurrence identity. Reminder acknowledgements use account/event/occurrence identity and remain separate from RSVP.
- **Derived state:** message-level search indexes, account-scoped conversation projections and occurrence windows can be rebuilt. Gmail uses provider threads; IMAP uses reference-based threading without merging unrelated equal-subject mail. Preserve remote cursors, coverage and local pending operations separately from these projections.

Proposed first storage experiment: a single local SQLite database with account-scoped tables and a separate blob directory. Compare partitioning by account only if the measured write contention, recovery, or query behavior justifies it. A separate search engine requires a durable change journal, replay, tombstones and a visible index watermark.

## Complete offline mail

Eventually download and retain all messages and attachments within the account's synchronized mailbox scope. Recent conversations and user-requested content have priority, while older bodies and attachments continue in a resumable background queue. On-demand retrieval is an acceleration path during initial import, not the final retention policy.

Never silently evict old attachments or replace full history with a rolling window. Estimate and display storage needs, preserve disk headroom, and pause with an actionable explanation if space is insufficient. Store attachments and the message content required for complete offline reading and reply/forward. Do not claim complete offline availability until required content is present.

Offline draft creation is proposed as mandatory reliability behavior; durable queues for sends and supported edits are recommended. Remote deletion and permanent local archival behavior are separate: the full offline copy is not automatically an independent backup. Trash/spam inclusion and retention after remote deletion remain explicit follow-up details.

## Synchronization lifecycle

1. Authenticate and discover capabilities, folders/labels, calendars, permissions, and initial synchronization state.
2. Fetch a useful recent conversation slice; expose it immediately. Track inventory, text indexing and full attachment acquisition separately. Continue until all required mail and attachments are stored; no silent age-based eviction.
3. Interleave history backfill with fresh changes. Allocate bounded per-account and global concurrency, honor retry guidance, and prioritize foreground requests.
4. Apply remote changes and commit their checkpoint safely. Replaying a page must not duplicate records; advancing a cursor must never skip unapplied pages.
5. Recover invalid cursors with staged reconciliation. Keep the readable store available where possible; only conclude that missing records were deleted after a complete applicable inventory. Merge pending local intent deliberately.
6. Resume after sleep, loss of network, authentication expiry or process restart. Checkpoint attachment downloads and verify completeness before declaring them available offline. Pause imports on low disk space without discarding unsent work or silently pruning old content.

Provider state remains authoritative for synchronized records, with pending local intent overlaid until confirmed. Calendar version conflicts must be surfaced or merged under documented rules rather than silently overwriting another device's edits.

## Sending and mutation reliability

Persist the intent and required content before showing a queued action. Make retryable state changes idempotent where the provider allows it. Sending has a separate state machine because an acknowledgement may be lost. An internal operation ID or stable Message-ID alone does not guarantee server deduplication.

Proposed send states: draft, queued, submitting, accepted, failed, and acceptance unknown. A sent-copy failure after acceptance must not cause a second submission. Show enough information to recover without promising recipient delivery merely because the submission server accepted the message.

## Acceptance gates

- Build/test the proposed Rust domain engine without Qt. Exercise the same protocol fixtures from the Qt GUI adapter and Rust CLI; use bounded pages and revisioned changes, keeping Qt models on the GUI thread and database access inside the engine.
- Run equivalent GUI and headless CLI operations against one store/journal, including concurrent launch/mutations, client crashes, schema/protocol mismatches, explicit request-ID retries and shutdown after the final client. CLI startup must not load UI/rendering libraries.
- Replay a provider page and restart mid-transaction without duplicate records or a skipped checkpoint. Invalid cursors reconcile against a complete inventory while readable local state and pending intent survive.
- Read/search during ingestion; retain every synchronized body and attachment. Track inventory, text/index and attachment completion separately; pause on low disk without silent history eviction.
- Preserve authored drafts, staged attachments, manual corrections and queued operations across restart and index rebuild. An uncertain send cannot become an automatic duplicate submission.
- Account removal clears its derived/model results and cancels its work; no search or view can expose data through another account. Coordinate handling of durable unsent work with Platform before removal ships.

## D038 recurring journal and reminder storage

Schema 20 adds nullable bounded `recurring_steps_json` to the existing `calendar_write_operations`; it retains the first following-step lease, outcome and receipt while the same row advances to its frozen replacement. Existing v1/v2/v3 intent, dispatch and receipt bytes remain unchanged. Only recurring requests select version four. The schema bump prevents older engines from interpreting a new two-step intent as an ordinary one-request operation. Metadata-only status exposes step identity and resume eligibility without loading full sources. Related unresolved series work fences new mutations, including legacy occurrence entry points; exact immutable replay retains priority.

The same migration adds `calendar_reminder_receipts`, keyed by account/calendar/occurrence/exact start with shown/dismissed state and timestamp. This is local presentation acknowledgment, never a provider operation. Existing account lifecycle, cached-read policy, quota-before-memory ownership and permission/source publication guards remain unchanged. Following recovery cannot overwrite a newer or absent current master, and a fresh confirmed trim is cached before its never-attempted replacement is admitted.

## Open before implementation

Choose storage/schema and language after evidence; finalize shared IDs, revision/cursor types and error states with consumers. Resolve trash/spam scope, provider-deletion retention, backup/export and account-removal handling for unsent work before those behaviors ship. Actual account/provider counts and complete storage size are unmeasured. [Quality](quality.md) owns workload evidence and targets.

## D052 / P2-LLM consumer boundary

Revision 29 consumes [Local assistance 6](local-ai.md#d052--p2-llm-implementation-boundary): shared discovery/settings/jobs/results/correction commands, schema-21 storage and manual precedence, bounded owned Ollama work, native/CLI parity and scoped acceptance. Runtime implementation/verification is in progress; fixtures 0/3 and protocol 1 remain unchanged. Earlier evidence retains its original revisions.

## D061 label command details

Account-scoped `label_catalog` returns the existing category IDs/names/counts/mappings plus local descriptions and any creation state. `label_create` takes request_id/id/name; `label_describe` takes request_id/id/expected_description/description; `label_remove` takes request_id/id/expected_name/expected_provider_label_id (including explicit null). Writes preserve exact operation replay. Names are unique case-insensitively within an account, at most 256 UTF-8 bytes; descriptions are at most 512 bytes; the local catalog supports 64 labels. Account removal/archival fences new edits. A category ID with a provider creation record cannot be reused for a new local category.

Schema 26 retains provider creation identity independently of category deletion. Mark unknown before the first attempt, record received outcomes through owner cancellation, and project acceptance only to the still-current active category. Explicit Google inventory may reconcile an uncertain matching name. Local label removal clears current memberships/mappings/intents and matching view filters while retaining receipts, source bytes and review/star state. No Gmail label is deleted.
