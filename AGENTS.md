# Agent instructions

fuckMail is a personal desktop mail and calendar client. **Phase 0 synthetic foundation is complete on available hosts** under the founder's end-to-end goal. Rust engine/CLI, Qt Quick GUI, shared fixtures and native tooling pass Mac/Windows/Ubuntu CI and packaged runtime checks at `204fa62`. The Mac upgrade/refusal walkthrough also passes. Treat `docs/status.md` and recorded execution evidence as the source of truth; source files or an isolated compile do not prove the full milestone.

## Talk to the user

- Lead with the answer or next action. Keep it short.
- No walls of text. Extra technical detail only when asked, or when the user needs it to decide.
- Use bullets when they make the reply shorter.
- Speak plainly. No jargon, no process theater.

## Read first

1. [docs/status.md](docs/status.md) — what exists, what is next, Google-first readiness (currently 0/9)
2. [docs/north-star.md](docs/north-star.md) and [docs/product-scope.md](docs/product-scope.md) — what the product is and is not
3. [docs/decisions.md](docs/decisions.md) — accepted choices vs proposals vs open questions
4. The owning file under [docs/contracts/](docs/contracts/README.md) or [docs/research/](docs/research/README.md) for the area you are touching
5. [docs/development-plan.md](docs/development-plan.md) and the approaching [docs/plans/phase-2.md](docs/plans/phase-2.md) — sequence, start packet, dependencies and evidence gates
6. [docs/ui-ux.md](docs/ui-ux.md) for screen structure; [docs/README.md](docs/README.md) for where new writing belongs

Do not invent product behavior, stack locks or “done” features from memory. If a doc and a later decision disagree, the later decision wins; do not rewrite old decision entries.

## Current stage rules

- The founder authorized completed Phase 0/1 and active Phase 2 implementation on a new branch under D021. Follow docs/plans/phase-2.md and current status for implemented and verified scope. Tokens and real mailbox data stay out of Git.
- Recommended stack is **not locked**: Rust engine and CLI, C++20 / Qt Quick UI, SQLite/FTS5, private local IPC. Validate before treating it as decided.
- The second September 12 Flight Progress v2 update is the current Phase 1 design reference under D018; preserve it and support.js as reference assets only. Reimplement in native Qt/QML, including the three themes, General settings and mail refinements. D017 explicitly includes durable local New message, Reply all, Forward and Drafts alongside replies. D014 confirms read-on-open, Done=read+unstar, untimed Delay, combined full permitted calendar details and a browsable timeline only in Mail. Local settings preferences share the engine/CLI boundary; live OAuth/account removal/model downloads and prototype provider examples do not expand scope.
- Contracts are drafts. Do not invent placeholder APIs, fake benchmarks or pretend prose is a test suite.
- P1-10/Phase 1 are complete on available hosts: Windows runtime `25b8b1e` passes package/navigation/search/resize acceptance; final CI `34726506199` passes 9/9 and relocated packages on Mac/Windows/Ubuntu. Generated merge `8e9179b`, pushed `cb6489b` and merged main `885de67` have identical Git trees. Preserve this evidence and explicit physical/vault/live-provider/trusted-distribution limits.
- Readiness stays 0% until a required area is implemented and verified. Docs, screenshots and unrun tests do not count.
- D026/P2-08A's first generated-provider Windows navigation tier passes all four 100-observation routes during two-account Gmail acquisition at 84.59/82.38/91.30/77.03 ms p95, with 1,290 independently verified MIME/text/FTS messages, 141 part receipts and separate GUI/CLI lifetime checks. Platform/Quality 12 add the default-off isolated verification build; schema 11 and fixtures 0/3 stay unchanged. Scoped engine/process tests and both RSVP popup regressions pass. Preserve the 213.99 ms maximum reader sample, Qt shutdown warning and failed attempts. Calendar concurrency, large-mailbox/search/resize, packages and real accounts remain separate gates; this is not a new full-workspace/native run.
- D020 prepared Phase 2 on `codex/phase-2` from merged `885de67`; D021 authorizes implementation under the founder’s active end-to-end goal. D024/D025 advance Core 17/Gmail 7/Calendar 15/Mail UI 15/CLI 10 to schema 11 with native sync ownership/limits, capabilities, cached label setup, exact mail delivery and conditional Calendar recovery. Live fixture 0, synthetic fixture 3 and protocol 1 remain unchanged. P2-07A has 376 passing Rust workspace tests (five ignored) and strict lint. The first full run passed 11/12; final native rerun passes all ten GUI/lifecycle checks plus their build fixture, 11/11. All 271 source hashes match, with Rust unchanged from its passing run. Next: acquisition/navigation measurements, drafts/send, full Calendar workflows, reminders and packages. The Qt shutdown warning remains recorded. Sign-in alone never imports. Use `docs/research/phase-2-execution.md` for exact source hashes, passes, failures and limitations; historical checkpoints remain preserved. The plan covers personal Gmail and Workspace mail/Calendar together through Google APIs. The founder reports existing Cloud/OAuth/admin access and chose OS permissions plus verified disk encryption for the pilot, with app-level store encryption later. Inspect actual scopes/quotas and native disk/vault protection; settle retention/backup choices before real import. No live account or automated send/invitation is authorized by a planning document.

- Google APIs are the first product. Do not start IMAP/SMTP or external iCalendar subscription work until the founder has dogfooded working Google GUI and CLI workflows. Preserve their specs as deferred.
- Start with readable text and human composition. Best-effort received HTML display is later work, not a WebEngine/stack gate. Preserve MIME, HTML sources and attachments; text extraction and Google recurrence/invitations stay early.

## Product constraints agents must not drift

- Mail and calendar only. Separate accounts. Conversation-only browsing. No unified inbox.
- Unread = unreviewed. Read-on-open follows a deliberate user-open, not prefetch or pure CLI queries. Done clears unread+stars; Delay is an untimed star hold. Daily aim: zero unread and zero starred through real work. Calendar combines permitted details; mail remains separate.
- Humans write email. Only the user chooses Done/Delayed. Optional local models tag human correspondence versus automation/user-defined noise and category/scope, and may summarize; they never complete, delay or mark mail reviewed.
- GUI close ends GUI work. CLI is a second interface with its own explicit lifetime. Build a small real CLI and GUI together, expanding shared workflows incrementally. No default always-on service.
- Windows, macOS and Linux (Ubuntu, Arch, Omarchy). One shared visual design. QML syntax is accepted; no Node.js or separate JS/TS compilation/bundling pipeline.
- Out of scope: tasks, contacts app, dashboards, AI writing, assistant chat, semantic search, editable CalDAV, app-operated remote services.

## How to change the repo

Follow [docs/README.md](docs/README.md):

1. Evidence and comparisons go in a scoped `docs/research/` file.
2. Decisions and Q&A are appended to `docs/decisions.md` (new ID, ISO timestamp). Never edit or delete old entries.
3. Living behavior belongs in the owning `docs/contracts/` file. Bump its revision and name affected consumers.
4. Update the phased plan when sequence or dependencies change; expand only the approaching phase.
5. Update `docs/status.md` and the root README when scope, active work or readiness changes.

Do not create empty speculative docs. Prefer editing the owning file over copying it. When code exists, reuse existing functions instead of duplicating them.

Never commit secrets, real mailbox dumps, tokens or auth material.

## When implementation is requested

- Start from the verified foundation in status/evidence and the prepared Phase 2 plan. Phase 0 is complete on available hosts; full feature areas, physical other-host input/keyrings, live Google workflows and trusted distribution remain unverified. Inspect actual tools and host access before installing or assuming they exist.
- Name the packet, scoped contract revision, fixture revision and what evidence will count as done before writing code. Feasibility can begin against Draft contracts; production consumers need the relevant shared boundary agreed. Do not mark a whole feature Ready from one passing slice.
- Keep detailed planning focused on the next phase. Preserve completed evidence and append pivots; missing credentials, final mockups, runners or remote test hosts block their affected gate, not unrelated authorized work.
- Keep engine/CLI and UI on the agreed boundary. GUI and CLI share one store and one command/query model.
- Integrate small end-to-end slices. Do not build a second schema to unblock a module.
- Record measured results in the owning research file and quality contract, then update status.
