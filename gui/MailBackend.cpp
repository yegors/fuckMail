#include "MailBackend.h"
#include "IpcTrust.h"
#include "CalendarLayout.h"

#include <QCoreApplication>
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

}

MailBackend::MailBackend(QString profile, QString engine, QObject *parent, bool fixtureClock, bool liveProfile)
    : QObject(parent), m_accountModel(this), m_conversationModel(this), m_messageModel(this), m_profile(std::move(profile)), m_engine(std::move(engine)), m_fixtureClock(fixtureClock), m_liveProfileRequested(liveProfile), m_syntheticProfile(!liveProfile) {
    initializeCalendarSearch();
    connect(this, &MailBackend::conversationChanged, this, [this] {
        const auto scope = QJsonDocument(QJsonArray{m_account, m_selected}).toJson(QJsonDocument::Compact);
        if (!m_messageModel.replace(QString::fromUtf8(scope), m_conversation.value("messages").toList())) {
            m_messageModel.replace({}, {});
            fail("The message reader received an invalid identity. Refresh to continue.");
        }
    });
    m_notificationTimer.setInterval(15000);
    connect(&m_notificationTimer, &QTimer::timeout, this, &MailBackend::pollNotifications);
    for (auto signal : {&MailBackend::preferencesChanged, &MailBackend::connectedChanged, &MailBackend::startupReadyChanged,
            &MailBackend::accountsChanged, &MailBackend::eventsChanged})
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
    m_calendarDate = todayDate(); m_timelineDate = todayDate();
    m_clockTimer.setInterval(30000);
    connect(&m_clockTimer, &QTimer::timeout, this, [this] { emit eventsChanged(); });
    m_clockTimer.start();
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
            m_calendarRevision = -1;
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
            m_diagnostics = result.toVariantMap(); emit preferencesChanged();
            auto revision = result["revision"].toInteger();
            const auto calendarRevision = result["calendar_generation"].toInteger(-1);
            const bool calendarChanged = calendarRevision < 0 || calendarRevision != m_calendarRevision;
            if (calendarChanged) loadCalendarSources();
            const bool changed = m_revision != revision;
            if (changed) m_listRefreshPending = true;
            // A foreground query already reads current store state. Between
            // successive inputs, do not place another expensive background
            // query ahead of the next input on this serial socket. Retain the
            // invalidation and deliver it once editing has settled.
            const bool editing = m_queryActivity.isValid() && m_queryActivity.elapsed() < 500;
            const bool refreshList = m_listRefreshPending && !m_loading && !m_searchTimer.isActive() && !editing;
            if (refreshList) m_listRefreshPending = false;
            if (changed) refreshState(refreshList, calendarChanged, true);
            else {
                if (refreshList) loadConversations(false, true);
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
    for (auto timer : {&m_clockTimer, &m_connectTimer, &m_pollTimer, &m_searchTimer, &m_draftTimer, &m_viewTimer, &m_notificationTimer}) {
        timer->stop(); QObject::disconnect(timer, nullptr, this, nullptr);
    }
    QObject::disconnect(&m_socket, nullptr, this, nullptr);
    m_socket.abort();
}

void MailBackend::start() {
    if (m_connected || m_socket.state() == QLocalSocket::ConnectingState) return;
    clearError(); cancelShutdown(); m_shutdownEmitted = false; m_buffer.clear(); m_startupClock.start();
    m_launchAttempts = 0; m_launchDelay = 1000; m_nextLaunch = 0;
    m_connectTimer.start(); attemptConnection();
}

void MailBackend::attemptConnection() {
    if (m_startupClock.elapsed() >= 30000) {
        m_connectTimer.stop(); m_socket.abort(); fail("Cannot start the local engine. Check that fm-engine is installed beside the app, then reconnect."); return;
    }
    if (m_socket.state() != QLocalSocket::UnconnectedState) return;
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
        if (!process.startDetached()) {
            m_connectTimer.stop(); fail("Could not launch fm-engine. Use --engine to select its executable.");
        }
        else if (qEnvironmentVariableIsSet("FM_GUI_TEST_TRACE"))
            qInfo() << "GUI engine launch attempt" << m_launchAttempts;
    }
}

void MailBackend::request(const QString &method, const QString &account, const QJsonObject &params, Callback callback) {
    if (blockArchivedRequest(method, account)) {
        callback({{"code", "account_removed"}, {"message", "This account is a read-only archive. Restore it explicitly before making changes."}}, false); return;
    }
    const bool automaticRead = m_automaticReadScope && (method == "accounts" || method == "preferences_get"
        || method == "categories" || method == "conversation" || method == "draft_get" || method == "capture_history"
        || method == "google_sync_status" || method == "google_mail_mutation_status");
    // Synchronous cancellation/error callbacks can issue user work. The caller's
    // polling origin applies only to this read, never to a reentrant request.
    QScopedValueRollback<bool> origin(m_automaticReadScope, false);
    if (method == "conversations" || method == "conversation" || method == "drafts" || method == "sender_summary") pruneSupersededQueries();
    if (m_socket.state() != QLocalSocket::ConnectedState || m_pending.size() >= 32) {
        callback({}, false); return;
    }
    QString id = QString::number(++m_sequence);
    QJsonObject envelope{{"version", 1}, {"id", id}, {"method", method}, {"params", params}};
    if (!account.isEmpty()) envelope["account"] = account;
    const auto wire = QJsonDocument(envelope).toJson(QJsonDocument::Compact) + '\n';
    if (wire.size() > maxFrame) { fail("This request is too large for the current local interface."); callback({}, false); return; }
    m_pending.insert(id, {std::move(callback), 0, method, account, m_generation, m_listGeneration, m_calendarGeneration, m_senderCardGeneration, automaticRead, 0});
    m_outgoing.enqueue({id, wire});
    sendNext();
}
void MailBackend::pruneSupersededQueries() {
    QQueue<QPair<QString, QByteArray>> retained;
    QList<Callback> cancelled;
    for (auto &next : m_outgoing) {
        auto pending = m_pending.find(next.first);
        if (pending == m_pending.end()) continue;
        if (((pending->method == "conversations" || pending->method == "drafts")
            && (pending->account != m_account || pending->listGeneration != m_listGeneration))
            || (pending->method == "conversation" && (pending->account != m_account || pending->generation != m_generation))
            || (pending->method == "sender_summary" && (pending->account != m_account || pending->senderGeneration != m_senderCardGeneration))) {
            // These requests have not reached the engine. Their callbacks also
            // check generation/account, so cancellation cannot replace current
            // rows or mark a superseded reader load reviewed. Draft recovery
            // reads and all durable writes retain their ordering barriers.
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
        bool currentRead = true;
        if (pending.method == "conversations" || pending.method == "drafts") currentRead = pending.account == m_account && pending.listGeneration == m_listGeneration;
        else if (pending.method == "conversation" || pending.method == "draft_get") currentRead = pending.account == m_account && pending.generation == m_generation;
        else if (pending.method == "categories" || pending.method == "capture_history" || pending.method == "view_get") currentRead = pending.account == m_account;
        else if (pending.method == "calendar_combined") currentRead = pending.calendarGeneration == m_calendarGeneration;
        else if (pending.method == "sender_summary") currentRead = pending.account == m_account && pending.senderGeneration == m_senderCardGeneration;
        const bool retryCursor = pending.method == "conversations" && response["error"].toObject()["code"].toString() == "stale_cursor";
        if (!ok && currentRead && !retryCursor) fail(response["error"].toObject()["message"].toString("The operation failed."));
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
        if (!m_initialAccountsLoaded) { m_initialAccountsLoaded = true; emit startupReadyChanged(); }
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

void MailBackend::selectAccount(const QString &id) {
    ++m_accountSelectionRequests;
    m_lastAccountSelectionRequest = id;
    m_lastAccountSelectionDirty = m_draftDirty;
    m_lastAccountSelectionBusy = draftBusy();
    if (id == m_account) return;
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
    m_account = id; m_conversation.clear(); m_draft.clear(); m_activeDraft.clear(); m_recipientInputs.clear(); m_selected.clear(); m_draftStatus.clear(); m_pinnedRow.clear(); m_pinnedIndex = -1;
    m_query.clear(); m_category.clear(); m_filters.clear(); m_querySummary.clear(); m_captureStatus.clear(); m_folder = "inbox"; publishConversations({}); m_categories.clear(); m_captures.clear(); m_nextOffset = -1; m_nextCursor.clear(); m_loading = false;
    // Restore only observed mail projections. The fresh view/source requests
    // below still own current state and every read-on-open mutation.
    restoreMailView();
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
        m_query = view["query"].toString(); m_selected = view["selected"].toString();
        m_folder = view.value("folder", "inbox").toString();
        m_category = view.value("category").toString();
        m_filters = view.value("filters").toMap();
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
    if (value != "inbox" && value != "sent" && value != "all" && value != "drafts" && value != "trash") return;
    if (m_folder == value) return;
    m_queryActivity.start();
    m_folder = value; m_pinnedRow.clear(); m_pinnedIndex = -1;
    m_collapsedGroups.clear();
    if (value == "drafts") { m_filters.clear(); m_category.clear(); remember("filters", QVariantMap{}); remember("category", ""); }
    remember("folder", value); emit queryChanged(); loadConversations();
}
void MailBackend::setCategory(const QString &value) {
    if (m_category == value) return;
    m_queryActivity.start();
    m_category = value; m_pinnedRow.clear(); m_pinnedIndex = -1; remember("category", value); emit queryChanged(); loadConversations();
}
void MailBackend::setCollapsedGroups(const QStringList &groups) {
    QStringList next;
    if (m_folder == "inbox") {
        for (const auto &group : {QStringLiteral("review"), QStringLiteral("held"), QStringLiteral("done")})
            if (groups.contains(group)) next.append(group);
    }
    if (next == m_collapsedGroups) return;
    m_queryActivity.start();
    m_collapsedGroups = next; m_pinnedRow.clear(); m_pinnedIndex = -1;
    ++m_listGeneration; emit queryChanged(); loadConversations();
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

bool MailBackend::publishConversations(QVariantList rows) {
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
    if (!m_conversationModel.replace(QString::fromUtf8(scope), m_conversations)) {
        return reject("The local engine returned missing or duplicate mail row identities.");
    }
    return true;
}

QVariantMap MailBackend::mailViewScope() const {
    return {{"folder", m_folder}, {"query", m_query}, {"category", m_category}, {"filters", m_filters},
        {"collapsed_groups", m_collapsedGroups}};
}

void MailBackend::cacheMailView() {
    if (m_account.isEmpty() || m_folder == "drafts" || accountArchived()) return;
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
    if (m_folder == "drafts") { loadDraftRows(append, preserveLoaded); return; }
    const int generation = ++m_listGeneration;
    const QString account = m_account;
    const int offset = append ? m_nextOffset : 0;
    if (offset < 0) return;
    m_loading = true; emit loadingChanged();
    QJsonObject params{{"query", m_query}, {"folder", m_folder}, {"limit", 50}, {"offset", offset}};
    params["grouped"] = true;
    for (auto it = m_filters.cbegin(); it != m_filters.cend(); ++it) params[it.key()] = QJsonValue::fromVariant(it.value());
    if (m_folder == "inbox" && !m_collapsedGroups.isEmpty()) params["collapsed_groups"] = QJsonArray::fromStringList(m_collapsedGroups);
    if (append && !m_nextCursor.isEmpty()) { params.remove("offset"); params["cursor"] = m_nextCursor; }
    if (!m_category.isEmpty()) params["category"] = m_category;
    // Keep visible rows until a refresh has recovered the same page count. Each
    // IPC response remains bounded; a view-state write must not collapse paging.
    const int desired = append ? int(m_conversations.size()) + 50 : preserveLoaded ? std::max({50, int(m_conversations.size()), m_desiredConversationCount}) : 50;
    m_desiredConversationCount = desired;
    loadConversationPage(account, generation, params, append ? m_conversations : QVariantList{}, desired);
}
void MailBackend::loadConversationPage(const QString &account, int generation, QJsonObject params, QVariantList collected, int desiredCount, int cursorRetries) {
    request("conversations", account, params,
        [this, generation, account, params, collected = std::move(collected), desiredCount, cursorRetries](auto result, bool ok) mutable {
            if (generation != m_listGeneration || account != m_account) return;
            if (!ok && result["code"].toString() == "stale_cursor") {
                if (cursorRetries >= 3) { m_loading = false; emit loadingChanged(); fail("Mail kept changing during pagination. Retry loading more."); return; }
                params.remove("cursor"); params["offset"] = 0;
                loadConversationPage(account, generation, params, {}, desiredCount, cursorRetries + 1); return;
            }
            if (ok) {
                const auto previousSummary = m_querySummary;
                m_querySummary = {{"filtered_conversation_count", result["filtered_conversation_count"].toVariant()},
                                  {"filtered_group_counts", result["filtered_group_counts"].toVariant()},
                                  {"expanded_conversation_count", result["expanded_conversation_count"].toVariant()},
                                  {"account_counts", result["account_counts"].toVariant()}, {"coverage", result["coverage"].toVariant()}};
                const auto list = result["conversations"].toArray().toVariantList();
                collected.append(list);
                const auto next = result["next_offset"].isDouble() ? result["next_offset"].toInt() : -1;
                if (!list.isEmpty() && next > params["offset"].toInt() && collected.size() < desiredCount) {
                    params["offset"] = next;
                    const auto cursor = result["next_cursor"].toString();
                    if (!cursor.isEmpty()) { params.remove("offset"); params["cursor"] = cursor; }
                    loadConversationPage(account, generation, params, std::move(collected), desiredCount, cursorRetries);
                    return;
                }
                const auto previous = m_conversations;
                const auto previousNext = m_nextOffset;
                if (m_pinnedIndex >= 0 && !m_pinnedRow.isEmpty()) {
                    auto pinned = m_pinnedRow;
                    for (qsizetype i = collected.size(); i-- > 0;) {
                        if (collected[i].toMap()["id"].toString() == m_selected) { pinned = collected[i].toMap(); collected.removeAt(i); }
                    }
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
            m_loading = false; emit loadingChanged();
        });
}
void MailBackend::loadMore() { if (!m_loading && hasMore()) loadConversations(true); }

void MailBackend::selectConversation(const QString &id) {
    selectConversationInternal(id, false);
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
    if (review && m_mutatingConversation) {
        m_afterConversationMutation = [this, id, clickedRow, clickedIndex] { selectConversationInternal(id, true, clickedRow, clickedIndex); }; return;
    }
    if (m_draftDirty || draftBusy()) {
        m_afterDraftSave = [this, id, review, clickedRow, clickedIndex] { selectConversationInternal(id, review, clickedRow, clickedIndex); }; saveDraft(); return;
    }
    if (id != m_selected || (review && m_pinnedIndex < 0)) {
        m_pinnedRow.clear(); m_pinnedIndex = -1;
        if (review) { m_pinnedRow = std::move(clickedRow); m_pinnedIndex = clickedIndex; }
    }
    if (id == m_selected && m_activeDraft.isEmpty()) { if (review) loadConversation(false, true); return; }
    ++m_generation; m_selected = id; m_activeDraft.clear(); m_recipientInputs.clear(); m_conversation.clear(); m_draft.clear(); m_draftStatus.clear();
    restoreConversation(m_account, id);
    remember("selectedDraft", "");
    remember("selected", id); remember("readerY", 0); emit conversationChanged(); emit draftChanged(); emit mailDeliveryChanged();
    loadConversation(true, review);
}

void MailBackend::loadConversation(bool loadDraft, bool reviewOnLoad) {
    if (m_selected.isEmpty()) return;
    const auto id = m_selected, account = m_account; const int generation = m_generation;
    const auto lifecycle = lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong();
    request("conversation", account, {{"id", id}}, [this, id, account, generation, lifecycle, reviewOnLoad](auto result, bool ok) {
        if (ok) cacheConversation(account, id, result.toVariantMap(), lifecycle);
        if (!ok || id != m_selected || account != m_account || generation != m_generation) return;
        const auto data = result.toVariantMap();
        if (m_conversation != data) { m_conversation = data; emit conversationChanged(); }
        if (reviewOnLoad && accountCapability(account, "mail_write")) {
            const auto members = result["messages"].toArray();
            if (std::any_of(members.cbegin(), members.cend(), [](auto m) { return m.toObject()["unread"].toBool(); }))
                mutateConversation("flag", {{"read", true}});
        }
        emit ready();
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
            m_draft = draft; m_draftConflict = false; m_draftStatus = status;
            emit draftChanged();
        }
        else if (metadataChanged) emit draftChanged();
    });
}

QJsonObject MailBackend::mutation() const { return {{"request_id", uuid()}}; }
void MailBackend::flag(const QString &name, bool value) {
    mutateConversation("flag", {{name, value}});
}
void MailBackend::mutateConversation(const QString &method, QJsonObject params) {
    if (m_conversation.isEmpty() || m_mutatingConversation) return;
    if (!accountCapability(m_account, "mail_write")) { fail("Reconnect Google with Mail access to change this conversation."); return; }
    params["request_id"] = uuid(); params["id"] = m_selected;
    params["message_ids"] = QJsonArray::fromVariantList(m_conversation["message_ids"].toList());
    params["expected_revision"] = m_conversation["revision"].toLongLong();
    const auto id = m_selected, account = m_account;
    m_mutatingConversation = true; emit mutationChanged();
    request(method, account, params, [this, id, account, method](auto result, bool ok) {
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
            m_conversation = result.toVariantMap();
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
    if (attachmentImportPending()) {
        finishGoogleSendSave(false);
        if (m_shutdown && m_draftDirty) {
            cancelShutdown(); fail("Resolve the attachment import before closing with unsaved edits.");
        }
        return;
    }
    if (draftBusy()) return;
    if (!commitRecipientInputs()) { finishGoogleSendSave(false); return; }
    if (!m_draftDirty) {
        finishGoogleSendSave(true);
        if (m_afterDraftSave) { auto callback = std::move(m_afterDraftSave); m_afterDraftSave = {}; callback(); }
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
    request("draft_save", account, params, [this, account, context, saveGeneration, body, html, attachments, signature, headers](auto result, bool ok) {
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
        m_draftDirty = !m_recipientInputs.isEmpty() || m_draft["body"].toString() != body || m_draft["body_html"] != html || m_draft["attachment_ids"] != attachments || m_draft["signature_text"].toString() != signature;
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
    request("attachment", m_account, {{"id", id}}, [this](auto result, bool ok) {
        if (!ok) return;
        const auto path = result["path"].toString();
        if (path.isEmpty()) { fail("This attachment has no complete, verified local copy (" + result["state"].toString("unavailable") + ")."); return; }
        if (!QDir::isAbsolutePath(path) || !QDesktopServices::openUrl(QUrl::fromLocalFile(path))) fail("No application could open this retained attachment.");
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
    m_events = calendarPresentation(result["events"].toArray(), QDate::fromString(from, Qt::ISODate), days, QTimeZone(result["time_zone"].toString().toUtf8()));
    m_calendarReady = true; m_calendarCoverage = calendarCoverageText(result["coverage"].toObject());
}
void MailBackend::loadCalendar() {
    const auto from = calendarFrom(), timeline = m_timelineDate;
    const int days = calendarDays(), generation = ++m_calendarGeneration;
    m_weekStart = from;
    const auto selection = QJsonValue::fromVariant(m_preferences.value("selected_calendars"));
    const QJsonObject calendarParams{{"from", from}, {"to", QDate::fromString(from, Qt::ISODate).addDays(days).toString(Qt::ISODate)}, {"calendars", selection}};
    const QJsonObject timelineParams{{"from", timeline}, {"to", QDate::fromString(timeline, Qt::ISODate).addDays(1).toString(Qt::ISODate)}, {"calendars", selection}};
    const auto displayZone = m_preferences.value("calendar_display_time_zone").toString();
    const auto calendarScope = QString::fromUtf8(QJsonDocument(calendarParams).toJson(QJsonDocument::Compact)) + "/" + displayZone;
    const auto timelineScope = QString::fromUtf8(QJsonDocument(timelineParams).toJson(QJsonDocument::Compact)) + "/" + displayZone;
    if (calendarScope != m_calendarScope) {
        m_calendarScope = calendarScope; m_events.clear(); m_calendarReady = false;
    }
    if (timelineScope != m_timelineScope) {
        m_timelineScope = timelineScope; m_todayEvents.clear(); m_timelineReady = false;
    }
    const bool hadCalendarSnapshot = m_calendarReady, hadTimelineSnapshot = m_timelineReady;
    if (!m_connected) {
        m_calendarCoverage = hadCalendarSnapshot ? "Cached calendar is stale — reconnect to refresh" : "Calendar is waiting for the local engine";
        m_timelineCoverage = hadTimelineSnapshot ? "Cached timeline is stale — reconnect to refresh" : "Timeline is waiting for the local engine";
        m_calendarReady = false; m_timelineReady = false; emit eventsChanged(); return;
    }
    m_calendarCoverage = hadCalendarSnapshot ? "Refreshing cached calendar…" : "Loading calendars…";
    m_timelineCoverage = hadTimelineSnapshot ? "Refreshing cached timeline…" : "Loading timeline…";
    emit eventsChanged();
    auto coverageText = [this](const QJsonObject &coverage) {
        return calendarCoverageText(coverage);
    };
    request("calendar_combined", {}, calendarParams,
      [this, from, days, generation, hadCalendarSnapshot, coverageText](auto result, bool ok) {
        if (generation != m_calendarGeneration) return;
        if (!ok) {
            m_calendarReady = false;
            const auto code = result["code"].toString();
            if (!hadCalendarSnapshot || code == "permission_denied" || code == "not_found" || code == "scope_mismatch") m_events.clear();
            m_calendarCoverage = (m_events.isEmpty() ? "Calendar unavailable: " : "Cached calendar is stale: ") + result["message"].toString();
            emit eventsChanged(); return;
        }
        publishCalendarWindow(result, from, days);
        emit eventsChanged();
    });
    request("calendar_combined", {}, timelineParams,
        [this, timeline, generation, hadTimelineSnapshot, coverageText](auto result, bool ok) {
            if (generation != m_calendarGeneration || timeline != m_timelineDate) return;
            if (!ok) {
                m_timelineReady = false;
                const auto code = result["code"].toString();
                if (!hadTimelineSnapshot || code == "permission_denied" || code == "not_found" || code == "scope_mismatch") m_todayEvents.clear();
                m_timelineCoverage = (m_todayEvents.isEmpty() ? "Timeline unavailable: " : "Cached timeline is stale: ") + result["message"].toString();
                emit eventsChanged(); return;
            }
            m_todayEvents = calendarPresentation(result["events"].toArray(), QDate::fromString(timeline, Qt::ISODate), 1, QTimeZone(result["time_zone"].toString().toUtf8()));
            m_timelineReady = true; m_timelineCoverage = coverageText(result["coverage"].toObject()); emit eventsChanged();
        });
}
void MailBackend::moveWeek(int delta) {
    m_calendarView = "week"; m_calendarDate = QDate::fromString(m_calendarDate, Qt::ISODate).addDays(delta * 7).toString(Qt::ISODate);
    loadCalendar(); emit eventsChanged();
}
void MailBackend::showFixtureWeek() {
    m_calendarView = "week"; m_calendarDate = m_fixtureToday; loadCalendar(); emit eventsChanged();
}
void MailBackend::refresh() { m_listRefreshPending = false; refreshState(true, true); }
void MailBackend::refreshState(bool refreshList, bool refreshCalendar, bool automatic) {
    {
        QScopedValueRollback<bool> origin(m_automaticReadScope, automatic);
        loadAccounts(); loadPreferences(); loadCategories();
    }
    if (refreshList) loadConversations(false, true);
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
        } else {
            m_viewDirty.insert(account); cancelShutdown();
            fail("The current mail view was not saved. Your changes remain here. Reconnect if needed, then close again to retry.");
            if (result["code"].toString() == "stale_revision") {
                request("view_get", account, {}, [this, account](auto latest, bool loaded) {
                    if (!loaded) return;
                    auto view = latest["view"].toObject().toVariantMap();
                    const auto pending = m_viewPending.value(account);
                    for (auto it = pending.cbegin(); it != pending.cend(); ++it) view[it.key()] = it.value();
                    m_viewRevisions[account] = latest["revision"].toInteger(); m_views[account] = view;
                    if (account == m_account) emit presentationChanged();
                });
            }
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
    if (m_unsavedAccountBuffer) { m_afterAccountDisposition = [this] { shutdown(); }; accountNavigationAllowed(); return; }
    cancelPendingGoogleSend("Send cancelled before closing.");
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
    if (!m_shutdown || m_shutdownEmitted || m_draftDirty || draftBusy() || m_preferencesSaving || !m_preferencesPending.isEmpty() || m_calendarWrites || m_accountWrites || m_mutatingConversation || !m_viewSaving.isEmpty() || !m_viewDirty.isEmpty()) return;
    m_shutdownEmitted = true;
    m_pollTimer.stop();
    m_socket.disconnectFromServer(); emit shutdownReady();
}
