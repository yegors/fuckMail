# Agent instructions

fuckMail is a personal desktop mail and calendar client for Google accounts: a Rust engine
and `fm` CLI, a C++20/Qt Quick GUI, and SQLite/FTS5 storage behind private local IPC.
[CHANGELOG.md](CHANGELOG.md) records what shipped and [docs/decisions.md](docs/decisions.md)
records why. Source files or an isolated compile don't prove a feature is done.

If `AGENTS.local.md` exists (it's ignored by Git), read it too: it holds host- and
account-specific rules.

## Talk to the user

- Lead with the answer or next action. Keep it short.
- No walls of text. Add technical detail only when asked, or when the user needs it to decide.
- Use bullets when they make the reply shorter.
- Speak plainly. No jargon, no process theater.

## Read first

1. [CHANGELOG.md](CHANGELOG.md): what shipped recently (newest first)
2. [docs/north-star.md](docs/north-star.md) and [docs/product-scope.md](docs/product-scope.md): what the product is and is not
3. [docs/decisions.md](docs/decisions.md): accepted choices vs proposals vs open questions
4. The owning file under [docs/contracts/](docs/contracts/README.md) or [docs/research/](docs/research/README.md) for the area you are touching
5. [docs/architecture.md](docs/architecture.md) for how the processes, IPC, storage and sync fit together; [docs/development-plan.md](docs/development-plan.md) for sequence
6. [docs/ui-ux.md](docs/ui-ux.md) for screen structure; [docs/README.md](docs/README.md) for where new writing belongs
7. [crates/README.md](crates/README.md) and [gui/README.md](gui/README.md) for the source layout

Don't invent product behaviour, stack locks or "done" features from memory. If a doc and a later decision disagree, the later decision wins. Don't rewrite old decision entries.

## Standing rules

- **Real data.** Never automate sending mail or invitations to real people, and never put raw mailbox data, tokens or OAuth client files in Git.
- **Repository hygiene.** Generated evidence (logs, manifests, run archives, profiles) stays out of Git, in ignored local or CI artifact storage. Keep authored reports, supplied design assets and runtime fixtures. Run `python3 scripts/check_repo_hygiene.py` (`python` on Windows) before committing.
- **Gmail only.** The product is Gmail and Google Workspace through Google's APIs. IMAP/SMTP, other providers and external calendar subscriptions are out of scope; people who want them can fork. Bug fixes are welcome.
- **Stale docs.** When a plan is finished or a record is superseded, delete it rather than letting it drift; Git history keeps it.
- **Done means verified.** A feature is done only when it is implemented and its tests pass; docs, screenshots and unrun tests don't count. Don't invent placeholder APIs or fake benchmarks, or treat prose as a test suite.
- **Design reference.** The Flight Progress v2 exports under `docs/research/assets/` are reference assets only. Reimplement them in native QML, including the themes.
- **Licensing.** The project is MIT-licensed. When you bundle or vendor a new third-party component, add it to [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and make sure packages install its license text.

## Product constraints agents must not drift

- Mail and calendar only. Separate accounts. Conversation-only browsing. No unified inbox.
- Unread = unreviewed. Read-on-open follows a deliberate user open, not prefetch or pure CLI queries. Done clears unread and stars; Delay is an untimed star hold. The daily aim is zero unread and zero starred, reached through real work. Calendar combines permitted details; mail stays separate.
- Humans write email. Only the user chooses Done or Delayed. Optional local models tag human correspondence versus automation or user-defined noise, assign category and scope, and may summarize. They never complete or delay mail. They mark it read or move it to Spam only through per-label rules the user configured.
- Closing the GUI ends GUI work. The CLI is a second interface with its own explicit lifetime. Build a small real CLI and GUI together, expanding shared workflows incrementally. No default always-on service.
- Windows, macOS and Linux (Ubuntu, Arch, Omarchy). One shared visual design. QML is fine; no Node.js or separate JS/TS compilation or bundling pipeline.
- Out of scope: tasks, contacts app, dashboards, AI writing, assistant chat, semantic search, editable CalDAV, app-operated remote services.

## Code conventions

- Match the surrounding code: naming, idiom and comment density.
- Every Rust module starts with a `//!` doc. Every C++, QML and script file starts with a short comment saying what it owns. Unsafe blocks carry a `// SAFETY:` comment.
- Keep process identifiers out of code. Decision, packet and phase IDs (`D0xx`, `P2-05`, "Phase 1") and contract revision tags ("Search 10") belong in docs. Comments explain the code; identifiers describe what things do.
- Before committing, run `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings`; CI enforces both.
- Reuse existing functions instead of duplicating them. Keep engine/CLI and UI on the agreed boundary: the GUI and CLI share one store and one command/query model, and only the engine writes the database.

## How to change the docs

Follow [docs/README.md](docs/README.md):

1. Evidence and comparisons go in a scoped `docs/research/` file.
2. Append decisions and Q&A to `docs/decisions.md`, with a new ID and an ISO timestamp. Never edit or delete old entries.
3. Living behaviour belongs in the owning `docs/contracts/` file. Bump its revision and name affected consumers.
4. Update the phased plan when sequence or dependencies change; expand only the approaching phase.
5. Add a terse bullet under today's date in `CHANGELOG.md` for user-visible changes (no decision IDs or test counts). Update the root README when scope changes.

Don't create empty speculative docs. Edit the owning file instead of copying it. Never commit secrets, real mailbox dumps, tokens or auth material.

## When implementation is requested

- Start from the current code, `CHANGELOG.md` and `docs/decisions.md`. Inspect the actual tools and host access before installing anything or assuming it exists.
- Name the scoped contract revision, the fixture revision and what evidence will count as done before writing code. Don't mark a whole feature Ready from one passing slice.
- Integrate small end-to-end slices. Don't build a second schema to unblock a module.
- Record measured results in the owning research file and the quality contract, then add a CHANGELOG bullet. Missing credentials, mockups or remote hosts block only the gate they affect, not unrelated authorized work.
