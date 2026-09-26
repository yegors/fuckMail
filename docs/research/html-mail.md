# Mail content: text first, best-effort HTML later

Source checks previously recorded on 2026-09-10/11. Product direction updated 2026-09-11 under [D009](../decisions.md); this update adds no new renderer benchmark. Owner: [Mail UI](../contracts/mail-ui.md).

## Current requirement

The founder mainly reads and replies to human correspondence. Received HTML display is useful for occasional automated/newsletter mail but can be added later, with good-enough readability rather than browser-level fidelity. It is no longer a first-usable or toolkit-selection gate. Preserve original source content and attachments even when displaying text.

The earlier WebEngine-first recommendation below has been superseded as the baseline. Qt Quick remains the proposed UI, but no browser engine is now required to start. QML syntax is accepted; Node.js and separate JS compilation/bundling are excluded. A later renderer must fit that native build boundary.

## Early content work that remains necessary

Gmail still exchanges MIME messages. Decode multipart content, choose an available plain-text part and safely extract readable text from HTML-only messages. Human-origin messages can also be HTML-only or contain rich signatures; sender classification must not determine whether their content is accessible. Preserve links, quotations, attachment identity and original bytes. Text extraction must not execute scripts or fetch remote resources.

Use the same parser/text representation for search, CLI and the GUI. Keep source HTML for later viewing and retain all attachments offline. No silent blank message or discarded source is acceptable merely because HTML display is deferred.

The human composer is a separate concern. Plain replies, signatures, recipients, attachments and durable drafts are early requirements; restrained formatted composition remains intended. Qt text/document facilities are candidates for that scope. Validate undo/IME, quoting and save/reopen/MIME round trips without requiring an HTML browser. [Qt Quick TextEdit](https://doc.qt.io/qt-6/qml-qtquick-textedit.html), [text editor example](https://doc.qt.io/qt-6/qtquickcontrols-texteditor-example.html).

## Later display options

| Candidate | Why evaluate it | Limits and evidence needed |
| --- | --- | --- |
| Qt rich-text/document display | An explicit limited HTML/CSS implementation may meet the relaxed reading requirement with fewer dependencies | Useful formatting/images/links and safe resource policy; complex newsletter layouts may simplify. It is a subset renderer, not proof that all email follows that subset |
| Isolated Qt WebEngine | Broader layout support if actual readability needs justify its cost | Chromium helpers, memory, updates and deployment; use a dependency route without requiring Node in our build |
| Another native/OS component | Compare only when a concrete gap remains | Cross-platform integration, content access, maintenance and resource policy still need evidence |

Qt documents its supported rich-text HTML subset. Real email markup is not a single reliably restricted language; the agreed simplification is our presentation target, not a guarantee about incoming content. [Supported subset](https://doc.qt.io/qt-6/richtext-html-subset.html).

If WebEngine is later considered, its browser/helper process architecture and build constraints must be counted. Building it from source requires upstream tools including Node; this cannot silently become our required build path. Its `loadHtml` string size limit also makes concatenating an entire long thread an unsuitable assumption. Controlled per-message resources and bounded rendering remain possible evaluation paths, not accepted APIs. [Architecture](https://doc.qt.io/qt-6/qtwebengine-overview.html), [platform/build notes](https://doc.qt.io/qt-6/qtwebengine-platform-notes.html), [loadHtml limit](https://doc.qt.io/qt-6/qml-qtwebengine-webengineview.html#loadHtml-method).

## What good enough means

Proposed later fixture review: readable authored text, usable links, useful inline images when permitted, understandable quotations/basic formatting and a clear text fallback. Layout differences, simplified tables or omitted decorative styling are acceptable candidates for founder review. Do not require pixel equivalence with Gmail or a browser. Do not silently truncate important text or drop offline attachments.

Reduced visual fidelity does not relax isolation: no message scripts, privileged app bridge, arbitrary local-file access or unrequested remote content. Test harmless fixtures and actual resource requests. A limited document renderer needs a resource policy too; it is not automatically safe because it is not Chromium.

## Execution

P0-05 proves MIME, safe text extraction and shared outgoing serialization. P0-04 proves text reading, human composition and shared GUI/CLI state. Both are early. Later Google polish evaluates HTML viewing when the founder needs it, against this narrower readability target. No HTML-engine experiment blocks the first paired-client milestone.

Keep evidence for source preservation, readability, editor round trips and any later renderer memory/isolation separate. Original source and text search remain usable regardless of whether the HTML component is enabled or installed.

## 2026-09-23 renderer candidates

Checked for [D100](../decisions.md) and the HTML reader plan. Web sources were checked on 2026-09-23; no renderer benchmark has been run yet.

- **litehtml:** BSD-3 C++ HTML/CSS layout engine. It parses with a bundled gumbo (Apache-2.0) and does no drawing, networking or scripting itself; the embedder's container supplies fonts, drawing and every image. Claws Mail replaced its WebKit "fancy" viewer with a litehtml plugin, and describes the WebKit deprecation as a security decision. Qt Creator's default help viewer is litehtml-based. [litehtml](https://github.com/litehtml/litehtml), [Claws Mail plugins](https://claws-mail.org/plugins.php), [Debian package](https://packages.debian.org/unstable/claws-mail-litehtml-viewer), [Qt Creator help](https://doc.qt.io/qtcreator/creator-how-to-get-help.html).
- **Blitz (Rust):** html5ever + Stylo + Taffy + Parley, Apache-2.0/MIT. The project describes itself as beta, with GPU (Vello) examples, and no email or text-selection claims were found. Worth re-evaluating as a memory-safe option. [Blitz](https://github.com/DioxusLabs/blitz).
- **QtWebEngine:** still excluded for this use. It is not in the installed Qt 6.11.2 kit, and the process/memory objections above still apply.

Inference, not measured: email HTML is mostly tables plus inline CSS, which suits litehtml's scope. Speed, memory, fidelity and fuzz results belong to plan packets H0–H1.

## D105 — HTML reader implementation (2026-09-24)

This records [D105](../decisions.md): H0–H5 of the HTML reader plan. It ran on one Windows 11 host with Qt 6.11.2 (MSVC) and the Rust workspace in its Debug profile. No test used a real mailbox, the real internet or a real proxy.

**Built:**
- **Engine:** `fm_mail::html_view` (the sanitiser, version 1) and `fm_mail::image_view` (image decoding). Core adds `html_content`, `view_cache`, `link_index` and `remote_prefs`, plus schema34. The engine adds its `remote_content` runtime and link-index task. `fm_ipc::Client::call_within` gives the CLI its 30 s image and proxy-test calls.
- **GUI:** litehtml at `b58fd1023244557949d859b61dd52149420b8762` with its bundled gumbo, 166 files and 3.4 MB, vendored under `gui/third_party/litehtml`. On top of it: `FmHtmlView`, `HtmlDocument`, `HtmlContent`, `HtmlMessageBody.qml` and `RemoteContentSettings.qml`.

**Results:**

| Check | Result |
| --- | --- |
| `cargo test --workspace --no-fail-fast` | 734 passed, 6 ignored, 0 failed. It includes the new sanitiser (7), image (2), Core HTML view (2), link index (1), preferences (1), cache (1), schema33 upgrade (1), fetch-path (3), engine session (1) and CLI scope (1) tests. |
| `cargo clippy --workspace --all-targets -- -D warnings` | This packet's code is clean. At first the workspace failed on five lints in other packets' code. After the D104 session fixed its fm-core and fm-cli findings, a rerun fails only on `manual_inspect` (`local_model/backlog.rs`) and `too_many_arguments` (`local_model.rs`), both in the D103 packet's code. |
| `cargo fmt --check` | This packet's files and hunks are formatted. Other packets' hunks in `google_sync`, `local_model` and `ollama` still differ. |
| `python scripts/check_repo_hygiene.py` | Passed: 729 source files. |
| Native HTML tests, Debug, offscreen/software | `gui-html-view` passes 45/45 and `gui-html-message-body` 43/43 on every run: 6 and 8 direct runs, plus CTest `--repeat until-fail:3`. |
| Mutation check | With the layout-height memory disabled, three first-frame checks fail: one in the view test and two in the body test. The code was restored and the suite rebuilt. |
| GUI suite, Debug, `-j4` | 27/29. `gui-google-send-controls` failed because its startup-sync harness did not answer `google_sent`, a request from outgoing mail; the D104 session has since fixed that harness. `gui-mail-navigation-cache` failed its "queued paging burst" timing check under load; alone it once passed in 23.8 s and once timed out at 30 s. |
| GUI suite, Release | **29/29 in 45.2 s.** `gui-html-view` 0.64 s, `gui-html-message-body` 0.62 s, `gui-mail-navigation-cache` 5.91 s. |

**Problems found and fixed:**
- **Tiles missing after scrolling (software renderer).** Under Qt's software scene-graph renderer, tiles created after scrolling were not drawn, although their images were correct; Direct3D was unaffected. A `QSGTransformNode` root fixes it.
- **Body collapse when blocking a link.** Blocking a link rebuilt the document and collapsed the body. A rebuild of the same content now keeps its height and stale tiles until the new layout paints.
- **Empty first frame.** Every HTML message's first frame was empty, and its height jumped a frame later, because cache hits were answered through a queued call and each property change laid out separately. Now:
  - held results are assigned synchronously;
  - content, links and font lay out once, at polish;
  - a revisited message takes its remembered height.
- **Test click race.** The body test clicked Load images as soon as the bar appeared. The bar appears as the paper grows, and until the column lays out again the paper covers it, so the click reached the view about three times in five. The test now waits until the paper is below the bar. A person cannot click within that window, because no frame is drawn before the layout.

**Not measured and still open:**
- **Plan budgets:** open-message p95 with HTML, sanitiser and layout time, scrolling frames, memory by the D099 method, link statistics at 100k messages, and package size.
- **H1 checks:** cargo-fuzz and libFuzzer/ASan runs, the litehtml differential tree check, the founder's fidelity review of real newsletters, and Mac and Linux builds.
- **Real services:** a real HTTPS proxy, and a Mac Keychain refusal of the proxy password.
- **First visit:** a message's first HTML visit still appears after the IPC round trip and the worker layout.

## D107 — sixth export list and HTML fixes (2026-09-24)

This records [D107](../decisions.md). It ran on one Windows 11 host with Qt 6.11.2 (MSVC) and the Rust workspace.

The founder authorized reading their actual mailbox. Only these touched it:
- read-only CLI calls: `mail list --json` and `mail html`;
- offscreen captures of the real profile that opened no unread conversation.

Message copies lived in the session's temporary scratchpad and were deleted afterwards. None are in Git, and this record gives no message content.

**Method**
- **Corpus:** every cached message with an HTML part, 515 in all: 111 from one account and 404 from the other. The original HTML was used where it was exported, and otherwise the engine's `mail html` output. All of it went through the current sanitiser.
- **Probe:** `fm-html-probe` (`gui/HtmlProbe.cpp`, built with the GUI) runs the reader's `HtmlDocument` on each message. It parses, lays out at 720 px and paints the first 2,400 px, reporting times, size, scale and painted text.
- **Chromium:** the same files were measured in Chromium, in a 720 px iframe tall enough to avoid scrollbars.
- **Reduction:** automatic reducers shrank each fault's message while the fault persisted. Every fix has a synthetic regression test shaped like the reduced case, not the mail.

**Faults found and fixed**

| Symptom | Cause | Fix |
| --- | --- | --- |
| Amazon order mail never appears | Nested tables re-measured exponentially: 11.3 s layout in Release, 13 s in the app | Width measurements are reused within one render: 5 ms |
| Amazon prices paint about 245 px low | `position:relative` on an inline box added 2 px per re-layout, about 150 times | The shift is applied from zero |
| RDSC scaled down, images off-centre | Declared cell widths forced the table wider than the column | Cells shrink to their content minimum, as in Chromium. Layout 43 → 2 ms; height 2,897 px against Chromium's 2,742 |
| GitHub notifications scaled to about 60% | Long unbroken runs (URLs, hashes) and unwrapped `pre` log lines set the minimum width | Break points after `/`, `-`, `?`, `&`, `=`, `\|` and `\`, and every 30 characters in runs over 60. `pre` wraps unless the sender's CSS says otherwise |
| Amex laid out at about 1,440 px and scaled | Attribute styles were lost when a media query flipped between the 600 px first pass and 720 px; a blocked image sized only by height was drawn four times as wide | Restyling starts from the recorded base styles; height-only images stay inside their column, keeping their shape |
| Two store messages nearly blank; another took 448 ms to lay out | The same media-query flip rebuilt CSS tables without their content and kept stale render items | The render tree is rebuilt with the base styles: content returns, 448 → 12 ms |
| Very tall blank areas | Percentage heights resolved against the viewport | They compute to auto inside an auto height |
| Replies with only quoted history render empty | The sanitiser removed the quote, leaving nothing | The quote stays when nothing else is visible |
| Digest mail empty | The first HTML part is empty | The first non-empty part is used |
| Five phishing messages empty | The real part declared the transfer encoding `amazonses`; mail-parser read it as text, outside the HTML body | Any part labelled `text/html` is considered (sanitiser version 4) |

**List fixes, found by scanning `mail list --json`:**

| Check | Before | After |
| --- | --- | --- |
| Markup or web addresses in previews | 3 of 66 and 16 of 600 previews | 0 |
| Other remaining finds | — | One preview quotes a sender's `<address>`, which is text, not markup |
| "+N" noise from own and no-reply mailboxes | Plus-tagged own address; GitHub `noreply` | Plus-tags and Gmail dots fold into one mailbox; no-reply mailboxes are excluded |
| Spam recipient `me@aol.com` on 163 spam threads | — | Kept: a real listed recipient |

**Final results, Release build**

| Measure | Result |
| --- | --- |
| Messages rendered with painted text | 515 of 515; none invalid |
| Scaled below 1.0 | 1: a row of three 323 px images, at 0.85 |
| Sanitising | Median 0.8 and 0.9 ms by account, maximum 5.1 ms |
| Parse plus layout | Median 5 ms, p95 58 ms, p99 93 ms. Maximum 771 ms (616 + 155) for a 924 KB message. All on the worker thread. |
| First-screen paint | Median 2 ms, p95 11 ms, maximum 29 ms |
| Chromium comparison | Remaining width and height differences are blocked-image placeholders (Chromium draws large broken-image boxes), intentional `pre` wrapping and font metrics |
| Real list capture | "+N" only where another person is on the thread; summaries in place of previews; thread and attachment counts beside the subject |

**Tests**

| Check | Result |
| --- | --- |
| `cargo test -p fm-core -p fm-mail -p fm-engine -p fm-cli` | 618 passed, 4 ignored, 0 failed |
| `cargo clippy -p fm-core -p fm-mail --all-targets -- -D warnings` | Clean |
| `cargo fmt --check` | This packet's files are formatted. Earlier packets' hunks in `fm-engine` (`google_sync`, `local_model`, `ollama`) still differ. |
| Release GUI suite | 29/29 in 18.8 s. `gui-html-view` passes 54 checks, 9 of them new; `gui-html-message-body` passes 43. |
| Debug GUI suite | 29/29 after the last GUI change |

**Profile.** The engine upgraded the founder's real profile to schema35. The schema34 store remains beside it as `store-schema34-backup.sqlite3`.

**Not done and still open**
- Messages already imported with an empty text part keep an empty preview until they are imported again. This affects only the five phishing messages found.
- Bulk remote images were not loaded or compared.
- The founder's visual review, and Mac and Linux builds.
- The full workspace `cargo test` and clippy runs.
