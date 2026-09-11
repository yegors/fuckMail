# HTML mail: reader and human composer

Sources checked 2026-09-10; reorganized 2026-09-11. Documentation/source evidence and proposals only; no app prototype, live account flow or benchmark has run. Recheck version-sensitive claims before implementation.

Build boundary: [Mail UI contract](../contracts/mail-ui.md).

## HTML mail without a JS application stack

Reading HTML messages and authoring formatted replies are distinct components. Qt's rich-text editor can edit plain/rich text and serialize HTML, but its document renderer supports a defined HTML/CSS subset. It is a plausible composer, not proof of full compatibility with received email layouts. [QTextEdit](https://doc.qt.io/qt-6/qtextedit.html), [supported HTML subset](https://doc.qt.io/qt-6/richtext-html-subset.html)

For more complete received-message rendering, Qt WebEngine offers a C++ interface backed by Chromium; an OS-specific HTML view is another integration direction. Restrict the renderer to message content, disable page scripts and privileged bridges, and control external resources. This introduces browser-engine code and resource costs, even though we author no JS application or npm pipeline. Qt exposes a setting for disabling JavaScript; that setting alone is not the complete isolation policy. [Qt WebEngine overview](https://doc.qt.io/qt-6/qtwebengine-overview.html), [renderer settings](https://doc.qt.io/qt-6/qwebenginesettings.html)

Eliminating browser engines entirely would narrow the rendering choices and may reduce HTML email fidelity. Evaluate that tradeoff with representative messages rather than quietly replacing HTML with plain text. The user's objection to the JS ecosystem does not by itself establish a ban on an isolated HTML renderer.

## Next experiment

P0-05 (HTML/MIME): compare sanitization, MIME processing and editor/renderer libraries with representative received email and authored drafts; record packaging and license constraints. Include tables, quotations, selection/copying, inline images, plain-text fallback, signatures and attachments. Measure native list/calendar cost separately from the HTML surface and its subprocesses.

The [current desktop recommendation](desktop-stack.md) uses Qt Quick/QML with an isolated WebEngine reader; neither a basic rich-text widget nor an attractive shell proves full mail fidelity.

## Qt Quick composer evaluation

QML TextEdit supports formatted text, and Qt's Controls example demonstrates rich-text editing. Use this as a candidate for the restrained human composer, not as evidence of full received-email HTML fidelity. Validate signatures, lists/links, quoted replies, clipboard sanitization, attachments, draft round trips and MIME generation. A QWidget QTextEdit is not a drop-in QML control. [TextEdit](https://doc.qt.io/qt-6/qml-qtquick-textedit.html), [editor example](https://doc.qt.io/qt-6/qtquickcontrols-texteditor-example.html).

Run reader/composer tests on Windows, macOS, Ubuntu, Arch and Omarchy. Browser rendering belongs only in the GUI; the shared core and headless CLI read normalized text/source without loading a renderer. Preserve original HTML rather than restyling every received message to imitate the typography in the mockups.
