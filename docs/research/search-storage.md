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

Related acceptance: [Core](../contracts/core-storage.md), [Search](../contracts/search.md) and [quality](../contracts/quality.md).

## 2026-09-23: schema-29 storage review

This is a read-only review of schema-29 source on `main` at `31688003d8c0` with concurrent uncommitted work, aggregate metadata from the local `default` profile, and `EXPLAIN QUERY PLAN` on that profile. No message text, identifiers, credentials or raw profile files were copied into the repository. The profile was active during inspection, so row counts are a point-in-time sample. The external `blobs/` total supplied in a separate review was not independently verified. The proposed work packet is awaiting review; none of its schema or cleanup changes is implemented.

| Observation | Read-only evidence and limit |
| --- | --- |
| Profile scale | SQLite file: 148,025,344 bytes; 5,423 messages and two accounts at the first count. This is not a complete offline-content or representative-scale benchmark. |
| Retained Gmail work | 3,269 of 3,269 `google_live_pages` were complete; all 8,914 page-item rows belonged to completed pages. `dbstat` attributed about 36.3 MB to page-item table pages, 3.0 MB to page table pages, plus their indexes. No routine completed-page deletion was found in Core. Current `google_live_coverage` still queries a completed delta page as a freshness proof. |
| Retained Calendar work | All 4,020 `calendar_sync_runs` were published; 5,315 items remained, with no recorded item errors. No source had a pending generation or a source generation beyond its published generation at inspection. `dbstat` attributed about 7.5 MB to items, 1.2 MB to runs and 0.8 MB to pages before their indexes. Calendar coverage and cleanup still reference published run IDs, so publication alone is not a safe deletion rule. |
| Derived text and results | `message_fts_data` and `message_fts_content` occupied about 21.9 MB and 17.2 MB respectively; `messages` occupied about 19.9 MB. `local_model_results` occupied about 10.8 MB; its 1,323 `snapshot_json` values totalled about 7.3 MB. Those snapshots contain identity, settings and source/text hashes, not another copy of message bodies. |
| Calendar source copies | All 1,201 joined `calendar_sources.json` and `calendar_event_state.observed_json` values compared equal in this profile. They can differ for sparse cancelled observations, so equality in this sample does not make the second field universally redundant. |
| Index footprint | The database has 49 named indexes, 60 SQLite autoindexes and 12 named indexes whose SQL contains `json_extract`. Its account/message/conversation ID lengths were 71/78/77 characters; observed remote message IDs were 16 characters. Provider IDs remain opaque rather than a fixed-length storage contract. |
| Planner | The ordinary account-scoped FTS query started at `messages_projection` and probed FTS per message; a forced FTS-first form started at FTS and looked up message rowids. The profile has no `sqlite_stat1` table. The quoted 55–60 ms versus under-3 ms comparison was not reproduced here. The opt-in 100k benchmark explicitly runs `ANALYZE`; normal `Store::open_profile` does not. |
| Dates and categories | `ORDER BY julianday(sent_at)` used a temporary sort even for one conversation. `julianday(sent_at)` date filters cannot use a stored-time range in the existing index; plain `sent_at` range/order queries can. Category filtering used only the `(account,message_id,category_id)` primary-key prefix because no `(account,category_id,message_id)` index exists. The narrower `messages_conversation` index was selected for a plain per-conversation date order, so overlapping columns alone do not prove it removable. |

The implementation keeps one Store writer, WAL, `synchronous=FULL`, foreign keys, trusted-schema protection and a two-second busy timeout. GUI/CLI calls reach the serialized engine worker; remote HTTP and bounded content preparation are off that writer. A read-only connection might improve measured queue delay but would need consistent snapshots and invalidation without changing deliberate read-on-open behavior. [SQLite's WAL documentation](https://sqlite.org/wal.html) also notes that long readers can delay checkpoints.

For planner maintenance, [SQLite recommends `PRAGMA optimize`](https://sqlite.org/lang_analyze.html) at opening and periodically for long-lived connections, and after schema changes. It is a proposal to measure on a disposable profile before adding it to production. Cache size, memory mapping, in-memory temporary tables, a WAL size limit and incremental auto-vacuum are separate tuning choices, not established fixes. [Incremental auto-vacuum](https://sqlite.org/pragma.html#pragma_auto_vacuum) needs an explicit call to reclaim freed pages and does not remove retained journal rows. Current mail-retention settings bound initial acquisition; they do not age out saved messages.

## 2026-09-23 audit notes

The following audit notes were supplied by another AI after the initial review. They describe a read-only check of schema-29 source and the local `default` profile using aggregate counts, `EXPLAIN QUERY PLAN` and warm timings. The planner-statistics test ran on an in-memory backup. No message text or identifiers were included in the notes. The profile was active, so counts are point-in-time. The assessment below independently checks the proposed changes against code; it does not reproduce the reported timings. These notes approve nothing above and change no readiness.

### Corrections to the earlier separate review

- **Journal disposal:** withdrawn as "delete once complete/published". `google_live_coverage` reads the previous completed delta page as a freshness proof ([google_live_mail.rs:532](../../crates/fm-core/src/google_live_mail.rs#L532)). Calendar readiness reads the run named by a source generation ([combined_calendar.rs](../../crates/fm-core/src/combined_calendar.rs)). Compaction by proof, as packet B requires, stands.
- **Calendar observations:** equal in this sample, but that is not a safe single-copy rule. Withdrawn in favour of packet C's reversible override.
- **Local-model snapshots:** these are not copies of message content. Withdrawn; see gap 5 for what they do repeat.
- **Retention and auto-vacuum:** withdrawn. `mail_since` is read only by acquisition, so retention creates no deletion workload.
- **Blob store:** 765,958,485 bytes across 5,929 files. The earlier "746 MB" was a rounded `du` figure.
- **`messages_conversation`:** it is a strict prefix of `messages_projection`, but SQLite prefers the narrower index. At about 1.7 MB, packet B's keep-until-measured rule is fine.

### Verified

- **Search planner: reproduced.** The account-scoped inbox search was timed as the current join versus a forced FTS-first join. Each figure is the median of seven warm runs on SQLite 3.45.1 (Python); the 3.50.4 CLI chose the same plan. This supersedes the "not reproduced" note in the [research review](search-storage.md#2026-09-23-schema-29-storage-review).

  | Term | Rows | Current plan ms | FTS-first ms |
  | --- | ---: | ---: | ---: |
  | meeting | 54 | 57.62 | 0.03 |
  | invoice | 131 | 67.13 | 0.04 |
  | the | 2,536 | 143.62 | 7.65 |
  | re | 3,110 | 62.12 | 6.98 |
  | (no match) | 0 | 41.57 | 0.01 |

- **Planner statistics fix it.** On the in-memory copy, `PRAGMA optimize=0x10002` and a full `ANALYZE` each switched the ordinary query to FTS-first: meeting 43.47→0.03 ms, the 46.84→0.64 ms, no match 27.91→0.01 ms. The bundled SQLite is 3.46.0 (`libsqlite3-sys` 0.30.1), which supports this `optimize` form.
- **Two secondary plans confirmed.** Other-account recipient suggestions use `messages_order (account=?)` and then a temporary sort. Calendar mail counts use `google_mail_reconcile (account=? AND deleted=?)` with no provider-date range.
- **Old-engine refusal confirmed.** A version-29 engine opening a version-1 profile runs `migration-002.sql` ([lib.rs:189](../../crates/fm-core/src/lib.rs#L189)). Decision 1's version-30-or-higher rule is required.

### Gaps in this proposal

1. **Ship the planner fix first.** One statement at open, plus periodic or at-close `PRAGMA optimize`, fixes the measured plan. Make it a standalone packet with a plan-assertion test and the existing search tests, not gated on packet A's corpus work. Re-verify on a large generated profile afterwards.
2. **Split journal compaction into payloads and headers.** Payload rows are the bulk: about 36.3 MB in `google_live_page_items` and 8.3 MB in Calendar items and pages. Headers are about 3.0 MB of Gmail pages and 1.2 MB of Calendar runs.
   - Every payload read found is on active work. Listing pending items returns nothing once a page is complete ([google_live_mail.rs:324](../../crates/fm-core/src/google_live_mail.rs#L324)). Re-beginning an existing page compares only the header fingerprint ([google_live_mail.rs:272](../../crates/fm-core/src/google_live_mail.rs#L272)). Calendar item and page reads are scoped to a run lease.
   - One exception must stay: a published run whose sources still await consolidation can receive a lease again ([live_calendar.rs:95](../../crates/fm-core/src/live_calendar.rs#L95)). Keep its rows until consolidation finishes.
   - Deleting completed or published payloads is therefore the larger saving with the simpler proof, and header retention can stay conservative. Packet B's crash/replay tests still apply.
3. **Consider the FTS index shape.** The index is ordinary FTS5 with `prefix='2 3 4'` and `detail=full`.
   - Search emits only AND-ed single-token prefix phrases, optionally column-filtered ([queries.rs:850](../../crates/fm-core/src/queries.rs#L850), [queries.rs:1418](../../crates/fm-core/src/queries.rs#L1418)). It uses no FTS auxiliary functions; snippets are built in Rust.
   - A synthetic `detail=column` table accepted every one of those query shapes. It rejected only a multi-token phrase: "phrase queries are not supported (detail!=full)". [SQLite's FTS5 documentation](https://sqlite.org/fts5.html) reports one index at 743 MiB with `detail=full` and 340 MiB with `detail=column`.
   - Measure `detail=column` and fewer prefix lengths on the generated corpus. Also confirm that Rust's `is_alphanumeric` splitting never produces a term that `unicode61` splits into several tokens; such a term would become an unsupported phrase.
4. **Give receipt and work tables a retention rule.** `operations`, `google_mail_mutations`, `capture_operations`, `calendar_write_operations` and `preference_operations` are deleted only by account removal. They are small now (508 operations; 68 confirmed and 14 superseded mutations), but every flag change adds rows. Keep unresolved or uncertain rows and anything replay needs. Then either age out terminal rows after a bounded window, or record that permanent growth is accepted.
5. **Deduplicate local-model settings through a new table.** The `settings` object is 6.0 MB of the 7.3 MB of `snapshot_json`: about 4.5 KB per row, most of it a roughly 4 KB prompt. The 1,323 rows hold only 13 distinct settings values. `local_model_settings_history` cannot be the reference target, because only migrations write it (3 rows). Use a content-addressed settings table referenced by hash, and keep the stale-result comparison exact.
6. **Retire the Rust projection caches with the summary.** If packet C adds a persisted conversation summary, remove the `update_hook` change tracking and the conversation, projection and coverage caches ([queries.rs:86](../../crates/fm-core/src/queries.rs#L86), [queries.rs:136](../../crates/fm-core/src/queries.rs#L136)). Maintaining both leaves two sources that can disagree.
7. **Sequence.** D038 asks to avoid over-engineering, and the founder is willing to start fresh. Run gaps 1 and 2, plus packet B's two index additions, first with focused correctness and crash tests, then decide packet C from a smaller baseline. Packet A's full tier list remains the acceptance gate for scale claims, not a prerequisite for these fixes.

### Assessment for the review plan

- **Adopt the planner work as packet 0.** The follow-up timings give a strong reason to test planner statistics promptly. They were measured by the separate audit, not reproduced in this review on the bundled engine. The proposed change still needs exact-result and plan checks on a disposable profile and then the generated corpus.
- **Adopt payload/header compaction as an investigation, with a correction.** The audit's statement that completed Gmail payload reads serve only active work overlooks `google_live_commit_part`: it reads `google_live_page_items` and returns the exact prior part receipt before checking whether the page is complete ([google_live_mail.rs:417–439](../../crates/fm-core/src/google_live_mail.rs)). Deleting those rows without an equivalent compact proof would change replay. Calendar rows also need consolidation and generation checks. Packet B therefore makes replacement proof and restart tests prerequisites for pruning.
- **Adopt FTS shape and settings deduplication as measured candidates.** `detail=column` is compatible with the current simple prefix shapes in the audit, but [FTS5](https://sqlite.org/fts5.html) excludes phrase and proximity queries in that mode. Test real tokenizer boundaries and future query needs before changing it. A separate read-only aggregate query reconfirmed 1,323 local-model result rows, 13 distinct settings values and 5,985,239 bytes of settings JSON at a later point-in-time. The repeated settings bytes justify testing an immutable shared settings object while preserving exact stale-result authority.
- **Qualify receipt retention.** Confirmed operation rows are small today, but the current exact-replay contract has no safe time-based expiry rule. Packet B requires a durable duplicate fence or an explicit contract revision before compacting terminal receipts; unknown outcomes must remain recoverable.
- **Reject blanket cache retirement.** A conversation summary may replace the conversation projection after parity checks. The update hook also drives search generations and the first-page and coverage caches ([queries.rs:136](../../crates/fm-core/src/queries.rs)). Evaluate each cache and its invalidation separately.
- **Keep focused fixes ahead of scale claims.** Packet 0 and proof-preserving journal/index changes can run with targeted checks. The quality corpus and package gates remain necessary before claiming scale or product readiness.

## 2026-09-23 packet 0 execution

D091 authorizes a fresh-profile cutover with no old-profile migration. Packet 0 is being developed in the managed isolated checkout `sqlite-reset` from `31688003d8c0`, while D090's schema-30 work continues in the shared tree. The candidate runs bundled SQLite `PRAGMA optimize=0x10002` after Store open/recovery and best-effort `PRAGMA optimize` on normal Store drop. It has not been integrated into the D090 build or run on the current real profile.

A new disposable 6,000-message/two-account regression passes: after reopening, `sqlite_stat1` contains message statistics, `EXPLAIN QUERY PLAN` starts the rare-term search at FTS, and the account-scoped query returns the one expected conversation. `cargo fmt --all -- --check` and strict offline Core library Clippy pass. The scoped command `cargo test -p fm-core --lib --offline --locked reopened_mailbox_builds_search_statistics_and_starts_at_fts -- --nocapture` passes (1/1). The full isolated Core library suite reports 336 passed, three ignored and three failed in 112.32 seconds. Each failure was rerun on a separate untouched-HEAD worktree and reproduced with the same assertion: `google_labels::tests::linking_after_import_projects_saved_labels_without_inbox_or_remote_writes` expects schema 20 but sees 29; `mutations::tests::a_new_reply_does_not_inherit_an_earlier_snapshot_action` expects an older totals object; `phase1_tests::phase1_status_filters_use_entire_conversation_and_source_filters_share_one_member` expects 90 where current behavior returns 85. These are baseline failures, not a passing whole-Core gate.

The isolated candidate now also has a direct schema-31 creation path from one generated DDL snapshot rather than replaying the old migration chain. Its 60 ordinary tables are `STRICT`; live and synthetic creation/reopen, foreign-key checks and pre-reset version refusal pass. The generated schema is still a prototype based on schema 29 and must be reconciled with D090 before integration. The historical upgrade tests are marked ignored because D091 removes that behavior; fresh-profile/refusal tests replace their version boundary coverage. One Google Draft paging test now uses malformed routing JSON to check projection isolation without inserting a BLOB into a `STRICT` TEXT column.

After excluding the three independently reproduced Core baseline failures, the candidate Core library run reports **315 passed, 26 ignored, 0 failed** in 97.23 seconds. A fresh live/synthetic/refusal check passes 3/3, and `cargo fmt --all -- --check` passes. The candidate engine run reported 161 passed and three failed; two Gmail full-inventory assertions reproduce on untouched HEAD (baseline engine: 162 passed, two failed), while the third candidate failure was a test that opened an old schema. That handshake test now uses the fresh fixture and awaits rerun. No whole-suite pass is claimed.

The existing 20,000-message generated browsing workload was run sequentially on untouched HEAD and the rebuilt candidate (same debug configuration and host, 50 observations). Both returned the same first conversation. Baseline/candidate p95 milliseconds were **38.76/31.12** for page, **0.85/0.95** for reader and **43.15/43.95** for review-refresh. An initial candidate result used a stale baseline Cargo binary because two worktrees shared a target directory; it was discarded and the candidate package rebuilt before the reported run. These are diagnostic warm-cache timings under concurrent host work, not evidence of a performance improvement. Disposable reports are ignored under `.tools/`.

A later isolated FTS candidate changes only the fresh baseline's `message_fts` to [SQLite's contentless-delete mode](https://sqlite.org/fts5.html#contentless_delete_tables), preserving current `INSERT`/`DELETE`, rowid joins and `MATCH` expressions. On the 20,000-message corpus, `dbstat` totals for the FTS shadow tables were **5,996,544 bytes** contentless versus **9,134,080 bytes** for a full-content mirror built from the same `messages` rows (34.4% lower). This corpus uses short generated bodies; it does not predict the real-profile saving. The candidate Core run with this FTS shape reports **316 passed, 26 ignored, 0 failed** after excluding the three baseline failures, including search, source-version mutation and coverage checks. The candidate engine run after rewriting its one old-schema handshake test reports **162 passed** in its main target after excluding the same two Gmail sync failures reproduced on untouched HEAD; process targets report eight passed/one ignored and two passed. These results do not cover the current D090 source or packaged GUI/CLI.

The candidate also clears `google_live_page_items.manifest_json` in the same transaction that finishes a page. It keeps each row's manifest hash, part key and applied revision so a repeated `google_live_commit_part` returns the exact receipt. The focused interrupted-page/reopen/replay test checks that completed payloads are absent and replay still matches. A rerun after this change retains the candidate Core result of 316 passed/26 ignored (three baseline tests filtered) and the engine result of 162 passed (two baseline tests filtered), with eight passing session and two passing verification process tests. This clears large staged JSON for future page rows but does **not** prune proof rows or reclaim an existing database file; broader journal compaction still needs the packet B proof/recovery work.

The isolated offline `cargo test --workspace --locked` run then passed **672 tests**, with 29 ignored and the five independently reproduced baseline failures filtered. It includes Core coverage/mutation/search processes, CLI arguments, engine sessions and provider feasibility. `cargo fmt --all -- --check`, strict workspace all-target Clippy and `git diff --check` pass. This is source-level candidate evidence; D090 integration, native UI, configured package, other hosts and real cutover remain open.

A subsequent focused 4/4 fresh-schema run also verifies an interrupted live initialization marker cannot seed synthetic accounts when the first SQLite transaction was absent. The preceding workspace count predates this one additional test.

Calendar staging now releases successful `calendar_sync_items.json` after publication in the publication transaction. Published source/catalog rows retain the content; failed items keep their raw JSON for diagnosis, and item fingerprints plus run/page headers remain for replay and coverage. A new publish/reopen test passes, and the existing duplicate-identity test now asserts the failed item's JSON remains. The scoped Calendar suite passes 28 tests with two historical migration tests ignored. This is payload compaction only, with no measured file-size reduction or proof-row pruning yet.

After the Calendar change, the isolated offline workspace rerun passes **674 tests**, with 29 ignored and the same five baseline failures filtered. Core contributes 318 passing library tests. This supersedes the earlier 672-test candidate run; native/package/D090 integration remain unverified.

The generated 20,000-message corpus also probed the category filter's `message_categories(account,category_id)` lookup. With 20,000 common associations and 198 selective associations across two accounts, the current primary key yielded `account=?` only and 100 warm selective lookups had **0.824 ms p95**. A covering `(account,category_id,message_id)` index yielded both equality terms and **0.062 ms p95**; `dbstat` measured **1,355,776 bytes** for that index. A single rolled-back insert of 20,000 associations took **57.25 ms** without and **77.10 ms** with the index. This one-host write measurement is directional, not a throughput guarantee. The fresh-schema candidate now includes the index, and a focused plan assertion passes. Its insertion into the baseline predates another full workspace run.

The candidate also reserves version 31 as the reset baseline and derives later versions from an ordered post-reset SQL-step list, each applied in its own transaction with `user_version`. That list is empty today; the four fresh-profile, interrupted-initialization and pre-reset refusal tests pass after adding the runner. No migration from versions below 31 is present.

D090 subsequently added a schema-31 retention migration during its active work. The isolated reset candidate moved its baseline to **32**, incorporated the expanded retention `CHECK` values in its direct DDL, and now refuses versions 2, 29, 30 and 31. The focused four-test fresh-schema suite passes after that adjustment. The earlier 674-test workspace result is from the candidate before this version/constraint update; integration against D090's final sources remains required.

The separate audit's real-profile FTS timing remains its own evidence. D090 integration, current-source engine/CLI/native checks, generated-corpus breadth, and a configured package remain open. No mailbox content, identifiers, auth data or generated database entered Git.

## 2026-09-23 shared-tree integration

D093 merged the isolated reset implementation into the current dirty D092 Windows tree at `3168800` plus its uncommitted UI, retention, CLI and engine work. The integration preserved D092's new account commands and schema31 retention constraint; `fresh-schema.sql` creates their expanded `90_days`, `2_years` and `5_years` values directly at schema32. The old `migration-031-retention.sql` remains historical source and is not replayed for new profiles. `Store::open_profile` refuses versions 1–31 before profile identity and recovery logic. A post-reset SQL-step list is empty today and commits each future step with its own version. `PRAGMA optimize=0x10002` runs after recovery; normal Store drop best-effort refreshes planner statistics. New FTS is contentless-delete, and completed Gmail/Calendar staging rows clear payload JSON while keeping replay/proof fields.

The combined offline Rust workspace passes **691 tests, 29 ignored, zero failed**. Core library: 331 passed/26 historical upgrade tests ignored; engine library: 166 passed; engine sessions: 8 passed/1 ignored. Existing three fixed-version/count assertions were corrected to compare current schema and actual count fields. Three D092-era tests now compare the plain reader response to the flag receipt without its receipt-only `mail_undo` metadata, while still checking exact immutable retry responses. Two Gmail engine failures were caused by a synthetic HTTP fixture classifying both the recent and retained-range `q=after:` list as recent; the fixture now distinguishes the first recent query from the inventory query. All ten focused Gmail mail worker tests pass, including the previously failing continuation and coverage cases. No provider account was contacted by these tests. Strict workspace all-target Clippy, Rust formatting and repository hygiene pass.

The configured Windows Release tree is `dist/windows-amd64/bin/{fuckmail-gui.exe,fm.exe,fm-engine.exe}`. The relocated package script passed SDK-free CLI search, GUI smoke, empty-live CLI/GUI startup, normal default creation under an isolated `LOCALAPPDATA`, setup-status readback and final engine shutdown at each scoped step. Direct staged CLI readback on another disposable profile returned `schema_version=32`, `profile_kind=live`, `fixture_revision=0`. The final broad `--self-test` gate fails, so the script did not emit a verified ZIP. Its first failure was an older pointer helper omitting a move before pressing Done; after matching the focused native pointer sequence, it progressed to Settings stage4 because the test targeted D092's read-only account-name label. The Settings harness now uses the Account Editor and passes through its appearance and draft checks. The next direct staged run reaches native search case4 and times out at stage5 while restoring the saved list position (`listY=333`, `readerY=0`); the exact failure and earlier Settings errors are in ignored local logs. This is **not** a passed full GUI or packaged acceptance gate. The staged executable directory passed the scoped checks and can be launched. D092's prior native control/frame evidence remains separate.

The standard Windows profiles were located read-only: `%LOCALAPPDATA%\fuckMail\default\store.sqlite3` (148,627,456 bytes at inspection) and `%LOCALAPPDATA%\fuckMail\demo-v1\store.sqlite3` (1,368,064 bytes). Both have external `blobs/` directories; neither had a `store.sqlite3-wal` or `-shm` file at inspection. The agent did not open, move, delete or reset either profile. Moving the complete `default` directory after all clients exit is the founder's cutover step; a new launch creates the empty schema32 profile, followed by re-authorization and explicit sync. This is a local development handoff, not the 100k/1m/3m performance or other-host result.

Staged binary SHA-256 values at handoff: GUI `C7056540B826984DE8521915833106243483843C63CFAE01AE086C37D4E5F15C`; engine `5AA548C182477BEB61EF59F9EFFCB7858CA2444B23531FC2F4209294BE548BF2`; CLI `476D0AF970239CFCBC17880017E9FAC0D8B635C5273C9CC9E704BF865DB1AA30`. The staged README and build notes name the required profile move. The package directory and generated test logs remain ignored local artifacts; no archive was created by the failed final self-test gate.

## 2026-09-23 legacy SQL cleanup

D095 supersedes D093's decision to leave migration source files in the active tree. The 30 tracked pre-reset migration SQL files, the untracked schema-31 retention SQL, and the old `schema.sql` baseline were removed. The tracked files remain in Git at `3168800`; the v31 retention choices are in `fresh-schema.sql`. The 23 ignored historical upgrade tests, their `test_profiles.rs` helper and dedicated `migration_tests.rs` module were removed. One active content test helper now uses the fresh schema and names the `accounts` columns explicitly. The production schema32 create/refusal path and empty future-migration runner are unchanged; no user profile was opened or modified.

The first full run found two content tests inserting four values into the new seven-column `accounts` table. After correcting their test setup, the focused 16 content tests pass. The final offline Rust workspace passes **691 tests, 6 ignored, zero failed**; the six retained ignores are three Core performance experiments, one engine test and two Google tests. The earlier D093 691/29 result remains its historical checkpoint. Strict workspace all-target Clippy, Rust formatting, repository hygiene and `git diff --check` pass. This cleanup did not repeat GUI/package acceptance; D093's search scroll-restoration failure remains open.


## 2026-09-23 audit corrections

D096 fixes the post-D095 audit findings in the current shared tree. No real profile was opened for these changes; generated and disposable test profiles only.

- **Fresh-profile planner statistics.** A new live profile runs open-time `PRAGMA optimize=0x10002` while `messages` is empty, so no statistics describe it. `fresh_profile_first_sync_refreshes_search_statistics_without_restart` inserts 6,000 generated messages into such a profile without reopening. On the bundled SQLite 3.46.0, without the refresh the plan is `SEARCH m USING INDEX messages_projection (account=?)` followed by per-row `SCAN message_fts VIRTUAL TABLE INDEX 0:=M3`, and the test fails. With the refresh it starts at FTS. The writer now repeats bounded `PRAGMA optimize=0x10002` after a completed Gmail page, at most every five minutes, outside any transaction. A 3.50.4 CLI simulation showed that statistics from even 60 rows already yield the FTS-first plan, and that the next open re-analyzes after substantial growth. `ANALYZE` does not change `total_changes()`, so this does not disturb the update-hook change accounting.
- **Undo.** `mail_undo` no longer records a human intent for every account category and flag. `mail_undo_records_and_delivers_only_the_fields_it_restores` checks that undoing a read change stores and delivers only `read`, leaves model labeling unblocked, and queues no removal when a local label is later linked.
- **Reminders.** An explicit null reminder is "none" again, as Calendar30 already specifies; only an absent value uses the saved default. `google_events_with_reminders_turned_off_never_alert` covers a synced Google event with `useDefault:false` and no overrides.
- **Auto-read.** Toggling a label's Auto-read no longer deletes model People/Automated results (checked in `receipts_auto_read_defaults_on_and_respects_account_label_toggle`). The engine's arrival flag is an internal argument and is ignored in client requests.
- **Retention.** Pruning uses Gmail's internal date when coverage has it, matching acquisition, and deletes conversations left empty and unreferenced. See `retention_uses_gmail_received_date_and_removes_emptied_conversations`.
- **Cleanup.** Account listing reuses one oldest-message query for `cached_since` and `mail_oldest_at`. Undo reads the category list once. Reminder listing reads preferences once.

The full offline Rust workspace passes **695 tests, 6 ignored, zero failed**: D095's 691 plus the four new tests above. Strict workspace all-target Clippy with warnings denied, Rust formatting and repository hygiene pass. GUI, package and scale gates were not rerun.

## 2026-09-26 ten years of Inbox months

`cargo test -p fm-core --release --lib inbox_months_ten_years_60k -- --ignored` seeds one account with 60,000 conversations, 500 a month for 120 months: 300 unread, 50 starred, the rest Done. It then reads the first Inbox page 30 times for each folding: none, every other month, and all but the newest.

| Months folded | Uncached first page | Cached p50 | Cached p95 |
| --- | --- | --- | --- |
| 0 | 28.5 ms | 0.08 ms | 0.24 ms |
| 60 | 38.4 ms | 0.10 ms | 0.12 ms |
| 119 | 38.0 ms | 0.12 ms | 0.14 ms |

Windows 11, SQLite 3.46.0, one connection, warm OS cache, no concurrent writer. The query is fast at this size. The month-toggle jump the founder saw came from refreshes delayed behind sync writes (D149).

## 2026-09-27 received attachment storage and extraction

D152 retains exact incoming MIME once and resolves attachments from that source. SQLite keeps decoded checksums, sizes and parser positions. Authored files and outgoing MIME remain durable. Exports target 512 MiB, with a five-minute recent-use grace and retry of locked files. Original messages still download during sync; this reduces permanent duplicate storage, not network transfer.

Windows release build, generated binary attachments in temporary demo3 profiles, warm OS cache after import, no concurrent writer. The experiment measures actual additional durable blob bytes, checks exact exported bytes, and averages ten repeated exports. It measures extraction and file publication, not image/video/PDF rendering or a cold-disk percentile.

| Decoded attachment | Original MIME retained | Permanent bytes saved | First export | Mean repeat |
| --- | --- | --- | --- | --- |
| 1 MiB | 1,398,604 B | 1,048,576 B | 13.536 ms | 0.926 ms |
| 5 MiB | 6,991,008 B | 5,242,880 B | 40.724 ms | 0.941 ms |
| 20 MiB | 27,962,529 B | 20,971,520 B | 146.507 ms | 1.114 ms |

The saved bytes are the separate decoded blob that the previous import wrote alongside the same original. Opening a file adds a disposable decoded export. Repeated previews reuse its verified file stamp, and authored operations reuse the same copy. Different messages retain different originals even when their attachments match; exact whole-blob hashes still deduplicate identical originals and authored files.

Scoped regressions cover source-only sync and staged replay, exact offline reopen/cache regeneration, invalid sources and part positions, schema37-to38 conversion, unavailable originals, shared authored hashes during cleanup, draft attachment survival after received-message purge, independent account transfer, and eviction that preserves recent exports and user downloads. An engine test holds received attachment preparation while an unrelated writer query completes, then deletes the export and checks exact regeneration.

The existing Windows native attachment walkthrough passed against the new engine: draft import/removal/retry, preview handlers, synthetic capture and independent CLI inspection after GUI/engine shutdown. This uses temporary synthetic profiles; it makes no claim about a physical file picker, live sending, or other platforms. Initial checks exposed a Windows file-timestamp handle permission error and an old test expecting a received decoded blob; both were corrected. One full-suite attempt hit the existing simultaneous-client status comparison while background indexing advanced `mail_generation` between reads.

The final full Rust workspace run passed 831 tests, with 9 ignored and none failed, including the simultaneous-client test. After strengthening the received-draft test to remove its export before both reopening and capture, the attachment slice passed again (6 passed, 1 measurement ignored). Strict workspace/all-target Clippy, formatting, repository hygiene and diff whitespace checks passed. The ignored release measurement was run separately and passed.

Generated logs and measurement output stay in ignored `build/verification/attachment-storage/`. No real mailbox was opened or modified for these checks. Older duplicates are converted in the background, then considered for deletion during the next startup; shared references and journals can retain them longer.

## 2026-09-27 running-engine I/O and memory audit

The founder reported approximately 191 MB/s of engine reads and 447 MB of engine memory during sync. Keep the flat blob directory. This audit changes no runtime behavior and does not restart, pause or reconfigure the running app. Measurements use the existing Windows release engine (schema38), Windows process/performance counters, resident mapping metadata, metadata-only diagnostic logs, and read-only queries through the existing CLI. No message bodies, addresses, credentials or raw mailbox dumps are recorded here.

### Live observations

- The database file was 1,704,804,352 bytes; the WAL was approximately 4 MiB. The source configures a 1 GiB SQLite mapping ceiling and a 16 MiB private page-cache target. The mapping ceiling is not an up-front allocation.
- During an 18-second sample the engine working set was approximately 432 MiB and private committed memory approximately 77 MiB. Resident attribution: 339.76 MiB mapped `store.sqlite3`, 71.54 MiB private pages, 20.25 MiB executable/library pages, and less than 1 MiB other mappings. These categories describe resident pages; the 77 MiB commit number is a separate measure, not another amount to add.
- Simultaneous performance counters showed engine reads of 81.4 and 77.2 MiB/s while physical disk reads across the entire machine were only 0.72 and 0.16 MiB/s. Most observed process read traffic was served from the OS cache. It still incurs calls, copying and query CPU time. Process I/O counters also include non-file I/O, so they are not a disk-device throughput measurement.
- Engine read calls averaged approximately 4 KiB during these bursts, consistent with SQLite page reads. The latest sync log entries showed zero-item mail delta passes and waiting states, so the recurring work also persists between active message downloads.
- The GUI was separate: approximately 522 MiB working set, 648 MiB private committed memory and 400 MiB resident private pages. SQLite mapping does not explain that GUI memory. This audit did not attribute its heap allocations or establish a GUI leak.

SQLite shares mapped pages with the OS cache and falls back to ordinary reads beyond the mapped range; mapping can avoid duplicate buffers. See [SQLite's mapping explanation](https://www.sqlite.org/mmap.html). Windows private commit and working set measure different things; see [Microsoft's memory-counter definitions](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters_ex).

### Main finding: progress polling repeatedly scans mail

[`AccountProgress.qml`](../../gui/AccountProgress.qml) requests `assist_progress` for all accounts every three seconds. [`Runtime::progress`](../../crates/fm-engine/src/local_model.rs) calls [`Store::local_model_progress`](../../crates/fm-core/src/assist_backlog.rs) on the single writer. It recounts local messages and evaluates eligibility, saved results, failures and queue state across each account, including `length(m.body)>0`. There is no reuse when nothing relevant changed.

Two explicit read-only progress calls on the running profile returned unchanged totals (98,567 local messages, 91,025 eligible, 458 done, zero queued):

| Request | Wall time including CLI | Engine reads during request |
| --- | --- | --- |
| `assist_progress`, first | 769.46 ms | 163.85 MiB |
| `assist_progress`, second | 824.06 ms | 163.21 MiB |
| `status`, after first progress | 11.71 ms | 0 MiB |
| `accounts` | 27.07 ms | 2.18 MiB |
| `status`, final | 11.02 ms | 0 MiB |

An initial `status` took 670.70 ms while 158.04 MiB were read, consistent with waiting behind existing writer work. These are whole-process interval measurements: the GUI and background jobs remained active, so they are not isolated SQL instrumentation. The reproducible progress-call cost, its unconditionally scanning implementation, and the three-second GUI cadence identify a concrete recurring source of unnecessary work. The screenshot's exact 191 MB/s instant was not reproduced as an isolated request. Windows FileIO tracing was unavailable (`0x80070005`, insufficient tracing privileges), so there is no per-event file/stack attribution claim.

**Priority:** make progress a cheap read of maintained per-account totals, or cache totals using changes relevant to model progress. Calendar activity or an unrelated global revision must not force a rebuild. During bulk mail import, bounded incremental updates should keep progress current without rescanning the mailbox per poll. Keep running/paused/current-job state independently fresh. Verify unchanged polls perform no mailbox scan, changes update all affected totals, and foreground requests remain responsive during a rebuild.

### Other confirmed read and allocation inefficiencies

1. **Two full reads in the shared blob reader.** [`local_content::read_blob`](../../crates/fm-core/src/local_content.rs) first calls `verified_blob`, which reads and hashes the entire file, then opens it again, reads it into a vector and hashes those bytes again. This affects source readers, extraction, composition, indexing and conversion. Use one bounded verified read from one regular-file handle, preserving hash/size checks and path protections. The new attachment extraction path inherits this duplication.
2. **Three reads during staged-message recovery.** [`verify_staged_google_item`](../../crates/fm-core/src/google_live_mail/preparation.rs) verifies the original once, then `verify_parts` reads that original through the twice-reading helper. A ready staged message therefore incurs three full original-file reads before MIME parsing. Recovery should verify the source and its parts from the same verified bytes.
3. **Reopening newly imported MIME for link/people indexing.** [`link_index_prepare`](../../crates/fm-core/src/link_index.rs) reads and parses every original again after sync already parsed it. The current helper makes this two further file reads, including large attachment-bearing MIME. Capture links/people from the initial parse where possible; reserve the background pass for older records. Its current batch bound is a message count, not a byte budget.
4. **Avoidable encoding round trip for retained content.** `prepare_google_item_retained` reads verified raw MIME, base64-encodes it to construct a provider-shaped response, then the normal path decodes it again and re-verifies the existing staged blob. Share a preparation function accepting decoded bytes instead. This removes temporary full-message buffers and redundant work while retaining the same validation and publication rules. This fallback is conditional; ordinary retained-message label updates already have a lighter route.

The one-time attachment conversion also reads/parses older originals in the background. It uses a durable cursor and can add initial activity, but it does not explain the indefinitely repeating progress recount. Its current 50 ms pause limits messages per second, not bytes per second; a byte/time budget or yielding to foreground demand would be more predictable with large files.

### What should remain

Immutable blob publication, bounded workers, off-writer MIME preparation, verified exports, cached repeat attachment opens, and SQLite WAL durability are appropriate foundations. The measured write traffic was much smaller than read traffic; this audit found no reason to weaken flush/commit guarantees. Keep the mapping/cache policy initially: reducing the displayed working set would not remove the expensive scans and may increase reads. Compare private commit, resident mapped pages and request latency before and after eliminating redundant work. The remembered sub-10-MB reading is not a comparable baseline without the old build, workload and memory-column definition.

This is audit evidence and recommended scope, not an implemented optimization or leak diagnosis. Diagnostic helpers and generated counters remain under ignored `build/verification/attachment-storage/`; the earlier implementation tests do not validate these proposed fixes.

## 2026-09-27 progress/read improvements and GUI memory follow-up

### Implemented engine scope

D156 / Core79 / Gmail40 / Quality83 retain the existing progress SQL but reuse each account's small result until relevant data changes. Policy and history/catch-up bounds are compared on each call. The existing SQLite change tracker covers mail, provider draft/deleted state, model results/failures and account changes; its conservative transaction/missed-hook handling also covers this cache. Calendar publications and unrelated global revisions leave cached counts usable. Session/job state is still assembled outside these counts. The cache holds at most 256 account rows, never mail bodies.

The shared blob reader now opens and reads once, bounds the read by the declared size, then hashes the returned bytes. Ready staged recovery validates its attachment manifest against those same bytes: two ordinary reads become one, and three recovery reads become one. Hash, size, regular-file/path and 64 MiB checks remain. Flat layout, durability and cache limits do not change. Incremental progress during active sync, reuse of the first MIME parse for link indexing and removal of the retained-content base64 round trip remain separate work.

### Verification and limits

- On the demo3 fixture, instrumenting SQLite's VM gave **27,090 steps for initial counts, 68 for an unchanged poll, and 68 after unrelated calendar/global-revision writes**: 99.75% fewer steps for the cached path. These are executed SQL steps, not a disk-throughput benchmark or a 100k-message latency claim. Relevant mail changes during sync still cause full scans.
- Regression checks cover results, exhausted failures, policy/bounds, account removal, provider DRAFT/deletion changes, transaction commit, outer rollback, savepoint rollback and unqualified DELETE. Blob checks cover exact binary/empty bytes, bad/missing hashes, same-size corruption, size mismatches, limits and a directory replacing the file. Existing staged-source and attachment/draft/retention tests also run.
- Windows Rust workspace: **836 passed, 9 ignored**. The final four progress tests passed separately after adding provider-state coverage. Strict all-target Clippy, formatting, repository hygiene and whitespace checks passed. The first workspace attempt found a migration test hardcoded to schema38 after concurrent work introduced schema39; it now compares with `SCHEMA_VERSION`, and the complete suite was rerun successfully. This optimization itself adds no migration. Live0/demo3 and protocol1 remain unchanged.
- Release engine/CLI built successfully. With this engine, native empty-profile, mail smoke and attachment workflows passed with actual D3D11 rendering; the attachment walkthrough's independent CLI check after GUI/engine shutdown also passed. Tests used disposable synthetic profiles and never sent mail. The already running real-profile processes were not restarted or replaced.

### GUI audit

The founder reports high memory regardless of the selected view. Read-only process/window metadata showed the running GUI at roughly **414 MiB resident / 493 MiB private commit** in one sample, with variation across samples. Its client area was **1918 × 2061 physical pixels at 125% DPI**: one full RGBA surface alone is 15.08 MiB. A virtual-allocation census found fifteen approximately 15.8 MiB private allocations plus other heaps. Their similar sizes suggest image/rendering buffers, but this census does **not** identify allocating call stacks or prove their owners.

Disposable native workloads sampled process memory every 150 ms:

| Workload | Renderer | Peak resident MiB | Peak private commit MiB |
| --- | --- | ---: | ---: |
| Fresh empty live fixture | Software/offscreen | 98.74 | 77.97 |
| Demo mail smoke | Software/offscreen | 120.05 | 99.09 |
| Attachment walkthrough | Software/offscreen | 142.34 | 119.84 |
| Fresh empty live fixture | D3D11 | 142.73 | 152.49 |
| Demo mail, 1534 × 1649 logical window | D3D11 | 258.96 | 252.95 |
| Attachment walkthrough, default test window | D3D11 | 279.86 | 240.46 |

These are short workload peaks, not the five-minute combined idle budget or a matched before/after regression benchmark. The large mail case also captures a screenshot, which temporarily needs readback memory. Qt's log confirms D3D11 on the NVIDIA adapter. Earlier offscreen runs requesting D3D11 actually selected software; they are not hardware evidence. An intermediate Windows run launched hidden from the restricted environment failed to connect the smoke/attachment workflows; the interactive-desktop runs with the rebuilt engine passed. The current GUI executable includes concurrent work and is not a reconstruction of the previous optimization baseline.

The existing navigation benchmark was also attempted on a disposable demo profile. It stalled in its initial readiness/visible-content check before sample zero (exit 9), so it provides **no repeated-navigation or interaction-performance result**. While stationary, resident memory was 219.69/219.36/219.40/219.34 MiB at 10/20/30/40 seconds, with private commit around 199 MiB; that short interval did not show continuing idle growth. Failure diagnostics and all samples are retained locally. The benchmark failure remains a limitation of this audit; it is not treated as a passing regression gate or evidence that prolonged navigation cannot grow memory.

Source audit:

- Mail view, reader, source and body caches have **28 MiB combined serialized-JSON cost limits**, not measured heap limits. QVariant maps, UTF-16 strings, QML conversions and allocator overhead can exceed that accounting. Additional list caches are bounded by 2,000 and 3,000 rows. The active view and retained caches can hold overlapping representations, although Qt implicit sharing can avoid some copies.
- Mail and Calendar are instantiated together. Settings remains instantiated after its first open. Message body loaders stay built after scrolling out of view; the current conversation therefore retains visited content. Collapsing an HTML body unloads its HTML loader. These are retention choices, not demonstrated leaks.
- HTML has a 32 MiB process-wide tile target, but visible tiles may exceed it. Decoded images start being scaled above 24 MiB and reject over the larger 96 MiB bound. These are not total-GUI hard caps. An attachment thumbnail can decode to 760 × 475 × 4 bytes (about 1.38 MiB); multiple visible cards and transient video frames add more. Closing the attachment viewer releases its media loader; posters retain paths rather than all decoded frames in C++.
- Full-window blur effects were investigated with a separate Qt scene. **Never-shown hidden effects did not raise the approximately 101 MiB private-commit baseline.** Showing two effects then hiding them left about 208 MiB committed; clearing their sources left about 202 MiB, while resident memory stayed much smaller (about 75/59 MiB). This demonstrates graphics/driver retention in that synthetic workload, but does not explain the running GUI's entire resident increase or justify a speculative blur change.

The fresh hardware GUI already costs more than the software test baseline. The remaining long-session excess needs allocation-stack/retained-object attribution, using the same build, window size, DPI and navigation history. The existing 300 MB combined idle target is not established by these measurements. Prefer measured cache/object accounting and an actual retained-allocation profile before shrinking caches or eagerly unloading views; otherwise responsiveness can regress without fixing the large allocations. No GUI memory optimization or leak fix is claimed here. Generated measurements remain in ignored local verification storage.

## 2026-09-27 GUI memory changes with retained navigation caches

D157 / Mail UI108 / Quality84 implement two small changes supported by the audit. Fixtures remain live0/demo3, with generated native-test data and unchanged protocol/schema. Builds and synthetic profiles are isolated under ignored `build/verification/gui-memory/`; the user's running app was not restarted by this task.

### Source sharing

`prepareReader` previously converted a cached QVariant source to QJsonObject, removed raw content that was already absent, converted it back to QVariant, serialized it to calculate cost and replaced the cache entry. This recreated source data during normal revisits. Cache hits now pass the existing map directly. Misses still validate identity/version and populate the bounded cache; lifecycle and navigation checks still apply. `cacheConversation` also skips identical replies without skipping changed metadata at the same revision.

The native regression creates a 198,000-character generated source and prepares it twelve times. Exact map sharing and the **396,000-byte UTF-16 string's allocation identity** remain the same between cache and readers; no source request is made. Changed same-revision metadata replaces the conversation, and an older revision is refused. The sharing assertion rejects a separately recreated map. This proves sharing for the fixture; it is not a claim of 396,000 bytes saved for every real message. Cache capacities and instant-navigation coverage remain unchanged.

### Attachment images

The card grid wraps before the image area exceeds 400 × 250 logical pixels. Its old fixed 760 × 475 decode and the unbounded poster decode are replaced by a stable request sized for those maximum dimensions at the screen's density. The viewer's old per-pixel 2× request becomes a display-density request rounded up in 128-pixel steps, with asynchronous image retention during replacement. Original files and saved copies are untouched. At high DPI, the request is allowed to grow to preserve sharpness; savings are not uniform across densities.

[Qt's Image documentation](https://doc.qt.io/qt-6/qml-qtquick-image.html#sourceSize-prop) describes the memory effect of source size, decoder/format differences and reloads when it changes. The measurements below use the actual installed Qt 6.11.2 runtime rather than estimating only from requested dimensions. Qt's aspect-ratio rounding can produce a dimension slightly larger than requested. PNG and other formats may still decode a full source temporarily; reducing JPEG decoding is not proof of equal peak savings for every format.

**Thumbnail comparison:** sixteen distinct generated 2400 × 1500 JPEG files, displayed at 300 × 187.5 logical pixels, software/offscreen rendering at 125% scaling. Three separate-process runs per variant, alternating order. Process counters were sampled every 100 ms and averaged from 2–3.5 seconds; OS cache was not flushed.

| Measurement | Former size | New size |
| --- | ---: | ---: |
| Reported decoded dimensions | 760 × 475 | 501 × 313 |
| Mean settled private commit | 51.841 MiB | 39.978 MiB |
| Mean settled resident memory | 59.003 MiB | 46.684 MiB |
| Time until all sixteen images ready | 140 / 84 / 85 ms | 71 / 72 / 73 ms |

This workload saves **12.32 MiB resident / 11.86 MiB private commit**. Readiness includes QML construction and asynchronous image loading, not monitor presentation. Small samples establish a scoped improvement, not production percentiles.

**Viewer comparison:** one generated 6000 × 4000 JPEG, a 900 × 600 logical image area widened in twenty 4-pixel steps at 40 ms intervals, the same 125% software rendering, one process per variant. Counters sampled every 50 ms; settled interval 2.5–3.5 seconds.

| Measurement | Former sizing | New sizing |
| --- | ---: | ---: |
| Completed loads, including first open | 21 | 2 |
| First image ready | 37 ms | 30 ms |
| Settled private commit | 37.762 MiB | 32.762 MiB |
| Settled resident memory | 44.590 MiB | 39.660 MiB |
| Sampled peak private commit | 50.098 MiB | 37.277 MiB |

This isolates the image sizing/retention behavior, not the complete app or its graphics driver. Retaining an image during replacement can temporarily hold two decodes; this fixture still had a lower peak. Qt's image cache remains enabled.

### Native checks and boundaries

- Release GUI and mail-selection executable built in an isolated directory with the existing Qt/MSVC toolchain. Native mail-selection and the real-engine attachment walkthrough passed with software rendering and D3D11. The independent CLI check after GUI/engine shutdown passed in each walkthrough.
- The final targeted `gui-mail-selection-test --memory-check` and attachment walkthrough passed at 200% scaling. Initial full-suite high-DPI testing stopped in the separate avatar pixel check before attachments; no full-suite 200% pass is claimed. The tests cover exact sharing, metadata/revision guards, source request reuse, physical-pixel sizing, stable thumbnail decode requests during resizing, aspect/EXIF orientation, poster handoff, video stepping/playback and download behavior.
- Existing D3D11 folder-switch test: first visits were 230 ms to Sent and 37 ms to Inbox; subsequent observations were 70/38/88/45 ms. All warm observations met its 300 ms gate. These six observations are not the broader 100-sample/100 ms interaction acceptance test or a matched pre-change benchmark.
- Diff whitespace and repository hygiene checks passed. Rust production code was not changed by this GUI slice; the preceding engine suite is recorded above.

The GUI's full long-session memory excess is still not attributed. These changes remove verified copies and oversized/repeated image decodes while keeping useful caches. Broad cache shrinking, eager unloading of visited views, decoder replacement and SQLite mapping changes remain outside this patch. No claim is made that these scoped savings solve the earlier 400–500 MiB running-session reading.

## Selective UI refresh — 2026-09-27

D158 / Core80, Mail UI109, CLI58, Quality85; live0/demo3 and schema unchanged.

### Change and dependency boundaries

The one-second status poll previously used the global revision to request accounts, categories, the selected conversation and its draft, and capture history. Reader preparation also requested assistance and outgoing status. A view save, calendar update or unrelated metadata write could trigger that work.

Five in-memory counters now follow the tables read by these projections, using the existing SQLite hook. `meta` still invalidates accounts because it contains last-mail-sync timestamps. Drafts and capture operations invalidate draft/delivery projections; category changes invalidate labels and reader assistance; conversation changes include legacy capture subjects. Staging and view tables have no direct projection dependency. Unknown tables and missed hooks invalidate everything, as do reads inside or just after a transaction. Counters carry an opaque store-open epoch and need no database scan or migration.

The GUI preserves existing mail/calendar/preference handling, handles draft delivery without preparing received mail, and retains list/reader changes deferred by user activity or an in-flight read. Explicit refresh stays available and older engines use the previous broad refresh.

### Evidence

- Rust workspace: **839 passed, 9 ignored, 0 failed**. The new real-store tests cover stable repeated status, view-save, calendar publication, labels, draft-save, message-body changes, unknown tables, truncate writes missed by the row hook, rollback and reopen. `cargo fmt`, workspace/all-target Clippy with warnings denied, repository hygiene and diff whitespace checks passed.
- Native scripted IPC: an account-only generation change makes **one** projection request (`accounts`), with no category, reader, draft or capture query. A simulated global revision with unchanged area counters makes **zero** projection requests. The real view-save test advances the account counter only; it is not claimed to make zero requests.
- Native `--refresh-check` passed: scoped requests, dirty draft protection, outgoing-status requests, reader publication, no read-on-open mutation, deferred list/reader refresh, new epoch and older-engine fallback. The same run includes existing selection mutations, undo, remote mail and review-transition checks.
- Matching Release engine/CLI/GUI binaries built. The real CLI against a disposable demo profile reports all five counters and the epoch. Generated logs and test profiles remain under ignored `build/verification/gui-memory/`.
- The matching GUI/engine attachment and composition walkthrough passed with software rendering. It verified draft text and attachments, discarded-reply revival, navigation/cancellation isolation and a synthetic capture; an independent CLI after GUI/engine shutdown verified retained identities, bytes and text. No real mail was sent.
- Full native-suite attempts encountered concurrent GUI work: first a QML property named `top` overriding a final Item property, then the new paging-length assertion (19,494 to 18,200). The reserved property and a separate QML group-property semicolon compile error were corrected minimally. A separate controls run stopped at the reply-bottom layout assertion. These full-suite/layout attempts are **not** reported as passing; the targeted refresh checks pass independently.

### What to observe

Use the matching updated engine and GUI after closing the old session. During sync, open a long conversation, scroll it, type a reply and switch folders while progress changes. Account/progress-only, calendar-only and draft-only updates should no longer cause the full set of reader/labels/capture queries. In the developer console (`~`), `fm --json status` exposes `ui_generations`: stable `reader`/`drafts`/`captures` while `accounts` advances is a concrete example of work skipped by the GUI.

This removes known background work rather than proving a latency percentile. Cold opens are unchanged. Generations are area-wide, so mail changes in another account/thread, ambiguous writes and legacy capture subjects can still cause broader refreshes. No running real session was restarted for this verification.
