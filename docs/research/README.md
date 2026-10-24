# Research

Evidence, comparisons and experiment reports, grouped by subject. Each file records dates and sources and separates what was observed from what was inferred. Files are append-mostly histories: the newest section is usually at the top, and older sections describe the project as it was at the time. For current behavior, read the owning contract.

## Product and platform

| Subject | What it covers | Contract |
| --- | --- | --- |
| [Desktop stack and QML](desktop-stack.md) | Why a Rust engine and CLI with a C++20/Qt Quick UI, and the alternatives | [Platform](../contracts/platform.md) |
| [CLI and local engine](cli-and-local-engine.md) | Headless access, one shared engine and store, process lifetime | [CLI](../contracts/cli.md), [Core](../contracts/core-storage.md) |
| [Linux and distribution](linux-and-distribution.md) | Supported distributions, runtime packaging, GUI and CLI installation | [Platform](../contracts/platform.md) |
| [Search and storage](search-storage.md) | SQLite/FTS5 options, WAL, scale experiments and the storage resets | [Core](../contracts/core-storage.md), [Search](../contracts/search.md) |
| [Mail content](html-mail.md) | Text-first reading, sanitized HTML rendering, remote images and the proxy | [Mail UI](../contracts/mail-ui.md) |
| [Local AI](local-ai.md) | Ollama and OpenAI-compatible runners, structured output, evaluation | [Local AI](../contracts/local-ai.md) |
| [Local AI execution](local-assistance-execution.md) | Implementation and test results for the local model pipeline | [Local AI](../contracts/local-ai.md) |

## Google integration

| Subject | What it covers | Contract |
| --- | --- | --- |
| [Gmail](gmail.md) | OAuth scopes, quotas, history-based sync, drafts and sending | [Gmail](../contracts/gmail.md) |
| [Calendar](calendar.md) | Quotas, permissions, sync, recurrence and invitations | [Calendar](../contracts/calendar.md) |
| [Authorization and credential storage](phase-2-auth.md) | Desktop OAuth with loopback callback, OS keychains | [Platform](../contracts/platform.md), [Gmail](../contracts/gmail.md) |
| [OpenPGP](openpgp.md) | Library choice, key and validity rules, sealing secret keys, PGP/MIME and GnuPG interoperability | [Core](../contracts/core-storage.md), [Mail UI](../contracts/mail-ui.md), [Platform](../contracts/platform.md) |
| [HTTPS transport](phase-2-transport.md) | Bounded native HTTPS, quota scheduling, cancellation and fault tests | [Gmail](../contracts/gmail.md), [Quality](../contracts/quality.md) |
| [Gmail HTTP calls](phase-2-gmail-http.md) | Typed Gmail endpoints, method costs, send outcomes | [Gmail](../contracts/gmail.md) |
| [Mail acquisition](phase-2-mail-acquisition.md) | Preparing and publishing downloaded mail into the store | [Core](../contracts/core-storage.md), [Gmail](../contracts/gmail.md) |
| [Calendar adapter](phase-2-calendar.md) | Calendar list and events, limited-access calendars, conditional writes, view performance | [Calendar](../contracts/calendar.md) |
| [Phase 2 execution log](phase-2-execution.md) | Detailed record of the Google integration test runs, including failures | [Core](../contracts/core-storage.md), [Quality](../contracts/quality.md) |

## Design

| Subject | What it covers | Contract |
| --- | --- | --- |
| [September 23 prototype](prototype-september-23.md) | Current design exports and their native implementation | [Mail UI](../contracts/mail-ui.md) |
| [September 22 prototype](prototype-september-22.md) | Mail list and Accounts design | [Mail UI](../contracts/mail-ui.md), [Local AI](../contracts/local-ai.md) |
| [September 15 prototype](prototype-september-15.md) | Earlier alignment work and reader fixes | [Mail UI](../contracts/mail-ui.md) |
| [Flight Progress v2](flight-progress-v2.md) | The original HTML design source and its native adaptation | [Mail UI](../contracts/mail-ui.md), [Calendar](../contracts/calendar.md) |
| [Early mockups](design-references.md) | The first PNG mockups and visual direction | [Mail UI](../contracts/mail-ui.md) |
| [Motion](motion.md) | Lasseter's animation principles applied to timing, folds, popups and reduced motion | [Mail UI](../contracts/mail-ui.md), [Platform](../contracts/platform.md) |

## Adding findings

Add to the matching file with the date and source. Keep observed behavior, inference, untested proposals and measured results distinct, and record versions and commands for anything reproducible. Create a new file only when a subject has real material of its own.

When research changes a decision, append it to the [decision log](../decisions.md) and update the owning [contract](../contracts/README.md).
