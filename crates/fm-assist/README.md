# Fake assistance boundary — Phase 0

This crate tests the optional assistance boundary without a model or network dependency. Default/off starts no inference. Received and already-sent sources are account scoped; unpublished drafts are excluded. Context, queue and result sizes are bounded. Unknown fields, arbitrary destinations, action commands, duplicate/unmapped categories and stale source/policy/manual/provider revisions are rejected. Only configured category suggestions or explicitly enabled category application intent can leave the boundary. Summary coverage comes from the input builder, never from the fake output.

`Runner` exposes explicit cancellation. Ending one owner cancels its fake jobs and rejects late results while another owner's work remains valid. This proves the fake adapter contract only; no Ollama/vLLM runtime, locality, quality or server cancellation has been tested. There is no settings control that enables a real model.

Validation on the development Mac with Rust 1.88.0: `cargo test -p fm-assist --offline` (7 tests) and `cargo clippy -p fm-assist --all-targets --offline -- -D warnings` passed. Core integration must recheck the returned source stamps inside the same durable transaction that applies categories. This library alone does not prove that persistence path.
