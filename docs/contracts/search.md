# Local mail search

State: Draft · Revision: 3 · Updated: 2026-09-12 · Implementation: Phase 1 typed/grouped search slice implemented and tested on Windows; whole area remains Draft.

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

Current implemented limits remain explicit: a 512-byte query, at most 32 Unicode alphanumeric prefix-AND tokens, 1–100 conversations per page, offset paging and at most 200 members per conversation. The field selector remains all/sender/subject/body. Structured filters add sender display-name/address substring, status all/review/held/done, has_attachment boolean and strict YYYY-MM-DD after/before dates. After is inclusive and before exclusive at UTC midnight. Source constraints combine on the same member; status evaluates all members. Held includes mixed unread+held conversations. `grouped:true` orders review/held/done, latest conversation instant descending and stable ID; default legacy date ordering remains. Responses include full filtered conversation total, separate unfiltered account counts and coverage. `next_cursor` binds account, complete query/page-size and writer revision; stale cursors fail explicitly. Grouping/filtering preserve bounded responses and cancellation. Larger-thread transfer remains a separate Core change. Existing offsets remain available beside the revision-bound cursor.

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
