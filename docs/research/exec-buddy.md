# exec-buddy: mail and calendar product lessons

Sources checked 2026-09-11; reorganized 2026-09-11. Documentation/source evidence and proposals only; no app prototype, live account flow or benchmark has run. Recheck version-sensitive claims before implementation.

Build boundary: [Mail UI](../contracts/mail-ui.md).

## Product lessons from exec-buddy

Source inspected 2026-09-11 in the sibling exec-buddy project; no live account flows were run. The founder requested selected product ideas, not reuse of its implementation or unrelated integrations. The following references explain the origin of refinements now specified in [mail UI](../contracts/mail-ui.md), [triage](../contracts/triage.md), [local assistance](../contracts/local-ai.md) and [calendar](../contracts/calendar.md):

| Observed source behavior | Adaptation for fuckMail |
| --- | --- |
| Current and next event cards, including concurrent events; grouped agenda and hidden all-day counts ([UI](../../../exec-buddy/www/index.html), [event selection](../../../exec-buddy/www/js/store.js)) | Compact current/next status and expandable all-day rows within existing mail/calendar UI; keep calendar identities and true overlaps visible |
| Recognized conference links and link-click reminder acknowledgement ([calendar service](../../../exec-buddy/internal/calendar/service.go), [link helper](../../../exec-buddy/www/js/helpers.js)) | Explicit Join action acknowledges only the occurrence reminder; retain physical location separately; no conferencing integration |
| Sender-click filtering and selected-message restoration ([mail UI](../../../exec-buddy/www/js/app-alpine.js)) | Visible address/category filter chips and per-account conversation/member/scroll restoration |
| Flagged mail, People/Automated filters, and generated snippets ([UI state](../../../exec-buddy/www/js/store.js), [email service](../../../exec-buddy/internal/email/service.go)) | Stars are temporary action holds under the founder's clarified semantics; configurable provider categories and optional local summaries preserve human decisions and original content |
| Calendar search over people/location/description and plain-text/source/rendered reader tabs ([UI](../../../exec-buddy/www/index.html), [filters](../../../exec-buddy/www/js/store.js)) | Calendar search with explicit coverage; plain-text fallback in a secondary menu, without developer-oriented tabs in everyday reading |

Do not adopt its mandatory modal/audio escalation, generic integration dashboard, task generation, AI writing, or limited-history/individual-message mail model. Its implemented Google Calendar API path is still a TODO; source UI ideas are not evidence that our full recurrence/edit/RSVP or complete offline-mail requirements are solved. Its older performance claims are not benchmarks for this project.

## Next experiment

P0-09: Review exec-buddy-inspired interactions: sender chips, older held mail, per-account view restoration, concurrent meetings, Join-and-acknowledge, compact all-day rows with hidden counts, calendar search and plain-text fallback. Verify these in the existing mail/calendar UI rather than adding a dashboard.

These sibling-repository source links are provenance only. They require the adjacent exec-buddy checkout; it is not a fuckMail build dependency.
