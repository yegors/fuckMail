# Local mail search

State: Draft · Revision: 6 · Updated: 2026-09-23 · Implementation: D093 fresh-profile FTS and planner maintenance; earlier measurements retain their source scope. Whole area remains Draft.

Revision 6 uses bundled SQLite FTS5 contentless-delete storage for new schema32 profiles, retaining explicit rowid/MATCH and account-scoped query behavior. Core runs `PRAGMA optimize=0x10002` after opening/recovery and bounded `PRAGMA optimize` on normal close; generated two-account search tests assert a rare-term FTS-first plan after reopen. No query grammar or public result shape changes. Core51 owns update/delete coverage and Quality68 owns integrity and scale evidence. Real-profile and million-message latency remain unclaimed.

Revision 5 adds conversation `kind` filtering (all/people/automated/unknown), exact sender address filtering and account-scoped sender history. Kind uses the newest incoming member, with newest-member fallback for all-outgoing threads; missing or stale source-hash annotations remain unknown. It is independent of which member matches a keyword or source filter. `exact_sender` combines with sender/category/date/attachment/text predicates on the same member. `filtered_group_counts:{review,held,done}` covers the entire filtered conversation set before paging, using the mutually exclusive displayed groups; account counts retain their unfiltered meaning. `sender_summary` returns at most three conversations ordered by actual latest conversation instant and stable ID plus the full exact-address total. Existing legacy list ordering and opaque cursor rules remain. Annotation/source changes invalidate kinds and cursors; navigation preferences do not. Consumers: Core 13, CLI 7, Triage 6, Mail UI 12 and Quality; schema 7/fresh fixture 3/additive protocol 1.

`collapsed_groups` is an optional unique list of up to three displayed groups (review/held/done). It preserves full filtered counts but excludes those groups before sorting/paging, so a large collapsed review group cannot conceal expanded held/done rows. `expanded_conversation_count`, next offset and cursor describe only expanded rows; the cursor includes collapsed groups in its query scope. Empty/default selection preserves existing paging. Exact sender matching supports the mailbox syntax accepted by the existing MIME parser, using the same Unicode lowercase comparison on input and source candidates; ordinary queries do not fetch extra address columns.

P1-09 verification also covers automatic GUI list refresh during active input. Polling retains a pending list invalidation while a query is loading, search debounce is active, or fewer than 500 ms have elapsed since a user query/filter/folder/category/group-collapse change. It delivers that refresh on the next quiet poll, even if the store revision is then unchanged. Explicit queries, paging and manual refresh remain immediate; status and account refresh continue. Native05 verifies actual independent CLI changes converging in the idle list/reader without losing the active query, selection or draft. Concurrent dev02 still exceeds the unchanged 200 ms paint budget at 200.81 ms for combined filters; the offered 25-source batches every 100 ms remain unchanged.

The subsequent bounded GUI queue correction lets an explicit conversation/draft/sender query precede unsent automatic account/preferences/category/reader/draft/capture reads. An active request is never interrupted; manual reads and every write retain their order as barriers. A pending automatic read can be bypassed at most four times before FIFO forces its progress. The existing 32-request bound and stale-generation cancellation remain. Synchronous callbacks cannot inherit the caller's automatic-read origin, and only the explicit pure-read allowlist is eligible. The complete release01 native walkthrough verifies actual response order, the write barrier and bounded polling progress alongside idle CLI convergence. Dev03 passes all four native 100-observation classes at p95 67.74/73.01/98.64/168.36 ms with 5,150 independently verified concurrent source arrivals. The clean `8743162` Windows package also passes at p95 **70.18/76.96/110.34/189.57 ms**, 100 target/reset pairs per class and 5,100 independently checked arrivals, without SDK paths; the source seed and 200 ms budget are unchanged. Other-host and full-product scope remain separate. [Executed evidence](../research/phase-1-execution.md#p1-09-packaged-concurrent-search--passing-windows-runtime).

Revision 4 binds cursors to the writer instance and search-table generation, preserving pagination through unrelated view/preferences/draft/calendar writes. Search mutations or writer restart explicitly return `stale_cursor`; clients keep cursors opaque and restart the first page. Global response `revision` retains its existing store-wide meaning. Consumers: Core 11, Mail UI, CLI 5 and Quality. The bounded metadata append cache is derived from actual message and FTS document inventories; it never caches complete search responses.

Revision 3 records implemented structured sender/status/attachment/date filters, opt-in review/held/done grouping, account/filtered conversation totals and revision-bound cursor paging. Consumers: Core 10, Mail UI, Triage, CLI 5 and Quality. Protocol 1 remains additive; schema 5/fresh fixture 2 preserve migrated fixture 1. Exact request fields are in the [fixture API](../../fixtures/README.md); measurements and limitations remain in [search execution](../research/search-feasibility-execution.md).

Revision 2 defines the [Phase 1](../plans/phase-1.md) search boundary for the Flight Progress v2 design: real sender/source-text retrieval, typed filters, scoped conversation totals and matching-member navigation. Consumers: Core, Mail UI, Triage, CLI and Quality. Current runtime remains schema 4, protocol 1 and fixture 1.

Revision 1 records D012’s implemented experimental slice. SQLite FTS5 sender/subject/body prefix-AND search now returns account-scoped conversations and matching-member IDs. The measured 100k experiment and limitations are in [execution evidence](../research/search-feasibility-execution.md). Consumers: Core, Qt UI, CLI and Quality. Larger corpora and real mailbox import are unverified.

Owner: D — Search. These are living specifications of accepted requirements and proposed behavior; exact types, libraries and unresolved details are not committed. See [contract workflow](README.md).

## Build boundary

| Boundary | Responsibility |
| --- | --- |
| Owns | Source-text indexing, sender normalization, query/grouping/ranking, snippets, coverage, index generation and search benchmarks |
| Consumes | Core normalized message changes/tombstones, account scope and conversation identities; field/filter query and cancellation |
| Produces | Bounded stable conversation results with exact matching message IDs/snippets, cursor and explicit coverage/index watermark |
| Independent start | Generated normalized messages and deterministic queries; no live providers, UI framework or model required |

Related: [core](core-storage.md), [mail UI](mail-ui.md), [quality](quality.md), [search/storage research](../research/search-storage.md).

## Phase 1 boundary — implemented local slice

The search panel and chips submit typed sender, status, category, attachment-presence and date filters to the shared engine. Sender filtering includes both display name and address; keyword search uses original subject/body and sender text. Derive review/delayed/done filters from the agreed read/star semantics. Keep search and totals within the selected mail account, group Sent as conversations too, and preserve exact matching message IDs through paging and refresh. Clicking a result reveals and scrolls to the matching member, including an earlier collapsed message.

P1-00 fixes filter combination, date/time-zone boundaries, sorting, result/count units and validation in the shared [fixture API](../../fixtures/README.md). Keep ordinary words usable. Offer textual operators only when a parser validates and implements them; reject unsupported or malformed syntax clearly instead of silently ignoring it or treating a displayed operator as an unrelated keyword. UI filter chips and CLI structured arguments share the same query semantics. Do not search generated summaries, classifications or unsent drafts.

Current implemented limits remain explicit: a 512-byte query, at most 32 Unicode alphanumeric prefix-AND tokens, 1–100 conversations per page, offset paging and at most 200 members per conversation. The field selector remains all/sender/subject/body. Structured filters add sender display-name/address substring, status all/review/held/done, has_attachment boolean and strict YYYY-MM-DD after/before dates. After is inclusive and before exclusive at UTC midnight. Source constraints combine on the same member; status evaluates all members. Held includes mixed unread+held conversations. `grouped:true` orders review/held/done, latest conversation instant descending and stable ID; default legacy date ordering remains. Responses include full filtered conversation total, separate unfiltered account counts and coverage. `next_cursor` binds account, complete query/page-size, writer instance and search-table generation; search changes or a writer restart fail explicitly, while unrelated navigation/preferences/draft/calendar writes preserve pagination. Grouping/filtering preserve bounded responses and cancellation. Larger-thread transfer remains a separate Core change. Existing offsets remain available beside the generation-bound cursor.

Core tests and the live CLI smoke verify combined filters, sender/address variations, source-only matching, grouped pagination, stale-cursor rejection and exact matching-member identity. Native matching-member reveal remains part of integrated UI evidence. Account totals cannot depend on the loaded page; filtered-empty, incomplete-index and no-outstanding-work states remain distinct. A result from an earlier query/account must not replace the current view. The extended 100k workload records 100 observations per class, first50 grouped/filter requests and independent normalized-insertion/search producers sharing the one-writer queue. Its initial Phase 1 measurement failed the proposed 100 ms Core search target; subsequent optimization and measured limits are recorded without replacing that failed baseline. These experiments do not establish million-message, provider-import or user-visible paint performance.

## Search

The MVP search bar operates within the current account. Sender name/address, subject and body are the required indexed fields. Support ordinary keyword input with simple mouse-accessible field filters; quoted phrases are a proposed useful addition. Keep sender matching robust to display-name/address variations and evaluate partial names/addresses with real examples.

Results show conversation subject, relevant sender/date and matching text. Clicking a result brings the matching message into view. Relevance versus newest-first ordering needs a simple, consistent default. Proposed advanced operators can follow if they help, but an extensive query language is not an MVP requirement.

Full offline attachment storage does not imply searching inside PDF/Office files. Attachment-content extraction, OCR, typo tolerance, transliteration and semantic search are outside the required search scope. Local AI classification/summaries do not replace the original sender/subject/body index or require embeddings. Language coverage and precise matching semantics need representative examples rather than an assumption of English-only mail.

Show inventory, message-text indexing and attachment-download coverage separately. A partially synchronized account must never be presented as fully searchable or fully available offline.

## Index boundary

Index individual source messages and return conversations in the selected account. Summaries and classification never replace the source index. Keep large attachments out of queries. Benchmark SQLite/FTS5 first; a separate engine is only justified by evidence and requires durable replay, tombstones and a visible watermark. Core owns the schema/change protocol; Search owns its derived index. Calendar search uses Calendar's event views and is tested separately.

## Acceptance gates

- Retrieve by sender display name, address, subject and body; open the actual matching message. Test common/rare terms, partial sender input, account/category filters, languages and long conversations.
- Meet or explicitly revise the [quality budgets](quality.md) with measurements that include grouping and snippet hydration during ingestion and inference.
- Cancellation and rebuilding remain responsive. Report incomplete coverage; replay changes and deletion/account-removal tombstones without stale results or cross-account leakage.

## Open before affected behavior ships

Choose exact partial-match and tokenization semantics, supported language fixtures, phrase behavior and relevance/newest ordering. Obtain representative retrieval tasks and record index size/recovery alongside latency. Storage engine choice is proposed, not accepted.
