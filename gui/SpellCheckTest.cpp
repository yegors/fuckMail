// Composer spelling test with a real QML TextEdit and a scripted checker:
// underlines on misspelled words only (not quoted or forwarded text, not the
// word being typed), saved HTML and plain text untouched by the marks,
// suggestions and Ignore, automatic correction with Undo, and a light check of
// the real macOS spell checker when FM_NATIVE_SPELLING=1 (it needs the
// system spelling service, which headless sessions may not reach).
#include "ReplyDocument.h"
#include "SpellChecker.h"

#include <QElapsedTimer>
#include <QFile>
#include <QGuiApplication>
#include <QImage>
#include <QKeyEvent>
#include <QQmlComponent>
#include <QQmlEngine>
#include <QQuickTextDocument>
#include <QQuickWindow>
#include <QSet>
#include <QTextBlock>
#include <QTextDocument>
#include <QTextLayout>
#include <QThread>
#include <cstdlib>
#include <memory>
#include <utility>

namespace {
void require(bool condition, const char *message) {
    if (!condition) { qCritical().noquote() << "FAIL:" << message; std::exit(1); }
}
template <class Condition> void await(Condition condition, const char *message) {
    QElapsedTimer timer; timer.start();
    while (!condition() && timer.elapsed() < 5000) { QCoreApplication::processEvents(QEventLoop::AllEvents, 20); QThread::msleep(2); }
    require(condition(), message);
}
// Knows a fixed word list; everything else is misspelled.
class ScriptedChecker final : public SpellChecker {
public:
    QSet<QString> known{"Hello", "world", "Another", "line", "the", "quick", "From", "and", "Help"};
    QHash<QString, QStringList> guesses{{"Helo", {"Hello", "Help"}}};
    QHash<QString, QString> corrections{{"teh", "the"}};
    QStringList asked, learned, ignored;
    int calls = 0;
    // When set, answers wait in `held` until the test releases them.
    bool deferred = false;
    QList<std::function<void()>> held;
    void check(const QStringList &words, std::function<void(const QList<bool> &)> done) override {
        ++calls; QList<bool> result;
        for (const auto &word : words) { asked << word; result << !known.contains(word); }
        if (deferred) held << [done, result] { done(result); }; else done(result);
    }
    QStringList suggestions(const QString &word, int) override { return guesses.value(word); }
    void correction(const QString &word, std::function<void(const QString &)> done) override {
        const auto fixed = corrections.value(word);
        if (deferred) held << [done, fixed] { done(fixed); }; else done(fixed);
    }
    void learn(const QString &word) override { learned << word; known.insert(word); }
    void ignore(const QString &word) override { ignored << word; known.insert(word); }
};
struct Harness {
    std::unique_ptr<QObject> root;
    QQuickWindow *window = nullptr;
    QQuickItem *editor = nullptr;
    QTextDocument *text = nullptr;
    ReplyDocument reply;
    explicit Harness(QQmlEngine &engine) {
        QQmlComponent component(&engine);
        component.setData("import QtQuick\nWindow { width: 640; height: 400; visible: true; color: 'white'\n"
                          " TextEdit { objectName: 'editor'; anchors.fill: parent; textFormat: TextEdit.RichText; color: 'black'; font.pixelSize: 24 } }",
                          QUrl("inmemory:/SpellHarness.qml"));
        await([&] { return !component.isLoading(); }, "harness imports load");
        require(!component.isError(), qPrintable(component.errorString()));
        root.reset(component.create());
        window = qobject_cast<QQuickWindow *>(root.get());
        require(window != nullptr, "harness window");
        editor = window->contentItem()->childItems().value(0);
        require(editor && editor->objectName() == "editor", "harness editor");
        auto document = qvariant_cast<QQuickTextDocument *>(editor->property("textDocument"));
        require(document != nullptr, "editor document");
        text = document->textDocument();
        text->setUndoRedoEnabled(true);
        reply.setTextDocument(document);
        reply.setEditor(editor);
    }
    int caret() const { return editor->property("cursorPosition").toInt(); }
    void setCaret(int position) { editor->setProperty("cursorPosition", position); }
    void key(int code, const QString &typed, Qt::KeyboardModifiers modifiers = Qt::NoModifier) {
        QKeyEvent press(QEvent::KeyPress, code, modifiers, typed), release(QEvent::KeyRelease, code, modifiers, typed);
        QCoreApplication::sendEvent(window, &press); QCoreApplication::sendEvent(window, &release);
        QCoreApplication::processEvents();
    }
    void type(const QString &typed) {
        for (const QChar c : typed) key(c == ' ' ? Qt::Key_Space : Qt::Key_A + (c.toLower().unicode() - 'a'), QString(c));
    }
    // Whether document position `position` carries the spelling mark.
    bool marked(int position) const {
        const auto block = text->findBlock(position);
        const int offset = position - block.position();
        for (const auto &range : block.layout()->formats())
            if (offset >= range.start && offset < range.start + range.length && range.format.boolProperty(SpellHighlighter::Mark)) return true;
        return false;
    }
    int position(const QString &needle) const { return int(text->toPlainText().indexOf(needle)); }
};
}

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    QQmlEngine engine;
    Harness h(engine);
    ScriptedChecker checker;
    h.reply.setSpellChecker(&checker);
    int edits = 0;
    QObject::connect(&h.reply, &ReplyDocument::contentChanged, [&] { ++edits; });

    const QString draft = "Helo world\n> quoted wrnog\nAnother lin with https://exampel.test/pathh and me@exampel.test\n"
                          "---------- Forwarded message ----------\nFrom: Sendr\n\n> forwrded text\nplain forwrded line";
    h.reply.load(draft, {});
    QCoreApplication::processEvents(); QCoreApplication::processEvents();
    require(h.marked(0) && h.marked(3), "a misspelled word is underlined");
    require(!h.marked(h.position("world")), "a correctly spelled word is not underlined");
    require(!h.marked(h.position("wrnog")), "quoted lines are not underlined");
    require(h.marked(h.position("lin ")), "each line is checked");
    require(!h.marked(h.position("exampel")) && !h.marked(h.position("pathh")) && !h.marked(h.position("me@")), "links and addresses are not checked");
    require(!h.marked(h.position("Sendr")) && !h.marked(h.position("plain forwrded")), "the forwarded part is not underlined");
    require(!checker.asked.contains("wrnog") && !checker.asked.contains("forwrded") && !checker.asked.contains("exampel"), "skipped text never reaches the checker");
    require(edits == 0, "loading and highlighting are not reported as edits");

    // The marks live on the layout only: the saved draft is the same as a
    // document that was never checked.
    QTextDocument unchecked; unchecked.setPlainText(draft);
    require(h.reply.plainText() == draft, "plain text is unchanged by spelling marks");
    require(h.reply.html() == ReplyDocument::limitedHtml(unchecked.toHtml()), "saved HTML is unchanged by spelling marks");
    require(!h.text->toHtml().contains("underline") && !h.text->toHtml().contains("#e5484d", Qt::CaseInsensitive), "the document's own HTML has no spelling formats");

    // The editor gets the marked ranges to underline; the layout marks
    // themselves draw nothing.
    await([&] { return !h.reply.misspellings().isEmpty(); }, "marks are published");
    QStringList listed;
    for (const auto &value : h.reply.misspellings()) { const auto mark = value.toMap(); listed << h.text->toPlainText().mid(mark["start"].toInt(), mark["length"].toInt()); }
    require(listed == QStringList({"Helo", "lin", "with"}), "the published marks are exactly the misspelled words");
    await([&] { return h.window->isExposed(); }, "window exposed");
    const QImage marked = h.window->grabWindow();
    h.reply.setSpellChecker(nullptr);
    await([&] { return h.reply.misspellings().isEmpty(); }, "no checker, no marks");
    require(h.window->grabWindow() == marked, "layout marks do not change how the text is drawn");
    h.reply.setSpellChecker(&checker);
    require(edits == 0, "rechecking is not reported as an edit");

    // Suggestions come from the checker; choosing one is an undoable edit.
    auto spelling = h.reply.spellingAt(2);
    require(spelling.value("word") == "Helo" && spelling.value("start") == 0, "the word under the pointer is found");
    require(spelling.value("suggestions").toStringList() == QStringList({"Hello", "Help"}), "the checker's suggestions are offered in order");
    require(h.reply.spellingAt(h.position("world") + 1).isEmpty(), "a correct word offers nothing");
    require(h.reply.spellingAt(h.position("wrnog") + 1).isEmpty(), "quoted text offers nothing");
    require(!h.reply.replaceWord(0, "Hxlo", "Hello"), "a stale replacement is refused");
    require(h.reply.replaceWord(0, "Helo", "Hello"), "a suggestion replaces the word");
    require(h.reply.plainText().startsWith("Hello world\n") && !h.marked(0), "the replacement is in the text and unmarked");
    require(edits == 1, "the replacement is a normal edit that autosaves");
    require(h.caret() == 5, "the cursor follows the replacement");
    h.reply.undo();
    require(h.reply.plainText() == draft && h.marked(0), "undo restores the misspelled word and its mark");
    h.reply.redo(); h.reply.undo();

    const int beforeAccepting = edits;
    h.reply.ignoreSpelling("lin");
    require(checker.ignored == QStringList{"lin"} && !h.marked(h.position("lin ")), "Ignore removes the mark");
    h.reply.learnSpelling("Helo");
    require(checker.learned == QStringList{"Helo"} && !h.marked(0), "Learn removes the mark");
    require(h.reply.plainText() == draft && edits == beforeAccepting, "Learn and Ignore change no text");

    // Typing: the word in progress is left alone, a finished word is checked
    // and may be corrected, and Undo brings back what was typed.
    h.reply.setIdentity("typing");
    h.reply.load("", {});
    h.window->requestActivate();
    h.editor->forceActiveFocus();
    await([&] { return h.editor->hasActiveFocus(); }, "editor focused");
    h.type("wrold");
    require(h.reply.plainText() == "wrold" && !h.marked(0), "the word being typed is not underlined");
    require(!checker.asked.contains("wro"), "partial words are not sent to the checker");
    h.setCaret(0);
    require(h.marked(0), "leaving the word checks it");
    h.setCaret(5);
    h.type(" teh");
    require(!h.marked(h.position("teh")), "the new word in progress is not underlined");
    const int before = edits;
    h.type(" ");
    await([&] { return h.reply.plainText() == "wrold the "; }, "a finished word is corrected automatically");
    require(h.caret() == 10 && edits > before, "the correction keeps the caret after the space and autosaves");
    h.key(Qt::Key_Z, {}, Qt::ControlModifier);
    require(h.reply.plainText() == "wrold teh ", "Undo restores the typed word");
    require(h.caret() == 10, "Undo keeps the caret after the space");
    require(h.marked(h.position("teh")), "the restored word is underlined");
    h.key(Qt::Key_Backspace, {});
    h.type(" ");
    QCoreApplication::processEvents(); QCoreApplication::processEvents();
    require(h.reply.plainText() == "wrold teh ", "a correction the user undid is not applied again");
    h.type("quick teh ");
    await([&] { return h.reply.plainText() == "wrold teh quick the "; }, "later words are still corrected");
    h.reply.load("> teh", {});
    h.setCaret(5);
    h.type(" ");
    QCoreApplication::processEvents(); QCoreApplication::processEvents();
    require(h.reply.plainText() == "> teh ", "quoted text is not corrected");

    // A slow checker: nothing waits for it, words stay unmarked until the
    // answers come, and a late correction still applies once.
    checker.deferred = true;
    const int callsBefore = checker.calls;
    h.reply.load("Fresh wurds here\nmore wurds and zzyzx", {});
    require(checker.calls - callsBefore == 1, "a loaded draft is asked about in one request");
    require(!h.marked(h.position("wurds")) && h.reply.plainText().startsWith("Fresh"), "unanswered words are not marked");
    h.setCaret(h.reply.plainText().size());
    h.type(" teh");
    h.type(" ");
    require(h.reply.plainText().endsWith("zzyzx teh "), "typing does not wait for the checker");
    for (const auto &answer : std::exchange(checker.held, {})) answer();
    await([&] { return h.marked(h.position("wurds")) && h.marked(h.position("zzyzx")); }, "late answers mark the words");
    await([&] { return h.reply.plainText().endsWith("zzyzx the "); }, "a late correction still applies");
    for (const auto &answer : std::exchange(checker.held, {})) answer();
    checker.deferred = false;

    h.reply.setSpellChecker(nullptr);
    require(h.reply.spellingAt(3).isEmpty() && !h.marked(2), "without a checker nothing is marked");

    require(SpellChecker::system() == nullptr, "headless runs never use the system checker");
#ifdef Q_OS_MACOS
    if (qEnvironmentVariable("FM_NATIVE_SPELLING") == "1") {
        // The real checker: stable English answers only, plus its speed.
        auto native = SpellChecker::platform();
        require(native != nullptr, "macOS has a system spell checker");
        QList<bool> answers;
        QElapsedTimer timer; timer.start();
        native->check({"the", "qzxwvkj", "receive", "don't", "recieve"}, [&](const QList<bool> &result) { answers = result; });
        require(answers.isEmpty(), "the system checker answers later, not during the call");
        await([&] { return !answers.isEmpty(); }, "the system checker answers");
        qInfo().noquote() << "NSSpellChecker: first answer after" << timer.elapsed() << "ms";
        require(answers == QList<bool>({false, true, false, false, true}), "the system checker answers a batch word by word");
        const auto guesses = native->suggestions("recieve", 2000);
        qInfo().noquote() << "NSSpellChecker suggestions for 'recieve':" << guesses.join(", ");
        require(guesses.contains("receive"), "the system suggests the right spelling");
        QFile dictionary("/usr/share/dict/words");
        if (dictionary.open(QIODevice::ReadOnly)) {
            const auto all = QString::fromUtf8(dictionary.readAll()).split('\n', Qt::SkipEmptyParts);
            QStringList prose; for (int i = 0; i < 1000 && i * 97 < all.size(); ++i) prose << all[i * 97];
            bool done = false;
            timer.restart(); native->check(prose, [&](const QList<bool> &result) { done = result.size() == prose.size(); });
            const auto posted = timer.elapsed();
            await([&] { return done; }, "a long batch is answered");
            qInfo().noquote() << "NSSpellChecker:" << prose.size() << "words answered in" << timer.elapsed() << "ms; the GUI thread spent" << posted << "ms";
        }
    }
#endif
    qInfo() << "Spell check passed: marks, saved draft unchanged, suggestions, ignore/learn, typing, correction and undo";
    return 0;
}
