# Local protocol — protocol 1, Phase 1 additive boundary

Implementation is in `crates/fm-ipc`, `crates/fm-engine`, `crates/fm-cli` and `gui/MailBackend.cpp`. This is the shared interface for the synthetic Phase 1 implementation, preserving the Phase 0 protocol. The domain method fields and fixture expectations are in [the fixture API](../fixtures/README.md). It is not a promise of the final CLI command catalogue or provider APIs.

## Ownership and discovery

One engine owns a profile's SQLite connection, migration and journal; the core holds an OS file lock for its lifetime. Clients never open SQLite. A newly created profile seeds synthetic fixture revision 2. Existing profiles are never reset on startup; schema 4 migrates to schema 5 while retaining fixture revision 1 and its original accounts. The default is `fuckMail/demo-v1` beneath the user's local application-data directory (macOS `~/Library/Application Support`, Windows `%LOCALAPPDATA%`, Linux `$XDG_DATA_HOME` or `~/.local/share`). `--profile` chooses an explicit absolute path.

The engine writes `engine.json` inside the private profile with protocol version, process ID, local endpoint and a random per-launch handshake token. The token is credential material; do not log, commit or put it in command-line arguments. Unix uses a user-private directory and socket plus peer credentials. Deep Unix paths use a hashed socket in a private `/tmp/fuckmail-UID` directory. Windows uses a named pipe with a current-user DACL and remote clients rejected. Clients validate the expected engine peer before sending the token. The supported boundary is the current OS user, not mutually hostile processes already running as that user.

Concurrent launches acquire the core writer lock before replacing an old owned socket/discovery file. Each connected GUI/CLI owns an explicit session. The last disconnect gives a 350 ms reconnection grace, then the engine closes. A newly started engine allows 10 seconds for its first client. Queued durable data alone never keeps it alive. An independent `fm watch` session may outlive the GUI, and ends on Ctrl-C or its requested duration. There is no installed background service.

## Wire format

Messages are UTF-8 JSON objects terminated by one newline. This selects bounded newline framing from the earlier alternatives; no browser, HTTP listener or Rust/C++ ABI is involved. Both directions allow at most 1 MiB per frame, including the newline. Binary attachments are separate retained files; they are not embedded in list responses.

First request:

```json
{"version":1,"id":"1","method":"hello","params":{"token":"<private discovery token>","client":"cli"}}
```

Normal request and successful response:

```json
{"version":1,"id":"2","method":"conversations","account":"windscribe","params":{"limit":30,"query":"Maya infra"}}
{"version":1,"id":"2","ok":true,"result":{"conversations":[],"next_offset":null,"coverage":{}}}
```

The illustrative response above is only the envelope/field layout; actual rows and coverage come from core state. Errors preserve correlation:

```json
{"version":1,"id":"2","ok":false,"error":{"code":"stale_revision","message":"The target changed; refresh before retrying."}}
```

IDs are strings of at most 128 bytes. `params` is an object; unknown envelope fields are rejected. `hello`, `accounts`, `status`, `preferences_get`, `preferences_save`, `calendar_sources` and `calendar_combined` omit account scope. Other methods must validate the account and the target together. Request `id` correlates a response; mutation `params.request_id` deduplicates durable intent. Reusing a mutation ID with a different payload is an error. Expected revisions prevent overwriting newer state; snapshot message IDs prevent a flag action from silently including a new reply.

At most 16 clients connect and 32 core operations queue; each connection permits exactly one request in flight. The Qt adapter queues bounded asynchronous UI requests locally. Queued work is bounded independently of UI paging. Disconnect cancels work not yet begun and interrupts active SQL reads through a per-request progress callback. The callback is removed before the next owner’s request; late cancellation cannot affect that request. An already-started short mutation transaction completes durably and is recoverable with its request ID. Bounded local file work is not asynchronously interrupted. Clients allow 30 seconds for engine startup and fixture initialization, measured with monotonic clocks. If an initial launch loses to a previous writer that is still closing, clients retry with 1/2/4-second capped backoff and at most ten launch attempts. Rust reaps its own exited child before launching again; Qt keeps detached engine ownership so GUI close cannot kill an independent CLI session. Privacy, incompatible discovery and rejected authentication fail directly. Endpoint cleanup remains inside the old writer lock; absent discovery does not by itself prove the lock has been released. The handshake times out after 5 seconds, incomplete authenticated frames and blocked writes after 15 seconds. An idle authenticated GUI/watch connection is a valid owner. The small implementation uses revision polling for change observation, not a promised lossless event stream.

## CLI surface

`fm --help` and nested help describe the executable's supported commands. Global `--profile`, `--engine`, `--account` and `--json` work before or after the subcommand. `FM_ENGINE_PATH` is an optional development override; installed clients find the sibling engine.

Examples (conversation IDs come from list results):

```sh
fm accounts
fm --account windscribe mail list --query 'Maya infra' --field all
fm --account windscribe mail read CONVERSATION_ID
fm --account windscribe mail flag CONVERSATION_ID --starred true
fm --account windscribe draft save CONVERSATION_ID --body-file reply.txt
fm --account windscribe draft show CONVERSATION_ID --json
fm --account windscribe draft capture CONVERSATION_ID --expected-revision 1 --request-id capture-example --json
fm --account windscribe calendar --from 2026-09-07 --to 2026-09-14
fm watch --seconds 30 --json
```

`--body-file -` and `call METHOD --params-file -` accept bounded UTF-8/JSON input from stdin. Human output is for reading; `--json` emits versioned response envelopes. Core errors have stable codes; CLI exit codes are 2 for invalid input, 3 for authentication/permission failures, 4 for stale revisions/cursors, 5 for unavailable/not-found content and 1 for other failures. Closing an output pipe ends cleanly. The initial CLI does not authenticate real accounts or send messages. `draft capture` is an explicit local diagnostic: it builds a saved revision into a verified `.eml` file through the shared MIME builder and a persisted synthetic Gmail capture journal. `--outcome unknown` tests ambiguous acceptance; retrying the same immutable request cannot submit it twice. Schema 5 migrates schemas 1/2/3/4 in place; protocol remains 1 and existing profiles keep their recorded fixture revision. `draft save --html-file`, `--signature-file`, repeated `--attachment` and `--clear-attachments` expose authored formatting/signature/retained selections through the same revisioned API. Per-draft identity/signature snapshots preserve exact capture content even when account preferences change.

## Phase 1 commands and scope

```sh
fm --account windscribe mail list --grouped --sender maya --status review --has-attachment true --after 2026-09-01 --before 2026-09-12
fm --account windscribe mail review CONVERSATION_ID
fm --account windscribe mail done CONVERSATION_ID
fm --account windscribe mail delay CONVERSATION_ID
fm --account windscribe mail undelay CONVERSATION_ID
fm preferences show --json
fm preferences save --preferences-file preferences.json --expected-revision 0 --request-id settings-1
fm --account windscribe account-update --name Windscribe --signature-file signature.txt --expected-revision 0 --request-id account-1
fm --account windscribe draft save CONVERSATION_ID --body-file reply.txt --html-file reply.html --signature-file reply-signature.txt --attachment RETAINED_ATTACHMENT_ID
fm --account windscribe draft history --conversation-id CONVERSATION_ID --json
fm --account windscribe draft discard CONVERSATION_ID --expected-revision 1 --request-id discard-1
fm calendar-sources --json
fm calendar --combined --from 2026-09-07 --to 2026-09-14 --json
fm --account windscribe calendar-write --params-file event.json --json
fm --account windscribe calendar-operations --json
```

`mail read` stays a query. Explicit review/Done/Delay/undelay commands load a snapshot and submit the same `flag` method as the GUI; their `--expected-revision` and `--request-id` options support explicit callers. For exact snapshot retry after a concurrent arrival, use `fm call flag` with the original immutable payload. Source date filters are UTC, inclusive after/exclusive before. `--grouped` selects review/held/done ordering; ordinary lists retain legacy date order. `--cursor` follows `next_cursor` and replaces offset; a changed account/query/search state or writer restart fails `stale_cursor` rather than silently drifting through pages. View/preferences/draft/calendar writes preserve paging. Treat the cursor as opaque; global response `revision` remains store-wide.

Preferences are validated patches; null calendar selection means all sources, [] means none. These settings never filter mail across accounts. `account-update` changes domain metadata, and existing authored draft snapshots remain unchanged. `draft history` describes local MIME capture, including acceptance unknown, without manufacturing delivered/sent provider mail. `draft history --conversation-id CONVERSATION_ID --unresolved-only --limit 1` checks pending/unknown captures independently of ordinary history pages. Recovery uses the history row's original `operation_id`, `draft_revision`, `conversation_id` and `synthetic_outcome` with `draft_capture`; it does not create a fresh operation ID. `calendar-write` is a deterministic local provider-fake create/edit/self-RSVP command with durable identity, permission and ETag checks; live writes remain later work. `calendar-operations --cursor '<returned continuation JSON>'` pages the account's public journal, with pending/failed operations before acknowledged receipts. Exact field names, limits and error semantics are in the [fixture API](../fixtures/README.md).

## Verification and limits

Framing/private-discovery unit tests and engine session integration tests live beside the Rust code. The Qt client has a native trust test and a `--self-test` path that invokes the separate CLI. These must be run on the native target; writing a test is not pass evidence. Current execution and remaining Phase 1 work are tracked in [status](status.md).

Full change streams, long-body transfer/paged members, real credential-vault integration, provider execution, recurring-calendar mutations and the larger retention/recovery matrix remain their subsequent product gates as specified in the [plan](plans/phase-1.md). This initial protocol document does not remove those obligations.
