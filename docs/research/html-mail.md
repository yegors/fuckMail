# HTML mail: reader and human composer

Sources checked 2026-09-10; reorganized 2026-09-11. Documentation/source evidence and proposals only; no app prototype, live account flow or benchmark has run. Recheck version-sensitive claims before implementation.

Build boundary: [Mail UI contract](../contracts/mail-ui.md).

## HTML mail without a JS application stack

Reading HTML messages and authoring formatted replies are distinct components. Qt's rich-text editor can edit plain/rich text and serialize HTML, but its document renderer supports a defined HTML/CSS subset. It is a plausible composer, not proof of full compatibility with received email layouts. [QTextEdit](https://doc.qt.io/qt-6/qtextedit.html), [supported HTML subset](https://doc.qt.io/qt-6/richtext-html-subset.html)

For more complete received-message rendering, Qt WebEngine offers a C++ interface backed by Chromium; an OS-specific HTML view is another integration direction. Restrict the renderer to message content, disable page scripts and privileged bridges, and control external resources. This introduces browser-engine code and resource costs, even though we author no JS application or npm pipeline. Qt exposes a setting for disabling JavaScript; that setting alone is not the complete isolation policy. [Qt WebEngine overview](https://doc.qt.io/qt-6/qtwebengine-overview.html), [renderer settings](https://doc.qt.io/qt-6/qwebenginesettings.html)

Eliminating browser engines entirely would narrow the rendering choices and may reduce HTML email fidelity. Evaluate that tradeoff with representative messages rather than quietly replacing HTML with plain text. The user's objection to the JS ecosystem does not by itself establish a ban on an isolated HTML renderer.

## Next experiment

P0-04 (reader/composer), with P0-05 (MIME): compare sanitization, MIME processing and editor/renderer libraries with representative received email and authored drafts; record packaging and license constraints. Include tables, quotations, selection/copying, inline images, plain-text fallback, signatures and attachments. Measure native list/calendar cost separately from the HTML surface and its subprocesses.

The [current desktop recommendation](desktop-stack.md) uses Qt Quick/QML with an isolated WebEngine reader; neither a basic rich-text widget nor an attractive shell proves full mail fidelity.

## Qt Quick composer evaluation

QML TextEdit supports formatted text, and Qt's Controls example demonstrates rich-text editing. Use this as a candidate for the restrained human composer, not as evidence of full received-email HTML fidelity. Validate signatures, lists/links, quoted replies, clipboard sanitization, attachments, draft round trips and MIME generation. A QWidget QTextEdit is not a drop-in QML control. [TextEdit](https://doc.qt.io/qt-6/qml-qtquick-textedit.html), [editor example](https://doc.qt.io/qt-6/qtquickcontrols-texteditor-example.html).

Run reader/composer tests on Windows, macOS, Ubuntu, Arch and Omarchy. Browser rendering belongs only in the GUI; the shared core and headless CLI read normalized text/source without loading a renderer. Preserve original HTML rather than restyling every received message to imitate the typography in the mockups.

## Phase 0 reader/composer slice

Checked 2026-09-11. P0-04 owns the integrated Qt reader/composer experiment; P0-05 owns shared MIME/library evaluation. Use the same local fixture corpus so rendering, indexing and outgoing mail do not evolve separate interpretations.

One concrete integration limit: QML WebEngineView's `loadHtml` has an approximately 2 MB limit after percent encoding. Loading a whole long conversation as one HTML string is therefore not an adequate design. Evaluate a restricted custom URL scheme backed by approved message resources, with bounded visible/expanded message rendering and explicit origin/resource policy. Qt supplies a scheme-handler API, but its configuration can grant wider access; it is a building block rather than a complete mail-content boundary. [WebEngineView loading limits](https://doc.qt.io/qt-6/qml-qtwebengine-webengineview.html#loadHtml-method), [custom scheme handler](https://doc.qt.io/qt-6/qwebengineurlschemehandler.html)

The closer mockups' collapsed earlier messages make bounded thread presentation plausible. The prototype must nevertheless expose every original message and attachment, preserve selection/reading position and keep a long thread usable when expanded. A local summary is separate metadata, never replacement content or outgoing draft text.

| Question | Input and required output | Acceptance and fallback |
| --- | --- | --- |
| Can the received-mail surface preserve fidelity? | Local MIME fixtures: nested quotations, tables/newsletters, Unicode, inline CID images, large HTML, long threads and missing attachments. Output screenshots and a content/fidelity checklist on required desktops, plus latency and process-memory measurements. | Readable original content and working selection/copy; no silent truncation at string-load limits; list/calendar remain responsive. If cost is high, adjust renderer reuse and bounded presentation before proposing a less capable renderer. Plain text remains an explicit fallback, not proof HTML support passed. |
| Can content loading stay inside its intended boundary? | Harmless local fixtures containing scripts, remote image/style URLs, navigation and non-message resource references. Output observed resource requests and allowed user-triggered link behavior. | Page scripts and privileged bridges are absent; rendering does not fetch blocked external resources or arbitrary local files. Select the remote-content policy before live mail; do not mistake script disabling for complete isolation. Failed isolation blocks live HTML reading, while synthetic UI work can continue. |
| Can humans compose without a web editor? | QML TextEdit/TextDocument prototype with supported formatting, signature, links, quoted reply, clipboard input and attachments; pass draft content to the Rust MIME candidate. Output reopen/round-trip comparisons and captured messages through a local sink. | Text, formatting and attachments survive edit/save/restart; correct account and reply headers; GUI close preserves the last durably saved draft. Check IME, undo, selection and scaling across desktops. If the QML facilities fail, investigate a contained Qt document/editor adapter before reopening the UI stack; do not add AI writing. |
| Does the shared boundary work? | Reader/editor attached to a fixture-backed engine through the proposed IPC, with delayed bodies, attachment downloads, disconnect and restart. Output event traces and visible pending/error states. | The GUI receives only needed content, the engine owns durable drafts and MIME, the CLI works with no renderer, and the bottom strip stays responsive. No direct GUI database access or alternate draft schema to make the demo work. |

The clean-install experiment must include the renderer subprocess/resources and composer, not just an empty Qt window. Measure total GUI/renderer resource use separately from the engine and optional model runner. Final fonts, colors, dimensions and composer layout await the founder's final mockups; these feasibility questions do not.
