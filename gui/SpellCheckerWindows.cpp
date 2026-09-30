// SpellChecker::platform() for Windows: the Windows Spell Checking API
// (ISpellChecker) in the user's default language, falling back to en-US. It
// answers on the GUI thread, word by word, and applies no automatic
// corrections.
#include "SpellChecker.h"

#include <windows.h>
#include <objbase.h>
#include <spellcheck.h>
#include <wrl/client.h>

using Microsoft::WRL::ComPtr;

namespace {
LPCWSTR wide(const QString &text) { return reinterpret_cast<LPCWSTR>(text.utf16()); }

class WindowsSpellChecker final : public SpellChecker {
public:
    explicit WindowsSpellChecker(ComPtr<ISpellChecker> checker) : m_checker(std::move(checker)) {}
    void check(const QStringList &words, std::function<void(const QList<bool> &)> done) override {
        QList<bool> result;
        for (const auto &word : words) {
            ComPtr<IEnumSpellingError> errors;
            ComPtr<ISpellingError> error;
            result.append(SUCCEEDED(m_checker->Check(wide(word), errors.GetAddressOf())) && errors && errors->Next(error.GetAddressOf()) == S_OK);
        }
        done(result);
    }
    QStringList suggestions(const QString &word, int) override {
        QStringList result;
        ComPtr<IEnumString> guesses;
        if (FAILED(m_checker->Suggest(wide(word), guesses.GetAddressOf())) || !guesses) return result;
        LPOLESTR guess = nullptr;
        while (guesses->Next(1, &guess, nullptr) == S_OK) {
            result.append(QString::fromWCharArray(guess));
            CoTaskMemFree(guess);
        }
        return result;
    }
    void correction(const QString &, std::function<void(const QString &)> done) override { done({}); }
    void learn(const QString &word) override { m_checker->Add(wide(word)); }
    void ignore(const QString &word) override { m_checker->Ignore(wide(word)); }
private:
    ComPtr<ISpellChecker> m_checker;
};

SpellChecker *create() {
    // Qt's Windows platform has already joined this thread to a COM
    // apartment; this only matters for other platforms such as offscreen.
    CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);
    ComPtr<ISpellCheckerFactory> factory;
    if (FAILED(CoCreateInstance(__uuidof(SpellCheckerFactory), nullptr, CLSCTX_INPROC_SERVER,
                                __uuidof(ISpellCheckerFactory), reinterpret_cast<void **>(factory.GetAddressOf()))))
        return nullptr;
    wchar_t locale[LOCALE_NAME_MAX_LENGTH] = {};
    const QString user = GetUserDefaultLocaleName(locale, LOCALE_NAME_MAX_LENGTH) > 0 ? QString::fromWCharArray(locale) : QString();
    for (const QString &language : {user, QStringLiteral("en-US")}) {
        BOOL supported = FALSE;
        if (language.isEmpty() || FAILED(factory->IsSupported(wide(language), &supported)) || !supported) continue;
        ComPtr<ISpellChecker> checker;
        if (SUCCEEDED(factory->CreateSpellChecker(wide(language), checker.GetAddressOf())) && checker)
            return new WindowsSpellChecker(std::move(checker));
    }
    return nullptr;
}
}

SpellChecker *SpellChecker::platform() {
    static SpellChecker *checker = create();
    return checker;
}
