# Local assistance execution

## D053 summary UI and lifecycle repair (2026-09-14)

The founder’s review found D052’s summary UI unlike the latest prototype. D053 uses the supplied Update 2 Summary card: a 6-pixel radius, quiet 30%-opacity outline and 2-pixel left rule in the theme’s assistance color, compact uppercase heading, right-aligned model attribution, and a readable body. Thread summaries sit below reader metadata; individual summaries sit beside their message. Re-summarize, source access and timestamps are secondary actions. Output stays plain text; long summaries and source lists use a bounded scrollable popover. The shipped font and the prototype’s Graphite/Paper/Harbour assistance colors are used.

The repair also fixes:

- Summary-only accounts can summarize individual received **and sent** messages. Classification/correction remains received-only. A sent-only thread does not offer classification.
- Exact source links call the reader’s existing account/conversation-scoped expansion and stable scroll routine. The earlier hook used an unscoped expansion key and the wrong coordinate origin.
- Duplicate activation while a start request is pending is blocked. A late start reply is canceled after cancellation or navigation; obsolete polls cannot clear replacement jobs. Disconnect/poll failure releases the busy state, and a separate correction failure cannot orphan an active inference.
- Sources/details stay expanded across unchanged-result refresh. Summary text no longer has a circular width dependency. Long output stays inside its card.
- Settings offers explicit reload after a save conflict, preserves unsaved edits during discovery, rejects obsolete catalog/sample replies, and cancels late/hidden sample jobs. Preview results are readable Summary cards or classification text, without protocol JSON. Programmatic prompt reload no longer re-enters the edit binding.
- Each task’s cache validity follows its own model/prompt/enablement and relevant source state. Classification changes no longer stale source-text summaries; summary-only edits preserve sender classifications. Earlier snapshot versions stay readable and stale until explicitly refreshed. Schema 21 and command/fixture versions are unchanged.

### Verification

The expanded native harness uses production SummaryCard, MessageAssistance, controller and Settings QML. It passes saved-model/prompt and conflict/reload checks, duplicate/pending sample cancellation, summary-only sent/received controls, exact message/source targets, delayed start and poll replies, cancel/switch/disconnect/poll-failure recovery, external-Settings watcher restart without resetting unchanged cursors, and all nine theme/text-size combinations at a 480-pixel window. An 8,000-character summary stays within the 210-pixel body scroll bound. Actual rendered frames were inspected in all three themes; the harness is a synthetic component scene, not a real mailbox screenshot. No QML runtime warnings remain; the existing Sans Serif font-alias startup diagnostic is retained.

Focused Core storage testing passes with new cross-task validity assertions. Strict Clippy passes for Core/engine/CLI across all targets on the combined D053+D054 source. The actual engine and typed CLI pass the synthetic HTTP regression with **10 generation requests**, including summary-only operation, independent task settings, explicit label acceptance, source coverage, protected flags, stale-result rejection, cancellation and independent session shutdown. An additional synthetic request to installed Ollama 0.34.0 / `gemma3n:e4b` returns the correct design approval and proposed Friday-at-10 meeting, with the supplied source ID. No real mail was used for model inference.

Ignored evidence is under `.tools/local-logs/llm-ui-repair-*`; source/build/package work is isolated in `.tools/llm-ui-repair-source`. Preserve native attempts 01–04: a harness syntax error, the width-binding/source disclosure failure, a test accidentally selecting the hidden Settings preview instead of the reader, and the actual prompt-reload binding loop. Native05 passes those repairs. Native06 exposed a test ordering mistake around the initial watcher request; Native07 passes the corrected wait and external-Settings watcher restart. Native08 exposed a source delegate being destroyed while its click handler still referred to its QML context; capturing the owner/source before dispatch fixes it. Native09 is the final passing native rerun, including the bounded source popover. These findings improved both implementation and assertions; none of the failed checks was relabeled as passing.

Configured final **Package03 passes** SDK-free relocated GUI smoke/self-test, empty live profiles, configured Google registration discovery, CLI query, native mail/composition/Calendar workflows, exact restart persistence and final engine shutdown. Packages01/02 also passed on their earlier source scopes and remain preserved; Package03 adds the final watcher/source-popover repairs. The combined delivery is `dist/ollama-ui-darwin-arm64/fuckmail-gui.app` and `dist/fuckmail-ollama-ui-darwin-arm64.tar.gz` (**58,655,811 bytes**, SHA-256 `9f9268838eb595bffd1b5d84e6780898c42ec0dda5cb1f8b8eac71d343829880`). All **223 regular files and 136 symlinks** match both the final stage/copy and tar archive. The existing unsigned macdeployqt diagnostics for JSON/license files remain; runtime verification does not imply signing/notarization.

Final GUI/engine/CLI SHA-256:

- `2954356ac0cdb2c2ea9af3272c8e62ab613b2cd6ed2f7b5c305f563213610dcc`
- `b9491bdf6d7795c472151789587f685171563331c9f6ad5780a32836a284306a`
- `1a43fe3c03c6a328f795524c0b797a04215e00af233784ae64c069179294fe44`

The source freeze contains **487 authored inputs**, including the parallel D054 Keychain/Sync repair. Separate frozen D052 and D054 packages retain their own evidence. Full Phase 2, real-mailbox visual readback, other-host acceptance and general model-quality qualification remain open; readiness stays 0/9. Summaries remain on demand.

## D052 / P2-LLM Mac implementation (2026-09-14)

Ollama discovery, independent summary/classification models, editable prompts, existing account label descriptions, per-message Human/Automated/Unknown annotations, explicit suggestion acceptance and source-linked reader summaries are implemented. GUI and CLI consume Local assistance 6/Core 29/CLI 23/Mail UI 30/Platform 17/Quality 30 with schema 21; live/synthetic fixtures 0/3 and IPC protocol 1 stay unchanged. Assistance defaults off. New-mail classification is prospective and owned by a running GUI or explicit CLI watch. Historical conversation/message work requires an explicit action.

### Scoped checks

- Shared assistance tests: **8 passed**. Focused Core settings/restart/result/manual-precedence/staleness test passes, including preservation across read-on-open. Schema-related Core groups: **17 passed**. Strict Clippy passes for assistance/Core/engine/CLI across all targets. These are scoped checks, not a new full-workspace result.
- Synthetic HTTP server + actual engine and typed CLI pass: disabled/save starts no inference, catalog excludes cloud entries, selected remote model is refused before generation, shared settings and session isolation, summary coverage, classification and explicit label acceptance without changing read/star state or unrelated members, invalid/oversized output rejection, manual correction during generation, cancellation, independent surviving owner and final shutdown. Nine generated responses.
- Native production QML harness passes model selection, editable prompts, refresh preserving edits, save conflict preserving edits, saved model/prompt, three themes and text sizes, and reader job/result readback. Opening Settings/reader and saving do not launch historical inference. No QML runtime warnings; the existing missing Sans Serif alias diagnostic remains.
- Actual old schema-20 engine/CLI → new schema-21 engine/CLI, old-binary refusal and new-binary reopen pass in **6.804 seconds**, preserving the synthetic mail/composition/Calendar workflow checks. The schema-20 binaries are the preserved UI/mail package; the schema-21 binaries are the isolated debug build. This test predates only the final default prompt refinement; storage code is unchanged.

### Installed Ollama sample quality

This Mac has Ollama **0.34.0**, served by the local Ollama process on 127.0.0.1:11434. The chosen installed `gemma3n:e4b` has local GGUF weights, completion capability and digest `15cb39fd9394fd2549f6df9081cfc84dd134ecf2c9c5be911e5629920489ac32`; metadata has no remote routing fields. The adapter refuses cloud entries and checks that digest before and after generation. No service configuration, model download or real mailbox inference was performed. Other listed models are not thereby qualified.

The authored seven-case set covers personal correspondence, a receipt, human mailing-list post, important automated alert, ambiguous missing content, Spanish correspondence and embedded instructions. Expectations existed before evaluation. Initial prompt: **7/7 sender kinds, 3/7 exact label sets**. The refined prompt explicitly separates the label catalog from message evidence and requires each definition to match: **7/7 sender kinds, 5/7 exact label sets**. It still added Receipts to the mailing-list and Spanish discussions. Missing-content abstention and the embedded-instruction case passed. These seven examples are a small development sample, not general accuracy estimates or an auto-apply qualification.

The synthetic summary correctly reports the design approval and proposed Friday-at-10 discussion, with its supplied source ID; no factual error was observed in that one sample. Final classification requests took **1.708–2.475 seconds** end to end on the already-running model. This is not a cold-start, memory, throughput or UI benchmark. On explicit CLI cancellation, the Ollama server reported processing stopped and all slots idle after **0.412 seconds**, using only a bounded synthetic generation and diagnostic log offset. No shared runner shutdown was used.

Reproducible authored inputs:

| Input | SHA-256 |
| --- | --- |
| Classification prompt | `dd0ed15ee1ebbdce83df308b4716b7ee23295cea86f307db8d86c125e948f204` |
| Summary prompt | `ec5b788571621386ef4d4fd680f4d0f74c6d23c55be0dd296e4d719600dbb10b` |
| Shared schema/validator source | `5c059173e4c5c5f30577098c8ff599f1a0e23842d2298fdbe52c335fdede8e39` |
| Labeled Ollama fixture | `a54c4134aa555ff002b119fd4418505aab6d4bd5b32ff774ebf55c99019e82ea` |

### Configured Mac package

Release package02 passes SDK-free relocated GUI smoke/self-test, paired fresh empty live profiles, configured Google registration discovery, fresh-terminal CLI query, GUI restarts with preserved mail/composition/Calendar/settings state and final engine shutdown. A further synthetic Ollama verifier passes against the packaged engine/CLI, including explicit label acceptance and cancellation. The current GUI executable includes both D049/D050 UI/mail work and D052 Ollama controls; the separate schema-20 package remains unchanged. No real profile was opened.

The delivery is `dist/ollama-darwin-arm64/fuckmail-gui.app` plus `dist/fuckmail-ollama-darwin-arm64.tar.gz`: **58,572,806 bytes**, SHA-256 `a25678fb648940c7e5ed1bc1d619e0826e9a392043ce4bf5052402b6c375e025`. All **223 regular files** match the package-stage hashes in both the copied folder and tar archive; copied symlinks match their targets. The unsigned deployment retains the existing macdeployqt diagnostics for non-object JSON and license-file signing; direct relocated runtime checks pass, without a signing/notarization claim.

Packaged executable SHA-256 values:

- GUI: `203c4ea238ef08819e3ece35d4f6a58eade27a2e3ade21a7e97c283c8ac328b3`
- Engine: `24b423d38c24683fb3ba06f9252da929f5ee5014e168d5aaaa9a7c3bccb8ddda`
- CLI: `e099aef5f4a9f901fd6198c4dff9975a12d3aa8f0ee1d6332050d4502284d3f3`

The isolated build copied **485 authored inputs** from the uncommitted shared branch; exact hashes live in the ignored release-source manifest. All **367 scoped runtime inputs** still match that freeze. Later changes strengthen the standalone native/HTTP verifiers and update authored reports; they do not change packaged runtime inputs. The initial and final prompt trial outputs, native reader check, packaged integration and actual migration report retain their respective executable/source scopes.

### Evidence and limits

Generated logs and manifests (77 indexed files at final readback) remain ignored under `.tools/local-logs/llm-*`, builds in `.tools/llm-target`, `.tools/llm-gui` and `.tools/llm-release-source`. Preserve the earlier sandbox connection failures, CLI envelope-polling failure, missing synthetic-profile flag and Python 3.9 verifier incompatibility; final retries fix those specific causes. The first package check was sandbox-blocked at local engine connection; its build/deployment output is retained. Keep model-quality misses and both prompt trials.

Whole-product readiness remains **0/9**. Current Windows/Linux qualification, a founder on-screen Ollama walkthrough, larger labeled quality/summary sets, generated Google delivery of accepted suggestions, and resource/cold-start measurements remain separate gates. No auto-apply, historical sweep, real account inference, external mail, model installation or app-owned service is introduced.

## Phase 0 durable fake-assistance integration

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
