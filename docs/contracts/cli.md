# CLI and external agent interface

## D216 — forward format (CLI65)

- `fm draft create --mode forward --forward-format html|text` passes `forward_format` (Core103). Without it a forward quotes the original's text, as before.


## D212 — terminal-safe text output (CLI64)

- Human output (not `--json`) passes mail, calendar and engine text through one filter: C0 controls, DEL, C1 controls and bidi overrides/isolates (U+202A–U+202E, U+2066–U+2069) become U+FFFD. Single-line fields turn line breaks and tabs into a space; bodies keep `\n` and tabs and turn CRLF into `\n`. The pretty-JSON fallback and error lines are filtered too. `--json` output is unchanged.
- `fm mail unsubscribe` passes the address it read from the message source as `address` (Gmail46).


## D199 — thread summary (CLI63)

`fm --account ID assist thread CONVERSATION [--json]` summarizes the whole conversation with the local model (Local assistance32). Without `--account` it exits 2 before opening storage. It polls the job, Ctrl-C cancels it, and it prints the summary, adding "(Partial: older messages were shortened or left out.)" when that applies. `--json` prints the final `assist_job` reply; a failed job exits with its error code.

## D198 — the shortcut bar preference (CLI63)

`preferences show` includes `key_hints` (boolean, default true; Core91), and `preferences save` accepts it. Other commands are unchanged.

## D183 — forwards with files (CLI62)

`draft create --mode forward` returns a draft that already selects the source message's attachments (Core86). `draft save --attachment` replaces that selection and `--clear-attachments` empties it. `google sent --conversation-id` also lists forwards from that conversation. Other commands are unchanged.

## D182 — starred contacts in preferences (CLI61)

`preferences show` includes Core85's `starred_senders`; `preferences save` replaces the whole list. Other commands are unchanged.

## D162 — unchecked mail in sync status (CLI60)

Account and status sync JSON add Core83's `unchecked_messages`: kept messages the current listing has yet to see. Other fields are unchanged.

## D160 — reading text in `mail source` (CLI59)

`mail source` adds Core82's `body_text` and `text_truncated`. Other fields are unchanged, and querying still never marks mail reviewed.

## D158 — status refresh generations (CLI58)

`status` exposes Core80's additive `ui_generations` object with `epoch`, `accounts`, `categories`, `reader`, `drafts` and `captures`. Compare counters only within the same epoch; a new epoch invalidates all areas. Existing command grammar, calendar/mail generations, preference revision and protocol1 stay unchanged. Consumers: Mail UI109; Quality85.



## D154 — HTML senders (CLI57)

`preferences show` includes `html_senders` instead of `message_format`; `preferences save` replaces the whole list. `mail read`, `mail html` and `mail source` are unchanged.

## D144 — calendar search around a day (CLI56)

- `fm calendar search QUERY --around YYYY-MM-DD [--limit N]` lists the next time of each matching event within a year of that day, then its latest time in the 60 days before (Calendar38). `--from`/`--to` remain the range form, and `--around` cannot be combined with them.
- The query takes `title:`, `with:` and plain words as the GUI does; `date:` is read but never filters.

Consumers: Calendar38.

## D133 — run from the developer console (CLI55)

The GUI's developer console runs `fm` as a child process with this profile, engine and (for account commands) the selected account filled in (Mail UI97). The CLI itself is unchanged: its arguments, output, exit codes and connection to the running engine are the same as in a terminal. No stdin is attached, so commands that wait for input see end of file.

## D127 — invitation details (CLI54)

`fm --account ID mail invite MESSAGE --source-version V` prints `message_invite` (Core71) read-only: the invitation's UID, method, times, the time it moved from and the linked Calendar event.

## D123 — label Read and Spam (CLI53)

`fm --account ID labels auto-read LABEL on|off --expected on|off --request-id R` (`label_auto_read`) and `labels spam LABEL on|off --expected on|off --request-id R` (`label_auto_spam`, Core68). Gmail drafts appear in `drafts` as ordinary local drafts (Gmail31); the draft source commands remain.

## D121 — to: search (CLI52)

`mail list --query` reads the Search 9 grammar: `from:` is the sender, `to:` a recipient and a bare address either. `mail senders` takes `--role sender|recipient` for the people a `to:` can name. Without it, `--folder sent|drafts` still lists recipients.

## D120 — Spam and Trash (CLI51)

`fm --account ID google bin-count spam|trash` (`google_bin_counts`), `google not-spam THREAD_ID` (`google_not_spam`), `google delete-forever spam|trash ID --yes` (Spam takes a Gmail thread id, Trash a conversation id; `google_delete_forever`) and `google empty spam|trash --yes` (`google_empty`). Without `--yes` the two deleting commands refuse with an input error. Deleting needs the account reconnected with full mail access. Consumers: Gmail30, Core67.

## D118 — remote search and unsubscribe (CLI50)

`fm --account ID google search QUERY [--folder inbox|sent|all|trash|drafts]` (`google_remote_search`), `google thread THREAD_ID` (`google_remote_thread`) and `google attachment ID` (`google_remote_attachment`, prints the cached path) read live and store nothing. `fm --account ID mail unsubscribe ID --source-version V` reads the message's `unsubscribe` and calls `google_unsubscribe_mail` for mailto or `message_unsubscribe` otherwise; for a link it records the page as opened and prints its URL.

## D116 — live Spam (CLI49)

`fm --account ID google spam [--page-token T]` lists Gmail's Spam live (`google_spam_list`); `fm --account ID google spam-show ID` prints one message's text (`google_spam_message`). Both need the account's sync running and store nothing.

## D111 — local model queue (CLI48)

- `assist run` (alias `assist watch`) keeps the engine's one queue running until Ctrl-C.
- `--account ID assist catch-up --range week|month|year|all|stop` widens or clears the account's Catch up bound and returns its progress. `assist backlog` prints each range's unprocessed count.
- `--account ID assist pause` / `assist resume` save the account's model pause.
- `assist analyze CONVERSATION --message-id ID` analyses one received message now, replacing its saved results. `assist summarize`, `assist classify` and `assist batch` are removed; `assist test` no longer takes a task.
- `assist progress` adds `failed`, `catching_up`, `model_error` and `running`. See [Local assistance25](local-ai.md#d111--one-queue-one-email-systemuser-prompts-local-assistance25).

## D112 — progress and account colour (CLI47)

- `assist progress` prints per-account analysis counts: done, queued, unprocessed and seconds per message (Local assistance24).
- `account-update --color #6FB5B2` sets the account's colour from the 16-entry palette. `accounts --json` rows include `color` and `calendar_events`.
- `account-update --analyze-history true|false` (D108) is unchanged.

## D107 — list people and message format (CLI45)

- `mail list --json` rows include Core's `others` and `other_names` (Core59).
- `preferences show` includes `message_format`. `preferences save` accepts `html` or `text` for it and changes the GUI's HTML/Text choice for every message. (Retired by D154/CLI57: see `html_senders`.)

Neither marks mail reviewed.

## D105 — HTML reader, links and proxy (CLI44)

Every command below uses the same engine methods as the GUI. None marks mail reviewed.

**Mail**
- `mail html ID --source-version V` prints `message_html`: the sanitised HTML or its file path, with the resource and link lists.
- `mail images ID --source-version V [--load]` reports each image's state. Without `--load` it reads only the message's own parts and the image cache. With `--load` it fetches remote images through the engine: over https only, through the proxy when one is on, and never from a blocked host. `insecure` images are reported without a request. Each image call allows 30 s.

**Links**
- `links summary HOST [--conversation C]` prints `link_host_summary`.
- `links blocked` lists `blocked_hosts` with the `link_pattern_counts` for each pattern.
- `links block PATTERN` and `links unblock PATTERN` save the list through `preferences_save` against the revision just read, with a generated request ID. Input is reduced to its host: lowercase, with no scheme, path or `www.`, and `*.` kept. Blocking an existing pattern, or unblocking an absent one, is an input error.

**Proxy**
- `proxy show` prints `remote_proxy` and whether a password is saved. The password is never shown.
- `proxy set HOST[:PORT] [--username U] [--password-stdin]` turns the proxy on, with port 443 by default. `--password-stdin` reads up to 4 KiB from standard input into the system keychain.
- `proxy clear` turns it off and forgets the password.
- `proxy test` makes one request through the proxy, allowing 30 s.
- `proxy cache [--clear]` reports or clears the image cache.

`links blocked|block|unblock` and every `proxy` command need no account scope. `mail html|images` and `links summary` need one, and scope is checked before storage is touched.

Consumers: Core57, Mail UI74 and Quality75; Search7, schema34, fixtures0/3 and protocol1. [Execution record](../research/html-mail.md#d105--html-reader-implementation-2026-09-24).

## D101 — search grammar and people (CLI43)

`mail list --query TEXT` takes the same [Search 7](search.md#query-grammar-revision-7) text as the GUI search box, and JSON results carry Core's `search_terms`. `--field all` now means subject or message text; `sender`, `subject` and `body` still narrow bare words to one column. `--month-zone ZONE` and repeatable `--collapsed-month YYYY-MM` return and fold the Inbox's Done months. `mail senders [QUERY] --folder inbox|all|sent|drafts|trash --limit 1-10` lists the people a `from:` would offer, reading only metadata; it never marks mail reviewed. Existing typed filter flags keep working and conflict explicitly with the same operator in the text. Consumers: Core56, Search7, Mail UI71 and Quality72; schema33, fixtures0/3 and protocol1 unchanged.

## D092 — shared Undo and account controls (CLI42)

`mail undo` accepts the original Undo request ID and expected conversation revision, using the same durable `mail_undo` boundary as GUI toasts. `account-update` accepts the extended retention choices plus paired `--mail-access` and `--calendar-access` values (`none`, `read`, `write`). Both cannot be `none`. Preferences expose the same typed editable shortcut map as the GUI; conflicts and fixed-key reservations are validated in Core. Consumers: Core50/Mail UI67/Calendar29/Quality67; schema31, fixtures0/3/protocol1.

## D086 — mail list and account backlog

`fm --account ID assist backlog` reads cached analysis coverage. `assist batch --range week|month|year|all` starts explicit missing-only account analysis, waits for its result and cancels on Ctrl-C. The GUI uses the same `assist_backlog` and `assist_batch_start` methods plus existing job status/cancel. Session ownership, settings/lifecycle fences and existing incoming-only source/manual precedence apply. No restart or settings-save historical sweep.

Consumers: Core47 / Local assistance16 / Mail UI62 / CLI41 / Quality62. Schema29, fixtures0/3 and protocol1 unchanged.


## D083 — daily mail-count query (CLI40)

The existing generic call exposes the same read-only Calendar counts: `fm --json [--account ID] call calendar_mail_activity --params-file range.json`, where the file contains `{"from":"2026-09-01","to":"2026-10-01"}`. Omit account for combined active accounts. Dates use the saved Calendar display zone and an exclusive end; 1–42 days are allowed. Results include each day's received/sent counts and explicit cached-only coverage. CLI uses the shared engine and does not start provider acquisition. Consumers: Core46/Calendar26/Mail UI59/Quality60; schema29/fixtures0,3/protocol1 unchanged.

## D077 — Drafts parity (CLI39)

CLI Drafts listings, searches and account counts share Core44's accepted-revision filter with the GUI: an unchanged accepted draft is absent, a newer revision is present, and in-flight/unknown work stays visible. Existing authored-history and submission commands preserve their exact identities and explicit recovery semantics. GUI startup acquisition does not change CLI ownership or lifetime.

Core44/Gmail24/CLI39/Mail UI55/Quality55/schema29; fixtures0/3 and protocol1 unchanged.

## D073 — shared Inbox and draft status (CLI38)

`mail list --folder inbox` and account Inbox counts exclude archived and sent-only mail. Explicit All/received queries retain archived incoming conversations. Existing archive/trash/restore command grammar, exact targets, revision checks and delivery lookup remain unchanged. Google draft status optionally includes `baseline.content_matches`; absent is unknown, true is decoded equivalence and false is a content difference. `fidelity_warning` reports the latter; both raw hashes and conflict authority remain available.

Consumers: Core43/Gmail23/Mail UI50/CLI38/Quality50; schema29, Calendar25, Local assistance14, fixtures0/3 and protocol1 unchanged. Whole-area readiness remains 0/9.

## D072 — account and model feedback (CLI37)

`fm --account ID account delete-archive --expected-revision N --request-id ID` uses the shared permanent local deletion boundary. Require explicit scope and a positive revision before storage; replay the same request to recover its result. `account-update --mail-retention 1_month|3_months|1_year|all` optionally saves initial history coverage with existing name/signature revision checks. Omission preserves the choice; it cannot change after acquisition starts.

Consumers: Core42/Gmail22/Local assistance14/Mail UI49/CLI37/Quality49, schema29; Calendar25, fixtures0/3 and protocol1 unchanged. Whole-area readiness remains 0/9.


D070 / P2-UI3 (CLI36): generic `preferences show` and `preferences save` expose `calendar_account_visibility`, accepting `all` or `current` with existing revision/request identity checks. The stored choice follows the current GUI account and does not narrow ordinary explicit CLI Calendar queries. Existing `selected_calendars` and unrelated preferences survive edits and old-engine round trips. Consumers: Core41/Calendar25/Mail UI48/Quality48; Gmail21/schema28/fixtures0,3/protocol1 unchanged.

D067 / P2-UI3 outgoing presentation. `google sent [--offset N] [--limit 1..100] [--conversation-id ID]` lists retained outgoing submissions before acquisition. `google sent-message --request-id ID` reads the frozen plain text, recipients and attachment metadata. Both require explicit account scope and run locally without provider calls. Existing send/status/reconcile/abandon commands remain the authority for delivery state and recovery. Consumers: Core40/Gmail21/Mail UI45/CLI35/Quality45, schema28, fixtures0/3 and protocol1.

D065 / P2-UI3 (CLI34) implements durable draft-discard Undo with exact revision/lifecycle checks, captured authored buffers and unfinished recipient input, shared CLI/GUI recovery and grouped six-second toast Undo. Schema28 adds the snapshot journal and recipient-input storage; fixtures0/3 and protocol1 stay unchanged. Capture/upload continues verifying retained bytes before use. Scoped Windows Core, engine, CLI and native checks, actual schema27→28 migration/refusal/reopen, strict lint and configured relocated package146 pass. The full prototype goal and provider-cleanup/Calendar-Undo/pending-Sent gaps remain open. See D065 and the prototype execution record.

D065 CLI adds optional `draft discard --buffer-file <JSON>` (object, ≤768 KiB), `draft discard-status --request-id <discard>` and `draft undo-discard --draft-id <id> --discard-request-id <discard> --expected-revision <discarded-revision> --request-id <undo>`. These use the shared store; status does not mutate or contact Google.

D064 / P2-UI3 adds the prototype's six-second GUI Send Undo interval and shared action toast. The client saves first, binds the pending action to the original account/lifecycle/authored revision, and starts no Send command during the interval. Undo, GUI close or disconnection cancels pending actions and preserves drafts. Navigation may continue; a late result cannot replace a different editor. Dispatch rereads the same draft/current Google revision and uses the unchanged exact Core Send protocol; it never automatically resubmits an uncertain result. CLI33 exposes an optional six-second pre-request cancellation interval. Mail UI42/Quality42, Core38/Gmail19/schema27, fixtures0/3 and protocol1. Scoped Windows native/CLI checks, explicit Ctrl+C cancellation, strict lint and configured relocated package135 pass. The GUI test uses the actual MailView, closes/reopens its composer, waits the real six seconds, and verifies lifecycle, disconnect, late-read and close cancellation. The previous package115 is preserved. Draft-discard, Calendar-delete Undo and full pending-Sent presentation remain required for prototype parity.

`fm --profile PROFILE --account ACCOUNT google send --undo --draft-id DRAFT --expected-revision REV --expected-google-revision GREV --request-id REQUEST` waits six seconds before connecting. Ctrl+C returns `cancelled` without creating an engine or invoking Send. Omit `--undo` for the existing immediate exact-request behavior. The option is client-side and adds no fields to the Core request. On Windows the CLI registers its signal listener and clears its own inherited ignore-Ctrl+C attribute before advertising the interval; failure to enable cancellation prevents dispatch.

D063 / P2-UI3 adds explicit From-account transfer with source revision and Google revision checks, verified retained content, destination ownership and durable exact-request replay. A move preserves the original local history and waits for the destination Google draft readback before removing the unchanged original Google draft. Unknown deletion outcomes only reconcile the same request; explicit retry starts a new deletion attempt only after a known rejection. Account lifecycle changes fence cleanup. The composer and CLI share transfer/status/retry; source provider IDs never become destination IDs. Schema 27 adds the bounded transfer journal and preserves existing receipt bytes. Core 38/Gmail 19/Mail UI 41/CLI 32/Quality 41, fixtures 0/3 and protocol 1. Scoped Windows Core/provider/native/CLI, actual migration/refusal/reopen, strict lint and relocated package115 checks pass. Full prototype alignment and other-host acceptance remain open; package94/schema26 is preserved in ignored relocation94.

D062 / P2-UI3 adds serial multi-file composer input and bounded cached-recipient suggestions with account attribution. Preserve exact attachment retry receipts, captured draft ownership, unsaved edits and selected routing. Suggestions are read-only until chosen; schema 26, fixtures 0/3 and protocol 1 are unchanged. The D062 decision and [execution record](../research/prototype-september-15.md) own the boundary and pending verification. `fm --account ACCOUNT mail recipients QUERY [--exclude EMAIL] [--limit 6]` exposes the same recent cached suggestions; repeat `--exclude` for existing recipients. It does not create contacts or modify a draft.

D061 / P2-UI3 adds account-local label creation, descriptions and removal plus explicit Google name merging/creation. Removal clears local membership and mappings, retaining historical receipts; it does not delete Gmail labels or mail. Descriptions use the existing assistance settings and stay local. Schema 26 journals provider creation before dispatch; unknown outcomes require inventory reconciliation, with no blind retry. Unmapped local memberships wait for an explicit link/sync before provider delivery. The decision and [prototype execution record](../research/prototype-september-15.md) own the detailed scope and evidence.

D060 / P2-UI3 adds `assist discover --provider ollama|openai_compatible` with the existing endpoint override. Settings and inference use the same provider selection as the GUI. Compatible discovery returns model IDs with an empty digest. Schema 25 preserves prior data and refuses older engines; protocol 1 is unchanged. See Local assistance 12.

P2-UI3 `mail source <id> --source-version <hash>` reads the same bounded retained message source and recipient/HTML preview as the native reader. Obtain the version from `mail read`; stale versions fail. The command starts no Google work and never marks reviewed. `google connect` and restore accept optional Mail/Calendar access choices through the shared sign-in boundary.

P2-UI3 `google connect` and `google restore` accept `--mail-access none|read|write` and `--calendar-access none|read|write`, defaulting to the existing write request. These pass the same typed `access` object as the native connect modal. At least one service is required; verified provider scopes remain the authority for actual capabilities. Shared preferences expose `sync_every_minutes` and `google_sync_services`; Calendar writes accept the editor options defined in Calendar 22.

D059 / P2-UI3 adds `conversations --folder received` for the same non-Trash incoming-member scope used by native Inbox. Existing `inbox` keeps Google INBOX meaning; `all`, `sent` and `trash` retain their meanings. Shared preferences accept Terminal and legacy Harbour. Schema 24 and protocol 1 are unchanged.

D058 / P2-LLM2D limits all assistance to received mail. Reader queries expose the actual sent flag; sent targets fail before inference and historical sent results remain stored but hidden. Combined failures display once; null/empty tool calls are accepted as no tools, and truncation has its own error. Schema 24 preserves prompt/account choices while expanding the former default response allowance to 4,096 tokens. See Local assistance 11 and its scoped execution evidence.

D056 / P2-LLM2B exposes the complete editable Ollama JSON request: system/user messages, all five variables, full response schema and generation options. No hidden envelope or schema is injected. Schema 23 archives and wraps prior prompt text, preserving model/account choices. Application output/action validation remains independent of editable instructions. Consumers: Local assistance 9/Core 32/Mail UI 34/CLI 26/Quality 34; fixtures 0/3/protocol 1 unchanged. See [the current request contract](local-ai.md) and D056 evidence.

State: Draft · Revision: 41 · Updated: 2026-09-22 · Implementation: D086 implemented; scoped Rust/native/engine-CLI and signed Mac relocation/startup checks pass. Broader post-arrival scroll-restoration verification remains open. Schema29 and whole-area readiness0/9 unchanged.

D055 / P2-LLM2: one shared model and visible template (`{{labels}}`, `{{messages}}`, `{{partial}}`), combined per-message output, default-on prospective summaries/automatic single-label classification, manual precedence, and summary list previews. Schema 22 fences settings and assisted delivery; fixtures 0/3/protocol 1 unchanged. This supersedes conflicting D051–D053 delivery defaults below; historical evidence remains scoped to its original version. See the D055 decision and Local assistance 8.


D054 exposes google sync --refresh --account through the shared google_sync_refresh command; it starts/wakes the explicit session, and status includes persisted last_mail_sync_at. Existing sync ownership and bounded --seconds remain the CLI lifetime boundary.

D050 makes `google sync` use built-in Google limits with no quota flags. Complete optional four-value overrides remain for verification and older clients. Default and explicit equivalent budgets share engine ownership. Generic preferences commands expose validated `mail_date_grouping` (`grouped`/`plain`).

D040 adds `google connect --email ADDRESS` and an optional `--email` on restore, sharing the app registration and native engine flow. `--client-config` remains an optional developer override; users of configured builds do not need it. `google setup-status` reports registration availability without opening a browser or accessing Google/vault credentials. Existing explicit live-profile/account/revision requirements remain. JSON status never contains credentials or authorization URLs; existing cancellation, restoration and browser isolation behavior is preserved. Consumers: Platform 15/Gmail 13/Mail UI 26/CLI 21/Quality 24 with Core 27/Calendar 20/Search 5/schema 20 and fixtures 0/3/protocol 1 unchanged.

D039 adds fm calendar search QUERY --from YYYY-MM-DD --to YYYY-MM-DD [--limit 1..100] against the global calendar_search query and selected-calendar preferences. It returns the same permitted occurrences, date range, coverage and has_more as the GUI, without provider acquisition or mutation. The general call interface can provide explicit calendar identities. Consumers: Core 27/Calendar 20/Mail UI 25/CLI 20/Quality 23; schema 20, Platform 14/Gmail 12/Search 5, fixtures 0/3 and protocol 1 unchanged.

Revision 18 applies D037/P2-06C. Add a bounded explicit provider occurrence read to resolve a selected account/calendar/series/typed original start, with an optional verified instance hint. Extend the existing Google Calendar write request with occurrence identity and expected master version; actual instance ID/ETag remain the target/version. Reuse operation/status/resume/reconcile/abandon and exact request-ID semantics. Lookup alone performs no mutation and cached queries remain independent of Google. Distinguish read-only sparse cancellation evidence, unresolved/bounded lookup, stale versions, observed convergence and provider acknowledgment. Ordinary v1/v2 requests and receipts keep their meaning. Consumers: Core 25/Calendar 18/Mail UI 23/Quality 21/schema 19; Gmail 12/Platform 13/Search 5 and fixtures 0/3/protocol 1 unchanged.

The isolated actual-process CLI account walkthrough `cli-accounts-04` passes in 7.282 s; paired native `native-accounts-06` passes in 14.265 s with independent retained-source, authored-draft, lifecycle and provider-counter checks. Final native CTest passes 18/18 after a Send test fixture correction for known-account navigation; no runtime guard was relaxed. Earlier failed attempts remain evidence, and these generated-account results do not authorize or verify real-account removal, restoration or import. Interim evidence: [P2-01B manifest](https://github.com/yegors/fuckMail/blob/8155e998d5957e60c491475df7877553db2b453b/docs/research/assets/phase-2/p2-01b-interim/logs-manifest.json).

Revision 17 applies D036/P2-01B paired account workflows: explicit archive discovery, removal preview, revision/request-bound removal, cached removal status and same-subject Google restoration. Use the same Core/engine boundary as native Settings. Exact historical replay is inspection, not authority to delete a restored credential or resume archived prepared work. Cached archived mail/draft/source/attachment/Calendar/operation queries start no HTTP and never mark reviewed. Keep system-browser OAuth in ordinary builds; any browser-launch bypass used by paired synthetic verification must be default-off and tied to an isolated account fixture. CLI completion/status distinguishes account removal from credential cleanup and OAuth restoration from sync ownership. Consumers: Core 24/Gmail 12/Calendar 17/Platform 13/Mail UI 22/CLI 17/Quality 20/schema 18; Search 5, fixtures 0/3/protocol 1 remain unchanged.

Revision 16 applies D035 while retaining existing Google Calendar command shapes and explicit account/JSON request input. fields.attendees omission preserves guests; a supplied string array defines the removable guest set and empty removes all removable guests, with shared validation and protected identities. Status/history/reconciliation distinguish provider-response evidence from terminal observed-state reconciled evidence and unverified notifications. Unknown attempts are never replayed automatically. Consumers: Core 23/Calendar 16/Mail UI 21/Quality 19/schema 17; fixtures 0/3/protocol 1 unchanged.

Revision 15 pairs exact-source authoring assessment, explicit plain-text copy, supported adoption and remote-draft deletion/recovery with the native UI. Every command requires explicit profile/account scope; identifiers, source revisions, conversion choices and request IDs are validated before connecting. Assessment/copy and retained status remain local; adoption, deletion and reconciliation require an owned Google session. Copy uses explicit primary/plain_text/attachments/omit policies and normal autosync. Exact retries preserve their original receipt and never manufacture mutation certainty. The executable grammar is recorded in the local protocol reference as implementation lands. Consumers: Core 22/Gmail 11/Mail UI 20/Quality 18/schema 16; fixtures 0/3/protocol 1 unchanged.

Revision 14 adds the paired D032 account-scoped boundary: `google drafts-refresh` begins/resumes inventory only through an existing owned sync lifetime; `google drafts-status`, existing `draft list/show`, bounded `draft sources`, and revision-bound `draft source` use cached state/local exports. CLI and native controls share the same provenance/refusal and source identity rules. Validate bounded IDs, positive source revisions and paging before opening a profile; a caller cannot convert an imported snapshot to authored work through generic save/capture/Send commands. Exact wire fields and executable grammar are recorded with implementation. Consumers: Core 21/Gmail 10/Mail UI 19/Quality 17/schema 15; Calendar 15/Platform 12, fixtures 0/3/protocol 1 unchanged.

Revision 13 adds exact-account explicit Send, cached operation status, read-only reconciliation and revision-bound abandonment of proven-undispatched prepared work under D029. Execution/reconciliation requires an owned account-sync lifetime; status and local abandonment start no HTTP. Validate positive local/operation revisions, nonnegative Google revision and bounded request/draft IDs before connection. Consumers: Core 20/Gmail 9/Mail UI 18/Quality 16/schema 14; fixtures 0/3/protocol 1, Calendar 15/Platform 12 unchanged. Exact grammar follows the shared implementation boundary.

Revision 12 adds exact-account Google draft synchronization, cached status, read reconciliation and explicit current-revision conflict replacement beside the same native workflow. Network operations require an explicit owned sync lifetime; cached draft/status queries never start HTTP. Requests retain draft/revision/request identity, including unknown outcomes and restart; no command automatically repeats unknown creation or claims a missing twin was sent. Sending remains the next paired packet. Consumers: Core 19/Gmail 8/Mail UI 17/Quality 14, schema 13/fixtures 0 and 3/protocol 1; Calendar 15/Platform 12 unchanged. Exact command grammar is recorded alongside executable implementation.

Revision 11 adds `draft attach --draft-id ID --file PATH --expected-revision REV --request-id ID` under explicit account scope. It invokes the same local import/selection receipt as the native composer, retaining bytes before success and resolving matching replay before source-file access. Existing save/capture requests and receipts remain compatible while the engine moves content verification off the Store writer. Import never sends mail or creates a received message. Consumers: Core 18, Mail UI 16, Quality 13, Gmail 7/MIME, Calendar 15 and Platform 12; schema 12/fixtures 0 and 3/protocol 1. Executable argument tests and paired process readback cover this implemented command boundary.

Revision 10 adds cached label pagination/explicit category binding, exact mail request delivery lookup and live Calendar activity/resume/abandon alongside the same GUI boundaries. Cached queries start no network work. Explicit label refresh and Calendar execution retain account scope and owned sync lifetimes; unknown Calendar work is reconciled rather than replayed. Preferences may retain the four confirmed Console limits without starting sync automatically. Consumers: Core 17, Gmail 7, Calendar 15, Mail UI 15, Platform 11 and Quality 11; schema 11, fixtures 0/3 and protocol 1. Exact command grammar and tested coverage follow implementation.

Revision 9 concretizes the D023/Core 16 boundary: `mail filing ID --action archive|trash|restore` freezes the queried conversation member IDs/revision, with optional stable `--request-id` and explicit `--expected-revision`. Its local receipt reports pending delivery; `mail delivery` reads bounded pending/failure state. `mail list --folder trash` is explicit and ordinary folders exclude Trash. `google calendar-write --params-file FILE` submits one frozen conditional operation through an already active account sync session; `google calendar-reconcile REQUEST_ID` reads provider state without repeating a mutation, and `google calendar-operation REQUEST_ID` reads the retained local outcome. Google commands require explicit profile/account scope. Closing a requesting client cancels provider work while allowing its admitted outcome receipt to finish. Existing synthetic calendar-write/ack remain distinct. Consumers: Core 16, Gmail 6, Calendar 14, Mail UI 14, Platform 11 and Quality 11; schema 10, live fixture 0, synthetic fixture 3 and protocol 1.

Revision 8 prepares D020's Google-account pilot alongside the GUI. Grow real account authorization/reconnect/disconnect, sync/progress, mail state/draft/send/attachment and Calendar read/write/RSVP coverage as each shared engine workflow lands. Exact command names, fields and exit codes are agreed in P2-00 and published with implementation, not invented here. Pure read/query commands never mark mail reviewed, launch hidden login or activate ongoing background work. Explicit login can use the shared browser/vault path; noninteractive credential failure returns an actionable error without plaintext fallback.

Every mutating operation has explicit account/calendar/draft scope and a stable durable identity. Structured results distinguish locally accepted/pending, provider-acknowledged, denied, quota-paused, conflict and acceptance-unknown states; send retries cannot convert ambiguity into duplicate mail. Calendar invitation effects and destructive/revocation scope are explicit. GUI and CLI share the same draft revisions, source snapshots, quotas and owned engine lifetime; tests compare independent Google state as well as local readback. Routine logs and committed evidence contain no credentials or private mail/calendar payloads. Consumers: Core 14, Gmail 4, Calendar 12, Mail UI 14, Platform 11, Quality 11; existing schema 7/fixture 3/protocol 1 stay unchanged until the next boundary is implemented. See [Phase 2 plan](../plans/phase-2.md).

Revision 7 adds `mail list --kind all|people|automated|unknown --exact-sender ADDRESS` and `mail sender ADDRESS`. The latter queries at most three recent conversations and the exact conversation total in the selected account; it never marks mail reviewed. Exact address filtering and sender history share trimming/case-insensitive identity semantics, while existing `--sender` remains a display-name/address substring filter. JSON list results add conversation kind and full filtered review/held/done group counts; member read results add kind. Consumers: Core 13, Search 5, Triage 6 and Mail UI 12; additive protocol 1, schema 7, fresh fixture 3. Migrated fixtures and command receipts retain their original revisions/data.

Revision 6 adds account-scoped `draft list` and `draft create --mode new|reply|reply_all|forward`, independent `--draft-id` selectors for show/save/discard/capture/history, and editable subject/recipient files. Legacy positional conversation selectors retain their meaning; supplying both selectors fails. `draft save --recipients-file` reads a bounded JSON object with optional `to_recipients`, `cc_recipients` and `bcc_recipients` arrays of `{name?,email}`. Omitted arrays preserve saved recipients and empty arrays clear them. First-save legacy CLI requests bind the member returned by `draft_get`. Existing explicit request IDs, optimistic revisions, exact capture replay and no-network semantics remain. Consumers: Core 12, Mail UI 11, MIME and Platform; schema 6/protocol 1/fixture 2. See [P1-08 local composition](../local-protocol.md#drafts) for the draft methods and fields.

Windows verification passed all three CLI argument tests and the [21-call real CLI composition check](https://github.com/yegors/fuckMail/blob/885de67d2d3807f73374ea2975a63fb9476d6e7f/docs/research/assets/phase-1/p1-08-cli-composition.json) in 18.534 seconds. The latter starts with the preserved schema-5 CLI, creates a first-save receipt, migrates through the schema-6 engine, and repeats that same command with the updated CLI. Its result and stored fingerprint/JSON remain exact. New first-save replay, independent draft creation/save/list/search/discard, Reply-all/Forward source routing, immutable unknown capture replay after later edits and an engine restart, and unchanged source-mail rows/counts also pass. The report binds both old and new binaries by SHA-256; final relocated-package and cross-platform checks remain separate.

Revision 5 records the implemented local CLI boundary: grouped/typed mail query, explicit review/Done/Delay/undelay, signature/formatted/retained-attachment reply save, discard/capture history, local preferences/account metadata, combined calendar and deterministic local fake create/edit/self-RSVP commands. Consumers: Core 10, Mail UI, Triage, Search 3, Calendar and Platform. Exact grammar is `fm --help` and [local protocol](../local-protocol.md); protocol 1 remains additive, schema 5/fresh fixture 2 preserve old fixture profiles. Live provider/credential/model execution remains later work.

Revision 4 applies D014 and the Phase 1 plan: query/action, reply/capture and combined-calendar parity grows alongside the updated GUI. Consumers: Core, Mail UI, Triage, Search, Calendar and Platform. Current runtime remains schema 4, protocol 1 and fixture 1; this revision does not invent executable command names or mark planned commands implemented.

Revision 3 records the next Phase 0 integration checkpoint. The draft response adds authored HTML and retained attachment IDs; structured draft_save can preserve or explicitly clear them. Existing plain-text commands clear stale HTML when changing the body. Lost-response replay survives an actual test-engine kill. Consumers: Core, Qt UI and MIME; protocol/fixture revisions remain 1.

Revision 2 records D012’s implemented experimental slice. The implemented partial `fm` CLI is documented in [local protocol](../local-protocol.md); `fm --help` is the current command reference. It shares the engine with Qt and has no Qt dependency. Consumers: Core, Mail UI and Platform. Future command descriptions below retain their proposed status.

Revision 1 applies D009: a small real CLI ships in the first usable GUI slice; coverage grows with Google workflows. Legacy transport/feed commands were deferred then; they are now out of scope. Affected consumers: Core, Mail UI, Gmail, Calendar, Local assistance and Platform.

Owner: I — CLI. A full, feature-rich CLI is required alongside the desktop UI on Windows, macOS and Linux. Third-party agents use it for external workflows. Exact command names and schemas below are proposals; this is not a runnable command reference.

## Build boundary

| Boundary | Responsibility |
| --- | --- |
| Owns | Command discovery/help/completion, argument and structured-input handling, human/JSON output, exit/error semantics and compatibility tests |
| Consumes | Core's versioned account-scoped queries, commands, capabilities, revisions, operation journal and bounded event/change stream |
| Produces | Explicit user/caller commands and machine-readable results with IDs, coverage and operation outcomes |
| Independent start | Fake local engine and versioned fixtures; no desktop display, live provider, model or GUI automation needed |

The GUI and CLI are peer clients of the same local core. Providers, search, calendar and category policies have one implementation and one store. CLI is a public product interface; direct SQL, scraping the UI or changing private database files is not its API.

## Phase 1 boundary — implemented local slice

CLI coverage lands with each shared GUI workflow. P1-00 records the supported request/output shapes, capabilities, bounds and command grammar in `fm --help` and [local protocol](../local-protocol.md); the synthetic demo data is described in [fixtures/README.md](../../fixtures/README.md). A Windows 19-call live CLI-to-engine smoke exercised synthetic account/filter/actions/preferences, formatted attachment reply/capture replay/discard/history and local calendar creation replay. This is bounded local evidence; native UI parity and other-host delivery remain integrated gates.

- **Mail queries/actions:** typed sender/status/category/attachment/date filters, stable conversation grouping in Inbox and Sent, explicit count units and honest coverage. Reveal matching-member IDs. `mail review`, `mail done`, `mail delay` and `mail undelay` expose explicit review, atomic snapshot Done (read and clear stars), and untimed Delay/undelay using the same journal and stale-revision rules as the GUI. Fetching source content is separate from the GUI's user-open-triggered review action; an external reader cannot silently mark mail reviewed.
- **Replies:** `draft save/show` reads/saves plain or formatted authored content, per-draft editable signature snapshots and selected retained attachments; explicit discard and local MIME capture with replayable operation outcomes and bounded capture inspection linked to the original conversation. First-save From/signature snapshots and subsequent explicit signature edits participate in the saved revision and captured-content guarantee; preference changes cannot rewrite an existing reply. A local capture never claims provider delivery. The CLI accepts content through bounded files/stdin and preserves the draft on uncertain capture. `draft history --unresolved-only` filters prepared/in-flight/acceptance-unknown operations before paging, so newer accepted history cannot hide an old unresolved capture; known rejections remain distinct. New mail, forward and local-file attachment import can follow in Phase 2; live submission is outside Phase 1.
- **Calendar:** query the same combined account/calendar scope, permitted event details and bounded occurrences as day/week/month/agenda and the mail timeline. Preserve each event's owning account/calendar on output and require that target for a mutation. Add bounded nonrecurring create/edit and self-RSVP through deterministic fake acknowledgement; recurring-series edits and live writes stay deferred. `calendar-operations --limit 30 --cursor '<returned continuation JSON>'` inspects the durable account journal, with pending/failed operations before acknowledged operations. Forward the returned continuation object unchanged; each page is bounded and restarting inspection does not execute a write.
- **Settings/state:** `preferences show/save`, `account-update` and `status` expose typed local settings, metadata and diagnostics. Account names/signatures have metadata revisions; calendar/timeline selection and dates are app-wide, independent of mailbox selection. Mouse gestures, animation and layout controls need no artificial CLI equivalent. Do not add credential setup, fake OAuth completion, model downloads or legacy-provider commands to fill out the prototype's pictured settings.

Each packet verifies GUI-to-CLI and CLI-to-GUI readback where applicable, restart/replay, foreign-account rejection and stale revisions. Calendar scope/permissions apply before serialization in either interface; query filtering cannot fabricate all-clear or offline-complete status. Retain explicit client lifetimes and last-owner shutdown. Extend the small real CLI incrementally; exhaustive grammar, long-lived streaming and bulk tooling remain later product work.

## Required functional coverage

Coverage below is the intended product surface, delivered incrementally. Phase 0 provides account selection, bounded conversation list/search/read, explicit flag/category actions, durable human drafts/capture and basic calendar queries alongside the GUI. Phase 1 expands the local workflows above, including a bounded calendar edit slice; setup/sync/live submission arrive with their corresponding Google workflows. Do not finish all command grammar, streaming, bulk tooling or assistance commands before expanding the GUI. Both interfaces must cover the agreed everyday Google workflows. IMAP/SMTP and external calendar feeds are out of scope, so the CLI has no commands for them.

| Area | CLI capabilities |
| --- | --- |
| Accounts and setup | Discover/select accounts, inspect provider capabilities, configure accounts/categories/local assistance, initiate explicit authentication and report actionable setup state |
| Mail and search | List/search conversations, inspect specific messages/headers/bodies and source representations, fetch/save attachments, expose full offline/index coverage |
| Mail management | Read/unread, star/unstar, labels, category mappings, archive/trash/restore and explicitly scoped bulk operations with provider-specific semantics |
| Human-authored mail | Create/edit/reopen drafts, recipients/signatures/attachments, reply/reply-all/forward and explicit submission; preserve uncertain-send handling |
| Calendar | Google calendar visibility/permissions, day/range/agenda/search, event edits, recurrence, RSVP and shared calendars |
| Optional assistance | Inspect/configure mode and runner availability, request allowed local classification/summaries, accept/correct results and cancel an explicitly scoped batch |
| Operations | Explicit sync, inspect progress/errors/pending operations, retry or cancel where valid, coverage, version/capability/schema discovery and useful diagnostics |
| Change consumption | An explicitly invoked foreground watch/change stream with cursors, reconnection and bounded output for external automation |

GUI-only presentation controls need no artificial command equivalent. Every applicable core workflow is eventually reachable through CLI; a read-only exporter is not the final product. A useful partial CLI is explicitly acceptable at the first milestone and grows with the GUI. Unsupported provider capabilities return explicit errors, not invented parity. MCP servers, hosted-agent integrations and built-in scheduling are not prerequisites; external tools can wrap the CLI.

## Interface rules for agents

- Provide versioned JSON input/output and line-delimited JSON for streaming/batches; stable account/conversation/message/event IDs, bounded pagination and documented filtering/field selection. Keep human diagnostics on stderr and structured results on stdout.
- Document consistent exit codes and distinguish invalid input, authentication required, unavailable capability, stale revision, partial/batch failure and unknown provider outcome. Do not silently prompt or launch a browser in noninteractive operation; an explicit setup/auth command may do so.
- Writes require explicit account and target scope, with expected revisions and client request IDs where appropriate. Snapshot bulk targets; new arrivals must not inherit an earlier command. Offer preview/dry-run for applicable bulk/category changes.
- Report local acceptance, queued/offline, provider confirmation and acceptance-unknown distinctly. Provide bounded waiting/progress/cancellation. Retrying a request ID can deduplicate local intent; it does not promise exactly-once delivery.
- Accept bodies and structured payloads from files/stdin, so callers need not put message content or secrets in shell arguments. Do not mix binary attachment bytes into JSON; use an explicit output destination or stream mode.
- Help, discoverable capabilities/schema and examples ship with the CLI. Record compatibility/version rules before agents depend on command output. File/terminal content is data, not executable instructions.

Configured automation uses explicit commands without a mandatory GUI approval for every ordinary write. Callers remain responsible for the authority and scope of their requested actions. Keep provider permissions and the same operation validation used by the GUI.

## AI scope and authorship

Built-in AI remains local classification/summarization only; it has no drafting or sending tool. External agents can consume selected mail/calendar data and drive the CLI under the user's control. The app does not choose an external agent's model or silently route mail to one.

The existing “humans write email for humans” preference still governs outgoing content. CLI draft/send capabilities support human-authored mail and explicit submission; adding an agent interface does not introduce an internal writing assistant. Text provenance cannot be reliably inferred by the CLI. External caller policy, including whether a particular workflow needs human review, belongs to that integration; do not claim this interface can prove authorship.

## Headless execution and lifecycle

Proposed architecture: an on-demand local engine owns the store/journal and serves both GUI and CLI. CLI commands work with the GUI closed and without loading Qt Quick/WebEngine or requiring a display server. GUI and concurrent CLI clients must not run independent migrations, provider submission loops or conflicting writers.

Closing the GUI stops its sync/inference/reminder session. A separately invoked CLI command or foreground watch has its own explicit lifetime and can run without the GUI. End its work on completion/cancellation; when no client/job is active, stop the engine. Durable queued intent can remain on disk without continuing execution. Do not install an always-running daemon, startup service or scheduler by default.

Use a private authenticated local transport with OS-appropriate access checks, protocol negotiation and single-engine ownership. A local endpoint name alone is not authorization. Locked/unavailable credentials return an actionable error; headless mode must not fall back to plaintext credentials.

## Acceptance gates

- Exercise implemented Google mail, labels, human draft/submission, calendar and sync through CLI-only fixtures on every required platform, including Linux with no display session. Initial OAuth/keyring setup has its own explicit prerequisites.
- Run the same operations through GUI and CLI and compare resulting state, coverage, flags, category overrides and provider outcomes.
- Validate JSON schemas, pagination, Unicode/multiline input, binary outputs, exit codes, noninteractive failures, partial batches and cancellation.
- Race GUI edits and multiple CLI clients, restart the engine and repeat a client request ID; preserve drafts/newer intent and never blindly resubmit an uncertain send.
- Close the GUI during an independent CLI command, end all clients and verify lifecycle behavior. Ordinary installation must make the CLI discoverable from the terminal.

Related: [Core](core-storage.md), [Platform](platform.md), [quality](quality.md), [CLI research](../research/cli-and-local-engine.md). Open: executable/command naming, first schema/version, stream protocol, auth provisioning for unattended use and packaging-specific command discovery.

## D052 / P2-LLM consumer boundary

Revision 23 consumes [Local assistance 6](local-ai.md): shared discovery/settings/jobs/results/correction commands, schema-21 storage and manual precedence, bounded owned Ollama work, native/CLI parity and scoped acceptance. Runtime implementation/verification is in progress; fixtures 0/3 and protocol 1 remain unchanged. Earlier evidence retains its original revisions.

## D061 label commands

Use `--account` with `labels list`, `labels create NAME --id ID --request-id REQUEST`, `labels describe ID DESCRIPTION --expected-description OLD --request-id REQUEST`, `labels remove ID --expected-name NAME --expected-provider-label-id GOOGLE_ID --request-id REQUEST` (omit the provider option for an unmapped label), and `labels sync --request-id REQUEST`. Description comparison can use an empty string. Local operations are durable and replayable.

`labels sync` requires an active Google sync session and mail editing permission. It waits on the same owned `google_labels_sync` job with `google_labels_sync_status`; Ctrl+C requests `google_labels_sync_cancel`. Start takes request_id; status/cancel take job_id and the same account/session. The result distinguishes running/complete/failed/cancelled; unknown provider creates are checked from inventory before any later creation.
