#include "ReplyDocument.h"
#include <QClipboard>
#include <QGuiApplication>
#include <QKeyEvent>
#include <QRegularExpression>
#include <QScopedValueRollback>
#include <QTextCursor>
#include <QTextDocument>
#include <QUrl>

QString ReplyDocument::limitedHtml(const QString &input) {
    // Reconstruct every tag/attribute, keeping only the formatting we expose.
    // Resource tags, image URLs, CSS URLs and event attributes cannot survive.
    QString body = input;
    const auto bodyMatch = QRegularExpression("<body\\b[^>]*>([\\s\\S]*?)</body\\s*>", QRegularExpression::CaseInsensitiveOption).match(body);
    if (bodyMatch.hasMatch()) body = bodyMatch.captured(1);
    body.remove(QRegularExpression("<(head|style|script)\\b[^>]*>[\\s\\S]*?</\\1\\s*>", QRegularExpression::CaseInsensitiveOption));
    static const QRegularExpression tags("<[^>]*>");
    static const QRegularExpression name("^<\\s*(/?)\\s*([a-zA-Z0-9]+)");
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
        if (tag != "p" && tag != "div" && tag != "br" && tag != "span" && tag != "b" && tag != "strong" && tag != "i" && tag != "em" && tag != "u" && tag != "ul" && tag != "ol" && tag != "li" && tag != "a") continue;
        if (closing) { if (tag != "br") output += "</" + tag + ">"; continue; }
        output += "<" + tag;
        if (tag == "p" || tag == "li") {
            // Qt records the trailing empty text block explicitly. Without this
            // marker, <p><br></p> reloads as two line breaks and changes the draft.
            // Preserve authored repeated spaces too, without retaining arbitrary CSS.
            QString style = "white-space:pre-wrap";
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
    return output + tail;
}
void ReplyDocument::setTextDocument(QQuickTextDocument *document) {
    if (document == m_document) return;
    if (m_document) m_document->textDocument()->disconnect(this);
    m_document = document;
    if (m_document) connect(m_document->textDocument(), &QTextDocument::contentsChanged, this, [this] { if (!m_loading) emit contentChanged(); });
    emit documentChanged();
}
void ReplyDocument::setEditor(QQuickItem *editor) {
    if (editor == m_editor) return;
    if (m_editor) m_editor->removeEventFilter(this);
    m_editor = editor;
    if (m_editor) m_editor->installEventFilter(this);
    emit documentChanged();
}
QString ReplyDocument::plainText() const { return m_document ? m_document->textDocument()->toPlainText() : QString{}; }
QString ReplyDocument::html() const { return m_document ? limitedHtml(m_document->textDocument()->toHtml()) : QString{}; }
void ReplyDocument::setIdentity(const QString &identity) {
    if (m_identity == identity) return;
    m_identity = identity;
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
    if (!m_document || !m_editor) return;
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
    if (!m_document || !m_editor) return;
    QTextCursor cursor(m_document->textDocument());
    cursor.setPosition(m_editor->property("selectionStart").toInt());
    cursor.setPosition(m_editor->property("selectionEnd").toInt(), QTextCursor::KeepAnchor);
    cursor.insertText(QGuiApplication::clipboard()->text());
    m_editor->setProperty("cursorPosition", cursor.position());
}
void ReplyDocument::undo() { if (m_document && m_document->textDocument()->isUndoAvailable()) m_document->textDocument()->undo(); }
void ReplyDocument::redo() { if (m_document && m_document->textDocument()->isRedoAvailable()) m_document->textDocument()->redo(); }
bool ReplyDocument::eventFilter(QObject *watched, QEvent *event) {
    if (watched == m_editor && event->type() == QEvent::KeyPress && static_cast<QKeyEvent *>(event)->matches(QKeySequence::Paste)) { pastePlain(); return true; }
    if (watched == m_editor && event->type() == QEvent::Drop) return true;
    return QObject::eventFilter(watched, event);
}
