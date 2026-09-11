#include "MailBackend.h"
#include "IpcTrust.h"
#include "CalendarLayout.h"

#include <QCoreApplication>
#include <QDateTime>
#include <QDesktopServices>
#include <QDir>
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QProcess>
#include <QRegularExpression>
#include <QUrl>
#include <QUuid>
#include <QTimeZone>

namespace {
constexpr qsizetype maxFrame = 1024 * 1024;
QString uuid() { return QUuid::createUuid().toString(QUuid::WithoutBraces); }

}

MailBackend::MailBackend(QString profile, QString engine, QObject *parent)
    : QObject(parent), m_profile(std::move(profile)), m_engine(std::move(engine)) {
    m_socket.setReadBufferSize(maxFrame + 1);
    m_connectTimer.setInterval(150);
    connect(&m_connectTimer, &QTimer::timeout, this, &MailBackend::attemptConnection);
    connect(&m_socket, &QLocalSocket::readyRead, this, &MailBackend::receive);
    connect(&m_socket, &QLocalSocket::connected, this, [this] {
        QString trustError;
        if (!verifyEnginePeer(m_socket.socketDescriptor(), m_enginePid, trustError)) {
            m_connectTimer.stop(); fail(trustError); m_socket.abort(); return;
        }
        request("hello", {}, {{"token", m_token}, {"client", "gui"}}, [this](auto, bool ok) {
            if (!ok) { m_socket.abort(); return; }
            m_connectTimer.stop();
            m_connected = true; m_error.clear();
            emit connectedChanged(); emit errorChanged();
            if (m_account.isEmpty()) loadAccounts();
            else refresh();
            m_pollTimer.start();
        });
    });
    connect(&m_socket, &QLocalSocket::disconnected, this, [this] {
        m_connected = false; m_pollTimer.stop(); emit connectedChanged();
        m_outgoing.clear(); m_inFlight.clear(); m_buffer.clear();
        auto pending = std::move(m_pending); m_pending.clear();
        for (auto &item : pending) item.callback({}, false);
        if (!m_shutdown) fail("The local engine disconnected. Your unsaved reply remains here. Reconnect to continue.");
        else finishShutdown();
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
        request("status", {}, {}, [this](auto result, bool ok) {
            if (!ok) return;
            auto revision = result["revision"].toInteger();
            if (m_revision != revision) refresh();
            m_revision = revision;
        });
    });
    m_searchTimer.setSingleShot(true); m_searchTimer.setInterval(180);
    connect(&m_searchTimer, &QTimer::timeout, this, [this] { loadConversations(); });
    m_draftTimer.setSingleShot(true); m_draftTimer.setInterval(450);
    connect(&m_draftTimer, &QTimer::timeout, this, &MailBackend::saveDraft);
    m_viewTimer.setSingleShot(true); m_viewTimer.setInterval(700);
    connect(&m_viewTimer, &QTimer::timeout, this, [this] {
        for (const auto &account : m_viewDirty.values()) saveView(account);
    });
}

void MailBackend::start() {
    if (m_connected || m_socket.state() == QLocalSocket::ConnectingState) return;
    clearError(); m_shutdown = false; m_buffer.clear(); m_launched = false; m_connectStarted = QDateTime::currentMSecsSinceEpoch();
    m_connectTimer.start(); attemptConnection();
}

void MailBackend::attemptConnection() {
    if (QDateTime::currentMSecsSinceEpoch() - m_connectStarted > 30000) {
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
        if (info["protocol"].toInt() == 1 && !endpoint.isEmpty() && !m_token.isEmpty()) {
#ifdef Q_OS_WIN
            if (endpoint.startsWith("\\\\.\\pipe\\")) endpoint = endpoint.mid(9);
#endif
            m_socket.connectToServer(endpoint);
        }
    }
    if (!m_launched) {
        m_launched = true;
        QProcess process;
        process.setProgram(m_engine); process.setArguments({"--profile", m_profile});
        process.setStandardOutputFile(QProcess::nullDevice());
        process.setStandardErrorFile(QProcess::nullDevice());
        if (!process.startDetached()) fail("Could not launch fm-engine. Use --engine to select its executable.");
    }
}

void MailBackend::request(const QString &method, const QString &account, const QJsonObject &params, Callback callback) {
    if (m_socket.state() != QLocalSocket::ConnectedState || m_pending.size() >= 32) {
        callback({}, false); return;
    }
    QString id = QString::number(++m_sequence);
    QJsonObject envelope{{"version", 1}, {"id", id}, {"method", method}, {"params", params}};
    if (!account.isEmpty()) envelope["account"] = account;
    const auto wire = QJsonDocument(envelope).toJson(QJsonDocument::Compact) + '\n';
    if (wire.size() > maxFrame) { fail("This request is too large for the current local interface."); callback({}, false); return; }
    m_pending.insert(id, {std::move(callback), 0});
    m_outgoing.enqueue({id, wire});
    sendNext();
}
void MailBackend::sendNext() {
    if (!m_inFlight.isEmpty() || m_outgoing.isEmpty() || m_socket.state() != QLocalSocket::ConnectedState) return;
    auto next = m_outgoing.dequeue();
    m_inFlight = next.first;
    m_pending[m_inFlight].started = QDateTime::currentMSecsSinceEpoch();
    m_socket.write(next.second);
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
        if (!ok) fail(response["error"].toObject()["message"].toString("The operation failed."));
        pending.callback(ok ? response["result"].toObject() : response["error"].toObject(), ok);
        sendNext();
    }
}

void MailBackend::fail(const QString &message) { m_error = message; emit errorChanged(); }
void MailBackend::clearError() { m_error.clear(); emit errorChanged(); }
QVariantMap MailBackend::currentAccount() const {
    for (const auto &entry : m_accounts) if (entry.toMap()["id"].toString() == m_account) return entry.toMap();
    return {};
}

void MailBackend::loadAccounts() {
    request("accounts", {}, {}, [this](auto result, bool ok) {
        if (!ok) return;
        m_accounts = result["accounts"].toArray().toVariantList(); emit accountsChanged();
        if (m_account.isEmpty() && !m_accounts.isEmpty()) selectAccount(m_accounts.first().toMap()["id"].toString());
    });
}

void MailBackend::selectAccount(const QString &id) {
    if (id == m_account) return;
    if (m_draftDirty || m_savingDraft) { m_afterDraftSave = [this, id] { selectAccount(id); }; saveDraft(); return; }
    if (!m_account.isEmpty()) saveView(m_account);
    ++m_generation; const int generation = m_generation;
    m_account = id; m_conversation.clear(); m_draft.clear(); m_selected.clear(); m_draftStatus.clear();
    m_query.clear(); m_category.clear(); m_folder = "inbox"; m_conversations.clear(); m_events.clear(); m_todayEvents.clear(); m_categories.clear(); m_nextOffset = -1; m_loading = false;
    emit accountChanged(); emit accountsChanged(); emit queryChanged(); emit conversationChanged(); emit draftChanged(); emit conversationsChanged(); emit eventsChanged();
    request("view_get", id, {}, [this, id, generation](auto result, bool ok) {
        if (ok) { m_views[id] = result["view"].toObject().toVariantMap(); m_viewRevisions[id] = result["revision"].toInteger(); }
        if (generation != m_generation) return;
        auto view = m_views.value(id).toMap();
        m_query = view["query"].toString(); m_selected = view["selected"].toString();
        m_folder = view.value("folder", "inbox").toString();
        m_category = view.value("category").toString();
        m_weekStart = view.value("weekStart", "2026-09-07").toString();
        emit queryChanged(); emit presentationChanged();
        loadConversations(); loadCalendar(); loadCategories();
        if (!m_selected.isEmpty()) loadConversation(true);
    });
}

void MailBackend::setQuery(const QString &value) {
    if (value == m_query) return;
    m_query = value; ++m_listGeneration; remember("query", value); emit queryChanged(); m_searchTimer.start();
}
void MailBackend::setFolder(const QString &value) {
    if (value != "inbox" && value != "sent" && value != "all") return;
    if (m_folder == value) return;
    m_folder = value; remember("folder", value); emit queryChanged(); loadConversations();
}
void MailBackend::setCategory(const QString &value) {
    if (m_category == value) return;
    m_category = value; remember("category", value); emit queryChanged(); loadConversations();
}
void MailBackend::loadCategories() {
    const auto account = m_account;
    request("categories", account, {}, [this, account](auto result, bool ok) {
        if (!ok || account != m_account) return;
        m_categories = result["categories"].toArray().toVariantList(); emit accountsChanged();
    });
}

void MailBackend::loadConversations(bool append, bool preserveLoaded) {
    if (!m_connected || m_account.isEmpty()) return;
    const int generation = ++m_listGeneration;
    const QString account = m_account;
    const int offset = append ? m_nextOffset : 0;
    if (offset < 0) return;
    m_loading = true; emit loadingChanged();
    QJsonObject params{{"query", m_query}, {"folder", m_folder}, {"limit", 50}, {"offset", offset}};
    if (!m_category.isEmpty()) params["category"] = m_category;
    // Keep visible rows until a refresh has recovered the same page count. Each
    // IPC response remains bounded; a view-state write must not collapse paging.
    const int desired = append ? int(m_conversations.size()) + 50 : preserveLoaded ? std::max(50, int(m_conversations.size())) : 50;
    loadConversationPage(account, generation, params, append ? m_conversations : QVariantList{}, desired);
}
void MailBackend::loadConversationPage(const QString &account, int generation, QJsonObject params, QVariantList collected, int desiredCount) {
    request("conversations", account, params,
        [this, generation, account, params, collected = std::move(collected), desiredCount](auto result, bool ok) mutable {
            if (generation != m_listGeneration || account != m_account) return;
            if (ok) {
                const auto list = result["conversations"].toArray().toVariantList();
                collected.append(list);
                const auto next = result["next_offset"].isDouble() ? result["next_offset"].toInt() : -1;
                if (!list.isEmpty() && next > params["offset"].toInt() && collected.size() < desiredCount) {
                    params["offset"] = next;
                    loadConversationPage(account, generation, params, std::move(collected), desiredCount);
                    return;
                }
                const auto previous = m_conversations;
                const auto previousNext = m_nextOffset;
                m_conversations = std::move(collected); m_nextOffset = next;
                if (m_selected.isEmpty() && !m_conversations.isEmpty()) selectConversation(m_conversations.first().toMap()["id"].toString());
                if (previous != m_conversations || previousNext != m_nextOffset) emit conversationsChanged();
            }
            m_loading = false; emit loadingChanged();
        });
}
void MailBackend::loadMore() { if (!m_loading && hasMore()) loadConversations(true); }

void MailBackend::selectConversation(const QString &id) {
    if (id == m_selected) return;
    if (m_draftDirty || m_savingDraft) { m_afterDraftSave = [this, id] { selectConversation(id); }; saveDraft(); return; }
    ++m_generation; m_selected = id; m_conversation.clear(); m_draft.clear(); m_draftStatus.clear();
    remember("selected", id); remember("readerY", 0); emit conversationChanged(); emit draftChanged();
    loadConversation(true);
}

void MailBackend::loadConversation(bool loadDraft) {
    if (m_selected.isEmpty()) return;
    const auto id = m_selected, account = m_account; const int generation = m_generation;
    request("conversation", account, {{"id", id}}, [this, id, account, generation](auto result, bool ok) {
        if (!ok || id != m_selected || account != m_account || generation != m_generation) return;
        const auto data = result.toVariantMap();
        if (m_conversation != data) { m_conversation = data; emit conversationChanged(); }
        emit ready();
    });
    if (loadDraft) request("draft_get", account, {{"conversation_id", id}}, [this, id, account, generation](auto result, bool ok) {
        if (!ok || id != m_selected || account != m_account || generation != m_generation || m_draftDirty) return;
        m_draft = result.toVariantMap(); m_draftStatus = m_draft.value("revision").toInt() > 0 ? "Saved on this machine" : "A reply stays here until you save it";
        emit draftChanged();
    });
}

QJsonObject MailBackend::mutation() const { return {{"request_id", uuid()}}; }
void MailBackend::flag(const QString &name, bool value) {
    if (m_conversation.isEmpty()) return;
    auto params = mutation(); params["id"] = m_selected;
    params["message_ids"] = QJsonArray::fromVariantList(m_conversation["message_ids"].toList());
    params["expected_revision"] = m_conversation["revision"].toLongLong(); params[name] = value;
    const auto id = m_selected, account = m_account;
    request("flag", account, params, [this, id, account](auto result, bool ok) {
        if (ok && id == m_selected && account == m_account) { m_conversation = result.toVariantMap(); emit conversationChanged(); }
        refresh();
    });
}
void MailBackend::setRead(bool read) { flag("read", read); }
void MailBackend::setStarred(bool starred) { flag("starred", starred); }

void MailBackend::editDraft(const QString &body) {
    if (m_draft.isEmpty() || body == m_draft["body"].toString()) return;
    m_draft["body"] = body; m_draftDirty = true; m_draftStatus = "Saving…"; emit draftChanged(); m_draftTimer.start();
}
void MailBackend::saveDraft() {
    m_draftTimer.stop();
    if (m_savingDraft) return;
    if (!m_draftDirty) {
        if (m_afterDraftSave) { auto callback = std::move(m_afterDraftSave); m_afterDraftSave = {}; callback(); }
        if (m_shutdown) finishShutdown();
        return;
    }
    if (!m_connected) { m_draftStatus = "Not saved — reconnect to keep this reply"; emit draftChanged(); return; }
    auto params = mutation(); const auto body = m_draft["body"].toString();
    params["conversation_id"] = m_selected; params["body"] = body; params["expected_revision"] = m_draft["revision"].toLongLong();
    m_savingDraft = true;
    request("draft_save", m_account, params, [this, body](auto result, bool ok) {
        m_savingDraft = false;
        if (!ok) { m_draftStatus = "Not saved — your reply is still in the editor"; m_shutdown = false; emit draftChanged(); return; }
        m_draft["revision"] = result["revision"].toVariant();
        m_draftDirty = m_draft["body"].toString() != body;
        m_draftStatus = m_draftDirty ? "Saving…" : "Saved on this machine"; emit draftChanged();
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
void MailBackend::loadCalendar() {
    const auto account = m_account, week = m_weekStart;
    const auto end = QDate::fromString(week, Qt::ISODate).addDays(7).toString(Qt::ISODate);
    request("calendar", account, {{"from", week}, {"to", end}}, [this, account, week](auto result, bool ok) {
        if (!ok || account != m_account || week != m_weekStart) return;
        m_events = calendarPresentation(result["events"].toArray(), QDate::fromString(week, Qt::ISODate), 7, QTimeZone(result["time_zone"].toString("America/Toronto").toUtf8()));
        m_fixtureToday = result["fixture_today"].toString("2026-09-11"); emit eventsChanged();
    });
    request("calendar", account, {{"from", m_fixtureToday}, {"to", QDate::fromString(m_fixtureToday, Qt::ISODate).addDays(1).toString(Qt::ISODate)}},
        [this, account](auto result, bool ok) {
            if (!ok || account != m_account) return;
            m_todayEvents = calendarPresentation(result["events"].toArray(), QDate::fromString(m_fixtureToday, Qt::ISODate), 1, QTimeZone(result["time_zone"].toString("America/Toronto").toUtf8())); emit eventsChanged();
        });
}
void MailBackend::moveWeek(int delta) {
    m_weekStart = QDate::fromString(m_weekStart, Qt::ISODate).addDays(delta * 7).toString(Qt::ISODate);
    remember("weekStart", m_weekStart); loadCalendar(); emit eventsChanged();
}
void MailBackend::showFixtureWeek() {
    const auto today = QDate::fromString(m_fixtureToday, Qt::ISODate);
    m_weekStart = today.addDays(1 - today.dayOfWeek()).toString(Qt::ISODate);
    remember("weekStart", m_weekStart); loadCalendar(); emit eventsChanged();
}
void MailBackend::refresh() { loadAccounts(); loadCategories(); loadConversations(false, true); loadConversation(!m_draftDirty && !m_savingDraft); loadCalendar(); }

void MailBackend::remember(const QString &key, const QVariant &value) {
    if (m_account.isEmpty()) return;
    auto view = m_views.value(m_account).toMap();
    if (view.value(key) == value) return;
    view[key] = value; m_views[m_account] = view; m_viewDirty.insert(m_account); m_viewTimer.start();
}
void MailBackend::saveView(const QString &account) {
    if (!m_connected || account.isEmpty() || m_viewSaving.contains(account) || !m_viewDirty.contains(account)) return;
    m_viewDirty.remove(account); m_viewSaving.insert(account);
    auto params = mutation(); params["view"] = QJsonObject::fromVariantMap(m_views.value(account).toMap()); params["expected_revision"] = m_viewRevisions.value(account);
    request("view_save", account, params, [this, account](auto result, bool ok) {
        m_viewSaving.remove(account);
        if (ok) m_viewRevisions[account] = result["revision"].toInteger();
        else m_viewDirty.insert(account);
        if (ok && m_viewDirty.contains(account)) saveView(account);
        if (m_shutdown) finishShutdown();
    });
}
void MailBackend::shutdown() {
    m_shutdown = true; m_pollTimer.stop(); m_searchTimer.stop(); m_viewTimer.stop();
    for (const auto &account : m_viewDirty.values()) saveView(account);
    saveDraft(); finishShutdown();
}
void MailBackend::finishShutdown() {
    if (!m_shutdown || m_draftDirty || m_savingDraft || !m_viewSaving.isEmpty()) return;
    m_socket.disconnectFromServer(); emit shutdownReady();
}
