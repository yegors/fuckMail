# Documentation index

fuckMail is a personal, opinionated mail/calendar client for large separate accounts. Windows, macOS and required Linux targets share one design. The full CLI supports external agent workflows; nonfinal mockups guide the current stack recommendation. The daily objective is zero unread and zero starred mail, with complete offline history, reliable calendar behavior, provider-native categories and optional local classification/summaries. Humans write the email.

## Start here

| Document | Purpose |
| --- | --- |
| [Repository overview](../README.md) | Project summary, built feature status and MVP readiness |
| [North star](north-star.md) | The experience we are striving for and the scope filter |
| [Product scope](product-scope.md) | Accepted requirements, intended workflows and exclusions |
| [Visual design and UX](ui-ux.md) | Working brief for the founder/Claude design collaboration: screen elements, layout, state meanings and review scenarios |
| [Current status](status.md) | What exists, what is active/next, what is deferred or excluded, and evidence for readiness |
| [Decision log](decisions.md) | Append-only timestamped decisions, pivots and Q&A |
| [Research index](research/README.md) | Evidence, alternatives and experiments organized by subject |
| [Contract index](contracts/README.md) | Living build specifications, including the full CLI, ownership and integration gates |
| [Current stack recommendation](research/desktop-stack.md) | Rust engine/CLI + C++20/Qt Quick UI proposal, alternatives and evidence needed |
| [Installation research](research/linux-and-distribution.md) | Required Linux targets and simple GUI/CLI delivery |
| [Mockup references](research/design-references.md) | Supplied visual direction; not final screens |

## Where information belongs

Keep this directory at the product level, including the shared visual/interaction brief in **ui-ux.md**. Put source-dependent findings, comparisons, discovered ideas and experiment reports in a named topic under **research/**; there is no general research.md. Put current behavior, implementation boundaries, dependencies and acceptance gates in the relevant **contracts/** file. Contracts move as we learn; they are not promises that everything has been built.

Use **status.md** as the current progress record and **decisions.md** as history. Root README summarizes status and links here. Avoid duplicating technical specifications in product pages; link to the owning contract.

## When an idea or requirement changes

1. Add the evidence or product idea to its scoped research file, with source/date and a clear distinction between observation, inference and untested proposal.
2. When there is a decision or Q&A, append a dated entry with its outcome and rationale. Unresolved questions may be logged, but a proposal does not become accepted without a decision.
3. Update the owning contract and any affected shared boundaries. Change product scope/north star only when the product direction changes.
4. Update status and the root README when scope, active work, verified behavior or readiness changes. Link evidence; do not count planned work as implemented.

## Documentation conventions

Use descriptive kebab-case Markdown filenames. Each research/contract README indexes its current files; add new files to that index when they contain real material. Split a subject when it develops an independent scope, not to pre-create empty placeholders. Future design assets, benchmark reports, build/setup and release instructions should be indexed here or by their owning sub-index when those artifacts exist.

Research records checked dates, versions/source revisions when known, findings, product implications and remaining experiments. Contracts use the [shared contract format and readiness rules](contracts/README.md). Acceptance details and exact APIs should be filled only as evidence and implementation require them; unresolved items stay explicit.

Decision entries use a unique ID, an ISO 8601 timestamp with timezone, context/question, outcome (accepted/proposed/unresolved), rationale and affected files. Append at the end. Never rewrite or remove earlier entries; corrections and superseding choices get new entries referring to the earlier ID. Imported history states when the original timestamp is unknown. Timestamps record when an entry was recorded, not an invented time for the original discussion.
