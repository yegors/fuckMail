# Local classification and summarization: Ollama and vLLM

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

The [Phase 0 plan](../plans/phase-0.md) separates fake-runner boundary tests from live runner and quality evidence. First exercise Off, unavailable/invalid/abstaining/late output, mapping changes and newer manual corrections with synthetic received mail. Core must reject stale or disallowed results before any provider operation; summaries retain source IDs/coverage and cannot touch unpublished drafts.

When a supported same-machine setup is available, record the runner/model/version, local-only configuration, weights availability and one bounded classification/summary result. Structured-output support does not prove accuracy or local execution. Current primary docs continue to describe schema output for Ollama and vLLM; validate our HTTP adapter against the actual configuration. [Ollama structured outputs](https://docs.ollama.com/capabilities/structured-outputs), [vLLM structured outputs](https://docs.vllm.ai/en/latest/features/structured_outputs/).

Observe server-side cancellation and resource use after ending the owning session; a client disconnect is not a pass by itself. If a supported cancellation path is unavailable, record that runner as unverified and resolve the contract conflict before enabling it. Do not kill an unrelated/shared runner or silently relax close behavior. Missing runner/GPU access does not block manual mail or the core fixture slice. Full labeled category/summary evaluation, both supported adapters and agreed thresholds remain later feature gates; no model download, live inference or quality measurement occurred here.

## Next experiment

P0-08: Evaluate the same synthetic classification/summary fixtures through local Ollama and a supported local vLLM setup. Record local-only configuration, model/schema compatibility, quality, cold/warm latency, memory/VRAM and cancellation. Do not make a vLLM installation a prerequisite for a working Windows/macOS app; record supported runner/platform combinations explicitly.
