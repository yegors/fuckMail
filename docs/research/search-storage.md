# Local search and storage

Sources checked 2026-09-11; initial comparison and proposed scale tiers retained. Documentation/source evidence and proposals only; no app prototype, live account flow or benchmark has run. Recheck version-sensitive claims before implementation.

Build boundary: [Core/storage](../contracts/core-storage.md).

## Local search and storage options

| Candidate | Evidence | Project implication |
| --- | --- | --- |
| SQLite + FTS5 | Phrase, prefix, Boolean and proximity search, ranked results, configurable tokenization; trigram substring search has specific limitations | First benchmark candidate: relational filters and text search in one store; assess query latency during writes. [FTS5](https://sqlite.org/fts5.html) |
| SQLite + Tantivy | Embedded Rust search library with ranking, incremental indexing and typed fields; fuzzy-term queries are available | Candidate if agreed search quality/performance needs exceed FTS5; adds index consistency/recovery work. A Rust engine can consume it directly, but that does not remove the consistency cost. [Tantivy](https://github.com/quickwit-oss/tantivy), [fuzzy queries](https://docs.rs/tantivy/latest/tantivy/query/struct.FuzzyTermQuery.html) |
| Notmuch/Xapian | Separates mail indexing/search from the UI and does not receive or send mail | Useful architectural reference; not a complete mail/calendar foundation. Windows packaging and fit would need investigation before reuse. [Notmuch](https://notmuchmail.org/) |

The confirmed search need is sender name/address and subject/body keywords within an account. This strengthens the case for starting with FTS5, while benchmarking conversation grouping and partial address/name matching. FTS5 substring matching is not the same as typo correction, and tokenization is not universal language understanding. Do not infer application performance from library marketing benchmarks.

SQLite WAL permits concurrent readers and a writer, but there is still only one writer at a time; checkpoint behavior and long reads affect performance. Recommendation: short transactions, bounded indexing batches, measured checkpointing, and local-disk storage. Use the backup facilities for coherent copies rather than copying a live database file alone. [SQLite WAL](https://sqlite.org/wal.html)

## Rust storage boundary to validate

Start with rusqlite and a controlled bundled SQLite build, verifying FTS5 availability in the packaged executable. Bundling can avoid divergent system SQLite features but makes its updates our responsibility; native C compilation still exists. Record actual compile options/library version, licenses and target support before accepting it. Mail-store encryption remains a separate unresolved product decision, not something SQLite or the OS credential store supplies automatically. [rusqlite build features](https://github.com/rusqlite/rusqlite).

Candidate execution model: one dedicated writer owner inside the engine, bounded read workers/connections and short account-scoped transactions. rusqlite's Connection is Send but not Sync; do not assume an Arc makes concurrent use safe or hold transactions across network/model awaits. SQLite calls are blocking, so keep them off the asynchronous network executor's ordinary tasks and the GUI thread. Give queued work deadlines, cancellation checks and backpressure; read snapshots should finish promptly rather than pin the WAL indefinitely. [rusqlite Connection](https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html).

Keep required record changes, operation acceptance and the relevant checkpoint in one transaction when they share a persistence boundary. No network acknowledgement or inference call belongs inside that transaction. Select a durability policy explicitly: SQLite documents different guarantees for synchronous modes. Start the durable-draft/operation experiment with WAL and FULL synchronous behavior; optimize only against a stated acceptable-loss policy, not just a faster benchmark. Connection initialization must consistently set the required foreign-key, busy/timeout and durability behavior. [Transactions](https://sqlite.org/lang_transaction.html), [SQLite synchronous policy](https://www.sqlite.org/pragma.html#pragma_synchronous).

Large message/attachment bytes remain outside the hot database. Their completion record and filesystem bytes are not one SQLite transaction. Experiment with a staged-write, verified-content, atomic-finalization protocol and interruption at each boundary. Recovery must distinguish incomplete staging, finalized unreferenced bytes, and a referenced missing/corrupt blob. Never report offline completion until the required bytes are durable and verified. Protect staged draft attachments from cleanup. Design account references before deduplication; garbage collection must respect every remaining account/draft reference. This is a proposed application protocol whose durability must be tested on each target filesystem.

A coherent database backup alone is insufficient for the combined database/blob store. Use the SQLite backup API plus a coordinated content manifest/reference snapshot and a restore test; do not copy only a live database file or treat a retained offline cache as independent archival storage. The final user-facing backup/export feature is still a scope decision; migration recovery needs a safe internal preservation path. [SQLite online backup](https://sqlite.org/backup.html).

## P0-01: inventory and fixture definition

Before a benchmark, record available reference machines, disk headroom, OS/filesystem and build mode. Actual account/provider/message counts remain unknown. The roughly 25 GB eM Client footprint anchors a scenario, not a conversion into message count. Begin with deterministic synthetic mail; request a scoped founder-owned inventory only when it becomes necessary and authorized. Never ingest private mailbox data just to start foundation work.

Define seeded fixtures with several isolated accounts; varied sender names/addresses/languages, subject/body lengths, common and rare terms, quoted replies, skewed conversation lengths, labels/folders, independent unread/star flags and body/attachment availability. Include exact expected matches and conversation membership. Two accounts may contain the same remote-looking ID or attachment bytes without sharing identity. Keep extraction fixtures for HTML/character sets separate from normalized index fixtures so parser faults and retrieval faults can be distinguished.

Use the [quality contract](../contracts/quality.md)'s 100k rapid, 1m primary and 3m stress message tiers, plus roughly 25 GB content and 100 GB growth cases. Generate large corpora later outside version control with a manifest/seed; repeated identical text or sparse placeholder files cannot substantiate realistic search or disk throughput. The fixture design is work to execute, not a dataset that already exists.

## P0-03: storage/search decision packet

Prerequisites: future implementation authorization; a minimal agreed account/message/conversation identity fixture; the selected Rust build on at least the first reference machine. Provider credentials and final UI styling are unnecessary. Start narrow, then carry the same harness into integration.

| Step | Experiment to implement later | Evidence that permits progress |
| --- | --- | --- |
| Correctness first | Insert/replay normalized pages, persist one draft/operation, revise flags, cancel reads and restart at transaction/blob boundaries | No skipped checkpoint or duplicate record, no lost authored fixture, honest blob/index coverage and no cross-account query result |
| FTS design | Compare ordinary FTS content with external-content indexing; define source row ownership, update/delete/rebuild path and normalized sender fields | Correct matching-message IDs and conversation grouping; updates/tombstones/account removal reach the index; no generated summaries in source search |
| Query semantics | Compare Unicode word/prefix search with a narrowly scoped substring strategy for partial sender input; test literal punctuation and supported language examples | Written query behavior and representative retrieval tasks; no accidental exposure of raw FTS syntax as ordinary user-input behavior |
| Rapid benchmark | Run the 100k tier with bounded ingestion, first-page grouping/snippets and concurrent read/star operations | Query plans, p50/p95/p99, memory/disk/WAL growth and cancellation evidence; fix indexing/hydration/locking before changing engines |
| Integrated scale | Repeat at 1m messages and meaningful offline bytes; use 3m/100 GB as stress/growth cases | Results against the existing quality budgets, documented cache protocol and recovery/rebuild costs; expand platform coverage before readiness |

FTS5 external-content tables need deliberately maintained index consistency; creating triggers after pre-existing content does not index that history automatically. Unicode word, prefix and trigram behavior differ, particularly for short substrings. Keep the initial choice simple and test required sender searches rather than enabling multiple indexes speculatively. [FTS5 content and tokenizer documentation](https://sqlite.org/fts5.html).

A minimum durable slice and correct FTS queries can unblock Phase 1 after a recorded provisional storage decision. Million-message load, full import amplification, power-loss behavior and all-platform recovery are later evidence gates before real daily-driver readiness; do not claim that killing one process simulated a power failure. Search comparisons must include account filtering, conversation grouping and snippet hydration, not raw MATCH timing alone.

**Decision output:** the shared schema/fixture revision, ownership/transaction policy, index-maintenance approach, blob recovery rules, known query semantics and benchmark report. Compare Tantivy only if FTS5 misses agreed retrieval or performance needs after inspection. A separate index requires durable replay, deletion propagation and a visible watermark even in a Rust engine. No storage experiment or benchmark has run.

Related acceptance: [Core](../contracts/core-storage.md), [Search](../contracts/search.md), [quality](../contracts/quality.md) and the [Phase 0 plan](../plans/phase-0.md).
