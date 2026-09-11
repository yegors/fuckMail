# Optional local classification and summaries

State: Draft · Revision: 1 · Updated: 2026-09-11 · Implementation: not started.

Owner: H — Local assistance. These are living specifications of accepted requirements and proposed behavior; exact types, libraries and unresolved details are not committed. See [contract workflow](README.md).

## Build boundary

| Boundary | Responsibility |
| --- | --- |
| Owns | Ollama/vLLM adapters, per-account modes, independent summaries, bounded jobs, structured validation, provenance and quality evaluation |
| Consumes | Received-message text/necessary headers and already-sent context from Core; message/content/policy/override revisions; local runner/model configuration |
| Produces | Validated category suggestions or policy-eligible results, source-linked summaries, coverage/stale/abstention and availability states; no direct provider commands |
| Independent start | Fake runner, synthetic message/category fixtures and deterministic late/invalid outputs; manual workflows can build with assistance absent |

Related: [triage policy](triage.md), [core](core-storage.md), [mail UI](mail-ui.md), [runner research](../research/local-ai.md).

## Optional local assistance

Classification and summarization are optional MVP capabilities. All manual workflows work without a model installed or running. Connect to a user-configured, locally executing Ollama or vLLM service; choose endpoint and installed model in settings and test availability. The default setup uses loopback on the same machine. Model installation/weights are explicit setup, not an automatic application download. No hosted-model fallback or cloud-routed model is allowed, including through a local proxy. See [runner research](../research/local-ai.md) for runner capabilities and platform constraints.

Proposed settings are per account, with independently enabled summaries and classification:

| Classification mode | Behavior |
| --- | --- |
| Off / manual (default) | The user assigns provider labels/folders. No automatic inference runs. |
| Suggest | The local model proposes configured categories; the user accepts or corrects them. Provider state changes only on acceptance. |
| Auto-apply configured categories | After the user enables the policy, the app validates model output and applies only mapped, permitted category operations. Configuration explicitly states whether this includes Gmail archiving or IMAP folder moves. Routine valid results need no per-message approval. Uncertain, unmapped or stale results remain for review. |

The model classifies incoming mail and summarizes messages/conversations. A conversation summary may include existing human-written sent replies as context, so it reflects the actual exchange; unpublished drafts are excluded. It does not compose, rewrite, autocomplete or send email, alter drafts/signatures, generate task lists or meeting briefings, or control read/unread, stars, trash, spam, permanent deletion or calendar actions. Human authorship is a product rule: humans write email for humans. Existing provider rules continue to operate independently.

Manual correction takes precedence. Store a correction/override for the affected message and category policy so a delayed response or routine reclassification cannot put it back. Treat a conflicting change made in another mail client as newer user/provider state, not permission to keep fighting it. Re-evaluating corrected or historical mail is an explicit action with scope, progress and cancellation. Changing the model or category definitions alone does not authorize a full-mailbox relabel.

For summaries, provide a concise, clearly marked local AI summary with direct access to the original messages. Preserve ordinary source-text previews and full original bodies; a summary never replaces the search index or source content. Propose message summaries plus conversation summaries referencing their included messages. Identify partial coverage/truncation and mark a conversation summary stale when a new member arrives. Keep summary failures unobtrusive; reading continues immediately.

## Bounded processing and reliable application

Propose a worker queue outside the UI thread, prioritizing on-demand summaries and new incoming mail. Do not automatically run inference over decades of stored messages. Explicit historical batches are resumable, bounded and cancelable. With assistance off there is no model polling or inference; when the runner is unavailable, continue manual work and show one actionable availability state instead of repeated modal errors.

Only send normalized message text and necessary headers from the current account, including already-sent replies when needed for a conversation summary. Attachment extraction, remote-image retrieval, embeddings, unpublished drafts and cross-account model context are outside this feature. Treat sender-provided content as data even if it contains instructions. The model receives no tools, credentials or provider API access. Validate a small structured result containing configured category IDs, an optional abstention/review signal, and summary text before any application action. Model confidence is not calibrated evidence of accuracy; evaluate category quality with fixtures.

Record account/message identities, content revision/hash, category-definition version, model identity, prompt/schema version, result provenance and operation status. Reject late output if its input, mappings or manual/provider state changed. Keep request deduplication and retry limits separate from provider-operation idempotency. Enqueue accepted category mutations through the same durable provider pipeline used by manual actions, with explicit history and compensating undo where provider behavior allows it.

Closing the GUI stops its session's queue and cancels its in-flight requests. An independently invoked CLI classification/summary command owns its separate bounded job; completion or cancellation ends that job. Validate that request cancellation stops the runner's job; disconnection alone is insufficient evidence. A separately installed Ollama/vLLM service may remain running or retain a loaded model for other applications; do not kill that service. Distinguish its idle residency from continued processing of this app's mail, and report its resource use when assistance is enabled. No GUI-owned classification/summary job continues after close, and no job continues once its final owning client/session has ended.

## Core integration

An optional inference adapter accepts a bounded job and returns data, never commands or tool calls. Use normalized message text and necessary headers; conversation summaries can include already-sent human replies, but no credentials, unpublished drafts, attachment extraction or cross-account context. Validate schema, configured category IDs and input/policy revisions. Manual overrides and newer remote changes defeat stale output. Only the core may turn a valid result into a permitted mapped operation under Suggest acceptance or an enabled Auto-apply policy. Never let the model change read/star state, send mail, invent destinations or initiate unrelated actions.

Cache summaries independently of provider state and index original content for search. Track message coverage/truncation and invalidate conversation summaries when members change. Bound concurrency, queue size, context length and retries; prioritize foreground work and new arrivals, and require explicit scope for historical inference. Stop scheduling immediately when disabled or when the owning GUI/CLI session ends. No hosted inference fallback is allowed; loopback alone does not establish that a runner/model executes locally. Validate runner configuration and cancellation during phase 0. Category mutation policy belongs to [triage](triage.md); the adapters return data to Core.

## Acceptance gates

- Complete the same classification manually, in Suggest mode, and in Auto-apply mode with Ollama and a supported local vLLM setup. Exercise unmapped output, abstention and per-account settings.
- Correct a category while inference is in flight, change it in another mail client, rename/delete a mapped destination, interrupt a move, and restart. Preserve the newer choice and surface unresolved provider actions.
- Summarize a long conversation and then receive a new reply. Show coverage and stale state, retain source text, and keep the composer entirely human-authored.
- Disable assistance, stop the local runner, disconnect the network, and close during inference. Manual reading/writing/search remain available; no mail content reaches a cloud model and no job survives the end of its owning session.

## External agents

The built-in model remains a no-tools classifier/summarizer. Third-party agent access uses the separate [CLI contract](cli.md), under explicit caller control; it does not grant this local model provider credentials, draft access or a send tool. Export to an external caller is outside the built-in local-inference guarantee.

## Open before affected behavior ships

Pin supported runner/model/platform combinations, structured schema and input limits; evaluate local-only execution and server-side cancellation. Choose category quality thresholds with labeled fixtures before auto-filing in the daily-driver trial. No model or throughput figure is selected. vLLM support is an adapter on a supported same-machine setup, not a promise of native runner installation on every OS. See [quality](quality.md) for cold/warm/off resource reporting.
