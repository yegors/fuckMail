#include "ReplyDocument.h"

#include <QDebug>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QGuiApplication>
#include <QQmlComponent>
#include <QQmlEngine>
#include <QTextCursor>
#include <QTextDocument>
#include <QThread>
#include <cstdlib>
#include <memory>

namespace {
void require(bool condition, const char *message) {
    if (!condition) { qCritical().noquote() << message; std::exit(1); }
}
struct Editor {
    std::unique_ptr<QObject> object;
    QQuickItem *item = nullptr;
    QQuickTextDocument *document = nullptr;
    ReplyDocument reply;
    explicit Editor(QQmlEngine &engine) {
        QQmlComponent component(&engine);
        component.setData("import QtQuick\nTextEdit { width: 640; height: 400; textFormat: TextEdit.RichText; selectByMouse: true }", QUrl("inmemory:/ReplyEditor.qml"));
        QElapsedTimer loading; loading.start();
        while (component.isLoading() && loading.elapsed() < 5000) {
            QCoreApplication::processEvents(QEventLoop::AllEvents, 20);
            QThread::msleep(5);
        }
        require(!component.isLoading(), "QML editor imports finished within five seconds");
        require(!component.isError(), qPrintable(component.errorString()));
        object.reset(component.create());
        require(object != nullptr, qPrintable(component.errorString()));
        item = qobject_cast<QQuickItem *>(object.get());
        require(item != nullptr, "QML TextEdit supplies a real QQuickItem");
        document = qvariant_cast<QQuickTextDocument *>(item->property("textDocument"));
        require(document != nullptr, "QML TextEdit supplies its actual QTextDocument");
        document->textDocument()->setUndoRedoEnabled(true);
        reply.setTextDocument(document);
        reply.setEditor(item);
    }
    void select(int start, int end) {
        require(QMetaObject::invokeMethod(item, "select", Q_ARG(int, start), Q_ARG(int, end)), "TextEdit selection method is callable");
        require(item->property("selectionStart").toInt() == start && item->property("selectionEnd").toInt() == end, "Selection reaches the QML editor");
    }
    QTextCharFormat formatAt(int start, int end) const {
        QTextCursor cursor(document->textDocument());
        cursor.setPosition(start); cursor.setPosition(end, QTextCursor::KeepAnchor);
        return cursor.charFormat();
    }
};
}

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    QQmlEngine engine;
    Editor authored(engine);
    const auto text = QString::fromUtf8("Hello Zoë,\nMontréal — 東京 👋\nLiteral  <angle> & plain entities.\n");
    int changes = 0;
    QObject::connect(&authored.reply, &ReplyDocument::contentChanged, [&changes] { ++changes; });
    authored.reply.load(text, {});
    require(authored.reply.plainText() == text, "Loading preserves exact Unicode, literal angle brackets, ampersands and trailing newline");
    require(changes == 0, "Loading saved work is not reported as a new user edit");

    authored.select(6, 9);
    authored.reply.format("bold");
    authored.reply.format("italic");
    authored.reply.format("underline");
    auto format = authored.formatAt(6, 9);
    require(format.fontWeight() >= QFont::DemiBold && format.fontItalic() && format.fontUnderline(), "Selected text receives bold, italic and underline together");
    require(authored.reply.plainText() == text, "Formatting never changes authored words");
    require(changes >= 3, "Formatting emits observable content changes for durable saves");

    authored.reply.undo();
    require(!authored.formatAt(6, 9).fontUnderline() && authored.formatAt(6, 9).fontItalic(), "Undo removes the most recent formatting action");
    authored.reply.redo();
    require(authored.formatAt(6, 9).fontUnderline(), "Redo restores the most recent formatting action");
    require(authored.reply.plainText() == text, "Undo and redo preserve Unicode text");

    const auto exported = authored.reply.html();
    require(!exported.contains("<head", Qt::CaseInsensitive) && !exported.contains("<style", Qt::CaseInsensitive), "Export contains no document head or stylesheet");
    require(!exported.contains("p, li"), "Qt stylesheet contents do not leak into body text");
    Editor reopened(engine);
    QTextDocument serialized;
    serialized.setHtml(exported);
    if (serialized.toPlainText() != text) qWarning() << "Serialized text differs:" << serialized.toPlainText() << "expected" << text << "HTML" << exported;
    reopened.reply.load(text, exported);
    require(reopened.reply.plainText() == text, "Export and reload preserve exact authored Unicode text");
    format = reopened.formatAt(6, 9);
    if (format.fontWeight() < QFont::DemiBold || !format.fontItalic() || !format.fontUnderline()) qWarning() << "Restored style" << format.fontWeight() << format.fontItalic() << format.fontUnderline() << "HTML" << exported;
    require(format.fontWeight() >= QFont::DemiBold && format.fontItalic() && format.fontUnderline(), "Export and reload preserve all three offered formats");
    require(reopened.formatAt(0, 5).fontWeight() < QFont::DemiBold, "Formatting stays on the selected word after reload");
    reopened.select(6, 9);
    reopened.reply.load(text, {});
    format = reopened.formatAt(6, 9);
    require(format.fontWeight() < QFont::DemiBold && !format.fontItalic() && !format.fontUnderline(), "An explicit plain alternative removes old formatting even when the words match");
    require(reopened.item->property("selectionStart").toInt() == 6 && reopened.item->property("selectionEnd").toInt() == 9 && reopened.item->property("cursorPosition").toInt() == 9, "Removing only formatting preserves the active text selection");
    require(reopened.reply.plainText() == text, "Removing formatting preserves exact Unicode text");

    Editor isolated(engine);
    const auto sharedText = QString::fromUtf8("Shared Montréal draft text");
    isolated.reply.setIdentity("windscribe/conversation-a");
    isolated.reply.load("Earlier words belonging only to draft A", {});
    auto *isolatedDocument = isolated.document->textDocument();
    QTextCursor authoring(isolatedDocument);
    authoring.beginEditBlock();
    authoring.select(QTextCursor::Document);
    authoring.insertText(sharedText);
    authoring.endEditBlock();
    isolated.select(0, 6);
    isolated.reply.format("bold");
    isolated.reply.undo();
    isolated.select(0, 6);
    const auto sharedHtml = isolated.reply.html();
    require(isolatedDocument->isUndoAvailable() && isolatedDocument->isRedoAvailable(), "Draft A has both an earlier authored text edit and a redoable formatting edit");
    int identityChanges = 0;
    QObject::connect(&isolated.reply, &ReplyDocument::contentChanged, &isolated.reply, [&identityChanges] { ++identityChanges; });

    // A legacy reply keeps this key when its first save gives it the same
    // conversation-based draft ID. Save acknowledgements retain editor history.
    isolated.reply.setIdentity("windscribe/conversation-a");
    isolated.reply.load(sharedText, sharedHtml);
    require(isolatedDocument->isUndoAvailable() && isolatedDocument->isRedoAvailable(), "The same draft identity and content preserve undo and redo across a save acknowledgement");
    require(isolated.item->property("selectionStart").toInt() == 0 && isolated.item->property("selectionEnd").toInt() == 6 && isolated.item->property("cursorPosition").toInt() == 6, "The same draft identity retains its active selection");

    isolated.reply.setIdentity("windscribe/draft-b");
    require(identityChanges == 0, "Clearing history for a different draft is not a new authored edit");
    isolated.reply.load(sharedText, sharedHtml);
    require(identityChanges == 0, "Loading another draft's identical content is not a new authored edit");
    require(!isolatedDocument->isUndoAvailable() && !isolatedDocument->isRedoAvailable(), "A different draft with identical text and formatting starts without another draft's undo or redo history");
    require(isolated.item->property("selectionStart").toInt() == 0 && isolated.item->property("selectionEnd").toInt() == 0 && isolated.item->property("cursorPosition").toInt() == 0, "Changing draft identity clears the previous draft's selection and cursor position");
    isolated.reply.undo();
    isolated.reply.redo();
    require(isolated.reply.plainText() == sharedText && isolated.reply.html() == sharedHtml, "Undo and redo cannot restore draft A's earlier words or formatting into draft B");
    require(identityChanges == 0, "Identity resets and unchanged loads do not emit a new authored edit");

    QTextCursor nextAuthoring(isolatedDocument);
    nextAuthoring.movePosition(QTextCursor::End);
    nextAuthoring.insertText(" from B");
    isolated.reply.undo();
    require(isolated.reply.plainText() == sharedText && isolatedDocument->isRedoAvailable(), "New edits in draft B still have working undo and redo");
    isolated.reply.setIdentity("personal/draft-b");
    isolated.reply.load(sharedText, sharedHtml);
    isolated.reply.redo();
    require(!isolatedDocument->isUndoAvailable() && !isolatedDocument->isRedoAvailable() && isolated.reply.plainText() == sharedText, "Changing accounts also isolates history when the draft identifier and content match");

    const auto richInput = QStringLiteral("<html><head><style>p { color: red; }</style><title>Not authored text</title></head><body><p onclick=\"ignored()\">Keep <strong>bold</strong> and <a href=\"https://example.test/meeting\" onmouseover=\"ignored()\">link</a>.</p><img src=\"https://example.test/image.png\"><iframe src=\"https://example.test/embed\"></iframe></body></html>");
    const auto limited = ReplyDocument::limitedHtml(richInput);
    require(!limited.contains("onclick", Qt::CaseInsensitive) && !limited.contains("onmouseover", Qt::CaseInsensitive), "Unrelated event attributes are absent from serialized formatting");
    require(!limited.contains("<img", Qt::CaseInsensitive) && !limited.contains("<iframe", Qt::CaseInsensitive) && !limited.contains("image.png") && !limited.contains("embed"), "Resource elements and their URLs are not retained");
    require(!limited.contains("color: red") && !limited.contains("Not authored text"), "Metadata and stylesheet contents are not authored text");
    QTextDocument visible;
    visible.setHtml(limited);
    require(visible.toPlainText() == "Keep bold and link.", "Limited formatting retains the intended visible body text");
    require(limited.contains("https://example.test/meeting"), "An explicit ordinary link survives without extra attributes");

    const auto rejectedLink = ReplyDocument::limitedHtml("<p><a href=\"file:///local/example.txt\">label</a></p>");
    require(!rejectedLink.contains("href="), "Unsupported link schemes are absent from the saved formatting");
    reopened.reply.load("Authoritative words", "<p>Different stale words</p>");
    require(reopened.reply.plainText() == "Authoritative words", "A stale HTML alternative cannot replace authoritative authored text");

    qInfo() << "Reply document passed: real QML editor, exact Unicode, selected formatting, undo/redo with draft identity isolation, HTML round trip and limited formatting attributes.";
    return 0;
}
