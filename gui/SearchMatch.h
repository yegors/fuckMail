#pragma once
#include <QColor>
#include <QList>
#include <QPair>
#include <QString>
#include <QStringList>
#include <QSyntaxHighlighter>
#include <QTextCharFormat>
#include <algorithm>
#include <vector>

// Where the search's phrases appear in shown text, by the rules Core's search
// matched them with: case, Latin accents and runs of whitespace ignored,
// punctuation kept, starting at the beginning of a word. Ranges are UTF-16
// [start, end) offsets into the original text.
namespace SearchMatch {

// Text folded for matching, with a map back to the original.
struct Folded {
    QString text;
    // For each unit of `text`, the original range it came from.
    std::vector<int> from, to;
};

// Folds `text` as Core's search does: lower case, combining marks dropped
// after canonical decomposition, stroked letters such as ł mapped to their
// base, and each run of whitespace between words turned into one space.
// Keeping the source range of every folded unit lets matches found in the
// folded text be drawn over the original, even where one character folds to
// several units or to none.
inline Folded fold(const QString &text) {
    Folded out;
    out.text.reserve(text.size());
    int space = -1;
    const auto push = [&out](QChar c, int from, int to) { out.text.append(c); out.from.push_back(from); out.to.push_back(to); };
    for (int i = 0; i < text.size();) {
        const int start = i;
        char32_t code = text.at(i).unicode();
        if (QChar::isHighSurrogate(code) && i + 1 < text.size() && text.at(i + 1).isLowSurrogate())
            code = QChar::surrogateToUcs4(text.at(i), text.at(i + 1)), ++i;
        ++i;
        if (QChar::isSpace(code)) { if (space < 0) space = start; continue; }
        if (space >= 0 && !out.text.isEmpty()) push(u' ', space, space + 1);
        space = -1;
        if (code < 0x80) { push(QChar(char16_t(code >= 'A' && code <= 'Z' ? code + 32 : code)), start, i); continue; }
        const QString decomposed = QString::fromUcs4(&code, 1).toLower().normalized(QString::NormalizationForm_D);
        for (const QChar c : decomposed) {
            if (c.isMark()) continue;
            // Letters with a stroke or dot have no decomposition; the index folds them.
            static const QString stroked = QStringLiteral(u"łđøħŧıŀſ"), bases = QStringLiteral(u"ldohtils");
            const auto at = stroked.indexOf(c);
            push(at >= 0 ? bases.at(at) : c, start, i);
        }
    }
    return out;
}

// Every place any of `phrases` appears in `text`, as sorted UTF-16
// [start, end) ranges with overlapping or touching ranges merged. A phrase
// starting with a letter or digit matches only at the start of a word; its
// end may fall inside one, like Core's prefix match.
inline QList<QPair<int, int>> ranges(const QString &text, const QStringList &phrases) {
    QList<QPair<int, int>> found;
    if (phrases.isEmpty() || text.isEmpty()) return found;
    const auto folded = fold(text);
    for (const auto &value : phrases) {
        const auto phrase = fold(value.trimmed()).text;
        if (phrase.isEmpty()) continue;
        const bool wordStart = phrase.at(0).isLetterOrNumber();
        for (qsizetype at = folded.text.indexOf(phrase); at >= 0; at = folded.text.indexOf(phrase, at + 1)) {
            if (wordStart && at > 0 && folded.text.at(at - 1).isLetterOrNumber()) continue;
            found.append({folded.from[size_t(at)], folded.to[size_t(at + phrase.size() - 1)]});
            at += phrase.size() - 1;
        }
    }
    std::sort(found.begin(), found.end());
    QList<QPair<int, int>> merged;
    for (const auto &range : found) {
        if (!merged.isEmpty() && range.first <= merged.last().second) merged.last().second = std::max(merged.last().second, range.second);
        else merged.append(range);
    }
    return merged;
}

}

// Tints the search's matches in a text document, paragraph by paragraph.
// A phrase that spans a paragraph break is not tinted.
class SearchHighlighter : public QSyntaxHighlighter {
public:
    using QSyntaxHighlighter::QSyntaxHighlighter;
    // Sets the phrases and tint, re-highlighting the document only if either changed.
    void set(const QStringList &phrases, const QColor &color) {
        if (phrases == m_phrases && color == m_color) return;
        m_phrases = phrases; m_color = color;
        rehighlight();
    }
protected:
    void highlightBlock(const QString &text) override {
        if (m_phrases.isEmpty()) return;
        QTextCharFormat mark;
        mark.setBackground(m_color);
        for (const auto &range : SearchMatch::ranges(text, m_phrases)) setFormat(range.first, range.second - range.first, mark);
    }
private:
    QStringList m_phrases;
    QColor m_color;
};
