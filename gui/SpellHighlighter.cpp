// SpellHighlighter: finds the checkable words of each block, skips quoted and
// forwarded text, caches the checker's answers and marks misspelled words on
// the text layout for the composer to underline. Also SpellChecker::system().
#include "SpellChecker.h"

#include <QRegularExpression>
#include <QTextBlock>
#include <QTextDocument>
#include <QTextLayout>
#include <QGuiApplication>
#include <QPointer>

namespace {
constexpr int ForwardedState = 1;
bool forwardedMarker(const QString &text) {
    return text.trimmed().startsWith(QLatin1String("---------- Forwarded message"));
}
bool quoted(const QString &text) {
    for (const QChar c : text) {
        if (c == '>') return true;
        if (!c.isSpace()) return false;
    }
    return false;
}
// Addresses, links, paths, domains, identifiers and codes are not prose.
bool prose(const QString &text, int start, int end) {
    while (start > 0 && !text[start - 1].isSpace()) --start;
    while (end < text.size() && !text[end].isSpace()) ++end;
    for (int i = start; i < end; ++i) {
        const QChar c = text[i];
        if (c.isDigit() || c == '@' || c == '/' || c == '\\' || c == '_' || c == '#' || c == '=') return false;
        if (c == '.' && i + 1 < end && text[i + 1].isLetter()) return false;
    }
    return true;
}
}

SpellChecker *SpellChecker::system() {
    // Headless runs (tests, captures) never reach the system service.
    const auto name = QGuiApplication::platformName();
    if (name == QLatin1String("offscreen") || name == QLatin1String("minimal")) return nullptr;
    return platform();
}

SpellHighlighter::SpellHighlighter(QObject *parent, SpellChecker *checker, Hooks hooks)
    : QSyntaxHighlighter(parent), m_checker(checker), m_hooks(std::move(hooks)) {
    // Qt Quick ignores the colour and style of layout-only underlines, so the
    // mark itself draws nothing; the composer draws its own red rule.
    m_format.setProperty(Mark, true);
}

QList<QPair<int, int>> SpellHighlighter::marks(const QTextDocument *document) {
    QList<QPair<int, int>> result;
    for (auto block = document->begin(); block.isValid(); block = block.next())
        for (const auto &range : block.layout()->formats())
            if (range.format.boolProperty(Mark)) result.append({block.position() + range.start, range.length});
    return result;
}

void SpellHighlighter::setChecker(SpellChecker *checker) {
    m_checker = checker;
    m_cache.clear(); m_asked.clear();
    ++m_generation;
}

void SpellHighlighter::ask(const QStringList &words) {
    if (!m_checker) return;
    QStringList unknown;
    for (const auto &word : words)
        if (!m_cache.contains(word) && !m_asked.contains(word)) { m_asked.insert(word); unknown.append(word); }
    if (unknown.isEmpty()) return;
    if (m_cache.size() > 50000) m_cache.clear();
    // An answer given during the call is read by the highlighting pass that
    // asked; a later one rechecks the document if it found anything.
    const QPointer<SpellHighlighter> self(this);
    const int generation = m_generation;
    m_asking = true;
    m_checker->check(unknown, [self, generation, unknown](const QList<bool> &answers) {
        if (!self || self->m_generation != generation) return;
        bool found = false;
        for (qsizetype i = 0; i < unknown.size(); ++i) {
            self->m_asked.remove(unknown[i]);
            self->m_cache.insert(unknown[i], answers.value(i));
            found = found || answers.value(i);
        }
        if (found && !self->m_asking && self->m_hooks.recheck) self->m_hooks.recheck();
    });
    m_asking = false;
}

void SpellHighlighter::prefetch(const QString &text) {
    if (!m_checker) return;
    QStringList found;
    for (const auto &line : text.split(QRegularExpression(QStringLiteral("[\\n\\x{2029}]")))) {
        if (forwardedMarker(line)) break;
        if (quoted(line)) continue;
        for (const auto &word : words(line)) found.append(word.text);
    }
    ask(found);
}

QList<SpellHighlighter::Word> SpellHighlighter::words(const QString &text) {
    static const QRegularExpression pattern(QStringLiteral("[\\p{L}\\p{M}]+(?:['\\x{2019}][\\p{L}\\p{M}]+)*"));
    QList<Word> result;
    auto matches = pattern.globalMatch(text);
    while (matches.hasNext()) {
        const auto match = matches.next();
        const auto word = match.captured();
        const int start = int(match.capturedStart()), end = int(match.capturedEnd());
        const bool capitals = word == word.toUpper() && word != word.toLower();
        if (word.size() < 2 || capitals || !prose(text, start, end)) continue;
        result.append({start, word});
    }
    return result;
}

bool SpellHighlighter::checks(const QTextBlock &block) {
    return block.isValid() && block.userState() != ForwardedState && !quoted(block.text());
}

void SpellHighlighter::highlightBlock(const QString &text) {
    const bool forwarded = previousBlockState() == ForwardedState || forwardedMarker(text);
    setCurrentBlockState(forwarded ? ForwardedState : 0);
    const int base = currentBlock().position();
    if (m_pending >= base && m_pending <= base + text.size()) m_pending = -1;
    if (m_hooks.marked) m_hooks.marked();
    if (!m_checker || forwarded || quoted(text)) return;
    const int caret = m_hooks.caret ? m_hooks.caret() : -1;
    auto found = words(text);
    QStringList asked;
    for (qsizetype i = 0; i < found.size(); ++i) {
        const int end = base + found[i].start + int(found[i].text.size());
        if (end == caret) { m_pending = end; found.remove(i--); continue; }
        asked.append(found[i].text);
    }
    ask(asked);
    for (const auto &word : found)
        if (m_cache.value(word.text)) setFormat(word.start, int(word.text.size()), m_format);
}
