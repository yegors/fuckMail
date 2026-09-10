# Performance and search quality

Status: proposed budgets, not measured results. Required behavior is fast sender/subject/body search, conversation-only mail, full offline messages/attachments and reliable calendar views on Windows/macOS. The founder specified high-end machines. Exact models and mailbox message counts remain unmeasured.

## Representative workload

Use synthetic mail and non-sensitive fixtures for repeatable development. Add founder-owned data only through a deliberately scoped local evaluation. Do not put private messages, attachments, addresses or credentials into the repository or benchmark reports.

The supplied `du -h` snapshot reports about 25 GB for the existing eM Client local store, with major directory totals around 2.9 GB, 7.6 GB and 15 GB. This is user-supplied disk-usage evidence, not a verified account inventory, full server size, or message count. No eM Client database was inspected.

Proposed scale tiers: 100,000 messages for rapid iteration, 1 million across several accounts as the primary large-corpus test, and 3 million as a stress tier. Include a roughly 25 GB offline-content case anchored to the current store and a 100 GB growth case. These are synthetic capacity scenarios, not claims about the founder's message count. Include both many short messages and attachment-heavy mailboxes.

Record message count, unique text bytes, binary bytes, average and tail body size, number of accounts/folders/labels, thread lengths, languages, calendar series, local retention coverage and index size. Repeated identical text alone is not a representative search corpus.

## Candidate budgets

Proposed reference: a high-end computer with 32 GB RAM and an NVMe SSD; record exact hardware before benchmarking. Report the required macOS and Windows results separately; Linux does not block MVP.

| Interaction | Proposed target | Measurement boundary |
| --- | --- | --- |
| Warm local search | p95 <= 100 ms | Core query to first 50 conversation results, including grouping, matching-message snippets and filters |
| Search as seen by user | p95 <= 200 ms | Final input event to painted first results, including debounce and any cross-thread/process communication |
| Cold local search | p95 <= 500 ms | First query after a documented cold-start/cache protocol |
| Cached mailbox switch/conversation open | p95 <= 100 ms | User action to usable visible content for normal-sized conversations |
| Cached calendar view switch | p95 <= 100 ms | User action to visible events after bounded recurrence expansion |
| App launch into cached inbox | p95 <= 2 s | Process launch to usable cached list; excludes first import and auth |
| Scrolling | 60 Hz target with no sustained visible stalls | Frame timing while backfill/indexing runs; inspect tail frames |
| Idle resource use | <= 300 MB combined resident memory, < 1% average CPU | All app/rendering processes over a quiet 5-minute interval while open |

Long conversations and large/complex messages get bounded asynchronous rendering and progress rather than blocking the whole UI. Fetch and render conversation members incrementally without exposing a separate message mode. The memory budget is a starting hypothesis and must include any HTML-renderer/runtime subprocesses, even with a native shell. Measure native list/calendar interaction separately from HTML reader cost. After an explicit app close, verify that no app-owned process, sync request or reminder remains active. Record the peak during import separately; set its release budget after phase 0 measurements.

Do not impose a fictional whole-mailbox import time independent of provider quotas and bandwidth. Measure time to useful recent conversations, messages/text and attachment bytes per minute, quota consumption, disk amplification, projected completion and resume behavior. Include disk space for full attachment retention and temporary download/maintenance overhead. Define the recent-mail target after a real authorization and provider experiment.

## Evaluation method

Run representative queries with and without simultaneous synchronization, insertion, deletion and index maintenance. Include common subject/body terms with huge match sets, rare terms, sender names and addresses, partial sender input, current-account filters, long conversations and selected languages. Measure grouping and matching-message hydration, not only raw full-text query time. Report p50/p95/p99, repetitions, fixture version, build mode, hardware, cache state, and storage growth. A process restart alone does not prove the OS page cache was cold.

Use at least 100 observations per interaction class where practical. Save reproducible commands and reports when the harness exists. If a target fails, investigate query plans, result hydration, locks, indexing batches and UI work before replacing a library.

## Search usefulness

Collect 10–20 retrieval tasks covering the founder's confirmed needs: sender display name, sender email address, subject keyword and body keyword. Include a few exact identifiers and multi-message conversations. Attachment-content search is not part of the required MVP benchmark.

For known-item tasks, propose the desired conversation appearing in the first 10 relevance results in at least 90% of cases, with the actual matching message revealed on opening. Exact identifiers and explicitly matching filters should never be missed once their fields are indexed. Evaluate quoted-reply duplication and required languages separately; typo handling is a future scope decision. Show incomplete coverage whenever the underlying data has not been acquired or indexed.

## Reliability under load

Acceptance scenarios include interruption during initial import, expired provider cursors, remote deletions during backfill, concurrent edits from another device, low disk space, index rebuild during reading, lost submission acknowledgements, revoked calendar permissions, and daylight-saving transitions. Verify that unsent drafts survive restart and that stale index entries never expose removed-account data.
