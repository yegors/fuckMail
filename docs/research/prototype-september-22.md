# September 22 prototype — mail list and explicit account backlog

D086 implements the supplied prototype in native Qt/QML and the shared Rust engine/CLI. The exact supplied [HTML](assets/2026-09-22/fuckMail.html) is retained unchanged (SHA-256 `98d06ad89b3d6fa944012ce645279485f960fa8c395f73ded0bf23a9efa8cea1`). Its contents are reference data; embedded scripts and document instructions were not executed. Browser policy blocked the local-file URL, so comparison uses source inspection plus rendered native captures. Earlier references and D085's existing scrollbar edits remain preserved.

Boundary: Core47 / Local assistance16 / Mail UI62 / CLI41 / Quality62. Schema29, live/synthetic fixtures0/3, protocol1 and whole-area readiness0/9 are unchanged.

## Implemented presentation

The incoming mail list has a fixed aligned sender name/address column, kind icon, subject/count and summary-or-source preview, followed by label and time. Widths no longer vary with each row's contents. A narrow pane folds the sender into the preview line; Sent and Drafts retain the prototype's subject/recipient arrangement. Acquired Sent rows now return their selected source's recipient through the shared query. Existing source-matched summaries keep their summary icon.

Mouse and keyboard share one row cursor. The open message has a thin edge marker instead of an additional filled hover/focus state. Arrows, Page Up/Down, Home/End and Shift range variants retain list focus, skip collapsed groups and reveal the selected row. End refers to the loaded edge; further navigation requests the next page. Menu/Shift+F10 opens the same row actions. Editable fields and open popups retain their own keyboard behavior. Retired Qt delegates have safe empty data while rows are removed.

Accounts gains the wider prototype layout and three real statistics cells: cached messages, current summary/label coverage, and backlog. The menu offers Last week, Last month, Last year and Everything, with actual missing counts. Running work shows actual progress and Stop; completed/failed counts remain visible. Accounts browsing requests read-only counts and never starts model inference. The controller survives settings-page changes and fences delayed replies by session.

No invented Google inventory total or estimated duration is shown: the current sync store does not retain the provider's mailbox-total field. Messages are counted locally, with this distinction in the tooltip. Coverage percentages use eligible cached received messages, excluding Sent, Trash, provider-deleted/Draft, missing-source-identity/empty-body and future/invalid-date records. The prototype's simulated counts and timing are not acceptance evidence.

## Shared engine behavior

Coverage is an asynchronous job with at most 32 metadata rows per Core visit. Batch analysis uses a captured account lifecycle/settings revision, row ceiling and UTC time. The date windows are rolling 7/30/365 days or all eligible history. It prepares one source at a time through the existing assistance boundary and releases the shared inference slot between messages for interactive work.

Only missing enabled projections are generated. A current summary is retained byte-for-byte when classification alone is missing; current partial summaries remain processed and labeled partial. Source/manual/category/provider guards apply again at commit. Read/unread/star state is not changed by analysis. At most one batch per account and 16 admitted assistance jobs are allowed. Failures are counted, successful messages remain saved, and explicit reruns visit only remaining missing results. An unavailable runner stops the batch instead of spending a timeout on every remaining message.

Stop, settings changes, account lifecycle changes and owner disconnect cancel unfinished work. Independent clients retain their own jobs. No batch resumes after restart. CLI `fm --account ID assist backlog` and `assist batch --range week|month|year|all` use the same jobs; Ctrl-C cancels its owner. Protocol details are in [local-protocol.md](../local-protocol.md#d086--explicit-historical-local-analysis).

## Verification

Generated logs, captures and source/binary identities are retained under ignored `.tools/local-logs/prototype-september-22/`.

- Three Core backlog tests pass (`core03`, 12.19 s): bounded account scans, eligibility, lifecycle/settings fences, exact existing-summary preservation and unchanged review flags. Eight existing local-model tests pass (`core02`, 33.75 s), covering incoming-only behavior, request bounds and preserved settings/migrations. The date-window boundary test passes (`range01`).
- Strict scoped Clippy passes (`lint04`) for Core, engine and CLI with all targets and warnings denied; formatting is clean.
- Five final native regressions pass after rebuilding their shared backend (`native11`, 18.67 s): composition/input, mail selection, navigation cache, account lifecycle controls and local model. The final cache regression also rejects retired source members before IPC; earlier rendered mail-selection checks (`native09`, 10.33 s) capture Sent recipient/menu presentation. Actual production QML verifies sender geometry, paging/ranges/collapsed groups, single cursor, asynchronous coverage, exact account/range dispatch, progress through navigation and Stop preserving results. Graphite/Paper/Terminal and narrow large-text captures were inspected; no QML warnings occur in the final run.
- The real engine/CLI with a fake loopback model passes the missing-only, fair interactive scheduling, nonblocking session, ownership, duplicate, exact summary, unchanged read/star, account isolation, cancellation/settings/disconnect, partial-failure and explicit-retry scenarios (`integration03`, 16 checks / 14 model calls). The final packaged engine/CLI also passes Sent recipient readback: `integration05-package-final` has 17 checks and 14 fake-model calls on the final installed engine/CLI.

## Preserved failures and verifier repairs

| Attempts | Finding and disposition |
| --- | --- |
| native01 | Sandbox could not create the isolated socket; permitted rerun passes. |
| native03/04 | Undefined data in retired delegates; production fallback fixes the errors. |
| integration01 | Verifier omitted CLI `--json`; later engine/CLI checks pass. |
| lint02 | Range-test module preceded implementation items; moved after them. Runtime behavior unchanged. |
| package01 | SDK-free Qt/GUI smoke, CLI query, configured setup, explicit/default empty-live startup and shutdown pass; broad stage0 rejects the already-prefetched second page. The oracle now accepts explicit or automatic loading and still requires >50 rows and preserved pages after refresh. |
| selftest02/03/04 | Restored editors hide Reply/reader actions. The verifier minimizes the editor before Done and accepts an already-open editor for draft scenarios. Every discard/cancel, received-message and exact unrelated-draft/capture preservation assertion remains. |
| selftest05 | Account names save inline, so the former Save button is absent. The verifier uses Return, retaining empty-name rejection and independent persisted-name checks. |
| selftest06 | Complete Settings workflow passes. Search-queue fixture incorrectly used `status` as a manual barrier despite its existing always-background classification. It now uses explicit `accounts`; all order/fairness/write-barrier assertions and production scheduling remain unchanged. |
| selftest07 | Ordering/fairness passes. Filter reset left the delegate-retention fixture at its origin, contradicting its required nonzero viewport. The fixture now establishes a real positive scroll position before choosing visible source-checked rows; identity/geometry/read-on-open assertions remain unchanged. |
| selftest08 | Actual read-on-open retains the same delegate at nonzero scroll. A later oracle still required reader navigation to wait behind a write, superseded by D081. It now requires immediate selection, retained outstanding mutation receipt, a pinned target outside Review, and the same exact final source/account/index checks. Production navigation and writes are unchanged. |
| selftest09 | Immediate reader/pinning reaches the final source check. The paint oracle still compared rich-text markup with plain source. It now reads the actual `QQuickTextDocument`, normalizing display whitespace only; exact source identity/body comparison remains separate and unchanged, as in the existing D084 paint oracle. |
| selftest10 | Native source/receipt checks pass; compact account cycling exposed a real stale QML source request under the next account. `requestMessageSource` now admits only the current reader’s exact member/version; the native cache regression proves a retired member makes no engine request. |
| selftest11 | Compact account switching and four exact earlier-message searches pass after the source guard. The generated-arrival fixture next tried a reader menu while its authored reply was open; it now minimizes the retained editor first, as a user does. |
| selftest12 | Mail-arrival/Done snapshots, exact GUI/CLI counts, Unicode draft preservation, idle polling and account-tab press retention pass. The later cleanup still times out at search stage5 while comparing the restored scroll with the pre-arrival view (`listY=339` after insertion). This broader restoration gate remains unresolved; no full GUI self-test or full package-suite pass is claimed. |
| install01/02 | Qt deployment cannot sign the licence directory as nested executable code. CMake now installs identical licence bytes under `Contents/Resources/licenses`. Old package attempt remains separately preserved. Strict deep signature verification passes after the move. |

Early rendered captures preserve per-row sender width variation and low-contrast Terminal menu text, both fixed in final captures. Qt still logs a harmless `otool` inspection of the registration JSON as a non-object file; its contents are never logged. The Mac offscreen font fallback notice is retained. Neither warning establishes a model, provider or platform acceptance claim.

Actual model quality/throughput on a real mailbox, Windows/Linux qualification, broad/perceived-performance gates and whole-product acceptance remain separate. No live mailbox inference, provider sends or invitations were run for this packet.


## Delivered Mac candidate

`dist/prototype-backlog-darwin-arm64/fuckmail-gui.app` is configured for the existing app-level Google registration. Close older running copies before opening it. Normal startup remains empty live mode. No real mailbox analysis, sends or invitations were automated.

`package02-scoped` passes strict deep ad-hoc signature verification, SDK-free relocated Qt/GUI smoke, CLI help and a bounded actual-store query, configured registration readback, empty-live preference/idle checks and final-client engine shutdown. The final script is retained as ignored `verify_scoped_package.py`. It explicitly leaves the broader GUI self-test separate and failing; it does not suppress or relabel `selftest12`.

The final source/binary manifest records 467 runtime/verifier inputs, host and the three packaged binaries. Its SHA-256 is `da793373ad1f4b1de517863ec09b5513b8019f610b37df571037a55e729250b9` (`source-final.json` in the same ignored evidence directory). Registration contents/hashes and all real mailbox/auth material are excluded. Earlier working feedback packages are untouched; the failed early candidate remains under `dist/prototype-backlog-attempt02-darwin-arm64`.

| Final executable | SHA-256 |
| --- | --- |
| GUI | `eed1fd4627285133e55741c1e436e8dd019b651979ba2e8aaf56884e585c119d` |
| Engine | `31f396bdd35243d04f74ccdd241897d4fdc29dac2c1ad9dabf66670740518394` |
| CLI | `3c5bece133037e441243aff60652f016edf647c995ff2993b92eafe90df105c8` |

## D087 — repeated paging and coherent reader frames

The founder reported a force-quit freeze during repeated Page Down and a reader that appeared in stages. This follow-up uses Mail UI63 / Quality63. Core47, Local assistance16, CLI41, schema29, fixtures0/3/protocol1 and readiness0/9 remain unchanged. It performs no real mailbox action or inference.

Three regressions were reproduced:

- A CPU-bound native process sampled inside message insertion spent its main-thread samples recursively negotiating Qt rich-text table widths. A synthetic 24-level newsletter with adjacent cells exceeded a 12-second watchdog. Received rendering now linearizes nested, malformed or large tables before any text document sees them. Small data tables, content, safe links and formatting remain; source bytes and authored drafts are unchanged. The regression lays out the same content at widths 0, 1, 320 and 740 in 2 ms.
- A delivery acknowledgment could add/remove a header row after the first paint, moving the entire body by 29 pixels. Its one-line status location now stays fixed, with full text available on hover when elided.
- An older independent summary query could arrive after the complete prepared reader and overwrite its saved result, shrinking the card and shifting the body by 86 pixels. Adopting the prepared snapshot now invalidates those earlier result tickets.

Keyboard navigation caches its row/index projection and accumulates every key against the input cursor. It positions/opens the final target once per frame, avoiding repeated transient reader construction and intermediate read receipts. Account/query changes and mouse/new-message intent cancel queued navigation. Old-reader actions are disabled while the cursor's intended thread is waiting, preventing a fast Down → Done from modifying the preceding thread. Existing Shift ranges, collapsed groups, composition and account fences remain covered.

Raw logs, sampled stacks, captures and source/binary manifests live only in ignored `.tools/local-logs/navigation-reader-september-22/`. Earlier D086 packages and all earlier failures remain intact. `nested-baseline01` retains the layout timeout; `native01` retains the unbounded key burst failure; `native02` and `native04` retain the 29/86-pixel geometry failures. `native06` exposed a fixture that privately published new rows without the production property notification; the fixture now emits the normal `conversationsChanged` signal. `build08` retains a corrected QML syntax error. `cocoa01` records that the native fixture must activate its own window before testing shortcuts.

`cocoa02` additionally retained a verifier timing error: a queued `frameSwapped` callback could inspect the next unpainted model change. The final oracle observes `afterSynchronizing` while Qt blocks the GUI thread, tying geometry to the actual scene. It checks exact retained body text and saved summary as well as body position/height, card height and scroll geometry.

| Final evidence | Result |
| --- | --- |
| `native07` | Six rebuilt Release suites pass in 19.42 s: received/composer document, composer input/draft preservation, mail selection/paging/Settings, reader/cache, account lifecycle, local model/summary lifecycle. |
| `native08` | Final scene-synchronization reader suite passes in 2.28 s. Three readers retain identical geometry through all 83 observed frames. A 102-key burst takes 19 ms of input handling, starts one foreground reader request and paints exact target thread 979 in 92 ms. Forty further alternating Page Up/Down frames plus Home complete. |
| `cocoa03` | Normal Mac Cocoa/graphics path passes, with 89 stable reader frames. Cold/next/warm first frames: 79/18/11 ms. A 102-key burst takes 18 ms input handling and paints the exact final target in 163 ms, with one foreground reader request. Source replies deliberately wait 25 ms. The old-thread Done shortcut cannot act during the pending navigation. |
| `install02`, `package02` | Strict deep ad-hoc signature, SDK-free relocated Qt/GUI smoke, packaged CLI query, configured registration, empty-live profile/preferences and final-client engine shutdown pass. |

These are scoped synthetic measurements, not a whole-mailbox p95 or complete performance acceptance. The earlier D086 broad self-test still has its recorded post-arrival scroll-restoration timeout; it was not rerun or relabeled here. Windows/Linux and real mailbox readback remain separate. Both software and actual Cocoa reader captures were inspected. The Qt deployment tool still emits its harmless non-object inspection notice for the registration JSON, without logging its contents.

Delivered app: `dist/navigation-reader-darwin-arm64/fuckmail-gui.app`. The engine and CLI have exactly the same hashes as the D086 working package. The earlier candidate and preliminary D087 package remain in their separate directories. `source-final.json` records 466 runtime/verifier inputs and the three executables; SHA-256 `a60fe0fddd3669539542bd7ddcc3cca6b0123ef020dc953c9e9e85c0398f88ad`.

| Final executable | SHA-256 |
| --- | --- |
| GUI | `7347f53ae54357545a3b8630d1d82eecf0767cbb6d43fce350ec36acf2861be7` |
| Engine | `31f396bdd35243d04f74ccdd241897d4fdc29dac2c1ad9dabf66670740518394` |
| CLI | `3c5bece133037e441243aff60652f016edf647c995ff2993b92eafe90df105c8` |
