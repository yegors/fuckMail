# Local protocol — initial revision 1

Implementation is in `crates/fm-ipc`, `crates/fm-engine`, `crates/fm-cli` and `gui/MailBackend.cpp`. This is the small shared interface for the synthetic Phase 0 application. The domain method fields and fixture expectations are in [the fixture API](../fixtures/README.md). It is not a promise of the final CLI command catalogue or provider APIs.

## Ownership and discovery

One engine owns a profile's SQLite connection, migration and journal; the core holds an OS file lock for its lifetime. Clients never open SQLite. A newly created profile seeds only synthetic fixture revision 1. Existing profiles are never reset on startup. The default is `fuckMail/demo-v1` beneath the user's local application-data directory (macOS `~/Library/Application Support`, Windows `%LOCALAPPDATA%`, Linux `$XDG_DATA_HOME` or `~/.local/share`). `--profile` chooses an explicit absolute path.

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

IDs are strings of at most 128 bytes. `params` is an object; unknown envelope fields are rejected. Only `hello`, `accounts` and `status` omit account scope. Other methods must validate the account and the target together. Request `id` correlates a response; mutation `params.request_id` deduplicates durable intent. Reusing a mutation ID with a different payload is an error. Expected revisions prevent overwriting newer state; snapshot message IDs prevent a flag action from silently including a new reply.

At most 16 clients connect and 32 core operations queue; each connection permits exactly one request in flight. The Qt adapter queues bounded asynchronous UI requests locally. Queued work is bounded independently of UI paging. Disconnect cancels work not yet begun and interrupts active SQL reads through a per-request progress callback. The callback is removed before the next owner’s request; late cancellation cannot affect that request. An already-started short mutation transaction completes durably and is recoverable with its request ID. Bounded local file work is not asynchronously interrupted. The handshake times out after 5 seconds, incomplete authenticated frames and blocked writes after 15 seconds. An idle authenticated GUI/watch connection is a valid owner. The small implementation uses revision polling for change observation, not a promised lossless event stream.

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

`--body-file -` and `call METHOD --params-file -` accept bounded UTF-8/JSON input from stdin. Human output is for reading; `--json` emits versioned response envelopes. Core errors have stable codes; CLI exit codes are 2 for invalid input, 3 for authentication/permission failures, 4 for stale revisions, 5 for unavailable/not-found content and 1 for other failures. Closing an output pipe ends cleanly. The initial CLI does not authenticate real accounts or send messages. `draft capture` is an explicit local diagnostic: it builds a saved revision into a verified `.eml` file through the shared MIME builder and a persisted synthetic Gmail capture journal. `--outcome unknown` tests ambiguous acceptance; retrying the same immutable request cannot submit it twice. Schema 3 migrates schemas 1/2 in place; wire/fixture revisions remain 1. Optional authored HTML/retained attachment IDs can use the structured `draft_save` API described in the fixture contract; dedicated formatting CLI controls are not implemented.

## Verification and limits

Framing/private-discovery unit tests and engine session integration tests live beside the Rust code. The Qt client has a native trust test and a `--self-test` path that invokes the separate CLI. These must be run on the native target; writing a test is not pass evidence. Current execution and remaining Phase 0 work are tracked in [status](status.md).

Full change streams, long-body transfer/paged members, real credential-vault integration, provider execution, advanced calendar commands and the larger retention/recovery matrix remain their Phase 0 or subsequent product gates as specified in the [plan](plans/phase-0.md). This initial protocol document does not remove those obligations.
