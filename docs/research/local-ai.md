# Local classification and summarization: Ollama and vLLM

**D052 (2026-09-14): Ollama support is implemented with native/CLI controls.** [Execution results](local-assistance-execution.md#d052--p2-llm-mac-implementation-2026-09-14) record local model selection, structured samples, actual cancellation and quality misses. vLLM findings below remain research for later. [Local assistance revision 6](../contracts/local-ai.md) owns behavior; historical inspection below retains its original scope.

## September 14 Ollama reference inspection (D051)

Read-only inspection of this Mac at approximately 2026-09-14T22:09Z, separate from the running UI/mail work:

- `GET http://127.0.0.1:11434/api/version` returns **0.34.0**. `GET /api/tags` returns **20 model entries**, including `gemma3n:e4b`, `gpt-oss:20b`, and `kimi-k2.5:cloud`. The inventory contains a cloud entry, so counting tags as usable local models would be wrong. `/usr/local/bin/ollama` exists; the app adapter must not require that executable.
- `POST /api/show` for `gemma3n:e4b` returns `completion` capability and no `remote_model`/`remote_host` fields. This is a candidate for a small synthetic qualification run, not proof of local execution, correct structured results or acceptable quality.
- The same metadata request for `gpt-oss:20b` returns **HTTP 500**. The request for the cloud entry returns **HTTP 410** with a retirement error. Preserve both as reference compatibility findings. Do not let one broken entry suppress the rest of the picker; do not infer cloud eligibility by successfully listing its name. No generation, model download, service configuration change or real mailbox inference was performed.
- The first sandboxed loopback request could not connect; the permitted direct local request succeeded. This was a tool-environment restriction, not evidence that the service was stopped.

Official documentation checked September 14: Ollama supplies a [model-list endpoint](https://docs.ollama.com/api/tags), [chat API with JSON/schema output](https://docs.ollama.com/api/chat), and [structured output guidance](https://docs.ollama.com/capabilities/structured-outputs). Native Rust HTTP plus Core validation fits the existing app; no SDK/runtime dependency is needed. Schema conformance alone does not establish faithful summaries or label quality.

The [Ollama FAQ](https://docs.ollama.com/faq) documents cloud disabling through `OLLAMA_NO_CLOUD=1` or `disable_ollama_cloud`, and explains model residency. Localhost alone cannot prove local processing. Qualify actual selected weights/routing and cloud-disabled execution before mail inference, without silently changing the shared service. Cancellation of this app's request must be checked against server-side work; an idle loaded model or a disconnected client alone proves neither success nor failure. These gates remain untested.

**Classifier intent clarified by D009 (2026-09-11):** tag human correspondence versus bots/lists/user-defined noise and the configured account/category/scope. Only the user chooses Done/Delayed. A classifier tag never marks mail reviewed or changes read/star state, and a personal spam/noise category does not authorize Gmail system Spam/Trash routing. Gmail labels are the only integration; IMAP filing is out of scope.

Sources checked 2026-09-11; reorganized 2026-09-11. Documentation/source evidence and proposals only; no app prototype, live account flow or benchmark has run. Recheck version-sensitive claims before implementation.

Build boundary: [Local assistance contract](../contracts/local-ai.md).

## Current platform and caller scope

Linux is now required alongside Windows/macOS; extend the runner test matrix to the supported Ubuntu/Arch/Omarchy environments. This does not require every runner/model on every OS. The full CLI can invoke allowed local assistance independently of the GUI; built-in jobs remain local and bounded. External agents using CLI output are separate user-controlled callers, not a built-in cloud fallback.

## Optional local classification and summaries

The founder now explicitly includes optional local-model classification and summarization while excluding all AI email authoring. This supersedes the original blanket MVP AI exclusion. The product contracts are in [triage](../contracts/triage.md) and [local assistance](../contracts/local-ai.md); the following are integration findings, not model-quality results.

Ollama supports JSON-schema-constrained responses through its `format` field and structured responses through its compatible API. This suits category IDs and summary fields without a JS application dependency. Validate results in the native core; a valid schema does not establish a correct category or faithful summary. [Ollama structured outputs](https://docs.ollama.com/capabilities/structured-outputs)

Ollama documents Windows/macOS operation and a configuration switch to disable cloud features (`OLLAMA_NO_CLOUD=1` or `disable_ollama_cloud`). A localhost address alone does not prove local inference if the runner can route requests to cloud models. Recommend explicitly configured local execution, locally available weights and no hosted fallback; do not silently modify a user's runner configuration or download models. [Ollama FAQ](https://docs.ollama.com/faq)

vLLM offers compatible HTTP serving and structured output facilities. Its model/chat-template and structured-output configuration must be tested against the pinned runner/model combination. Use a native HTTP adapter rather than adding a Python or JS SDK to the desktop app. The separately installed model server has its own dependencies. [vLLM serving](https://docs.vllm.ai/en/latest/serving/openai_compatible_server/), [structured outputs](https://docs.vllm.ai/en/latest/features/structured_outputs/)

vLLM's GPU installation documentation specifies Linux and describes WSL as a Windows route. Do not promise equivalent native Windows/macOS runner installation. Proposed evaluation: Ollama as the first desktop setup to validate across the required OSes, plus the vLLM adapter on supported same-machine configurations; pin the Linux distro/GPU environment explicitly. The app always operates manually regardless of runner availability. [vLLM platform requirements](https://docs.vllm.ai/en/latest/getting_started/installation/gpu/)

Engineering implications: inference is optional, bounded and separate from indexing; record model/content/category versions, preserve manual corrections and test cancellation against the actual runner. Summaries are derived reader content and never replace original-text search. Measure inference RAM/VRAM and cold/warm latency separately from the off-mode app budget. No particular model, throughput, confidence threshold or summary quality has been accepted or measured.

## Phase 0 staging

The Phase 0 plan separated fake-runner boundary tests from live runner and quality evidence. First exercise Off, unavailable/invalid/abstaining/late output, mapping changes and newer manual corrections with synthetic received mail. Core must reject stale or disallowed results before any provider operation; summaries retain source IDs/coverage and cannot touch unpublished drafts.

When a supported same-machine setup is available, record the runner/model/version, local-only configuration, weights availability and one bounded classification/summary result. Structured-output support does not prove accuracy or local execution. Current primary docs continue to describe schema output for Ollama and vLLM; validate our HTTP adapter against the actual configuration. [Ollama structured outputs](https://docs.ollama.com/capabilities/structured-outputs), [vLLM structured outputs](https://docs.vllm.ai/en/latest/features/structured_outputs/).

Observe server-side cancellation and resource use after ending the owning session; a client disconnect is not a pass by itself. If a supported cancellation path is unavailable, record that runner as unverified and resolve the contract conflict before enabling it. Do not kill an unrelated/shared runner or silently relax close behavior. Missing runner/GPU access does not block manual mail or the core fixture slice. Full labeled category/summary evaluation, both supported adapters and agreed thresholds remain later feature gates; no model download, live inference or quality measurement occurred here.

## Next experiment

P0-08: Evaluate the same synthetic classification/summary fixtures through local Ollama and a supported local vLLM setup. Record local-only configuration, model/schema compatibility, quality, cold/warm latency, memory/VRAM and cancellation. Do not make a vLLM installation a prerequisite for a working Windows/macOS app; record supported runner/platform combinations explicitly.
