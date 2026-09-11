# Local mail search

State: Draft · Revision: 1 · Updated: 2026-09-11 · Implementation: Phase 0 synthetic experiment verified; whole area remains Draft.

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
