#include "MailBackend.h"
#include <QDateTime>
#include <QJsonArray>
#include <QTimeZone>
#include <algorithm>

namespace {
QString people(const QVariantList &rows) {
    QStringList names;
    for (const auto &value : rows) { const auto person = value.toMap(); names.append(person.value("name").toString().isEmpty() ? person.value("email").toString() : person.value("name").toString()); }
    return names.join(", ");
}
QString stateText(const QString &state) {
    if (state == "queued" || state == "preparing" || state == "in_flight") return "sending";
    if (state == "accepted") return "sent";
    if (state == "prepared") return "not sent yet";
    return "check send status";
}
}

QVariantMap MailBackend::queuedOutgoing(const QJsonObject &params) const {
    auto headers = m_draft;
    headers["preview"] = m_draft.value("body").toString().left(320);
    const auto mode = headers.value("mode").toString();
    return {{"account",m_account},{"id","outgoing:" + params.value("request_id").toString()},
        {"request_id",params.value("request_id").toString()},{"request",params.toVariantMap()},{"draft_id",params.value("draft_id").toString()},
        {"draft_revision",params.value("expected_revision").toInteger()},{"state","queued"},
        {"headers",headers},{"text",m_draft.value("body").toString() + (m_draft.value("signature_text").toString().isEmpty() ? "" : "\n\n" + m_draft.value("signature_text").toString())},
        {"conversation_id", mode == "reply" || mode == "reply_all" ? m_draft.value("conversation_id").toString() : QString{}},
        {"attachments",m_draft.value("attachments",QVariantList{})},{"created_at",QDateTime::currentSecsSinceEpoch()}};
}

QVariantList MailBackend::outgoingMessages() const {
    QMap<QString,QVariantMap> messages;
    for (const auto &value : m_outgoingPreviews.value(m_account)) { const auto row=value.toMap(); messages[row.value("request_id").toString()]=row; }
    if (m_outgoingThreadKey == m_account+'\n'+m_selected)
        for (const auto &value : m_outgoingThread) { const auto row=value.toMap(); messages[row.value("request_id").toString()]=row; }
    for (const auto &row : m_transientOutgoing) if (row.value("account") == m_account) messages[row.value("request_id").toString()]=row;
    for (const auto &send : m_queuedSends) if (send.account == m_account) {
        auto preview=send.preview;
        if (m_actionClock.elapsed() >= send.deadline) preview["state"]="preparing";
        messages[send.preview.value("request_id").toString()]=preview;
    }
    QVariantList rows;
    for (auto row : messages) {
        const auto body=m_outgoingBodies.value(m_account+'\n'+row.value("request_id").toString());
        if (!body.isEmpty() && body.value("source_version") == row.value("source_version"))
            for (const auto &key : {"text","text_truncated","attachments","content_error"}) row[key]=body.value(key);
        rows.append(row);
    }
    std::stable_sort(rows.begin(),rows.end(),[](const auto &a,const auto &b){
        const auto left=a.toMap(),right=b.toMap();
        return left.value("created_at").toLongLong() == right.value("created_at").toLongLong()
            ? left.value("id").toString() < right.value("id").toString() : left.value("created_at").toLongLong() < right.value("created_at").toLongLong();
    });
    return rows;
}

int MailBackend::sendingCount() const {
    const auto rows=outgoingMessages();
    return int(std::count_if(rows.cbegin(),rows.cend(),[](const auto &row){const auto state=row.toMap().value("state").toString();return state=="queued" || state=="preparing" || state=="in_flight";}));
}

QVariantMap MailBackend::outgoingMessage(const QVariantMap &value) const {
    const auto headers=value.value("headers").toMap();
    QVariantList metadata;
    for (const auto &pair : {qMakePair("To","to_recipients"),qMakePair("Cc","cc_recipients"),qMakePair("Bcc","bcc_recipients")})
        if (!headers.value(pair.second).toList().isEmpty()) metadata.append(QVariantMap{{"label",pair.first},{"people",headers.value(pair.second)}});
    return {{"id",value.value("id")},{"account",value.value("account")},{"sender_name","You"},
        {"sender_email",headers.value("from")},{"subject",headers.value("subject")},
        {"to",people(headers.value("to_recipients").toList())},{"metadata",metadata},
        {"sent_at",QDateTime::fromSecsSinceEpoch(value.value("created_at").toLongLong(),QTimeZone::UTC).toString(Qt::ISODate)},
        {"text",value.value("text",headers.value("preview"))},{"text_truncated",value.value("text_truncated",false)},
        {"content_pending",!value.contains("text")},{"content_error",value.value("content_error")},{"attachments",value.value("attachments",QVariantList{})},
        {"sent",true},{"unread",false},{"starred",false},{"categories",QVariantList{}},
        {"is_outgoing",true},{"outgoing_state",value.value("state")},{"send_status",stateText(value.value("state").toString())},
        {"request_id",value.value("request_id")},{"draft_id",value.value("draft_id")},
        {"source_available",false},{"source_version",value.value("source_version")}};
}

QVariantList MailBackend::conversations() const {
    auto rows=m_conversations;
    // Filters describe acquired mail. A pending submission cannot satisfy labels,
    // review state or sender-kind filters, and never contributes to Inbox.
    if (m_folder != "sent" || !m_query.isEmpty() || !m_filters.isEmpty() || !m_category.isEmpty()) return rows;
    for (const auto &value : outgoingMessages()) {
        const auto message=outgoingMessage(value.toMap());
        auto row=message;
        row["subject"]=message.value("subject").toString().isEmpty() ? "(No subject)" : message.value("subject");
        row["snippet"]=message.value("text"); row["sender_name"]=message.value("to");
        row["recipient_names"]=message.value("to"); row["last_at"]=message.value("sent_at");
        row["latest_at"]=message.value("sent_at"); row["message_count"]=1; row["revision"]=0;
        row["message_ids"]=QVariantList{}; row["group"]="done";
        rows.append(row);
    }
    std::stable_sort(rows.begin(),rows.end(),[](const auto &a,const auto &b){
        const auto left=a.toMap(),right=b.toMap();
        return left.value("latest_at").toString() > right.value("latest_at").toString();
    });
    return rows;
}

QVariantMap MailBackend::conversation() const {
    auto result=m_conversation;
    auto members=result.value("messages").toList();
    for (const auto &value : outgoingMessages()) {
        const auto outgoing=value.toMap();
        if (outgoing.value("id") == m_selected) {
            result={{"id",m_selected},{"account",m_account},{"is_outgoing",true},
                {"subject",outgoing.value("headers").toMap().value("subject")},
                {"messages",QVariantList{outgoingMessage(outgoing)}},{"message_ids",QVariantList{}},{"revision",0}};
            return result;
        }
        if (!result.isEmpty() && !m_selected.isEmpty() && outgoing.value("conversation_id") == m_selected) members.append(outgoingMessage(outgoing));
    }
    if (!result.isEmpty()) result["messages"]=members;
    return result;
}

void MailBackend::refreshOutgoingPresentation() {
    publishConversations(m_conversations);
    emit conversationsChanged(); emit conversationChanged();
}

void MailBackend::loadOutgoingThread() {
    if (!m_connected || m_syntheticProfile || m_selected.isEmpty() || m_selected.startsWith("outgoing:") || m_shutdown) return;
    const auto account=m_account, selected=m_selected;
    const auto lifecycle=lifecycleAccount(account).value("lifecycle").toMap().value("revision");
    const auto generation=m_generation;
    request("google_sent",account,{{"conversation_id",selected},{"limit",100}},[this,account,selected,lifecycle,generation](auto result,bool ok){
        if (!ok || account != m_account || selected != m_selected || generation != m_generation || result["account"] != account
            || lifecycleAccount(account).value("lifecycle").toMap().value("revision") != lifecycle) return;
        m_outgoingThreadKey=account+'\n'+selected;
        m_outgoingThread=result["outgoing"].toArray().toVariantList();
        refreshOutgoingPresentation();
        for (const auto &row : m_outgoingThread) loadOutgoingMessage(row.toMap());
    });
}

void MailBackend::loadOutgoing(bool more) {
    if (!m_connected || m_syntheticProfile || m_account.isEmpty() || m_shutdown) return;
    const auto account=m_account;
    const auto lifecycle=lifecycleAccount(account).value("lifecycle").toMap().value("revision");
    const auto generation=++m_outgoingGeneration[account];
    const int offset=more ? m_outgoingNext.value(account,-1) : 0;
    if (offset < 0) return;
    loadOutgoingPage(account,lifecycle,generation,offset,std::max(100,int(m_outgoingPreviews.value(account).size())+(more ? 100 : 0)),more ? m_outgoingPreviews.value(account) : QVariantList{});
}
void MailBackend::loadOutgoingPage(const QString &account, const QVariant &lifecycle, int generation, int offset, int desired, QVariantList rows) {
    request("google_sent",account,{{"offset",offset},{"limit",100}},[this,account,lifecycle,generation,offset,desired,rows=std::move(rows)](auto result,bool ok) mutable {
        if (!ok || result["account"] != account || generation != m_outgoingGeneration.value(account)
            || lifecycleAccount(account).value("lifecycle").toMap().value("revision") != lifecycle) return;
        const auto incoming=result["outgoing"].toArray().toVariantList();
        for (const auto &value : incoming) {
            const auto row=value.toMap(); if (row.value("account") != account || row.value("request_id").toString().isEmpty()) continue;
            rows.erase(std::remove_if(rows.begin(),rows.end(),[&](const auto &old){return old.toMap().value("id")==row.value("id");}),rows.end());
            rows.append(row); m_transientOutgoing.remove(account+'\n'+row.value("request_id").toString());
        }
        const int next=result["next_offset"].isDouble() ? result["next_offset"].toInt() : -1;
        if (next > offset && !incoming.isEmpty() && rows.size() < desired) {
            loadOutgoingPage(account,lifecycle,generation,next,desired,std::move(rows)); return;
        }
        auto previouslySelected=QVariantMap{};
        for (const auto &value : m_outgoingPreviews.value(account)) if (value.toMap().value("id") == m_selected) previouslySelected=value.toMap();
        m_outgoingPreviews[account]=rows;
        m_outgoingNext[account]=next;
        if (!previouslySelected.isEmpty() && std::none_of(rows.cbegin(),rows.cend(),[this](const auto &row){return row.toMap().value("id")==m_selected;})) {
            m_outgoingBodies.remove(account+'\n'+previouslySelected.value("request_id").toString());
            loadOutgoingMessage(previouslySelected);
        }
        // A very fast acquisition may precede the Send callback and omit its
        // preview from this page. Resolve that exact receipt before removing it.
        for (const auto &row : m_transientOutgoing) if (row.value("account") == account && row.value("state") == "accepted") loadOutgoingMessage(row);
        if (account != m_account) return;
        refreshOutgoingPresentation();
        for (const auto &value : outgoingMessages()) {
            const auto row=value.toMap();
            if (row.value("id") == m_selected || (!m_selected.isEmpty() && row.value("conversation_id") == m_selected)) loadOutgoingMessage(row);
        }
    });
}

void MailBackend::loadOutgoingMessage(const QVariantMap &value) {
    const auto account=value.value("account").toString(), requestId=value.value("request_id").toString(), key=account+'\n'+requestId;
    if (!m_connected || value.value("state") == "queued" || value.value("state") == "preparing" || m_outgoingReads.contains(key)
        || (m_outgoingBodies.contains(key) && m_outgoingBodies.value(key).value("source_version") == value.value("source_version"))) return;
    m_outgoingReads.insert(key);
    const auto lifecycle=lifecycleAccount(account).value("lifecycle").toMap().value("revision");
    const auto version=value.value("source_version");
    request("google_sent_message",account,{{"request_id",requestId}},[this,key,account,requestId,lifecycle,version](auto result,bool ok){
        m_outgoingReads.remove(key);
        if (lifecycleAccount(account).value("lifecycle").toMap().value("revision") != lifecycle) return;
        if (!ok || result["account"] != account || result["request_id"] != requestId) {
            if (m_outgoingBodies.size() >= 100) m_outgoingBodies.erase(m_outgoingBodies.begin());
            m_outgoingBodies[key]={{"source_version",version},{"content_error","The sent message could not be read. Reopen it to try again."}};
            if (account == m_account) refreshOutgoingPresentation();
            return;
        }
        const auto observed=result["observed_conversation_id"].toString();
        if (!observed.isEmpty()) {
            m_transientOutgoing.remove(key);
            auto &rows=m_outgoingPreviews[account];
            rows.erase(std::remove_if(rows.begin(),rows.end(),[&](const auto &row){return row.toMap().value("request_id")==requestId;}),rows.end());
            if (m_outgoingThreadKey.startsWith(account+'\n'))
                m_outgoingThread.erase(std::remove_if(m_outgoingThread.begin(),m_outgoingThread.end(),[&](const auto &row){return row.toMap().value("request_id")==requestId;}),m_outgoingThread.end());
            if (account == m_account && m_selected == "outgoing:"+requestId && m_activeDraft.isEmpty()
                && !m_draftDirty && !draftBusy() && !presentation().value("replyOpen").toBool()) selectConversation(observed);
        }
        // This cache contains only bounded read projections, never draft authority.
        if (m_outgoingBodies.size() >= 100) m_outgoingBodies.erase(m_outgoingBodies.begin());
        m_outgoingBodies[key]=result.toVariantMap();
        if (account == m_account) refreshOutgoingPresentation();
    });
}

void MailBackend::outgoingAction(const QString &id, bool undo) {
    for (const auto &value : outgoingMessages()) {
        const auto row=value.toMap();
        if (row.value("id") != id) continue;
        const auto key=m_account+'\n'+row.value("draft_id").toString();
        if (undo) {
            if (m_queuedSends.contains(key) && m_queuedSends.value(key).preview.value("id") == id
                && m_actionClock.elapsed() < m_queuedSends.value(key).deadline) cancelQueuedSend(key,true);
        } else {
            auto &ui=m_googleSendUi[key];
            // Select this precise historical request; opening the latest draft
            // must not silently show a different submission's recovery controls.
            if (!ui.active && !m_queuedSends.contains(key)) {ui.request=QJsonObject::fromVariantMap(row.value("request").toMap()); ui.selected={}; ui.uncertain=true;}
            openDraftInAccount(m_account,row.value("draft_id").toString());
        }
        return;
    }
}
