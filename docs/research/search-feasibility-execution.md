# Search and coverage feasibility execution

P0-03, executed 2026-09-11 against small fixture revision **1** plus the opt-in 100k generator. Consumes Core draft revision 5 and Search draft revision 0. The actual shared `Store.handle` path now performs one FTS matching pass per page and reads small member metadata plus one selected body per returned conversation. It preserves account/category/folder scope, original matching message IDs and conversation grouping.

The baseline repeated the FTS matching query for each returned conversation and hydrated every member's body and attachments. Removing those repeated scans materially reduced the observed common-query median. Adding truthful index coverage then cost about 31 ms per 50k-message account. That remaining counting cost is visible in the final measurements; it is not hidden behind inventory counts.

## Commands and evidence

```sh
cargo test -p fm-core --test mutations
cargo test -p fm-core --test coverage --test search
cargo test -p fm-core --release search_100k -- --ignored --nocapture
cargo clippy -p fm-core --all-targets -- -D warnings
```

The mutation suite passed 7 integration cases; its additional unit test verifies a new reply does not inherit an older snapshot action. Coverage passed 2 fault cases and search passed 5 correctness cases. These exercise the real SQLite store, scoped revisions, exact operation retries after restart, independent flags, durable drafts/views, search grouping/pagination, and exclusion of unpublished draft text from source search. Tests and generator are in [fm-core](../../crates/fm-core/).

Retained reports: [baseline](assets/phase-0/search-100k-baseline.json), [optimized query before corrected coverage](assets/phase-0/search-100k-optimized.json), and [final query with actual coverage counting](assets/phase-0/search-100k-final.json). The final report includes an `EXPLAIN QUERY PLAN` of the same SQL helper used by production search; SQLite uses its FTS index, rowid lookups, and temporary grouping/order trees.

## Observed 100k result

Host: macOS **15.7.7 (24G720)**, Apple Silicon `aarch64`; Rust/Cargo **1.88.0** (Homebrew), bundled SQLite **3.46.0**, release build. CPU model and RAM were not obtained by this experiment; its `sysctl` read was sandbox-denied. Other development work on this host was not suspended, so before/after timing is an engineering observation, not an isolated hardware comparison.

The generator adds **100,000 messages and conversations** across two accounts to the existing **286-message** fixture. Database size after checkpoint was **126,177,280 bytes**. Final seed time was **3.701 s**, versus 1.583 s in the baseline run; seed timing is not provider import speed. Generated rows use repeated synthetic text and a shared retained source to measure index density, not a representative large mailbox or retained binary corpus.

Each query has one initial observation after reopening SQLite, then **40 warm observations**, returning up to **20 conversations** with matching snippets. OS filesystem caches were not cleared. There was no benchmark insertion, deletion or inference during these samples. All required query/result/account expectations passed before timing was accepted.

Times below are milliseconds, rounded; JSON reports retain the recorded values.

| Query | Baseline p50 | Baseline p95 | Baseline p99 | Final p50 | Final p95 | Final p99 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Sender punctuation | 311.02 | 324.78 | 324.82 | 85.23 | 88.20 | 93.67 |
| Sender, other account, zero results | 32.23 | 37.77 | 42.02 | 60.19 | 61.69 | 61.74 |
| Rare subject | 3.18 | 3.73 | 3.98 | 33.93 | 34.93 | 35.20 |
| Rare body | 5.51 | 6.98 | 7.26 | 34.66 | 35.49 | 35.79 |
| Common body | 273.72 | 2463.71 | 2479.84 | 79.06 | 80.90 | 81.66 |
| Unicode/diacritic body | 270.61 | 1481.19 | 1484.39 | 80.70 | 85.38 | 88.26 |

The very high baseline tails may include competing host work. Median changes and the removed repeated-query mechanism support the optimization; the tail difference alone does not establish its isolated effect.

## Coverage semantics and limits

`indexed_messages` counts actual FTS rows joined to the selected account's messages. A missing FTS row reduces that count even while the message remains in inventory. `retained_source_records` and `recorded_available_attachments` are database metadata. Aggregate `retained_sources`, `complete_attachments` and `verified_attachments` are **null**, verification is `not_run`, and `offline_complete` is false. List/search never scan or hash the whole blob corpus. An opened conversation checks source-file presence; attachment use verifies its size and checksum. Presence alone is not an integrity claim.

The fault tests remove one FTS row and its source file in a closed disposable profile, then confirm incomplete index coverage, missing source availability and unaffected other-account counts. A corrupt attachment is refused at use while aggregate verification remains explicitly unknown.

This experiment does **not** satisfy the full [quality budget](../contracts/quality.md): that calls for 50 returned conversations, more observations where practical, representative varied content and concurrent work. It also does not prove user-visible paint latency, an OS-cold query, ingestion responsiveness, 1m/3m capacity, 25 GB/100 GB retention, hardware power-loss durability, or another OS. A transactionally maintained, rebuildable index-coverage counter is a future optimization; returning inventory size as index coverage is not acceptable.

## Phase 1 Windows search and concurrent insertion — P1-07

Executed 2026-09-12 UTC against the working-tree Core 10 / Search 3 / CLI 5 implementation, protocol 1, schema 5 and fresh fixture 2. The original Phase 0 observations above are unchanged. Retained reports: [initial Phase 1 baseline](assets/phase-1/search-100k-baseline.json) and [final optimized result](assets/phase-1/search-100k-final.json). The benchmark exits successfully when correctness expectations pass; that exit status does **not** assert its latency budgets passed.

Host/toolchain: Windows 11 x86_64, Ryzen 9800X3D / 128 GB / RTX 5090 as recorded by the [Windows integration evidence](phase-1-execution.md), Rust 1.88.0, bundled SQLite 3.46.0, release optimization. Other development activity was not suspended. These are engineering observations on one host, not an isolated hardware comparison. The Core measurement does not use the GPU or measure painted frames.

Each run seeds 100,000 generated message/conversation rows across two accounts plus the 429-message, three-account small fixture. Synthetic metadata repeats common text and uses one retained generated source, so this remains an index-density workload. The reopened writer performs one initial query followed by **100 warm observations per class**. The six historical classes return up to 20 conversations; the two new app-consumed grouped/filter classes return the first 50 with exact filtered totals, account counts, coverage, member IDs and snippets. OS filesystem caches were not cleared.

| Query | Baseline p50 ms | Baseline p95 ms | Final p50 ms | Final p95 ms | Final p99 ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| sender punctuation | 281.95 | 332.43 | 90.88 | 99.88 | 108.15 |
| sender account isolation | 223.98 | 258.88 | 51.76 | 58.47 | 90.17 |
| rare subject | 115.46 | 126.51 | 0.95 | 1.21 | 1.47 |
| rare body | 117.23 | 148.60 | 1.83 | 2.46 | 2.94 |
| common body | 254.53 | 299.72 | 76.69 | 87.30 | 154.13 |
| unicode body | 261.90 | 296.93 | 77.80 | 86.19 | 96.44 |
| phase1 grouped first50 | 236.91 | 253.12 | 46.11 | 54.69 | 72.01 |
| phase1 combined filters first50 | 517.85 | 647.85 | 105.68 | 133.92 | 139.83 |

Grouped first-page input is `{folder:"all",grouped:true,limit:50}`. The combined case adds body prefix `deployment`, a sender substring, review status, no attachment and the half-open UTC date interval 2026-09-01 through 2026-09-12. Every generated row in that account satisfies these source predicates, deliberately exercising a saturated common query with about 50,000 matching conversations. Date/status/attachment filters are evaluated rather than omitted because the generated values happen to match.

### Changes supported by the measurements

The final path directly joins FTS matches to their source rows once, avoiding a temporary global rowid list. It creates only requested SQL predicates, with all values still bound. Covering message projection and attachment indexes in migration 5 support metadata and attachment checks. Core groups exact source identities, ranks only the requested prefix and hydrates bodies/member IDs for returned conversations. Temporary grouping uses borrowed conversation identities and SQLite rowids; it avoids allocating every full message ID and an unnecessary date string for grouped sorting. Legacy default ordering remains separate from opt-in review/held/done ordering.

Account conversation state, actual index coverage and counts are derived from the same exclusive writer connection and cached against SQLite `total_changes`. Every write invalidates the cache, including normalized insertion, flags and FTS removal. This is an in-memory metadata projection; complete query responses are not cached. Fault tests exercise invalidation and incomplete FTS counts. The recorded EXPLAIN plan covers the production source-match SQL helper; Rust grouping, ranking and subsequent bounded row hydration are included in end-to-end timings but are not represented by SQLite EXPLAIN.

The baseline checkpointed database was 126,222,336 bytes; the final database was 145,973,248 bytes, including the new covering indexes. Final seed time was 3.616 seconds (baseline 3.095). Seeding is not provider-import or durable attachment-throughput evidence.

### Concurrent normalized insertion

Independent insertion and search producers feed one serialized Store writer through a bounded queue of two requests. The experiment performs 100 batches of 25 messages, adding 2,500 rows, alongside 100 combined-filter first-page queries. It includes shared source/FTS transactions and real storage writes, but no provider transport or model inference. Its queue-to-response measurement starts before enqueueing and includes time waiting behind other work.

| Concurrent measurement | Baseline p95 ms | Final p50 ms | Final p95 ms | Final p99 ms |
| --- | ---: | ---: | ---: | ---: |
| Core combined query | 601.00 | 106.35 | 234.45 | 300.87 |
| Queue to query response | 601.53 | 106.38 | 259.69 | 308.40 |
| 25-message insertion transaction | 10.70 | 2.36 | 10.82 | 62.75 |

### Budget outcome and limits

**The 100 ms Core target is not fully met.** The unfiltered grouped first50 case passes; the combined-filter case remains 133.92 ms p95, and concurrent combined queries remain 234.45 ms p95. The six historical 20-result classes are all at or below 99.89 ms p95, but they are not substitutes for a 50-result gate. The optimization substantially reduces the failed baseline without justifying a claim that all search performance is complete. Saturated exact-match enumeration and writer-wide metadata invalidation remain identifiable costs; they are current implementation limits, not proof that further optimization is impossible.

Future performance work should measure incremental metadata invalidation and account-aware FTS retrieval against the same exact-count and replay rules. Do not replace actual index coverage with inventory counts or omit matching members to meet a timer. GUI final-input-to-paint measurement is a separate native test recorded by the integrated phase evidence. This corpus does not prove 1m/3m scale, representative varied languages/thread sizes, 25 GB/100 GB retention, OS-cold behavior, physical input, attachment throughput, real provider ingestion or power-loss durability.

Run the experiment from the recorded VS 2022 environment with the project-local Cargo/Rustup homes:

```powershell
$env:FM_BENCHMARK_ITERATIONS="100"
$env:FM_BENCHMARK_REPORT=".tools/phase1-search-100k-final.json"
$env:FM_BENCHMARK_KEEP_PROFILE="1"
.tools/cargo/bin/cargo.exe test -p fm-core --release search_100k --locked -- --ignored --nocapture
```

`FM_BENCHMARK_KEEP_PROFILE=1` retains only the newly generated disposable directory and reports its absolute path for the GUI benchmark. It never accepts or overwrites an existing profile. Generated databases remain outside source control.
