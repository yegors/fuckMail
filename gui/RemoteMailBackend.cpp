// Mail read live from the server and not kept: remote search rows mixed into
// the list, the thread opened from one, and unsubscribing from a list.
#include "MailBackend.h"
#include "ReplyDocument.h"
#include <QDateTime>
#include <QJsonArray>
#include <QTimeZone>
#include <QUuid>

void MailBackend::setRemoteSearch(bool value) {
    if (value == m_remoteSearch) return;
    m_remoteSearch = value;
    ++m_remoteSerial; m_remoteBusy = false; m_remoteError.clear(); m_remoteRows.clear();
    m_remoteAccount.clear(); m_remoteFolder.clear(); m_remoteQuery.clear();
    m_remoteSearchTimer.stop();
    emit remoteSearchChanged();
    if (value) { scheduleRemoteSearch(); if (m_remoteSearchTimer.isActive()) { m_remoteSearchTimer.stop(); runRemoteSearch(); } }
    else { publishConversations(m_conversations); emit conversationsChanged(); }
}

QVariantMap MailBackend::remoteSearchState() const {
    const auto address = lifecycleAccount(m_account).value("email").toString();
    return {{"busy", m_remoteBusy}, {"error", m_remoteError}, {"address", address}, {"count", m_remoteRows.size()}};
}

// Each account, folder and query is searched once; its rows show only while it is current.
void MailBackend::scheduleRemoteSearch() {
    if (!m_remoteSearch) return;
    const auto query = m_query.trimmed();
    if (m_remoteAccount == m_account && m_remoteFolder == m_folder && m_remoteQuery == query) return;
    const bool hadRows = !m_remoteRows.isEmpty();
    ++m_remoteSerial; m_remoteRows.clear(); m_remoteError.clear();
    m_remoteAccount = m_account; m_remoteFolder = m_folder; m_remoteQuery = query;
    m_remoteBusy = !query.isEmpty() && !m_account.isEmpty() && m_folder != u"spam";
    emit remoteSearchChanged();
    if (m_remoteBusy) m_remoteSearchTimer.start(); else m_remoteSearchTimer.stop();
    if (hadRows) { publishConversations(m_conversations); emit conversationsChanged(); }
}

void MailBackend::runRemoteSearch() {
    if (!m_remoteSearch || !m_remoteBusy) return;
    const auto query = m_remoteQuery, account = m_remoteAccount, folder = m_remoteFolder;
    const int serial = ++m_remoteSerial;
    googleOperation("google_remote_search", account, {{"query", query}, {"folder", folder}}, [this, serial](auto result, bool ok) {
        // A newer query, folder or account bumped the serial; drop this answer.
        if (serial != m_remoteSerial) return;
        m_remoteBusy = false;
        const auto code = result["code"].toString();
        m_remoteRows = ok ? result["conversations"].toArray().toVariantList() : QVariantList{};
        m_remoteError = ok ? QString{}
            : code == u"sync_not_running" ? QStringLiteral("Start syncing this account to search the server")
            : code == u"capability_unavailable" ? QStringLiteral("This account has not allowed reading mail")
            : code == u"google_rate_limited" ? QStringLiteral("The server asked us to slow down. Try again in a minute.")
            : QStringLiteral("The server could not be searched");
        emit remoteSearchChanged();
        if (!m_remoteRows.isEmpty()) { publishConversations(m_conversations); emit conversationsChanged(); }
    });
}

// Remote rows join the list by date. In the Inbox they fall under Done, by month.
QVariantList MailBackend::withRemoteRows(QVariantList rows) const {
    if (!m_remoteSearch || m_remoteRows.isEmpty() || m_remoteAccount != m_account || m_remoteFolder != m_folder
        || m_remoteQuery != m_query.trimmed()) return rows;
    const bool inbox = m_folder == u"inbox";
    const auto done = doneSections();
    QSet<QString> ids;
    for (const auto &value : rows) ids.insert(value.toMap().value("id").toString());
    for (const auto &value : m_remoteRows) {
        auto row = value.toMap();
        // A conversation already stored locally is shown from the store.
        if (ids.contains(row.value("id").toString())) continue;
        row["account"] = m_account;
        const auto when = QDateTime::fromString(row.value("latest_at").toString(), Qt::ISODate);
        if (inbox) {
            const auto section = done.of(row.value("latest_at").toString());
            if (sectionCollapsed(section)) continue;
            row["section"] = section;
        }
        qsizetype at = rows.size();
        for (qsizetype i = 0; i < rows.size(); ++i) {
            const auto other = rows[i].toMap();
            if (inbox && !other.value("section").toString().startsWith(u"done")) continue;
            if (QDateTime::fromString(other.value("latest_at").toString(), Qt::ISODate) < when) { at = i; break; }
        }
        rows.insert(at, row);
    }
    return rows;
}

void MailBackend::loadRemoteThread() {
    const auto id = m_selected, account = m_account;
    const int generation = m_generation;
    googleOperation("google_remote_thread", account, {{"thread_id", id.mid(QStringLiteral("remote:").size())}},
        [this, id, account, generation](auto result, bool ok) {
        if (id != m_selected || account != m_account || generation != m_generation) return;
        if (!ok) {
            fail(result["code"].toString() == u"sync_not_running" ? QStringLiteral("Start syncing this account to open mail from the server.")
                : result["message"].toString("This conversation could not be read from the server."));
            return;
        }
        auto data = result.toVariantMap();
        auto members = data.value("messages").toList();
        if (!members.isEmpty()) {
            // Formatted text for the newest message, as the reader prepares stored mail.
            auto latest = members.last().toMap();
            const auto html = latest.take("body_html").toString();
            const auto plain = latest.value("text").toString().isEmpty() ? QStringLiteral("(No text)") : latest.value("text").toString();
            latest["reader_sections"] = ReplyDocument::receivedBody(html, plain);
            latest["reader_plain_sections"] = html.isEmpty() ? latest.value("reader_sections") : QVariant(ReplyDocument::receivedBody({}, plain));
            members.last() = latest;
        }
        data["messages"] = members;
        data["reader_prepared"] = true;
        m_conversation = data;
        emit conversationChanged();
        emit ready();
    });
}

QString MailBackend::unsubscribe(const QVariantMap &message) {
    const auto ticket = QUuid::createUuid().toString(QUuid::WithoutBraces);
    const auto account = m_account, conversation = m_conversation.value("id").toString();
    // {id, source_version, method}: the method the reader read from the message's source.
    // An email also names the address the person confirmed, and `fallback`
    // when it replaces a one-click POST the list refused.
    const auto how = message.value("method").toString();
    QJsonObject params{{"id", message.value("id").toString()}, {"source_version", message.value("source_version").toString()}};
    if (how == u"mailto") {
        params["address"] = message.value("address").toString();
        if (message.value("fallback").toBool()) params["fallback"] = true;
    }
    const QString method = how == u"mailto" ? "google_unsubscribe_mail" : "message_unsubscribe";
    QTimer::singleShot(0, this, [this, ticket, account, conversation, params, method] {
        const auto finish = [this, ticket, account, conversation](const QJsonObject &result, bool ok) {
            if (ok && result["method"].toString() == u"link") openLink(result["url"].toString());
            if (ok && account == m_account && conversation == m_conversation.value("id").toString()) {
                auto list = m_conversation.value("mailing_list").toMap();
                list["unsubscribed"] = QVariantMap{{"method", result["method"].toString()}, {"at", result["at"].toString()}};
                m_conversation["mailing_list"] = list;
                emit conversationChanged();
            }
            emit unsubscribeReady(ticket, result.toVariantMap(), ok);
        };
        // A refused one-click POST answers `unsubscribe_by_email`; the reader
        // then asks before sending the list's unsubscribe email.
        googleOperation(method, account, params, finish);
    });
    return ticket;
}

// Spam is read live from Gmail each visit and never stored. Its messages show
// one row per conversation, like any list; opening one reads the thread live.
void MailBackend::loadSpamRows(bool append) {
    const auto account = m_account;
    const auto token = append ? m_spamToken : QString{};
    if (append && token.isEmpty()) return;
    const int generation = ++m_listGeneration;
    if (!append) {
        m_spamToken.clear(); m_nextOffset = -1; m_nextCursor.clear();
        publishConversations({}); emit conversationsChanged();
        refreshBinCounts();
    }
    m_listError.clear(); m_loading = true; emit loadingChanged();
    QJsonObject params;
    if (!token.isEmpty()) params["page_token"] = token;
    googleOperation("google_spam_list", account, params, [this, generation, account, append](auto result, bool ok) {
        if (generation != m_listGeneration || account != m_account || m_folder != u"spam") return;
        m_loading = false;
        if (!ok) {
            const auto code = result["code"].toString();
            m_listError = code == u"sync_not_running" ? QStringLiteral("Start syncing this account to see its Spam.")
                : code == u"capability_unavailable" ? QStringLiteral("This account has not allowed reading mail.")
                : code == u"google_rate_limited" ? QStringLiteral("Google asked us to slow down. Try again in a minute.")
                : QStringLiteral("Spam could not be read from Google.");
            emit loadingChanged(); return;
        }
        QVariantList rows = append ? m_conversations : QVariantList{};
        QHash<QString, qsizetype> index;
        for (qsizetype i = 0; i < rows.size(); ++i) index.insert(rows[i].toMap().value("id").toString(), i);
        for (const auto &value : result["messages"].toArray()) {
            const auto message = value.toObject();
            const auto thread = message["thread_id"].toString();
            if (thread.isEmpty()) continue;
            const auto id = "remote:" + thread;
            if (index.contains(id)) {
                // Newest first: an older message of a listed thread only counts.
                auto row = rows[index.value(id)].toMap();
                row["message_count"] = row.value("message_count").toInt() + 1;
                rows[index.value(id)] = row;
                continue;
            }
            const auto at = QDateTime::fromMSecsSinceEpoch(message["internal_date_ms"].toVariant().toLongLong(), QTimeZone::UTC);
            index.insert(id, rows.size());
            rows.append(QVariantMap{{"id", id}, {"account", account}, {"remote", true}, {"spam", true}, {"remote_thread_id", thread},
                {"subject", message["subject"].toString()}, {"sender_name", message["sender_name"].toString()},
                {"sender_email", message["sender_email"].toString()}, {"snippet", message["snippet"].toString()},
                {"latest_at", at.toString(Qt::ISODate)}, {"message_count", 1}, {"unread", false}, {"starred", false},
                {"revision", 0}, {"message_ids", QVariantList{}}, {"categories", QVariantList{}}, {"kind", "unknown"}, {"group", "spam"}});
        }
        m_spamToken = result["next_page_token"].toString();
        m_nextOffset = m_spamToken.isEmpty() ? -1 : 0;
        publishConversations(rows); emit conversationsChanged(); emit loadingChanged();
    });
}

void MailBackend::refreshBinCounts() {
    if (m_folder != u"spam" && m_folder != u"trash") return;
    const auto account = m_account, folder = m_folder;
    googleOperation("google_bin_counts", account, {{"folder", folder}}, [this, account, folder](auto result, bool ok) {
        if (!ok || account != m_account || folder != m_folder) return;
        m_binCounts = {{"account", account}, {"folder", folder}, {"messages", result["messages"].toVariant()},
            {"conversations", result["conversations"].toVariant()}};
        if (result.contains("stored")) m_binCounts["stored"] = result["stored"].toVariant();
        emit binCountsChanged();
    });
}

void MailBackend::loadBinMenuCounts() {
    if (m_syntheticProfile || m_account.isEmpty()) return;
    const auto account = m_account;
    if (m_binMenuCounts.value("account") != account) m_binMenuCounts = {{"account", account}};
    for (const QString folder : {"spam", "trash"})
        googleOperation("google_bin_counts", account, {{"folder", folder}}, [this, account, folder](auto result, bool ok) {
            if (!ok || account != m_account) return;
            if (m_binMenuCounts.value("account") != account) m_binMenuCounts = {{"account", account}};
            m_binMenuCounts[folder] = result["conversations"].toVariant();
            emit binCountsChanged();
        });
}

void MailBackend::notSpam() {
    if (m_binBusy || m_folder != u"spam" || !m_selected.startsWith(u"remote:")) return;
    const auto account = m_account, id = m_selected;
    m_binBusy = true; emit binCountsChanged();
    googleOperation("google_not_spam", account, {{"thread_id", id.mid(QStringLiteral("remote:").size())}}, [this, account, id](auto result, bool ok) {
        finishBinAction("not_spam", account, "spam", id, result, ok);
    });
}

void MailBackend::deleteForever() {
    if (m_binBusy || (m_folder != u"spam" && m_folder != u"trash") || m_selected.isEmpty()) return;
    const auto account = m_account, folder = m_folder, id = m_selected;
    QJsonObject params{{"folder", folder}};
    if (folder == u"spam") {
        if (!id.startsWith(u"remote:")) return;
        params["thread_id"] = id.mid(QStringLiteral("remote:").size());
    } else params["conversation_id"] = id;
    m_binBusy = true; emit binCountsChanged();
    googleOperation("google_delete_forever", account, params, [this, account, folder, id](auto result, bool ok) {
        finishBinAction("delete", account, folder, id, result, ok);
    });
}

void MailBackend::emptyBin() {
    if (m_binBusy || (m_folder != u"spam" && m_folder != u"trash")) return;
    const auto account = m_account, folder = m_folder;
    m_binBusy = true; emit binCountsChanged();
    googleOperation("google_empty", account, {{"folder", folder}}, [this, account, folder](auto result, bool ok) {
        finishBinAction("empty", account, folder, {}, result, ok);
    });
}

// The acted-on row leaves the list and the next one opens; emptying reloads.
void MailBackend::finishBinAction(const QString &action, const QString &account, const QString &folder, const QString &id, const QJsonObject &result, bool ok) {
    m_binBusy = false; emit binCountsChanged();
    emit binActionFinished(action, ok, result.toVariantMap());
    if (!ok) {
        // Reconnecting is offered where the person asked to delete.
        if (result["code"].toString() != u"reconnect_required")
            fail(result["message"].toString(action == u"not_spam" ? "This conversation could not be moved to the Inbox." : "Gmail did not delete this mail."));
        return;
    }
    if (action == u"not_spam") showActionToast("Moved to Inbox \u00b7 marked not spam");
    else if (action == u"delete") showActionToast("Deleted permanently");
    else showActionToast(folder == u"spam" ? "Spam emptied" : "Trash emptied");
    if (account != m_account || folder != m_folder) return;
    if (action == u"empty") {
        if (!m_selected.isEmpty()) selectConversationInternal({}, false);
        loadConversations(); refreshBinCounts(); return;
    }
    QVariantList rows = m_conversations;
    QString next;
    for (qsizetype i = 0; i < rows.size(); ++i) {
        if (rows[i].toMap().value("id").toString() != id) continue;
        rows.removeAt(i);
        if (!rows.isEmpty()) next = rows[std::min(i, rows.size() - 1)].toMap().value("id").toString();
        break;
    }
    publishConversations(rows); emit conversationsChanged();
    if (m_selected == id) selectConversationInternal(next, false);
    // Trash is local: the engine removed its copies; counts follow.
    if (folder == u"trash") refresh();
    refreshBinCounts();
}
