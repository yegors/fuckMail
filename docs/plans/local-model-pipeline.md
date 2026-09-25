# Local model pipeline refactor — plan

**Status:** implemented by D114 (2026-09-24) as [Local assistance25](../contracts/local-ai.md#d111--one-queue-one-email-systemuser-prompts-local-assistance25), schema36 and CLI48. The contract is now the source of behaviour; this file keeps the audit, the reasoning and the differences below. Readiness is unchanged.

**Goal:** one simple, robust pipeline for summaries, labels and sender kind. Each email is summarized alone, without its quoted reply chain. One queue serves single emails and mass catch-up. New mail goes to the front of the queue. Each account's queue can be paused, resumed or started (Catch up). No wasted HTTP calls, retries or whole-thread packing.

## What exists today (audit, working tree 2026-09-24)

- **Email text is already mostly stripped.** Gmail sync stores `extract_parsed_incoming_contribution` output in `messages.body` (`fm-core/src/google_store.rs:423`). This drops `>` lines, English "On … wrote:", "Original Message" separators, Outlook header blocks and HTML `blockquote`. Raw MIME stays outside the database as the exact offline source (product requirement). Gaps:
  - Only English attributions are recognised. French "Le … a écrit :", German "Am … schrieb …:", Spanish, Russian and similar are missed.
  - Outlook `divRplyFwdMsg` and "Begin forwarded message:" are only handled by a separate display-only C++ copy, `gui/ReceivedBody.h`. The engine and GUI stripping rules have drifted apart.
  - Bodies over 256 KiB after stripping are rejected, not trimmed (`google_store.rs:425`).
- **Automatic and backlog jobs already send one message.** The whole-thread path (up to 128 members, newest first, trimmed by a byte-level search that re-renders the request each step) only runs for a manual job with no target (`fm-core/src/local_model.rs:442`). The GUI no longer offers thread summaries.
- **Three schedulers share one inference slot:**
  - an in-memory interactive job map (32 entries, 16 admission slots)
  - the D097 arrivals watcher (rowid cursor, requeue key, 5 s / 30 s polling)
  - the D086 per-account backlog job (paged scan, one per account)

  All of them process oldest rowid first, so new mail waits behind any history. The watcher holds its cursor on runner failures, so one email that keeps timing out can stall it indefinitely. Saving settings cancels every job.
- **Pause is partial.** The uncommitted `Account.paused` flag stops only the watcher. Manual and backlog runs ignore it.
- **Waste in every request:**
  - three extra Ollama calls: tags + show before the request, tags after
  - a new HTTP client built for every call
  - a correction retry and fence stripping, which cannot help once the JSON schema is enforced
  - an 11.5 KB request cap, while the context window is 16k tokens
  - `num_predict` 4096 for about 100 output tokens
  - no `keep_alive` while draining a queue
- **Prompt is the whole raw request.** Settings store the full Ollama request JSON as `prompt`, including schema, options and five mandatory placeholders. The rules sit in the user message and the system message is boilerplate. `upgrade_settings` still migrates pre-reset settings, which D091/D095 made unreachable.
- **Output schema is larger than needed:** `sender_kind`, `category_ids`, `category_abstain`, `reason`, `summary`, `source_ids`. With a single message, `source_ids` means nothing. `category_abstain` duplicates an empty label. Label IDs are checked after the response instead of being limited by the schema.

The latest prototype (`fuckMail-7.html`, owned by the design work in progress) shows:
- **Local LLM settings:** provider URL + Detect, model + tok/s + Test, and **System prompt** / **User message** editors with `{{messages}}` and `{{labels}}` chips.
- **Per-account row:** "N of M", a processed/queued bar, pause/resume, and Catch up (Last week / month / year / All N unprocessed, with count + ETA).
- **Statuses:** No model · Off · N not processed · Paused · N queued · 42% · N queued · 20 min · Complete.
- **Prototype output:** `{summary, label, sender: human|bot}`.

## Target design

### 1. Input: one email, reply chain removed

- **One stripping implementation, in Rust:** `fm_mail::reply_contribution`. Move the extra rules from `gui/ReceivedBody.h` into it: `divRplyFwdMsg`, "Begin forwarded message:", Cc in header blocks. Add attribution patterns for the common Gmail/Outlook locales. The GUI shows engine-provided text and deletes its C++ copy. The existing "unknown formats stay unchanged" rule stays.
- The model receives the stored stripped body of the one target message. Headers: sender name/email, to, subject, date. Stored bodies are already stripped, so no work happens at request time.
- **Forwards keep the forwarded content.** For a forward, that is what the sender sent. Length caps still apply.
- **Length:** a fixed cap on the stripped body (proposal: 24,000 characters, head kept, `partial=true` beyond it). This replaces the byte-level search. The cap is a plain character count, independent of the prompt, so it cannot fail because of a long prompt.
- **Sync over 256 KiB:** store the first 256 KiB of stripped text marked truncated, instead of rejecting the message. Raw MIME is unchanged.
- Delete the whole-thread request path, the 128-member packing and the conversation-level result rows.

### 2. Prompt and request

- **Settings shape:** `{provider, endpoint, model, system, user, accounts}`.
  - `system` holds all behaviour rules. The default is the tightened current rules, in the mockup's layout.
  - `user` is minimal: `{{labels}}`, `{{messages}}` and optional user context.
  - Both are plain text. `{{messages}}` (the one email) is required; `{{labels}}` is optional, with the mockup's warning when missing.
  - `{{model}}`, `{{partial}}` and `{{source_ids}}` stop being user variables. Partial state goes into the email data.
- **Schema and options live in code, not settings:** `think:false`, temperature 0, `num_ctx` sized to the request (8,192 by default), `num_predict` 512, `keep_alive` "10m" while the queue has work.
- **Output schema:** `{summary, label, sender}`.
  - `label` is an enum of the account's label IDs plus `null`, so the model cannot invent one. The validator still checks it.
  - `sender` is `human|bot` on the wire, stored as `human|automated`.
  - Drop `source_ids`, `category_abstain` and `reason`.
- **Validation:** parse the JSON, check it against the captured label set and summary bounds, reject on failure, and record the failure. Drop the correction retry, fence/`<|eot|>` stripping and the legacy summary/classification projections.
- **One reused HTTP client.** Check the model when it is selected, and after a model-related error. Do not check before and after every request. For Ollama, store the digest in the result's provenance instead of comparing it twice per call.
- Delete `upgrade_settings`, `wrap_prompt`, the request-template renderer and the three legacy prompt files. One schema bump converts saved settings: take the existing system/user message texts from the saved request, and drop the rest.

### 3. One queue

There is no queue table. The queue is a query over received messages that have no result. A result from an older prompt or model counts as done (D111). It is ordered by class, then newest message first:

1. **Now:** a message the user asked to summarize or resummarize. This is an in-memory front slot; a new request replaces an unstarted one.
2. **New:** received mail dated at or after the account's `analyze_from`. This includes mail arriving mid-catch-up, which naturally sorts ahead of older mail.
3. **Catch up:** mail from the chosen range start up to `analyze_from`. Starting Catch up only moves the account's lower bound (`catch_up_from`). Already-processed mail is skipped because it has a result.

Rules:

- **One worker, one request at a time, across all accounts.** Round-robin between accounts within a class, so one large account cannot starve the others.
- **Pause** is a per-account flag, set by its own method (`assist_pause`), not a settings save. Saving settings no longer cancels unrelated work. A paused account skips New and Catch up. An explicit Summarize on one message still runs, because the user asked for it.
- **Failures:**
  - A failure in one email (bad JSON, timeout) records `{error, attempts}` on that message. The message leaves the queue after 2 attempts and shows Try again.
  - A runner failure (Ollama down, model missing) pauses the whole queue with a visible "Model unavailable" status. It re-checks every 30 s. It never skips or loses mail.
- **Lifetime:** the queue runs while the GUI is open, or while an explicit CLI `assist run` is running, as today. Nothing runs in the background after close. The state is derived from stored results, so reopening resumes where it stopped.
- **Progress** (`assist_progress`, extending the in-progress method):
  - Per account: processed, local, queued (by class), paused, state (No model / Off / Paused / Running / Model unavailable / Complete) and the current message.
  - ETA comes from measured tok/s (`eval_count` / `eval_duration`) and average output length.
  - One query per poll. No scan jobs.
- **Deleted:** watcher cursor + requeue keys, the backlog job + range scans, the 16/32 admission slots and job map (except the Now slot), the per-owner watch loops and `local_model_history_cutoff` (replaced by `analyze_from`).

Existing D090/D096 label authority, manual overrides, assisted Gmail label intents and Auto-read on arrivals stay as they are. "Arrival" becomes class 2 mail dated after the session started, as today.

### 4. Engine/CLI surface

| Method | CLI | Purpose |
| --- | --- | --- |
| `assist_summarize {account, message}` | `assist analyze <msg>` | Put a message in the Now slot |
| `assist_pause {account, paused}` | `assist pause/resume <account>` | Per-account pause |
| `assist_catch_up {account, range}` | `assist catch-up --range week\|month\|year\|all` | Move the lower bound |
| `assist_progress` | `assist progress` | Status for all accounts |
| `assist_test` | `assist test` | Mockup Test button: sample email → timing + JSON |
| — | `assist run` | Drain the queue in the foreground until Ctrl-C |

Retire `assist_start` for batches, `assist_batch_start`, `assist_backlog`, `assist_watch` and the thread target. The GUI uses the same methods.

## Coordination

- **Design work:** another agent is producing the next mockup from `fuckMail-7.html`. This plan owns the engine, store and CLI only. The GUI wiring maps to the states and copy above. Visuals, copy changes and Test/settings layout follow the design work. Summary box, list preview and Catch up menu appearance are unchanged here.
- **Uncommitted queue work:** the working tree has uncommitted work on `Account.paused`, `assist_progress`, `record_speed`, `AccountCard/AccountProgress.qml`, and relaxed placeholders in `render_request`. Start L1 only after that work is committed, and build on its pause/progress/speed pieces instead of redoing them.

## Packets

| Packet | Scope | Done when |
| --- | --- | --- |
| L0 | Consolidate quote stripping in `fm-mail`; add locales, Outlook HTML, forward headers; GUI uses engine text; >256 KiB bodies truncate | Fixture tests for each pattern; GUI C++ copy deleted; existing reader/search tests pass |
| L1 | Settings `{system,user}` + schema bump; code-owned schema/options; `{summary,label,sender}` with label enum; single client; drop retry/fence/legacy/upgrade code | Live check on the three installed models returns valid JSON; request ≤ one HTTP call per email; settings convert on the founder's profile |
| L2 | Single derived queue, priority classes, per-account pause, Catch up bound, failure attempts, runner-down pause; delete watcher/backlog/job map | Tests: new mail overtakes a running catch-up; pause stops all classes; Ollama stop → queue holds and resumes; a timing-out email leaves after 2 attempts |
| L3 | `assist_progress` states + ETA; CLI verbs; GUI wiring to the current account card and settings | GUI and CLI show the same counts; a GUI close stops work and a reopen resumes |

Measure: time per email and tok/s on `gemma4:e4b` and `qwen3.8-obliterated:27b`, before and after, on the founder's mailbox. Record them in [local assistance execution](../research/local-assistance-execution.md). Model quality claims remain separate.

## As built (D114)

Where the implementation differs from the design above:
- **GUI quote code stays.** `gui/ReceivedBody.h` splits rich HTML into reader sections, which overlaps the HTML reader work. Its extra rules, plus localized attributions and Outlook headers, were added to the engine's `reply_contribution`, which produces the stored text the model reads. Replacing the GUI copy is left to the HTML reader.
- **Pause uses the existing saved flag.** Saving settings no longer cancels work, so no separate `assist_pause` method was needed. CLI `assist pause`/`resume` save the flag.
- **Explicit requests use the inference seat directly.** Tokio's semaphore is first-come first-served, so a Summarize waits at most for the email in progress. There is no separate replaceable Now slot.
- **The Test button stays `assist_start {sample:true}`.** The old sample path was already right.
- **Added:** `local_model_failures` (schema36) for per-email attempts; when the queue is empty the worker skips rescans until the store revision changes or something wakes it; the model check is cached per settings and cleared after model errors.
- **`num_ctx` is fixed at 16,384**, not sized per request, because a changing value makes Ollama reload the model.

## Founder answers (D111)

1. **Output:** the mockup shape `{summary, label, sender: human|bot}`. `unknown` and `reason` are dropped. Stored as `human|automated` internally, so existing sender corrections and filters keep working.
2. **Summary length:** one sentence, at most 25 words.
3. **Prompt/model change:** keep old results, captioned with their model. Resummarize or Catch up redoes them on demand. No automatic requeue.
4. **Catch up order:** newest first. New arrivals still go ahead of it.

