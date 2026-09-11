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
