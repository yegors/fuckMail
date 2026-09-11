#include <QGuiApplication>
#include <QDebug>
#include <QFont>
#include <QDir>
#include <QSaveFile>
#include <QTextCursor>
#include <QTextDocument>
#include <QTextDocumentFragment>

// P0-04 feasibility probe only. The initial GUI persists plain text; formatted
// drafts will join the shared schema before exposing formatting controls.
int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    QTextDocument document;
    document.setDefaultFont(QFont("Sans Serif", 12));
    document.setUndoRedoEnabled(true);
    const QString authored = QString::fromUtf8("Hello Maya,\nMontréal at 14:00 — confirmed.\n");
    QTextCursor cursor(&document);
    cursor.insertText(authored);
    if (document.toPlainText() != authored) return 1;
    cursor.setPosition(6); cursor.setPosition(10, QTextCursor::KeepAnchor);
    if (cursor.selectedText() != "Maya") return 2;
    QTextCharFormat bold;
    bold.setFontWeight(QFont::Bold);
    cursor.mergeCharFormat(bold);
    auto exported = document.toHtml();
    if (!exported.contains("font-weight:700") || document.toPlainText() != authored) return 3;
    document.undo();
    if (document.toHtml().contains("font-weight:700") || document.toPlainText() != authored) return 4;
    document.redo();
    QTextDocument restored;
    restored.setDefaultFont(document.defaultFont());
    restored.setHtml(document.toHtml());
    if (restored.toPlainText() != authored) return 5;
    QTextCursor restoredCursor(&restored);
    restoredCursor.setPosition(6); restoredCursor.setPosition(10, QTextCursor::KeepAnchor);
    if (restoredCursor.charFormat().fontWeight() != QFont::Bold) return 6;
    QTextCharFormat emphasis; emphasis.setFontItalic(true);
    restoredCursor.mergeCharFormat(emphasis);
    if (!restored.toHtml().contains("font-style:italic")) return 7;
    if (app.arguments().size() == 3 && app.arguments()[1] == "--export-dir") {
        const QDir directory(app.arguments()[2]);
        if (!directory.exists()) return 8;
        const auto write = [&directory](const QString &name, const QByteArray &bytes) {
            QSaveFile file(directory.filePath(name));
            return file.open(QIODevice::WriteOnly) && file.write(bytes) == bytes.size() && file.commit();
        };
        if (!write("composer-qt.html", restored.toHtml().toUtf8()) ||
            !write("composer-qt.txt", restored.toPlainText().toUtf8())) return 9;
    }
    qInfo() << "Composer probe passed: exact Unicode text, selection, bold/italic, undo/redo, native HTML round-trip. GUI draft persistence remains plain text.";
    return 0;
}
