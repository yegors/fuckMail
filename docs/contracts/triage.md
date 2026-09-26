# Daily triage and mailbox categories

State: Draft · Revision: 8 · Updated: 2026-09-14 · Implementation: Existing local actions and sender-kind projections retain their recorded evidence; P2-LLM integration is implemented (see [Local assistance](local-ai.md)). Whole area remains Draft.

D055 / P2-LLM2: one shared model and visible template (`{{labels}}`, `{{messages}}`, `{{partial}}`), combined per-message output, default-on prospective summaries/automatic single-label classification, manual precedence, and summary list previews. Schema 22 fences settings and assisted delivery; fixtures 0/3/protocol 1 unchanged. This supersedes conflicting D051–D053 delivery defaults below; historical evidence remains scoped to its original version. See the D055 decision and Local assistance 8.


Revision 7 records D051's planned Ollama producer for the existing sender-kind projection and account category catalog. Human/Automated/Unknown is a source-bound, user-correctable annotation, not an address heuristic or provider label. Classification uses each received message separately; existing newest-incoming-member conversation semantics remain. Optional category descriptions refine the existing taxonomy. Suggest acceptance uses the existing snapshot/manual category path; Auto-apply remains gated. Manual corrections, later provider state, read/stars and category mappings retain precedence. Consumers: Local assistance 5, Core, Gmail, Mail UI, Search and CLI. No schema, fixture or runtime change is made by this revision; detailed target behavior and prompts belong to [Local assistance](local-ai.md).

Revision 6 binds D018's All/People/Automated views to explicit source-hash-bound sender kinds, with Unknown available through the shared API. Kind is never guessed from an address, name, subject, read state or category. It follows the newest incoming conversation member, falling back to the newest member only when every member is outgoing. Filtered group counts cover the full matching conversation set before paging and use review first, then held, then done; a mixed unread/starred thread counts only in the displayed review group. Existing unfiltered account held counts still include mixed unread/held threads. Missing/stale annotations do not manufacture an all-clear or change flags. Consumers: Core 13, Search 5, CLI 7 and Mail UI 12; schema 7/fresh fixture 3, preserved migrated fixtures and exact action receipts.

Revision 5 applies D014: read-on-open is confirmed, Done clears unread and stars, and Delay remains an untimed hold. Phase 1 proposes explicit existing-member snapshots, overlapping-flag group rules and labeled conversation/message counts on the shared native GUI/CLI boundary. No timed snooze or local completion state is introduced. Consumers: Core, Mail UI, Search, Gmail, CLI and Local assistance.

Revision 4 records the integrated Phase 0 experiment. Manual category intent takes precedence over accepted assistance intent and provider observations. GUI category Apply/Clear uses the same message snapshot and durable operation response as the CLI. Categories preserve read/star state; configured mappings reject Gmail system labels. Consumers: Core, Gmail, Local assistance, Mail UI and CLI.

Revision 3 records D012’s implemented experimental slice. Synthetic manual read/star/category actions share one journal through both clients; mixed member snapshots preserve independent state. Exact Done/Delayed button mappings remain open. Consumers: Core, GUI, CLI, Gmail and assistance. No classifier can issue action-state commands.

Revision 2 applies D009: the user alone chooses Done/Delayed; classification supplies account-specific human/automation/noise and category/scope labels without changing review/action state. Gmail mappings are first; IMAP mappings were deferred then and are now out of scope. Affected consumers: Core, Mail UI, Gmail, Local assistance and CLI.

Owner: A — Core/storage; E — Mail UI and H — Local assistance consume the policy. These are living specifications of accepted requirements and proposed behavior; exact types, libraries and unresolved details are not committed. See [contract workflow](README.md).

## Build boundary

| Boundary | Responsibility |
| --- | --- |
| Owns | Unread/star meanings and projections, daily scope, configurable categories, manual precedence and allowed filing effects |
| Consumes | Provider message flags/membership, account coverage, explicit member selection, configured mappings and accepted classification |
| Produces | Conversation filters/counts, account totals, snapshot-targeted journaled commands, override and policy revisions |
| Independent start | Message/label fixtures covering Gmail system and user labels |

Related: [Gmail](gmail.md), [local assistance](local-ai.md).

## A finished day

The user always chooses Done or Delayed, regardless of classifier output. Done clears unread and stars on the explicitly targeted existing members; it does not archive or clear future replies. Delay is an untimed star hold. Read-on-open is a user-triggered reading convention, separate from either action. A model may suggest or apply configured category/scope labels under policy, but cannot mark mail read, star/unstar it, resolve it or postpone it. The personal meaning of spam/noise is a classification label, not permission to use Gmail system Spam, Trash or deletion. “Needs my attention” is a tag, never proof that review occurred.

The desired end-of-day state is zero unread messages and zero starred messages in each account's agreed mail scope. Unread means the user has not reviewed the message. Starring an email means delaying an action: it remains outstanding, even if work has begun. Remove the star when that action is complete. The UI must communicate deferred action rather than presenting stars as a permanent favorites collection. This is a personal workflow goal, not an instruction to automatically clear indicators or hide unfinished work.

Proposed daily flow:

1. Open an account's Unread conversation filter and review new mail. A successful user-activated open marks the loaded member snapshot read; show the relevant messages and preserve access to collapsed members.
2. Assign the appropriate configured categories manually, accept a local model's suggestion, or use configured automatic classification/filing.
3. If action remains, use Delay to set an untimed star hold and continue from the Starred conversation filter. Explicit read/unread controls remain available.
4. Write replies personally, complete any other action, then choose Done to clear unread and stars on the targeted existing members, or explicitly remove the hold. Sending a reply alone does not prove all action is complete.
5. Check the account's unread and starred totals. Categorizing, moving, archiving, generating a summary, or changing a filter never clears read/star state by itself.

The app should make remaining work easy to find, including older holds. Propose showing the date a hold was set when known and an oldest-holds sort. For stars first discovered during synchronization, distinguish first observed locally from the unknown original star date. No tasks area, overdue scoring, streaks, or guilt notifications are needed. A legitimate unfinished action remains visible across days.

## Visual meaning

The [visual design brief](../ui-ux.md) makes deferred action explicit through proposed star labels/tooltips, a clear Starred view and honest hold age. Exact copy and visual treatment remain open. Star age does not create a due date, task or scheduled snooze. Starring unread mail leaves it unread; reading starred mail leaves its hold intact. Clearing a star is explicit and does not prove unrelated work is complete.

## Counts and conversation semantics

The UI remains conversation-only. A conversation appears in Unread if any in-scope member is unread, and in Starred if any in-scope member is starred. Show conversation counts in those filters and make underlying message counts available in their detail; either can be zero only when every corresponding in-scope message is clear. A conversation can appear in both filters, but appears once within each.

Proposed Phase 1 combined-list grouping: show a conversation once in To review if any member is unread, otherwise in Delayed if any member is starred, otherwise in Done. A conversation in To review can still carry a hold marker. Done is the presentation of read/unstarred state, not another stored flag or evidence that external work was completed. Keep an open conversation pinned in place until the user navigates away when read-on-open changes its group.

Proposed count labels: account-tab badges and list/group totals count conversations and identify that unit in visible detail/tooltips; unread/starred account totals count underlying messages over the stated synchronized account scope. Counts for filtered results describe that filter and must not replace account totals. A mixed conversation contributes to both independent unread/starred totals even when displayed in only one group. Coverage and pending/failure state qualify any zero-work claim.

Use native provider state: Gmail's UNREAD/STARRED system labels. The category taxonomy uses user labels separately from these states; it must not substitute a local-only read/star system.

Proposed Phase 1 action targets: Delay sets Star on the explicitly selected received member, or the latest received member when invoked from a conversation row. Undelay clears existing stars on the displayed conversation snapshot so an older held member is not left invisibly outstanding. Done clears both unread and stars for that existing-member snapshot in one durable operation. Keep message-level provider state even though browsing is by conversation. New members are excluded from old actions.

Read-on-open is confirmed. Proposed trigger: after a user-activated conversation load succeeds, issue the shared read command for the loaded existing-member snapshot, including collapsed members. Prefetch, background refresh, automatic initial selection/restore, classification, summaries and CLI read queries never trigger it. A later-arriving member remains unread. Record the snapshot and operation receipt so reconnect/replay cannot broaden the action or clear new mail. This convention does not assert that every collapsed member was inspected.

Account totals span its synchronized mail scope, including archived mail and mail in configured category labels. Changing a folder, category, sender filter, or People/Automated filter must not manufacture a zero-work state. Keep scope, incomplete synchronization, and pending/failed provider writes visible. Spam/trash inclusion remains the existing retention-scope decision and must be stated rather than silently changing the meaning of zero. Do not provide an unqualified all-clear while coverage is incomplete or state changes await confirmation. Accounts retain separate navigation and counts; there is no unified inbox.

## Configurable categories using provider storage

First-product intent: distinguish human correspondence from automated senders, mailing lists and user-defined noise, then assign the configured category and scope within that account. The user defines these meanings; human mail is not automatically important and automated mail is not automatically unwanted. Uncertain mail may remain unclassified and visible. Categories are Gmail labels; IMAP folders and other providers are out of scope.

Categories belong to the user's mailbox organization. Each account has configurable category names, short descriptions for classification, and mappings to actual Gmail labels. Example optional categories are People, Automated, Receipts, Newsletters and Work updates; none is a fixed taxonomy. People/Automated is a filter over the chosen mapping, not a separate hidden AI classification system. The user may leave mail unclassified or choose another category.

Allow selecting existing destinations and explicitly creating a label as part of category setup. Support editing category definitions and remapping destinations. Treat provider rename/deletion/permission changes as mapping changes requiring reconciliation; changing a category definition must not silently move historical mail. Provider-backed assignments are the durable categorization state; local IDs, descriptions, suggestions and provenance are supporting metadata. A refresh/reinstall must not lose assignments already stored with the provider.

| Provider | Proposed category behavior |
| --- | --- |
| Gmail | Apply one or more configured user labels to explicitly targeted existing messages. Preserve other user labels. Show mixed membership honestly at conversation level. Filing out of Inbox is a separate configured action; labeling alone does not archive. New thread members are classified independently unless a user-defined policy explicitly covers them. |
| Manual and assisted | Manual and model-assisted actions use the same mappings and durable operations. Missing destinations, read-only access, ambiguous changes and offline writes have visible pending/failure states. Do not create arbitrary model-invented labels. |

When a conversation has members with different labels/categories, display that state and identify the messages affected by a bulk action.

## Mutation contract

Persist read-on-open and explicit read/star/Done/category commands through the same operation journal as other mail edits. Done changes unread and stars atomically for its member snapshot; a partial flag change is not completion. Preserve the user's reading position during refresh and pending mutations. Derive Unread/Starred conversation views from message flags, with account-wide totals over the agreed synchronized scope; include archived and category-labeled mail and mark incomplete coverage. Moving or labeling mail does not complete triage. A command acts on a snapshot of the selected member IDs; a new message does not inherit a previously completed read/unstar command.

Category definitions map to provider destinations per account. Gmail classification may add/remove configured category labels while retaining unrelated memberships; Inbox removal is a separate configured filing action. Missing/renamed destinations and partial thread changes require visible reconciliation.

## Acceptance gates

- Distinguish unread attention from deferred starred action in the UI. Show a recent and an older hold, with known versus first-observed age represented honestly; neither reading nor sending auto-clears a star.
- Open a conversation, verify read-on-open against its loaded snapshot, Delay while taking action, file it, write a reply personally, then choose Done. Verify the independent flags, group/count projections and exact journal replay from the other local client; confirm the resulting Gmail state.
- Add a new member during read/Done processing and leave it unread and untouched by the older command. Failed loads, prefetch, background refresh, automatic restore and CLI queries must not mark mail read. Undelay must clear an older held member in the explicitly targeted snapshot without changing unread state.
- Archive or label a still-unread Gmail message: account Unread still includes it. A hidden category or incomplete import cannot produce a false all-clear.
- Classify a new reply in an already labeled/starred conversation without clearing its unread state, assigning unrelated categories to older members, or clearing another outstanding hold.
- Correct a category while inference is in flight, change it in another mail client, rename/delete a mapped destination, interrupt a move, and restart. Preserve the newer choice and surface unresolved provider actions.

## Open before affected behavior ships

Name and test the proposed read-on-open trigger, action snapshots, group precedence and count units in the Phase 1 packet. Refine old-hold presentation, final count scope including spam/trash, and category setup UX before their affected real-account workflows ship. Category names/descriptions and provider destinations are user configuration, not a fixed taxonomy. Filing and inference must never clear unread/stars to manufacture completion.
