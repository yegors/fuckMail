// The operating system's spell checker behind one small interface, plus the
// composer's display-only highlighter that finds misspelled words.
// SpellHighlighter.cpp holds the highlighter and SpellChecker::system();
// SpellChecker::platform() comes from SpellChecker.mm (NSSpellChecker) on
// macOS, SpellCheckerWindows.cpp (the Windows Spell Checking API) on Windows
// and SpellChecker.cpp (none) elsewhere.
#pragma once
#include <QHash>
#include <QList>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QSyntaxHighlighter>
#include <QTextBlock>
#include <QTextCharFormat>
#include <functional>

/// Answers spelling questions for words. Used from the GUI thread; answers
/// arrive there too, possibly before the call returns. A checker that talks
/// to a system service does so off the GUI thread, so typing never waits.
class SpellChecker {
public:
    virtual ~SpellChecker() = default;
    /// Calls `done` with one answer per word: true when it is not in the
    /// system dictionary, the user's learned words or this session's ignored
    /// words. Never called when the service does not answer.
    virtual void check(const QStringList &words, std::function<void(const QList<bool> &)> done) = 0;
    /// The system's replacement guesses for `word`, best first. Waits at most
    /// `waitMs` and returns none if the service has not answered by then.
    virtual QStringList suggestions(const QString &word, int waitMs) = 0;
    /// Calls `done` with the replacement the system would apply automatically
    /// as `word` is finished, or an empty string when there is none or the
    /// user has turned automatic correction off.
    virtual void correction(const QString &word, std::function<void(const QString &)> done) = 0;
    /// Adds `word` to the user's own dictionary.
    virtual void learn(const QString &word) = 0;
    /// Accepts `word` for the rest of this session.
    virtual void ignore(const QString &word) = 0;
    /// The checker the app uses: platform(), except in headless runs (the
    /// offscreen and minimal platforms) where there is none.
    static SpellChecker *system();
    /// The platform's checker, created on first use; nullptr where the
    /// platform has none (Linux) or it cannot start.
    static SpellChecker *platform();
};

/// Marks words the checker reports as misspelled in a QTextDocument.
///
/// Marks go on the text layout only, never into the document's character
/// formats, so the saved HTML and plain text are unchanged. They draw nothing
/// themselves; marks() lists them for the editor to underline. Quoted lines
/// ("> …") and everything from a "---------- Forwarded message ----------"
/// line on are not checked; neither are URLs, addresses, words with digits,
/// all-capital words or single letters. The word the caret is still typing is
/// not flagged until the caret leaves it. Answers are cached per word; a word
/// counts as correct until its answer arrives.
class SpellHighlighter : public QSyntaxHighlighter {
public:
    /// One checkable word in a block: its offset from the block start.
    struct Word { int start = 0; QString text; };
    /// What the owning editor supplies.
    struct Hooks {
        /// The editor's cursor position while it is typing, or -1.
        std::function<int()> caret;
        /// Runs whenever a block's marks may have changed.
        std::function<void()> marked;
        /// Runs when a later answer found misspelled words; the owner
        /// rehighlights the document.
        std::function<void()> recheck;
    };
    /// Attach a document with setDocument().
    SpellHighlighter(QObject *parent, SpellChecker *checker, Hooks hooks);
    /// The layout format property that marks a misspelled range.
    static constexpr int Mark = QTextFormat::UserProperty + 0x5e11;
    /// Every marked range in `document` as (position, length), in order.
    static QList<QPair<int, int>> marks(const QTextDocument *document);
    /// Replaces the checker (nullptr disables checking) and forgets every
    /// answer. Does not rehighlight.
    void setChecker(SpellChecker *checker);
    SpellChecker *checker() const { return m_checker; }
    /// The known answer for `word`: false while it is unknown.
    bool misspelled(const QString &word) const { return m_cache.value(word); }
    /// Asks about every checkable word of `text` (which may span lines) in
    /// one request, so highlighting it afterwards sends none.
    void prefetch(const QString &text);
    /// Records that `word` is now accepted (learned or ignored).
    void accept(const QString &word) { m_cache.insert(word, false); }
    /// The checkable words of one block's text.
    static QList<Word> words(const QString &text);
    /// Whether this block is checked: false for quoted lines and for the
    /// forwarded part. Valid once the block has been highlighted.
    static bool checks(const QTextBlock &block);
    /// The document position just past a word left unchecked because the
    /// caret was at its end, or -1.
    int pending() const { return m_pending; }
protected:
    void highlightBlock(const QString &text) override;
private:
    /// Sends the words with no answer yet and none on the way to the checker.
    void ask(const QStringList &words);
    SpellChecker *m_checker = nullptr;
    Hooks m_hooks;
    QHash<QString, bool> m_cache;
    QSet<QString> m_asked;
    QTextCharFormat m_format;
    int m_pending = -1;
    // Bumped by setChecker() so answers meant for an earlier checker are dropped.
    int m_generation = 0;
    bool m_asking = false;
};
