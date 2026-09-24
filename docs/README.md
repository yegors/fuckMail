# Documentation index

fuckMail is a personal, opinionated mail/calendar client for large separate accounts. Windows, macOS and required Linux targets share one design. A small CLI starts beside the GUI and grows to support Google/external-agent workflows; nonfinal mockups guide the current stack recommendation. Google is the first product; IMAP/SMTP and external iCalendar subscriptions follow founder dogfooding. Human mail is text-first with later best-effort HTML. The daily objective is user-achieved zero unread and zero starred mail, with complete offline history, reliable calendar behavior, provider-native categories and optional local classification/summaries. Humans write the email.

## Start here

| Document | Purpose |
| --- | --- |
| [Repository overview](../README.md) | Project summary, built feature status and MVP readiness |
| [North star](north-star.md) | The experience we are striving for and the scope filter |
| [Product scope](product-scope.md) | Accepted requirements, intended workflows and exclusions |
| [Visual design and UX](ui-ux.md) | Working brief for the founder/Claude design collaboration: screen elements, layout, state meanings and review scenarios |
| [Development plan](development-plan.md) | Full phased route, integration milestones, ownership and decisions to resolve as needed |
| [Phase 2 execution plan](plans/phase-2.md) | Current Google mail/Calendar packets, real-account admission, API limits and founder test milestones |
| [HTML reader plan](plans/html-reader.md) | Proposed D100 safe HTML rendering, blocked remote images through an engine fetch path and optional proxy, link hover cards and domain blocklist |
| [Local model pipeline plan](plans/local-model-pipeline.md) | Proposed D111 refactor: per-email input without reply chains, code-owned schema, one priority queue with pause and Catch up |
| [SQLite storage reset plan](plans/sqlite-storage-reset.md) | Implemented schema32 fresh start, storage measurements and remaining scale gates |
| [Phase 1 execution plan](plans/phase-1.md) | Completed local client and navigation/rendering packets with scoped acceptance |
| [Phase 0 execution backlog](plans/phase-0.md) | Completed foundation packets and their original acceptance scope |
| [Development commands](build-development.md) | Repeatable Mac/Windows bootstrap, build, test, run and packaging |
| [Local protocol](local-protocol.md) | Implemented synthetic CLI/GUI command boundary and limits |
| [Phase 0 execution evidence](research/phase-0-execution.md) | Observed checks, artifacts and remaining gates |
| [Phase 1 execution evidence](research/phase-1-execution.md) | Completed local client, native integration and acceptance results |
| [Phase 2 execution evidence](research/phase-2-execution.md) | Active Google/live-profile integration, tests and admission gaps |
| [Evidence storage and recovery](research/evidence-archive.md) | Archived raw execution evidence, retained design assets and repository size checks |
| [Current status](status.md) | What exists, what is active/next, what is deferred or excluded, and evidence for readiness |
| [Decision log](decisions.md) | Append-only timestamped decisions, pivots and Q&A |
| [Research index](research/README.md) | Evidence, alternatives and experiments organized by subject |
| [Contract index](contracts/README.md) | Living build specifications, including the full CLI, ownership and integration gates |
| [Current stack recommendation](research/desktop-stack.md) | Rust engine/CLI + C++20/Qt Quick UI proposal, alternatives and evidence needed |
| [Installation research](research/linux-and-distribution.md) | Required Linux targets and simple GUI/CLI delivery |
| [Current HTML prototype](research/prototype-september-22.md) | September 22 mail-list/Accounts design, native alignment and explicit backlog verification |
| [Phase 1 design reference](research/flight-progress-v2.md) | Original source review, unchanged exports and native adaptation |
| [Earlier mockup references](research/design-references.md) | Unchanged Phase 0 PNGs and historical visual direction |

## Where information belongs

Keep this directory at the product level, including the shared visual/interaction brief in **ui-ux.md**. Put source-dependent findings, comparisons, discovered ideas and experiment reports in a named topic under **research/**; there is no general research.md. Put current behavior, implementation boundaries, dependencies and acceptance gates in the relevant **contracts/** file. Contracts move as we learn; they are not promises that everything has been built.

Use **development-plan.md** for the high-level build sequence and **plans/** for detailed execution packets only as their phase approaches. These refer to contracts and scoped research instead of owning a second copy of behavior or evidence. Phases 0 and 1 have completed packet files on available hosts; approaching Phase 2 has a detailed real-Google pilot plan. Later phases remain deliberately lighter.

Use **status.md** as the current progress record and **decisions.md** as history. Root README summarizes status and links here. Avoid duplicating technical specifications in product pages; link to the owning contract.

## When an idea or requirement changes

1. Add the evidence or product idea to its scoped research file, with source/date and a clear distinction between observation, inference and untested proposal.
2. When there is a decision or Q&A, append a dated entry with its outcome and rationale. Unresolved questions may be logged, but a proposal does not become accepted without a decision.
3. Update the owning contract and any affected shared boundaries. Change product scope/north star only when the product direction changes.
4. Adjust the development plan/next packets when sequencing, dependencies or a decision gate changes; expand only the approaching phase.
5. Update status and the root README when scope, active work, verified behavior or readiness changes. Link evidence; do not count planned work as implemented.

## Documentation conventions

Keep authored summaries and reproducible test inputs in Git. Generated logs, traces, screenshots and bulk manifests belong in ignored local storage or CI artifacts; link their location and checksum from the owning report. D046's [archive index](research/evidence-archive.md) preserves earlier evidence and explains recovery. Run `python3 scripts/check_repo_hygiene.py` before committing documentation assets.

Use descriptive kebab-case Markdown filenames. The documentation index lists active phase plans; each research/contract README indexes its current files; add new files to that index when they contain real material. Split a subject when it develops an independent scope, not to pre-create empty placeholders. Future design assets, benchmark reports, build/setup and release instructions should be indexed here or by their owning sub-index when those artifacts exist.

Research records checked dates, versions/source revisions when known, findings, product implications and remaining experiments. Contracts use the [shared contract format and readiness rules](contracts/README.md). Acceptance details and exact APIs should be filled only as evidence and implementation require them; unresolved items stay explicit.

Decision entries use a unique ID, an ISO 8601 timestamp with timezone, context/question, outcome (accepted/proposed/unresolved), rationale and affected files. Append at the end. Never rewrite or remove earlier entries; corrections and superseding choices get new entries referring to the earlier ID. Imported history states when the original timestamp is unknown. Timestamps record when an entry was recorded, not an invented time for the original discussion.
