# Local mail analysis

## D103 — model diagnostic metadata (Local assistance21)

Every admitted sample, interactive, arrival and backlog inference records a start and a committed completion or failure in the private engine diagnostics log. Entries contain a short non-authorizing job reference, per-process keyed account/message references, task/origin, provider, selected model and its available digest, input character count, source and label-choice counts when available, partial state, elapsed time, Unicode summary character count, sender kind, chosen category IDs, abstention, label application state and saved prompt SHA-256 when available, and failure code. A malformed Ollama response that is eligible for its one correction records a format-retry event. No prompts, message bodies, summary text, model response, raw account/message ID, credential or complete job token is logged. The engine writes these records for GUI and CLI sessions; the GUI imports them into the Model scope. This is diagnostic coverage, not a model-quality or live-mail acceptance claim. Consumers: engine, Mail UI72 and Quality73; schema33/fixtures0,3/protocol1 unchanged.

## D097 — synced-mail queue (Local assistance20)

The watcher processes every synced received message missing an enabled summary or classification, in store order, from a cursor persisted in `meta` (`local_model_watch_cursor`). This includes a new account's first-sync history and mail synced while no watcher ran. Each writer call scans at most 32 rows and returns at most four messages; the watcher runs them one at a time through the shared inference seat. A busy seat retries after five seconds. A runner-level failure (assistance unavailable or disabled, model changed or unavailable, engine stopped, account removed) holds the cursor and retries after 30 seconds. A message that cannot be analysed is skipped. The cursor advances only past admitted work. A message is an arrival, eligible for label Auto-read, only when dated at or after the watcher's start and at most one minute in the future. A one-shot lower bound (`local_model_watch_requeue`) rescans when an account's assistance turns from off to on, when its first label import adds labels, and after messages are deleted, because SQLite can reuse the newest row IDs. An absent account policy still means Auto-summarize and Auto-label on. Consumers: GUI and CLI `assist watch` through the shared engine; Core54, Gmail27.

## D096 — Auto-read and arrival authority (Local assistance19)

Changing a label's Auto-read choice saves settings without clearing model People/Automated results; only classification or label-description changes invalidate them. The arrival flag that permits Auto-read is set only by the engine's new-arrival watcher. Client `assist_start` requests cannot claim it. Consumers: GUI and CLI through the shared engine; Core53.

## D092 — per-account assistance placement (Local assistance18)

Auto-summarize and Auto-label live in Settings → Mail's selected-account section, above the same account's Labels. They save by compare-and-swap into the existing assistance settings document, preserving other accounts, prompts and label Auto-read. Local model settings retains provider/model/prompt controls. New-arrival work follows the saved account policy; switching a settings account never starts historical analysis. Summary presentation uses the existing result's model and generation timestamp. Consumers: Core50/Mail UI67/Quality67; schema31, fixtures0/3/protocol1 unchanged.

## D090 — per-label Auto-read (Local assistance17)

Each account's label settings hold an explicit Auto-read choice. A missing choice resolves to on for a Receipts label and off for every other label. The choice may be switched independently per account. Only a complete, current classification started by the new-arrival watcher that assigns a permitted label can mark that incoming message read; manual/backlog analysis, summaries, partial results, sent/trashed mail and labels blocked by a manual override cannot. Existing manual read intent wins. Live accounts require mail-write authority and enqueue the same durable Gmail read mutation as an explicit read action; synthetic accounts update local read state. Previously read messages are unchanged. This founder opt-in choice supersedes the earlier manual-only review rule for matching labels. Schema30 fences older engines from silently losing the new typed settings field. Consumers: Core49, Mail UI66, Quality66; fixtures0,3/protocol1 unchanged.

## D086 — mail list and account backlog

Explicit account backlog analysis is now authorized through the Accounts menu or CLI `assist batch --range week|month|year|all`. It is never triggered by browsing, settings save or startup. `assist_backlog` starts a read-only asynchronous coverage job; poll `assist_job` for local/eligible/processed/missing counts and four date-window counts. GUI counts never hold the IPC connection open for the full scan. Week/month/year mean rolling 7/30/365 days through the captured UTC time; Everything means all eligible cached dates through that time. Future/invalid dates are excluded. `assist_batch_start` creates a session-owned job; existing `assist_job`, `assist_jobs` and `assist_cancel` expose progress and cancellation. Scan bounded pages up to the captured time/row ceiling, then prepare one target at a time. Only missing enabled projections are generated; current summaries survive classification-only work. At most one batch per account and 16 admitted jobs, sharing the single inference slot. Release inference ownership between messages for interactive jobs. Isolated failures remain counted and do not retry indefinitely. Settings or lifecycle changes stop the batch. Completed results persist, canceled jobs do not resume. A current partial result is processed but remains labeled partial in the reader; this avoids endlessly regenerating a source beyond the context limit.

Consumers: Core47 / Local assistance16 / Mail UI62 / CLI41 / Quality62. Schema29, fixtures0/3 and protocol1 unchanged.


## D080 — summary reliability (Local assistance15)

Consumers: fm-assist, Core44 request preparation, Engine/Ollama, CLI39 and Mail UI56; Quality57. Schema29, live/synthetic fixtures0/3 and protocol1 unchanged. Default direct answers apply to older saved Ollama requests as well as the shipped template. User instructions, settings bytes and receipt history remain intact. Validation keeps source/category identity and output bounds authoritative while accepting normal multilingual punctuation and whitespace. Quality evaluation must distinguish request completion from factual correctness and classification accuracy; do not silently substitute another model.

Accept an exact trailing `<|eot|>` token and a fence around the complete JSON response before schema validation. Do not search arbitrary prose or thinking for JSON, discard extra fields/objects, or trim model-generated facts to fit. Empty/punctuation-only or code-block summaries remain invalid. The 16 KiB content limit applies before wrapper normalization.

## D072 — account and model feedback (Local assistance14)

Keep the selected model and discovered catalog visible while Detect refreshes. Show unavailable only after a successful current-endpoint discovery no longer contains the saved name and digest. Native selection saves the exact chosen identity; inference retains existing identity checks.

Fit incoming text against the fully expanded serialized request, including message content, schema, labels, options, sources and JSON escaping. Keep a nonempty UTF-8 prefix within the existing 12,000-byte boundary and mark truncation partial. Do not discard the whole email merely because escaping expands it. Incoming-only classification, one-label application, summaries and manual precedence remain unchanged. Schema29 accompanies account changes; it does not migrate prompts.

Consumers: Core42/Gmail22/Local assistance14/Mail UI49/CLI37/Quality49, schema29; Calendar25, fixtures0/3 and protocol1 unchanged. Whole-area readiness remains 0/9.


D061 / P2-UI3 adds account-local label creation, descriptions and removal plus explicit Google name merging/creation. Removal clears local membership and mappings, retaining historical receipts; it does not delete Gmail labels or mail. Descriptions use the existing assistance settings and stay local. Schema 26 journals provider creation before dispatch; unknown outcomes require inventory reconciliation, with no blind retry. Unmapped local memberships wait for an explicit link/sync before provider delivery. The decision and [prototype execution record](../research/prototype-september-15.md) own the detailed scope and evidence.

State: Draft · Revision: 17 · Updated: 2026-09-23 · Implementation: D090 scoped arrival/remote-journal and native label-control checks pass; configured package and real-provider readback remain open. Schema30, fixtures0/3, protocol1 and whole-area readiness0/9 unchanged.

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

The Engine sends the expanded object to Ollama's `/api/chat`. D080 defaults an omitted `think` to false, preserving saved template bytes and explicit boolean overrides. It does not replace the schema or override other options. After a format/completion failure only, it may append one fixed correction instruction and retry once with the same model and source; rejected output never enters the correction prompt. The chosen model must occupy `model`; `stream` must be false. Accept only model/messages/format/stream/options and optional boolean think, 1–8 system/user text messages and the bounded options below. The default schema spells out every combined output field. Candidate label IDs are checked against the captured account catalog by application validation, even if the user changes the displayed schema. Legacy CLI summary/classification requests use the same combined model response, then project their existing return shape after validation. `assist settings` returns the same complete template as the GUI.

Application validation is not extra prompt text and cannot be disabled by editing the prompt: category/source identity, output size, prose, staleness, manual precedence and allowed actions still apply. No tools, hosted-model fallback, cross-account content, unpublished drafts, attachments or remote images enter inference. Ollama applies the installed model's own chat template and tokenization after receiving the app's JSON; the app does not claim to display that service-side token stream.

## One result per received message

Only received messages are eligible. Sent mail has no assistance controls or visible historical summaries/classifications/previews; stored historical results are retained. The reader query must include the actual `sent` flag. Reject explicit sent targets and exclude sent members from thread/batch requests.

The `message` task makes one model call returning `summary`, `source_ids`, `sender_kind`, `category_ids`, `category_abstain` and `reason`. Persist the enabled summary and classification projections atomically with model/prompt/source provenance. Sent messages are never eligible. Legacy explicit summary/classification methods remain readable; ordinary GUI analysis and CLI `assist analyze` / `assist classify` use the combined task.

- Summary: prompt guidance is one or two short prose sentences and at most 65 words. Validation normalizes whitespace and enforces nonempty prose containing letters/numbers, at most 100 words and 600 characters; sentence punctuation is not an acceptance gate; no bullets, headings or incomplete generations. Cover the substance and explicit recipient request, including material deadlines/amounts. Preserve conditional warnings, proposals and uncertainty. Do not invent actions, commitments or security incidents. Describe the event directly and address the reader as “you.”
- Sender kind: Human, Automated or Unknown, judged from the current authored content rather than forwarded text or a no-reply address alone. Human mailing-list posts may still be Human. Importance is separate from origin. Manual corrections remain authoritative.
- Label: at most one existing account category, supported by its name and instructions. No fitting definition means an empty array; insufficient content means abstention. Never invent categories, clear existing labels or remove Inbox. Partial source results cannot auto-apply a label.

Validate the complete output before any application. Reject extra fields, invalid IDs, multiple labels, invalid prose, actual tool calls, oversized output and incomplete responses. Null or empty `tool_calls` is not a tool request. Report model mismatch, truncated output, unfinished responses and actual tool requests distinctly. A response-limit error never applies a partial classification or label. Show a combined failure once in the message summary, or once inline when summaries are disabled. Coverage, timestamps and staleness are Core-owned. An invalid result does not partially apply a classification or label. No heuristic replacement or automatic retry repairs invalid model output.

## Automatic labels and manual precedence

Apply the selected label locally and enqueue its exact message/category intent through the existing Google mutation journal. Maintain an immutable assisted operation receipt and a distinct assisted intent; never pretend the model's choice was a human edit. Existing permission, connection, account-lifecycle, mapping, observation and reconciliation checks govern remote delivery. Only mapped Gmail user labels are eligible, and missing write permission or mapping is reported in the result's label state. No Send or invitation is involved.

Manual category changes record a durable message-level override and discard outstanding assisted intents. Later inference and queued label delivery yield to that override, including after the manual provider acknowledgement. A newly observed competing mapped provider category before first submission cancels the assisted intent. Confirmed provider readback retires the assisted intent, so later provider changes remain authoritative. Read/unread, stars, Done/Delay, drafts, composition, Inbox/Trash/Spam and Calendar remain outside model authority.

## New-mail scope and session lifetime

The existing watch establishes a prospective arrival cursor for the owning GUI or explicit CLI watch session. It processes newly retained incoming messages whose message date is within that session's arrival window; initial history/backfill and older messages remain excluded. This date-based admission can omit delayed deliveries and is not a complete historical classifier. Enabling, restarting, browsing, model changes and edited definitions never authorize a mailbox sweep. Explicit message/thread analysis processes captured received-message identities, with bounded progress and cancellation.

Use one active inference and at most 16 admitted jobs. Keep the existing 3-second connect, 5-second discovery and 120-second generation limits; total response ≤256 KiB and accepted content ≤16 KiB. The initial serialized expanded request is ≤11,500 UTF-8 bytes, reserving space for a correction within the unchanged 12,000-byte wire limit. Both generation attempts share one 120-second timeout and cancellation token. Transport, model-identity and tool-call failures are not retried. Editable options are temperature 0–2, context 1,024–16,384 tokens and output 1–4,096 tokens; defaults are 0/16,384/4,096. Unsupported fields or out-of-bound settings fail validation rather than being silently replaced. Surface context incompatibility. Queue saturation retains the arrival cursor; failures remain visible after at most one correction attempt; there is no persistent retry loop. Closing an owner cancels its jobs; never stop or reconfigure the shared Ollama service.

Ollama endpoints must be HTTP loopback with explicit ports. Bypass proxies; reject redirects, credentials, non-loopback hosts and arbitrary URL paths. Check installed model metadata, local weight format, completion capability and pinned digest before use and digest after generation. Do not install or silently switch models. Cloud entries are unavailable.

## Reader and conversation list

Only per-message Summary cards appear in the reader. Remove the top thread card and Summarize thread control. Keep message regeneration, cancellation, source details and sender correction. Hide old summary text that violates the new prose format until explicit regeneration; retain original stored bytes.

For each conversation row, use the current summary of the same message that supplies its source preview. Show the existing assistance icon beside summary text. Without a current complete summary, show the ordinary source snippet. A stale, partial or different-message summary cannot silently replace the preview. Preserve source bodies, MIME and the search index. Model summaries do not mark mail reviewed. New source/deletion identity, model or prompt changes invalidate the relevant preview; changing unrelated messages must not retarget it.

## Verification and limits

Require settings migration/reopen/refusal, combined output/template validation, default policies, manual and provider precedence, exact single-label delivery, unchanged review flags, source-matched previews, native Settings/reader/list controls and scoped engine/CLI session regressions. Evaluate the installed model with synthetic mail, including the conditional access-confirmation example, and retain failures. A small fixture pass does not prove arbitrary-mail quality, continuous coverage, other-host acceptance or full Phase 2 readiness.
