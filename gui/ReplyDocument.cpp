// ReplyDocument: loads saved drafts into the QML editor's QTextDocument without
// disturbing selection or undo, applies bold/italic/underline and bulleted or
// numbered lists, pastes as plain text, reduces HTML to the allowed formatting
// (also for received bodies), and connects the editor to the system spell checker.
#include "ReplyDocument.h"
#include <QSet>
#include "ReceivedBody.h"
#include "SpellChecker.h"
#include <QClipboard>
#include <QElapsedTimer>
#include <QFileInfo>
#include <QImage>
#include <QMimeData>
#include <QGuiApplication>
#include <QKeyEvent>
#include <QRegularExpression>
#include <QScopedValueRollback>
#include <QTextBlock>
#include <QTextCursor>
#include <QTextDocument>
#include <QTextList>
#include <QUrl>

QString ReplyDocument::limitedHtml(const QString &input, bool received) {
    // Reconstruct every tag/attribute, keeping only the formatting we expose.
    // Resource tags, image URLs, CSS URLs and event attributes cannot survive.
    QString body = input;
    const auto bodyMatch = QRegularExpression("<body\\b[^>]*>([\\s\\S]*?)</body\\s*>", QRegularExpression::CaseInsensitiveOption).match(body);
    if (bodyMatch.hasMatch()) body = bodyMatch.captured(1);
    body.remove(QRegularExpression("<(head|style|script)\\b[^>]*>[\\s\\S]*?</\\1\\s*>", QRegularExpression::CaseInsensitiveOption));
    static const QRegularExpression tags("<[^>]*>");
    static const QRegularExpression name("^<\\s*(/?)\\s*([a-zA-Z0-9]+)");
    // Qt recursively negotiates the minimum/maximum width of each nested
    // table. Newsletter layout tables can monopolize the GUI for minutes at
    // a delegate's initial narrow width. Keep small data tables; linearize
    // nested/large/malformed ones before any QTextDocument sees them.
    bool linearTables = false;
    if (received) {
        static const QRegularExpression tableTags("<\\s*(/?)\\s*(table|td|th|tr)\\b[^>]*>", QRegularExpression::CaseInsensitiveOption);
        auto tableMatches = tableTags.globalMatch(body);
        int depth = 0, cells = 0, rows = 0;
        while (tableMatches.hasNext()) {
            const auto token = tableMatches.next();
            const auto tag = token.captured(2).toLower();
            const bool closing = !token.captured(1).isEmpty();
            if (tag == "table") {
                depth += closing ? -1 : 1;
                linearTables |= depth < 0 || depth > 1;
            } else if (!closing) {
                if (tag == "tr") ++rows; else ++cells;
            }
        }
        linearTables |= depth != 0 || cells > 64 || rows > 32;
    }
    // Text between tags is copied with stray angle brackets escaped; each tag
    // is re-emitted from scratch or dropped, never copied through.
    QString output; qsizetype end = 0;
    auto matches = tags.globalMatch(body);
    while (matches.hasNext()) {
        const auto match = matches.next();
        auto text = body.mid(end, match.capturedStart() - end);
        text.replace('<', "&lt;").replace('>', "&gt;"); output += text;
        end = match.capturedEnd();
        const auto token = match.captured();
        const auto parsed = name.match(token);
        if (!parsed.hasMatch()) continue;
        const auto tag = parsed.captured(2).toLower(); const bool closing = !parsed.captured(1).isEmpty();
        if (linearTables && QStringList{"table","tr","td","th","thead","tbody","tfoot","caption"}.contains(tag)) {
            if (closing && (tag == "td" || tag == "th")) output += " ";
            else if (closing && (tag == "tr" || tag == "table" || tag == "caption")) output += "<br>";
            continue;
        }
        if (received && tag == "img" && !closing) {
            const auto alt = QRegularExpression("\\balt\\s*=\\s*[\"']([^\"']*)[\"']", QRegularExpression::CaseInsensitiveOption).match(token);
            if (alt.hasMatch() && !alt.captured(1).isEmpty()) output += "[" + alt.captured(1).toHtmlEscaped() + "]";
            continue;
        }
        const bool receivedTag = received && QStringList{"table","tr","td","th","thead","tbody","h1","h2","h3","h4","blockquote","hr","pre","code"}.contains(tag);
        if (!receivedTag && tag != "p" && tag != "div" && tag != "br" && tag != "span" && tag != "b" && tag != "strong" && tag != "i" && tag != "em" && tag != "u" && tag != "ul" && tag != "ol" && tag != "li" && tag != "a") continue;
        if (closing) { if (tag != "br") output += "</" + tag + ">"; continue; }
        output += "<" + tag;
        if (tag == "p" || tag == "li") {
            // Qt records the trailing empty text block explicitly. Without this
            // marker, <p><br></p> reloads as two line breaks and changes the draft.
            // Preserve authored repeated spaces too, without retaining arbitrary CSS.
            QString style = received ? "white-space:pre-wrap;margin-top:0;margin-bottom:7px;line-height:145%" : "white-space:pre-wrap";
            if (tag == "p" && token.contains(QRegularExpression("-qt-paragraph-type\\s*:\\s*empty", QRegularExpression::CaseInsensitiveOption)))
                style += ";-qt-paragraph-type:empty";
            output += " style=\"" + style + "\"";
        }
        if (tag == "span") {
            QStringList styles;
            if (token.contains(QRegularExpression("font-weight\\s*:\\s*(?:[6-9]00|bold)", QRegularExpression::CaseInsensitiveOption))) styles << "font-weight:700";
            if (token.contains(QRegularExpression("font-style\\s*:\\s*italic", QRegularExpression::CaseInsensitiveOption))) styles << "font-style:italic";
            if (token.contains(QRegularExpression("text-decoration\\s*:\\s*underline", QRegularExpression::CaseInsensitiveOption))) styles << "text-decoration:underline";
            if (!styles.isEmpty()) output += " style=\"" + styles.join(';') + "\"";
        }
        if (tag == "a") {
            const auto href = QRegularExpression("href\\s*=\\s*[\"']([^\"']*)[\"']", QRegularExpression::CaseInsensitiveOption).match(token);
            const QUrl url(href.captured(1), QUrl::StrictMode);
            if (url.isValid() && (url.scheme() == "https" || url.scheme() == "http" || url.scheme() == "mailto")) output += " href=\"" + url.toString().toHtmlEscaped() + "\"";
        }
        output += ">";
    }
    auto tail = body.mid(end); tail.replace('<', "&lt;").replace('>', "&gt;");
    output += tail;
    if (received) {
        output.remove(QRegularExpression("<p\\b[^>]*>(?:\\s|&nbsp;|<br\\s*/?>)*</p>",QRegularExpression::CaseInsensitiveOption));
        output.replace(QRegularExpression("(?:<br\\s*/?>\\s*){3,}",QRegularExpression::CaseInsensitiveOption),"<br><br>");
    }
    return output;
}
ReplyDocument::ReplyDocument(QObject *parent)
    : QObject(parent),
      m_spelling(new SpellHighlighter(this, SpellChecker::system(), {
          [this] { return typingCaret(); },
          [this] { queueMarks(); },
          [this] { queueRecheck(); }})) {
    connect(QGuiApplication::clipboard(), &QClipboard::dataChanged, this, &ReplyDocument::clipboardAttachableChanged);
}
namespace {
// Copied local files, or none when anything else is copied with them.
QList<QUrl> clipboardFiles(const QMimeData *mime) {
    QList<QUrl> files;
    if (!mime || !mime->hasUrls()) return files;
    for (const auto &url : mime->urls()) {
        if (!url.isLocalFile() || !QFileInfo(url.toLocalFile()).isFile()) return {};
        files.append(url);
    }
    return files;
}
// An image copied without text: a screenshot or a copied picture. Text with
// an image (a spreadsheet range, say) pastes as text.
bool clipboardImage(const QMimeData *mime) { return mime && mime->hasImage() && !mime->hasText(); }
}
bool ReplyDocument::clipboardAttachable() const {
    const auto *mime = QGuiApplication::clipboard()->mimeData();
    return !clipboardFiles(mime).isEmpty() || clipboardImage(mime);
}
// Answers arrive in batches; one recheck covers all that arrived this turn.
void ReplyDocument::queueRecheck() {
    if (m_recheckQueued) return;
    m_recheckQueued = true;
    QMetaObject::invokeMethod(this, [this] {
        m_recheckQueued = false;
        if (m_document) restyle([this] { m_spelling->rehighlight(); });
    }, Qt::QueuedConnection);
}
// Collected once per event-loop pass, after every highlight of that pass.
void ReplyDocument::queueMarks() {
    if (m_marksQueued) return;
    m_marksQueued = true;
    QMetaObject::invokeMethod(this, [this] {
        m_marksQueued = false;
        QVariantList marks;
        if (m_document)
            for (const auto &[start, length] : SpellHighlighter::marks(m_document->textDocument()))
                marks.append(QVariantMap{{"start", start}, {"length", length}});
        if (marks == m_misspellings) return;
        m_misspellings = marks;
        emit misspellingsChanged();
    }, Qt::QueuedConnection);
}
template <class Change> void ReplyDocument::restyle(Change change) {
    const QScopedValueRollback<bool> restyling(m_restyling, true);
    change();
}
QVariantList ReplyDocument::receivedBody(const QString &html,const QString &plain) {
    return ReceivedBody::sections(html,plain,[](const QString &body){return limitedHtml(body,true);});
}
void ReplyDocument::setTextDocument(QQuickTextDocument *document) {
    if (document == m_document) return;
    if (m_document) m_document->textDocument()->disconnect(this);
    m_document = document;
    m_correction = {};
    restyle([this] {
        m_spelling->setDocument(m_document ? m_document->textDocument() : nullptr);
        if (m_document) { m_spelling->prefetch(m_document->textDocument()->toPlainText()); m_spelling->rehighlight(); }
    });
    queueMarks();
    if (m_document) connect(m_document->textDocument(), &QTextDocument::contentsChange, this, &ReplyDocument::typedAt);
    if (m_document) connect(m_document->textDocument(), &QTextDocument::contentsChanged, this, [this] {
        // Spelling marks change only the layout, not the draft.
        if (m_restyling) return;
        updateFormats();
        // Loaded text updates property bindings (including action availability),
        // while only authored edits notify the autosave consumer.
        emit textChanged();
        if (!m_loading) emit contentChanged();
    });
    emit documentChanged();
    emit textChanged();
}
void ReplyDocument::setEditor(QQuickItem *editor) {
    if (editor == m_editor) return;
    if (m_editor) { m_editor->removeEventFilter(this); disconnect(m_editor, nullptr, this, nullptr); }
    m_editor = editor;
    if (m_editor) {
        m_editor->installEventFilter(this);
        // The word being typed is checked once the caret leaves it.
        if (m_editor->metaObject()->indexOfSignal("cursorPositionChanged()") >= 0)
            connect(m_editor, SIGNAL(cursorPositionChanged()), this, SLOT(caretMoved()));
        for (const char *signal : {SIGNAL(cursorPositionChanged()), SIGNAL(selectionStartChanged()), SIGNAL(selectionEndChanged())})
            if (m_editor->metaObject()->indexOfSignal(signal + 1) >= 0) connect(m_editor, signal, this, SLOT(updateFormats()));
        connect(m_editor, &QQuickItem::activeFocusChanged, this, &ReplyDocument::caretMoved);
    }
    emit documentChanged();
}
// The words as sent in plain text: list items read "• " or "1. " first, as
// the engine writes received lists.
QString ReplyDocument::plainText() const {
    if (!m_document) return {};
    const auto document = m_document->textDocument();
    bool lists = false;
    for (auto block = document->begin(); block.isValid() && !lists; block = block.next()) lists = block.textList();
    if (!lists) return document->toPlainText();
    QStringList lines;
    for (auto block = document->begin(); block.isValid(); block = block.next()) {
        auto text = block.text();
        text.replace(QChar::LineSeparator, u'\n').replace(QChar::Nbsp, u' ');
        if (const auto list = block.textList())
            text.prepend(list->format().style() == QTextListFormat::ListDecimal ? QString::number(list->itemNumber(block) + 1) + ". " : QStringLiteral("• "));
        lines << text;
    }
    return lines.join(u'\n');
}
QString ReplyDocument::html() const { return m_document ? limitedHtml(m_document->textDocument()->toHtml()) : QString{}; }
void ReplyDocument::setIdentity(const QString &identity) {
    if (m_identity == identity) return;
    m_identity = identity;
    m_correction = {};
    m_typingAt = -1;
    ++m_session;
    // Equal text still belongs to a different authoring session. Its undo stack
    // must never be able to restore words from the previously selected draft.
    const QScopedValueRollback<bool> loading(m_loading, true);
    if (m_document) m_document->textDocument()->clearUndoRedoStacks();
    if (m_editor) {
        QMetaObject::invokeMethod(m_editor, "select", Q_ARG(int, 0), Q_ARG(int, 0));
        m_editor->setProperty("cursorPosition", 0);
    }
}
void ReplyDocument::load(const QString &body, const QString &value) {
    if (!m_document) return;
    // The saved draft comes back here whenever it changes, including after
    // this editor's own edits are saved. Skip reloads that change neither the
    // words nor the normalised formatting: setPlainText()/setHtml() would
    // reset the cursor and clear undo.
    const bool sameBody = plainText() == body;
    if (sameBody) {
        QString expectedHtml;
        if (value.isEmpty()) {
            QTextDocument plain; plain.setPlainText(body);
            expectedHtml = limitedHtml(plain.toHtml());
        } else expectedHtml = limitedHtml(value);
        if (html() == expectedHtml) return;
    }
    const int start = m_editor ? m_editor->property("selectionStart").toInt() : 0;
    const int end = m_editor ? m_editor->property("selectionEnd").toInt() : 0;
    const int position = m_editor ? m_editor->property("cursorPosition").toInt() : 0;
    m_loading = true;
    m_correction = {};
    m_typingAt = -1;
    ++m_session;
    m_spelling->prefetch(body);
    auto document = m_document->textDocument();
    if (value.isEmpty()) document->setPlainText(body);
    else {
        document->setHtml(limitedHtml(value));
        // Plain content is authoritative: never silently substitute different
        // words from a stale or incompatible HTML alternative. Lists compare
        // with their markers, as they were saved.
        if (plainText() != body) document->setPlainText(body);
    }
    if (sameBody && m_editor) {
        // Another interface may remove formatting without changing the words.
        // Reflect that saved change while retaining this editor's selection.
        if (start != end) QMetaObject::invokeMethod(m_editor, "select", Q_ARG(int, position == start ? end : start), Q_ARG(int, position == start ? start : end));
        else m_editor->setProperty("cursorPosition", position);
    }
    m_loading = false;
}
void ReplyDocument::format(const QString &kind) {
    if (m_readOnly || !m_document || !m_editor) return;
    if (kind == "bullet" || kind == "numbered") { toggleList(kind == "numbered"); m_editor->forceActiveFocus(); return; }
    if (kind != "bold" && kind != "italic" && kind != "underline") return;
    updateFormats();
    const bool on = m_formats.value(kind).toBool();
    QTextCharFormat style;
    if (kind == "bold") style.setFontWeight(on ? QFont::Normal : QFont::Bold);
    else if (kind == "italic") style.setFontItalic(!on);
    else style.setFontUnderline(!on);
    const auto start = m_editor->property("selectionStart").toInt(), end = m_editor->property("selectionEnd").toInt();
    if (start != end) {
        QTextCursor cursor(m_document->textDocument());
        cursor.setPosition(start); cursor.setPosition(end, QTextCursor::KeepAnchor);
        cursor.mergeCharFormat(style);
    } else {
        // Nothing selected: what is typed next here takes the style.
        if (m_typingAt != start) m_typing = {};
        m_typing.merge(style); m_typingAt = start;
    }
    updateFormats();
    m_editor->forceActiveFocus();
}
// The paragraphs from the selection's first to its last become list items,
// or stop being ones when they all already are of this kind.
void ReplyDocument::toggleList(bool numbered) {
    const auto document = m_document->textDocument();
    const auto style = numbered ? QTextListFormat::ListDecimal : QTextListFormat::ListDisc;
    const int last = document->characterCount() - 1;
    const auto first = document->findBlock(qBound(0, m_editor->property("selectionStart").toInt(), last));
    const auto end = document->findBlock(qBound(0, m_editor->property("selectionEnd").toInt(), last));
    bool all = true;
    for (auto block = first; block.isValid(); block = block.next()) {
        const auto list = block.textList();
        if (!list || list->format().style() != style) { all = false; break; }
        if (block == end) break;
    }
    QTextCursor edit(document);
    edit.beginEditBlock();
    // Numbering runs on from a list of the same kind just above.
    QTextList *list = nullptr;
    if (!all) if (const auto above = first.previous().textList(); above && above->format().style() == style) list = above;
    for (auto block = first; block.isValid(); block = block.next()) {
        if (const auto current = block.textList()) current->remove(block);
        QTextCursor at(block);
        if (all) { auto plain = block.blockFormat(); plain.setIndent(0); at.setBlockFormat(plain); }
        else if (list) list->add(block);
        else { QTextListFormat format; format.setStyle(style); format.setIndent(1); list = at.createList(format); }
        if (block == end) break;
    }
    edit.endEditBlock();
    updateFormats();
}
// Text typed where a style was chosen with nothing selected takes it, in the
// same undo step as the typing; typing on carries it along.
void ReplyDocument::typedAt(int position, int removed, int added) {
    if (m_typingAt < 0 || m_loading || m_restyling) return;
    if (removed != 0 || added <= 0 || position != m_typingAt) return;
    m_typingAt = position + added;
    const auto style = m_typing;
    QMetaObject::invokeMethod(this, [this, position, added, style, session = m_session] {
        if (!m_document || session != m_session || position + added > m_document->textDocument()->characterCount() - 1) return;
        QTextCursor cursor(m_document->textDocument());
        cursor.setPosition(position); cursor.setPosition(position + added, QTextCursor::KeepAnchor);
        cursor.joinPreviousEditBlock(); cursor.mergeCharFormat(style); cursor.endEditBlock();
    }, Qt::QueuedConnection);
}
void ReplyDocument::updateFormats() {
    QVariantMap next;
    if (m_document && m_editor) {
        const auto document = m_document->textDocument();
        const int last = document->characterCount() - 1;
        const int start = qBound(0, m_editor->property("selectionStart").toInt(), last), end = qBound(0, m_editor->property("selectionEnd").toInt(), last);
        // The caret moved away from where a style waits for typing.
        if (start != end || start != m_typingAt) m_typingAt = -1;
        QTextCursor cursor(document);
        cursor.setPosition(start); if (end > start) cursor.setPosition(end, QTextCursor::KeepAnchor);
        auto format = cursor.charFormat();
        if (m_typingAt >= 0) format.merge(m_typing);
        const auto list = document->findBlock(start).textList();
        const auto style = list ? list->format().style() : QTextListFormat::ListStyleUndefined;
        next = {{"bold", format.fontWeight() >= QFont::DemiBold}, {"italic", format.fontItalic()}, {"underline", format.fontUnderline()},
            {"bullet", style == QTextListFormat::ListDisc}, {"numbered", style == QTextListFormat::ListDecimal}};
    }
    if (next == m_formats) return;
    m_formats = next; emit formatsChanged();
}
void ReplyDocument::pastePlain() {
    if (m_readOnly || !m_document || !m_editor) return;
    const auto *mime = QGuiApplication::clipboard()->mimeData();
    if (const auto files = clipboardFiles(mime); !files.isEmpty()) { emit filesPasted(files); return; }
    if (clipboardImage(mime)) {
        const auto image = qvariant_cast<QImage>(mime->imageData());
        if (!image.isNull()) { emit imagePasted(image); return; }
    }
    QTextCursor cursor(m_document->textDocument());
    cursor.setPosition(m_editor->property("selectionStart").toInt());
    cursor.setPosition(m_editor->property("selectionEnd").toInt(), QTextCursor::KeepAnchor);
    const auto text = QGuiApplication::clipboard()->text();
    m_spelling->prefetch(text);
    cursor.insertText(text);
    m_editor->setProperty("cursorPosition", cursor.position());
}
void ReplyDocument::undo() { if (!m_readOnly && m_document && m_document->textDocument()->isUndoAvailable()) m_document->textDocument()->undo(); }
void ReplyDocument::redo() { if (!m_readOnly && m_document && m_document->textDocument()->isRedoAvailable()) m_document->textDocument()->redo(); }
namespace {
// Characters that finish a word and so trigger automatic correction.
bool endsWord(const QString &typed) {
    return typed.size() == 1 && (typed[0].isSpace() || QStringLiteral(".,;:!?)]}\"\u201d").contains(typed[0]));
}
}
// Paste shortcuts and drops would otherwise insert rich text from outside the
// allowed formatting; paste goes through pastePlain() and drops are refused.
// A finished word may be corrected, and Undo right after restores it.
bool ReplyDocument::eventFilter(QObject *watched, QEvent *event) {
    if (watched != m_editor) return QObject::eventFilter(watched, event);
    if (event->type() == QEvent::KeyPress) {
        const auto key = static_cast<QKeyEvent *>(event);
        if (key->matches(QKeySequence::Paste)) { pastePlain(); return true; }
        if (leaveList(key)) return true;
        // Undoing without the editor's cursor leaves the caret after the
        // separator that was typed, as it was before the undo.
        if (key->matches(QKeySequence::Undo) && correctionOnTop()) { undo(); return true; }
        if (!(key->modifiers() & (Qt::ControlModifier | Qt::MetaModifier)) && endsWord(key->text())) correctBeforeCaret();
    } else if (event->type() == QEvent::InputMethod) {
        const auto input = static_cast<QInputMethodEvent *>(event);
        if (input->preeditString().isEmpty() && endsWord(input->commitString())) correctBeforeCaret();
    } else if (event->type() == QEvent::Drop) return true;
    return QObject::eventFilter(watched, event);
}
// Return on an empty list item, or Backspace at an item's start, takes that
// paragraph out of its list instead of adding an item or joining paragraphs.
bool ReplyDocument::leaveList(const QKeyEvent *key) {
    if (m_readOnly || !m_document || !m_editor || key->modifiers() & ~(Qt::KeypadModifier)) return false;
    const bool enter = key->key() == Qt::Key_Return || key->key() == Qt::Key_Enter;
    if (!enter && key->key() != Qt::Key_Backspace) return false;
    const int start = m_editor->property("selectionStart").toInt(), end = m_editor->property("selectionEnd").toInt();
    if (start != end) return false;
    const auto block = m_document->textDocument()->findBlock(start);
    const auto list = block.textList();
    if (!list || (enter ? !block.text().isEmpty() : start != block.position())) return false;
    QTextCursor cursor(block);
    cursor.beginEditBlock();
    list->remove(block);
    auto plain = block.blockFormat(); plain.setIndent(0); cursor.setBlockFormat(plain);
    cursor.endEditBlock();
    updateFormats();
    return true;
}
int ReplyDocument::typingCaret() const {
    if (!m_editor || m_readOnly || !m_editor->hasActiveFocus()) return -1;
    return m_editor->property("cursorPosition").toInt();
}
void ReplyDocument::caretMoved() {
    const int pending = m_spelling->pending();
    if (!m_document || pending < 0 || pending == typingCaret()) return;
    const auto block = m_document->textDocument()->findBlock(qMax(0, pending - 1));
    restyle([&] { m_spelling->rehighlightBlock(block); });
}
void ReplyDocument::setSpellChecker(SpellChecker *checker) {
    m_spelling->setChecker(checker);
    m_correction = {};
    if (m_document) restyle([this] { m_spelling->prefetch(m_document->textDocument()->toPlainText()); m_spelling->rehighlight(); });
}
QVariantMap ReplyDocument::spellingAt(int position) {
    const auto checker = m_spelling->checker();
    if (!checker || !m_document) return {};
    const auto block = m_document->textDocument()->findBlock(position);
    if (!SpellHighlighter::checks(block)) return {};
    const int offset = position - block.position();
    for (const auto &word : SpellHighlighter::words(block.text())) {
        if (offset < word.start || offset > word.start + word.text.size()) continue;
        if (!m_spelling->misspelled(word.text)) return {};
        // A menu that opens without guesses beats one that waits for them.
        return {{"word", word.text}, {"start", block.position() + word.start}, {"suggestions", checker->suggestions(word.text, 200).mid(0, 6)}};
    }
    return {};
}
bool ReplyDocument::replaceText(int start, const QString &word, const QString &replacement) {
    if (m_readOnly || !m_document || start < 0 || word.isEmpty()) return false;
    const auto document = m_document->textDocument();
    if (start + word.size() > document->characterCount() - 1) return false;
    QTextCursor cursor(document);
    cursor.setPosition(start); cursor.setPosition(start + int(word.size()), QTextCursor::KeepAnchor);
    if (cursor.selectedText() != word) return false;
    // The last letter's format, so a bold or linked word stays so.
    const auto format = cursor.charFormat();
    cursor.beginEditBlock(); cursor.insertText(replacement, format); cursor.endEditBlock();
    return true;
}
bool ReplyDocument::replaceWord(int start, const QString &word, const QString &replacement) {
    if (!replaceText(start, word, replacement)) return false;
    if (m_editor) m_editor->setProperty("cursorPosition", start + int(replacement.size()));
    return true;
}
void ReplyDocument::learnSpelling(const QString &word) {
    const auto checker = m_spelling->checker();
    if (!checker || word.isEmpty()) return;
    checker->learn(word); m_spelling->accept(word);
    if (m_document) restyle([this] { m_spelling->rehighlight(); });
}
void ReplyDocument::ignoreSpelling(const QString &word) {
    const auto checker = m_spelling->checker();
    if (!checker || word.isEmpty()) return;
    checker->ignore(word); m_spelling->accept(word);
    if (m_document) restyle([this] { m_spelling->rehighlight(); });
}
// Runs before the editor inserts the separator; the correction follows it as
// its own undo step. A word whose correction was undone is left alone.
void ReplyDocument::correctBeforeCaret() {
    const auto checker = m_spelling->checker();
    if (m_readOnly || !checker || !m_document || !m_editor) return;
    if (m_editor->property("selectionStart").toInt() != m_editor->property("selectionEnd").toInt()) return;
    const int caret = m_editor->property("cursorPosition").toInt();
    const auto block = m_document->textDocument()->findBlock(caret);
    if (!SpellHighlighter::checks(block)) return;
    const int offset = caret - block.position();
    for (const auto &word : SpellHighlighter::words(block.text())) {
        if (word.start + word.text.size() != offset) continue;
        const int start = block.position() + word.start;
        if (m_correction.start == start && m_correction.original == word.text) return;
        // Applied after the editor has inserted the separator, and only while
        // the same word is still there in the same session, within a moment.
        const QPointer<ReplyDocument> self(this);
        const auto session = m_session;
        QElapsedTimer asked; asked.start();
        checker->correction(word.text, [self, session, asked, start, original = word.text](const QString &fixed) {
            if (!self || fixed.isEmpty() || fixed == original) return;
            QMetaObject::invokeMethod(self, [self, session, asked, start, original, fixed] {
                if (!self || self->m_session != session || asked.elapsed() > 1500 || !self->replaceText(start, original, fixed)) return;
                self->m_correction = {start, original, fixed, self->m_document->textDocument()->availableUndoSteps()};
            }, Qt::QueuedConnection);
        });
        return;
    }
}
bool ReplyDocument::correctionOnTop() const {
    if (m_readOnly || !m_document || m_correction.steps < 0) return false;
    const auto document = m_document->textDocument();
    if (document->availableUndoSteps() != m_correction.steps) return false;
    QTextCursor cursor(document);
    cursor.setPosition(qMin(m_correction.start, document->characterCount() - 1));
    cursor.setPosition(qMin(m_correction.start + int(m_correction.replacement.size()), document->characterCount() - 1), QTextCursor::KeepAnchor);
    return cursor.selectedText() == m_correction.replacement;
}
