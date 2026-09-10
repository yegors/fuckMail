# fuckMail: product and engineering plan

Research date: 2026-09-10. Status: both product interview rounds incorporated; engineering proposals await feasibility measurements.

We are building a local Windows and macOS mail and calendar client for several very large, separate accounts. Mail is conversation-only and mouse-first, with fast sender/subject/body search and complete offline storage of messages and attachments. Google APIs are preferred; IMAP/SMTP, full Google calendar workflows and read-only iCalendar URL subscriptions are required. Linux is a stretch goal. Tasks, widgets, contacts, and AI features are excluded from the MVP.

The brief and both interview rounds establish those requirements. Everything labeled **proposed** is an engineering recommendation, not a measured result or accepted technology decision. Editable CalDAV is deferred; closing the app stops background activity. No implementation or mailbox access has taken place.

Subsequent technology preference: use a native cross-platform desktop toolkit and avoid the JS ecosystem. The founder requested a closer investigation of Rust + Slint + SQLite, especially visual quality; this is now the next candidate to evaluate, with Qt retained as a fallback. No implementation stack is selected. The HTML message renderer and formatted composer need separate feasibility work.

| Document | Purpose |
| --- | --- |
| [North star](north-star.md) | The experience we want and the rules for keeping scope small |
| [Product scope](product-scope.md) | Confirmed requirements, proposed MVP behavior, and exclusions |
| [Research](research.md) | Primary sources, practical constraints, and technical options |
| [Slint UI investigation](slint-ui-investigation.md) | Published visual examples, native UI capabilities, limitations and proposed evaluation |
| [Architecture](architecture.md) | Proposed boundaries, data ownership, and reliability contracts |
| [Performance](performance.md) | Proposed measurable targets and a realistic evaluation plan |
| [Delivery plan](delivery-plan.md) | Phases, parallel workstreams, dependencies, and completion gates |
| [Decisions and questions](decisions-and-questions.md) | Open interview questions and the decision register |

Start with the north star and product scope. Resolve the remaining questions and run the phase 0 experiments before committing to a stack. Keep source-dependent observations in the research document and record subsequent decisions with their rationale and evidence.
