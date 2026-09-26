# Crates

The Rust side of fuckMail: a local engine process, the `fm` command-line
client, and the libraries they share. The Qt GUI in `gui/` is a separate C++
client that speaks the same protocol to the same engine.

| Crate | Purpose |
|---|---|
| `fm-engine` | Binary `fm-engine`: the local background process both clients talk to. Owns the store writer and runs Google sync, sending, remote content and local-model work. |
| `fm-cli` | Binary `fm`: the command-line client. Every command is an engine request. |
| `fm-core` | The SQLite store and the single-writer logic behind every request: mail, drafts, calendar cache, preferences and accounts. |
| `fm-ipc` | Local transport: JSON frames over a per-user Unix socket or named pipe, engine discovery and on-demand launch. |
| `fm-google` | Google OAuth sign-in and token refresh, the Gmail API client, quota scheduling and OS keychain storage. |
| `fm-calendar` | Google Calendar event model: normalization, recurrence, permission-filtered projection, write requests and the Calendar API client. Vendors a patched `rrule`. |
| `fm-mail` | MIME parsing and construction, plain-text extraction, and sanitized HTML and image views. Pure transforms. |
| `fm-assist` | Optional local-model settings, prompts, request bodies and reply validation. |

## Dependencies

Each arrow points from a crate to the workspace crates it depends on.

```text
fm-cli      -> fm-ipc
fm-engine   -> fm-core, fm-ipc, fm-google, fm-calendar, fm-mail, fm-assist
fm-core     -> fm-calendar, fm-google, fm-mail, fm-assist
fm-calendar -> fm-google
```

`fm-ipc`, `fm-google`, `fm-mail` and `fm-assist` depend on no other workspace
crate. `fm-cli` does not link `fm-core`: only the engine opens the database.

## How the processes fit

A client finds the engine for a profile through `<profile>/engine.json`, or
starts `fm-engine` if none is running, and authenticates with the token in
that file. The engine exits shortly after its last client disconnects; there
is no always-on service.

## Build and test

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The `verification-google` feature of `fm-engine` and `fm-cli` adds an
in-memory Google fixture for developer verification; normal builds leave it
out. See [docs/build-development.md](../docs/build-development.md) for the
full toolchain and GUI build.
