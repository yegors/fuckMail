# Test fixtures

Everything here is synthetic: no real mail, accounts or credentials.

| Path | What it is |
| --- | --- |
| `messages/*.eml` | Five MIME messages in wire format, with CRLF line endings |
| `mime-manifest.json` | Expected size, SHA-256, text and attachments for each message |
| `composer-qt.txt`, `composer-qt.html` | Plain text and Qt rich-text HTML of a formatted draft, as the composer's `QTextDocument` produces them |

The demo profile's accounts are not stored here; they are generated in code (see [Demo data](#demo-data)). Other tests keep their fixtures next to the code: Gmail API pages and synthetic OAuth keys in [crates/fm-google/tests/fixtures](../crates/fm-google/tests/fixtures/README.md), and sample mail for local-model tests in `crates/fm-assist/fixtures/ollama-mail.json`.

## MIME messages

| File | What it covers |
| --- | --- |
| `plain-utf8.eml` | UTF-8 plain text (accents, €, Japanese, Cyrillic) with reply headers and a quoted line |
| `multipart-alternative.eml` | Plain and HTML alternatives; the plain part is the body |
| `html-only.eml` | Automated HTML-only mail with a script, a remote tracker and image, an iframe, a form and an `onerror` handler. None of them may reach the extracted text, and nothing is fetched. |
| `inline-and-attachment.eml` | HTML with an inline image referenced by Content-ID (`logo.png`) and a regular attachment (`deployment-notes.txt`) |
| `malformed-encoding.eml` | Broken quoted-printable. Parsing must not panic, must keep the exact bytes and must return the readable part. |

`mime-manifest.json` has a corpus `fixture_revision` (currently 1) and one entry per message:

- `path`, `source_size` and `source_sha256` pin the exact bytes.
- `expected_text_contains` and the optional `expected_text_not_contains` list substrings that the extracted text must or must not contain.
- `attachment_count` counts regular attachments; `inline_count` counts parts with a Content-ID.
- `attachments` gives each part's `name`, the SHA-256 of its decoded content, and `content_id` for inline parts.

The remaining fields (`preferred_body`, `network_requests_allowed`, `expected_behavior`, `database_fixture_revision`, `sender_kind_annotations`) document intent; no test reads them.

Two tests read the manifest:

- `shared_incoming_corpus_retains_sources_and_attachment_hashes` in `crates/fm-mail/tests/roundtrip.rs` checks each file's size and hash, parses it, counts and hashes its attachments, and runs the shared text extraction against the expected substrings. It also asserts `fixture_revision` is 1.
- `mime_corpus_import_preserves_bytes_visible_search_and_attachment_checksums` in `crates/fm-core/src/google_store_tests.rs` imports every message through the Gmail staging path into a store. It checks the stored text, that the retained source matches the file byte for byte, the attachment checksums, and that search finds the HTML text but not the script. Import drops quoted history, so the quoted line in `plain-utf8.eml` must be absent there.

The `composer-qt` pair is used by the draft save and capture tests in `crates/fm-core/src/capture.rs`.

## Demo data

`crates/fm-core/src/fixture.rs` generates the demo content, plus a few extra calendar events from `seed_fixture_calendars`. It is seeded in one transaction when a new demo profile is created (`--demo-profile`), or when a Rust test calls `Store::open` on an empty directory. The profile records database fixture revision 3 (`FIXTURE_REVISION` in `crates/fm-core/src/lib.rs`). Live profiles record 0 and get no demo content. Opening an existing demo profile never reseeds it or resets drafts and actions.

| Account | Name | Address | Shown as |
| --- | --- | --- | --- |
| `windscribe` | Windscribe | `you@windscribe.example` | Google Workspace (synthetic) |
| `personal` | Personal | `you@personal.example` | Gmail (synthetic) |
| `sideproject` | Side Project | `you@sideproject.example` | Gmail (synthetic) |

The accounts stay separate. Provider message IDs deliberately repeat across accounts, so every local ID and command is account-scoped. Each account has 110 conversations, `<account>-thread-000` to `-109`, with messages named `<account>-message-<thread>-<member>`:

- Threads 000–003 are recent human mail from 11 September 2026. Thread 000, "Q3 infra review: v4 attached" from Maya Lindqvist, has three messages; the second is outgoing.
- Thread 004 is a 32-message discussion; each message has a distinctive `longthread<N>` keyword.
- Thread 005 is automated: the exact `html-only.eml` bytes from Status Robot. It is the only message classified as automated; every other message is classified as people.
- The rest are one-message "Project correspondence" threads spread across August 2026, each with a searchable `archive<NNN>` keyword. Threads numbered in multiples of 17 are outgoing.
- Threads 000–003 and every fifth thread are unread. Every 11th thread and alternate messages in thread 004 are starred. Threads 090–109 are archived out of the Inbox, some of them still unread.
- Each account has six labels (Infra, Finance, Apps, Hiring, Planning, Noise), a signature, and a saved reply draft in thread 003.
- Three attachment records are available, missing and partial respectively, so the app can never claim full offline availability.

Each primary calendar uses `America/Toronto` and covers the week of 7–13 September 2026. It has a Monday/Wednesday/Friday standup, overlapping events on 11 September (one with attendees and a pending response) and an all-day weekend event. `personal` adds four all-day events on 11 September. `sideproject` also has a free/busy-only calendar whose event's title, description and location must never be shown.

Demo profiles report `synthetic: true` and `fixture_today: "2026-09-11"` in `status`, and the GUI's `--fixture-clock` pins the clock to that date. Demo profiles can't sign in to Google or send mail. Drafts are only captured locally as MIME, and calendar writes go to a local fake.

Tests rely on this data. Rust tests use fixed IDs such as `windscribe-thread-000` and `windscribe-attachment-0`. The GUI's `--smoke` and `--self-test` runs (CTest `gui-smoke`, `gui-integration`) use a temporary demo profile. The package check runs `fm --demo-profile --account windscribe mail list --query Maya --field sender` on the relocated build.

The engine methods these tests exercise are specified in [docs/local-protocol.md](../docs/local-protocol.md) and [docs/contracts/core-storage.md](../docs/contracts/core-storage.md).

## Changing fixtures

**MIME messages.** The `.eml` files are wire format with CRLF line endings. [.gitattributes](../.gitattributes) marks them `-text` so Git never converts them, and [.editorconfig](../.editorconfig) keeps their CRLF and trailing whitespace. Use an editor that preserves CRLF, or write the bytes from a script.

- Any byte change must update the entry's `source_size` and `source_sha256`, and an attachment's `sha256` if its part changes. Find out why the bytes changed before updating a hash to make a test pass. To print both values:

  ```sh
  python -c "import hashlib,sys; d=open(sys.argv[1],'rb').read(); print(len(d), hashlib.sha256(d).hexdigest())" fixtures/messages/plain-utf8.eml
  ```

- To cover a new case, add a new `.eml` file and manifest entry; both tests read every entry. Prefer adding over editing: other tests depend on existing bytes, and `html-only.eml` is also demo mail, checked byte for byte.
- Changing or removing an existing message makes a new corpus revision. Bump `fixture_revision` in the manifest and the value asserted in `crates/fm-mail/tests/roundtrip.rs`.

**Composer files.** The `gui-composer-probe` test program writes a fresh pair with `--export-dir <dir>`. Replace these files when Qt's HTML output changes.

**Demo data.** Edit `crates/fm-core/src/fixture.rs`, then run the full Rust tests, since many tests use fixed demo IDs and counts. Existing demo profiles are never reseeded; move the `demo-v1` profile aside or use a new `--profile` directory to see the change. A build refuses a demo profile whose fixture revision is higher than its own `FIXTURE_REVISION`.
