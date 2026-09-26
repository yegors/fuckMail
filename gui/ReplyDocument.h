// Bridges the composer's QML TextEdit document to a draft's plain text and a
// restricted HTML alternative; the plain text stays authoritative.
#pragma once
#include <QObject>
#include <QPointer>
#include <QQuickItem>
#include <QQuickTextDocument>
#include <QtQml/qqmlregistration.h>

/// A human-authored, limited-format editor. Received messages never enter it.
///
/// Registered in the FuckMail QML module and instantiated by the composer
/// QML beside its TextEdit. GUI thread only. It holds weak (QPointer)
/// references to the document and editor, which QML owns.
class ReplyDocument : public QObject {
    Q_OBJECT
    QML_ELEMENT
    /// The TextEdit's `textDocument`. Its content changes drive textChanged
    /// and contentChanged.
    Q_PROPERTY(QQuickTextDocument *textDocument READ textDocument WRITE setTextDocument NOTIFY documentChanged)
    /// The TextEdit itself. Used for its selection and cursor, and filtered so
    /// that paste inserts plain text and drag-and-drop is ignored.
    Q_PROPERTY(QQuickItem *editor READ editor WRITE setEditor NOTIFY documentChanged)
    /// The draft's words; the saved body and always authoritative.
    Q_PROPERTY(QString plainText READ plainText NOTIFY textChanged)
    /// The same content as restricted HTML (paragraphs, line breaks, bold,
    /// italic, underline, lists and http/https/mailto links), from limitedHtml().
    Q_PROPERTY(QString html READ html NOTIFY textChanged)
    /// Disables format(), pastePlain(), undo() and redo(). The TextEdit's own
    /// readOnly must be bound separately.
    Q_PROPERTY(bool readOnly READ readOnly WRITE setReadOnly NOTIFY readOnlyChanged)
public:
    /// Reader sections for a received body: ReceivedBody::sections() with
    /// limitedHtml(..., true) as the sanitiser. Reentrant; MailBackend also
    /// calls it from its reader worker threads.
    static QVariantList receivedBody(const QString &html, const QString &plain);
    explicit ReplyDocument(QObject *parent = nullptr) : QObject(parent) {}
    QQuickTextDocument *textDocument() const { return m_document; }
    QQuickItem *editor() const { return m_editor; }
    void setTextDocument(QQuickTextDocument *document);
    void setEditor(QQuickItem *editor);
    QString plainText() const;
    QString html() const;
    bool readOnly() const { return m_readOnly; }
    void setReadOnly(bool value) { if (m_readOnly == value) return; m_readOnly = value; emit readOnlyChanged(); }
    /// Marks the start of a different authoring session (another draft or
    /// reply target). On a change it clears undo/redo, so undo cannot bring
    /// back another draft's words, and moves the cursor to the start. Does
    /// not load text; call load() for that.
    Q_INVOKABLE void setIdentity(const QString &identity);
    /// Shows a saved draft: `body` is its plain text and `html` its optional
    /// restricted HTML. Does nothing when the editor already shows the same
    /// words and formatting. If `html` yields different words, `body` wins.
    /// When only the formatting changed, the selection is kept. Emits
    /// textChanged but not contentChanged.
    Q_INVOKABLE void load(const QString &body, const QString &html);
    /// Toggles "bold", "italic" or "underline" on the editor's selection.
    /// Other values are ignored.
    Q_INVOKABLE void format(const QString &kind);
    /// Replaces the selection with the clipboard's plain text.
    Q_INVOKABLE void pastePlain();
    Q_INVOKABLE void undo();
    Q_INVOKABLE void redo();
    /// Rebuilds `html` keeping only the allowed tags, a fixed paragraph style,
    /// bold/italic/underline span styles and http/https/mailto hrefs. Images,
    /// scripts, styles, event handlers and other attributes are dropped.
    ///
    /// With `received`, headings, tables, blockquote, hr, pre and code are
    /// also kept, images become their `[alt]` text, empty paragraphs and long
    /// runs of line breaks collapse, and nested or large tables are flattened
    /// to text so QTextDocument layout stays fast.
    static QString limitedHtml(const QString &html, bool received = false);
signals:
    void readOnlyChanged();
    /// The textDocument or editor property changed.
    void documentChanged();
    /// The document content changed for any reason, including load().
    void textChanged();
    /// The user edited the content. Not emitted for load() or setIdentity(),
    /// so it can drive autosave without echoing saved drafts back.
    void contentChanged();
protected:
    bool eventFilter(QObject *watched, QEvent *event) override;
private:
    QPointer<QQuickTextDocument> m_document;
    QPointer<QQuickItem> m_editor;
    QString m_identity;
    bool m_loading = false;
    bool m_readOnly = false;
};
