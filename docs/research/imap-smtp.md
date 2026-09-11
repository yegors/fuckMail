# IMAP and SMTP: identities, filing and submission

Sources checked 2026-09-10; category sources rechecked 2026-09-11; reorganized 2026-09-11. Documentation/source evidence and proposals only; no app prototype, live account flow or benchmark has run. Recheck version-sensitive claims before implementation.

Build boundary: [IMAP/SMTP contract](../contracts/imap-smtp.md).

## IMAP and SMTP

IMAP handles mailbox access and manipulation; mail submission is separate. Message identity must include the account, mailbox, UIDVALIDITY, and UID. A sequence number or Internet Message-ID is not a durable substitute. Negotiate server capabilities and support the older IMAP4rev1 servers still encountered, rather than requiring rev2 everywhere. [IMAP4rev2](https://www.rfc-editor.org/rfc/rfc9051), [mail submission](https://www.rfc-editor.org/rfc/rfc6409)

CONDSTORE/QRESYNC can reduce work when reconnecting, but they are capability-dependent. Use IDLE where supported for change hints and provide a bounded polling/reconciliation fallback. Folder moves, deletion semantics, sent-copy behavior, and OAuth/app-password requirements need provider fixtures. [IMAP resynchronization extensions](https://www.rfc-editor.org/rfc/rfc7162)

SMTP can leave a client uncertain whether a message was accepted when a connection times out. Proposed product behavior: persist an ambiguous-send state and reconcile where possible; do not blindly retry as if exactly-once delivery were guaranteed. [SMTP timeout and duplicate-delivery discussion](https://www.rfc-editor.org/rfc/rfc5321)

Generic IMAP folders are not interchangeable with Gmail's multi-label model. The MOVE extension transfers messages between mailboxes; capabilities and returned UID mappings affect implementation. Recommendation: map classification to one primary folder, expose assignment as filing/moving, preserve local identity/content and flags, and test interrupted moves/fallbacks without expunging unrelated messages. Do not copy mail into multiple folders merely to emulate multiple category labels. [IMAP MOVE](https://www.rfc-editor.org/rfc/rfc6851), [IMAP message flags and identities](https://www.rfc-editor.org/rfc/rfc9051)

## Next experiments

P0-05 (mail protocols): evaluate maintained MIME, IMAP and SMTP libraries for behavior, packaging and licenses. Exercise constrained server capabilities, interrupted folder moves and ambiguous submission using local fixtures. Record the actual non-Google provider matrix before promising compatibility.

P0-07 (IMAP filing): test configured primary-category folders, creation/mapping, mixed conversation membership, UID changes, partial moves and cross-client edits. Read/star state must survive filing. The Gmail side belongs in its own contract.
