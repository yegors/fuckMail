# Contributing

Thanks for taking a look. fuckMail does one thing: Gmail and Google Workspace mail and
calendar, through Google's APIs, in a fast native client. It is deliberately narrow, and
it isn't trying to cover every use case.

- **Found a bug? Send a fix.** A pull request with a test that fails before and passes
  after is the quickest way in. An issue with clear reproduction steps helps too.
- **Want a new feature, another provider, IMAP/SMTP or Outlook?** Fork it. The code is
  MIT-licensed so you can take it in your own direction. This repository will stay
  focused on Gmail.

## What the app is

- **Mail and calendar only**, for Google accounts (Gmail and Google Workspace) through
  Google's APIs.
- **Separate accounts, conversations only.** No unified inbox.
- **Unread means unreviewed.** Only the user marks mail Done or Delayed.
- **People write their own email.** Optional local models may tag or summarize mail; they
  never write, send, complete or hide it.
- **The GUI and the CLI are equal clients** of one local engine, with one store and one
  command model. There is no always-on background service.
- **One design on Windows, macOS and Linux**, in native Qt Quick. No Node.js or JavaScript
  build pipeline.
- **Out of scope:** tasks, a contacts app, dashboards, AI writing, assistant chat, semantic
  search and hosted services.

[docs/north-star.md](docs/north-star.md) and [docs/product-scope.md](docs/product-scope.md)
explain the reasoning.

## Getting set up

Follow [Build and run](README.md#build-and-run) in the README. `bootstrap` puts the pinned
Rust, Qt, CMake and Ninja into `.tools/` inside the checkout. Use `--demo-profile` to work
with sample data. Connecting a real account needs your own Google Cloud client; see
[Google sign-in setup](docs/build-development.md#google-sign-in-setup).

Where things live:

| Path | Contents |
| --- | --- |
| [crates/](crates/README.md) | Rust engine, store, Google sync, CLI and shared libraries |
| [gui/](gui/README.md) | Qt Quick desktop app (QML views, C++ backend, tests) |
| [fixtures/](fixtures/README.md) | Synthetic MIME messages and demo data |
| [scripts/](scripts/) | Developer workflow (`dev.py`), bootstrap, packaging and checks |
| [docs/](docs/README.md) | Product scope, design, specifications and the development log |

## Before you send a fix

Run these from the repository root. On Windows, use `./scripts/dev.ps1` and `python`.

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
scripts/dev.sh test --preset release
python3 scripts/check_repo_hygiene.py
```

CI runs the same checks, plus packaging, on macOS, Windows and Ubuntu.

- **Match the surrounding code.** Every Rust module starts with a `//!` doc. Every C++ and
  QML file starts with a short comment saying what it owns. Explain *why* in comments,
  not *what*.
- **Keep the engine as the only database writer.** The GUI and CLI go through the local
  protocol ([docs/local-protocol.md](docs/local-protocol.md)).
- **Update the documentation for behaviour changes.** Specifications live in
  [docs/contracts/](docs/contracts/README.md).
- **Add or adjust tests with the change.** Rust tests run under `cargo test`; GUI tests are
  registered with CTest in [gui/CMakeLists.txt](gui/CMakeLists.txt).

## Never commit

- OAuth client files, access or refresh tokens, or any other credentials;
- real mailbox or calendar data, profiles or screenshots of them (use the synthetic fixtures);
- build output, logs or other generated artifacts. `check_repo_hygiene.py` catches most of these.

## Conduct

Be kind and assume good intent. Harassment and personal attacks aren't welcome in issues,
pull requests or anywhere else in the project.

By contributing, you agree that your contributions are licensed under the
[MIT License](LICENSE).
