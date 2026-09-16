# Local mail analysis

## D072 — account and model feedback (Local assistance14)

Keep the selected model and discovered catalog visible while Detect refreshes. Show unavailable only after a successful current-endpoint discovery no longer contains the saved name and digest. Native selection saves the exact chosen identity; inference retains existing identity checks.

Fit incoming text against the fully expanded serialized request, including message content, schema, labels, options, sources and JSON escaping. Keep a nonempty UTF-8 prefix within the existing 12,000-byte boundary and mark truncation partial. Do not discard the whole email merely because escaping expands it. Incoming-only classification, one-label application, summaries and manual precedence remain unchanged. Schema29 accompanies account changes; it does not migrate prompts.

Consumers: Core42/Gmail22/Local assistance14/Mail UI49/CLI37/Quality49, schema29; Calendar25, fixtures0/3 and protocol1 unchanged. Whole-area readiness remains 0/9.


D061 / P2-UI3 adds account-local label creation, descriptions and removal plus explicit Google name merging/creation. Removal clears local membership and mappings, retaining historical receipts; it does not delete Gmail labels or mail. Descriptions use the existing assistance settings and stay local. Schema 26 journals provider creation before dispatch; unknown outcomes require inventory reconciliation, with no blind retry. Unmapped local memberships wait for an explicit link/sync before provider delivery. The decision and [prototype execution record](../research/prototype-september-15.md) own the detailed scope and evidence.

State: Draft · Revision: 14 · Updated: 2026-09-16 · Implementation: D072 account/model feedback at schema29. Scoped evidence is recorded in the prototype execution report; whole-area readiness remains 0/9.

## Local compatible servers (D060)

Consumers: Core 35, Engine/fm-assist, Mail UI 38, CLI 29 and Quality 38; schema 25, fixtures 0/3, protocol 1. Settings add optional `provider`: `ollama` (default, omitted when serialized) or `openai_compatible`. Ollama retains its exact prior settings and snapshot representation. Compatible models use a server model `name` and empty `digest`; no weight verification is claimed. Provider changes invalidate pending/derived results but preserve manual annotations. Schema 25 copies sender-kind rows unchanged into a table additionally permitting `openai_compatible` provenance; all other stored settings, histories and receipt bytes remain intact.

Discover compatible models with GET `/v1/models`; submit POST `/v1/chat/completions`. Accept only HTTP loopback origins with explicit ports, optionally ending `/v1` for this provider. Send the expanded System/User messages unchanged, the stored `format` as strict `response_format.json_schema`, `temperature`, `max_tokens` and `stream:false`. Context capacity is managed by the server; `num_ctx` applies only to Ollama. Retain the 12,000-byte request, 256-KiB response, cancellation, queue and timeout bounds. Require one complete response from the selected model with nonempty content and no tools, then validate the same combined result. Model IDs must remain advertised before and after the request; that does not establish unchanged weights. Unsupported schemas or server errors remain visible without fallback/retry.

The GUI presents provider, endpoint and one model selector. Switching providers clears incompatible model/discovery state and preserves prompts/account choices. Discovery never selects a model or runs inference. The description explains that mail goes to the chosen local server and that its configuration controls model execution. CLI discovery accepts `--provider ollama|openai_compatible` and the same endpoint. Standard-compatible discovery cannot prove a server's local execution or weight identity.

Owner: Local assistance. Consumers: Core 33, Engine/fm-assist, Triage 8, Gmail 16, Mail UI 36, CLI 27 and Quality 36. Schema 24; fixtures 0/3 and protocol 1 are unchanged. D055 supersedes D051–D053's separate model/prompt, suggestion acceptance and default-off delivery choices. Historical reports and result bytes retain their original scope.

## Settings and defaults

Use Settings → Local model. Discover the same-machine Ollama service and its installed models on entry, with Refresh models and the loopback endpoint override. Show one **Model** choice and editable **System** and **User** message-content fields under **Prompt**, in their stored order. Display decoded plain text with normal newlines and paragraph breaks. Request JSON, schema and generation options are not displayed. Editing a message changes only that message’s content; preserve all other request fields and roles. Remove Test with sample and Restore default buttons. Preserve Save, Cancel edits, explicit conflict reload and unsaved-edit protection.

The shared settings object is `{endpoint, model, prompt, accounts}`. A model is `{name, digest}` or null. Each account is `{classification, summaries, descriptions}`. Missing account policy defaults both switches on. **Summarize new mail** enables prospective per-message summaries; **Auto-label new mail** enables sender-kind classification and automatic label application together. Explicit saved off choices remain off. No model is selected automatically, and no inference can run until the user chooses an eligible installed model. Missing/unavailable runners leave manual mail usable.

Schema-21 settings upgrade atomically to schema 22. Prefer the saved classification model, then the summary model. Start with the new common prompt and append customized old instructions; store the exact original settings JSON and revision in `local_model_settings_history`. Preserve account switches and descriptions, existing messages, old results and operation receipts. Schema 22 upgrades to 23 by archiving its exact settings JSON/revision and wrapping its saved prompt in the complete request template. Existing complete templates are not wrapped twice. Schema 23 upgrades to 24, archiving exact prior settings and changing only the former 1,024-token default to 4,096. Preserve custom message content and other settings. Old binaries refuse schema 24. Prompt/model edits invalidate applicable cached results and pending inference; they do not trigger a historical sweep.

## Visible dynamic prompt

The [shipped request template](../../crates/fm-assist/src/message-request.json) is the editable default. Storage and CLI retain the complete JSON request. Under D057 the GUI displays only its decoded system/user message contents, with no JSON escapes. It preserves the undisplayed request fields when saving content edits. Keep these variables exactly once in the stored request:

| Variable | Engine-supplied value |
| --- | --- |
| `{{model}}` | Selected installed model name |
| `{{labels}}` | Current account category IDs, names, mapped provider IDs and user-written label instructions |
| `{{messages}}` | Selected source ID, sender/recipient, subject/date, sent flag and retained text |
| `{{partial}}` | Whether supplied source content was shortened |
| `{{source_ids}}` | Allowed source IDs for the response schema |

Parse the JSON, then substitute string values in a single pass. A whole-value variable receives its JSON type, such as the source-ID array; variables embedded in content strings receive serialized data. Inserted email text and label descriptions are never interpreted as templates. Reject missing, duplicate, unknown or unclosed variables. Do not permit variables in keys. Label instructions remain editable below the request and enter only through `{{labels}}`.

The Engine sends that expanded object directly to Ollama's `/api/chat`; it appends no messages, replaces no schema, and overrides no displayed options. The chosen model must occupy `model`; `stream` must be false. Accept only model/messages/format/stream/options, 1–8 system/user text messages and the bounded options below. The default schema spells out every combined output field. Candidate label IDs are checked against the captured account catalog by application validation, even if the user changes the displayed schema. Legacy CLI summary/classification requests use the same combined model response, then project their existing return shape after validation. `assist settings` returns the same complete template as the GUI.

Application validation is not extra prompt text and cannot be disabled by editing the prompt: category/source identity, output size, prose, staleness, manual precedence and allowed actions still apply. No tools, hosted-model fallback, cross-account content, unpublished drafts, attachments or remote images enter inference. Ollama applies the installed model's own chat template and tokenization after receiving the app's JSON; the app does not claim to display that service-side token stream.

## One result per received message

Only received messages are eligible. Sent mail has no assistance controls or visible historical summaries/classifications/previews; stored historical results are retained. The reader query must include the actual `sent` flag. Reject explicit sent targets and exclude sent members from thread/batch requests.

The `message` task makes one model call returning `summary`, `source_ids`, `sender_kind`, `category_ids`, `category_abstain` and `reason`. Persist the enabled summary and classification projections atomically with model/prompt/source provenance. Sent messages are never eligible. Legacy explicit summary/classification methods remain readable; ordinary GUI analysis and CLI `assist analyze` / `assist classify` use the combined task.

- Summary: one or two short prose sentences, at most 65 words and 600 characters; no bullets, headings or incomplete generations. Cover the substance and explicit recipient request, including material deadlines/amounts. Preserve conditional warnings, proposals and uncertainty. Do not invent actions, commitments or security incidents. Describe the event directly and address the reader as “you.”
- Sender kind: Human, Automated or Unknown, judged from the current authored content rather than forwarded text or a no-reply address alone. Human mailing-list posts may still be Human. Importance is separate from origin. Manual corrections remain authoritative.
- Label: at most one existing account category, supported by its name and instructions. No fitting definition means an empty array; insufficient content means abstention. Never invent categories, clear existing labels or remove Inbox. Partial source results cannot auto-apply a label.

Validate the complete output before any application. Reject extra fields, invalid IDs, multiple labels, invalid prose, actual tool calls, oversized output and incomplete responses. Null or empty `tool_calls` is not a tool request. Report model mismatch, truncated output, unfinished responses and actual tool requests distinctly. A response-limit error never applies a partial classification or label. Show a combined failure once in the message summary, or once inline when summaries are disabled. Coverage, timestamps and staleness are Core-owned. An invalid result does not partially apply a classification or label. No heuristic replacement or automatic retry repairs invalid model output.

## Automatic labels and manual precedence

Apply the selected label locally and enqueue its exact message/category intent through the existing Google mutation journal. Maintain an immutable assisted operation receipt and a distinct assisted intent; never pretend the model's choice was a human edit. Existing permission, connection, account-lifecycle, mapping, observation and reconciliation checks govern remote delivery. Only mapped Gmail user labels are eligible, and missing write permission or mapping is reported in the result's label state. No Send or invitation is involved.

Manual category changes record a durable message-level override and discard outstanding assisted intents. Later inference and queued label delivery yield to that override, including after the manual provider acknowledgement. A newly observed competing mapped provider category before first submission cancels the assisted intent. Confirmed provider readback retires the assisted intent, so later provider changes remain authoritative. Read/unread, stars, Done/Delay, drafts, composition, Inbox/Trash/Spam and Calendar remain outside model authority.

## New-mail scope and session lifetime

The existing watch establishes a prospective arrival cursor for the owning GUI or explicit CLI watch session. It processes newly retained incoming messages whose message date is within that session's arrival window; initial history/backfill and older messages remain excluded. This date-based admission can omit delayed deliveries and is not a complete historical classifier. Enabling, restarting, browsing, model changes and edited definitions never authorize a mailbox sweep. Explicit message/thread analysis processes captured received-message identities, with bounded progress and cancellation.

Use one active inference and at most 16 admitted jobs. Keep the existing 3-second connect, 5-second discovery and 120-second generation limits; total response ≤256 KiB and accepted content ≤16 KiB. The serialized expanded request is ≤12,000 UTF-8 bytes. Editable options are temperature 0–2, context 1,024–16,384 tokens and output 1–4,096 tokens; defaults are 0/16,384/4,096. Unsupported fields or out-of-bound settings fail validation rather than being silently replaced. Surface context incompatibility. Queue saturation retains the arrival cursor; failures remain visible without a retry loop. Closing an owner cancels its jobs; never stop or reconfigure the shared Ollama service.

Ollama endpoints must be HTTP loopback with explicit ports. Bypass proxies; reject redirects, credentials, non-loopback hosts and arbitrary URL paths. Check installed model metadata, local weight format, completion capability and pinned digest before use and digest after generation. Do not install or silently switch models. Cloud entries are unavailable.

## Reader and conversation list

Only per-message Summary cards appear in the reader. Remove the top thread card and Summarize thread control. Keep message regeneration, cancellation, source details and sender correction. Hide old summary text that violates the new prose format until explicit regeneration; retain original stored bytes.

For each conversation row, use the current summary of the same message that supplies its source preview. Show the existing assistance icon beside summary text. Without a current complete summary, show the ordinary source snippet. A stale, partial or different-message summary cannot silently replace the preview. Preserve source bodies, MIME and the search index. Model summaries do not mark mail reviewed. New source/deletion identity, model or prompt changes invalidate the relevant preview; changing unrelated messages must not retarget it.

## Verification and limits

Require settings migration/reopen/refusal, combined output/template validation, default policies, manual and provider precedence, exact single-label delivery, unchanged review flags, source-matched previews, native Settings/reader/list controls and scoped engine/CLI session regressions. Evaluate the installed model with synthetic mail, including the conditional access-confirmation example, and retain failures. A small fixture pass does not prove arbitrary-mail quality, continuous coverage, other-host acceptance or full Phase 2 readiness.
