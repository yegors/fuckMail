# Synthetic desktop fixture — revision 1

This is a disposable, deterministic local development profile, never real mail. The Rust generator in `crates/fm-core/src/fixture.rs` is its source. Opening a new empty profile seeds it transactionally. Opening an existing profile never resets authored drafts or actions. Provider IDs intentionally collide across accounts; local identities and all commands include account scope.

Accounts: `windscribe` (synthetic Google Workspace) and `personal` (synthetic consumer Gmail). Each has 110 conversations, with a long 32-member thread, ordinary human mail, automated HTML-only mail, mixed read/star/category state, archived unread mail, and an explicitly missing and partial attachment. Dates are fixed to the week 2026-09-07 through 2026-09-13; fixture today is **2026-09-11**. The app must label the profile and clock as synthetic. No live provider/model connection or automatic read-on-open occurs.

## Core API revision 1 (schema 3)

`Store::open(profile: &Path)` acquires an exclusive store lock. `Store::handle(method, account, params)` returns JSON or a structured `CoreError { code, message, details }`. Transport framing, client sessions and authentication belong to the engine. The store validates every account, target, revision and mutation identifier. Account is mandatory except for `accounts` and `status`.

| Method | Parameters | Result |
| --- | --- | --- |
| `accounts` | `{}` | `{accounts:[{id,name,email,provider,unread_count,starred_count,conversation_count,coverage}],fixture_revision}` |
| `status` | `{}` | `{schema_version,fixture_revision,revision,synthetic,fixture_today,capabilities}` |
| `conversations` | `{query?,field?:all/sender/subject/body,category?,folder?:inbox/sent/all,limit?:1..100,offset?}` | `{conversations:[{id,subject,sender_name,sender_email,snippet,latest_at,message_count,unread,starred,revision,categories,message_ids,matching_message_ids}],next_offset,coverage}` |
| `conversation` | `{id}` | `{id,subject,revision,message_ids,categories,messages:[{id,sender_name,sender_email,to,subject,sent_at,text,unread,starred,revision,source_available,attachments:[{id,name,mime_type,size,state}]}]}` |
| `flag` | `{id,message_ids:[snapshot IDs],expected_revision,request_id,read?:bool,starred?:bool}` | Updated `conversation` result. Only supplied member IDs change. |
| `draft_get` | `{conversation_id}` | `{conversation_id,body,body_html,attachment_ids,to,from,subject,revision,attachments:[metadata]}`. A nonexistent draft has revision zero. |
| `draft_save` | `{conversation_id,body,body_html?:string/null,attachment_ids?:[IDs],expected_revision,request_id}` | Same draft shape with incremented revision; empty body explicitly persists an empty draft. |
| `draft_capture` | `{conversation_id,expected_revision,request_id,outcome?:accepted/unknown}` | Local diagnostic: serialize the persisted draft through the shared MIME builder into a durable synthetic Gmail capture; returns state/checksum/private `.eml` export path. Never uses the network or clears the draft. |
| `calendar` | `{from?:YYYY-MM-DD,to?:YYYY-MM-DD}` | `{events:[{id,calendar_id,title,start,end,time_zone,all_day,read_only,location}],from,to,time_zone,fixture_today}`. Half-open range; at most 42 days/500 events. |
| `attachment` | `{id}` | `{id,name,mime_type,size,state,path}`. Path returned only for fully retained, checksum-verified content in this account. |
| `categories` | `{}` | `{categories:[{id,name,conversation_count}]}` |
| `set_category` | `{id,message_ids:[snapshot IDs],category_id,present,expected_revision,request_id}` | Updated conversation; manual label mutation never changes read/star. |
| `view_get` | `{}` | `{account,revision,view:{}}` |
| `view_save` | `{view:{...},expected_revision,request_id}` | Same shape after saving account-scoped presentation state. |

All mutations are atomic with a durable operation response. Retrying the same account/request ID and identical command returns the original response, including after restart. Reusing that ID with changed command content returns `request_id_conflict`; an outdated expected revision returns `stale_revision`. Identifiers are opaque, bounded strings. Unknown request fields are rejected. List pages are bounded, detail threads are limited to 200 members, draft bodies to 256 KiB and view JSON to 16 KiB. The first fixture's longest thread has 32 members.

Search tokenizes Unicode alphanumeric words, treats tokens as prefix matches joined with AND, and supports sender/subject/body fields. Email punctuation divides tokens; `maya@windscribe.example` matches those address segments. Results are ordered newest first with stable ID tie-breaking, grouped into conversations, with exact matching message IDs. Raw FTS syntax is not accepted as query language. Generated summaries/drafts are never indexed. Empty query lists conversations. Binary attachments are not searched or embedded in row JSON.

Coverage counts inventory and actual FTS rows separately. Source/attachment record counts describe database metadata; aggregate retained/verified counts are `null` with verification `not_run` until an explicit full verification exists. The fixture never claims full offline completeness. Individual attachment access verifies size and hash before returning an extension-preserving export path. Intentional missing/partial content prevents claiming full offline availability. Source MIME and HTML are retained as blobs even when the reader displays decoded text. Blob bytes are staged, flushed, renamed, and only then referenced in a SQLite transaction; orphan blobs are retained safely after an interrupted seed. This is crash recovery design and test evidence, not a claim to have simulated hardware power loss.

Calendar results include only the selected account's permitted fields. The fixture does not invent cross-account Busy overlay permission. Read-only events reject mutation because no event mutation method is exposed in this first API. Settings stores only bounded presentation state; no provider/model switches are implemented.

Schema 2 adds the local capture journal. Schema 3 transactionally adds optional authored HTML, retained draft attachment references and Google checkpoint/staging/receipt/label-intent tables. Schema 1 and 2 upgrade tests preserve existing drafts, flags, view state and captured bytes; replay does not submit again. Protocol and fixture revisions remain 1.

Omitted draft attachment IDs preserve the previous selection; `[]` clears it. Omitted HTML is preserved only if the plain body is unchanged: an older plain-text client editing the body clears the stale HTML alternative. Explicit `null` clears HTML. Save rejects cross-account, duplicate, missing and partial attachment references; capture re-verifies retained bytes. The GUI remains a plain-text composer; a real Qt formatting export is tested through the same durable Core/MIME capture path. None of these fields enables received HTML rendering or network sending.
