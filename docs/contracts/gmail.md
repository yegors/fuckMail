# Gmail provider

State: Draft · Revision: 3 · Updated: 2026-09-11 · Implementation: Phase 0 synthetic experiment verified; whole area remains Draft.

Revision 3 records the next Phase 0 integration checkpoint. The fake provider now publishes complete pages and FTS changes atomically with Core checkpoints and replay receipts. Staged or failed pages stay invisible, and pending manual flags/categories win over observed labels. Shared corpus imports verify retained bytes and safe text. Consumers: Core, Search, MIME, Triage and assistance. Real authorization and HTTP remain separate gates.

Revision 2 records D012’s implemented experimental slice. The fake transport, OAuth/PKCE, token-coordination and uncertain-send experiments are implemented; [execution evidence](../research/google-feasibility-execution.md) records prerequisites and limits. No live account has been connected. Consumers: Core, MIME, Calendar and Platform.

Revision 1 applies D009: Gmail/Workspace APIs are the first mail transport, including send; shared MIME handling stays early while IMAP/SMTP are deferred. Affected consumers: Core, Mail UI, Calendar, Triage, Local assistance and CLI.

Owner: B — Gmail. Accepted scope and proposed behavior are separated below; concrete APIs, libraries and outstanding choices await the relevant feasibility work. See [contract workflow](README.md).

## Build boundary

| Boundary | Responsibility |
| --- | --- |
| Owns | Google mail authorization flow, account discovery, initial/delta sync, labels/read/stars, bodies/attachments and draft/submission mapping |
| Consumes | Core account identities, checkpoint/journal contracts, credential references, explicit message targets and category policy |
| Produces | Normalized mail changes, capabilities, label mappings, coverage and durable mutation/submission outcomes |
| Independent start | Fake transport and recorded synthetic responses for pages, quota limits, expired history and write failures |

Related: [core](core-storage.md), [triage](triage.md), [calendar invitations](calendar.md), [Gmail research](../research/gmail.md).

## Required behavior

Prefer the Gmail API for Google accounts. Preserve provider thread IDs and message-level labels; conversations and storage never cross accounts or duplicate bodies for every label. Fetch recent conversations first, interleave deltas and full history/attachment acquisition, and use bounded fair scheduling. Provider quotas determine attainable import speed; no fixed whole-mailbox estimate is promised.

Use the system-browser installed-app authorization flow and OS credential storage. Prove durable personal/Workspace access, not just one successful login. Poll adaptively while running; reconcile after wake/reconnect. Commit history checkpoints only with applied pages; expired history triggers staged reconciliation preserving drafts and pending local intent.

Map categories to configured user labels. Apply changes to explicit existing message IDs; preserve unrelated labels, UNREAD and STARRED. Inbox removal is a separate configured filing effect. A later thread member does not inherit a completed read/unstar/label command. Surface mixed conversation membership, mapping changes and pending/failed writes. Provider mutations from manual use and validated auto-apply share Core's journal; the model never accesses Gmail directly.

Submission follows Core's accepted/failed/acceptance-unknown states. Mail invitations coordinate with Calendar's event identity and attendee notifications; do not independently send duplicate scheduling email.

## Acceptance gates

- Resume paginated import with interleaved new mail and attachments; replay without duplication and recover an expired history cursor without losing user work.
- Validate multi-account identity and durable OAuth on the actual personal/Workspace configurations; measure actual project quotas and backfill behavior.
- Apply/read/reconcile labels, stars and unread state across another client. Archived unread mail remains in the account totals; new replies remain independently reviewable.
- Exercise renamed/deleted labels, manual changes during inference, offline writes and uncertain submission. No late result overrides newer intent.

## Open before affected behavior ships

Record actual Cloud project limits, account types, required scopes and sustained authentication behavior. Finalize label setup/archiving policy and provider-draft synchronization. Public verification/distribution is deferred, but its requirements must be revisited if the audience changes. Protocol and category evidence is in the linked research, not a measured implementation result.
