# google-p0-v1

Independent expected outcomes for the fake Gmail sequence:

- `m-shared` appears on both inventory pages; a retry and a repeated ID must not duplicate it.
- `resultSizeEstimate` deliberately changes from 900 to 2. It must not be used as an authoritative count or coverage proof.
- Baseline history is `100`. Listing finishes before the two history pages are drained. The only committed final cursor is `120`, after applying the final page.
- `m-during-import` arrives while initial listing is in progress. Catch-up from `100` must discover it.
- History explicitly removes `m-old`. Missing-from-list `local-authored-draft` and pending intent must remain; absence alone never deletes anything.
- Label changes request a current scoped message reconciliation. Historical label changes do not directly overwrite newer user read/star intent.
- The same Gmail-looking `m-shared`/`t-common` identifiers belong to a second account independently.
- Store failure before a page commit preserves the exact old cursor/token. Expiry requests a new full-scan generation without clearing any source/user work.

These files are synthetic API-shaped JSON. Fake stores exercise the integration contract in memory; they are not SQLite, power-loss, TLS, live-provider or quota evidence.
