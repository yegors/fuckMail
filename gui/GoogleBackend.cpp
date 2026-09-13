#include "MailBackend.h"
#include <QDesktopServices>
#include <QFileInfo>
#include <QUrl>

void MailBackend::setGoogleAuth(const QJsonObject &status) {
    const auto phase = status["phase"].toString();
    m_googleAuthBusy = phase == "starting" || phase == "waiting_for_browser" || phase == "exchanging" || phase == "persisting" || phase == "cancelling";
    const auto error = status["error"].toObject();
    QString message = error["message"].toString();
    if (message.isEmpty()) {
        if (phase == "starting") message = "Preparing Google sign-in…";
        else if (phase == "waiting_for_browser") message = "Finish signing in with your browser.";
        else if (phase == "exchanging") message = "Verifying your Google account…";
        else if (phase == "persisting") message = "Saving access in your system credential store…";
        else if (phase == "connected") message = "Google account connected.";
        else if (phase == "cancelled") message = "Sign-in cancelled.";
        else if (phase == "cancelling") message = "Cancelling sign-in…";
        else if (phase == "disconnected") message = "Google access paused. Your local mail and drafts are retained.";
    }
    // Never retain authorization URLs or credential material in presentation state.
    m_googleAuth = {{"phase", phase}, {"message", message}, {"account", status["account"].toVariant()}, {"error_code", error["code"].toVariant()}};
    emit googleAuthChanged();
}

void MailBackend::connectGoogle(const QString &configFile, const QString &account, bool reauthorize) {
    if (!m_connected || m_shutdown || m_syntheticProfile || m_googleAuthBusy) return;
    const QUrl fileUrl(configFile);
    const auto path = fileUrl.isLocalFile() ? fileUrl.toLocalFile() : configFile;
    if (!QFileInfo(path).isAbsolute()) { fail("Choose your Google desktop client JSON file."); return; }
    m_googleAuthAttempt.clear();
    m_googleAuthCancelRequested = false;
    setGoogleAuth({{"phase", "starting"}});
    request("google_auth_begin", account, {{"client_config_path", path}, {"reauthorize", reauthorize}}, [this](auto result, bool ok) {
        if (!ok) { setGoogleAuth({{"phase", "failed"}, {"error", result}}); return; }
        m_googleAuthAttempt = result["attempt_id"].toString();
        if (m_shutdown || !m_connected) return;
        if (m_googleAuthCancelRequested) { cancelGoogleAuth(); return; }
        const QUrl authorization(result["authorization_url"].toString());
        if (authorization.scheme() != "https" || authorization.host() != "accounts.google.com" || authorization.path() != "/o/oauth2/v2/auth"
            || !authorization.userInfo().isEmpty() || authorization.port(-1) != -1 || authorization.hasFragment()
            || m_googleAuthAttempt.isEmpty() || !QDesktopServices::openUrl(authorization)) {
            cancelGoogleAuth(); fail("The browser could not open Google sign-in. Try again after checking your default browser."); return;
        }
        setGoogleAuth({{"phase", "waiting_for_browser"}});
        pollGoogleAuth();
    });
}

void MailBackend::pollGoogleAuth() {
    if (!m_connected || m_shutdown || !m_googleAuthBusy || m_googleAuthAttempt.isEmpty()) return;
    const auto attempt = m_googleAuthAttempt;
    request("google_auth_status", {}, {{"attempt_id", attempt}}, [this, attempt](auto result, bool ok) {
        if (attempt != m_googleAuthAttempt) return;
        if (m_googleAuthCancelRequested) return;
        if (!ok) { setGoogleAuth({{"phase", "failed"}, {"error", result}}); return; }
        setGoogleAuth(result);
        if (result["phase"] == "connected") { loadAccounts(); loadCalendarSources(); loadCalendar(); }
        else if (m_googleAuthBusy) QTimer::singleShot(500, this, &MailBackend::pollGoogleAuth);
    });
}

void MailBackend::cancelGoogleAuth() {
    if (!m_googleAuthBusy) return;
    m_googleAuthCancelRequested = true;
    setGoogleAuth({{"phase", "cancelling"}});
    if (m_googleAuthAttempt.isEmpty()) return; // Begin callback cancels before opening a browser.
    const auto attempt = m_googleAuthAttempt;
    request("google_auth_cancel", {}, {{"attempt_id", attempt}}, [this, attempt](auto result, bool ok) {
        if (attempt != m_googleAuthAttempt) return;
        setGoogleAuth(ok ? result : QJsonObject{{"phase", "failed"}, {"error", result}});
    });
}

void MailBackend::disconnectGoogle(const QString &account) {
    if (!m_connected || m_shutdown || m_syntheticProfile || m_googleAuthBusy || account.isEmpty()) return;
    request("google_disconnect", account, {}, [this, account](auto result, bool ok) {
        if (!ok) { setGoogleAuth({{"phase", "failed"}, {"error", result}}); return; }
        setGoogleAuth({{"phase", "disconnected"}, {"account", account}});
        loadAccounts(); loadCalendarSources(); loadCalendar();
    });
}
