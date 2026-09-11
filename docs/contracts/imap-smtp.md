# IMAP and SMTP provider

State: Draft · Revision: 0 · Updated: 2026-09-11 · Implementation: not started.

Owner: C — IMAP/SMTP. Accepted scope and proposed behavior are separated below; concrete APIs, libraries and outstanding choices await the relevant feasibility work. See [contract workflow](README.md).

## Build boundary

| Boundary | Responsibility |
| --- | --- |
| Owns | Server capabilities/authentication, mailbox discovery, retrieval/reconciliation, reference threading, flags/folder moves, SMTP submission and sent-copy behavior |
| Consumes | Core identity/checkpoint/operation contracts, credential references and explicit message/primary-folder targets |
| Produces | Normalized message changes and mailbox UID mappings, capabilities, coverage and durable move/send outcomes |
| Independent start | Local protocol fixtures and library experiments including limited capabilities; no Gmail multi-label assumptions |

Related: [core](core-storage.md), [triage](triage.md), [calendar invitations](calendar.md), [protocol research](../research/imap-smtp.md).

## Required behavior

Support the required non-Google accounts through IMAP and SMTP. Discover capabilities rather than requiring every server to implement rev2, QRESYNC or MOVE. Use account/mailbox/UIDVALIDITY/UID remote identities and stable local mappings; sequence numbers and Internet Message-ID alone are insufficient. Thread by reference headers with conservative fallback, never subject equality alone.

Use IDLE where supported for change hints, with bounded polling/reconciliation fallback. Fetch all required history and attachments through resumable queues and expose separate coverage. Preserve pending intent across UIDVALIDITY changes or resynchronization; infer removals only from an applicable complete inventory.

Use native Seen/Flagged state. Category filing moves explicit messages into one mapped primary folder; it does not copy mail into multiple folders to mimic Gmail labels. Preserve local content, conversation identity and unread/star state as remote UIDs change. Negotiate MOVE or a tested fallback without expunging unrelated messages. Partial/ambiguous moves require visible durable outcomes and recovery.

SMTP is separate from mailbox sync. Persist authored content and submission intent first; lost acknowledgements remain acceptance unknown. An accepted submission followed by a failed sent-copy operation must not cause resending. Invitation mail coordinates with Calendar and must not create a second scheduling path.

## Acceptance gates

- Prove the supported server matrix, including a constrained-capability configuration, reconnect, UIDVALIDITY reset and capability-dependent sync paths.
- Interrupt category moves between stages, rename/delete destinations and edit flags from another client. Preserve content/identity and surface uncertainty without unrelated expunge or duplication.
- Resume complete offline acquisition and preserve scope-aware unread/star totals after filing.
- Exercise rejected/accepted/unknown SMTP submission and separate sent-copy failure; drafts and pending operations survive restart.

## Open before affected behavior ships

Identify actual non-Google providers and authentication requirements. Select MIME/IMAP/SMTP libraries using fixtures, packaging and license evidence. Define supported move fallback and sent-copy rules per capability; never claim universal server compatibility from one successful fixture.
