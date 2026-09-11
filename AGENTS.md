# Agent instructions

fuckMail is a personal desktop mail and calendar client. This repo is still in the **pre-build / documentation stage**. There is no application code, build, installer or measured result yet. Treat `docs/` as the source of truth and progress log.

## Talk to the user

- Lead with the answer or next action. Keep it short.
- No walls of text. Extra technical detail only when asked, or when the user needs it to decide.
- Use bullets when they make the reply shorter.
- Speak plainly. No jargon, no process theater.

## Read first

1. [docs/status.md](docs/status.md) — what exists, what is next, readiness (currently 0/10)
2. [docs/north-star.md](docs/north-star.md) and [docs/product-scope.md](docs/product-scope.md) — what the product is and is not
3. [docs/decisions.md](docs/decisions.md) — accepted choices vs proposals vs open questions
4. The owning file under [docs/contracts/](docs/contracts/README.md) or [docs/research/](docs/research/README.md) for the area you are touching
5. [docs/ui-ux.md](docs/ui-ux.md) for screen structure; [docs/README.md](docs/README.md) for where new writing belongs

Do not invent product behavior, stack locks or “done” features from memory. If a doc and a later decision disagree, the later decision wins; do not rewrite old decision entries.

## Current stage rules

- Planning only unless the user clearly asks to implement or prototype.
- Recommended stack is **not locked**: Rust engine and CLI, C++20 / Qt Quick UI, SQLite/FTS5, private local IPC. Validate before treating it as decided.
- Mockups are visual direction, not final UI. Bottom calendar strip stays; the sidebar agenda in mockup 3 does not replace it.
- Contracts are drafts. Do not invent placeholder APIs, fake benchmarks or pretend prose is a test suite.
- Readiness stays 0% until a required area is implemented and verified. Docs, screenshots and unrun tests do not count.
- Do not start a feature workstream, pick versions or connect real mailboxes unless asked.

## Product constraints agents must not drift

- Mail and calendar only. Separate accounts. Conversation-only browsing. No unified inbox.
- Unread = unreviewed. Star = action left for later. Daily aim: zero unread and zero starred through real work, not auto-clear.
- Humans write email. Optional local models may classify and summarize only.
- GUI close ends GUI work. CLI is a full second interface with its own explicit lifetime. No default always-on service.
- Windows, macOS and Linux (Ubuntu, Arch, Omarchy). One shared visual design. No JS/TS app or npm/Node product pipeline.
- Out of scope: tasks, contacts app, dashboards, AI writing, assistant chat, semantic search, editable CalDAV, app-operated remote services.

## How to change the repo

Follow [docs/README.md](docs/README.md):

1. Evidence and comparisons go in a scoped `docs/research/` file.
2. Decisions and Q&A are appended to `docs/decisions.md` (new ID, ISO timestamp). Never edit or delete old entries.
3. Living behavior belongs in the owning `docs/contracts/` file. Bump its revision and name affected consumers.
4. Update `docs/status.md` and the root README when scope, active work or readiness changes.

Do not create empty speculative docs. Prefer editing the owning file over copying it. When code exists, reuse existing functions instead of duplicating them.

Never commit secrets, real mailbox dumps, tokens or auth material.

## When implementation is requested

- Name the contract, revision and what evidence will count as done before writing code.
- Keep engine/CLI and UI on the agreed boundary. GUI and CLI share one store and one command/query model.
- Integrate small end-to-end slices. Do not build a second schema to unblock a module.
- Record measured results in the owning research file and quality contract, then update status.
