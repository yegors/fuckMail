# Local search and storage

Sources checked 2026-09-10; reorganized 2026-09-11. Documentation/source evidence and proposals only; no app prototype, live account flow or benchmark has run. Recheck version-sensitive claims before implementation.

Build boundary: [Core/storage](../contracts/core-storage.md).

## Local search and storage options

| Candidate | Evidence | Project implication |
| --- | --- | --- |
| SQLite + FTS5 | Phrase, prefix, Boolean and proximity search, ranked results, configurable tokenization; trigram substring search has specific limitations | First benchmark candidate: relational filters and text search in one store; assess query latency during writes. [FTS5](https://sqlite.org/fts5.html) |
| SQLite + Tantivy | Embedded Rust search library with ranking, incremental indexing and typed fields; fuzzy-term queries are available | Candidate if agreed search quality/performance needs exceed FTS5; adds index consistency/recovery work and a Rust bridge if the app uses C++. [Tantivy](https://github.com/quickwit-oss/tantivy), [fuzzy queries](https://docs.rs/tantivy/latest/tantivy/query/struct.FuzzyTermQuery.html) |
| Notmuch/Xapian | Separates mail indexing/search from the UI and does not receive or send mail | Useful architectural reference; not a complete mail/calendar foundation. Windows packaging and fit would need investigation before reuse. [Notmuch](https://notmuchmail.org/) |

The confirmed search need is sender name/address and subject/body keywords within an account. This strengthens the case for starting with FTS5, while benchmarking conversation grouping and partial address/name matching. FTS5 substring matching is not the same as typo correction, and tokenization is not universal language understanding. Do not infer application performance from library marketing benchmarks.

SQLite WAL permits concurrent readers and a writer, but there is still only one writer at a time; checkpoint behavior and long reads affect performance. Recommendation: short transactions, bounded indexing batches, measured checkpointing, and local-disk storage. Use the backup facilities for coherent copies rather than copying a live database file alone. [SQLite WAL](https://sqlite.org/wal.html)

## Next experiment

P0-01: Establish actual account/provider inventory and message counts. Use the supplied approximately 25 GB local-store footprint as a starting reference, not a complete server inventory; model 100 GB growth and high-end hardware.

## Search experiment

P0-03: Benchmark SQLite/FTS5 for sender names/addresses and subject/body keywords, including conversation grouping under concurrent ingestion. Evaluate Tantivy only if needed. Measure index size and recovery effort alongside latency.

Related acceptance: [Search](../contracts/search.md) and [quality and performance](../contracts/quality.md). The supplied 25 GB disk snapshot and all synthetic scale tiers are retained in the quality contract.
