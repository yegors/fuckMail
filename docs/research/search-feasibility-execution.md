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
