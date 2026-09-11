# Phase 0 durable fake-assistance integration

2026-09-11. This experiment consumes [Local assistance revision 3](../contracts/local-ai.md), shared fixture revision 1 and the existing `fm-assist` output validator. It implements [Core's durable adapter](../../crates/fm-core/src/assist_store.rs) and the [schema-4 tables](../../crates/fm-core/src/migration-004-assist.sql). No model, network transport, IPC command or GUI model control is introduced.

## What is connected

The Rust-internal API configures a diagnostic per-account policy, starts an explicitly scoped fake job, accepts a bounded fake result and reads its durable receipt. Missing policy defaults to classification and summaries off. Off starts no job or availability probe; an absent runner starts no job. Summaries and classification have independent switches.

Core supplies the context from verified retained original MIME and the shared safe text extractor. Context contains necessary sender/subject headers and published received/sent text from one conversation in one account. Unpublished drafts and attachment extraction are excluded. Source snapshots retain exact blob hashes, message revisions, manual override records, provider history/labels/deletion, policy revision and configured category mappings.

When a fake result arrives, Core reloads and checks those snapshots inside the same SQLite transaction that stores the result and any permitted category changes. `fm-assist` still validates the output shape, configured category IDs, abstention, summary setting and input coverage. Neither library gives the fake output a command/action interface.

Suggest mode stores suggestions only. Explicit auto-apply mode can add configured categories with allowed Google label mappings to included received messages. It records separate `assist_category_intents`; it never writes manual-intent records, read/star/inbox/sent flags, source bodies, drafts, Done/Delayed state or calendar data. Manual category corrections block later routine reclassification as well as results already in flight. Provider reconciliation consults manual category intent first, accepted assistance intent second and observed mapped labels last, preserving pending permitted operations.

The result, source coverage, fake-runner/schema/policy provenance and operation fingerprint commit together. Repeating the same request/output returns the original receipt without reapplying; changed scope or output conflicts. A restarted client can retrieve a completed request without starting another runner. Current-result queries mark receipts stale after source/member, provider, manual, policy or mapping changes. The adapter accounts for its own accepted category revision changes so a freshly stored summary is not immediately marked stale by its own transaction.

## Validation

All six Core integration tests passed on the development Mac, and strict Core Clippy passed. The complete current command is:

```sh
cargo test -p fm-core assist_store::tests --locked
cargo clippy -p fm-core --all-targets --locked -- -D warnings
```

The checks cover:

- Off/absent runners, exact retained context and exclusion of an unpublished draft marker.
- Suggestion-only behavior, permitted auto-apply, summary persistence and immutable replay after closing/reopening the store; protected mail/draft fields remain unchanged.
- A manual category correction during a job, and a fresh job after that correction; neither puts the removed category back.
- Independent policy, provider, source-hash, mapping and new-member changes, including changes that deliberately leave the ordinary message revision unchanged; every stale result is rejected.
- Forbidden action/draft fields and system categories, plus an injected SQLite result-commit failure that rolls back category changes, pending intents and the result together.
- Independent summary switches and a conversation above the Core source-count bound, without additional runner work.

## Bounds and remaining gates

This Core experiment admits at most 200 conversation members and 16 MiB of retained raw sources per job. The shared boundary limits included context to 128 messages/128 KiB, active jobs to 16 and output to 16 KiB, with at most 16 returned categories and an 8 KiB summary. Coverage records the included members and partial state; auto-apply affects only eligible included received members. Oversized or unavailable retained sources fail explicitly rather than silently substituting other content.

These are real local SQLite transactions and verified source reads with a fake runner. They do not establish model classification quality, cloud-free execution through a real local service, inference performance, remote provider delivery, physical power-loss recovery or real runner cancellation. Owner/account cancellation remains the shared boundary's explicit operation; future GUI/CLI session integration must invoke it. No policy update authorizes a historical mailbox sweep, downloads a model or enables a production settings control.
