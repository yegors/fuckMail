# Phase 0 shared MIME construction

P0-05, 2026-09-11. Consumes Core contract revision 5, Mail UI contract revision 5, the shared MIME corpus revision 1 and Gmail fake `google-p0-v1`. Implemented in [fm-mail](../../crates/fm-mail/src/lib.rs). It has no database, transport, file lookup, model or JavaScript dependency.

## Observed result

On this arm64 Mac with Rust 1.88.0, `cargo test -p fm-mail --locked` passed **9 integration tests**, and `cargo clippy -p fm-mail --all-targets --locked -- -D warnings` passed. Tests are part of normal workspace validation.

The builder uses lettre 0.11.23 with only its `builder` feature enabled, Ammonia 4.1.4 for constrained authored HTML, and SHA-256 verification. The independent decoder is the same mail-parser 0.11.9 version used by Core. Cargo.lock records resolved dependencies. Lettre provides typed MIME alternatives/attachments and an explicit Bcc-preservation option for Gmail-style APIs that accept raw messages without a separate envelope. [lettre builder](https://docs.rs/lettre/0.11.23/lettre/message/index.html), [Bcc behavior](https://docs.rs/lettre/0.11.23/lettre/message/struct.MessageBuilder.html#method.keep_bcc).

| Check | Verified behavior |
| --- | --- |
| Human text/signature | UTF-8, emoji/non-English headers, quoted text and original body line endings survive MIME encoding/decoding. The selected account's signature is appended exactly once without modifying its stored authored body. |
| Replies/recipients | From, To, Cc, Bcc, subject, stable Message-ID, In-Reply-To and References round-trip. The parent ID is included once in References. No synthetic AI content is generated. |
| Rich alternative | A plain alternative remains alongside optional HTML. Qt-exported font-weight/font-style spans survive, as do semantic emphasis tags. Pasted scripts, handler attributes, remote images and unsupported layout styles are removed. Sanitization reports whether the serialized HTML changed; authored source remains unchanged. |
| Attachments | Regular and CID inline parts retain filenames and decoded bytes/checksums. Inline image references must match a supplied CID. Duplicate/invalid CIDs, wrong account, unverified bytes and invalid content type are errors. |
| Bounds/errors | Header newline injection is rejected. Body, recipient, reference, attachment-count/byte and serialized-message limits fail explicitly. Malformed HTML preserves the author's plain alternative; unknown CID references are removed. |
| Shared corpus | The five files in the [shared manifest](../../fixtures/mime-manifest.json) parse without dropping their exact original bytes. Source sizes/hashes and independently specified decoded attachment names/hashes/counts match. Core owns and tests safe incoming HTML-to-text behavior. |
| Google capture | The same output bytes pass through `fm-google::submit_once` into a local capture; decoding confirms the authored text/signature. Journal acceptance is recorded and a repeated operation does not submit again. No Google or SMTP connection occurs. |

## API for the GUI/CLI engine

Both clients use Core's durable draft; neither should construct MIME themselves. The engine calls:

```rust
let built = fm_mail::build_mail(&request, &account_identity, &staged_attachments)?;
// Persist built.raw and built.sha256 with the final user-approved operation before submission.
```

`ComposeRequest` supplies account identity, recipient vectors, subject, exact plain body, optional authored HTML, stable Message-ID/time and reply headers. `AccountIdentity` supplies the trusted From identity and that account's plain/optional HTML signature. `StagedAttachment` supplies account identity, filename/type, retained bytes and Core's checksum, plus an optional bare CID. The builder never opens a client-supplied filesystem path. Core must resolve and authorize blob references before calling it.

`BuiltMail` returns `raw`, `sha256` and `html_was_sanitized`. The bytes deliberately contain Bcc for Gmail API submission; they are private draft/submission data, not a recipient-visible preview. Deterministic MIME boundaries, stable Message-ID and an explicit timestamp make repeated builds of the same input identical. Persist these exact bytes before queuing immutable send intent; uncertain submission is handled by the shared journal, not by rebuilding and retrying.

The plain text is transported as base64 bytes to preserve author Unicode and line endings. Signature composition is explicit: nonempty signatures add two newlines, the standard `-- ` separator and the configured signature. The GUI can preview the signature separately without inserting it into the authored body. Account mismatch fails before serialization.

The HTML sanitizer allows paragraphs/lists/quotes/basic emphasis/links and matching CID images. It permits only `font-weight`, `font-style` and `text-decoration` from Qt style exports. This is composed-email serialization, not a received-HTML renderer or a browser-accuracy claim. The sanitizer parses and filters content without fetching external resources. [Ammonia attribute/style filtering](https://docs.rs/ammonia/4.1.4/ammonia/struct.Builder.html).

Current bounds: 1 MiB for each authored body/signature input, 100 recipients, 100 reply references including the parent, 50 attachments, 25 MiB total decoded attachments and 35 MiB serialized MIME. These are explicit feasibility bounds, not silently truncated data. Provider-specific limits and richer composer behavior need separate validation before live submission.

## Remaining integration and limits

The interactive GUI currently persists plain authored drafts. The Qt formatting/IME probe and this serializer establish that restrained formatting can cross the native boundary; they do not claim finished rich-draft GUI persistence or a formatting toolbar. Core's shared writer still owns durable draft/reply/attachment state and the final capture/send operation. No incoming parser, database or provider transport was forked here.

The source/attachment corpus check deliberately distinguishes parsing from safe readable extraction: mail-parser can offer a synthesized HTML accessor for a plain MIME part, so the tests inspect actual MIME content types when asserting alternatives. Core's own extraction test results must establish script/style exclusion for incoming HTML-only mail.

Live Gmail delivery, recipient-side Bcc behavior, provider draft synchronization, actual large attachments, cross-client rendering and irreversible sending have not been tested. The local capture does not prove remote acceptance or filesystem checkpoint atomicity. LGPL/other Qt packaging requirements and the full dependency-license inventory remain owned by Platform release work.

## Retained source and attachment integration

[Core content storage](../../crates/fm-core/src/content.rs) now supplies `Store::stage_blob(bytes)` and `Store::verified_blob(hash, expected_size)`. Eight focused tests pass on the same Mac with `cargo test -p fm-core content::tests --locked`. They use the real Core schema with isolated temporary profiles and cover exact bytes, idempotent retention, interrupted staging orphans, account-scoped attachment lookup, missing/partial states, corruption, the 64 MiB input bound, private Unix permissions, safe export names and Unix symlink rejection.

Staging writes a private temporary file, flushes it, atomically renames it within the blob directory, then verifies the content. The caller receives its SHA-256 only after publication and must insert the corresponding database reference afterward. Orphan temporary files from interrupted attempts remain distinguishable from complete content. This validates the filesystem side of the boundary; it does not alone prove the caller's SQLite checkpoint transaction or a physical power-loss recovery scenario.

`blob_present` is a bounded metadata check for list coverage; opening an attachment requires full hash and size verification. Attachment lookup includes both account and attachment ID. Missing/partial metadata returns no usable path; missing or modified bytes behind an available record produce `content_unavailable`.

The path returned to GUI/CLI is an independently verified copy under the private `exports` directory with a derived safe basename and preserved ordinary extension, allowing native application associations to work. Raw provider filenames cannot select directories or overwrite a user-chosen destination. Opening/editing this copy cannot alter the retained original. Both newly copied and reused exports are checked before returning; a modified cached export fails verification rather than being silently accepted. These copies and staging orphans currently have no garbage-collection policy.

Unix file and directory flushing was exercised on macOS. Windows uses flushed files and same-volume rename, while portable Rust does not expose the same directory-fsync operation. Windows/Linux native execution, OS attachment associations, disk-full interruption and physical power-loss durability remain separate validation gates; Unix symlink tests do not claim Windows reparse-point execution evidence.
