# Gmail sync fixtures

Synthetic, API-shaped Gmail responses used by `tests/workflows.rs`. They
describe one full listing followed by history catch-up, and the tests expect
these outcomes:

- `m-shared` appears on both listing pages; a retry and a repeated ID must not duplicate it.
- `resultSizeEstimate` deliberately changes from 900 to 2. It must not be used as an authoritative count or as proof of coverage.
- Baseline history is `100`. Listing finishes before the two history pages are drained. The only committed final cursor is `120`, after applying the final page.
- `m-during-import` arrives while the initial listing is in progress. Catch-up from `100` must discover it.
- History explicitly removes `m-old`. The unlisted `local-authored-draft` and pending intent must remain; absence alone never deletes anything.
- Label changes request a fresh reconciliation of the affected message. Historical label changes do not directly overwrite newer user read/star intent.
- The same Gmail-looking `m-shared`/`t-common` identifiers belong to a second account independently.
- A store failure before a page commit preserves the exact old cursor/token. Expiry requests a new full scan without clearing any source or user work.

The tests use in-memory stores. They exercise the sync state machine only, not
SQLite durability, TLS, quotas or a live Gmail account.
