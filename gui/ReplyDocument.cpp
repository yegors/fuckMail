// ReplyDocument: loads saved drafts into the QML editor's QTextDocument without
// disturbing selection or undo, applies bold/italic/underline, pastes as plain
// text, reduces HTML to the allowed formatting (also for received bodies), and
// connects the editor to the system spell checker.
#include "ReplyDocument.h"
#include <QSet>
#include "ReceivedBody.h"
#include "SpellChecker.h"
#include <QClipboard>
#include <QElapsedTimer>
#include <QGuiApplication>
#include <QKeyEvent>
#include <QRegularExpression>
#include <QScopedValueRollback>
#include <QTextCursor>
#include <QTextDocument>
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
          [this] { queueRecheck(); }})) {}
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
    if (m_document) connect(m_document->textDocument(), &QTextDocument::contentsChanged, this, [this] {
        // Spelling marks change only the layout, not the draft.
        if (m_restyling) return;
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
        connect(m_editor, &QQuickItem::activeFocusChanged, this, &ReplyDocument::caretMoved);
    }
    emit documentChanged();
}
QString ReplyDocument::plainText() const { return m_document ? m_document->textDocument()->toPlainText() : QString{}; }
QString ReplyDocument::html() const { return m_document ? limitedHtml(m_document->textDocument()->toHtml()) : QString{}; }
void ReplyDocument::setIdentity(const QString &identity) {
    if (m_identity == identity) return;
    m_identity = identity;
    m_correction = {};
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
    ++m_session;
    m_spelling->prefetch(body);
    auto document = m_document->textDocument();
    if (value.isEmpty()) document->setPlainText(body);
    else {
        document->setHtml(limitedHtml(value));
        // Plain content is authoritative: never silently substitute different
        // words from a stale or incompatible HTML alternative.
        if (document->toPlainText() != body) document->setPlainText(body);
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
    QTextCursor cursor(m_document->textDocument());
    const auto start = m_editor->property("selectionStart").toInt(), end = m_editor->property("selectionEnd").toInt();
    cursor.setPosition(start); cursor.setPosition(end, QTextCursor::KeepAnchor);
    QTextCharFormat style = cursor.charFormat();
    if (kind == "bold") style.setFontWeight(style.fontWeight() >= QFont::DemiBold ? QFont::Normal : QFont::Bold);
    else if (kind == "italic") style.setFontItalic(!style.fontItalic());
    else if (kind == "underline") style.setFontUnderline(!style.fontUnderline());
    else return;
    cursor.mergeCharFormat(style); m_editor->forceActiveFocus();
}
void ReplyDocument::pastePlain() {
    if (m_readOnly || !m_document || !m_editor) return;
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
