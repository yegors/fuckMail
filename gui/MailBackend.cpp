#include "MailBackend.h"
#include "ReplyDocument.h"
#include <QClipboard>
#include <QGuiApplication>
#include <QFontDatabase>
#include <QFutureWatcher>
#include <QPromise>
#include <algorithm>
#include <iterator>
#include <memory>
#include "IpcTrust.h"
#include "CalendarLayout.h"

#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDateTime>
#include <QDebug>
#include <QDesktopServices>
#include <QDir>
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QProcess>
#include <QRegularExpression>
#include <QScopedValueRollback>
#include <QUrl>
#include <QUuid>
#include <QTimeZone>

namespace {
constexpr qsizetype maxFrame = 1024 * 1024;
QString uuid() { return QUuid::createUuid().toString(QUuid::WithoutBraces); }
bool quietDiagnosticRead(const QString &method) {
    return method == "status" || method == "google_sync_status" || method == "google_auth_status"
        || method == "assist_job" || method == "assist_status" || method == "assist_progress" || method == "mail_notifications" || method == "calendar_reminders";
}
// A message source can carry up to ~1 MB of raw text that only the source
// dialog shows. Readers and their caches keep the rest.
QJsonObject withoutRaw(QJsonObject source) { source.remove(QStringLiteral("raw")); return source; }

}

MailBackend::MailBackend(QString profile, QString engine, QObject *parent, bool fixtureClock, bool liveProfile)
    : QObject(parent), m_accountModel(this), m_conversationModel(this), m_messageModel(this), m_profile(std::move(profile)), m_engine(std::move(engine)), m_fixtureClock(fixtureClock), m_liveProfileRequested(liveProfile), m_syntheticProfile(!liveProfile) {
    initializeCalendarSearch();
    initializeSendUndo();
    m_readerPool.setMaxThreadCount(1);
    m_readerPrefetchTimer.setSingleShot(true); m_readerPrefetchTimer.setInterval(60);
    connect(&m_readerPrefetchTimer, &QTimer::timeout, this, [this] {
        const auto id = m_readerPrefetchTarget, account = m_account;
        const auto generation = m_generation;
        const auto lifecycle = lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong();
        if (!m_connected || m_shutdown || id.isEmpty() || id == m_selected) return;
        requestReader(account, {{"id", id}}, [this, id, account, generation, lifecycle](auto result, bool ok) {
            if (generation != m_generation || account != m_account || id != m_readerPrefetchTarget) return;
            if (!ok) { m_readerPrefetchTarget.clear(); return; }
            prepareReader(account, id, result.toVariantMap(), generation, lifecycle, true,
                [this, id, account, lifecycle](auto data) { cacheConversation(account, id, data, lifecycle); });
        }, "conversation", true);
    });
    connect(this, &MailBackend::accountChanged, this, [this] { loadOutgoing(); });
    connect(this, &MailBackend::accountChanged, this, [this] {
        if (m_preferences.value("calendar_account_visibility").toString() == "current") loadCalendar();
    });
    connect(this, &MailBackend::conversationChanged, this, [this] {
        const auto scope = QJsonDocument(QJsonArray{m_account, m_selected}).toJson(QJsonDocument::Compact);
        if (!m_messageModel.replace(QString::fromUtf8(scope), conversation().value("messages").toList())) {
            m_messageModel.replace({}, {});
            fail("The message reader received an invalid identity. Refresh to continue.");
        }
    });
    m_notificationTimer.setInterval(15000);
    connect(&m_notificationTimer, &QTimer::timeout, this, &MailBackend::pollNotifications);
    for (auto signal : {&MailBackend::preferencesChanged, &MailBackend::connectedChanged, &MailBackend::startupReadyChanged,
            &MailBackend::accountsChanged, &MailBackend::calendarSourcesChanged})
        connect(this, signal, this, &MailBackend::configureNotifications);
    connect(this, &MailBackend::draftChanged, this, [this] {
        if (!m_googleSendPending.isEmpty() && m_googleSendPending["authored"].toObject() != googleSendAuthorSnapshot())
            cancelPendingGoogleSend("The draft changed before sending. Review it and press Send again.");
    });
    connect(this, &MailBackend::accountChanged, this, [this] {
        resetDraftSourcesContext();
        resetDraftAuthoringContext();
        emit googleDraftInventoryChanged(); emit draftSourcesChanged();
        emit draftAuthoringChanged(); emit draftDeletionChanged();
    });
    connect(this, &MailBackend::draftChanged, this, [this] {
        resetDraftSourcesContext(); emit draftSourcesChanged();
        resetDraftAuthoringContext(); emit draftAuthoringChanged(); emit draftDeletionChanged();
    });
    connect(this, &MailBackend::googleSyncChanged, this, &MailBackend::googleDraftInventoryChanged);
    connect(this, &MailBackend::googleSyncChanged, this, &MailBackend::draftAuthoringChanged);
    connect(this, &MailBackend::googleSyncChanged, this, &MailBackend::draftDeletionChanged);
    connect(this, &MailBackend::googleSyncChanged, this, &MailBackend::draftChanged);
    m_calendarDate = todayDate(); m_timelineDate = todayDate();
    m_clockTimer.setSingleShot(true);
    connect(&m_clockTimer, &QTimer::timeout, this, &MailBackend::tickClock);
    // A changed display zone or profile kind can move today and the current minute.
    connect(this, &MailBackend::preferencesChanged, this, &MailBackend::tickClock);
    connect(this, &MailBackend::connectedChanged, this, &MailBackend::tickClock);
    tickClock();
    m_calendarPrefetchTimer.setSingleShot(true); m_calendarPrefetchTimer.setInterval(250);
    connect(&m_calendarPrefetchTimer, &QTimer::timeout, this, &MailBackend::prefetchCalendarWindows);
    m_navigationSaveTimer.setSingleShot(true); m_navigationSaveTimer.setInterval(800);
    connect(&m_navigationSaveTimer, &QTimer::timeout, this, &MailBackend::savePreferences);
    m_socket.setReadBufferSize(maxFrame + 1);
    m_connectTimer.setInterval(150);
    connect(&m_connectTimer, &QTimer::timeout, this, &MailBackend::attemptConnection);
    connect(&m_socket, &QLocalSocket::readyRead, this, &MailBackend::receive);
    connect(&m_socket, &QLocalSocket::connected, this, [this] {
        QString trustError;
        if (!verifyEnginePeer(m_socket.socketDescriptor(), m_enginePid, trustError)) {
            m_connectTimer.stop(); fail(trustError); m_socket.abort(); return;
        }
        request("hello", {}, {{"token", m_token}, {"client", "gui"}}, [this](auto result, bool ok) {
            if (!ok) {
                // A rejected response already supplied a fatal error. A socket
                // disappearing before hello can be a writer still shutting down.
                if (!m_error.isEmpty()) m_connectTimer.stop();
                m_socket.abort(); return;
            }
            const bool synthetic = result["mode"] != "live";
            if ((m_liveProfileRequested && synthetic) || (!synthetic && m_fixtureClock)) {
                m_connectTimer.stop(); fail("The selected profile does not match the requested live or fixture mode."); m_socket.abort(); return;
            }
            m_syntheticProfile = synthetic;
            m_connectTimer.stop();
            m_connected = true; m_error.clear();
            m_calendarRevision = -1; m_mailRevision = -1;
            emit connectedChanged(); emit errorChanged();
            loadPreferences(); loadCalendarSources();
            if (m_account.isEmpty()) loadAccounts();
            else refresh();
            m_pollTimer.start();
            if (!m_viewDirty.isEmpty()) m_viewTimer.start();
        });
    });
    connect(&m_socket, &QLocalSocket::disconnected, this, [this] {
        const auto wasConnected = m_connected;
        m_connected = false; m_pollTimer.stop(); emit connectedChanged();
        cancelQueuedSends("Pending sends cancelled after disconnection. Drafts are saved.");
        cancelQueuedCalendarDeletions();
        m_mailViewCache.clear(); m_readerCache.clear();
        cancelGoogleOperations();
        m_googleSync.clear(); m_googleSyncLimits.clear(); emit googleSyncChanged();
        m_googleAuthAttempt.clear();
        if (m_googleAuthBusy) setGoogleAuth({{"phase", "cancelled"}});
        m_outgoing.clear(); m_inFlight.clear(); m_buffer.clear();
        auto pending = std::move(m_pending); m_pending.clear();
        for (auto &item : pending) item.callback({}, false);
        if (m_shutdown) finishShutdown();
        else if ((wasConnected || !m_connectTimer.isActive()) && m_error.isEmpty())
            fail("The local engine disconnected. Your unsaved reply remains here. Reconnect to continue.");
    });
    connect(&m_socket, &QLocalSocket::errorOccurred, this, [this](auto) {
        if (m_connected) fail(m_socket.errorString());
    });
    m_pollTimer.setInterval(1000);
    connect(&m_pollTimer, &QTimer::timeout, this, [this] {
        const auto now = QDateTime::currentMSecsSinceEpoch();
        const auto keys = m_pending.keys();
        for (const auto &id : keys) {
            if (m_pending[id].started > 0 && now - m_pending[id].started > 15000) {
                fail("The local engine did not answer in time. Reconnect to continue.");
                m_socket.abort(); return;
            }
        }
        if (m_pending.size() > 12 || m_shutdown) return;
        if (!m_syntheticProfile && (!m_queryActivity.isValid() || m_queryActivity.elapsed() >= 500)) { pollGoogleSync(); pollMailDelivery(); }
        request("status", {}, {}, [this](auto result, bool ok) {
            if (!ok) return;
            const auto diagnostics = result.toVariantMap();
            if (diagnostics != m_diagnostics) { m_diagnostics = diagnostics; emit diagnosticsChanged(); }
            auto revision = result["revision"].toInteger();
            const auto calendarRevision = result["calendar_generation"].toInteger(-1);
            const bool calendarChanged = calendarRevision < 0 || calendarRevision != m_calendarRevision;
            if (calendarChanged) loadCalendarSources();
            const bool changed = m_revision != revision;
            const auto mailRevision = result["mail_generation"].toInteger(-1);
            if (mailRevision < 0 ? changed : mailRevision != m_mailRevision) m_listRefreshPending = true;
            m_mailRevision = mailRevision;
            // A foreground query already reads current store state. Between
            // successive inputs, do not place another expensive background
            // query ahead of the next input on this serial socket. Retain the
            // invalidation and deliver it once editing has settled.
            const bool editing = m_queryActivity.isValid() && m_queryActivity.elapsed() < 500;
            const bool refreshList = m_listRefreshPending && !m_loading && !m_searchTimer.isActive() && !editing && !m_browsingOlder;
            if (refreshList) m_listRefreshPending = false;
            if (changed) refreshState(refreshList, calendarChanged, true);
            else {
                if (refreshList) { QScopedValueRollback<bool> origin(m_automaticReadScope, true); loadConversations(false, true); }
                if (calendarChanged) loadCalendar();
            }
            m_revision = revision;
            m_calendarRevision = calendarRevision;
        });
    });
    // Coalesce an input burst without spending most of the paint budget waiting.
    // Superseded queued queries are discarded by sendNext().
    m_searchTimer.setSingleShot(true); m_searchTimer.setInterval(25);
    m_searchTimer.setTimerType(Qt::PreciseTimer);
    connect(&m_searchTimer, &QTimer::timeout, this, [this] { loadConversations(); });
    m_draftTimer.setSingleShot(true); m_draftTimer.setInterval(450);
    connect(&m_draftTimer, &QTimer::timeout, this, &MailBackend::saveDraft);
    // Draft chips follow saves, account changes and synced Gmail drafts.
    m_draftChipsTimer.setSingleShot(true); m_draftChipsTimer.setInterval(600);
    connect(&m_draftChipsTimer, &QTimer::timeout, this, &MailBackend::refreshDraftChips);
    for (auto signal : {&MailBackend::draftChanged, &MailBackend::accountChanged, &MailBackend::accountsChanged, &MailBackend::connectedChanged})
        connect(this, signal, &m_draftChipsTimer, qOverload<>(&QTimer::start));
    // The server is searched once typing pauses, not per keystroke.
    m_remoteSearchTimer.setSingleShot(true); m_remoteSearchTimer.setInterval(600);
    connect(&m_remoteSearchTimer, &QTimer::timeout, this, &MailBackend::runRemoteSearch);
    connect(this, &MailBackend::queryChanged, this, &MailBackend::scheduleRemoteSearch);
    connect(this, &MailBackend::accountChanged, this, &MailBackend::scheduleRemoteSearch);
    m_viewTimer.setSingleShot(true); m_viewTimer.setInterval(700);
    connect(&m_viewTimer, &QTimer::timeout, this, [this] {
        for (const auto &account : m_viewDirty.values()) saveView(account);
    });
}

MailBackend::~MailBackend() {
    cancelGoogleOperations(false);
    // A fatal GUI exit may destroy a still-connected backend. Detach callbacks
    // before member destruction: QLocalSocket::~QLocalSocket can emit signals
    // after the pending-request maps and buffers have already been destroyed.
    for (auto timer : {&m_clockTimer, &m_connectTimer, &m_pollTimer, &m_searchTimer, &m_draftTimer, &m_viewTimer, &m_notificationTimer, &m_actionTimer, &m_calendarMailActivityTimer, &m_calendarPrefetchTimer, &m_navigationSaveTimer, &m_remoteSearchTimer}) {
        timer->stop(); QObject::disconnect(timer, nullptr, this, nullptr);
    }
    QObject::disconnect(&m_socket, nullptr, this, nullptr);
    m_socket.abort();
}

void MailBackend::start() {
    if (m_connected || m_socket.state() == QLocalSocket::ConnectingState) return;
    clearError(); cancelShutdown(); m_shutdownEmitted = false; m_buffer.clear(); m_startupClock.start();
    m_launchAttempts = 0; m_launchDelay = 1000; m_nextLaunch = 0; m_launchedPids.clear();
    m_connectTimer.start(); attemptConnection();
}

QString MailBackend::profilePath() const { return QDir::toNativeSeparators(m_profile); }
void MailBackend::revealProfile() { QDesktopServices::openUrl(QUrl::fromLocalFile(m_profile)); }

void MailBackend::attemptConnection() {
    if (m_startupClock.elapsed() >= 30000) {
        m_connectTimer.stop(); m_socket.abort(); fail("Cannot start the local engine. Check that fm-engine is installed beside the app, then reconnect."); return;
    }
    if (m_socket.state() != QLocalSocket::UnconnectedState) return;
    // An engine that refused this profile will refuse it again; say why now.
    if (const auto refusal = readStartupRefusal(m_profile, m_launchedPids); !refusal.isEmpty()) {
        m_connectTimer.stop(); fail(refusal); return;
    }
    QString trustError;
    const auto info = readPrivateDiscovery(m_profile, trustError);
    if (!trustError.isEmpty()) { m_connectTimer.stop(); fail(trustError); return; }
    if (!info.isEmpty()) {
        m_token = info["token"].toString();
        m_enginePid = info["pid"].toInteger();
        auto endpoint = info["endpoint"].toString();
        if (info["protocol"].toInt() != 1 || endpoint.isEmpty() || m_token.isEmpty()) {
            m_connectTimer.stop(); fail("The local engine discovery is incompatible. Install matching GUI and engine binaries."); return;
        }
        {
#ifdef Q_OS_WIN
            if (endpoint.startsWith("\\\\.\\pipe\\")) endpoint = endpoint.mid(9);
#endif
            m_socket.connectToServer(endpoint);
        }
    }
    if (m_launchAttempts < 10 && m_startupClock.elapsed() >= m_nextLaunch) {
        // Detached children keep their independent CLI-owned lifetime. Startup
        // retries are bounded; discovery can disappear before the old lock closes.
        ++m_launchAttempts;
        m_nextLaunch = m_startupClock.elapsed() + m_launchDelay;
        m_launchDelay = qMin(m_launchDelay * 2, 4000);
        QProcess process;
        process.setProgram(m_engine); process.setArguments({"--profile", m_profile});
        if (property("demoProfileRequested").toBool()) process.setArguments({"--profile", m_profile, "--demo-profile"});
        if (m_liveProfileRequested) process.setArguments({"--profile", m_profile, "--live-profile"});
        if (m_launchAttempts == 1 && property("verificationFixture").toString() == "snapshot")
            process.setArguments({"--profile", m_profile, "--verification-fixture", "snapshot"});
        process.setStandardOutputFile(QProcess::nullDevice());
        process.setStandardErrorFile(QProcess::nullDevice());
        qint64 pid = 0;
        if (!process.startDetached(&pid)) {
            m_connectTimer.stop(); fail("Could not launch fm-engine. Use --engine to select its executable.");
        }
        else {
            m_launchedPids.append(pid);
            if (qEnvironmentVariableIsSet("FM_GUI_TEST_TRACE"))
                qInfo() << "GUI engine launch attempt" << m_launchAttempts;
        }
    }
}

void MailBackend::request(const QString &method, const QString &account, const QJsonObject &params, Callback callback, bool retainOnNavigation) {
    if (blockArchivedRequest(method, account)) {
        callback({{"code", "account_removed"}, {"message", "This account is a read-only archive. Restore it explicitly before making changes."}}, false); return;
    }
    const bool automaticRead = method == "status" || method == "message_source" || method == "assist_progress" || (m_automaticReadScope && (method == "accounts" || method == "preferences_get"
        || method == "categories" || method == "conversation" || method == "draft_get" || method == "capture_history"
        || method == "google_sync_status" || method == "google_mail_mutation_status" || method == "conversations" || method == "google_sent" || method == "google_sent_message"));
    // Synchronous cancellation/error callbacks can issue user work. The caller's
    // polling origin applies only to this read, never to a reentrant request.
    QScopedValueRollback<bool> origin(m_automaticReadScope, false);
    if (method == "conversations" || method == "conversation" || method == "drafts" || method == "sender_summary") pruneSupersededQueries();
    if (m_socket.state() != QLocalSocket::ConnectedState || m_pending.size() >= 32) {
        emit debugEvent("WARN", "IPC", "Request not admitted", {{"method", method}, {"code", m_pending.size() >= 32 ? "queue_full" : "disconnected"}});
        callback({}, false); return;
    }
    QString id = QString::number(++m_sequence);
    QJsonObject envelope{{"version", 1}, {"id", id}, {"method", method}, {"params", params}};
    if (!account.isEmpty()) envelope["account"] = account;
    const auto wire = QJsonDocument(envelope).toJson(QJsonDocument::Compact) + '\n';
    if (wire.size() > maxFrame) { fail("This request is too large for the current local interface."); callback({}, false); return; }
    const bool searchText = (method == "conversations" || method == "drafts") && !params.contains("cursor") && params["offset"].toInt() == 0
        && !params["query"].toString().trimmed().isEmpty();
    m_pending.insert(id, {std::move(callback), 0, method, account, m_generation, m_listGeneration, m_quietCalendarRead ? -1 : m_calendarGeneration, m_senderCardGeneration, automaticRead, 0, searchText, retainOnNavigation});
    m_outgoing.enqueue({id, wire});
    sendNext();
}
void MailBackend::pruneSupersededQueries() {
    QQueue<QPair<QString, QByteArray>> retained;
    QList<Callback> cancelled;
    for (auto &next : m_outgoing) {
        auto pending = m_pending.find(next.first);
        if (pending == m_pending.end()) continue;
        if (pending->retainOnNavigation) { retained.enqueue(std::move(next)); continue; }
        if (((pending->method == "conversations" || pending->method == "drafts")
            && (pending->account != m_account || pending->listGeneration != m_listGeneration))
            || ((pending->method == "conversation" || pending->method == "message_source" || pending->method == "draft_get") && (pending->account != m_account || pending->generation != m_generation))
            || (pending->method == "sender_summary" && (pending->account != m_account || pending->senderGeneration != m_senderCardGeneration))) {
            // These requests have not reached the engine. Their callbacks also
            // check generation/account, so cancellation cannot replace current
            // rows or mark a superseded reader load reviewed. All durable
            // writes retain their ordering barriers.
            cancelled.append(std::move(pending->callback));
            m_pending.erase(pending);
        } else retained.enqueue(std::move(next));
    }
    m_outgoing.swap(retained);
    for (auto &callback : cancelled) callback({}, false);
}
void MailBackend::sendNext() {
    if (!m_inFlight.isEmpty() || m_socket.state() != QLocalSocket::ConnectedState) return;
    pruneSupersededQueries();
    while (!m_outgoing.isEmpty()) {
        qsizetype selected = 0;
        // Only an explicit query can pass an unsent prefix of automatic reads.
        // A manual read or write is an ordering barrier. After four bypasses a
        // polling read runs first, so sustained search cannot starve refresh.
        for (qsizetype index = 0; index < m_outgoing.size(); ++index) {
            const auto pending = m_pending.constFind(m_outgoing[index].first);
            if (pending == m_pending.cend()) break;
            if (pending->automaticRead && pending->queryBypasses < 4) continue;
            if (!pending->automaticRead && (pending->method == "conversations"
                || pending->method == "conversation" || pending->method == "view_get"
                || pending->method == "drafts" || pending->method == "sender_summary")) selected = index;
            break;
        }
        for (qsizetype index = 0; index < selected; ++index) ++m_pending[m_outgoing[index].first].queryBypasses;
        auto next = m_outgoing.takeAt(selected);
        auto pending = m_pending.find(next.first);
        if (pending == m_pending.end()) continue;
        m_inFlight = next.first;
        pending->started = QDateTime::currentMSecsSinceEpoch();
        if (!pending->automaticRead && !quietDiagnosticRead(pending->method))
            emit debugEvent("DEBUG", "IPC", "Request dispatched", {{"method", pending->method}, {"request_ref", next.first}, {"queued", m_outgoing.size()}});
        m_socket.write(next.second);
        return;
    }
}

void MailBackend::receive() {
    m_buffer += m_socket.readAll();
    if (m_buffer.size() > maxFrame && !m_buffer.contains('\n')) {
        fail("The local engine sent an oversized response."); m_socket.abort(); return;
    }
    while (m_buffer.contains('\n')) {
        const auto end = m_buffer.indexOf('\n');
        if (end > maxFrame) { fail("The local engine sent an oversized response."); m_socket.abort(); return; }
        const auto line = m_buffer.left(end); m_buffer.remove(0, end + 1);
        QJsonParseError error;
        const auto response = QJsonDocument::fromJson(line, &error).object();
        if (error.error != QJsonParseError::NoError || response["version"].toInt() != 1) {
            fail("The local engine returned an incompatible response."); m_socket.abort(); return;
        }
        const QString id = response["id"].toString();
        if (!m_pending.contains(id)) continue;
        if (id != m_inFlight) { fail("The local engine returned a response out of order."); m_socket.abort(); return; }
        auto pending = m_pending.take(id); m_inFlight.clear();
        const bool ok = response["ok"].toBool();
        const auto debugError = response["error"].toObject();
        QVariantMap debugFields{{"method", pending.method}, {"request_ref", id}, {"elapsed_ms", QDateTime::currentMSecsSinceEpoch() - pending.started}};
        if (!ok) {
            debugFields["code"] = debugError["code"].toString();
            const auto retry = debugError["details"].toObject().value("retry_after_secs");
            if (retry.isDouble()) debugFields["retry_after_secs"] = retry.toInteger();
        }
        if (!ok || (!pending.automaticRead && !quietDiagnosticRead(pending.method)) || debugFields["elapsed_ms"].toLongLong() >= 1000)
            emit debugEvent(ok ? "DEBUG" : "ERROR", "IPC", ok ? "Request completed" : debugError["message"].toString("Request failed"), debugFields);
        bool currentRead = true;
        if (pending.method == "conversations" || pending.method == "drafts") currentRead = pending.account == m_account && pending.listGeneration == m_listGeneration;
        else if (pending.method == "conversation" || pending.method == "draft_get") currentRead = pending.account == m_account && pending.generation == m_generation;
        else if (pending.method == "categories" || pending.method == "capture_history" || pending.method == "view_get") currentRead = pending.account == m_account;
        else if (pending.method == "calendar_combined") currentRead = pending.calendarGeneration == m_calendarGeneration;
        else if (pending.method == "sender_summary") currentRead = pending.account == m_account && pending.senderGeneration == m_senderCardGeneration;
        const auto errorCode = response["error"].toObject()["code"].toString();
        const bool retryCursor = pending.method == "conversations" && errorCode == "stale_cursor";
        const bool searchRefusal = pending.searchText && errorCode == "invalid_request";
        // saveView merges and retries a view another interface saved first.
        const bool viewMerge = pending.method == "view_save" && errorCode == "stale_revision";
        if (!ok && qEnvironmentVariableIsSet("FM_GUI_TEST_TRACE")) qInfo() << "GUI request failed:" << pending.method << errorCode;
        if (!ok && currentRead && !retryCursor && !searchRefusal && !viewMerge) fail(response["error"].toObject()["message"].toString("The operation failed."));
        pending.callback(ok ? response["result"].toObject() : response["error"].toObject(), ok);
        sendNext();
    }
}

void MailBackend::fail(const QString &message) { m_error = message; emit errorChanged(); }
void MailBackend::clearError() { m_error.clear(); emit errorChanged(); }
QVariantMap MailBackend::currentAccount() const {
    return lifecycleAccount(m_account);
}

void MailBackend::loadAccounts() {
    request("accounts", {}, {{"include_removed", true}}, [this](auto result, bool ok) {
        if (!ok) return;
        if (!m_initialAccountsLoaded) {
            for (const auto &entry : result["accounts"].toArray()) {
                const auto row = entry.toObject(), google = row["google"].toObject();
                if (row["lifecycle"].toObject()["state"] != "removed" && google["connection_state"] == "connected"
                    && google["sync"].toObject()["state"].isString() && google["sync"].toObject()["state"] != "not_started")
                    m_startupGoogleAccounts.insert(row["id"].toString());
            }
            m_initialAccountsLoaded = true; emit startupReadyChanged();
        }
        const auto all = result["accounts"].toArray().toVariantList();
        const bool changed = all != m_allAccounts;
        bool lifecycleChanged = false;
        for (const auto &entry : all) {
            const auto row = entry.toMap(), previous = lifecycleAccount(row.value("id").toString());
            if (!previous.isEmpty() && row.value("lifecycle") != previous.value("lifecycle")) lifecycleChanged = true;
        }
        m_allAccounts = all;
        QVariantList accounts;
        for (const auto &entry : all)
            if (entry.toMap().value("lifecycle").toMap().value("state").toString() != "removed") accounts.append(entry);
        if (accounts != m_accounts) {
            if (!publishAccounts(accounts)) return;
        }
        if (changed) { emit accountsChanged(); emit currentAccountChanged(); reconcileAccountLifecycle(); }
        if (lifecycleChanged) { loadCalendarSources(); loadCalendar(); }
        resumeStartupGoogleSync();
        if (m_account.isEmpty() && !m_accounts.isEmpty()) selectAccount(m_accounts.first().toMap()["id"].toString());
    });
}

bool MailBackend::publishAccounts(QVariantList accounts) {
    m_accounts = std::move(accounts);
    if (m_accountModel.replace(QStringLiteral("account-inventory"), m_accounts)) return true;
    m_accounts.clear();
    m_accountModel.replace(QStringLiteral("account-inventory"), {});
    emit accountsChanged(); emit currentAccountChanged();
    fail("The local engine returned missing or duplicate account identities.");
    return false;
}

// Inbox sections: review, held and one per Done month. Each keeps its own
// collapsed state in the account's saved view.
static bool validSection(const QString &section) {
    if (section == u"review" || section == u"held" || section == u"done:undated") return true;
    return section.startsWith(u"done:") && section.size() == 12
        && QDate::fromString(section.mid(5) + u"-01", Qt::ISODate).isValid();
}
void MailBackend::selectAccount(const QString &id) {
    ++m_accountSelectionRequests;
    m_lastAccountSelectionRequest = id;
    m_lastAccountSelectionDirty = m_draftDirty;
    m_lastAccountSelectionBusy = draftBusy();
    if (id == m_account) return;
    if (m_draftTransferDestination["account"].toString() != id) m_draftTransferDestination = {};
    if (m_unsavedAccountBuffer) { m_afterAccountDisposition = [this, id] { selectAccount(id); }; accountNavigationAllowed(); return; }
    if (id.isEmpty() || lifecycleAccount(id).isEmpty() || (accountRemoved(id) && m_archiveSelection != id)) return;
    cancelPendingGoogleSend("Send cancelled by mailbox navigation. Press Send again after reviewing the draft.");
    if (m_draftDirty || draftBusy()) { m_afterDraftSave = [this, id] { selectAccount(id); }; saveDraft(); return; }
    clearSenderCard();
    m_queryActivity.start();
    if (!m_account.isEmpty()) saveView(m_account);
    m_afterConversationMutation = {}; m_captureNextOffset = -1;
    m_collapsedGroups.clear();
    ++m_generation; const int generation = m_generation;
    m_accountViewRestorationGeneration = generation;
    ++m_draftSaveGeneration;
    if (!accountRemoved(id)) m_archiveSelection.clear();
    if (!m_attachmentQueues.value(attachmentImportKey()).isEmpty()) m_attachmentQueuePaused.insert(attachmentImportKey());
    dropEmptyNewDraft();
    m_account = id; m_conversation.clear(); m_draft.clear(); m_activeDraft.clear(); m_recipientInputs.clear(); m_selected.clear(); m_draftStatus.clear(); m_pinnedRow.clear(); m_pinnedIndex = -1;
    m_query.clear(); m_category.clear(); m_filters.clear(); m_querySummary.clear(); m_captureStatus.clear(); m_folder = "inbox"; m_categories.clear(); m_captures.clear(); m_nextOffset = -1; m_nextCursor.clear(); m_loading = false;
    m_searchTerms.clear(); m_searchError.clear(); m_searchPeople.clear(); m_searchPeopleQuery.clear(); ++m_searchPeopleGeneration;
    emit searchTermsChanged(); emit searchPeopleChanged(); emit recentSearchesChanged();
    // Restore only observed mail projections. The fresh view/source requests
    // below still own current state and every read-on-open mutation.
    // Publish the destination once. Clearing first destroys every delegate
    // before immediately replacing it with the already validated cached view.
    if (!restoreMailView()) publishConversations({});
    emit accountChanged(); emit currentAccountChanged(); emit accountLifecycleChanged(); emit categoriesChanged(); emit queryChanged(); emit conversationChanged(); emit draftChanged(); emit conversationsChanged(); emit capturesChanged(); emit mailDeliveryChanged();
    request("view_get", id, {}, [this, id, generation](auto result, bool ok) {
        if (m_accountViewRestorationGeneration == generation) m_accountViewRestorationGeneration = -1;
        if (ok) {
            auto latest = result["view"].toObject().toVariantMap();
            const auto pending = m_viewPending.value(id);
            for (auto it = pending.cbegin(); it != pending.cend(); ++it) latest[it.key()] = it.value();
            m_views[id] = latest; m_viewRevisions[id] = result["revision"].toInteger();
        }
        if (generation != m_generation) { resumeAccountRemovalAfterView(id, generation, false); return; }
        const auto previousSelected = m_selected;
        auto view = m_views.value(id).toMap();
        if (m_draftTransferDestination["account"].toString() == id) {
            remember("selectedDraft", m_draftTransferDestination["draft_id"].toString());
            remember("selected", QString{}); remember("folder", QStringLiteral("drafts"));
            remember("replyOpen", true); remember("readerY", 0);
            view = m_views.value(id).toMap(); m_draftTransferDestination = {};
        }
        m_query = view["query"].toString(); m_selected = view["selected"].toString();
        m_folder = view.value("folder", "inbox").toString();
        m_category = view.value("category").toString();
        m_filters = view.value("filters").toMap();
        QStringList sections;
        for (const auto &section : view.value("collapsedSections").toStringList())
            if (validSection(section) && !sections.contains(section)) sections.append(section);
        m_collapsedGroups = sections;
        convertLegacyFilters();
        emit recentSearchesChanged();
        if (m_selected != previousSelected) {
            m_conversation.clear(); restoreConversation(id, m_selected); emit conversationChanged();
        }
        emit queryChanged(); emit presentationChanged();
        loadConversations();
        const auto restoredDraft = view.value("selectedDraft").toString();
        if (!restoredDraft.isEmpty()) openDraft(restoredDraft, true);
        else if (!m_selected.isEmpty()) loadConversation(true);
        resumeAccountRemovalAfterView(id, generation, ok);
        // Combined calendar scope is independent of the selected mailbox. Its
        // initial preference load, calendar changes and polling own refreshes.
        // Hydrate the restored reader before account metadata and captures.
        QScopedValueRollback<bool> origin(m_automaticReadScope, true);
        loadCategories(); loadCaptures();
    });
}

void MailBackend::setQuery(const QString &value) {
    if (value == m_query) return;
    m_queryActivity.start();
    m_query = value; m_pinnedRow.clear(); m_pinnedIndex = -1; ++m_listGeneration; remember("query", value); emit queryChanged(); m_searchTimer.start();
}
void MailBackend::setFolder(const QString &value) {
    if (!accountNavigationAllowed()) return;
    if (value != "inbox" && value != "sent" && value != "all" && value != "drafts" && value != "trash" && value != "spam") return;
    if (m_folder == value) return;
    m_queryActivity.start();
    // Day chips are a Sent control; their dates must not filter other views unseen.
    if (m_folder == u"sent" && (m_filters.remove("after") | m_filters.remove("before"))) remember("filters", m_filters);
    m_folder = value; m_pinnedRow.clear(); m_pinnedIndex = -1;
    if (value == "drafts" || value == "spam" || value == "trash") { m_filters.clear(); m_category.clear(); remember("filters", QVariantMap{}); remember("category", ""); }
    // Spam is read live each visit, so it is not the view a restart reopens.
    if (value != "spam") remember("folder", value);
    if (!m_binCounts.isEmpty()) { m_binCounts.clear(); emit binCountsChanged(); }
    emit queryChanged(); loadConversations();
    if (value == "trash") refreshBinCounts();
}
void MailBackend::setCategory(const QString &value) {
    if (m_category == value) return;
    m_queryActivity.start();
    m_category = value; m_pinnedRow.clear(); m_pinnedIndex = -1; remember("category", value); emit queryChanged(); loadConversations();
}
void MailBackend::setCollapsedGroups(const QStringList &groups) {
    QStringList next;
    for (const auto &group : groups) if (validSection(group) && !next.contains(group) && next.size() < 258) next.append(group);
    if (next == m_collapsedGroups) return;
    m_queryActivity.start();
    m_collapsedGroups = next; m_pinnedRow.clear(); m_pinnedIndex = -1;
    remember("collapsedSections", next);
    ++m_listGeneration; emit queryChanged();
    if (m_folder == "inbox") loadConversations();
}
// Search 7 shows what filters the list in the search box. Typed filters no
// control shows any more become search text; the rest are cleared.
void MailBackend::convertLegacyFilters() {
    QStringList parts;
    const auto exact = m_filters.take("exact_sender").toString(), sender = m_filters.take("sender").toString();
    if (!exact.isEmpty()) parts << u"from:" + exact;
    if (!sender.isEmpty() && sender.compare(exact, Qt::CaseInsensitive) != 0) parts << u"from:" + sender;
    const auto status = m_filters.take("status").toString();
    if (status == u"review" || status == u"done") parts << u"is:" + status;
    else if (status == u"held") parts << QStringLiteral("is:delayed");
    const auto attachment = m_filters.take("has_attachment");
    if (attachment.isValid() && attachment.toBool()) parts << QStringLiteral("has:attachment");
    const bool dated = m_folder != u"sent" && (m_filters.remove("after") | m_filters.remove("before"));
    if (parts.isEmpty() && !attachment.isValid() && status.isEmpty() && exact.isEmpty() && sender.isEmpty() && !dated) return;
    if (!parts.isEmpty()) { m_query = (m_query.trimmed() + u' ' + parts.join(u' ')).trimmed(); remember("query", m_query); }
    remember("filters", m_filters);
}
void MailBackend::commitSearch(const QString &text) {
    const auto search = text.trimmed();
    if (search.isEmpty() || search.size() > 512 || search.contains(u'\n') || search.contains(u'\r')) return;
    auto recent = recentSearches();
    recent.removeAll(search); recent.prepend(search);
    while (recent.size() > 6) recent.removeLast();
    remember("recentSearches", recent); emit recentSearchesChanged(); emit presentationChanged();
}
void MailBackend::removeRecentSearch(const QString &text) {
    auto recent = recentSearches();
    if (!recent.removeAll(text)) return;
    remember("recentSearches", recent); emit recentSearchesChanged(); emit presentationChanged();
}
void MailBackend::requestSearchPeople(const QString &part, const QString &role) {
    if (!m_connected || m_account.isEmpty()) return;
    const auto account = m_account; const int generation = ++m_searchPeopleGeneration;
    const auto side = role == u"recipient" ? QStringLiteral("recipient") : QStringLiteral("sender");
    request("search_senders", account, {{"query", part.left(320)}, {"folder", m_folder}, {"role", side}, {"limit", 5}},
        [this, account, part, side, generation](auto result, bool ok) {
            if (account != m_account || generation != m_searchPeopleGeneration) return;
            m_searchPeople = ok ? result["people"].toArray().toVariantList() : QVariantList{};
            m_searchPeopleQuery = side + u':' + part; emit searchPeopleChanged();
        });
}
// A list or draft response reports how Core read the search text. An
// unreadable text keeps the rows shown and explains itself in the dropdown.
// Responses for older text are dropped by generation, so `text` names the
// current query; the dropdown shows a reading only for the text it was of.
void MailBackend::readSearchResult(const QJsonObject &result, bool ok) {
    auto terms = ok ? result["search_terms"].toObject().toVariantMap() : QVariantMap{};
    if (!ok || !terms.isEmpty()) terms["text"] = m_query;
    const auto error = ok ? QString{} : result["message"].toString();
    if (terms == m_searchTerms && error == m_searchError) return;
    m_searchTerms = terms; m_searchError = error; emit searchTermsChanged();
}
void MailBackend::loadCategories() {
    if (!m_connected || m_account.isEmpty()) return;
    const auto account = m_account;
    request("categories", account, {}, [this, account](auto result, bool ok) {
        if (!ok || account != m_account) return;
        const auto categories = result["categories"].toArray().toVariantList();
        if (categories != m_categories) { m_categories = categories; emit categoriesChanged(); }
        if (auto cached = m_mailViewCache.object(account); cached && cached->scope == mailViewScope()) cacheMailView();
    });
}

QString MailBackend::mailMonthZone() const {
    const auto id = QString::fromUtf8(QTimeZone::systemTimeZoneId());
    return id.contains(u'/') ? id : QStringLiteral("UTC");
}
bool MailBackend::publishConversations(QVariantList rows) {
    // Inbox rows name their section; Done rows split by the month of their
    // latest message, in the same zone Core counts them in.
    if (m_folder == u"inbox") {
        const QTimeZone zone(mailMonthZone().toUtf8());
        for (auto &value : rows) {
            auto row = value.toMap();
            auto section = row.value("display_group").toString();
            if (section.isEmpty()) section = row.value("group").toString();
            if (section == u"done") {
                const auto at = QDateTime::fromString(row.value("latest_at").toString(), Qt::ISODate);
                section = at.isValid() ? u"done:" + at.toTimeZone(zone).toString(u"yyyy-MM") : QStringLiteral("done:undated");
            }
            if (row.value("section").toString() != section) { row["section"] = section; value = row; }
        }
    }
    // The protocol-facing list remains authoritative. Only the native view uses
    // a stable model, whose changes must observe the same final row projection.
    const auto scope = QJsonDocument(QJsonObject{{"account", m_account}, {"folder", m_folder}, {"query", m_query},
        {"category", m_category}, {"filters", QJsonObject::fromVariantMap(m_filters)},
        {"collapsed_groups", QJsonArray::fromStringList(m_collapsedGroups)}}).toJson(QJsonDocument::Compact);
    // These fields already fit one accepted IPC frame. Keep the complete,
    // deterministic serialization, with bounded room for its wrapper keys.
    const auto reject = [this](const QString &error) {
        m_conversations.clear();
        m_conversationModel.replace({}, {});
        emit conversationsChanged();
        fail(error);
        return false;
    };
    if (scope.size() > 2 * maxFrame) return reject("The mail view context is too large to display.");
    m_conversations = std::move(rows);
    if (!m_conversationModel.replace(QString::fromUtf8(scope), conversations())) {
        return reject("The local engine returned missing or duplicate mail row identities.");
    }
    return true;
}

QVariantMap MailBackend::mailViewScope() const {
    return {{"folder", m_folder}, {"query", m_query}, {"category", m_category}, {"filters", m_filters},
        {"collapsed_groups", m_collapsedGroups}};
}

void MailBackend::cacheMailView() {
    if (m_account.isEmpty() || m_folder == "drafts" || m_folder == "spam" || accountArchived()) return;
    // Two small byte-bounded caches, independent of total mailbox size. Keep
    // pagination metadata with the exact list; a stale cursor is revalidated.
    const auto bytes = QJsonDocument(QJsonObject{{"rows", QJsonArray::fromVariantList(m_conversations)},
        {"summary", QJsonObject::fromVariantMap(m_querySummary)}, {"categories", QJsonArray::fromVariantList(m_categories)}}).toJson(QJsonDocument::Compact).size();
    if (bytes > 2 * maxFrame) { m_mailViewCache.remove(m_account); return; }
    auto entry = new MailViewSnapshot{mailViewScope(), m_querySummary, m_conversations, m_categories,
        lifecycleAccount(m_account).value("lifecycle").toMap().value("revision").toLongLong(), m_nextOffset, m_nextCursor};
    m_mailViewCache.insert(m_account, entry, int(bytes + 512));
}

bool MailBackend::restoreMailView() {
    auto cached = m_mailViewCache.object(m_account);
    const auto view = m_views.value(m_account).toMap();
    if (!cached || accountArchived() || !view.value("selectedDraft").toString().isEmpty()
        || cached->lifecycle != lifecycleAccount(m_account).value("lifecycle").toMap().value("revision").toLongLong()) return false;
    const QVariantMap wanted{{"folder", view.value("folder", "inbox")}, {"query", view.value("query", "")},
        {"category", view.value("category", "")}, {"filters", view.value("filters", QVariantMap{})},
        {"collapsed_groups", cached->scope.value("collapsed_groups")}};
    if (cached->scope != wanted) return false;
    m_folder = wanted.value("folder").toString(); m_query = wanted.value("query").toString();
    m_category = wanted.value("category").toString(); m_filters = wanted.value("filters").toMap();
    m_collapsedGroups = wanted.value("collapsed_groups").toStringList();
    m_selected = view.value("selected").toString();
    m_querySummary = cached->summary; m_categories = cached->categories;
    m_nextOffset = cached->nextOffset; m_nextCursor = cached->nextCursor;
    if (!publishConversations(cached->rows)) return false;
    restoreConversation(m_account, m_selected);
    return true;
}

void MailBackend::cacheConversation(const QString &account, const QString &id, const QVariantMap &data, qint64 lifecycle) {
    if (account.isEmpty() || id.isEmpty() || data.isEmpty() || accountRemoved(account)
        || lifecycle != lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong()) return;
    const auto key = qMakePair(account, id);
    const auto prior = m_readerCache.object(key);
    if (prior && prior->lifecycle == lifecycle && prior->data.value("revision").toLongLong() > data.value("revision").toLongLong()) return;
    const auto bytes = QJsonDocument::fromVariant(data).toJson(QJsonDocument::Compact).size();
    if (bytes > maxFrame) return;
    m_readerCache.insert(key, new ReaderSnapshot{data, lifecycle}, int(bytes + 128));
}

bool MailBackend::restoreConversation(const QString &account, const QString &id) {
    const auto cached = m_readerCache.object(qMakePair(account, id));
    if (!cached || accountRemoved(account)
        || cached->lifecycle != lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong()) return false;
    m_conversation = cached->data;
    return true;
}

void MailBackend::loadConversations(bool append, bool preserveLoaded) {
    if (!m_connected || m_account.isEmpty()) return;
    if (m_automaticReadScope && !m_listError.isEmpty()) return;
    if (!append && !preserveLoaded) m_browsingOlder = false;
    if (!append && m_folder == "sent") loadOutgoing();
    if (m_folder == "drafts") { loadDraftRows(append, preserveLoaded); return; }
    // Spam is read live from Gmail: store refreshes during sync leave it alone.
    if (m_folder == "spam") { if (append || !preserveLoaded) loadSpamRows(append); return; }
    const int generation = ++m_listGeneration;
    const QString account = m_account;
    const int offset = append ? m_nextOffset : 0;
    if (offset < 0) return;
    m_listError.clear(); m_loading = true; emit loadingChanged();
    QJsonObject params{{"live_paging", true}, {"query", m_query}, {"folder", m_folder}, {"limit", 100}, {"offset", offset}};
    params["grouped"] = m_folder != "sent";
    for (auto it = m_filters.cbegin(); it != m_filters.cend(); ++it) params[it.key()] = QJsonValue::fromVariant(it.value());
    if (m_folder == "inbox") {
        // Done is shown by month in this computer's zone, like row times.
        params["month_zone"] = mailMonthZone();
        QJsonArray groups, months;
        for (const auto &section : m_collapsedGroups) {
            if (section.startsWith(u"done:")) months.append(section.mid(5)); else groups.append(section);
        }
        if (!groups.isEmpty()) params["collapsed_groups"] = groups;
        if (!months.isEmpty()) params["collapsed_months"] = months;
    }
    if (append && !m_nextCursor.isEmpty()) { params.remove("offset"); params["cursor"] = m_nextCursor; }
    if (!m_category.isEmpty()) params["category"] = m_category;
    // Keep visible rows until a refresh has recovered the same page count. Each
    // IPC response remains bounded; a view-state write must not collapse paging.
    const int desired = append ? int(m_conversations.size()) + 100 : preserveLoaded ? std::max({100, int(m_conversations.size()), m_desiredConversationCount}) : 100;
    m_desiredConversationCount = desired;
    loadConversationPage(account, generation, params, append ? m_conversations : QVariantList{}, desired);
}
void MailBackend::loadConversationPage(const QString &account, int generation, QJsonObject params, QVariantList collected, int desiredCount, int cursorRetries) {
    request("conversations", account, params,
        [this, generation, account, params, collected = std::move(collected), desiredCount, cursorRetries](auto result, bool ok) mutable {
            if (generation != m_listGeneration || account != m_account) return;
            if (ok) m_mailScopeCount = result["folder_conversation_count"].toInt();
            if (!ok && result["code"].toString() == "stale_cursor") {
                if (cursorRetries >= 3) { m_listError = "Mail kept changing during pagination. Retry loading more."; m_loading = false; emit loadingChanged(); fail(m_listError); return; }
                params.remove("cursor"); params["offset"] = 0;
                loadConversationPage(account, generation, params, {}, desiredCount, cursorRetries + 1); return;
            }
            if (!ok && result["code"].toString() == "invalid_request" && !params.contains("cursor") && params["offset"].toInt() == 0
                && !params["query"].toString().trimmed().isEmpty()) {
                readSearchResult(result, false); m_loading = false; emit loadingChanged(); return;
            }
            if (ok && collected.isEmpty()) readSearchResult(result, true);
            if (ok) {
                const auto previousSummary = m_querySummary;
                m_querySummary = {{"filtered_conversation_count", result["filtered_conversation_count"].toVariant()},
                                  {"filtered_group_counts", result["filtered_group_counts"].toVariant()},
                                  {"done_months", result["done_months"].toVariant()},
                                  {"expanded_conversation_count", result["expanded_conversation_count"].toVariant()},
                                  {"account_counts", result["account_counts"].toVariant()}, {"coverage", result["coverage"].toVariant()}};
                const auto list = result["conversations"].toArray().toVariantList();
                QSet<QString> ids;
                for (const auto &row : collected) ids.insert(row.toMap().value("id").toString());
                for (const auto &row : list) if (!ids.contains(row.toMap().value("id").toString())) {
                    ids.insert(row.toMap().value("id").toString()); collected.append(row);
                }
                const auto next = result["next_offset"].isDouble() ? result["next_offset"].toInt() : -1;
                if (next >= 0 && (list.isEmpty() || (!params["cursor"].toString().isEmpty() && result["next_cursor"] == params["cursor"]))) {
                    m_listError = "Older mail did not advance. Retry loading.";
                    m_loading = false; emit loadingChanged(); return;
                }
                if (!list.isEmpty() && next > params["offset"].toInt() && collected.size() < desiredCount) {
                    params["offset"] = next;
                    const auto cursor = result["next_cursor"].toString();
                    if (!cursor.isEmpty()) { params.remove("offset"); params["cursor"] = cursor; }
                    loadConversationPage(account, generation, params, std::move(collected), desiredCount, cursorRetries);
                    return;
                }
                const auto previous = m_conversations;
                const auto previousNext = m_nextOffset;
                // A conversation read or filed while open moves to its new section
                // at once. It is kept in place only when this view no longer
                // returns it (such as an unread filter), so the open row stays.
                const bool returned = std::any_of(collected.cbegin(), collected.cend(),
                    [this](const QVariant &row) { return row.toMap().value("id").toString() == m_selected; });
                if (m_pinnedIndex >= 0 && !m_pinnedRow.isEmpty() && !returned) {
                    auto pinned = m_pinnedRow;
                    pinned["display_group"] = m_pinnedRow.value("display_group", m_pinnedRow["group"]); pinned["pinned"] = true;
                    collected.insert(qMin(m_pinnedIndex, int(collected.size())), pinned);
                }
                if (!publishConversations(std::move(collected))) {
                    m_querySummary = previousSummary; m_loading = false; emit loadingChanged(); return;
                }
                m_nextOffset = next; m_nextCursor = result["next_cursor"].toString();
                cacheMailView();
                if (m_selected.isEmpty() && m_activeDraft.isEmpty() && !m_resolvingDraft && !m_conversations.isEmpty()) selectConversation(m_conversations.first().toMap()["id"].toString());
                if (previous != m_conversations || previousNext != m_nextOffset || previousSummary != m_querySummary) emit conversationsChanged();
            }
            if (!ok) m_listError = result["message"].toString("Could not load older mail. Retry.");
            m_loading = false; emit loadingChanged();
        });
}
void MailBackend::loadMore() {
    if (m_loading) return;
    if (m_nextOffset >= 0) loadConversations(true);
    if (m_folder == "sent" && m_outgoingNext.value(m_account, -1) >= 0) loadOutgoing(true);
}
void MailBackend::prefetchMore() {
    if (!m_listError.isEmpty()) return;
    QScopedValueRollback<bool> origin(m_automaticReadScope, true);
    loadMore();
}

void MailBackend::selectConversation(const QString &id) {
    selectConversationInternal(id, false);
}
void MailBackend::prefetchConversation(const QString &id) {
    if (!m_connected || m_shutdown || id.isEmpty() || id == m_selected || id.startsWith("outgoing:")) return;
    const auto row = std::find_if(m_conversations.cbegin(), m_conversations.cend(), [&id](const auto &value) { return value.toMap().value("id") == id; });
    if (row == m_conversations.cend()) return;
    const auto lifecycle = lifecycleAccount(m_account).value("lifecycle").toMap().value("revision").toLongLong();
    if (const auto cached = m_readerCache.object(qMakePair(m_account, id)); cached && cached->lifecycle == lifecycle
        && cached->data.value("reader_prepared").toBool()
        && cached->data.value("revision").toLongLong() >= row->toMap().value("revision").toLongLong()) return;
    if (id == m_readerPrefetchTarget) return;
    m_readerPrefetchTarget = id;
    m_readerPrefetchFuture.cancel();
    m_readerPrefetchTimer.start();
}

QVariantMap MailBackend::retainReaderPresentation(QVariantMap data) const {
    if (data.value("id") != m_conversation.value("id")) return data;
    const auto prior = m_conversation.value("messages").toList();
    auto members = data.value("messages").toList();
    for (auto &value : members) {
        auto row = value.toMap();
        for (const auto &oldValue : prior) {
            const auto old = oldValue.toMap();
            if (row.value("id") != old.value("id") || row.value("source_version") != old.value("source_version") || row.value("text") != old.value("text")) continue;
            for (const auto key : {"reader_source", "reader_error", "reader_sections", "reader_plain_sections", "metadata"})
                if (old.contains(key)) row[key] = old.value(key);
            break;
        }
        value = row;
    }
    data["messages"] = members;
    if (m_conversation.contains("assistance_results")) data["assistance_results"] = m_conversation.value("assistance_results");
    if (m_conversation.contains("assistance_failures")) data["assistance_failures"] = m_conversation.value("assistance_failures");
    if (m_conversation.contains("assistance_settings")) data["assistance_settings"] = m_conversation.value("assistance_settings");
    data["reader_prepared"] = m_conversation.value("reader_prepared");
    return data;
}

void MailBackend::prepareReader(const QString &account, const QString &id, QVariantMap data, int generation,
                               qint64 lifecycle, bool prefetch, std::function<void(QVariantMap)> ready) {
    const auto serial = m_readerLoadSerial;
    const auto current = [this, account, id, generation, lifecycle, prefetch, serial] {
        return !m_shutdown && generation == m_generation && account == m_account
            && (prefetch || serial == m_readerLoadSerial)
            && (prefetch ? id == m_readerPrefetchTarget : id == m_selected)
            && lifecycle == lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong();
    };
    if (!current()) return;
    auto snapshot = std::make_shared<QVariantMap>(std::move(data));
    if (const auto cached = m_readerCache.object(qMakePair(account, id)); cached && cached->lifecycle == lifecycle) {
        auto members = snapshot->value("messages").toList();
        const auto oldMembers = cached->data.value("messages").toList();
        if (!members.isEmpty() && !oldMembers.isEmpty()) {
            auto latest = members.last().toMap(); const auto old = oldMembers.last().toMap();
            if (latest.value("id") == old.value("id") && latest.value("source_version") == old.value("source_version") && latest.value("text") == old.value("text")
                && (!latest.value("source_available").toBool() || !old.value("reader_source").toMap().isEmpty())) {
                for (const auto key : {"reader_sections", "reader_plain_sections"}) if (old.contains(key)) latest[key] = old.value(key);
                members.last() = latest; (*snapshot)["messages"] = members;
            }
        }
    }
    const auto finish = [this, snapshot, current, prefetch, ready = std::move(ready)] {
        if (!current()) return;
        // Only the current foreground projection may wait behind the one active
        // formatting worker. Superseded queued snapshots are discarded.
        if (prefetch && m_readerPool.activeThreadCount()) { m_readerPrefetchTarget.clear(); return; }
        if (!prefetch) { m_readerFuture.cancel(); m_readerPool.clear(); }
        QPromise<QVariantMap> promise;
        auto future = promise.future();
        (prefetch ? m_readerPrefetchFuture : m_readerFuture) = future;
        auto watcher = new QFutureWatcher<QVariantMap>(this);
        connect(watcher, &QFutureWatcher<QVariantMap>::finished, this, [watcher, current, ready] {
            watcher->deleteLater();
            if (!watcher->isCanceled() && current()) ready(watcher->result());
        });
        watcher->setFuture(future);
        m_readerPool.start([promise = std::move(promise), snapshot]() mutable {
            promise.start();
            if (!promise.isCanceled()) {
                auto members = snapshot->value("messages").toList();
                if (!members.isEmpty()) {
                    auto latest = members.last().toMap();
                    const auto source = latest.value("reader_source").toMap();
                    const auto plain = latest.value("text").toString();
                    const auto html = source.value("html_truncated").toBool() ? QString{} : source.value("body_html").toString();
                    if (!latest.contains("reader_sections")) {
                        latest["reader_sections"] = ReplyDocument::receivedBody(html, plain.isEmpty() ? "(No text)" : plain);
                        if (!promise.isCanceled()) latest["reader_plain_sections"] = html.isEmpty() ? latest.value("reader_sections")
                            : QVariant(ReplyDocument::receivedBody({}, plain.isEmpty() ? "(No text)" : plain));
                    }
                    members.last() = latest;
                }
                (*snapshot)["messages"] = members;
                (*snapshot)["reader_prepared"] = true;
                if (!promise.isCanceled()) promise.addResult(*snapshot);
            }
            promise.finish();
        }, prefetch ? 0 : 1);
    };
    const auto summaries = [this, snapshot, account, id, current, prefetch, finish] {
        if (!current()) return;
        requestReader(account, {{"conversation_id", id}}, [this, snapshot, account, current, prefetch, finish](auto result, bool ok) {
            if (!current()) return;
            (*snapshot)["assistance_results"] = ok ? result.value("items").toArray().toVariantList() : QVariantList{};
            (*snapshot)["assistance_failures"] = ok ? result.value("failures").toArray().toVariantList() : QVariantList{};
            requestReader(account, {}, [snapshot, current, finish](auto settings, bool available) {
                if (!current()) return;
                if (available) (*snapshot)["assistance_settings"] = settings.toVariantMap();
                finish();
            }, "assist_settings_get", prefetch);
        }, "assist_results", prefetch);
    };
    const auto members = snapshot->value("messages").toList();
    const auto latest = members.isEmpty() ? QVariantMap{} : members.last().toMap();
    const auto message = latest.value("id").toString(), version = latest.value("source_version").toString();
    if (!latest.value("source_available").toBool() || version.isEmpty()) { summaries(); return; }
    const auto key = account + '\n' + message + '\n' + version;
    const auto sourceReady = [this, snapshot, account, message, version, key, lifecycle, current, summaries](auto result, bool ok) {
        if (!current()) return;
        auto members = snapshot->value("messages").toList();
        auto latest = members.last().toMap();
        if (ok && result.value("account").toString() == account && result.value("id").toString() == message
            && result.value("source_version").toString() == version) {
            const auto prepared = withoutRaw(result);
            const auto source = prepared.toVariantMap();
            const auto bytes = QJsonDocument(prepared).toJson(QJsonDocument::Compact).size();
            m_sourceCache.insert(key, new ReaderSnapshot{source, lifecycle}, int(bytes + 128));
            latest["reader_source"] = source; latest["metadata"] = source.value("metadata");
        } else latest["reader_error"] = result.value("message").toString("Message source is unavailable.");
        members.last() = latest; (*snapshot)["messages"] = members;
        summaries();
    };
    if (const auto cached = m_sourceCache.object(key); cached && cached->lifecycle == lifecycle)
        sourceReady(QJsonObject::fromVariantMap(cached->data), true);
    else requestReader(account, {{"id", message}, {"source_version", version}}, sourceReady, "message_source", prefetch);
}

void MailBackend::selectConversationInternal(const QString &id, bool review, QVariantMap clickedRow, int clickedIndex) {
    if (m_unsavedAccountBuffer) { m_afterAccountDisposition = [this, id, review, clickedRow, clickedIndex] { selectConversationInternal(id, review, clickedRow, clickedIndex); }; accountNavigationAllowed(); return; }
    if (id != m_selected || !m_activeDraft.isEmpty()) cancelPendingGoogleSend("Send cancelled by draft navigation.");
    // Capture the visible navigation intent before waiting for durable work.
    // A preceding read can move this row outside the filtered/page result while
    // its write is pending. Looking it up only after that wait loses the pin.
    // This is presentation state only: the eventual load still supplies the
    // exact fresh source snapshot used by deliberate read-on-open.
    if (review && clickedRow.isEmpty()) {
        for (qsizetype i = 0; i < m_conversations.size(); ++i) if (m_conversations[i].toMap()["id"].toString() == id) {
            clickedRow = m_conversations[i].toMap(); clickedIndex = int(i); break;
        }
    }
    if (m_draftDirty || draftBusy()) {
        m_afterDraftSave = [this, id, review, clickedRow, clickedIndex] { selectConversationInternal(id, review, clickedRow, clickedIndex); }; saveDraft(); return;
    }
    if (id != m_selected || (review && m_pinnedIndex < 0)) {
        m_pinnedRow.clear(); m_pinnedIndex = -1;
        if (review) { m_pinnedRow = std::move(clickedRow); m_pinnedIndex = clickedIndex; }
    }
    if (id == m_selected && m_activeDraft.isEmpty()) { if (review) loadConversation(false, true); return; }
    if (!m_attachmentQueues.value(attachmentImportKey()).isEmpty()) m_attachmentQueuePaused.insert(attachmentImportKey());
    ++m_generation; m_readerFuture.cancel(); m_readerPrefetchFuture.cancel();
    m_readerPrefetchTimer.stop(); m_readerPrefetchTarget.clear();
    dropEmptyNewDraft();
    m_selected = id; m_activeDraft.clear(); m_recipientInputs.clear(); m_conversation.clear(); m_draft.clear(); m_draftStatus.clear();
    restoreConversation(m_account, id);
    remember("selectedDraft", "");
    // A server-only thread is not reopened after a restart.
    remember("selected", id.startsWith("remote:") ? QString{} : id); remember("readerY", 0); emit conversationChanged(); emit draftChanged(); emit mailDeliveryChanged();
    loadConversation(true, review);
}

void MailBackend::loadConversation(bool loadDraft, bool reviewOnLoad) {
    if (m_selected.isEmpty()) return;
    if (m_selected.startsWith("remote:")) { loadRemoteThread(); return; }
    if (m_selected.startsWith("outgoing:")) {
        if (loadDraft) {
            const auto key=m_account+'\n'+m_selected.mid(QStringLiteral("outgoing:").size());
            if (!m_outgoingBodies.value(key).value("content_error").toString().isEmpty()) m_outgoingBodies.remove(key);
        }
        for (const auto &value : outgoingMessages()) if (value.toMap().value("id") == m_selected) loadOutgoingMessage(value.toMap());
        emit conversationChanged(); return;
    }
    const auto id = m_selected, account = m_account; const int generation = m_generation;
    // A poll or another click on the same row must not cancel preparation or
    // lose the deliberate-open receipt while its source is still being read.
    if (m_readerLoadingGeneration == generation) { m_readerReviewPending |= reviewOnLoad; return; }
    m_readerLoadingGeneration = generation; m_readerReviewPending = reviewOnLoad;
    const auto serial = ++m_readerLoadSerial;
    const auto lifecycle = lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong();
    requestReader(account, {{"id", id}}, [this, id, account, generation, lifecycle, serial](auto result, bool ok) {
        if (serial != m_readerLoadSerial || id != m_selected || account != m_account || generation != m_generation) return;
        if (!ok) { m_readerLoadingGeneration = -1; m_readerReviewPending = false; return; }
        prepareReader(account, id, result.toVariantMap(), generation, lifecycle, false,
            [this, id, account, lifecycle, generation](auto data) {
        const bool reviewOnLoad = m_readerReviewPending;
        m_readerLoadingGeneration = -1; m_readerReviewPending = false;
        if (m_conversation.value("revision").toLongLong() > data.value("revision").toLongLong()) return;
        cacheConversation(account, id, data, lifecycle);
        if (m_conversation != data) { m_conversation = data; emit conversationChanged(); }
        loadOutgoingThread();
        for (const auto &value : outgoingMessages()) if (value.toMap().value("conversation_id") == id) loadOutgoingMessage(value.toMap());
        if (reviewOnLoad && accountCapability(account, "mail_write")) {
            const auto members = data.value("messages").toList();
            if (std::any_of(members.cbegin(), members.cend(), [](auto m) { return m.toMap().value("unread").toBool(); })) {
                if (m_mutatingConversation) m_afterConversationMutation = [this, id, account, generation] {
                    if (id == m_selected && account == m_account && generation == m_generation) loadConversation(false, true);
                };
                else mutateConversation("flag", {{"read", true}}, false);
            }
        }
        emit ready();
        const auto selected = std::find_if(m_conversations.cbegin(), m_conversations.cend(), [&id](const auto &row) { return row.toMap().value("id") == id; });
        if (selected != m_conversations.cend() && std::next(selected) != m_conversations.cend()) prefetchConversation(std::next(selected)->toMap().value("id").toString());
        });
    });
    if (loadDraft && !m_activeDraft.isEmpty()) { loadActiveDraft(); return; }
    if (loadDraft) request("draft_get", account, {{"conversation_id", id}}, [this, id, account, generation](auto result, bool ok) {
        if (!ok || id != m_selected || account != m_account || generation != m_generation
            || result["account"].toString() != account || result["conversation_id"].toString() != id) return;
        const bool sendChanged = mergeGoogleSendStatus(result["google_send"].toObject(), account, result["draft_id"].toString());
        const bool metadataChanged = mergeGoogleDraftStatus(result["google"].toObject(), account, result["draft_id"].toString()) || sendChanged;
        if (m_draftDirty || draftBusy() || olderDraftProjection(result)) { if (metadataChanged) emit draftChanged(); return; }
        const auto draft = mergedDraftProjection(result);
        const QString status = draft.value("discarded").toBool() ? "Draft discarded"
            : draft.value("revision").toInt() > 0 ? "Saved on this machine" : "A reply stays here until you save it";
        if (draft != m_draft || m_draftConflict || status != m_draftStatus) {
            m_draft = draft; restoreRecipientInputs(m_draft); m_draftConflict = false; m_draftStatus = status;
            emit draftChanged();
        }
        else if (metadataChanged) emit draftChanged();
    });
}

QJsonObject MailBackend::mutation() const { return {{"request_id", uuid()}}; }
void MailBackend::flag(const QString &name, bool value) {
    mutateConversation("flag", {{name, value}});
}
void MailBackend::mutateConversation(const QString &method, QJsonObject params, bool undoable) {
    // A pending send and a thread read live from the server have nothing to change here.
    if (m_selected.startsWith("outgoing:") || m_selected.startsWith("remote:")) return;
    if (m_conversation.isEmpty() || m_mutatingConversation) return;
    if (!accountCapability(m_account, "mail_write")) { fail("Reconnect Google with Mail access to change this conversation."); return; }
    params["request_id"] = uuid(); params["id"] = m_selected;
    params["message_ids"] = QJsonArray::fromVariantList(m_conversation["message_ids"].toList());
    params["expected_revision"] = m_conversation["revision"].toLongLong();
    const auto id = m_selected, account = m_account;
    m_mutatingConversation = true; emit mutationChanged();
    const auto undoEpoch = m_mailUndoEpoch;
    request(method, account, params, [this, id, account, method, params, undoable, undoEpoch](auto result, bool ok) {
        if (ok && undoable && undoEpoch == m_mailUndoEpoch) {
            const auto token=mailUndoToken(account,result);
            if (!token.isEmpty()) offerMailUndo(mailActionLabel(method,params), {token});
        }
        m_mutatingConversation = false; emit mutationChanged();
        if (!ok) cancelShutdown();
        if (ok && !m_syntheticProfile && result["delivery"].isObject()) {
            const auto key = qMakePair(account, id);
            m_mailDelivery[key] = result["delivery"].toObject().toVariantMap();
            m_mailDeliveryOrder.removeAll(key); m_mailDeliveryOrder.append(key);
            while (m_mailDeliveryOrder.size() > 128) m_mailDelivery.remove(m_mailDeliveryOrder.takeFirst());
            if (id == m_selected && account == m_account) emit mailDeliveryChanged();
        }
        if (ok && id == m_selected && account == m_account) {
            if (method == "mail_filing") { m_pinnedRow.clear(); m_pinnedIndex = -1; }
            m_conversation = retainReaderPresentation(result.toVariantMap());
            cacheConversation(account, id, m_conversation, lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong());
            if (!m_pinnedRow.isEmpty()) {
                bool unread = false, starred = false;
                for (const auto &member : result["messages"].toArray()) { unread |= member.toObject()["unread"].toBool(); starred |= member.toObject()["starred"].toBool(); }
                m_pinnedRow["unread"] = unread; m_pinnedRow["starred"] = starred;
            }
            emit conversationChanged();
        }
        if (m_shutdown) finishShutdown();
        else {
            if (m_afterConversationMutation) { auto next = std::move(m_afterConversationMutation); m_afterConversationMutation = {}; next(); }
            if (!ok) { refresh(); return; }
            // The receipt already contains the exact committed reader snapshot.
            // A mail flag/category change does not change a draft, preferences,
            // captures or combined calendars. Keep the list/count projections
            // current without rehydrating those unrelated surfaces.
            {
                QScopedValueRollback<bool> origin(m_automaticReadScope, true);
                loadAccounts();
                if (method == "set_category") loadCategories();
            }
            m_listRefreshPending = false;
            loadConversations(false, true);
        }
    });
}
void MailBackend::setRead(bool read) { flag("read", read); }
void MailBackend::setStarred(bool starred) { flag("starred", starred); }
void MailBackend::setCategoryMembership(const QString &categoryId, bool present) {
    mutateConversation("set_category", {{"category_id", categoryId}, {"present", present}});
}
void MailBackend::fileConversation(const QString &action) {
    if (m_syntheticProfile || (action != "archive" && action != "trash" && action != "restore")) return;
    mutateConversation("mail_filing", {{"action", action}});
}

QVariantList MailBackend::snapshotMailRows(const QStringList &ids) const {
    QVariantList result;
    const auto displayed = conversations();
    if (ids.size() > 200 || m_account.isEmpty()) return result;
    for (const auto &id : ids) {
        auto found = std::find_if(displayed.cbegin(), displayed.cend(), [&id](const auto &row) { return row.toMap().value("id").toString() == id && !row.toMap().value("remote").toBool(); });
        QVariantMap row;
        if (id.startsWith("outgoing:")) {
            for (const auto &value : outgoingMessages()) if (value.toMap().value("id") == id) { row=outgoingMessage(value.toMap());row["revision"]=0; }
        } else if (id == m_selected && !m_conversation.isEmpty() && m_folder != "drafts") {
            row = m_conversation;
            const auto messages = row.value("messages").toList();
            row["unread"] = std::any_of(messages.begin(), messages.end(), [](const auto &m) { return m.toMap().value("unread").toBool(); });
            row["starred"] = std::any_of(messages.begin(), messages.end(), [](const auto &m) { return m.toMap().value("starred").toBool(); });
        } else if (found != displayed.cend()) row = found->toMap();
        if (row.isEmpty()) return {};
        row["selection_account"] = m_account;
        row["selection_lifecycle"] = lifecycleAccount(m_account).value("lifecycle").toMap().value("revision");
        result.append(row);
    }
    return result;
}

void MailBackend::applyMailSelection(const QVariantList &rows, const QString &action, const QString &categoryId) {
    if (m_mutatingConversation || rows.isEmpty() || rows.size() > 200 || !m_connected || m_shutdown) return;
    if (!accountCapability(m_account, "mail_write")) { fail("Reconnect Google with Mail access to change selected mail."); return; }
    const auto lifecycle = lifecycleAccount(m_account).value("lifecycle").toMap().value("revision").toLongLong();
    QList<QJsonObject> commands;
    QSet<QString> seen;
    for (const auto &value : rows) {
        const auto row = value.toMap();
        if (row.value("is_outgoing").toBool()) { fail("Wait for Sent sync before changing this message. Open its send status to check delivery."); return; }
        const auto id = row.value("id").toString();
        if (row.value("selection_account").toString() != m_account || row.value("selection_lifecycle").toLongLong() != lifecycle
            || id.isEmpty() || seen.contains(id) || !row.contains("revision")) {
            fail("The selection changed account or is no longer valid. Select the messages again."); return;
        }
        seen.insert(id);
        QJsonObject params{{"request_id", uuid()}, {"expected_revision", row.value("revision").toLongLong()}};
        QString method;
        if (action == "discard") {
            // Remote source previews and unresolved sends require their existing
            // explicit recovery controls; bulk discard must never bypass them.
            if (!row.value("is_draft").toBool() || !row.value("remote_source").toMap().isEmpty()
                || row.value("read_only").toBool() || m_draftDiscardUi.contains(m_account + '\n' + id) || m_queuedSends.contains(m_account + '\n' + id)
                || (id == m_activeDraft && (draftBusy() || attachmentImportPending()))) {
                fail("Open this draft to resolve its saved content or Google state before discarding it."); return;
            }
            method = "draft_discard"; params["draft_id"] = id;
            if (id == m_draft.value("draft_id").toString()) {
                if (row.value("revision") != m_draft.value("revision")) { fail("The selected draft changed. Select it again before discarding."); return; }
                params["buffer"] = discardEditorBuffer();
            }
        } else {
            if (row.value("is_draft").toBool() || row.value("message_ids").toList().isEmpty()) { fail("Select received or sent conversations for this action."); return; }
            params["id"] = id; params["message_ids"] = QJsonArray::fromVariantList(row.value("message_ids").toList());
            method = "flag";
            if (action == "read" || action == "unread") params["read"] = action == "read";
            else if (action == "delay" || action == "undelay") params["starred"] = action == "delay";
            else if (action == "done") { params["read"] = true; params["starred"] = false; }
            else if (action == "label_add" || action == "label_remove") {
                method = "set_category"; params["category_id"] = categoryId; params["present"] = action == "label_add";
            } else if (action == "trash" || action == "restore" || action == "archive") {
                if (m_syntheticProfile) return;
                method = "mail_filing"; params["action"] = action;
            } else return;
        }
        commands.append(QJsonObject{{"method", method}, {"params", params}, {"reopen", method == "draft_discard" && id == m_activeDraft && presentation().value("replyOpen").toBool()}});
    }
    m_bulkMailUndo.clear(); m_bulkMailSelection = rows; m_bulkMailUndoEpoch=m_mailUndoEpoch;
    m_bulkMailAction=commands.isEmpty() ? QString{} : mailActionLabel(commands.first()["method"].toString(),commands.first()["params"].toObject());
    m_bulkCommands = std::move(commands); m_bulkIndex = 0; m_bulkAccount = m_account; m_bulkLifecycle = lifecycle;
    m_bulkIssuedAt = m_actionClock.elapsed(); m_bulkDiscardUndo.clear(); m_bulkDiscardTargets.clear();
    for (const auto &command : m_bulkCommands) if (command["method"] == "draft_discard") m_bulkDiscardTargets.insert(m_bulkAccount + '\n' + command["params"].toObject()["draft_id"].toString());
    m_mutatingConversation = true; m_mailSelectionStatus = "Applying to " + QString::number(rows.size()) + " selected conversations…";
    emit mutationChanged(); runMailSelection();
}

void MailBackend::runMailSelection() {
    const bool valid = !m_shutdown && m_connected && m_account == m_bulkAccount && !accountRemoved(m_bulkAccount)
        && accountCapability(m_bulkAccount, "mail_write")
        && lifecycleAccount(m_bulkAccount).value("lifecycle").toMap().value("revision").toLongLong() == m_bulkLifecycle;
    if (!valid || m_bulkIndex >= m_bulkCommands.size()) {
        m_mailSelectionStatus = QString::number(m_bulkIndex) + " of " + QString::number(m_bulkCommands.size())
            + (valid ? (m_syntheticProfile ? " updated locally." : " saved locally. Google sync reports delivery separately.") : " updated. Remaining actions stopped after the account or session changed.");
        m_bulkCommands.clear(); m_mutatingConversation = false; emit mutationChanged(); finishBulkDiscardUndo();
        if (!m_bulkMailUndo.isEmpty() && m_bulkMailUndoEpoch == m_mailUndoEpoch) offerMailUndo(m_bulkMailAction + (m_bulkMailUndo.size()>1 ? " · "+QString::number(m_bulkMailUndo.size())+" conversations" : ""),m_bulkMailUndo,m_bulkMailSelection.size()>1 ? m_bulkMailSelection : QVariantList{});
        m_bulkMailUndo.clear();
        if (m_shutdown) finishShutdown();
        else if (m_connected) {
            if (m_afterConversationMutation) { auto next = std::move(m_afterConversationMutation); m_afterConversationMutation = {}; next(); }
            loadAccounts(); loadCategories(); loadConversations(false, true);
        }
        return;
    }
    const auto command = m_bulkCommands[m_bulkIndex];
    const auto params = command["params"].toObject();
    const auto account = m_bulkAccount;
    auto done = [this, account, params, command](const QJsonObject &result, bool ok) {
        if (!ok) {
            m_mailSelectionStatus = QString::number(m_bulkIndex) + " of " + QString::number(m_bulkCommands.size())
                + " updated. Stopped: " + result["message"].toString("selection changed or response was lost") + ". Refresh and inspect before selecting again.";
            m_bulkCommands.clear(); m_mutatingConversation = false; emit mutationChanged(); finishBulkDiscardUndo();
        if (!m_bulkMailUndo.isEmpty() && m_bulkMailUndoEpoch == m_mailUndoEpoch) offerMailUndo(m_bulkMailAction + (m_bulkMailUndo.size()>1 ? " · "+QString::number(m_bulkMailUndo.size())+" conversations" : ""),m_bulkMailUndo,m_bulkMailSelection.size()>1 ? m_bulkMailSelection : QVariantList{});
        m_bulkMailUndo.clear(); cancelShutdown();
            if (m_afterConversationMutation) { auto next = std::move(m_afterConversationMutation); m_afterConversationMutation = {}; next(); }
            if (m_connected) { loadAccounts(); loadCategories(); loadConversations(false, true); }
            return;
        }
        ++m_bulkIndex;
        const auto undoToken=mailUndoToken(account,result);
        if (!undoToken.isEmpty()) m_bulkMailUndo.append(undoToken);
        if (command["method"] == "draft_discard") {
            const auto token = discardUndoToken(account, result, command["reopen"].toBool());
            if (!token.isEmpty()) m_bulkDiscardUndo.append(token);
        }
        const bool currentLifecycle = lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong() == m_bulkLifecycle;
        const auto id = params["id"].toString();
        if (!id.isEmpty()) {
            cacheConversation(account, id, result.toVariantMap(), m_bulkLifecycle);
            const auto key = qMakePair(account, id);
            if (result["delivery"].isObject()) {
                m_mailDelivery[key] = result["delivery"].toObject().toVariantMap();
                m_mailDeliveryOrder.removeAll(key); m_mailDeliveryOrder.append(key);
                while (m_mailDeliveryOrder.size() > 128) m_mailDelivery.remove(m_mailDeliveryOrder.takeFirst());
                if (key == qMakePair(m_account, m_selected)) emit mailDeliveryChanged();
            }
            if (currentLifecycle && account == m_account && id == m_selected) {
                if (command["method"] == "mail_filing") { m_pinnedRow.clear(); m_pinnedIndex = -1; }
                m_conversation = result.toVariantMap();
                if (m_pinnedRow.value("id").toString() == id) {
                    m_pinnedRow["revision"] = result["revision"].toVariant();
                    bool unread = false, starred = false;
                    for (const auto &member : result["messages"].toArray()) { unread |= member.toObject()["unread"].toBool(); starred |= member.toObject()["starred"].toBool(); }
                    m_pinnedRow["unread"] = unread; m_pinnedRow["starred"] = starred;
                }
                emit conversationChanged();
            }
        }
        runMailSelection();
    };
    if (command["method"] == "draft_discard") startDraftDiscardRequest("draft_discard", account, params, m_bulkIssuedAt, command["reopen"].toBool(), std::move(done));
    else request(command["method"].toString(), account, params, std::move(done));
}

void MailBackend::editDraft(const QString &body) {
    if (draftLocked() || m_draft.isEmpty() || body == m_draft["body"].toString()) return;
    m_draft["body"] = body; m_draft["body_html"] = QVariant(); m_draftDirty = true; m_draftStatus = "Saving…"; emit draftChanged(); m_draftTimer.start();
}
void MailBackend::saveDraft() {
    m_draftTimer.stop();
    if (m_unsavedAccountBuffer) { m_afterDraftSave = {}; finishGoogleSendSave(false); cancelShutdown(); emit draftChanged(); return; }
    if (accountArchived()) {
        finishGoogleSendSave(false);
        if (m_draftDirty) { freezeRemovedAccountBuffer(); return; }
        if (m_afterDraftSave) { auto callback = std::move(m_afterDraftSave); m_afterDraftSave = {}; callback(); }
        if (m_shutdown) finishShutdown();
        return;
    }
    if (draftReadOnly()) {
        // Preview/open/close may share this continuation with local editors.
        // Imported source is never turned into an authored save implicitly.
        finishGoogleSendSave(false);
        if (m_afterDraftSave) { auto callback = std::move(m_afterDraftSave); m_afterDraftSave = {}; callback(); }
        if (m_shutdown) finishShutdown();
        return;
    }
    // The import advances this exact draft's revision. Keep newer edits local
    // until its receipt, without occupying the cached-query connection.
    if (m_attachmentImports.contains(attachmentImportKey())) {
        finishGoogleSendSave(false);
        if (m_shutdown && m_draftDirty) {
            cancelShutdown(); fail("Resolve the attachment import before closing with unsaved edits.");
        }
        return;
    }
    if (draftBusy()) return;
    if (draftDiscardPending()) { m_draftStatus = "Check the pending draft action before saving."; finishGoogleSendSave(false); cancelShutdown(); emit draftChanged(); return; }
    if (draftTransferPending()) { m_draftStatus = "Check the pending account change before editing this draft."; cancelShutdown(); emit draftChanged(); return; }
    if (!m_draftDirty) {
        finishGoogleSendSave(true);
        if (m_afterDraftSave) { auto callback = std::move(m_afterDraftSave); m_afterDraftSave = {}; callback(); }
        advanceDraftAttachments();
        if (m_shutdown) finishShutdown();
        return;
    }
    if (m_draftConflict) { finishGoogleSendSave(false); m_draftStatus = "Not saved — another interface saved a newer draft. Choose which version to keep."; cancelShutdown(); emit draftChanged(); return; }
    if (!m_connected) { finishGoogleSendSave(false); m_draftStatus = "Not saved — reconnect to keep this reply"; cancelShutdown(); emit draftChanged(); return; }
    auto params = mutation(); const auto selector = draftSelector();
    for (auto it = selector.begin(); it != selector.end(); ++it) params[it.key()] = it.value();
    const auto body = m_draft["body"].toString();
    const auto html = m_draft["body_html"], attachments = m_draft.value("attachment_ids", QVariantList{});
    const auto signature = m_draft["signature_text"].toString();
    params["body"] = body; params["expected_revision"] = m_draft["revision"].toLongLong();
    params["body_html"] = QJsonValue::fromVariant(html);
    params["attachment_ids"] = QJsonValue::fromVariant(attachments);
    params["signature_text"] = signature;
    const auto pendingInputs = recipientInputs();
    params["recipient_inputs"] = QJsonObject::fromVariantMap(pendingInputs);
    // Keep the loaded source binding even after a revision rebase. Core must
    // reject a save against another client's different frozen reply source.
    if (!m_draft["source_message_id"].toString().isEmpty())
        params["source_message_id"] = m_draft["source_message_id"].toString();
    QVariantMap headers;
    for (const auto &key : {"subject", "to_recipients", "cc_recipients", "bcc_recipients"}) {
        if (m_draft.contains(key)) { headers[key] = m_draft[key]; params[key] = QJsonValue::fromVariant(m_draft[key]); }
    }
    m_savingDraft = true;
    const auto account = m_account;
    const auto context = draftContext();
    const auto saveGeneration = ++m_draftSaveGeneration;
    request("draft_save", account, params, [this, account, context, saveGeneration, body, html, attachments, signature, headers, pendingInputs](auto result, bool ok) {
        if (saveGeneration != m_draftSaveGeneration || account != m_account || context != draftContext() || m_unsavedAccountBuffer) return;
        m_savingDraft = false;
        if (accountRemoved(account) || (!ok && result["code"] == "account_removed")) { freezeRemovedAccountBuffer(); loadAccounts(); return; }
        if (!ok) {
            finishGoogleSendSave(false);
            m_draftConflict = result["code"].toString() == "stale_revision";
            m_draftStatus = m_draftConflict ? "Not saved — another interface saved a newer draft. Your version is retained here." : "Not saved — your reply is still in the editor";
            cancelShutdown(); emit draftChanged(); return;
        }
        m_draftConflict = false;
        m_draft["revision"] = result["revision"].toVariant();
        m_draft["from_name"] = result["from_name"].toVariant();
        m_draft["identity_revision"] = result["identity_revision"].toVariant();
        if (m_draft["attachment_ids"] == attachments) m_draft["attachments"] = result["attachments"].toVariant();
        m_draftDirty = recipientInputs() != pendingInputs || m_draft["body"].toString() != body || m_draft["body_html"] != html || m_draft["attachment_ids"] != attachments || m_draft["signature_text"].toString() != signature;
        if (!m_draftDirty) m_draft["recipient_inputs"] = result["recipient_inputs"].toVariant();
        for (auto it = headers.cbegin(); it != headers.cend(); ++it) m_draftDirty = m_draftDirty || m_draft[it.key()] != it.value();
        for (const auto &key : {"draft_id", "mode", "source_message_id", "updated_at", "reply_all_available", "discarded"})
            if (result.contains(key)) m_draft[key] = result[key].toVariant();
        mergeGoogleDraftStatus(result["google"].toObject(), account, result["draft_id"].toString());
        mergeGoogleSendStatus(result["google_send"].toObject(), account, result["draft_id"].toString());
        m_draftStatus = m_draftDirty ? "Saving…" : "Saved on this machine"; emit draftChanged();
        finishGoogleSendSave(true);
        if (m_folder == "drafts") loadDraftRows(false);
        loadAccounts();
        saveDraft();
    });
}

void MailBackend::openAttachment(const QString &id) {
    attachmentFile(m_account, id, [this, id](auto result, bool ok) {
        if (!ok) { if (id.startsWith(u"remote-att:")) fail(result["message"].toString("This attachment could not be read from the server.")); return; }
        const auto path = result["path"].toString();
        if (path.isEmpty()) { fail("This attachment has no complete, verified local copy (" + result["state"].toString("unavailable") + ")."); return; }
        if (!QDir::isAbsolutePath(path) || !QDesktopServices::openUrl(QUrl::fromLocalFile(path))) fail("No application could open this retained attachment.");
        else showActionToast("Opening " + result["name"].toString() + "…", {}, {}, 3500);
    });
}
QStringList MailBackend::linksInText(const QString &text) const {
    static const QRegularExpression pattern(R"((?:https?://|mailto:)[^\s<>"']+)");
    auto matches = pattern.globalMatch(text);
    QStringList urls;
    while (matches.hasNext() && urls.size() < 20) {
        auto url = matches.next().captured();
        while (url.endsWith('.') || url.endsWith(',') || url.endsWith(';')) url.chop(1);
        if (!urls.contains(url)) urls.append(url);
    }
    return urls;
}
void MailBackend::openLink(const QString &value) {
    const QUrl url(value, QUrl::StrictMode);
    if (!url.isValid() || (url.scheme() != "https" && url.scheme() != "http" && url.scheme() != "mailto")) { fail("This link type is not supported."); return; }
    if (!QDesktopServices::openUrl(url)) fail("No application could open this link.");
}
QString MailBackend::calendarCoverageText(const QJsonObject &coverage) const {
    if (coverage["state"] == "display_zone_required") return QString("Choose a display time zone in Calendar settings");
    if (!m_syntheticProfile && coverage["selected_calendars"].toInt() == 0)
        return QString("No Google calendars connected");
    const QString suffix = coverage["state"] == "restricted" ? "some calendars are unavailable"
        : coverage["complete"].toBool() ? (m_syntheticProfile ? "cached fixture data" : "cached calendar data") : "incomplete cached data";
    return QString("%1 selected calendars · %2").arg(coverage["selected_calendars"].toInt()).arg(suffix);
}
void MailBackend::publishCalendarWindow(const QJsonObject &result, const QString &from, int days) {
    const QTimeZone zone(result["time_zone"].toString().toUtf8());
    m_events = calendarPresentation(result["events"].toArray(), QDate::fromString(from, Qt::ISODate), days, zone);
    presentCalendarWeek(result["events"].toArray(), zone);
    m_calendarReady = true; m_calendarCoverage = calendarCoverageText(result["coverage"].toObject());
}
QVariantList MailBackend::timelinePanels() const {
    QVariantList panels;
    const auto center = QDate::fromString(m_timelineDate, Qt::ISODate);
    for (int offset = -1; offset <= 1; ++offset) {
        const auto date = center.addDays(offset).toString(Qt::ISODate);
        panels.append(QVariantMap{{"date", date},
            {"events", offset == 0 ? todayEvents() : visibleCalendarEvents(m_timelineDayEvents.value(date).toList(), true)},
            {"ready", offset == 0 ? m_timelineReady : m_timelineDayEvents.contains(date)}});
    }
    return panels;
}
// Day, Week and Month size their header band by the busiest all-day day of
// the anchor's week, so switching views never moves the hour axis. Day view
// presents one day, so the week is laid out separately from the same source.
void MailBackend::presentCalendarWeek(const QJsonArray &events, const QTimeZone &zone) {
    QJsonArray allDay;
    for (const auto &event : events) if (event.toObject()["all_day"].toBool()) allDay.append(event);
    m_calendarWeekAllDay = calendarPresentation(allDay, QDate::fromString(calendarRangeStart("week", m_calendarDate), Qt::ISODate), 7, zone);
}
int MailBackend::calendarBandRows() const {
    int counts[7] = {};
    for (const auto &value : visibleCalendarEvents(m_calendarWeekAllDay)) {
        const auto row = value.toMap();
        const int first = row.value("day_index").toInt(), span = std::max(1, row.value("span_days").toInt());
        for (int day = std::max(0, first); day < std::min(7, first + span); ++day) ++counts[day];
    }
    return *std::max_element(std::begin(counts), std::end(counts));
}
MailBackend::CalendarWindow MailBackend::calendarWindow(const QString &from, int days) const {
    const QJsonObject params{{"from", from}, {"to", QDate::fromString(from, Qt::ISODate).addDays(days).toString(Qt::ISODate)},
        {"calendars", effectiveCalendarSelection()}};
    return {from, days, params, QString::fromUtf8(QJsonDocument(params).toJson(QJsonDocument::Compact)) + "/"
        + m_preferences.value("calendar_display_time_zone").toString()};
}
// Day, week and month views all read their month's six-week grid, so moving
// between them, or within the month, needs no engine read.
MailBackend::CalendarWindow MailBackend::calendarBaseWindow(const QString &date) const {
    if (m_calendarView == "agenda") return calendarWindow(calendarRangeStart("agenda", date), 42);
    return calendarWindow(calendarRangeStart("month", date), 42);
}
namespace {
// A window over the engine's occurrence budget cannot stand in for a narrower range.
bool calendarWindowLimited(const QJsonObject &result) {
    return result["limited"].toBool() || result["coverage"].toObject()["projection_limited"].toBool();
}
}
void MailBackend::setCalendarStatus(bool ready, const QString &coverage) {
    if (ready == m_calendarReady && coverage == m_calendarCoverage) return;
    m_calendarReady = ready; m_calendarCoverage = coverage; emit calendarStatusChanged();
}
void MailBackend::storeCalendarWindow(const QString &key, const QJsonObject &result) {
    m_calendarWindowOrder.removeAll(key); m_calendarWindowOrder.append(key);
    m_calendarWindows.insert(key, result);
    while (m_calendarWindowOrder.size() > 8) m_calendarWindows.remove(m_calendarWindowOrder.takeFirst());
}
bool MailBackend::publishCachedCalendar() {
    const auto exact = calendarWindow(calendarFrom(), calendarDays());
    // The exact range's own read is shown even when incomplete; a wider window only when whole.
    auto source = m_calendarWindows.constFind(exact.key);
    if (source == m_calendarWindows.cend() || (*source)["limited"].toBool()) {
        source = m_calendarWindows.constFind(calendarBaseWindow(m_calendarDate).key);
        if (source == m_calendarWindows.cend() || calendarWindowLimited(*source)) return false;
    }
    // The anchor's week sizes the header band, so it is part of what is shown.
    const auto display = QString::number(m_calendarCacheEpoch) + '#' + source.key() + '#' + calendarRangeStart("week", m_calendarDate) + '#' + exact.key;
    if (display != m_calendarDisplayKey) {
        const QTimeZone zone((*source)["time_zone"].toString().toUtf8());
        m_events = calendarPresentation((*source)["events"].toArray(), QDate::fromString(exact.from, Qt::ISODate), exact.days, zone);
        presentCalendarWeek((*source)["events"].toArray(), zone);
        m_calendarDisplayKey = display;
    }
    setCalendarStatus(m_connected, m_connected ? calendarCoverageText((*source)["coverage"].toObject())
                                               : QStringLiteral("Cached calendar is stale — reconnect to refresh"));
    return true;
}
void MailBackend::requestCalendarWindow(const CalendarWindow &window) {
    if (!m_connected || m_calendarWindowRequests.contains(window.key)) return;
    m_calendarWindowRequests.insert(window.key);
    const auto epoch = m_calendarCacheEpoch; const auto key = window.key;
    // Window reads never raise the global error; the callback reports displayed failures.
    QScopedValueRollback<bool> quiet(m_quietCalendarRead, true);
    request("calendar_combined", {}, window.params, [this, epoch, key](auto result, bool ok) {
        if (epoch != m_calendarCacheEpoch) return;
        m_calendarWindowRequests.remove(key);
        const auto code = result["code"].toString();
        if (!ok && code == "limit_exceeded") storeCalendarWindow(key, {{"limited", true}});
        else if (ok) storeCalendarWindow(key, result);
        const auto exact = calendarWindow(calendarFrom(), calendarDays());
        if (key != exact.key && key != calendarBaseWindow(m_calendarDate).key) return;
        const auto before = m_calendarDisplayKey;
        if (publishCachedCalendar()) { if (m_calendarDisplayKey != before) emit eventsChanged(); return; }
        // An over-budget month falls back to exactly the visible range.
        if ((ok || code == "limit_exceeded") && key != exact.key) { requestCalendarWindow(exact); return; }
        if (code == "permission_denied" || code == "not_found" || code == "scope_mismatch") {
            m_events.clear(); m_calendarWeekAllDay.clear(); m_calendarDisplayKey.clear(); emit eventsChanged();
        }
        setCalendarStatus(false, (m_events.isEmpty() ? "Calendar unavailable: " : "Cached calendar is stale: ") + result["message"].toString());
        // Open event details re-check readiness on the window signal.
        emit eventsChanged();
        fail(result["message"].toString("The operation failed."));
    });
}
void MailBackend::showCalendarRange() {
    ++m_calendarGeneration;
    m_weekStart = calendarFrom();
    if (!publishCachedCalendar()) {
        // Earlier rows belong to another range and cannot be drawn at this one's positions.
        m_events.clear(); m_calendarWeekAllDay.clear(); m_calendarDisplayKey.clear();
        const auto base = calendarBaseWindow(m_calendarDate);
        const auto cached = m_calendarWindows.constFind(base.key);
        setCalendarStatus(false, m_connected ? QStringLiteral("Loading calendars…") : QStringLiteral("Calendar is waiting for the local engine"));
        requestCalendarWindow(cached != m_calendarWindows.cend() && calendarWindowLimited(*cached) ? calendarWindow(calendarFrom(), calendarDays()) : base);
    }
    emit eventsChanged();
    m_calendarPrefetchTimer.start();
}
// Neighbouring months load while Calendar is open and the connection is idle,
// so the next move is already local and never queues ahead of user work.
void MailBackend::prefetchCalendarWindows() {
    if (!m_connected || m_shutdown || !m_calendarMailActivityActive || m_calendarView == "agenda") return;
    if (!m_pending.isEmpty()) { m_calendarPrefetchTimer.start(); return; }
    // Neighbours of an over-budget month would exceed it too.
    const auto current = m_calendarWindows.constFind(calendarBaseWindow(m_calendarDate).key);
    if (current != m_calendarWindows.cend() && calendarWindowLimited(*current)) return;
    const auto date = QDate::fromString(m_calendarDate, Qt::ISODate);
    for (int offset : {0, 1, -1}) {
        const auto window = calendarBaseWindow(date.addMonths(offset).toString(Qt::ISODate));
        if (!m_calendarWindows.contains(window.key)) requestCalendarWindow(window);
    }
}
void MailBackend::loadCalendar() {
    ++m_calendarCacheEpoch; ++m_calendarGeneration;
    m_calendarWindows.clear(); m_calendarWindowOrder.clear(); m_calendarWindowRequests.clear();
    m_weekStart = calendarFrom();
    const auto exact = calendarWindow(calendarFrom(), calendarDays());
    // Keep rows for this same range visible until the refreshed read replaces them.
    const bool current = !m_calendarDisplayKey.isEmpty() && m_calendarDisplayKey.endsWith('#' + exact.key);
    if (!current) { m_events.clear(); m_calendarWeekAllDay.clear(); emit eventsChanged(); }
    m_calendarDisplayKey = current ? QStringLiteral("stale#") + exact.key : QString{};
    const bool snapshot = current && m_calendarReady;
    if (!m_connected) {
        setCalendarStatus(false, snapshot ? QStringLiteral("Cached calendar is stale — reconnect to refresh") : QStringLiteral("Calendar is waiting for the local engine"));
    } else {
        setCalendarStatus(snapshot, snapshot ? QStringLiteral("Refreshing cached calendar…") : QStringLiteral("Loading calendars…"));
        requestCalendarWindow(calendarBaseWindow(m_calendarDate));
        m_calendarPrefetchTimer.start();
    }
    loadTimeline(true);
}
void MailBackend::loadTimeline(bool force) {
    const auto timeline = m_timelineDate;
    const auto selection = effectiveCalendarSelection();
    const auto timelineDay = QDate::fromString(timeline, Qt::ISODate);
    const QJsonObject params{{"from", timelineDay.addDays(-1).toString(Qt::ISODate)}, {"to", timelineDay.addDays(2).toString(Qt::ISODate)}, {"calendars", selection}};
    const auto displayZone = m_preferences.value("calendar_display_time_zone").toString();
    const auto context = QString::fromUtf8(QJsonDocument(QJsonArray{selection}).toJson(QJsonDocument::Compact)) + "/" + displayZone;
    if (context != m_timelineCacheContext) { m_timelineCacheContext = context; m_timelineDayEvents.clear(); force = true; }
    const auto scope = QString::fromUtf8(QJsonDocument(params).toJson(QJsonDocument::Compact)) + "/" + displayZone;
    bool changed = false;
    if (scope != m_timelineScope) {
        m_timelineScope = scope; force = true; changed = true;
        m_todayEvents = m_timelineDayEvents.value(timeline).toList();
        m_timelineReady = m_timelineDayEvents.contains(timeline);
        for (const auto &date : m_timelineDayEvents.keys())
            if (qAbs(timelineDay.daysTo(QDate::fromString(date, Qt::ISODate))) > 1) m_timelineDayEvents.remove(date);
    }
    // The same timeline window with unchanged data needs no read.
    if (!force) { if (changed) emit timelineChanged(); return; }
    const bool snapshot = m_timelineReady;
    const int generation = ++m_timelineGeneration;
    if (!m_connected) {
        m_timelineCoverage = snapshot ? "Cached timeline is stale — reconnect to refresh" : "Timeline is waiting for the local engine";
        m_timelineReady = false; emit timelineChanged(); return;
    }
    m_timelineCoverage = snapshot ? "Refreshing cached timeline…" : "Loading timeline…";
    emit timelineChanged();
    request("calendar_combined", {}, params, [this, timeline, generation, snapshot](auto result, bool ok) {
        if (generation != m_timelineGeneration || timeline != m_timelineDate) return;
        if (!ok) {
            m_timelineReady = false;
            const auto code = result["code"].toString();
            if (!snapshot || code == "permission_denied" || code == "not_found" || code == "scope_mismatch") {
                m_todayEvents.clear(); m_timelineDayEvents.clear();
            }
            m_timelineCoverage = (m_todayEvents.isEmpty() ? "Timeline unavailable: " : "Cached timeline is stale: ") + result["message"].toString();
            emit timelineChanged(); return;
        }
        m_timelineDayEvents.clear();
        const auto day = QDate::fromString(timeline, Qt::ISODate);
        const auto zone = QTimeZone(result["time_zone"].toString().toUtf8());
        for (int offset = -1; offset <= 1; ++offset)
            m_timelineDayEvents[day.addDays(offset).toString(Qt::ISODate)] = calendarPresentation(result["events"].toArray(), day.addDays(offset), 1, zone);
        m_todayEvents = m_timelineDayEvents.value(timeline).toList();
        m_timelineReady = true; m_timelineCoverage = calendarCoverageText(result["coverage"].toObject()); emit timelineChanged();
    });
}
void MailBackend::moveWeek(int delta) {
    m_calendarView = "week"; m_calendarDate = QDate::fromString(m_calendarDate, Qt::ISODate).addDays(delta * 7).toString(Qt::ISODate);
    showCalendarRange();
}
void MailBackend::showFixtureWeek() {
    m_calendarView = "week"; m_calendarDate = m_fixtureToday; showCalendarRange();
}
void MailBackend::refresh() { m_listRefreshPending = false; refreshState(true, true); }
void MailBackend::refreshState(bool refreshList, bool refreshCalendar, bool automatic) {
    {
        QScopedValueRollback<bool> origin(m_automaticReadScope, automatic);
        // Polling reloads preferences only after another client saved them.
        const auto savedRevision = m_diagnostics.value("preferences_revision");
        const bool preferencesCurrent = automatic && m_initialPreferencesLoaded && savedRevision.isValid()
            && savedRevision.toLongLong() == m_preferencesRevision;
        loadAccounts(); if (!preferencesCurrent) loadPreferences(); loadCategories();
    }
    if (refreshList) { QScopedValueRollback<bool> origin(m_automaticReadScope, automatic); loadConversations(false, true); }
    {
        QScopedValueRollback<bool> origin(m_automaticReadScope, automatic);
        const bool refreshDraft = !m_syntheticProfile || (!m_draftDirty && !draftBusy());
        loadConversation(refreshDraft);
        if (m_selected.isEmpty() && !m_activeDraft.isEmpty() && refreshDraft) loadActiveDraft();
    }
    if (refreshCalendar) loadCalendar();
    {
        QScopedValueRollback<bool> origin(m_automaticReadScope, automatic);
        loadCaptures();
    }
}

void MailBackend::remember(const QString &key, const QVariant &value) {
    if (m_account.isEmpty()) return;
    auto view = m_views.value(m_account).toMap();
    if (view.value(key) == value) return;
    view[key] = value; m_views[m_account] = view;
    if (accountArchived() || m_unsavedAccountBuffer) return;
    m_viewPending[m_account][key] = value;
    m_viewDirty.insert(m_account); m_viewTimer.start();
}
void MailBackend::saveView(const QString &account) {
    if (accountRemoved(account)) { m_viewDirty.remove(account); m_viewPending.remove(account); return; }
    if (!m_connected || account.isEmpty() || m_viewSaving.contains(account) || !m_viewDirty.contains(account)) return;
    m_viewDirty.remove(account); m_viewSaving.insert(account);
    const auto submitted = m_viewPending.value(account);
    auto params = mutation(); params["view"] = QJsonObject::fromVariantMap(m_views.value(account).toMap()); params["expected_revision"] = m_viewRevisions.value(account);
    request("view_save", account, params, [this, account, submitted](auto result, bool ok) {
        m_viewSaving.remove(account);
        if (accountRemoved(account)) { m_viewDirty.remove(account); m_viewPending.remove(account); if (m_shutdown) finishShutdown(); return; }
        if (ok) {
            m_viewRevisions[account] = result["revision"].toInteger();
            auto &pending = m_viewPending[account];
            for (auto it = submitted.cbegin(); it != submitted.cend(); ++it)
                if (pending.value(it.key()) == it.value()) pending.remove(it.key());
            if (!pending.isEmpty()) m_viewDirty.insert(account);
        } else if (result["code"].toString() == "stale_revision") {
            // Another interface saved this view first: merge the unsaved keys
            // over its latest version and save again without an error.
            m_viewDirty.insert(account); m_viewSaving.insert(account);
            request("view_get", account, {}, [this, account](auto latest, bool loaded) {
                m_viewSaving.remove(account);
                if (!loaded) {
                    cancelShutdown();
                    fail("The current mail view was not saved. Your changes remain here. Reconnect if needed, then close again to retry.");
                    if (m_shutdown) finishShutdown();
                    return;
                }
                auto view = latest["view"].toObject().toVariantMap();
                const auto pending = m_viewPending.value(account);
                for (auto it = pending.cbegin(); it != pending.cend(); ++it) view[it.key()] = it.value();
                m_viewRevisions[account] = latest["revision"].toInteger(); m_views[account] = view;
                // Closing keeps the positions just submitted instead of re-applying them.
                if (account == m_account && !m_shutdown) emit presentationChanged();
                saveView(account);
                if (m_shutdown) finishShutdown();
            });
        } else {
            m_viewDirty.insert(account); cancelShutdown();
            fail("The current mail view was not saved. Your changes remain here. Reconnect if needed, then close again to retry.");
        }
        if (ok && m_viewDirty.contains(account)) saveView(account);
        if (m_shutdown) finishShutdown();
    });
}
void MailBackend::cancelShutdown() {
    if (!m_shutdown) return;
    m_shutdown = false;
    if (m_connected) m_pollTimer.start();
    if (m_resumeSearch) { m_resumeSearch = false; m_searchTimer.start(); }
}
void MailBackend::shutdown() {
    if (m_shutdown) return;
    if (!m_attachmentQueues.isEmpty()) { fail("Finish or skip the waiting attachments before closing. File selections remain in their original drafts."); return; }
    if (m_unsavedAccountBuffer) { m_afterAccountDisposition = [this] { shutdown(); }; accountNavigationAllowed(); return; }
    for (const auto &pending : m_draftDiscardUi) if (!pending.active) { fail("Check the pending draft action before closing. Its original request is kept here."); return; }
    cancelPendingGoogleSend("Send cancelled before closing.");
    cancelQueuedSends("Pending sends cancelled before closing. Drafts are saved.");
    cancelQueuedCalendarDeletions();
    m_shutdown = true; m_resumeSearch = m_searchTimer.isActive(); m_searchTimer.stop(); m_viewTimer.stop();
    // The poll timer still enforces request timeouts while closing; its status
    // query is suppressed by m_shutdown. A failed save keeps the app usable.
    if (!m_connected && (m_draftDirty || !m_preferencesPending.isEmpty() || !m_viewDirty.isEmpty())) {
        cancelShutdown(); fail("Unsaved changes remain here. Reconnect before closing to save them."); return;
    }
    for (const auto &account : m_viewDirty.values()) saveView(account);
    cancelGoogleOperations();
    savePreferences(); saveDraft(); finishShutdown();
}
void MailBackend::finishShutdown() {
    if (!m_shutdown || m_shutdownEmitted || m_draftDirty || draftBusy() || !m_draftDiscardUi.isEmpty() || m_preferencesSaving || !m_preferencesPending.isEmpty() || (m_connected && !m_navigationPending.isEmpty()) || m_calendarWrites || m_accountWrites || m_mutatingConversation || !m_viewSaving.isEmpty() || !m_viewDirty.isEmpty()) return;
    m_shutdownEmitted = true;
    m_pollTimer.stop();
    m_socket.disconnectFromServer(); emit shutdownReady();
}

QString MailBackend::assistanceRequest(const QString &method, const QVariantMap &params, const QString &account) {
    const QString ticket = QUuid::createUuid().toString(QUuid::WithoutBraces);
    if (!method.startsWith("assist_")) return {};
    // Queue dispatch so even immediate refusal reaches QML after it stores the ticket.
    QTimer::singleShot(0, this, [this, method, params, account, ticket] {
        request(method, account, QJsonObject::fromVariantMap(params), [this, ticket, method, account](auto result, bool ok) {
            emit assistanceReply(ticket, method, result.toVariantMap(), ok);
            if (ok && method == "assist_settings_save") request("assist_watch", {}, {}, [](auto, bool) {});
            if (ok && (method == "assist_correct" || method == "assist_accept") && account == m_account) {
                loadConversation(false, false); loadConversations(false, true);
            }
        });
    });
    return ticket;
}

QString MailBackend::labelRequest(const QString &method, const QVariantMap &params, const QString &account) {
    if (!QStringList{"label_catalog","label_create","label_describe","label_remove","label_auto_read","label_auto_spam","google_labels_sync","google_labels_sync_status","google_labels_sync_cancel"}.contains(method)) return {};
    const auto ticket=QUuid::createUuid().toString(QUuid::WithoutBraces);
    auto values=QJsonObject::fromVariantMap(params);
    if(method!="label_catalog"&&!method.endsWith("_status")&&!method.endsWith("_cancel")) {
        if(!values.contains("request_id")) values["request_id"]=ticket;
        if(method=="label_create"&&!values.contains("id")) values["id"]="label-"+values["request_id"].toString();
    }
    QTimer::singleShot(0,this,[this,method,values,account,ticket]{
        request(method,account,values,[this,method,values,account,ticket](auto result,bool ok){
            emit labelReply(ticket,method,result.toVariantMap(),ok);
            if(ok&&(method=="label_create"||method=="label_describe"||method=="label_remove"||method=="label_auto_read"||method=="label_auto_spam"||(method.startsWith("google_labels_sync")&&result["state"].toString()=="complete"))) {
                if(account==m_account&&method=="label_remove"&&values["id"].toString()==m_category) setCategory({});
                refresh();
            }
        });
    });
    return ticket;
}

QString MailBackend::requestMessageSource(const QString &id, const QString &version, bool withRaw) {
    // QML can briefly retain the preceding account's member while accountChanged
    // propagates. Never attach that obsolete source request to the new account.
    const auto members = m_conversation.value("messages").toList();
    if (std::none_of(members.cbegin(), members.cend(), [&](const auto &value) {
        const auto member = value.toMap();
        return member.value("id").toString() == id && member.value("source_version").toString() == version;
    })) return {};
    const auto ticket = QUuid::createUuid().toString(QUuid::WithoutBraces), account = m_account;
    const auto lifecycle = lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong();
    const auto key = account + '\n' + id + '\n' + version;
    const auto generation = m_generation;
    QTimer::singleShot(0, this, [this, ticket, account, id, version, key, lifecycle, generation, withRaw] {
        if (generation != m_generation || account != m_account) return;
        // The cache holds sources without raw text, so a raw request always reads.
        if (auto cached = m_sourceCache.object(key); !withRaw && cached && cached->lifecycle == lifecycle) {
            emit messageSourceReady(ticket, cached->data, true); return;
        }
        request("message_source", account, {{"id",id},{"source_version",version}}, [this, ticket, account, key, lifecycle, generation, withRaw](auto result, bool ok) {
            const auto prepared = withoutRaw(result);
            if (ok && lifecycle == lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong()) {
                const auto bytes = QJsonDocument(prepared).toJson(QJsonDocument::Compact).size();
                m_sourceCache.insert(key, new ReaderSnapshot{prepared.toVariantMap(), lifecycle}, int(bytes + 128));
            }
            if (account == m_account && generation == m_generation) emit messageSourceReady(ticket, (withRaw ? result : prepared).toVariantMap(), ok);
        });
    });
    return ticket;
}
QString MailBackend::receivedHtml(const QString &html) const { return ReplyDocument::limitedHtml(html, true); }
QVariantList MailBackend::receivedBody(const QString &html,const QString &plain) const {
    QCryptographicHash hash(QCryptographicHash::Sha256);
    hash.addData(html.toUtf8()); hash.addData(QByteArrayView("\0", 1)); hash.addData(plain.toUtf8());
    const auto key = hash.result();
    if (const auto cached = m_bodyCache.object(key)) return *cached;
    auto rows = ReplyDocument::receivedBody(html, plain);
    const auto bytes = QJsonDocument(QJsonArray::fromVariantList(rows)).toJson(QJsonDocument::Compact).size();
    if (bytes <= maxFrame) m_bodyCache.insert(key, new QVariantList(rows), int(bytes + 256));
    return rows;
}
QString MailBackend::fixedFontFamily() const {
    const auto families = QFontDatabase::families();
    const auto system = QFontDatabase::systemFont(QFontDatabase::FixedFont).family();
    if (families.contains(system, Qt::CaseInsensitive)) return system;
    for (const auto candidate : {"Cascadia Mono", "SF Mono", "Menlo", "Consolas", "DejaVu Sans Mono", "Liberation Mono", "Courier New"})
        if (families.contains(candidate, Qt::CaseInsensitive)) return candidate;
    for (const auto &family : families) if (QFontDatabase::isFixedPitch(family)) return family;
    return "Geist";
}
void MailBackend::copyText(const QString &text) { QGuiApplication::clipboard()->setText(text); }
