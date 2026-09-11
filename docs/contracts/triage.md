# Daily triage and mailbox categories

State: Draft · Revision: 4 · Updated: 2026-09-11 · Implementation: Phase 0 in progress.

Revision 4 records the integrated Phase 0 experiment. Manual category intent takes precedence over accepted assistance intent and provider observations. GUI category Apply/Clear uses the same message snapshot and durable operation response as the CLI. Categories preserve read/star state; configured mappings reject Gmail system labels. Consumers: Core, Gmail, Local assistance, Mail UI and CLI.

Revision 3 records D012’s implemented experimental slice. Synthetic manual read/star/category actions share one journal through both clients; mixed member snapshots preserve independent state. Exact Done/Delayed button mappings remain open. Consumers: Core, GUI, CLI, Gmail and assistance. No classifier can issue action-state commands.

Revision 2 applies D009: the user alone chooses Done/Delayed; classification supplies account-specific human/automation/noise and category/scope labels without changing review/action state. Gmail mappings are first; IMAP mappings are deferred. Affected consumers: Core, Mail UI, Gmail, Local assistance and CLI.

Owner: A — Core/storage; E — Mail UI and H — Local assistance consume the policy. These are living specifications of accepted requirements and proposed behavior; exact types, libraries and unresolved details are not committed. See [contract workflow](README.md).

## Build boundary

| Boundary | Responsibility |
| --- | --- |
| Owns | Unread/star meanings and projections, daily scope, configurable categories, manual precedence and allowed filing effects |
| Consumes | Provider message flags/membership, account coverage, explicit member selection, configured mappings and accepted classification |
| Produces | Conversation filters/counts, account totals, snapshot-targeted journaled commands, override and policy revisions |
| Independent start | Message/label/folder fixtures; Gmail and IMAP use the same logical policy with explicit capability differences |

Related: [Gmail](gmail.md), [IMAP/SMTP](imap-smtp.md), [local assistance](local-ai.md).

## A finished day

The user always chooses Done or Delayed, regardless of classifier output. A model may suggest or apply configured category/scope labels under policy, but it cannot mark mail read, star/unstar it, resolve it or postpone it. The personal meaning of spam/noise is a classification label, not permission to use Gmail system Spam, Trash or deletion. “Needs my attention” is a tag, never proof that review occurred. Exact Done flag/archive effects remain to define in the UI handoff; ownership is no longer open.

The desired end-of-day state is zero unread messages and zero starred messages in each account's agreed mail scope. Unread means the user has not reviewed the message. Starring an email means delaying an action: it remains outstanding, even if work has begun. Remove the star when that action is complete. The UI must communicate deferred action rather than presenting stars as a permanent favorites collection. This is a personal workflow goal, not an instruction to automatically clear indicators or hide unfinished work.

Proposed daily flow:

1. Open an account's Unread conversation filter and review new mail. Show the relevant unread messages within each conversation.
2. Assign the appropriate configured categories manually, accept a local model's suggestion, or use configured automatic classification/filing.
3. Mark reviewed messages read. If action remains, star the relevant message and continue the work from the Starred conversation filter.
4. Write replies personally, complete any other action, then explicitly remove the hold. Sending a reply alone does not prove all action is complete.
5. Check the account's unread and starred totals. Categorizing, moving, archiving, generating a summary, or changing a filter never clears read/star state by itself.

The app should make remaining work easy to find, including older holds. Propose showing the date a hold was set when known and an oldest-holds sort. For stars first discovered during synchronization, distinguish first observed locally from the unknown original star date. No tasks area, overdue scoring, streaks, or guilt notifications are needed. A legitimate unfinished action remains visible across days.

## Visual meaning

The [visual design brief](../ui-ux.md) makes deferred action explicit through proposed star labels/tooltips, a clear Starred view and honest hold age. Exact copy and visual treatment remain open. Star age does not create a due date, task or scheduled snooze. Starring unread mail leaves it unread; reading starred mail leaves its hold intact. Clearing a star is explicit and does not prove unrelated work is complete.

## Counts and conversation semantics

The UI remains conversation-only. A conversation appears in Unread if any in-scope member is unread, and in Starred if any in-scope member is starred. Show conversation counts in those filters and make underlying message counts available in their detail; either can be zero only when every corresponding in-scope message is clear. A conversation can appear in both filters, but appears once within each.

Use native provider state: Gmail's UNREAD/STARRED system labels and IMAP's Seen/Flagged flags (an unset Seen flag means unread). The category taxonomy uses user labels/folders separately from these states; it must not substitute a local-only read/star system.

Propose applying Star to the selected message, or the latest received message when invoked from a conversation row. A clearly named conversation action can clear stars on the existing member messages. Likewise, marking a whole conversation read applies to its known members at the time of the command; a later incoming message remains unread. Keep message-level provider state even though browsing is by conversation. Ordinary read-on-open timing remains a UX choice; classification and summaries never trigger it.

Account totals span its synchronized mail scope, including archived mail and configured category folders. Changing a folder, category, sender filter, or People/Automated filter must not manufacture a zero-work state. Keep scope, incomplete synchronization, and pending/failed provider writes visible. Spam/trash inclusion remains the existing retention-scope decision and must be stated rather than silently changing the meaning of zero. Do not provide an unqualified all-clear while coverage is incomplete or state changes await confirmation. Accounts retain separate navigation and counts; there is no unified inbox.

## Configurable categories using provider storage

First-product intent: distinguish human correspondence from automated senders, mailing lists and user-defined noise, then assign the configured category and scope within that account. The user defines these meanings; human mail is not automatically important and automated mail is not automatically unwanted. Uncertain mail may remain unclassified and visible. Implement Gmail labels first. The IMAP folder behavior below is retained for the later protocol phase only.

Categories belong to the user's mailbox organization. Each account has configurable category names, short descriptions for classification, and mappings to actual Gmail labels or IMAP folders. Example optional categories are People, Automated, Receipts, Newsletters and Work updates; none is a fixed taxonomy. People/Automated is a filter over the chosen mapping, not a separate hidden AI classification system. The user may leave mail unclassified or choose another category.

Allow selecting existing destinations and explicitly creating a label/folder as part of category setup. Support editing category definitions and remapping destinations. Treat provider rename/deletion/permission changes as mapping changes requiring reconciliation; changing a category definition must not silently move historical mail. Provider-backed assignments are the durable categorization state; local IDs, descriptions, suggestions and provenance are supporting metadata. A refresh/reinstall must not lose assignments already stored with the provider.

| Provider | Proposed category behavior |
| --- | --- |
| Gmail | Apply one or more configured user labels to explicitly targeted existing messages. Preserve other user labels. Show mixed membership honestly at conversation level. Filing out of Inbox is a separate configured action; labeling alone does not archive. New thread members are classified independently unless a user-defined policy explicitly covers them. |
| Generic IMAP | Use one configured primary category folder for filing. Assignment moves targeted messages into that folder using supported provider operations, preserving unread/star state and local content. Do not simulate Gmail's multiple labels by duplicating a message into several folders. If a suggested category set has no unambiguous primary destination, leave it for review. |
| Either | Manual and model-assisted actions use the same mappings and durable operations. Missing destinations, read-only access, ambiguous moves and offline writes have visible pending/failure states. Do not create arbitrary model-invented labels or folders. |

Propose single-primary-category definitions for an account that needs equivalent behavior across Gmail and IMAP. Existing Gmail multi-label organization can remain richer; do not flatten it merely for uniformity. When a conversation has members in different folders/categories, display that state and identify the messages affected by a bulk action. Moving between IMAP folders must preserve the local conversation and complete offline content through remote identity changes.

## Mutation contract

Persist explicit read/star/category commands through the same operation journal as other mail edits. Preserve the user's reading position during refresh and pending mutations. Derive Unread/Starred conversation views from message flags, with account-wide totals over the agreed synchronized scope; include archived/category-folder mail and mark incomplete coverage. Moving or labeling mail does not complete triage. A command acts on a snapshot of the selected member IDs; a new message does not inherit a previously completed read/unstar command.

Category definitions map to provider destinations per account. Gmail classification may add/remove configured category labels while retaining unrelated memberships; Inbox removal is a separate configured filing action. Generic IMAP classification selects a primary folder and journals a move, preserving local content/conversation identity as remote UIDs change. Negotiate MOVE or an explicitly tested fallback without broadly expunging unrelated messages. Do not represent multiple Gmail-like categories by copying IMAP messages across folders. Missing/renamed destinations and partial thread moves require visible reconciliation.

## Acceptance gates

- Distinguish unread attention from deferred starred action in the UI. Show a recent and an older hold, with known versus first-observed age represented honestly; neither reading nor sending auto-clears a star.
- Review a conversation, mark its inspected message read, star it while taking action, file it, write a reply personally, then unstar explicitly. Confirm the resulting provider states from another client.
- Archive a still-unread Gmail message or move it into an IMAP category folder: account Unread still includes it. A hidden category or incomplete import cannot produce a false all-clear.
- Classify a new reply in an already labeled/starred conversation without clearing its unread state, assigning unrelated categories to older members, or clearing another outstanding hold.
- Correct a category while inference is in flight, change it in another mail client, rename/delete a mapped destination, interrupt a move, and restart. Preserve the newer choice and surface unresolved provider actions.

## Open before affected behavior ships

Agree read-on-open timing, conversation-row Star targets, old-hold presentation, final count scope including spam/trash, and category setup UX. Category names/descriptions and provider destinations are user configuration, not a fixed taxonomy. Filing and inference must never clear unread/stars to manufacture completion.
