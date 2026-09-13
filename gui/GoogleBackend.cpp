#include "MailBackend.h"
#include "GoogleOperation.h"
#include <QDesktopServices>
#include <QFileInfo>
#include <QJsonArray>
#include <QScopedValueRollback>
#include <QUrl>

bool MailBackend::accountCapability(const QString &account, const QString &name) const {
    for (const auto &entry : m_accounts) {
        const auto row = entry.toMap();
        if (row["id"].toString() != account) continue;
        if (m_syntheticProfile) return name == "mail_read" || name == "drafts" || name == "mail_write"
            || name == "calendar_read" || name == "calendar_write";
        return row["google"].toMap()["capabilities"].toMap().value(name).toBool();
    }
    return false;
}

QVariantMap MailBackend::googleSyncState(const QString &account) const {
    auto state = m_googleSync.value(account).toMap();
    state["changing"] = m_googleSyncChanging.contains(account);
    return state;
}

void MailBackend::googleOperation(const QString &method, const QString &account, const QJsonObject &params, Callback callback) {
    if (!m_connected || m_shutdown || (m_syntheticProfile && method != "draft_attachment_import")) {
        callback({{"code", "cancelled"}, {"message", "Reconnect to continue Google work."}}, false); return;
    }
    if (m_googleOperations.size() >= 4) {
        callback({{"code", "busy"}, {"message", "Four Google operations are already running. Wait for one to finish."}}, false); return;
    }
    auto operation = new GoogleOperation(account, params["request_id"].toString(), this);
    m_googleOperations.insert(operation);
    operation->start(m_profile, m_enginePid, method, params, [this, operation, callback = std::move(callback)](auto result, bool ok) {
        m_googleOperations.remove(operation);
        operation->deleteLater();
        callback(result, ok);
    });
}

void MailBackend::cancelGoogleOperation(const QString &account, const QString &requestId) {
    const auto operations = m_googleOperations;
    for (auto operation : operations)
        if (operation->account() == account && operation->requestId() == requestId) operation->cancel();
}

void MailBackend::cancelGoogleOperations(bool notify) {
    const auto operations = m_googleOperations;
    for (auto operation : operations) operation->cancel(notify);
    if (!notify) { m_googleOperations.clear(); for (auto operation : operations) delete operation; }
}

void MailBackend::cancelGoogleProviderOperations(const QString &account) {
    const auto operations = m_googleOperations;
    for (auto operation : operations)
        if (operation->account() == account && operation->method() != "draft_attachment_import") operation->cancel();
}

void MailBackend::pollGoogleSync() {
    if (m_syntheticProfile || !m_connected || m_shutdown || m_googleSyncPolling) return;
    m_googleSyncPolling = true;
    QScopedValueRollback<bool> automatic(m_automaticReadScope, true);
    request("google_sync_status", {}, {}, [this](auto result, bool ok) {
        m_googleSyncPolling = false;
        if (!ok) return;
        QVariantMap state;
        for (const auto &entry : result["accounts"].toArray()) {
            auto row = entry.toObject().toVariantMap();
            row["limits"] = result["limits"].toVariant();
            state[row["account"].toString()] = row;
        }
        const auto limits = result["limits"].toObject().toVariantMap();
        if (m_googleSync != state || m_googleSyncLimits != limits) {
            m_googleSync = state; m_googleSyncLimits = limits; emit googleSyncChanged();
        }
    });
}

void MailBackend::pollMailDelivery() {
    if (m_syntheticProfile || !m_connected || m_shutdown || m_mailDeliveryPolling || m_mutatingConversation) return;
    const auto key = qMakePair(m_account, m_selected);
    const auto delivery = m_mailDelivery.value(key);
    const auto requestId = delivery["request_id"].toString(), state = delivery["state"].toString();
    if (requestId.isEmpty() || (state != "pending" && state != "unknown")) return;
    m_mailDeliveryPolling = true;
    QScopedValueRollback<bool> automatic(m_automaticReadScope, true);
    request("google_mail_mutation_status", m_account, {{"request_id", requestId}}, [this, key, requestId](auto result, bool ok) {
        m_mailDeliveryPolling = false;
        if (!ok || m_mailDelivery.value(key)["request_id"].toString() != requestId) return;
        m_mailDelivery[key] = result.toVariantMap();
        if (key == qMakePair(m_account, m_selected)) emit mailDeliveryChanged();
    });
}

void MailBackend::startGoogleSync(const QString &account, const QVariantMap &limits) {
    if (!m_connected || m_shutdown || m_syntheticProfile || account.isEmpty() || m_googleSyncChanging.contains(account)) return;
    m_googleSyncChanging.insert(account); ++m_accountWrites; emit googleSyncChanged();
    request("google_sync_start", account, QJsonObject::fromVariantMap(limits), [this, account, limits](auto result, bool ok) {
        m_googleSyncChanging.remove(account); --m_accountWrites; emit googleSyncChanged();
        if (!ok) { if (!m_shutdown) fail(result["message"].toString("Google sync could not start.")); finishShutdown(); return; }
        // Retain only limits that the engine validated. This preference does not
        // start an account on launch; ownership always comes from this action.
        setPreferences({{"google_sync_limits", limits}});
        pollGoogleSync(); loadAccounts();
        if (m_shutdown) finishShutdown();
    });
}

void MailBackend::pauseGoogleSync(const QString &account) {
    if (!m_connected || m_shutdown || m_syntheticProfile || !googleSyncState(account)["owned"].toBool()
        || m_googleSyncChanging.contains(account)) return;
    m_googleSyncChanging.insert(account); emit googleSyncChanged();
    cancelGoogleProviderOperations(account);
    request("google_sync_release", account, {}, [this, account](auto result, bool ok) {
        m_googleSyncChanging.remove(account); emit googleSyncChanged();
        if (!ok) fail(result["message"].toString("Google sync could not pause."));
        pollGoogleSync();
    });
}

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
