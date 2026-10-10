// Bridges the composer's QML TextEdit document to a draft's plain text and a
// restricted HTML alternative; the plain text stays authoritative. Also owns
// the editor's system spell checking: underlines, suggestions and correction.
#pragma once
#include <QImage>
#include <QObject>
#include <QUrl>
#include <QPointer>
#include <QQuickItem>
#include <QQuickTextDocument>
#include <QTextFormat>
class QKeyEvent;
#include <QtQml/qqmlregistration.h>

class SpellChecker;
class SpellHighlighter;

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
    /// The clipboard holds what paste attaches instead of typing: local
    /// files, or an image without text (a screenshot).
    Q_PROPERTY(bool clipboardAttachable READ clipboardAttachable NOTIFY clipboardAttachableChanged)
    /// What the selection, or the caret with what is typed next, carries:
    /// bold, italic, underline, bullet and numbered, each true or false.
    Q_PROPERTY(QVariantMap formats READ formats NOTIFY formatsChanged)
    /// Misspelled words to underline, as {start, length} document positions.
    /// Display only; updated shortly after each edit or recheck.
    Q_PROPERTY(QVariantList misspellings READ misspellings NOTIFY misspellingsChanged)
public:
    /// Reader sections for a received body: ReceivedBody::sections() with
    /// limitedHtml(..., true) as the sanitiser. Reentrant; MailBackend also
    /// calls it from its reader worker threads.
    static QVariantList receivedBody(const QString &html, const QString &plain);
    explicit ReplyDocument(QObject *parent = nullptr);
    QQuickTextDocument *textDocument() const { return m_document; }
    QQuickItem *editor() const { return m_editor; }
    void setTextDocument(QQuickTextDocument *document);
    void setEditor(QQuickItem *editor);
    QString plainText() const;
    QString html() const;
    bool readOnly() const { return m_readOnly; }
    QVariantList misspellings() const { return m_misspellings; }
    QVariantMap formats() const { return m_formats; }
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
    /// Toggles "bold", "italic" or "underline" on the editor's selection or,
    /// with none, on the text typed next at the caret. "bullet" or "numbered"
    /// turns the selected paragraphs (or the caret's) into that list, or out
    /// of it when they all are already; a list just above with the same style
    /// is continued. Each is one undo step. Other values are ignored.
    Q_INVOKABLE void format(const QString &kind);
    /// Replaces the selection with the clipboard's plain text. Copied local
    /// files emit filesPasted() and an image without text emits imagePasted()
    /// instead; neither changes the text.
    Q_INVOKABLE void pastePlain();
    bool clipboardAttachable() const;
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
    /// The misspelled word at document `position` as {word, start,
    /// suggestions}, or an empty map when that word is spelled correctly, is
    /// quoted or forwarded text, or there is no spell checker.
    Q_INVOKABLE QVariantMap spellingAt(int position);
    /// Replaces `word` at document position `start` with `replacement` as one
    /// undoable edit that keeps the word's formatting, then puts the cursor
    /// after it. Refused (false) when the text there is no longer `word`.
    Q_INVOKABLE bool replaceWord(int start, const QString &word, const QString &replacement);
    /// Adds `word` to the user's dictionary and removes its underlines.
    Q_INVOKABLE void learnSpelling(const QString &word);
    /// Accepts `word` for this session and removes its underlines.
    Q_INVOKABLE void ignoreSpelling(const QString &word);
    /// Uses `checker` instead of the system's (nullptr turns spelling off)
    /// and rechecks the whole document. The caller keeps ownership.
    void setSpellChecker(SpellChecker *checker);
signals:
    void readOnlyChanged();
    void misspellingsChanged();
    void formatsChanged();
    /// The textDocument or editor property changed.
    void documentChanged();
    /// The document content changed for any reason, including load().
    void textChanged();
    /// The user edited the content. Not emitted for load() or setIdentity(),
    /// so it can drive autosave without echoing saved drafts back.
    void contentChanged();
    void clipboardAttachableChanged();
    /// Paste found local files on the clipboard; the composer attaches them.
    void filesPasted(const QList<QUrl> &files);
    /// Paste found an image and no text, such as a screenshot; the composer
    /// attaches it.
    void imagePasted(const QImage &image);
protected:
    bool eventFilter(QObject *watched, QEvent *event) override;
private slots:
    void caretMoved();
    void updateFormats();
private:
    /// A word the checker corrected automatically, and the undo depth right
    /// after it, so Undo can restore the typed word in place.
    struct Correction { int start = -1; QString original; QString replacement; int steps = -1; };
    int typingCaret() const;
    void correctBeforeCaret();
    bool correctionOnTop() const;
    bool replaceText(int start, const QString &word, const QString &replacement);
    void queueMarks();
    void queueRecheck();
    /// Runs a highlighting pass. Layout-only format changes can still emit
    /// contentsChanged; these must not look like edits.
    template <class Change> void restyle(Change change);
    void toggleList(bool numbered);
    bool leaveList(const QKeyEvent *key);
    void typedAt(int position, int removed, int added);
    QPointer<QQuickTextDocument> m_document;
    QPointer<QQuickItem> m_editor;
    SpellHighlighter *m_spelling = nullptr;
    Correction m_correction;
    QVariantList m_misspellings;
    bool m_marksQueued = false;
    bool m_recheckQueued = false;
    // Bumped by load() and setIdentity(): late corrections for other text are dropped.
    int m_session = 0;
    QString m_identity;
    bool m_loading = false;
    bool m_readOnly = false;
    bool m_restyling = false;
    // Bold, italic or underline chosen with nothing selected, for what is
    // typed next at m_typingAt; the caret moving elsewhere drops it.
    QTextCharFormat m_typing;
    int m_typingAt = -1;
    QVariantMap m_formats;
};
