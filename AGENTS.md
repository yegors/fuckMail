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

- D029–D031 record a P2-05B2 functional interim checkpoint from accepted B1 `09eadfa`, not full B2 acceptance. Current boundaries are Core 20/Gmail 9/Mail UI 18/CLI 13/Quality 16/schema 14; Calendar 15/Platform 12/Search 5, fixtures 0/3 and protocol 1 stay unchanged. Workspace02 passes 469 tests with five ignored; the isolated feature build passes 132 unit and three process tests; CLI03 verifies four generated Sends and nine Google operations. Native04 passes 15/15; integration takes 150.731 seconds under the 180-second aggregate watchdog, retaining the primary 120-second and child 45-second bounds. Navigation01 FAILS the unchanged 100-ms p95 target at 158.3543/136.6387/177.9872/86.3356 ms for click/Tab/Shift+Tab/reader; all 400 exact paints, 1,640 sources and 176 parts pass. Preserve the [interim source/evidence archive](docs/research/assets/phase-2/p2-05b2-interim/logs-manifest.json), including failures. The founder says “Keep working; measure later”: continue B3 remote Drafts and remaining Calendar invitation/recurrence/reminder work after its next boundary is agreed; do not rerun performance, close other apps or change host settings merely to obtain a pass. No automatic sender, unknown resend, hidden draft recreation, live-account action or external send is authorized. Readiness remains 0/9; performance, complete remote Drafts/Calendar, current packages and live gates stay open.

- D028/P2-05B1 is accepted on Windows from A2 `cdf5d40`: locally authored Google draft create/update/readback, independent local autosave, acknowledged-ID/RAW baselines, explicit conflicts/unknown recovery and per-draft failure fairness. Core 19/Gmail 8/Mail UI 17/CLI 12/Quality 14/schema 13 preserve fixtures 0/3 and protocol 1. Existing drafts/capture journal/blobs retain exact synthetic receipts. Workspace: 438 passed, five ignored; isolated engine: 114 passed; final Release CLI/provider, native 14/14 and navigation 01 pass. Four-route p95 is 78.5675/77.5055/76.7026/38.1486 ms, with 400 exact paints, 1,250 sources and 137 parts. Preserve failures and native delegate diagnostics. P2-05B2 explicit send, complete remote-only Drafts and remaining Calendar/live gates are required next; no real account/send/invitation is verified. Use execution evidence for source/binary/host limits.

- The founder authorized completed Phase 0/1 and active Phase 2 implementation on a new branch under D021. Follow docs/plans/phase-2.md and current status for implemented and verified scope. Tokens and real mailbox data stay out of Git.
- Recommended stack is **not locked**: Rust engine and CLI, C++20 / Qt Quick UI, SQLite/FTS5, private local IPC. Validate before treating it as decided.
- The second September 12 Flight Progress v2 update is the current Phase 1 design reference under D018; preserve it and support.js as reference assets only. Reimplement in native Qt/QML, including the three themes, General settings and mail refinements. D017 explicitly includes durable local New message, Reply all, Forward and Drafts alongside replies. D014 confirms read-on-open, Done=read+unstar, untimed Delay, combined full permitted calendar details and a browsable timeline only in Mail. Local settings preferences share the engine/CLI boundary; live OAuth/account removal/model downloads and prototype provider examples do not expand scope.
- Contracts are drafts. Do not invent placeholder APIs, fake benchmarks or pretend prose is a test suite.
- P1-10/Phase 1 are complete on available hosts: Windows runtime `25b8b1e` passes package/navigation/search/resize acceptance; final CI `34726506199` passes 9/9 and relocated packages on Mac/Windows/Ubuntu. Generated merge `8e9179b`, pushed `cb6489b` and merged main `885de67` have identical Git trees. Preserve this evidence and explicit physical/vault/live-provider/trusted-distribution limits.
- Readiness stays 0% until a required area is implemented and verified. Docs, screenshots and unrun tests do not count.
- D026/P2-08A's first generated-provider Windows navigation tier passes all four 100-observation routes during two-account Gmail acquisition at 84.59/82.38/91.30/77.03 ms p95, with 1,290 independently verified MIME/text/FTS messages, 141 part receipts and separate GUI/CLI lifetime checks. Platform/Quality 12 add the default-off isolated verification build; schema 11 and fixtures 0/3 stay unchanged. Scoped engine/process tests and both RSVP popup regressions pass. Preserve the 213.99 ms maximum reader sample, Qt shutdown warning and failed attempts. Calendar concurrency, large-mailbox/search/resize, packages and real accounts remain separate gates; this is not a new full-workspace/native run.
- D027/P2-05A2 is accepted on Windows after tested `55ef82c`: local-file attachments, off-writer draft save/capture preparation and stable native account/list delegates. Core 18/Mail UI 16/CLI 11/Quality 13 use schema 12, preserving fixtures 0/3 and protocol 1. Separate Rust workspace: 397 passed, five ignored. Final normal-development native CTests: 13/13 in 179.39 seconds (twelve native tests plus Rust test-build; no workspace rerun), including press stability across two CLI account refreshes. Navigation05 passes four 100-observation routes at 75.9224/76.4642/80.6248/43.3367 ms p95, with 1,250 MIME/text/FTS messages, 137 parts and all source/ownership checks. The immutable A2 archive contains 138 artifacts, 289 source hashes and 258 runtime sources rechecked. Preserve failed navigation01/03/04 and earlier warnings; final navigation has no shutdown warning. Whole readiness stays 0/9; packages/current CI, Calendar concurrency, larger-scale, live accounts and physical input remain separate gates. Preserve exact replay before file access and worker permits through completed cancellation; Google drafts/send remains next.
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
