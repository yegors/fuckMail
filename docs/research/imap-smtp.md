# IMAP and SMTP: identities, filing and submission

**Deferred protocol work under D009 (2026-09-11):** all current accounts use Google APIs. Do not execute IMAP/SMTP capability, session or transport experiments below until the founder has dogfooded working Google GUI and CLI workflows. Retain this research for Phase 4. Shared MIME parsing/building and safe text extraction needed for Gmail remain in P0-05, without SMTP transport.

Sources checked 2026-09-10; category sources rechecked 2026-09-11; reorganized 2026-09-11. Documentation/source evidence and proposals only; no app prototype, live account flow or benchmark has run. Recheck version-sensitive claims before implementation.

Build boundary: [IMAP/SMTP contract](../contracts/imap-smtp.md).

## IMAP and SMTP

IMAP handles mailbox access and manipulation; mail submission is separate. Message identity must include the account, mailbox, UIDVALIDITY, and UID. A sequence number or Internet Message-ID is not a durable substitute. Negotiate server capabilities and support the older IMAP4rev1 servers still encountered, rather than requiring rev2 everywhere. [IMAP4rev2](https://www.rfc-editor.org/rfc/rfc9051), [mail submission](https://www.rfc-editor.org/rfc/rfc6409)

CONDSTORE/QRESYNC can reduce work when reconnecting, but they are capability-dependent. Use IDLE where supported for change hints and provide a bounded polling/reconciliation fallback. Folder moves, deletion semantics, sent-copy behavior, and OAuth/app-password requirements need provider fixtures. [IMAP resynchronization extensions](https://www.rfc-editor.org/rfc/rfc7162)

SMTP can leave a client uncertain whether a message was accepted when a connection times out. Proposed product behavior: persist an ambiguous-send state and reconcile where possible; do not blindly retry as if exactly-once delivery were guaranteed. [SMTP timeout and duplicate-delivery discussion](https://www.rfc-editor.org/rfc/rfc5321)

Generic IMAP folders are not interchangeable with Gmail's multi-label model. The MOVE extension transfers messages between mailboxes; capabilities and returned UID mappings affect implementation. Recommendation: map classification to one primary folder, expose assignment as filing/moving, preserve local identity/content and flags, and test interrupted moves/fallbacks without expunging unrelated messages. Do not copy mail into multiple folders merely to emulate multiple category labels. [IMAP MOVE](https://www.rfc-editor.org/rfc/rfc6851), [IMAP message flags and identities](https://www.rfc-editor.org/rfc/rfc9051)

## Retained experiments for the later protocol phase

P0-05 (mail protocols): evaluate maintained MIME, IMAP and SMTP libraries for behavior, packaging and licenses. Exercise constrained server capabilities, interrupted folder moves and ambiguous submission using local fixtures. Record the actual non-Google provider matrix before promising compatibility.

P0-07 (IMAP filing): test configured primary-category folders, creation/mapping, mixed conversation membership, UID changes, partial moves and cross-client edits. Read/star state must survive filing. The Gmail side belongs in its own contract.

## Rust shortlist: MIME now, legacy transports later

Checked 2026-09-11. Evaluate this small set against the same fixtures; names are candidates, not a version or library decision.

| Responsibility | First candidate | What it does not establish |
| --- | --- | --- |
| Incoming MIME | `mail-parser`: headers, MIME parts and decoded message content, with optional legacy-encoding support | Successful parsing is not preservation of original bytes, bounded memory, correct conversation grouping or safe HTML rendering. Keep source bytes and independently verify extracted text/attachment checksums. [Documentation](https://docs.rs/mail-parser/latest/mail_parser/) |
| Outgoing MIME | `lettre`'s builder without requiring its SMTP transport; compare `mail-builder` only if multipart/inline/quoting fixtures justify it | One serialization path for Gmail now and possible SMTP reuse later. `mail-builder` builds MIME but does not send or parse it. [lettre](https://docs.rs/lettre/latest/lettre/), [mail-builder](https://docs.rs/mail-builder/latest/mail_builder/) |
| IMAP session | `async-imap`, with its underlying `imap-proto` parser/types | A session client is not a resumable synchronization engine. Audit required extension support and raw-command escape hatches; do not assume QRESYNC/MOVE/UID mappings work because their RFCs exist. `imap-proto` alone is not a client. [async-imap](https://docs.rs/async-imap/latest/async_imap/), [imap-proto](https://docs.rs/imap-proto/latest/imap_proto/) |
| SMTP submission | `lettre` with an explicitly selected async runtime and TLS configuration | Durable outbox, ambiguous acceptance, provider Sent-copy policy and retry decisions remain ours. Its TLS options have different native/runtime dependencies; packaging must test the chosen feature set. [Transport and TLS](https://docs.rs/lettre/latest/lettre/) |

Build candidates inside the Qt-free engine boundary. Review the current release, source maintenance, supported runtime/features, direct/transitive licenses and native dependencies at execution time. Published Rust docs demonstrate interfaces, not compatibility with the founder's unknown non-Google servers.

## Executable investigation recipes, when implementation is requested

| Investigation | Inputs and output | Acceptance and fallback |
| --- | --- | --- |
| P0-05A: MIME correctness and resource use | Synthetic messages covering plain/HTML alternatives, Unicode and legacy encodings, nested messages, inline CID images, missing boundaries and large attachments; human-authored draft fixtures. Output normalized fields, part checksums, parse failures and MIME round-trip comparisons. | Original source remains recoverable; supported fixture parts and recipients survive; malformed input has a bounded failure path; measure peak memory instead of assuming streaming. If the first builder fails required cases, evaluate the alternative on the same corpus before selecting it. No sending to real recipients. |
| P0-05B: IMAP capability and identity behavior | Local scripted server/transcripts with UIDVALIDITY changes, IDLE disconnect, absent QRESYNC/MOVE, changed flags and interrupted filing. Output a capability matrix and restart/reconciliation trace. | Required baseline works without optional extensions; destination UID changes retain local identity; partial moves preserve unrelated messages and report uncertainty. If a missing extension is essential, evaluate a small upstreamable adapter or another client before locking the provider contract. Do not rewrite IMAP from scratch by default. |
| P0-05C: Submission outcome ownership | Local SMTP sink with accepted, rejected and connection-lost responses, plus a simulated IMAP Sent append. Output durable operation states and captured MIME. | No blind automatic retry after uncertain acceptance; explicit rejection differs from server acceptance; acceptance is not proof of recipient delivery; Sent-copy behavior cannot duplicate submission. A transport limitation must be resolved before live send is enabled. |
| P0-07: Native folder classification | Same local server with renamed/deleted category folders, concurrent manual changes and unread/starred messages in mixed conversations. Output before/after identity and flag assertions. | Manual and automatic filing share the same operation path, preserve flags and cannot expunge unrelated deleted mail. Capability limitations are surfaced with recoverable state. |

Only the shared MIME portion runs alongside Google OAuth and text/composer work now. IMAP/SMTP recipes retain their historical IDs but start after actual founder Google GUI/CLI dogfooding. Later interoperability needs provider names, authentication requirements and user-authorized test accounts. No current non-Google inventory is needed to build the Google product.
