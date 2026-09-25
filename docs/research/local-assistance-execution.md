# Local assistance execution

## D114 — local model pipeline (2026-09-24)

Mac, working tree after commit `56df239`, with another session's uncommitted signature/settings work in the same tree. Synthetic profiles and a fake model server except where noted.

- **Before:** the full Rust workspace passed 757 tests, zero failed.
- **After, full workspace:** 750 passed, 10 failed, 6 ignored. All 10 failures are draft/signature tests (`capture`, `composition`, `draft_discard`, `google_send`, `local_content`, `phase1`). They fail because the other session's uncommitted change moves the signature into the draft body; none touch the local model. After the last queue changes, rerun per crate: fm-assist 13, fm-mail 21 + 13, fm-engine 176 + 11 + 2, fm-cli 9 + 19, and the affected fm-core modules (queue 5, local model 8, schema 8, accounts 28, live profile 10, labels 2 + 1, sender kinds 5) all pass. Clippy on the five crates: no warnings.
- **End to end** (`scripts/verify_ollama.py`, built engine + CLI + fake server), all pass: system/user settings with no inference on save; one request per email with review state unchanged; the queue processes every eligible email once, newest first; CLI pause/resume; Catch up waits behind the cutoff, runs newest first, and an email stored mid-Catch-up is analysed next; with the model down the queue holds, marks nothing failed and resumes on a settings save; a failing email gets two attempts and then waits; Test with unsaved settings and timings, with a cloud model refused; the OpenAI-compatible provider; the engine exits when its clients close.
- **Native GUI:** `gui-local-model`, `gui-mail-selection`, `gui-calendar-search`, `gui-debug-console` and `gui-smoke` pass. `gui-integration` stops at the draft-discard stage and `gui-mail-navigation-cache` at the signature editor; both come from the other session's signature work.
- **Real models** (installed Ollama, synthetic sample emails through the engine, 8 cases each): `gemma4:e4b` about 1 s per email once loaded, `qwen3.8-obliterated:27b` about 2–2.5 s. All 16 completed, labels 16/16 as expected. Sender kind was 14/16; the two misses were the content-less sample, which expected the removed `unknown` and is now expected to be `automated`. Replies are about 50 output tokens, against 90–130 before. Output stays under ignored `build/verification/`.
- **Real profile:** a release GUI built from this tree (running since 18:14) had already opened the founder's profile. Read-only inspection: schema36, the old settings archived, the prompt removed, the model choice kept, 216 results kept, the watcher keys gone, no failures.

Limits: no real-mailbox queue run or large-mailbox timing yet, and no Windows/Linux run.

## D097 — first label import and synced-mail analysis (2026-09-23)

Founder report: a new account with a local model neither summarized nor labeled its mail until labels were synced by hand. Root causes: the watcher started at the newest stored message and only accepted mail dated after its own start, so first-sync history was never queued, and Gmail labels were imported only by the manual Sync with Google job, leaving Auto-label with no labels to choose. The per-account default was already on.

Changes: a one-time label import after a new account's first catalog; a persisted watcher cursor that queues all synced mail missing a result; one-shot rescans after policy re-enable, label import and message deletion; history runs never Auto-read. Generated and disposable test profiles only. No real profile, account or model was used.

Checks on Windows:

- The full offline Rust workspace passes **700 tests, 6 ignored, zero failed**. Strict Clippy (`-D warnings`) and formatting pass.
- New Core tests: synced history is queued once across several watcher pages, with no Auto-read, and the cursor survives reopening; assistance turned off and on queues the account again; only mail dated after the watcher starts is an arrival; mail stored under a reused row ID after deletion is still queued; only a new account (not a reconnect) waits for the import.
- New engine tests: the first label catalog refresh imports Gmail user labels once, creates no Google labels and publishes a refresh; later Gmail labels wait for explicit sync; a read-only account refreshes labels without importing them.

Not checked: native GUI tests, packages, real Gmail accounts and real model throughput on large mailboxes. A large first sync is analysed one message at a time, so completion depends on model speed.

## D080 — installed-model summary reliability (2026-09-22)

The founder requested reliability and speed testing of the installed Gemma4, Qwen and Muse Glimmer models. This packet changes fm-assist request rendering/validation, the Core request budget and the Ollama engine adapter. It uses Local assistance15/Quality57, Core44/Gmail24/CLI39/Mail UI56/schema29; live/synthetic fixtures0/3 and protocol1 are unchanged. Parallel D081 UI/query work is excluded from the frozen package source.

### Failures and repairs

- Gemma's baseline consumed all 4,096 response tokens in thinking and returned no answer twice (25.11 and 24.63 seconds). Ollama now receives `think:false` by default, including older saved templates that omit the field. Explicit overrides and saved prompt/settings bytes survive. The parameter follows [Ollama's thinking API](https://ollama.com/blog/thinking).
- Muse Glimmer appended `<|eot|>` to otherwise valid JSON. Accept that exact terminal wrapper and fences around complete JSON only. Arbitrary prose, extra objects/fields and thinking are still rejected, and the 16 KiB limit applies first.
- A punctuation-based 1–2 sentence rule rejected normal abbreviations, unpunctuated languages and harmless line wrapping. Normalize whitespace; retain a 600-character/100-word hard bound and reject empty/code-block/punctuation-only summaries. The prompt still requests 1–2 sentences and 65 words. No factual content is truncated or invented.
- Invalid output receives at most one fixed correction instruction, with the original model, sources, schema and options. Rejected output never enters the prompt or store. Reserve 500 bytes within the 12,000-byte wire budget; both attempts share one 120-second generation deadline. Cancellation, model identity, tools and source/category authority remain enforced. Transport/model/tool failures are not retried.

### Installed model evidence

Windows, Ollama **0.34.2**, five installed model identities recorded in ignored `build/verification/summary-reliability/models.json`. No downloads, global runner changes or real mailbox inference. Eight pre-existing authored synthetic cases cover personal mail, receipts, a human mailing-list post, an operational alert, missing content, Spanish, embedded instructions and conditional security warnings.

Twenty direct baseline/probe requests preserve both thinking modes. Before the complete repairs, the first 80 engine cases completed 12/16 for Gemma, 0/16 for Glimmer and 16/16 for each Qwen. The final repaired path completed **80/80** repeated cases. A separately frozen, packaged engine then completed **40/40** further cases, shown below. These are completion results, not a claim of general semantic accuracy. Concurrent host work and model swaps affect latency; no idle-host or rigorous cold-start percentile claim is made.

| Model | Completed | Median seconds | Maximum seconds (includes load) | Kind / label matches |
| --- | --- | --- | --- | --- |
| `gemma4-obliterated:e4b` | 8/8 | 1.52 | 7.75 | 4/8 · 7/8 |
| `qwen3.8-obliterated:q4` | 8/8 | 1.64 | 19.08 | 8/8 · 8/8 |
| `qwen3.8-obliterated:q8` | 8/8 | 11.14 | 52.22 | 8/8 · 8/8 |
| `muse-glimmer:30b` | 8/8 | 2.21 | 34.55 | 8/8 · 4/8 |
| `qwen3.8:27b` | 8/8 | 1.36 | 15.55 | 8/8 · 8/8 |

Regular `qwen3.8:27b` is the recommended tested model; selections in the founder's real profile were not changed. The repeated repaired run matched all 16 sender kinds and all 16 label sets on both Qwen Q4 variants and Q8. Gemma matched only 8/16 kinds and 14/16 label sets; it added an unsupported recommendation, mistranslated the Spanish case and misclassified the security notice. Glimmer matched 16/16 kinds but only 7/16 label sets, often omitting Work despite describing a matching project discussion. These misses remain preserved; successful JSON validation cannot establish factual reliability. The much larger Q8 offered no observed label/kind benefit on this small set and remained substantially slower.

### Verification and delivery

Thirteen fm-assist tests, eight scoped Core model tests and three compatible-provider engine tests pass, with strict Clippy for assistance/Core/engine/CLI. The synthetic production engine/typed-CLI integration passes **31 HTTP generations**, covering older saved requests, edited instructions/options, exact wrapper handling, normalization, bounded successful/failed correction, cancellation during correction, unknown/duplicate IDs, forbidden fields/tools, manual precedence, staleness, owner isolation and shutdown. It passes again against the isolated package engine. Native production Settings/summary lifecycle/geometry regression exits successfully. Repository hygiene passes.

The isolated source is the recorded Git base plus only the four runtime files named in `source.json`; no concurrent UI or mail-query changes enter it. Release engine/CLI are rebuilt from that snapshot. The GUI and Qt dependencies are byte-identical to the preserved D079 interaction package. The configured candidate is `dist/summary-reliability-windows-amd64`; package checks use disposable profiles and preserve the founder's profile. Exact source, binaries, model outputs, timings, failed attempts and package checks live under ignored `build/verification/summary-reliability/`.

The initial integration attempt failed only its stale schema27 expectation; schema29 and the expanded regression pass afterward. Initial shell execution could not launch the restricted Python runtime; approved execution succeeded. The broad package self-test failure and its baseline comparison remain in `package.log` and `selftest-compare.log`; do not relabel that attempt as passing. Final package readback: relocated SDK-free configured empty-live GUI, CLI setup/status, schema29 reopen and final-client shutdown pass. The unchanged D079 package and repaired package both fail the broader self-test at stage30 (draft discard), with the same MailView slice warning and state, after 69.84/70.34 seconds; this is a preserved pre-existing limitation, not summary acceptance. ZIP: 40,639,751 bytes, SHA-256 81190c5fa9583edac7f6fc1605cdb60ad2ecbc4730c229e585ed07904ab41ec7. A later native rerun encountered concurrent D081 FmScrollBar edits in the shared source; recompiling the exact existing harness against the frozen D079 QML passes all Settings/summary lifecycle and geometry checks. Preserve both attempts and the existing Qt font-directory warning. Whole-area readiness remains **0/9**; other hosts and arbitrary real-mail quality are not qualified by this packet.


## D058 incoming-only analysis and response errors (2026-09-15)

The founder reports absent labels/sender kinds after summarizing and the same generic error twice, then clarifies that sent mail must never be summarized. Read-only profile metadata confirmed both policies enabled: recent received analysis had a saved Human classification and no matching label; the two recent summary-only records belonged to sent messages. No mailbox content or credential data was printed or imported into this report. Source inspection found the reader query omitted `sent`, so the GUI displayed received-message controls for sent messages while the engine excluded classification for them.

P2-LLM2D adds the true sent flag to each reader message, rejects sent inference targets, excludes sent members from batch/thread source selection, hides historical sent results and removes their previews/controls without deleting stored history. Incoming combined analysis still atomically records the enabled summary/kind and at most one matching label with manual/provider precedence. Zero matching labels is valid.

The old generic failure combined model mismatch, unfinished responses, length truncation and tool requests; it also wrongly treated a nullable `tool_calls` field as a tool request. The new handling accepts null/empty tool lists, rejects actual/malformed tool requests, and reports each failure distinctly. Combined job errors appear once. The default generation allowance increases from 1,024 to 4,096 tokens, retaining the 120-second and byte bounds with no automatic retry. Schema 24 archives old settings and updates only the former default allowance, preserving system/user text, model and account choices.

The installed Ollama 0.33.3 reports thinking capability for `gemma4:e4b`. A synthetic short acknowledgment probe consumed 769 tokens with thinking enabled and completed; this does not reproduce the exact screenshot failure. Disabling thinking reduced the run to 108 tokens but produced an inappropriate Work label, so thinking remains unchanged. See [Ollama thinking controls](https://docs.ollama.com/capabilities/thinking). The original generic error cannot establish which condition occurred in that particular screenshot.

### Scoped Windows evidence

Ignored files under `build/verification/llm2d-*` contain the actual evidence:

- `assist-final`, `core-final`, `labels-final` logs: 10 validator, six Core and 14 Google-mutation tests pass. New Core checks cover received/sent query flags, sent rejection, hidden-but-preserved old results and exact settings migration. Existing received classification/single-label application remains covered.
- `lint01.log`: strict all-targets fm-assist/Core/Engine/CLI Clippy passes.
- `native02.log`: native Settings/assistance test passes in 2.98 seconds, including hidden sent controls and an error on the actual message summary with no duplicate inline text.
- `integration01.log`: 16 engine/CLI checks pass, 13 inference requests, including exact template-to-HTTP equality, successful null tool lists, separate truncation/tool errors, combined classification/label behavior, cancellation and independent owners.
- `build01.log` and `gui02.log`: final production binaries. The configured candidate is `dist/llm2d-windows-amd64/bin/fuckmail-gui.exe`.
- `package01.log`: relocated SDK-free CLI, Google registration, empty-live GUI, schema-24 reopen and engine shutdown pass.
- `upgrade01.log`: the previous package creates/configures schema 23; the new package preserves message content/model/policies and expands the allowance; the old engine refuses and the new package reopens schema 24.
- `probe01.log`: the three synthetic thinking/budget comparison runs above. No real email was submitted for inference by this task.

`quality01.log`: 8/8 installed-model cases return valid complete outputs, 8/8 sender kinds match and 7/8 label sets match. The operational-alert sample still over-selects Work; language preference and occasional summary filler remain model-quality limits. This correction does not establish arbitrary-mail labeling quality, real Google delivery, other-host acceptance, sustained inference or whole Phase 2 readiness. Broad workspace and performance work remain deferred; readiness stays 0/9.


## D057 readable prompt content (2026-09-15)

The founder wants only system and user content, with normal newlines. P2-LLM2C / Local assistance 10 / Mail UI 35 / Quality 35 changes only presentation. Settings decodes each stored message into a plain-text editor, labeled by its role and retaining order. Editing one message serializes only its new content into the existing request; schema/options/model variables and other messages remain intact. The complete request remains available to the unchanged CLI/engine. Schema 23, fixtures 0/3 and protocol 1 are unchanged.

`build/verification/llm2c-native01.log` passes the native test in 3.02 seconds: decoded message-only content, real paragraph breaks, native edits with quotes/backslashes, exact preservation of every other request field for both user and system edits, save/reload/conflict handling and existing assistance lifecycle coverage. `llm2c-settings.png` was visually inspected for readable system/user sections without the JSON wrapper. `llm2c-gui01.log` built the delivered GUI. The configured candidate is `dist/llm2c-windows-amd64/bin/fuckmail-gui.exe`; its engine/CLI hashes match D056. `llm2c-package01.log` passes relocated SDK-free CLI/Google registration/empty-live GUI and schema-23 reopen/shutdown. No Rust, model-quality, broad workspace or real-mailbox checks were rerun for this presentation-only change. D056 quality and other-host limits remain.


## D056 complete request visibility (2026-09-15)

The founder found that D055’s “complete” prompt excluded the fixed system message and structured response schema. P2-LLM2B corrects that mismatch. Local assistance 9/Core 32/Mail UI 34/CLI 26/Quality 34 use schema 23, unchanged fixtures 0/3/protocol 1.

`message-request.json` is the actual default request template. Settings shows and edits its system/user messages, response schema, model variable, stream flag and bounded generation options in a single expanded JSON editor. Label descriptions enter only through the labels variable. The engine parses and substitutes model/labels/messages/partial/source_ids exactly once, then sends that object directly without adding or replacing prompt fields. The previous fixed envelope and schema generator are removed from inference. Validation of allowed outputs, source/category identity, manual precedence and mail actions remains application code, not hidden model instructions. The app request is distinct from Ollama’s subsequent model-specific chat template/tokenization.

Schema 23 archives the exact schema-22 settings before wrapping the saved prompt. Model choice, account opt-outs and label instructions survive. The schema-21 path also preserves earlier separate prompt preferences. Existing complete templates are recognized by full request validation rather than a superficial JSON-key check.

### Verification

Ignored logs/captures are under `build/verification/llm2b-*`. The final source/binary manifest excludes credentials. The separate candidate is `dist/llm2b-windows-amd64/bin/fuckmail-gui.exe`; the previous package remains intact.

- `llm2b-assist-final.log`: 10 tests pass, including literal quotes/backslashes/newlines and template-like content in source data, exact edited system/schema/options, typed ID arrays, unknown/duplicate variables, prohibited fields and option bounds.
- `llm2b-core-final.log`: four tests pass, including exact prior prompt/settings history and schema-22 migration/reopen, existing settings/staleness and combined message persistence.
- `llm2b-labels-final.log`: all 14 scoped Google-mutation tests pass.
- `llm2b-lint-final.log`: strict all-targets lint passes for fm-assist/Core/Engine/CLI.
- `llm2b-native03.log`: native Settings/assistance test passes (2.99 seconds). The actual editor is compared against the entire stored request; both message roles, schema and options are present, separate rules absent, native editing/save/conflict behavior retained. `llm2b-settings-final.png` shows the request editor with its variables and system/user content.
- `llm2b-integration01.log`: 15 engine/CLI checks pass. An independent template expander at the fake HTTP endpoint compares the received JSON object with the edited template, including a replacement system message, modified schema limit and temperature. No hidden defaults may appear. Existing cancellation, separate owners, summary-only, automatic-label and invalid-output checks pass.
- `llm2b-quality01.log`: installed `gemma4:e4b` produced eight complete valid combined outputs with correct sender kinds; seven of eight expected label sets match. The production-outage sample is incorrectly labeled Work despite its narrower human-project description. The Spanish example remains summarized in English and some summary filler persists. These are recorded quality limits, not an eight-case classification pass.

The first native assertion failed because Qt normalizes Windows line endings; the comparison now normalizes only CRLF to LF. The next failure was an obsolete assertion expecting the old prose-only edit; it now checks the complete edited request. Both failed logs remain. No inference or model-quality result was fabricated or silently repaired.

`llm2b-build03.log` and `llm2b-gui04.log` produced the final release binaries. `llm2b-integration-final.log` repeats all 15 checks against the packaged engine, including exact request equality. `llm2b-package-final.log` passes relocated SDK-free CLI, configured Google registration, empty-live GUI, schema-23 reopen and shutdown. `llm2b-upgrade02.log` passes an actual schema-22 profile created/configured with the previous package, exact prompt/model/account preservation, direct old-engine refusal and new-package reopen. The first upgrade fixture tried a label description absent from its account; the old CLI correctly rejected it (`llm2b-upgrade-final.log`), and the fixture was corrected. No production code changed for that fixture error. No real mailbox inference, provider action, broad workspace test or performance run was performed for this correction. Other-host builds and whole-product readiness remain open (0/9).

## D055 unified message analysis (2026-09-15)

This Windows delivery implements Local assistance 8, Core 31, Triage 8, Gmail 16, Mail UI 33, CLI 25 and Quality 33, with schema 22 and unchanged fixtures 0/3/protocol 1. The user requested one model/prompt, automatic single-label classification, useful prose summaries, summary list previews and removal of duplicate/ineffective controls.

Settings now exposes one installed model and the whole editable template, including label descriptions, email JSON and partial-source variables. Missing account preferences default to summaries and Auto-label new mail enabled; saved opt-outs remain. A combined result supplies a 1–2 sentence summary, sender kind and at most one existing label. Invalid output is rejected atomically. Manual category/sender choices win. Eligible Google label changes use the existing durable delivery journal, mapped-label permissions and provider reconciliation. Summaries never mark mail reviewed. Source-matched summaries replace list snippets with the Summary icon; only message-level cards remain. Old malformed summaries retain their stored bytes but are hidden pending regeneration.

Schema 21 upgrades preserve the exact prior settings JSON in history, carry forward the selected model, explicit flags and custom instructions, and add durable manual-label overrides. No credential material is added to Git.

### Verification

Ignored evidence is under `build/verification/llm2-*`; the configured portable app is `dist/llm2-windows-amd64/bin/fuckmail-gui.exe`.

| Check | Final evidence | Result |
| --- | --- | --- |
| Validator/template/prose | `llm2-assist-final.log` | 9 tests pass |
| Combined persistence, previews, manual precedence, settings migration | `llm2-core-final.log` | 3 tests pass |
| Google mutation/reconciliation, including assisted delivery and manual/provider precedence | `llm2-labels-final.log` | 14 tests pass |
| Scoped all-targets strict Clippy | `llm2-lint03.log` | Pass |
| Engine/CLI fake-provider integration | `llm2-integration-final.log` | 14 checks pass, 10 inference requests |
| Actual MailView summary/icon/fallback and duplicate-card removal | `llm2-selection07.log` | Pass |
| Settings defaults, summary-only, correction, cancellation, disconnection and themes | `llm2-native-settings05.log` | Pass |
| Installed `gemma4:e4b`, final default prompt | `llm2-quality02.log` | 8/8 synthetic cases pass format, expected kind and label checks |
| Actual old CLI creates schema 21; new package upgrades; old engine refuses; new package reopens | `llm2-upgrade02.log` | Pass |
| Relocated package without development SDK paths: CLI, registered Google setup, empty-live GUI and shutdown/reopen | `llm2-package-final.log` | Pass |

Release Rust build `llm2-build04.log` and native build `llm2-gui10.log` produced the delivered binaries. `llm2-settings-final.png` and `llm2-mail-final.png` were visually inspected: readable expanded Settings, one summary card beside its message and matching list preview/icon. Screenshots contain synthetic data. The relocated offscreen run reports Qt's missing default font-directory notice; the GUI embeds Geist, and visual checks use that font.

Earlier failed attempts remain scoped: stale shared-prompt expectations, missing operation-journal receipt, premature Settings assertions, incomplete native test stubs/date-header lookup, compiler/tool-path mistakes and two strict-lint errors were corrected before final checks. Most earlier logs are retained; the initial label-test log was overwritten during its retry, so it is not an exact preserved failure artifact. The old CLI status probe timed out on the upgraded schema; the direct old-engine refusal and subsequent new-package reopen provide the final migration evidence. Initial model-quality output contained filler; the refined prompt improved it.

### Limits

These checks use synthetic mail and generated provider observations, not automated actions on the founder's mailbox. New-mail processing retains the existing prospective session/date window; it does not sweep historical mail. Existing explicit opt-outs remain off. A model must be selected and Ollama available. Eight examples are a small quality sample: the Spanish case still summarized in English despite the prompt, and some output retains unnecessary no-action phrasing. Real Gmail label delivery, sustained workload/model quality, physical input and current Mac/Linux packages remain separate acceptance gates. No broad workspace or performance run was made under D041. Whole-product readiness remains 0/9.

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

2026-09-11. This experiment consumes [Local assistance revision 3](../contracts/local-ai.md), shared fixture revision 1 and the existing `fm-assist` output validator. It implements [Core's durable adapter](../../crates/fm-core/src/assist_store.rs) and the [historical schema-4 tables](https://github.com/yegors/fuckMail/blob/3168800/crates/fm-core/src/migration-004-assist.sql). No model, network transport, IPC command or GUI model control is introduced.

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

## D103 — persistent scoped diagnostics (2026-09-24)

The existing D082 console retained 1,000 session events only; it did not write a file. Its GUI IPC events, sync snapshots and coarse `assist_job` status did not expose automatic/background model output. This packet adds private per-profile `diagnostics/gui.log` and `diagnostics/engine.log`, with one previous segment each. Each segment is capped at 2,560 KiB (10 MiB combined across four segments). The GUI logs projected/redacted Qt, IPC, sync and state events. The engine writes structured metadata for model attempts, successful Core commits, failures, eligible format retries, Google sync state changes, bounded mail part publication and Calendar pass summaries. The GUI reads engine events into its scoped view; it still keeps only 1,000 recent lines for UI/report responsiveness. Clearing the view retains disk history. No raw mail, prompt, summary prose, response body, credential or full job token is written by these event producers.

The Windows offline `cargo check -p fm-engine` passes. Focused engine diagnostic tests pass 3/3: engine segment rotation, session-keyed references without raw IDs, and Unicode summary/classification projection without source or explanation text. The pre-existing marked-profile refusal tests pass 3/3 after diagnostics initialization was placed after fixture/profile admission. A disposable live-profile engine launch wrote `diagnostics/engine.log` with a structured startup event; it made no provider request. Qt 6.11.2's normal `gui-debug-console` CTest passes. Its focused cases cover GUI file rotation, engine import and rotation, scopes, redaction, capture, filtering, new-profile admission and the existing keyboard/focus flow; the updated test also checks that the scroll offset survives an added event. An earlier isolated native build in ignored `build/verification/debug-log-isolated/` also passed with a test-only HTML-view registration stub while unrelated QML references were incomplete. The normal GUI target later compiled the Rust engine and DebugConsole QML with `CARGO_NET_OFFLINE=true`, but stopped in unrelated in-progress `gui/LinkCard.qml:91` (`Illegal method name`). Strict `fm-engine` Clippy was stopped by three unrelated concurrent `fm-core` warnings in `message_source.rs`, `remote_prefs.rs` and `queries.rs`. No full integrated GUI build, package, live mailbox run, other-host result or whole-area readiness claim is made from this packet. The shared working tree had concurrent changes before D103 and they were not formatted or rewritten.

## D106 — console viewer and reminder identity (2026-09-24)

The reported jump came from replacing a whole TextArea document and inferring follow mode from its changing geometry. The viewer now appends immutable sequence-keyed rows to a bounded ListModel. Explicit user navigation controls follow mode; a first-visible event and pixel offset anchor survive removal from the front. Full selected event details remain plain, selectable text, including formatted metadata. Five logical groups replace the earlier 14 subsystem choices. The centered panel leaves 32 px around the window and uses the app's four themes, a quiet terminal mark, aligned columns and level markers. Existing per-profile disk logging, redaction and metadata projection remain.

`calendar_reminders` read the serialized occurrence key and provider calendar ID through Core's generic `string()` helper, which rejects more than 256 bytes. Its acknowledgement helper also applied this smaller bound before its own intended limits. Both now use the existing dedicated bounds directly: calendar 1,024 bytes, occurrence 8,192 bytes. A generated Google-shaped calendar ID over 256 bytes and its longer occurrence key reproduce the affected path. The regression verifies poll, shown acknowledgement, snooze, later reappearance and dismissal, plus malformed/oversized refusal. The fix does not suppress error logging.

**Windows verification (Qt 6.11.2, MSVC Release; generated data only):**

- `cargo test -p fm-core calendar_reminders::tests --offline`: **11 passed**, including the long-ID lifecycle and existing permissions, due windows, reopen, snooze and mail cursor checks.
- `gui-debug-console-test`: **7 passed** (including init/cleanup), preserving disk rotation/import/admission/redaction checks. Native wheel and scrollbar input, repeated appends with head eviction, explicit resume, keyboard Home/End/PageUp, resize, expiration of the anchor itself, pause/copy, filters and shortcut focus are exercised. The final scoped CTest passes in **3.20 s** with no QML warnings. The worker-warning case intentionally emits its synthetic warning.
- Complete `fuckmail-gui` C++ and compiled QML build passes in a separate Release `FM_GUI_ONLY=ON` directory. Graphite, Paper, Harbour and Terminal screenshots were inspected.
- Repository hygiene passes; scoped `git diff --check` passes.

Evidence stays ignored under `build/verification/d106-console/`, including logs, native screenshots and source/binary hashes. Failed attempts are retained: the first Rust test build encountered temporary bare-CR literals in concurrently edited `fm-mail/incoming.rs`; the retry passed after that unrelated file changed. A direct offscreen test launch without the prescribed software backend produced no output; the prescribed CTest environment passed. The normal combined Release build stopped when Windows refused replacement of the running `target/release/fm-engine.exe`; no process was terminated. The separate GUI build verifies this console's production QML without replacing the active app. The first hygiene attempt used an inaccessible system Python; the bundled runtime passed. No current full package, real-mailbox run or Mac/Linux result is claimed, and unrelated HTML/schema work was neither reverted nor reformatted.
