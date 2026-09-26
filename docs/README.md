# Documentation

fuckMail is a desktop mail and calendar client for Gmail and Google Workspace, built on Google's APIs. A Rust engine owns a local SQLite store; a C++/Qt Quick GUI and the `fm` CLI both talk to it. Accounts stay separate, mail is browsed by conversation, and triage uses two flags: Unread means not reviewed, Starred means action still pending.

## Using the app

| Document | What it covers |
| --- | --- |
| [User guide](user-guide.md) | Accounts, triage, reading and writing mail, calendar, search, settings, local AI |
| [CLI reference](cli.md) | The `fm` command: output, exit codes, every command group |
| [Troubleshooting](troubleshooting.md) | Sign-in, sync, the engine, profiles and where logs live |
| [Privacy](../PRIVACY.md) | What is stored, what leaves your machine and when |
| [Security](../SECURITY.md) | Reporting a vulnerability, credential storage, attack surface |

## Working on the code

| Document | What it covers |
| --- | --- |
| [Contributing](../CONTRIBUTING.md) | What fixes are welcome, checks to run, what never gets committed |
| [Architecture](architecture.md) | Processes, crates, the store, sync, and how a request flows |
| [Building and development](build-development.md) | Toolchain, build, test and package on each OS; Google sign-in setup |
| [Local protocol](local-protocol.md) | The engine request boundary shared by the GUI and CLI |
| [Rust crates](../crates/README.md) · [GUI](../gui/README.md) · [Fixtures](../fixtures/README.md) | What lives where in the source tree |
| [Changelog](../CHANGELOG.md) | Notable changes by day |

## Specifications

| Document | What it covers |
| --- | --- |
| [North star](north-star.md) | The experience the product aims for, and the scope filter |
| [Product scope](product-scope.md) | Requirements, workflows and exclusions |
| [Visual design and UX](ui-ux.md) | Screen structure, layout, state meanings |
| [Contracts](contracts/README.md) | Living behavior specs per area: storage, Gmail, calendar, mail UI, search, triage, local AI, CLI, platform, quality |

## Design history

| Document | What it covers |
| --- | --- |
| [Decision log](decisions.md) | Append-only, timestamped decisions and Q&A |
| [Research](research/README.md) | Evidence, alternatives and experiment reports by subject |
| [Development plan](development-plan.md) · [Phase 2 plan](plans/phase-2.md) | Build sequence and the Google integration packets |
| [Phase 2 execution log](research/phase-2-execution.md) | Detailed record of the Google integration test runs |
| [Stack rationale](research/desktop-stack.md) | Why a Rust engine with a C++/Qt Quick UI, and what else was considered |
| [Design records](research/prototype-september-23.md) | Design exports and how the native UI follows them ([earlier](research/prototype-september-22.md), [original](research/flight-progress-v2.md), [mockups](research/design-references.md)) |

## Where writing goes

- **User-facing behavior** that changed: update the user guide or CLI reference, and add a line to the changelog.
- **How an area must behave**: the owning file under `contracts/`. Contracts describe what is built and what is still open; they are not promises.
- **Evidence, comparisons, measurements**: a scoped file under `research/`, with dates and sources, keeping observation separate from inference.
- **Decisions**: append an entry to `decisions.md` with a new ID and an ISO 8601 timestamp. Never edit or remove earlier entries; supersede them with a new one.
- **Finished plans**: delete them once their work lands; the decision log and changelog keep the history.

Keep generated logs, traces, screenshots of test runs and bulk manifests out of Git. Run `python scripts/check_repo_hygiene.py` before committing. Use kebab-case Markdown filenames, and don't create placeholder documents.
