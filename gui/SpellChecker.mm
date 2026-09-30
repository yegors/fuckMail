// SpellChecker::platform() for macOS: NSSpellChecker in the user's languages,
// called only from one serial background queue so a slow or unreachable
// spelling service never holds up the GUI thread. It has its own
// spell-document tag, so "Ignore" lasts only for this process, and corrects
// automatically only while System Settings' "Correct spelling automatically"
// is on.
#include "SpellChecker.h"

#include <QCoreApplication>
#include <algorithm>

#import <AppKit/AppKit.h>

namespace {
// Runs `answer` on the GUI thread, unless the application has gone.
template <class Answer> void deliver(Answer answer) {
    if (auto application = QCoreApplication::instance()) QMetaObject::invokeMethod(application, std::move(answer), Qt::QueuedConnection);
}

class MacSpellChecker final : public SpellChecker {
public:
    MacSpellChecker() : m_queue(dispatch_queue_create("com.fuckmail.spelling", DISPATCH_QUEUE_SERIAL)) {
        dispatch_async(m_queue, ^{ m_tag = [NSSpellChecker uniqueSpellDocumentTag]; });
    }
    // Blocks capture reference parameters by reference, so each method copies
    // its arguments into locals the block then copies.
    // One request for the whole batch: the words, space separated.
    void check(const QStringList &batch, std::function<void(const QList<bool> &)> answer) override {
        if (batch.isEmpty()) { answer({}); return; }
        const QStringList words = batch;
        const auto done = std::move(answer);
        dispatch_async(m_queue, ^{
            QList<bool> result(words.size(), false);
            QString joined;
            QList<qsizetype> starts;
            for (const auto &word : words) { starts.append(joined.size()); joined += word; joined += u' '; }
            @autoreleasepool {
                NSString *text = joined.toNSString();
                NSArray<NSTextCheckingResult *> *found = [[NSSpellChecker sharedSpellChecker] checkString:text range:NSMakeRange(0, text.length)
                    types:NSTextCheckingTypeSpelling options:nil inSpellDocumentWithTag:m_tag orthography:nullptr wordCount:nullptr];
                for (NSTextCheckingResult *wrong in found) {
                    const auto next = std::upper_bound(starts.cbegin(), starts.cend(), qsizetype(wrong.range.location));
                    if (next != starts.cbegin()) result[next - starts.cbegin() - 1] = true;
                }
            }
            deliver([done, result] { done(result); });
        });
    }
    QStringList suggestions(const QString &asked, int waitMs) override {
        const QString word = asked;
        // The block may outlive this call when the wait times out.
        __block QStringList result;
        dispatch_semaphore_t answered = dispatch_semaphore_create(0);
        dispatch_async(m_queue, ^{
            @autoreleasepool {
                NSString *text = word.toNSString();
                NSArray<NSString *> *guesses = [[NSSpellChecker sharedSpellChecker] guessesForWordRange:NSMakeRange(0, text.length) inString:text language:nil inSpellDocumentWithTag:m_tag];
                for (NSString *guess in guesses) result.append(QString::fromNSString(guess));
            }
            dispatch_semaphore_signal(answered);
        });
        const bool inTime = dispatch_semaphore_wait(answered, dispatch_time(DISPATCH_TIME_NOW, int64_t(waitMs) * int64_t(NSEC_PER_MSEC))) == 0;
        dispatch_release(answered);
        return inTime ? result : QStringList{};
    }
    void correction(const QString &asked, std::function<void(const QString &)> answer) override {
        const QString word = asked;
        const auto done = std::move(answer);
        dispatch_async(m_queue, ^{
            QString fixed;
            @autoreleasepool {
                NSSpellChecker *checker = [NSSpellChecker sharedSpellChecker];
                if (NSSpellChecker.isAutomaticSpellingCorrectionEnabled) {
                    NSString *text = word.toNSString();
                    NSString *replacement = [checker correctionForWordRange:NSMakeRange(0, text.length) inString:text language:checker.language inSpellDocumentWithTag:m_tag];
                    if (replacement) fixed = QString::fromNSString(replacement);
                }
            }
            deliver([done, fixed] { done(fixed); });
        });
    }
    void learn(const QString &asked) override {
        const QString word = asked;
        dispatch_async(m_queue, ^{ @autoreleasepool { [[NSSpellChecker sharedSpellChecker] learnWord:word.toNSString()]; } });
    }
    void ignore(const QString &asked) override {
        const QString word = asked;
        dispatch_async(m_queue, ^{ @autoreleasepool { [[NSSpellChecker sharedSpellChecker] ignoreWord:word.toNSString() inSpellDocumentWithTag:m_tag]; } });
    }
private:
    dispatch_queue_t m_queue;
    // Written and read only on m_queue.
    NSInteger m_tag = 0;
};
}

SpellChecker *SpellChecker::platform() {
    // Lives for the whole process; AppKit may be gone by static destruction.
    static auto *checker = new MacSpellChecker;
    return checker;
}
